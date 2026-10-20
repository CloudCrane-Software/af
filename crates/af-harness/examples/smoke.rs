//! 手动冒烟。不设置环境变量时直接退出，不访问网络。
//!
//! 需要同时提供：
//! `AF_HARNESS_BASE_URL`、`AF_HARNESS_API_KEY`、`AF_HARNESS_MODEL`。
//! 可选 `AF_HARNESS_PROTOCOL`：`openai-chat-completions`（默认）或 `anthropic-messages`。
//! 明文密钥只进请求头，不打印。

use af_credential::{issue_root, RootSpec};
use af_harness::{
    AgentSpec, AuthStyle, BudgetBinding, BudgetRef, BudgetUnit, ContextTemplate, EndpointKey,
    EndpointSpec, Launch, Opened, Policy, Protocol, ReasoningTier, RunStatus, StopCondition,
    ToolRegistry,
};
use af_meter::BudgetLedger;
use af_stop::StopPolicy;
use af_workorder::{BudgetVoucher, GraphVersion, LineBinding, Priority, Sla, WorkOrder};
use std::collections::BTreeMap;

fn main() {
    if let Err(err) = smoke() {
        eprintln!("af-harness smoke failed: {err}");
        std::process::exit(1);
    }
}

fn smoke() -> Result<(), Box<dyn std::error::Error>> {
    let Ok(base_url) = std::env::var("AF_HARNESS_BASE_URL") else {
        skip();
        return Ok(());
    };
    let Ok(api_key) = std::env::var("AF_HARNESS_API_KEY") else {
        skip();
        return Ok(());
    };
    let Ok(model) = std::env::var("AF_HARNESS_MODEL") else {
        skip();
        return Ok(());
    };
    if base_url.is_empty() || api_key.is_empty() || model.is_empty() {
        skip();
        return Ok(());
    }
    let protocol = match std::env::var("AF_HARNESS_PROTOCOL")
        .unwrap_or_else(|_| "openai-chat-completions".into())
        .as_str()
    {
        "openai-chat-completions" => Protocol::OpenaiChatCompletions,
        "anthropic-messages" => Protocol::AnthropicMessages,
        other => {
            return Err(format!(
                "AF_HARNESS_PROTOCOL 只能是 openai-chat-completions 或 anthropic-messages，收到 {other}"
            )
            .into());
        }
    };
    let auth = match std::env::var("AF_HARNESS_AUTH").ok().as_deref() {
        None => None,
        Some("bearer") => Some(AuthStyle::Bearer),
        Some("x-api-key") => Some(AuthStyle::XApiKey),
        Some(other) => {
            return Err(format!("AF_HARNESS_AUTH 只能是 bearer 或 x-api-key，收到 {other}").into());
        }
    };
    let spec = AgentSpec {
        endpoint: EndpointSpec {
            protocol,
            base_url,
            model,
            key_ref: "env".into(),
            reasoning_tier: ReasoningTier::Off,
            stream: false,
            auth,
            max_tokens: None,
            temperature_milli: None,
            top_p_milli: None,
        },
        context_template: ContextTemplate {
            system: "你是冒烟探针。只回答一个词。".into(),
            slots: Vec::new(),
        },
        toolset: Vec::new(),
        policy: Policy {
            max_turns: 1,
            stop: StopCondition {
                completion_signals: Vec::new(),
                tool_free_turn_limit: 8,
            },
            budget_ref: BudgetRef {
                dimension: "llm.cents".into(),
                per_turn: 1,
                unit: BudgetUnit::Cents,
            },
        },
    };
    let root = issue_root(&RootSpec {
        tools: vec!["git".into()],
        namespace: "line-a".into(),
        budget_cents: 10_000,
    })?;
    let public_key = root.keypair.public();
    let order = work_order();
    let dir = std::env::temp_dir().join(format!("af-harness-smoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let opened = Opened::open(&dir)?;
    let report = opened.run(Launch {
        spec,
        run_id: "smoke".into(),
        vars: BTreeMap::new(),
        user_message: "ping".into(),
        key: EndpointKey::new(api_key),
        key_refresh: None,
        parent: &root.token,
        public_key: &public_key,
        namespace: "line-a".into(),
        spend_cap_cents: 100,
        tool_spend_cents: 1,
        budget: BudgetBinding::Cents {
            dimension: "llm.cents".into(),
            ledger: BudgetLedger::new(100),
            spend_cap_cents: 100,
        },
        order: &order,
        tools: ToolRegistry::new(),
        stop_policy: StopPolicy::default(),
        breakpoint: af_harness::Breakpoint::None,
    })?;
    println!(
        "status={} turns={} model_calls={}",
        label(&report.status),
        report.turns,
        report.model_calls
    );
    Ok(())
}

fn skip() {
    println!(
        "跳过：设置 AF_HARNESS_BASE_URL、AF_HARNESS_API_KEY、AF_HARNESS_MODEL 后才会打一轮真实请求"
    );
}

fn work_order() -> WorkOrder {
    let mut order = WorkOrder::new(
        "smoke",
        GraphVersion::new("g1"),
        Sla::new(None, Priority::Normal),
    );
    order
        .bind_line(LineBinding {
            line_id: "line-a".into(),
            line_version: "1".into(),
        })
        .expect("bind line");
    order
        .attach_budget_voucher(BudgetVoucher {
            voucher_id: "v".into(),
            amount: 1,
            signed: true,
        })
        .expect("voucher");
    order.release().expect("release");
    order.start().expect("start");
    order
}

fn label(status: &RunStatus) -> String {
    match status {
        RunStatus::Completed { signal } => format!("completed {signal}"),
        RunStatus::MaxTurns => "max_turns".into(),
        RunStatus::ToolFreeLimit => "tool_free_limit".into(),
        RunStatus::BudgetExhausted { .. } => "budget_exhausted".into(),
        RunStatus::Stopped { .. } => "stopped".into(),
        RunStatus::Unauthorized { tool, code, .. } => format!("unauthorized {tool} {code}"),
    }
}
