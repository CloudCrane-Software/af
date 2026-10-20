use crate::endpoint::TranscriptItem;
use crate::spec::{decode_spec, EffectClass, ToolDecl};
use crate::stub::{StubResponse, StubServer};
use crate::{
    AuthzCode, Breakpoint, BudgetBinding, BudgetFault, EndpointKey, Error, Launch, OpenStatus,
    Opened, RunStatus, ToolFailure, ToolRegistry, EXAMPLE_SPEC_JSON,
};
use af_credential::{issue_root, RootSpec};
use af_meter::BudgetLedger;
use af_stop::{Decision, FingerprintStore, StopPolicy, StopReason};
use af_trace::{normal_log_path, read_trace_log};
use af_workorder::{BudgetVoucher, GraphVersion, LineBinding, Priority, Sla, WorkOrder};
use biscuit_auth::{Biscuit, PublicKey};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

fn scratch() -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "af-harness-run-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn in_progress(id: &str) -> WorkOrder {
    let mut order = WorkOrder::new(
        id,
        GraphVersion::new("g1"),
        Sla::new(None, Priority::Normal),
    );
    order
        .bind_line(LineBinding {
            line_id: "line-a".into(),
            line_version: "1".into(),
        })
        .unwrap();
    order
        .attach_budget_voucher(BudgetVoucher {
            voucher_id: "v".into(),
            amount: 1,
            signed: true,
        })
        .unwrap();
    order.release().unwrap();
    order.start().unwrap();
    order
}

struct Fixture {
    dir: PathBuf,
    clock: crate::ManualClock,
    token: Biscuit,
    public_key: PublicKey,
    order: WorkOrder,
}

impl Fixture {
    fn new() -> Self {
        let root = issue_root(&RootSpec {
            tools: vec!["git".into(), "pay".into(), "test".into()],
            namespace: "line-a".into(),
            budget_cents: 10_000,
        })
        .unwrap();
        let public_key = root.keypair.public();
        Self {
            dir: scratch(),
            clock: crate::ManualClock::new(1_700_000_000_000),
            token: root.token,
            public_key,
            order: in_progress("wo-1"),
        }
    }

    fn launch(&self, spec: crate::AgentSpec, tools: ToolRegistry) -> Launch<'_> {
        Launch {
            spec,
            run_id: "run-1".into(),
            vars: BTreeMap::from([
                ("task".into(), "发货".into()),
                ("artifacts".into(), "无".into()),
            ]),
            user_message: "开始".into(),
            key: EndpointKey::new("sk-test-secret"),
            key_refresh: None,
            parent: &self.token,
            public_key: &self.public_key,
            namespace: "line-a".into(),
            spend_cap_cents: 100,
            tool_spend_cents: 1,
            budget: cents(10_000),
            order: &self.order,
            tools,
            stop_policy: policy(3, 100),
            breakpoint: Breakpoint::None,
        }
    }
}

fn cents(budget_cents: i64) -> BudgetBinding {
    BudgetBinding::Cents {
        dimension: "llm.cents".into(),
        ledger: BudgetLedger::new(budget_cents),
        spend_cap_cents: 100,
    }
}

fn policy(streak: usize, threshold: usize) -> StopPolicy {
    StopPolicy {
        fingerprint_streak_limit: streak,
        normal_error_threshold: threshold,
        store: FingerprintStore::new(),
    }
}

fn spec_at(url: &str) -> crate::AgentSpec {
    let mut spec = decode_spec(EXAMPLE_SPEC_JSON).unwrap();
    spec.endpoint.base_url = url.into();
    assert_eq!(spec.policy.budget_ref.dimension, "llm.cents");
    spec
}

fn assistant_text(text: &str) -> String {
    json!({
        "choices": [{
            "message": {"role": "assistant", "content": text},
            "finish_reason": "stop"
        }]
    })
    .to_string()
}

