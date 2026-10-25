//! 消债 C 验收测试：HarnessRunner harness 侧账面工单构造件上移 +
//! 真实 harness 集成测试（非 FakeRunner）覆盖订单态门。
//!
//! P4 吸收按"af-harness 只查 id 非空（run.rs:221-223）"的误判省略了旧路
//! bench_order 四步生命周期（bind_line/voucher/release/start）——真实
//! af-harness effect gateway 对 declare 有订单态门（要求 [Released,
//! InProgress]，af-effect-gateway lib.rs:132），Created 态首个 declare 即拒；
//! FakeRunner（P4/P5 验证缝）与 L0 回放（P6）都无法暴露，F-首单首次真跑
//! 触发（EX3001：订单态拒绝 declare）。
//!
//! 本组测试用**真 af-harness**（真 Opened/真 effect gateway/真 ToolRegistry/
//! 真 RunCredential 签发；模型 HTTP 端点按 af-harness cargo test 惯例走本
//! crate 的 stub 回放——真供应商调用留 examples/smoke.rs 同款纪律）：
//! ①门钉：Created 工单首个 bash side-effect 被 gateway 拒（门真实存在）；
//! ②构造件：[`af_execline::bench_paper_order`] 四步后 declare 放行，
//! HarnessRunner 全链 Ok（当前红线：无构造件时同链路被门拒）。

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;

use af_execline::agent::{AgentLaunch, AgentRunner, HarnessRunner};
use af_harness::stub::{StubResponse, StubServer};
use af_harness::{
    AgentSpec, Breakpoint, BudgetBinding, BudgetRef, BudgetUnit, ContextTemplate, EffectClass,
    EndpointKey, EndpointSpec, Launch, Opened, Policy, Protocol, ReasoningTier, RunStatus,
    StopCondition, ToolDecl, ToolRegistry,
};
use af_meter::BudgetLedger;
use af_stop::StopPolicy;
use af_workorder::{
    BudgetVoucher, GraphVersion, LineBinding, OrderState, Priority, Sla, WorkOrder,
};
use serde_json::{json, Value};

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时钟")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "af-execline-paper-order-{}-{}-{}",
        process::id(),
        tag,
        nanos
    ));
    fs::create_dir_all(&dir).expect("创建测试 scratch 目录");
    dir
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 旧行 `printf` 假命令（≥16 字符过 resolve_key 下限；零真实凭证消耗）。
const FAKE_KEY_CMD: &str = "printf '%s' fake-key-0123456789abcd";

fn bash_tool() -> ToolDecl {
    ToolDecl {
        name: "bash".into(),
        description: "test bash".into(),
        parameters: json!({
            "type": "object",
            "properties": {"command": {"type": "string"}},
            "required": ["command"]
        }),
        effect: EffectClass::ExternalOnce,
    }
}

fn paper_spec(url: &str) -> AgentSpec {
    AgentSpec {
        endpoint: EndpointSpec {
            protocol: Protocol::OpenaiChatCompletions,
            base_url: url.into(),
            model: "stub-model".into(),
            key_ref: "paper-test".into(),
            reasoning_tier: ReasoningTier::Off,
            stream: false,
            auth: None,
            max_tokens: None,
            temperature_milli: None,
            top_p_milli: None,
        },
        context_template: ContextTemplate {
            system: "你是产线执行器。".into(),
            slots: Vec::new(),
        },
        toolset: vec![bash_tool()],
        policy: Policy {
            max_turns: 5,
            stop: StopCondition {
                completion_signals: vec!["TASK_COMPLETE".into()],
                tool_free_turn_limit: 3,
            },
            budget_ref: BudgetRef {
                dimension: "llm.cents".into(),
                per_turn: 1,
                unit: BudgetUnit::Cents,
            },
        },
    }
}

/// OpenAI ChatCompletions 形态的脚本化响应：一轮 bash 工具调用。
fn one_bash_call(content: &str) -> String {
    json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": content,
                "tool_calls": [{
                    "id": "call_0",
                    "type": "function",
                    "function": {
                        "name": "bash",
                        "arguments": json!({"command": "echo hi"}).to_string(),
                    }
                }],
            },
            "finish_reason": "tool_calls"
        }]
    })
    .to_string()
}

fn fresh_bench_order(id: &str) -> WorkOrder {
    // 与 HarnessRunner::run 的构造同款基底（Created 态）。
    WorkOrder::new(
        id,
        GraphVersion::new("bench"),
        Sla::new(None, Priority::Normal),
    )
}

// ---------- ③ 构造件形状（正式构造件，run 只调用） ----------

