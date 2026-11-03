//! PoC-F：门独立成单、三态映射、安灯与红门诚实关单。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use af_eval::{
    freeze_gate_suite, Catalog, GateCheckDefinition, GateCheckResult, GateCheckStatus,
    GateFailureCode, GateResultEnvelope, GateResultStore, GateRunnerRef, GateSubject,
    GateSubjectKind, GateVerdict, ManualClock, RunVerdict, SkipReason,
};
use af_execline::{
    DriverConfig, EngineClient, ExecContext, ExecuteUnit, FixedClock, GateCheckRunner,
    GateExecuteUnit, GateOrderMetrics, GateRunnerOutput, GateUnitSpec, MesFileWriter,
    OrderLifecycleDriver, PendingRecordSink, ProcessGateRunner,
};
use af_workorder::{
    validate_required_gate_inputs, BudgetVoucher, GateAdmitRejection, LineBinding, OrderState,
    Priority, ProvidedGateInput, RequiredGateInput, Sla,
};

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn digest(ch: char) -> String {
    std::iter::repeat_n(ch, 64).collect()
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "af-pocf-{}-{}-{}",
        std::process::id(),
        tag,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn definitions() -> Vec<GateCheckDefinition> {
    vec![GateCheckDefinition {
        check_id: "assertion".into(),
        required: true,
        input: serde_json::json!({"expected": true}),
    }]
}

fn frozen_catalog(clock: &ManualClock) -> (Arc<Mutex<Catalog>>, String) {
    let (mut catalog, _) = Catalog::open_in_memory_with_clock("eval-pocf", clock.clone()).unwrap();
    let frozen = freeze_gate_suite(&mut catalog, "f1", &definitions()).unwrap();
    (Arc::new(Mutex::new(catalog)), frozen.snapshot_hash.unwrap())
}

fn spec(snapshot_hash: String, run_id: &str) -> GateUnitSpec {
    GateUnitSpec {
        lineage_id: "f1".into(),
        version: 1,
        snapshot_hash,
        run_id: run_id.into(),
        subject: GateSubject {
            kind: GateSubjectKind::GitCommit,
            repo: "example/repo".into(),
            version: COMMIT.into(),
            files: Vec::new(),
        },
        recipe_digest: digest('a'),
        runner: GateRunnerRef {
            r#impl: "fake/structured".into(),
            digest: digest('b'),
            environment_digest: digest('c'),
        },
        inputs: Vec::new(),
        contamination_input: Vec::new(),
        budget_ms: 100,
    }
}

struct FakeRunner {
    status: GateCheckStatus,
    skip_reason: Option<SkipReason>,
    timeout_clock: Option<ManualClock>,
}

impl GateCheckRunner for FakeRunner {
    fn run(&self, defs: &[GateCheckDefinition], _ctx: &ExecContext<'_>) -> GateRunnerOutput {
        if let Some(clock) = &self.timeout_clock {
            clock.set(110);
        }
        GateRunnerOutput {
            checks: defs
                .iter()
                .map(|def| GateCheckResult {
                    check_id: def.check_id.clone(),
                    required: def.required,
                    status: self.status,
                    skip_reason: self.skip_reason,
                    detail: "structured fake result".into(),
                })
                .collect(),
            signals: af_eval::ExecSignals {
                subject: self.status == GateCheckStatus::Fail,
                ..af_eval::ExecSignals::default()
            },
            evidence: Vec::new(),
        }
    }
}

fn execute_direct(
    tag: &str,
    order_id: &str,
    unit: &GateExecuteUnit,
) -> (af_execline::ExecOutput, PathBuf) {
    let root = scratch(tag);
    let artifacts = root.join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let sink = PendingRecordSink;
    let ctx = ExecContext {
        order_id,
        stable_id: "sid-pocf",
        line_id: "line-pocf",
        out_dir: &artifacts,
        task_id: "gate-task",
        host_alias: "local",
        data_root: &root,
        attempt_dir: &root,
        record_sink: &sink,
        graph_version: None,
        task_sha256: None,
        plan_sha256: None,
        trial: false,
    };
    (unit.execute(&ctx).unwrap(), root)
}

fn load_output(output: &af_execline::ExecOutput, root: &Path) -> (GateResultEnvelope, String) {
    let digest = output.record.as_ref().unwrap()["content_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let result = GateResultStore::new(root.join("artifacts"))
        .load(&digest)
        .unwrap();
    (result, digest)
}

#[test]
fn runner_infra_bench_timeout_and_contamination_are_needs_human_with_andon() {
    for (tag, status, reason, timeout, expected) in [
        (
            "skip-infra",
            GateCheckStatus::Skip,
            SkipReason::EnvironmentUnavailable,
            false,
            GateFailureCode::Infra,
        ),
        (
            "skip-bench",
            GateCheckStatus::Skip,
            SkipReason::NotApplicable,
            false,
            GateFailureCode::Bench,
        ),
        (
            "timeout",
            GateCheckStatus::Skip,
            SkipReason::EnvironmentUnavailable,
            true,
            GateFailureCode::Timeout,
        ),
    ] {
        let clock = ManualClock::new(10);
        let (catalog, snapshot) = frozen_catalog(&clock);
        let unit = GateExecuteUnit::with_clock(
            spec(snapshot, &format!("run-{tag}")),
            catalog,
            Arc::new(FakeRunner {
                status,
                skip_reason: Some(reason),
                timeout_clock: timeout.then_some(clock.clone()),
            }),
            Arc::new(clock),
        );
        let (output, root) = execute_direct(tag, "wo-needs-human", &unit);
        let (result, _) = load_output(&output, &root);
        assert_eq!(result.payload.verdict, GateVerdict::NeedsHuman, "{tag}");
        assert_eq!(result.payload.failure_code, Some(expected), "{tag}");
        assert_eq!(output.verdict, None);
        let events = MesFileWriter::load(&root.join("mes-events.jsonl")).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].kind.starts_with("andon.needs_human."));
    }
}

