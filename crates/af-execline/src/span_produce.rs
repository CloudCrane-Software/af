//! 语义层 span 生产者（RT-E138）。
//!
//! 把订单产物投影为带**订单级标识键**的 span 行，写入状态面 JSON。
//! 权威载体与 [`crate::tracetree`] 同源（落地契约-v0 §4.2）：
//! L1 `invoke_workflow` ← ledger + MES；L2 `invoke_agent` ← harness.db；
//! L3 `chat` ← `harness.model`；L4 `execute_tool` ← `harness.tool`。
//!
//! 非关键车道：失败不阻断订单（与 `af-trace` OTel lane 同口径）。生产者
//! **只写数据、不判绿**。状态面路径由 `AF_OTEL_PRODUCTION_STATE` 显式指定；
//! 未设置则 `drive` 路径静默跳过，既有测试与 ledger/MES 语义不变。
//!
//! 写面：读-合并-tmp-rename 原子替换（同 `af-trace::anchor` 的
//! `atomic_write`）；同 `order_id` 重发覆盖该单旧行，其他订单保留。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tracetree::{verify_order_dir, AgentLayer, TraceTreeReport};

/// 状态面 schema 名。断言器按键消费；本生产者不自判。
pub const SCHEMA: &str = "af.otel.production.v1";
/// 显式状态面路径（绝对或相对 cwd）。未设置 ⇒ 生产路径不落盘。
pub const ENV_STATE_PATH: &str = "AF_OTEL_PRODUCTION_STATE";
const PRODUCER_ID: &str = "af-execline/span_produce";

/// 状态面整文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionState {
    pub schema: String,
    pub updated_at_unix: u64,
    pub producer: String,
    pub spans: Vec<SpanRow>,
}

impl ProductionState {
    fn empty() -> Self {
        Self {
            schema: SCHEMA.to_string(),
            updated_at_unix: now_unix(),
            producer: PRODUCER_ID.to_string(),
            spans: Vec::new(),
        }
    }
}

/// 一条语义 span。`order_id` 是订单级标识键，必须来自 ledger 真源。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanRow {
    pub order_id: String,
    pub stable_id: String,
    pub span_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_span_name: Option<String>,
    pub span_id: String,
    pub source: String,
    pub attrs: Value,
}

/// 状态面里 `order_id` 非空的 span 行数（本笔债硬指标）。
pub fn count_order_keyed_rows(state: &ProductionState) -> usize {
    state
        .spans
        .iter()
        .filter(|row| !row.order_id.is_empty())
        .count()
}

/// 从 ledger.jsonl 读订单级标识键。无真实行 ⇒ None（不造常量行）。
/// 只取 `order_id` 字段，不走 `OrderLedger::load` 全记录反序列化——夹具行与
/// 生产行只要带该键即算真源。
pub fn order_id_from_ledger(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join("ledger.jsonl")).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(id) = value.get("order_id").and_then(|v| v.as_str()) {
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
    }
    None
}

/// 把一份订单目录投影为 span 行。无 ledger 订单键或零账本行 ⇒ 空。
pub fn project_order(dir: &Path) -> Vec<SpanRow> {
    let Some(order_id) = order_id_from_ledger(dir) else {
        return Vec::new();
    };
    let report = verify_order_dir(dir);
    if report.ledger_entries == 0 {
        return Vec::new();
    }
    materialize(&order_id, &report)
}