/// [`bench_paper_order`] = 旧路 bench_order 语句级对照四步：
/// bind_line(bench-line/1) + voucher(v-{run_id}, 1, signed) + release + start
/// → 终态 InProgress（effect gateway 的 declare 允许集）。
#[test]
fn bench_paper_order_matches_old_bench_order_four_steps() {
    let order = af_execline::bench_paper_order("django__x-1-a1").expect("四步构造应成功");
    assert_eq!(order.id(), "django__x-1-a1");
    assert_eq!(order.state(), OrderState::InProgress);
    assert_eq!(
        order.line(),
        Some(&LineBinding {
            line_id: "bench-line".into(),
            line_version: "1".into(),
        })
    );
    assert_eq!(
        order.budget_voucher(),
        Some(&BudgetVoucher {
            voucher_id: "v-django__x-1-a1".into(),
            amount: 1,
            signed: true,
        })
    );
    // 与裸构造的对照：无四步 = Created（这正是 F-首单暴露的拒绝形态）
    assert_eq!(
        fresh_bench_order("django__x-1-a1").state(),
        OrderState::Created
    );
}

// ---------- ①② 真实 harness 集成（非 FakeRunner） ----------

/// 门钉：真 Opened + 真 effect gateway 下，Created 态工单的首个 bash
/// side-effect（declare）被订单态门拒（允许 [Released, InProgress]）。
/// 此测试在构造件存在前后都必须保持绿——它钉住的是门本身。
#[test]
fn created_order_first_side_effect_is_rejected_by_real_gateway() {
    let dir = Scratch(scratch("gate-pin"));
    let stub = StubServer::spawn().unwrap();
    stub.push(StubResponse::json(200, one_bash_call("working on it")));
    let root = af_credential::issue_root(&af_credential::RootSpec {
        tools: vec!["bash".to_string()],
        namespace: "bench".into(),
        budget_cents: 10_000,
    })
    .unwrap();
    let mut registry = ToolRegistry::new();
    registry.register("bash", |_call| Ok(Value::String("TASK_COMPLETE".into())));
    let order = fresh_bench_order("wo-gate-pin-1");
    let opened = Opened::open(&dir.0).unwrap();
    let result = opened.run(Launch {
        spec: paper_spec(&stub.url()),
        run_id: "run-gate-pin-1".into(),
        vars: BTreeMap::new(),
        user_message: "开始".into(),
        key: EndpointKey::new("fake-key-0123456789abcd"),
        key_refresh: None,
        parent: &root.token,
        public_key: &root.keypair.public(),
        namespace: "bench".into(),
        spend_cap_cents: 100,
        tool_spend_cents: 1,
        budget: BudgetBinding::Cents {
            dimension: "llm.cents".into(),
            ledger: BudgetLedger::new(100),
            spend_cap_cents: 100,
        },
        order: &order,
        tools: registry,
        stop_policy: StopPolicy::default(),
        breakpoint: Breakpoint::None,
    });
    let err = result.expect_err("Created 态首个 declare 必须被真 gateway 拒");
    assert!(
        err.to_string().contains("订单态拒绝"),
        "应为订单态门拒绝，实际: {err}"
    );
    assert!(err.to_string().contains("Created"), "{err}");
}

/// 构造件放行：HarnessRunner（真实现，非 FakeRunner）全链——issue_root →
/// 工具注册 → Opened::open → **账面工单四步构造** → Opened::run → 模型
/// （stub 端点）→ bash side-effect 经真 gateway declare/execute → 完成。
/// 消债前红线：无四步构造时同链路死于"订单态拒绝 declare"（F-首单 EX3001
/// 原地复现）；构造件上移后 Ok 且 effect_fresh ≥ 1。
#[test]
fn harness_runner_paper_order_passes_real_gateway_declare() {
    let dir = Scratch(scratch("runner-green"));
    let stub = StubServer::spawn().unwrap();
    stub.push(StubResponse::json(200, one_bash_call("run the fix")));
    let launch = AgentLaunch {
        attempt_dir: &dir.0,
        run_id: "wo-paper-a1".into(),
        user_message: "开始".into(),
        spec: paper_spec(&stub.url()),
        bash: Arc::new(|_command| "TASK_COMPLETE".to_string()),
        tools: Vec::new(), // 图执行器面；本测试环无附加工具
        key_cmd: FAKE_KEY_CMD,
        spend_cap_cents: 100,
        tool_spend_cents: 1,
        budget_ledger_cents: 100,
    };
    let runner = HarnessRunner::new().unwrap();
    let outcome = match runner.run(launch) {
        Ok(outcome) => outcome,
        Err(failure) => panic!(
            "构造件放行后，真实 gateway 的 declare/execute 应全链通过，实际失败: {}",
            failure.message
        ),
    };
    assert!(
        matches!(outcome.report.status, RunStatus::Completed { .. }),
        "bash 输出首行 TASK_COMPLETE 应触发完成: {:?}",
        outcome.report.status
    );
    assert!(
        outcome.report.effect_fresh >= 1,
        "bash side-effect 应经真 gateway 执行（非本地工具）"
    );
    assert_eq!(outcome.report.model_calls, 1);
    assert_eq!(outcome.http_retries, 0);
}