#[test]
fn contamination_halt_maps_to_timeout_not_subject_failure() {
    let clock = ManualClock::new(10);
    let (catalog, snapshot) = frozen_catalog(&clock);
    let mut spec = spec(snapshot, "run-contaminated");
    spec.contamination_input = vec![serde_json::to_string(&definitions()[0]).unwrap()];
    let unit = GateExecuteUnit::with_clock(
        spec,
        catalog,
        Arc::new(FakeRunner {
            status: GateCheckStatus::Pass,
            skip_reason: None,
            timeout_clock: None,
        }),
        Arc::new(clock),
    );
    let (output, root) = execute_direct("contamination", "wo-contaminated", &unit);
    let (result, _) = load_output(&output, &root);
    assert_eq!(result.payload.verdict, GateVerdict::NeedsHuman);
    assert_eq!(result.payload.failure_code, Some(GateFailureCode::Timeout));
    assert!(result.payload.checks.is_empty());
}

#[test]
fn malformed_frozen_gate_definition_is_e_bench() {
    let clock = ManualClock::new(10);
    let (mut catalog, _) = Catalog::open_in_memory_with_clock("eval-pocf", clock.clone()).unwrap();
    catalog.create_benchmark("f1").unwrap();
    catalog.put_case("f1", 1, "assertion", "not-json").unwrap();
    let snapshot = catalog.freeze("f1", 1).unwrap().snapshot_hash.unwrap();
    let unit = GateExecuteUnit::with_clock(
        spec(snapshot, "run-bad-definition"),
        Arc::new(Mutex::new(catalog)),
        Arc::new(FakeRunner {
            status: GateCheckStatus::Pass,
            skip_reason: None,
            timeout_clock: None,
        }),
        Arc::new(clock),
    );
    let (output, root) = execute_direct("bad-definition", "wo-bad-definition", &unit);
    let (result, _) = load_output(&output, &root);
    assert_eq!(result.payload.verdict, GateVerdict::NeedsHuman);
    assert_eq!(result.payload.failure_code, Some(GateFailureCode::Bench));
}

#[test]
fn runner_that_cannot_start_is_e_infra_not_a_green_or_subject_failure() {
    let clock = ManualClock::new(10);
    let (catalog, snapshot) = frozen_catalog(&clock);
    let missing = scratch("missing-runner").join("does-not-exist");
    let runner = ProcessGateRunner {
        program: missing,
        args: Vec::new(),
        per_check_timeout: Duration::from_millis(20),
    };
    let unit = GateExecuteUnit::with_clock(
        spec(snapshot, "run-spawn-infra"),
        catalog,
        Arc::new(runner),
        Arc::new(clock),
    );
    let (output, root) = execute_direct("spawn-infra", "wo-spawn-infra", &unit);
    let (result, _) = load_output(&output, &root);
    assert_eq!(result.payload.verdict, GateVerdict::NeedsHuman);
    assert_eq!(result.payload.failure_code, Some(GateFailureCode::Infra));
    assert!(result.payload.checks[0].detail.contains("runner 不可启动"));
}

#[test]
fn process_exit_code_cannot_turn_structured_result_green() {
    for (tag, json, exit, expected, code) in [
        (
            "exit-zero-fail",
            r#"{"check_id":"assertion","required":true,"status":"fail","detail":"assertion failed"}"#,
            "0",
            GateVerdict::Fail,
            Some(GateFailureCode::Subject),
        ),
        (
            "exit-nonzero-pass",
            r#"{"check_id":"assertion","required":true,"status":"pass","detail":"false green"}"#,
            "1",
            GateVerdict::NeedsHuman,
            Some(GateFailureCode::Bench),
        ),
    ] {
        let clock = ManualClock::new(10);
        let (catalog, snapshot) = frozen_catalog(&clock);
        let program = PathBuf::from("/bin/sh");
        let mut spec = spec(snapshot, &format!("run-{tag}"));
        spec.runner.r#impl = "process/sh".into();
        spec.runner.digest = af_execline::file_sha256_hex(&program).unwrap();
        let runner = ProcessGateRunner {
            program,
            args: vec!["-c".into(), format!("printf '%s' '{json}'; exit {exit}")],
            per_check_timeout: Duration::from_secs(1),
        };
        let unit = GateExecuteUnit::with_clock(spec, catalog, Arc::new(runner), Arc::new(clock));
        let (output, root) = execute_direct(tag, "wo-process", &unit);
        let (result, _) = load_output(&output, &root);
        assert_eq!(result.payload.verdict, expected, "{tag}");
        assert_eq!(result.payload.failure_code, code, "{tag}");
    }
}