fn assistant_calls(text: &str, calls: &[(&str, Value)]) -> String {
    let tool_calls: Vec<_> = calls
        .iter()
        .enumerate()
        .map(|(index, (name, args))| {
            json!({
                "id": format!("call_{index}"),
                "type": "function",
                "function": {
                    "name": name,
                    "arguments": args.to_string(),
                }
            })
        })
        .collect();
    json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": text,
                "tool_calls": tool_calls,
            },
            "finish_reason": "tool_calls"
        }]
    })
    .to_string()
}

fn counting(name: &str, hits: &Arc<AtomicUsize>, tools: &mut ToolRegistry) {
    let hits = Arc::clone(hits);
    tools.register(name, move |_args| {
        hits.fetch_add(1, Ordering::SeqCst);
        Ok(json!({"ok": true}))
    });
}

#[test]
fn pure_tool_stays_local_and_external_once_goes_through_gateway() {
    let fix = Fixture::new();
    let stub = StubServer::spawn().unwrap();
    let mut spec = spec_at(&stub.url());
    spec.policy.max_turns = 1;
    spec.policy.stop.tool_free_turn_limit = 8;
    let git_hits = Arc::new(AtomicUsize::new(0));
    let pay_hits = Arc::new(AtomicUsize::new(0));
    let mut tools = ToolRegistry::new();
    counting("git", &git_hits, &mut tools);
    counting("pay", &pay_hits, &mut tools);
    stub.push(StubResponse::json(
        200,
        assistant_calls(
            "DONE",
            &[
                ("git", json!({"args": "status"})),
                ("pay", json!({"cents": 1})),
            ],
        ),
    ));
    let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
    assert_eq!(opened.status(), OpenStatus::Created);
    let report = opened.run(fix.launch(spec, tools)).unwrap();
    assert!(matches!(report.status, RunStatus::Completed { ref signal } if signal == "DONE"));
    assert_eq!(git_hits.load(Ordering::SeqCst), 1);
    assert_eq!(pay_hits.load(Ordering::SeqCst), 1);
    assert_eq!(report.effect_fresh, 1);
    assert_eq!(report.effect_replays, 0);
    assert_eq!(report.model_calls, 1);
    assert_eq!(report.whitelist, vec!["git".to_string(), "pay".to_string()]);
    let body = &stub.requests()[0].body;
    assert!(body.contains("发货"), "{body}");
    assert!(body.contains("无"), "{body}");
    assert!(!body.contains("sk-test-secret"), "{body}");
    assert!(report
        .transcript
        .iter()
        .all(|item| !matches!(item, TranscriptItem::System { .. })));
    assert!(report.transcript.iter().any(|item| {
        matches!(item, TranscriptItem::Tool { name, is_error: false, .. } if name == "git")
    }));
}

