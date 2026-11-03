//! 独立门单执行单元：冻结 Evaluation → check runner → CAS 门工件 → 安灯。
//!
//! 本模块不定义 GateOrder，也不新增图节点型。直接驱动时它是既有
//! [`ExecuteUnit`]；图内由 `GraphExecuteUnit` 的既有 `check` 分派调用。
//! `WorkArtifact.qualified` 在门单只表示“结构有效的终态工件可登记”，被测是否
//! 通过永远只看 `af.gate_result.v1.payload.verdict`，二者不得互换。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use af_eval::{
    classify_gate, gate_definitions, Catalog, Clock as EvalClock, EvalError, ExecSignals,
    FailureCode, GateCheckDefinition, GateCheckResult, GateCheckStatus, GateError, GateEvalRef,
    GateEvidence, GateInput, GateResult, GateResultEnvelope, GateResultStore, GateRunnerRef,
    GateSubject, GateSuiteRef, GateVerdict, RunSpec, RunVerdict, SkipAccounting, SkipReason,
    SystemClock as EvalSystemClock, GATE_RESULT_TYPE,
};
use af_workorder::{OrderState, WorkArtifact, WorkOrder};
use serde::{Deserialize, Serialize};

use crate::error::ExeclineError;
use crate::exec::{ExecContext, ExecOutput, ExecuteUnit};
use crate::mes::MesFileWriter;

pub const GATE_UNIT_ID: &str = "gate-eval-unit";
pub const GATE_CHECK_IMPL: &str = "pkg.af.gate_result";
pub const GATE_ARTIFACT_PREFIX: &str = "af.gate_result.v1:";

#[derive(Debug, Clone)]
pub struct GateUnitSpec {
    pub lineage_id: String,
    pub version: u64,
    /// 组装订单时钉住的冻结快照；运行视图不符 = E_BENCH。
    pub snapshot_hash: String,
    pub run_id: String,
    pub subject: GateSubject,
    pub recipe_digest: String,
    pub runner: GateRunnerRef,
    pub inputs: Vec<GateInput>,
    /// 污染交集输入；禁止把冻结门定义 payload 当 subject input 再注入。
    pub contamination_input: Vec<String>,
    pub budget_ms: i64,
}

/// runner 的结构化回报。进程退出码不在这个结构里，不能单独成为 pass 依据。
#[derive(Debug, Clone, Default)]
pub struct GateRunnerOutput {
    pub checks: Vec<GateCheckResult>,
    pub evidence: Vec<GateEvidence>,
    pub signals: ExecSignals,
}

pub trait GateCheckRunner: Send + Sync {
    fn run(&self, definitions: &[GateCheckDefinition], ctx: &ExecContext<'_>) -> GateRunnerOutput;

    /// 可验真的 runner 返回实际 digest；验真不符按 E_INFRA，不运行检查。
    fn implementation_digest(&self) -> Option<String> {
        None
    }
}

/// 通用 JSON stdin/stdout runner。每项检查调用一次，无 shell 拼接。
#[derive(Debug, Clone)]
pub struct ProcessGateRunner {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub per_check_timeout: Duration,
}

impl ProcessGateRunner {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            per_check_timeout: Duration::from_secs(60),
        }
    }
}

impl GateCheckRunner for ProcessGateRunner {
    fn run(&self, definitions: &[GateCheckDefinition], ctx: &ExecContext<'_>) -> GateRunnerOutput {
        let mut out = GateRunnerOutput::default();
        for definition in definitions {
            let check = self.run_one(definition, ctx, &mut out);
            if check.required && check.status == GateCheckStatus::Fail {
                out.signals.subject = true;
            }
            out.checks.push(check);
        }
        out
    }

    fn implementation_digest(&self) -> Option<String> {
        crate::exec::file_sha256_hex(&self.program)
    }
}