fn engine_client() -> EngineClient {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let target = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("target"));
    EngineClient::locate(&root, &target).expect("真 af-engine")
}

#[test]
fn red_gate_closes_honestly_with_three_separate_metrics_and_downstream_rejects() {
    let clock = ManualClock::new(10);
    let (catalog, snapshot) = frozen_catalog(&clock);
    let unit = GateExecuteUnit::with_clock(
        spec(snapshot, "run-red-close"),
        catalog,
        Arc::new(FakeRunner {
            status: GateCheckStatus::Fail,
            skip_reason: None,
            timeout_clock: None,
        }),
        Arc::new(clock),
    );
    let root = scratch("red-close");
    let cfg = DriverConfig {
        order_id: "wo-red-gate".into(),
        graph_version: "graph-gate-v1".into(),
        sla: Sla::new(None, Priority::Normal),
        line: LineBinding {
            line_id: "line-pocf".into(),
            line_version: "1".into(),
        },
        voucher: BudgetVoucher {
            voucher_id: "voucher-red".into(),
            amount: 1,
            signed: true,
        },
        out_dir: root.clone(),
        station_id: "st-pocf".into(),
        host_alias: "local".into(),
        data_root: root.clone(),
        task_id: "gate-task".into(),
        plan_sha256: None,
        task_sha256: None,
        trial: false,
        profile: "gate".into(),
        attempt_layout: af_execline::AttemptLayout::SwevIncrement,
    };
    let mut driver =
        OrderLifecycleDriver::open(cfg, engine_client(), Box::new(FixedClock::new(10))).unwrap();
    let report = driver
        .drive(&unit)
        .expect("红门工件有效，订单应能诚实 close");
    assert_eq!(report.order.state(), OrderState::Closed);
    assert_eq!(report.exec_output.as_ref().unwrap().verdict, Some(false));
    assert!(
        report.order.artifacts()[0].qualified,
        "qualified=工件有效，不是被测通过"
    );
    let output = report.exec_output.as_ref().unwrap();
    let (result, digest) = load_output(output, &root);
    assert_eq!(result.payload.verdict, GateVerdict::Fail);
    assert_eq!(result.payload.failure_code, Some(GateFailureCode::Subject));
    assert_eq!(af_execline::load_gate_output(output).unwrap(), result);

    let metrics: GateOrderMetrics = serde_json::from_slice(
        &std::fs::read(root.join(format!("gate-order-metrics-{digest}.json"))).unwrap(),
    )
    .unwrap();
    assert!(metrics.artifact_valid);
    assert!(!metrics.subject_passed);
    assert!(metrics.order_execution_succeeded);

    let rejection = validate_required_gate_inputs(
        &[RequiredGateInput {
            input_id: "f1".into(),
        }],
        &[ProvidedGateInput {
            input_id: "f1".into(),
            verdict: serde_json::to_value(result.payload.verdict)
                .unwrap()
                .as_str()
                .unwrap()
                .into(),
        }],
    )
    .unwrap_err();
    assert!(matches!(
        rejection,
        GateAdmitRejection::InputGateNotGreen { .. }
    ));
    assert!(
        !root.join("mes-events.jsonl").exists()
            || MesFileWriter::load(&root.join("mes-events.jsonl"))
                .unwrap()
                .iter()
                .all(|event| !event.kind.starts_with("andon.needs_human")),
        "普通 subject fail 不伪造 infra 安灯"
    );
}

#[test]
fn green_gate_has_no_needs_human_andon() {
    let clock = ManualClock::new(10);
    let (catalog, snapshot) = frozen_catalog(&clock);
    let catalog_view = Arc::clone(&catalog);
    let unit = GateExecuteUnit::with_clock(
        spec(snapshot, "run-green"),
        catalog,
        Arc::new(FakeRunner {
            status: GateCheckStatus::Pass,
            skip_reason: None,
            timeout_clock: None,
        }),
        Arc::new(clock),
    );
    let (output, root) = execute_direct("green", "wo-green", &unit);
    let (result, _) = load_output(&output, &root);
    assert_eq!(result.payload.verdict, GateVerdict::Pass);
    assert!(!root.join("mes-events.jsonl").exists());
    assert_eq!(
        catalog_view
            .lock()
            .unwrap()
            .run_record("run-green")
            .unwrap()
            .verdict,
        RunVerdict::Passed
    );
}