#[test]
fn interrupted_side_effect_replays_on_resume() {
    let fix = Fixture::new();
    let stub = StubServer::spawn().unwrap();
    let mut spec = spec_at(&stub.url());
    spec.policy.max_turns = 4;
    spec.policy.stop.tool_free_turn_limit = 8;
    let hits = Arc::new(AtomicUsize::new(0));
    stub.push(StubResponse::json(
        200,
        assistant_calls("", &[("pay", json!({"cents": 1}))]),
    ));
    stub.push(StubResponse::json(
        200,
        assistant_calls("", &[("pay", json!({"cents": 1}))]),
    ));
    stub.push(StubResponse::json(200, assistant_text("任务 DONE")));
    let err = {
        let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
        let mut tools = ToolRegistry::new();
        counting("pay", &hits, &mut tools);
        counting("git", &Arc::new(AtomicUsize::new(0)), &mut tools);
        let mut launch = fix.launch(spec.clone(), tools);
        launch.breakpoint = Breakpoint::BeforeTurnCommit;
        opened.run(launch).unwrap_err()
    };
    assert!(matches!(err, Error::Interrupted { .. }), "{err}");
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
    assert_eq!(opened.status(), OpenStatus::AlreadyDeclared);
    let mut tools = ToolRegistry::new();
    counting("pay", &hits, &mut tools);
    counting("git", &Arc::new(AtomicUsize::new(0)), &mut tools);
    let report = opened.run(fix.launch(spec, tools)).unwrap();
    assert!(matches!(report.status, RunStatus::Completed { ref signal } if signal == "DONE"));
    assert_eq!(report.effect_fresh, 0);
    assert!(report.effect_replays >= 1, "{}", report.effect_replays);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[test]
fn budget_exhaustion_stops_before_the_next_model_call() {
    let fix = Fixture::new();
    let stub = StubServer::spawn().unwrap();
    let mut spec = spec_at(&stub.url());
    spec.policy.stop.tool_free_turn_limit = 8;
    spec.policy.stop.completion_signals.clear();
    stub.push(StubResponse::json(200, assistant_text("继续")));
    stub.push(StubResponse::json(200, assistant_text("继续")));
    let report = {
        let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
        let mut launch = fix.launch(spec.clone(), ToolRegistry::new());
        launch.budget = cents(2);
        opened.run(launch).unwrap()
    };
    match report.status {
        RunStatus::BudgetExhausted {
            fault:
                BudgetFault::OverBudget {
                    spend_cents: 1,
                    remaining_cents: 0,
                },
        } => {}
        other => panic!("{other:?}"),
    }
    assert_eq!(report.model_calls, 2);
    assert_eq!(stub.requests().len(), 2);

    let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
    assert_eq!(opened.status(), OpenStatus::AlreadyDeclared);
    let mut launch = fix.launch(spec, ToolRegistry::new());
    launch.budget = cents(2);
    let again = opened.run(launch).unwrap();
    assert_eq!(again.status, report.status);
    assert_eq!(stub.requests().len(), 2);
    assert_eq!(again.model_calls, 2);
}

#[test]
fn fingerprint_streak_stops_the_run() {
    let fix = Fixture::new();
    let stub = StubServer::spawn().unwrap();
    let mut spec = spec_at(&stub.url());
    spec.policy.max_turns = 6;
    spec.policy.stop.tool_free_turn_limit = 8;
    spec.policy.stop.completion_signals.clear();
    for _ in 0..3 {
        stub.push(StubResponse::json(
            200,
            assistant_calls("", &[("git", json!({"args": "status"}))]),
        ));
    }
    let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
    let mut tools = ToolRegistry::new();
    tools.register("git", |_args| {
        Err(ToolFailure {
            error_code: "E_TIMEOUT".into(),
            message: "超时".into(),
        })
    });
    tools.register("pay", |_args| Ok(json!({})));
    let report = opened.run(fix.launch(spec, tools)).unwrap();
    assert_eq!(report.model_calls, 3);
    assert_eq!(report.effect_fresh, 0);
    match report.status {
        RunStatus::Stopped { decision } => {
            assert_eq!(
                decision,
                Decision::Stop {
                    reason: StopReason::NoInfluence,
                }
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn tool_outside_intersection_is_refused() {
    let fix = Fixture::new();
    let stub = StubServer::spawn().unwrap();
    let mut spec = spec_at(&stub.url());
    spec.policy.max_turns = 2;
    spec.toolset.push(ToolDecl {
        name: "deploy".into(),
        description: String::new(),
        parameters: json!({"type": "object"}),
        effect: EffectClass::Pure,
    });
    stub.push(StubResponse::json(
        200,
        assistant_calls("", &[("deploy", json!({}))]),
    ));
    let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
    let report = opened
        .run(fix.launch(spec.clone(), ToolRegistry::new()))
        .unwrap();
    match report.status {
        RunStatus::Unauthorized {
            ref tool,
            fact_present: false,
            code: AuthzCode::OutsideIntersection,
            ..
        } if tool == "deploy" => {}
        other => panic!("{other:?}"),
    }
    assert_eq!(report.whitelist, vec!["git".to_string(), "pay".to_string()]);
    assert_eq!(stub.requests().len(), 1);
    drop(opened);
    let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
    let again = opened.run(fix.launch(spec, ToolRegistry::new())).unwrap();
    assert!(matches!(again.status, RunStatus::Unauthorized { .. }));
    assert_eq!(stub.requests().len(), 1);
}

#[test]
fn spend_over_credential_cap_is_refused() {
    let fix = Fixture::new();
    let stub = StubServer::spawn().unwrap();
    let spec = spec_at(&stub.url());
    stub.push(StubResponse::json(
        200,
        assistant_calls("", &[("git", json!({"args": "status"}))]),
    ));
    let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
    let mut tools = ToolRegistry::new();
    tools.register("git", |_| panic!("越权调用不应执行"));
    tools.register("pay", |_| Ok(json!({})));
    let mut launch = fix.launch(spec, tools);
    launch.spend_cap_cents = 100;
    launch.tool_spend_cents = 500;
    let report = opened.run(launch).unwrap();
    match report.status {
        RunStatus::Unauthorized {
            ref tool,
            fact_present: true,
            code: AuthzCode::CredentialDenied,
            ref detail,
            ..
        } if tool == "git" => {
            assert!(
                detail.contains("spend_cents") || detail.contains("<="),
                "{detail}"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn completion_signal_ends_the_run() {
    let fix = Fixture::new();
    let stub = StubServer::spawn().unwrap();
    let spec = spec_at(&stub.url());
    stub.push(StubResponse::json(200, assistant_text("可以发货 DONE")));
    stub.push(StubResponse::json(200, assistant_text("不应再调")));
    let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
    let report = opened.run(fix.launch(spec, ToolRegistry::new())).unwrap();
    assert!(matches!(report.status, RunStatus::Completed { ref signal } if signal == "DONE"));
    assert_eq!(report.model_calls, 1);
    assert_eq!(stub.requests().len(), 1);
}

#[test]
fn tool_free_turn_limit_ends_the_run() {
    let fix = Fixture::new();
    let stub = StubServer::spawn().unwrap();
    let mut spec = spec_at(&stub.url());
    spec.policy.stop.completion_signals.clear();
    spec.policy.stop.tool_free_turn_limit = 2;
    stub.push(StubResponse::json(200, assistant_text("继续")));
    stub.push(StubResponse::json(200, assistant_text("还在继续")));
    stub.push(StubResponse::json(200, assistant_text("不应再调")));
    let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
    let report = opened.run(fix.launch(spec, ToolRegistry::new())).unwrap();
    assert!(matches!(report.status, RunStatus::ToolFreeLimit));
    assert_eq!(report.model_calls, 2);
    assert_eq!(stub.requests().len(), 2);
}

#[test]
fn max_turns_stops_before_another_model_call() {
    let fix = Fixture::new();
    let stub = StubServer::spawn().unwrap();
    let mut spec = spec_at(&stub.url());
    spec.policy.max_turns = 1;
    spec.policy.stop.completion_signals.clear();
    spec.policy.stop.tool_free_turn_limit = 8;
    stub.push(StubResponse::json(200, assistant_text("继续")));
    stub.push(StubResponse::json(200, assistant_text("不应再调")));
    let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
    let report = opened.run(fix.launch(spec, ToolRegistry::new())).unwrap();
    assert!(matches!(report.status, RunStatus::MaxTurns));
    assert_eq!(report.model_calls, 1);
    assert_eq!(stub.requests().len(), 1);
    assert_eq!(report.turns, 1);
}

#[test]
fn tool_event_is_in_the_normal_trace() {
    let fix = Fixture::new();
    let stub = StubServer::spawn().unwrap();
    let mut spec = spec_at(&stub.url());
    spec.policy.max_turns = 1;
    spec.policy.stop.completion_signals.clear();
    spec.policy.stop.tool_free_turn_limit = 8;
    stub.push(StubResponse::json(
        200,
        assistant_calls("", &[("git", json!({"args": "status"}))]),
    ));
    let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
    let mut tools = ToolRegistry::new();
    tools.register("git", |_| Ok(json!({"ok": true})));
    tools.register("pay", |_| Ok(json!({})));
    let report = opened.run(fix.launch(spec, tools)).unwrap();
    assert!(matches!(report.status, RunStatus::MaxTurns));
    drop(opened);
    let events = read_trace_log(&normal_log_path(&fix.dir)).unwrap();
    assert!(events.iter().any(|event| event.kind == "harness.tool"));
    assert!(events.iter().any(|event| event.kind == "harness.model"));
    let normal = std::fs::read_to_string(normal_log_path(&fix.dir)).unwrap();
    assert!(!normal.contains("sk-test-secret"), "{normal}");
}

#[test]
fn missing_required_argument_does_not_call_the_handler() {
    let fix = Fixture::new();
    let stub = StubServer::spawn().unwrap();
    let mut spec = spec_at(&stub.url());
    spec.policy.max_turns = 1;
    spec.policy.stop.completion_signals.clear();
    spec.policy.stop.tool_free_turn_limit = 8;
    stub.push(StubResponse::json(
        200,
        assistant_calls("", &[("git", json!({}))]),
    ));
    let hits = Arc::new(AtomicUsize::new(0));
    let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
    let mut tools = ToolRegistry::new();
    counting("git", &hits, &mut tools);
    let report = opened.run(fix.launch(spec, tools)).unwrap();
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    assert_eq!(report.effect_fresh, 0);
    assert_eq!(report.effect_replays, 0);
    assert!(report.transcript.iter().any(|item| {
        matches!(
            item,
            TranscriptItem::Tool { name, is_error: true, output, .. }
                if name == "git" && output["error_code"] == "E_SCHEMA"
        )
    }));
}

#[test]
fn endpoint_401_refreshes_key_and_retries_the_same_request_once() {
    let fix = Fixture::new();
    let stub = StubServer::spawn().unwrap();
    let mut spec = spec_at(&stub.url());
    spec.policy.max_turns = 4;
    spec.policy.stop.tool_free_turn_limit = 8;
    stub.push(StubResponse::json(401, "invalid token: expired"));
    stub.push(StubResponse::json(200, assistant_text("任务 DONE")));
    let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
    let mut launch = fix.launch(spec, ToolRegistry::new());
    launch.key_refresh = Some(Arc::new(|| Ok("sk-rotated-fresh-key".into())));
    let report = opened.run(launch).unwrap();
    assert!(matches!(report.status, RunStatus::Completed { ref signal } if signal == "DONE"));
    assert_eq!(report.model_calls, 1); // 同一轮内重试，不烧轮次
    let recorded = stub.requests();
    assert_eq!(recorded.len(), 2);
    assert!(recorded[0]
        .headers
        .iter()
        .any(|(name, value)| name == "authorization" && value == "Bearer sk-test-secret"));
    assert!(recorded[1]
        .headers
        .iter()
        .any(|(name, value)| name == "authorization" && value == "Bearer sk-rotated-fresh-key"));
}

#[test]
fn endpoint_401_with_unchanged_key_fails_without_retry() {
    let fix = Fixture::new();
    let stub = StubServer::spawn().unwrap();
    let mut spec = spec_at(&stub.url());
    spec.policy.max_turns = 4;
    stub.push(StubResponse::json(401, "invalid token: expired"));
    let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
    let mut launch = fix.launch(spec, ToolRegistry::new());
    // 重取器返回与旧 key 相同的值 → 密钥面无变化，照常失败且不重发请求。
    launch.key_refresh = Some(Arc::new(|| Ok("sk-test-secret".into())));
    let err = opened.run(launch).unwrap_err();
    assert!(matches!(err, Error::Endpoint { status: 401, .. }), "{err}");
    let text = err.to_string();
    assert!(text.contains("与旧密钥相同"), "{text}");
    assert_eq!(stub.requests().len(), 1);
}

#[test]
fn endpoint_401_without_refresher_fails_unchanged() {
    let fix = Fixture::new();
    let stub = StubServer::spawn().unwrap();
    let mut spec = spec_at(&stub.url());
    spec.policy.max_turns = 4;
    stub.push(StubResponse::json(401, "invalid token: expired"));
    let opened = Opened::open_with_clock(&fix.dir, fix.clock.clone()).unwrap();
    let launch = fix.launch(spec, ToolRegistry::new());
    let err = opened.run(launch).unwrap_err();
    assert!(matches!(err, Error::Endpoint { status: 401, .. }), "{err}");
    assert!(!err.to_string().contains("重取"));
    assert_eq!(stub.requests().len(), 1);
}