impl ProcessGateRunner {
    fn run_one(
        &self,
        definition: &GateCheckDefinition,
        _ctx: &ExecContext<'_>,
        out: &mut GateRunnerOutput,
    ) -> GateCheckResult {
        let skipped = |reason: SkipReason, detail: String| GateCheckResult {
            check_id: definition.check_id.clone(),
            required: definition.required,
            status: GateCheckStatus::Skip,
            skip_reason: Some(reason),
            detail,
        };
        let payload = match serde_json::to_vec(definition) {
            Ok(payload) => payload,
            Err(err) => {
                out.signals.bench = true;
                return skipped(SkipReason::NotApplicable, format!("门定义编码失败: {err}"));
            }
        };
        let mut child = match Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                out.signals.infra = true;
                return skipped(
                    SkipReason::EnvironmentUnavailable,
                    format!("runner 不可启动 {}: {err}", self.program.display()),
                );
            }
        };
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(err) = stdin.write_all(&payload) {
                let _ = child.kill();
                let _ = child.wait();
                out.signals.infra = true;
                return skipped(
                    SkipReason::EnvironmentUnavailable,
                    format!("runner stdin 写入失败: {err}"),
                );
            }
        }
        let started = Instant::now();
        let timed_out = loop {
            match child.try_wait() {
                Ok(Some(_)) => break false,
                Ok(None) if started.elapsed() < self.per_check_timeout => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(None) => break true,
                Err(err) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    out.signals.infra = true;
                    return skipped(
                        SkipReason::EnvironmentUnavailable,
                        format!("runner wait 失败: {err}"),
                    );
                }
            }
        };
        if timed_out {
            let _ = child.kill();
        }
        let output = match child.wait_with_output() {
            Ok(output) => output,
            Err(err) => {
                out.signals.infra = true;
                return skipped(
                    SkipReason::EnvironmentUnavailable,
                    format!("runner 输出回收失败: {err}"),
                );
            }
        };
        out.evidence.push(inline_evidence(
            &format!("{}.stdout", definition.check_id),
            &output.stdout,
        ));
        out.evidence.push(inline_evidence(
            &format!("{}.stderr", definition.check_id),
            &output.stderr,
        ));
        if timed_out {
            // Catalog 的墙钟预算拥有最终 E_TIMEOUT 路由；infra 位保证假钟/较长预算
            // 下也不会把超时判成 subject fail。
            out.signals.infra = true;
            return skipped(
                SkipReason::EnvironmentUnavailable,
                format!(
                    "runner 超过 {}ms，已 kill",
                    self.per_check_timeout.as_millis()
                ),
            );
        }
        let check: GateCheckResult = match serde_json::from_slice(&output.stdout) {
            Ok(check) => check,
            Err(err) => {
                out.signals.bench = true;
                return skipped(
                    SkipReason::NotApplicable,
                    format!("runner 未返回 GateCheckResult JSON: {err}"),
                );
            }
        };
        if check.check_id != definition.check_id || check.required != definition.required {
            out.signals.bench = true;
            return skipped(
                SkipReason::NotApplicable,
                "runner 回报的 check_id/required 与冻结定义不符".into(),
            );
        }
        if !output.status.success() && check.status == GateCheckStatus::Pass {
            out.signals.bench = true;
            return skipped(
                SkipReason::NotApplicable,
                format!(
                    "runner 退出码 {} 与结构化 PASS 冲突，拒绝判绿",
                    output.status
                ),
            );
        }
        // 退出码仅作冲突检测；结构化 FAIL/SKIP 原样入账，绝不把 exit 0 改成 PASS。
        check
    }
}

pub struct GateExecuteUnit {
    spec: GateUnitSpec,
    catalog: Arc<Mutex<Catalog>>,
    runner: Arc<dyn GateCheckRunner>,
    clock: Arc<dyn EvalClock>,
}

impl std::fmt::Debug for GateExecuteUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GateExecuteUnit")
            .field("unit_id", &self.unit_id())
            .field("run_id", &self.spec.run_id)
            .finish_non_exhaustive()
    }
}