fn materialize(order_id: &str, report: &TraceTreeReport) -> Vec<SpanRow> {
    let mut spans = Vec::new();
    let stable = report.stable_id.as_str();
    spans.push(SpanRow {
        order_id: order_id.to_string(),
        stable_id: stable.to_string(),
        span_name: "invoke_workflow".into(),
        parent_span_name: None,
        span_id: format!("{order_id}:invoke_workflow"),
        source: "ledger.jsonl+mes-events.jsonl".into(),
        attrs: json!({
            "ledger_entries": report.ledger_entries,
            "final_state": report.final_state,
            "mes_kinds": report.mes_kinds,
        }),
    });
    for agent in collect_agents(report) {
        let agent_id = format!(
            "{order_id}:invoke_agent:{}:a{}",
            agent.run_dir_name, agent.attempt
        );
        spans.push(SpanRow {
            order_id: order_id.to_string(),
            stable_id: stable.to_string(),
            span_name: "invoke_agent".into(),
            parent_span_name: Some("invoke_workflow".into()),
            span_id: agent_id.clone(),
            source: "harness.db+critical.jsonl".into(),
            attrs: json!({
                "run_dir": agent.run_dir_name,
                "run_id": agent.run_id,
                "node_id": agent.node_id,
                "attempt": agent.attempt,
                "terminal_kind": agent.terminal_kind,
                "terminal_signal": agent.terminal_signal,
            }),
        });
        if agent.chat_events > 0 {
            spans.push(SpanRow {
                order_id: order_id.to_string(),
                stable_id: stable.to_string(),
                span_name: "chat".into(),
                parent_span_name: Some("invoke_agent".into()),
                span_id: format!("{agent_id}:chat"),
                source: "normal.jsonl harness.model".into(),
                attrs: json!({
                    "run_id": agent.run_id,
                    "count": agent.chat_events,
                    "tokens_input": agent.tokens.0,
                    "tokens_output": agent.tokens.1,
                    "tokens_cache_read": agent.tokens.2,
                }),
            });
        }
        if agent.tool_events > 0 {
            spans.push(SpanRow {
                order_id: order_id.to_string(),
                stable_id: stable.to_string(),
                span_name: "execute_tool".into(),
                parent_span_name: Some("invoke_agent".into()),
                span_id: format!("{agent_id}:execute_tool"),
                source: "normal.jsonl harness.tool+effects.db".into(),
                attrs: json!({
                    "run_id": agent.run_id,
                    "count": agent.tool_events,
                    "impure": agent.impure_tool_events,
                    "effect_intents": agent.effect_intents,
                }),
            });
        }
    }
    spans
}

fn collect_agents(report: &TraceTreeReport) -> Vec<&AgentLayer> {
    if report.ring.is_empty() && report.graph.is_empty() {
        report.agent.iter().collect()
    } else {
        report.ring.iter().chain(report.graph.iter()).collect()
    }
}

/// 把 `dir` 的 span 合并进状态面。返回本次写入后的订单级键行总数。
pub fn publish_order(dir: &Path, state_path: &Path) -> Result<usize, String> {
    let incoming = project_order(dir);
    if incoming.is_empty() {
        return Err(format!(
            "订单 {} 投影出零条 span（ledger 无订单级键或零账本行）",
            dir.display()
        ));
    }
    let order_id = incoming[0].order_id.clone();
    let mut state = load_or_empty(state_path)?;
    state.spans.retain(|row| row.order_id != order_id);
    state.spans.extend(incoming);
    state.updated_at_unix = now_unix();
    state.schema = SCHEMA.to_string();
    state.producer = PRODUCER_ID.to_string();
    atomic_write_json(state_path, &state)?;
    Ok(count_order_keyed_rows(&state))
}

/// `drive` 挂钩：仅当环境变量指向状态面时落盘，错误吞掉。
pub fn publish_from_env(order_dir: &Path) {
    let Ok(path) = std::env::var(ENV_STATE_PATH) else {
        return;
    };
    if path.is_empty() {
        return;
    }
    let _ = publish_order(order_dir, Path::new(&path));
}

/// 读状态面；缺席或无法解析 ⇒ 空壳（不造 span 行）。
pub fn load_state(path: &Path) -> Result<ProductionState, String> {
    let bytes = fs::read(path).map_err(|err| format!("读状态面失败: {err}"))?;
    serde_json::from_slice(&bytes).map_err(|err| format!("状态面 JSON 坏: {err}"))
}

fn load_or_empty(path: &Path) -> Result<ProductionState, String> {
    if !path.is_file() {
        return Ok(ProductionState::empty());
    }
    match load_state(path) {
        Ok(state) => Ok(state),
        Err(_) => Ok(ProductionState::empty()),
    }
}

fn atomic_write_json(path: &Path, state: &ProductionState) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| format!("建状态面目录失败: {err}"))?;
        }
    }
    let body = serde_json::to_vec_pretty(state).map_err(|err| format!("序列化失败: {err}"))?;
    let tmp = match path.file_name() {
        Some(name) => {
            let mut tmp_name = name.to_os_string();
            tmp_name.push(".tmp");
            path.with_file_name(tmp_name)
        }
        None => PathBuf::from("otel-production.json.tmp"),
    };
    {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .map_err(|err| format!("打开状态面临时文件失败: {err}"))?;
        file.write_all(&body)
            .map_err(|err| format!("写状态面临时文件失败: {err}"))?;
        file.flush()
            .map_err(|err| format!("flush 状态面临时文件失败: {err}"))?;
        file.sync_data()
            .map_err(|err| format!("sync 状态面临时文件失败: {err}"))?;
    }
    fs::rename(&tmp, path).map_err(|err| format!("rename 状态面失败: {err}"))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