impl GateExecuteUnit {
    pub fn new(
        spec: GateUnitSpec,
        catalog: Arc<Mutex<Catalog>>,
        runner: Arc<dyn GateCheckRunner>,
    ) -> Self {
        Self::with_clock(spec, catalog, runner, Arc::new(EvalSystemClock::new()))
    }

    pub fn with_clock(
        spec: GateUnitSpec,
        catalog: Arc<Mutex<Catalog>>,
        runner: Arc<dyn GateCheckRunner>,
        clock: Arc<dyn EvalClock>,
    ) -> Self {
        Self {
            spec,
            catalog,
            runner,
            clock,
        }
    }

    pub fn spec(&self) -> &GateUnitSpec {
        &self.spec
    }

    fn evaluate(&self, ctx: &ExecContext<'_>) -> Result<GateResultEnvelope, ExeclineError> {
        let mut catalog = self.catalog.lock().unwrap_or_else(|e| e.into_inner());
        let ledger_id = catalog.ledger_id().to_string();
        let fallback_started = self.clock.now_ms();
        let begun = catalog.begin_run(&RunSpec {
            run_id: &self.spec.run_id,
            lineage_id: &self.spec.lineage_id,
            version: self.spec.version,
            subject_id: &self.spec.subject.repo,
            subject_version: &self.spec.subject.version,
            input: &self.spec.contamination_input,
            budget_ms: self.spec.budget_ms,
        });
        let view = match begun {
            Ok(view) => view,
            Err(err) => {
                let (run, started, finished, evidence) = match &err {
                    EvalError::Contaminated { .. } => {
                        let record = catalog.run_record(&self.spec.run_id).map_err(eval_unit)?;
                        (
                            record.verdict,
                            record.started_at_ms,
                            record.recorded_at_ms.unwrap_or_else(|| self.clock.now_ms()),
                            inline_evidence("contamination", err.to_string().as_bytes()),
                        )
                    }
                    _ => {
                        let code = begin_failure_code(&err);
                        (
                            RunVerdict::Failed(code),
                            fallback_started,
                            self.clock.now_ms(),
                            inline_evidence("begin_run", err.to_string().as_bytes()),
                        )
                    }
                };
                return self.assemble(
                    ctx,
                    &ledger_id,
                    run,
                    Vec::new(),
                    vec![evidence],
                    started,
                    finished,
                );
            }
        };
        let started = view.started_at_ms;
        if view.snapshot.hash != self.spec.snapshot_hash {
            let record = catalog
                .finish_run(
                    &self.spec.run_id,
                    ExecSignals {
                        bench: true,
                        ..ExecSignals::default()
                    },
                )
                .map_err(eval_unit)?;
            return self.assemble(
                ctx,
                &ledger_id,
                record.verdict,
                Vec::new(),
                vec![inline_evidence(
                    "snapshot_pin",
                    b"frozen snapshot hash mismatch",
                )],
                started,
                record.recorded_at_ms.unwrap_or(started),
            );
        }
        if self
            .runner
            .implementation_digest()
            .is_some_and(|actual| actual != self.spec.runner.digest)
        {
            let record = catalog
                .finish_run(
                    &self.spec.run_id,
                    ExecSignals {
                        infra: true,
                        ..ExecSignals::default()
                    },
                )
                .map_err(eval_unit)?;
            return self.assemble(
                ctx,
                &ledger_id,
                record.verdict,
                Vec::new(),
                vec![inline_evidence(
                    "runner_digest",
                    b"runner implementation digest mismatch",
                )],
                started,
                record.recorded_at_ms.unwrap_or(started),
            );
        }
        let definitions = match gate_definitions(&view) {
            Ok(definitions) => definitions,
            Err(err) => {
                let record = catalog
                    .finish_run(
                        &self.spec.run_id,
                        ExecSignals {
                            bench: true,
                            ..ExecSignals::default()
                        },
                    )
                    .map_err(eval_unit)?;
                return self.assemble(
                    ctx,
                    &ledger_id,
                    record.verdict,
                    Vec::new(),
                    vec![inline_evidence(
                        "gate_definition",
                        err.to_string().as_bytes(),
                    )],
                    started,
                    record.recorded_at_ms.unwrap_or(started),
                );
            }
        };
        let runner_output = self.runner.run(&definitions, ctx);
        let (checks, normalization_bench) = normalize_checks(&definitions, runner_output.checks);
        let mut signals = runner_output.signals;
        signals.bench |= normalization_bench;
        let record = catalog
            .finish_run(&self.spec.run_id, signals)
            .map_err(eval_unit)?;
        let finished = record.recorded_at_ms.unwrap_or(started);
        self.assemble(
            ctx,
            &ledger_id,
            record.verdict,
            checks,
            runner_output.evidence,
            started,
            finished,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn assemble(
        &self,
        ctx: &ExecContext<'_>,
        ledger_id: &str,
        run: RunVerdict,
        checks: Vec<GateCheckResult>,
        evidence: Vec<GateEvidence>,
        started_at_ms: i64,
        finished_at_ms: i64,
    ) -> Result<GateResultEnvelope, ExeclineError> {
        let (verdict, failure_code) = classify_gate(&run, &checks);
        let result = GateResultEnvelope::new(GateResult {
            producer_order_id: ctx.order_id.to_string(),
            eval: GateEvalRef {
                ledger_id: ledger_id.into(),
                run_id: self.spec.run_id.clone(),
            },
            suite: GateSuiteRef {
                lineage_id: self.spec.lineage_id.clone(),
                version: self.spec.version,
                snapshot_hash: self.spec.snapshot_hash.clone(),
            },
            subject: self.spec.subject.clone(),
            recipe_digest: self.spec.recipe_digest.clone(),
            runner: self.spec.runner.clone(),
            inputs: self.spec.inputs.clone(),
            skip_accounting: SkipAccounting::of(&checks),
            checks,
            verdict,
            failure_code,
            evidence,
            started_at_ms,
            finished_at_ms,
        });
        result.validate().map_err(gate_unit)?;
        Ok(result)
    }
}

impl ExecuteUnit for GateExecuteUnit {
    fn unit_id(&self) -> &str {
        GATE_UNIT_ID
    }

    fn execute(&self, ctx: &ExecContext<'_>) -> Result<ExecOutput, ExeclineError> {
        let result = self.evaluate(ctx)?;
        let store = GateResultStore::new(ctx.out_dir);
        let stored = store.persist(&result).map_err(gate_unit)?;
        if result.payload.verdict == GateVerdict::NeedsHuman {
            emit_andon(ctx, &result, &stored.content_sha256)?;
        }
        let verdict = match result.payload.verdict {
            GateVerdict::Pass => Some(true),
            GateVerdict::Fail => Some(false),
            GateVerdict::NeedsHuman => None,
        };
        let artifact_id = format!("{GATE_ARTIFACT_PREFIX}{}", stored.content_sha256);
        // PoC-A 0.2.0 线协议：工件摘要只认实际字节。这里的字节就是 persist 落盘
        // 的同一份（persistent_bytes），sha256 与 artifact_id 内嵌 digest 自洽。
        let artifact_bytes = result.persistent_bytes().map_err(gate_unit)?;
        Ok(ExecOutput {
            artifacts: vec![WorkArtifact::from_bytes(
                artifact_id,
                true,
                // 红门/黄门也能诚实关单：这里只证明 CAS 工件结构有效，绝不表示被测通过。
                true,
                &artifact_bytes,
            )],
            // 实际字节随 ExecOutput 透传，引擎写入口按字节重算摘要，不信自报值。
            artifact_contents: vec![artifact_bytes],
            outputs: vec![stored.path.clone()],
            note: format!(
                "门工件有效=true；被测 verdict={:?}；门单执行单元成功=true（订单成功由 close 单独记）",
                result.payload.verdict
            ),
            verdict,
            record: Some(serde_json::json!({
                "artifact_type": GATE_RESULT_TYPE,
                "content_sha256": stored.content_sha256,
                "artifact_valid": true,
                "verdict": result.payload.verdict,
            })),
            evidence: vec![stored.path],
        })
    }
}

/// Graph check 分派读取同一份 CAS 工件，不复制判定逻辑。
pub fn load_gate_output(output: &ExecOutput) -> Result<GateResultEnvelope, ExeclineError> {
    let marker = output
        .record
        .as_ref()
        .ok_or_else(|| ExeclineError::UnitFailed {
            detail: "gate unit 缺执行标记".into(),
        })?;
    let digest = marker["content_sha256"]
        .as_str()
        .ok_or_else(|| ExeclineError::UnitFailed {
            detail: "gate unit 标记缺 content_sha256".into(),
        })?;
    let root = output
        .outputs
        .first()
        .and_then(|path| {
            // outputs = <order>/artifacts/cas/sha256/aa/digest.json；向上四级到 artifacts。
            path.parent()?.parent()?.parent()?.parent()
        })
        .ok_or_else(|| ExeclineError::UnitFailed {
            detail: "gate unit 输出路径不是预期 CAS 布局".into(),
        })?;
    GateResultStore::new(root).load(digest).map_err(gate_unit)
}

fn normalize_checks(
    definitions: &[GateCheckDefinition],
    reported: Vec<GateCheckResult>,
) -> (Vec<GateCheckResult>, bool) {
    let mut bench = false;
    let mut checks = Vec::with_capacity(definitions.len());
    for definition in definitions {
        let mut matches = reported
            .iter()
            .filter(|c| c.check_id == definition.check_id);
        let first = matches.next();
        if matches.next().is_some() {
            bench = true;
        }
        match first {
            Some(check) if check.required == definition.required => checks.push(check.clone()),
            _ => {
                bench = true;
                checks.push(GateCheckResult {
                    check_id: definition.check_id.clone(),
                    required: definition.required,
                    status: GateCheckStatus::Skip,
                    skip_reason: Some(SkipReason::ExplicitlyExcluded),
                    detail: "runner 缺失/重复回报或 required 漂移".into(),
                });
            }
        }
    }
    if reported.iter().any(|actual| {
        !definitions
            .iter()
            .any(|definition| definition.check_id == actual.check_id)
    }) {
        bench = true;
    }
    (checks, bench)
}

fn begin_failure_code(err: &EvalError) -> FailureCode {
    match err {
        EvalError::Contaminated { .. } | EvalError::ClockRegressed { .. } => FailureCode::Timeout,
        EvalError::Sqlite(_)
        | EvalError::DeclarationMismatch { .. }
        | EvalError::UnknownSchema { .. }
        | EvalError::SchemaDrift { .. }
        | EvalError::TornInitialization => FailureCode::Infra,
        _ => FailureCode::Bench,
    }
}

fn inline_evidence(name: &str, bytes: &[u8]) -> GateEvidence {
    GateEvidence {
        name: name.into(),
        uri: format!("inline:{name}"),
        sha256: af_eval::sha256_hex(bytes),
    }
}

fn eval_unit(err: EvalError) -> ExeclineError {
    ExeclineError::UnitFailed {
        detail: format!("门 Evaluation 账本失败: {err}"),
    }
}

fn gate_unit(err: GateError) -> ExeclineError {
    ExeclineError::UnitFailed {
        detail: format!("门工件失败: {err}"),
    }
}

/// 机制11状态面复用 MES 事件账本；needs_human 不转成 fail，也不豁免下游。
fn emit_andon(
    ctx: &ExecContext<'_>,
    result: &GateResultEnvelope,
    digest: &str,
) -> Result<(), ExeclineError> {
    let code = result
        .payload
        .failure_code
        .map(|c| {
            serde_json::to_value(c)
                .unwrap_or_default()
                .as_str()
                .unwrap_or("E_INFRA")
                .to_string()
        })
        .unwrap_or_else(|| "E_INFRA".into());
    let kind = format!("andon.needs_human.{code}");
    let event_id = format!(
        "{}:{kind}:{}:{digest}",
        ctx.stable_id, result.payload.eval.run_id
    );
    let order_root = ctx
        .out_dir
        .parent()
        .ok_or_else(|| ExeclineError::UnitFailed {
            detail: "门单 out_dir 无订单根，无法触发安灯".into(),
        })?;
    let path = order_root.join("mes-events.jsonl");
    if path.is_file() {
        let events = MesFileWriter::load(&path)?;
        if events.iter().any(|event| event.event_id == event_id) {
            return Ok(());
        }
    }
    let event = af_mes::MesEvent::new(
        ctx.stable_id,
        result.payload.finished_at_ms,
        event_id,
        ctx.line_id,
        "gate",
        kind,
        1,
    );
    MesFileWriter::new(path).append_event(&event)
}

/// 红门单三分的持久侧车。由订单驱动在 close 之后写入，此时 order_execution_succeeded
/// 才是事实而非预言。工件有效 ≠ 被测通过 ≠ 订单执行成功。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateOrderMetrics {
    pub producer_order_id: String,
    pub gate_result_sha256: String,
    pub artifact_valid: bool,
    pub subject_passed: bool,
    pub order_execution_succeeded: bool,
}

pub fn gate_digest_from_record(record: Option<&serde_json::Value>) -> Option<&str> {
    let record = record?;
    (record["artifact_type"].as_str() == Some(GATE_RESULT_TYPE))
        .then(|| record["content_sha256"].as_str())?
}

pub fn persist_gate_order_metrics(
    order_root: &Path,
    order: &WorkOrder,
    result: &GateResultEnvelope,
    digest: &str,
) -> Result<PathBuf, ExeclineError> {
    let persistent = result.persistent_bytes().map_err(gate_unit)?;
    let artifact_valid = af_eval::sha256_hex(&persistent) == digest;
    let order_execution_succeeded = order.state() == OrderState::Closed
        && order.artifacts().iter().any(|artifact| {
            artifact.qualified
                && artifact.is_final
                && artifact.artifact_id == format!("{GATE_ARTIFACT_PREFIX}{digest}")
        });
    let metrics = GateOrderMetrics {
        producer_order_id: order.id().into(),
        gate_result_sha256: digest.into(),
        artifact_valid,
        subject_passed: result.payload.verdict == GateVerdict::Pass,
        order_execution_succeeded,
    };
    let bytes = serde_json::to_vec(&metrics).map_err(|err| ExeclineError::UnitFailed {
        detail: format!("门单三分指标序列化失败: {err}"),
    })?;
    let path = order_root.join(format!("gate-order-metrics-{digest}.json"));
    if path.is_file() {
        let existing = std::fs::read(&path).map_err(|err| ExeclineError::LedgerIo {
            detail: format!("门单三分指标回读失败 {}: {err}", path.display()),
        })?;
        if existing == bytes {
            return Ok(path);
        }
        return Err(ExeclineError::LedgerCorrupt {
            detail: format!("门单三分指标已存在且内容不同，拒绝覆盖: {}", path.display()),
        });
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|err| ExeclineError::LedgerIo {
            detail: format!("门单三分指标落盘失败 {}: {err}", path.display()),
        })?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|err| ExeclineError::LedgerIo {
            detail: format!("门单三分指标写入失败: {err}"),
        })?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_runner_result_is_bench_skip_not_pass() {
        let definitions = vec![GateCheckDefinition {
            check_id: "required".into(),
            required: true,
            input: serde_json::Value::Null,
        }];
        let (checks, bench) = normalize_checks(&definitions, Vec::new());
        assert!(bench);
        assert_eq!(checks[0].status, GateCheckStatus::Skip);
        assert_eq!(
            classify_gate(&RunVerdict::Passed, &checks).0,
            GateVerdict::NeedsHuman
        );
    }

    #[test]
    fn inline_evidence_hashes_real_bytes() {
        let evidence = inline_evidence("stdout", b"not an exit code");
        assert_eq!(evidence.sha256, af_eval::sha256_hex(b"not an exit code"));
    }
}
