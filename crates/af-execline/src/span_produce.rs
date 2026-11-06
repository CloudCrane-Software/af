//! 语义层 span 生产者（RT-E138；A1 波次按名册正本取名）。
//!
//! 把订单产物投影为带**订单级标识键集**（`order_id` + `work_order_id`）的
//! span 行，写入**本地**状态面 JSON。权威载体与 [`crate::tracetree`] 同源
//! （落地契约-v0 §4.2）：
//! L1 `invoke_workflow` ← ledger + MES；L2 `invoke_agent` ← harness.db；
//! L3 `chat` ← `harness.model`；L4 `execute_tool` ← `harness.tool`。
//!
//! 语义层名册正本 = af 仓 `otel/semantic-roster.json`（全链单一源，A1）。
//! 本模块 `include_str!` 把正本编进二进制，`materialize` 只从正本取
//! 「层 → span 名」；正本缺席/不可解析 ⇒ 投影零行（fail-closed，绝不
//! panic 进 `drive`）。契约（L1~L4 各是什么名、订单域键是哪些）由
//! `tests/span_produce.rs` 的 `roster_contract_pins_*` 测试钉死——
//! 改正本 ⇒ 测试红（三面夹具见 poc/poc-a1-semantic-chain）。
//!
//! 非关键车道：失败不阻断订单（与 `af-trace` OTel lane 同口径）。生产者
//! **只写数据、不判绿**。本地状态面由 `AF_OTEL_PRODUCTION_STATE` 显式指定；
//! OTLP 入库由 `AF_OTEL_EMIT_CMD` 指向投影器发射脚本。两变量都未设则
//! `drive` 路径静默跳过，既有测试与 ledger/MES 语义不变。
//!
//! OTLP 发射带**幂等闸**（P3-A2 单根 trace）：派生 span ID 只由
//! `work_order_id` 决定，同一订单重发会在同一 trace 里再插一份全树、
//! 根 span 翻倍（断言器 S1 红）。`emit_otlp_from_env` 对「订单目录 +
//! 投影内容摘要」记标记（`AF_OTEL_EMIT_MARKER`），内容未变不重发。
//!
//! 写面：读-合并-tmp-rename 原子替换（同 `af-trace::anchor` 的
//! `atomic_write`）；同 `order_id` 重发覆盖该单旧行，其他订单保留。
//!
//! 键取法（A3 返修 v2，ADR-003）：Rust 与投影器**独立取键，容许漂移**。
//! 本模块 `work_order_id` = tracetree 的 ledger `stable_id` 集合最小值
//! （`BTreeSet::iter().next()`）；`tools/otel/af-otel-projector.py` 取
//! ledger **首行** `stable_id`（`:90`，写入 `af.work_order_id` 于 `:408`/
//! `:437`）。判据面（P2 / CH）以投影器侧为准。两侧不等时
//! [`probe_key_drift`] 记 WARN（`attrs.ledger_key_drift`），不改发射、
//! 不改 P2 红绿。本状态面 JSON **无消费者入 CH**；CH 订单域行来自投影器。
//! 缺 `stable_id`（集合空或最小值为空串）fail-closed **不产出行**，避免
//! 半键行。tracetree 对「字段缺席」当前不报 finding（只在多值漂移时报），
//! 本生产者不以核对器 finding 为门。

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tracetree::{verify_order_dir, AgentLayer, TraceTreeReport};

/// 状态面 schema 名。断言器按键消费；本生产者不自判。
pub const SCHEMA: &str = "af.otel.production.v1";
/// 显式状态面路径（绝对或相对 cwd）。未设置 ⇒ 生产路径不落盘。
pub const ENV_STATE_PATH: &str = "AF_OTEL_PRODUCTION_STATE";
/// 现役 OTLP collector HTTP 口（例 `http://<内网地址>:4318`）。未设置时
/// 缺省该值；真正开关是 `AF_OTEL_EMIT_CMD`。
pub const ENV_OTLP_ENDPOINT: &str = "AF_OTEL_ENDPOINT";
/// 投影+OTLP 发射脚本（`poc/.../emit-semantic-otlp.py`）。未设置 ⇒ 不发网络。
/// 失败忽略（非关键车道），不改 `drive` 返回值。
pub const ENV_OTLP_EMIT_CMD: &str = "AF_OTEL_EMIT_CMD";
/// 发射幂等标记件路径（JSON：订单目录 → 投影内容摘要）。未设置 ⇒ 落到
/// 系统临时目录 `af-otel-emit-marker.json`。见 [`emit_otlp_from_env`] 的
/// 幂等闸：同一订单目录、投影内容未变则不重复呼发射脚本——投影 span ID 由
/// `work_order_id` 确定性派生，重发会在**同一 trace** 里再插一份全树，
/// 根 span 翻倍（断言器 S1「每 trace 恰一个根 span」判红，P3-A2）。
pub const ENV_EMIT_MARKER: &str = "AF_OTEL_EMIT_MARKER";
const PRODUCER_ID: &str = "af-execline/span_produce";

// --------------------------------------------------------------------------- //
// 语义层名册正本（A1 全链单一源）
// --------------------------------------------------------------------------- //

/// 名册正本机读件（af 仓 `otel/semantic-roster.json`）。生产代码的唯一
/// 名册源；解析失败按缺席处理（`roster()` 返回 None → 投影零行）。
const SEMANTIC_ROSTER_JSON: &str = include_str!("../../../otel/semantic-roster.json");

static ROSTER: OnceLock<Option<SemanticRoster>> = OnceLock::new();

/// 名册正本的 typed 视图。未识别字段（kind/legs/unpaved_legs…）忽略。
#[derive(Debug, Clone, Deserialize)]
pub struct SemanticRoster {
    pub schema: String,
    /// 订单域判定锚（状态面行级 `order_id`+`stable_id` 对应的 OTLP 键名）。
    #[serde(rename = "order_domain_keys")]
    pub order_domain_keys: Vec<String>,
    /// span 名 → 层契约。键即 span 名（名册本身）。
    #[serde(rename = "semantic_layers")]
    pub layers: BTreeMap<String, RosterLayer>,
}

/// 单个语义层的名册契约（本生产者消费的子集）。
#[derive(Debug, Clone, Deserialize)]
pub struct RosterLayer {
    pub layer: String,
    pub parent: Option<String>,
    /// 该层 span attrs 必带的订单级键（L1 三键；L2~L4 经 Resource + run 链）。
    #[serde(default)]
    pub span_order_keys: Vec<String>,
}

/// 名册正本（解析失败 ⇒ None，调用方按「正本缺席」fail-closed）。
pub fn roster() -> Option<&'static SemanticRoster> {
    ROSTER
        .get_or_init(|| serde_json::from_str(SEMANTIC_ROSTER_JSON).ok())
        .as_ref()
}

impl SemanticRoster {
    /// 层号（"L1".."L4"）→ 正本 span 名。正本无此层 ⇒ None。
    pub fn layer_name(&self, layer: &str) -> Option<&str> {
        self.layers
            .iter()
            .find(|(_, v)| v.layer == layer)
            .map(|(k, _)| k.as_str())
    }
}

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

/// 一条语义 span。订单级标识键集必须来自 ledger 真源：`order_id`
///（`ledger.jsonl` 的 `order_id` 字段）+ `work_order_id`（同账本
/// `stable_id` 的 **Rust 侧**取值 = tracetree `BTreeSet` 最小值）。
///
/// 与投影器 `af.work_order_id` **同一字段真源、独立取键**：投影器取首行，
/// 本侧取集合最小；漂移时值可以不等。判据面以投影器 / CH 为准。旧状态面
/// 行缺本字段时按空串兼容读入（`serde(default)`）。缺 `stable_id` 的账本
/// 由 [`project_order`] fail-closed 不产出行，不落半键。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanRow {
    pub order_id: String,
    /// 第二订单级键 = 账本 `stable_id` 的 Rust 侧取值（集合最小）。
    /// 与投影器 `af.work_order_id` 独立取键，容许漂移。
    #[serde(default)]
    pub work_order_id: String,
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

/// 状态面里订单级键集齐全（`order_id` 与 `work_order_id` 均非空）的 span 行数。
/// 键推广不变量：真源齐备订单投影行双键计数 == 键行计数。缺 `stable_id` 的
/// 账本由 [`project_order`] fail-closed 不产出行，故新行不会以空
/// `work_order_id` 入面；本计数仍把空 `work_order_id` 排除，以兼容
/// `serde(default)` 读入的旧半键行。
pub fn count_fully_keyed_rows(state: &ProductionState) -> usize {
    state
        .spans
        .iter()
        .filter(|row| !row.order_id.is_empty() && !row.work_order_id.is_empty())
        .count()
}

/// 两侧取键漂移探针。`drifted == true` 即 WARN（计入
/// [`count_key_drift_warns`]，不改变发射 / 不改 P2 红绿）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyDriftProbe {
    /// 投影器算法：ledger.jsonl **首行** `stable_id`（字段缺席 → 空串）。
    pub projector_first_row: String,
    /// Rust / tracetree 算法：全部行现有 `stable_id` 的 `BTreeSet` 最小值。
    pub rust_set_min: String,
    pub drifted: bool,
}

/// 比较投影器首行取键与 Rust 集合最小取键。两侧独立，容许不等。
pub fn probe_key_drift(dir: &Path) -> KeyDriftProbe {
    let text = fs::read_to_string(dir.join("ledger.jsonl")).unwrap_or_default();
    let mut projector_first_row = String::new();
    let mut seen_first = false;
    let mut rust_ids: BTreeSet<String> = BTreeSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        // 投影器：lines[0].get("stable_id", "")——缺席与空串都是空。
        let as_projector = value
            .get("stable_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if !seen_first {
            projector_first_row = as_projector;
            seen_first = true;
        }
        // tracetree：字段在场（含空串）才入集合。
        if let Some(s) = value.get("stable_id").and_then(|v| v.as_str()) {
            rust_ids.insert(s.to_string());
        }
    }
    let rust_set_min = rust_ids.iter().next().cloned().unwrap_or_default();
    let drifted = projector_first_row != rust_set_min;
    KeyDriftProbe {
        projector_first_row,
        rust_set_min,
        drifted,
    }
}

/// 状态面里 `attrs.ledger_key_drift == true` 的 span 行数（WARN 计数）。
pub fn count_key_drift_warns(state: &ProductionState) -> usize {
    state
        .spans
        .iter()
        .filter(|row| row.attrs.get("ledger_key_drift").and_then(|v| v.as_bool()) == Some(true))
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

/// 把一份订单目录投影为 span 行。无 ledger 订单键、零账本行、或
/// `stable_id` 空（缺席 / 空串最小）⇒ 空（fail-closed，不落半键行）。
pub fn project_order(dir: &Path) -> Vec<SpanRow> {
    let Some(order_id) = order_id_from_ledger(dir) else {
        return Vec::new();
    };
    let report = verify_order_dir(dir);
    if report.ledger_entries == 0 {
        return Vec::new();
    }
    if report.stable_id.is_empty() {
        return Vec::new();
    }
    materialize(&order_id, &report, &probe_key_drift(dir))
}

fn materialize(order_id: &str, report: &TraceTreeReport, drift: &KeyDriftProbe) -> Vec<SpanRow> {
    // 名册正本缺席/坏 ⇒ 零行（fail-closed；契约由测试钉死，见模块头注）。
    let Some(roster) = roster() else {
        return Vec::new();
    };
    let Some(name_l1) = roster.layer_name("L1") else {
        return Vec::new();
    };
    let (Some(name_l2), Some(name_l3), Some(name_l4)) = (
        roster.layer_name("L2"),
        roster.layer_name("L3"),
        roster.layer_name("L4"),
    ) else {
        return Vec::new();
    };
    let parent_of_l2 = roster
        .layers
        .get(name_l2)
        .and_then(|l| l.parent.clone())
        .unwrap_or_else(|| name_l1.to_string());
    let mut spans = Vec::new();
    let stable = report.stable_id.as_str();
    let mut l1_attrs = json!({
        "ledger_entries": report.ledger_entries,
        "final_state": report.final_state,
        "mes_kinds": report.mes_kinds,
    });
    if drift.drifted {
        l1_attrs["ledger_key_drift"] = json!(true);
        l1_attrs["projector_first_row"] = json!(drift.projector_first_row);
        l1_attrs["rust_set_min"] = json!(drift.rust_set_min);
    }
    spans.push(SpanRow {
        order_id: order_id.to_string(),
        work_order_id: stable.to_string(),
        stable_id: stable.to_string(),
        span_name: name_l1.to_string(),
        parent_span_name: None,
        span_id: format!("{order_id}:{name_l1}"),
        source: "ledger.jsonl+mes-events.jsonl".into(),
        attrs: l1_attrs,
    });
    for agent in collect_agents(report) {
        let agent_id = format!(
            "{order_id}:{name_l2}:{}:a{}",
            agent.run_dir_name, agent.attempt
        );
        spans.push(SpanRow {
            order_id: order_id.to_string(),
            work_order_id: stable.to_string(),
            stable_id: stable.to_string(),
            span_name: name_l2.to_string(),
            parent_span_name: Some(parent_of_l2.clone()),
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
                work_order_id: stable.to_string(),
                stable_id: stable.to_string(),
                span_name: name_l3.to_string(),
                parent_span_name: Some(name_l2.to_string()),
                span_id: format!("{agent_id}:{name_l3}"),
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
                work_order_id: stable.to_string(),
                stable_id: stable.to_string(),
                span_name: name_l4.to_string(),
                parent_span_name: Some(name_l2.to_string()),
                span_id: format!("{agent_id}:{name_l4}"),
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
            "订单 {} 投影出零条 span（ledger 无订单级键、零账本行、stable_id 空或名册正本缺席——fail-closed 不落半键）",
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
        emit_otlp_from_env(order_dir);
        return;
    };
    if path.is_empty() {
        emit_otlp_from_env(order_dir);
        return;
    }
    let _ = publish_order(order_dir, Path::new(&path));
    emit_otlp_from_env(order_dir);
}

/// 经既有 OTLP collector 入库。只在 `AF_OTEL_EMIT_CMD` 指向发射脚本时动作；
/// 端点缺省 `http://<内网地址>:4318`（与 `tools/otel/otel_emit.py` 同源）。
/// 脚本/collector 失败一律吞掉，不阻断订单。
///
/// # 幂等闸（P3-A2 单根 trace）
///
/// 投影器派生的 span ID 只由 `work_order_id` 决定（`span-conventions.md`
/// §4.1 派生式），**同一订单重发 = 同一 trace 里再插一份全树**：根 span
/// （`invoke_workflow`，parent 全零）随之翻倍，断言器 S1「每 trace 恰一个
/// 根 span」判红（实测 trace `45c71671` 根 span 数=10=重发轮次）。
/// 因此本挂钩对「订单目录 + 投影内容摘要」记标记：内容未变 ⇒ 跳过呼脚本
/// （零 collector 增生）；内容变（新轮次/新账本行）⇒ 才重新发射。
/// 投影为空（fail-closed 不产行）时不设闸——交给发射脚本自裁（它按零行
/// fail-loud，rc=1），行为与加闸前一致。
pub fn emit_otlp_from_env(order_dir: &Path) {
    let Ok(script) = std::env::var(ENV_OTLP_EMIT_CMD) else {
        return;
    };
    if script.is_empty() {
        return;
    }
    let endpoint = std::env::var(ENV_OTLP_ENDPOINT)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://<内网地址>:4318".to_string());
    let invoke = || {
        std::process::Command::new("python3")
            .args(["-I", &script, "--endpoint", &endpoint, "--harvest"])
            .arg(order_dir)
            .status()
    };
    let rows = project_order(order_dir);
    if rows.is_empty() {
        // 投影为空（fail-closed 不产行）：不设闸，呼脚本让它按零行 fail-loud。
        let _ = invoke();
        return;
    }
    let digest = projection_digest(order_dir, &rows);
    let marker_path = emit_marker_path();
    let mut entries = read_emit_marker(&marker_path);
    let key = order_dir.display().to_string();
    if entries.get(&key).map(String::as_str) == Some(digest.as_str()) {
        return; // 已入库且内容未变：跳过（同 trace 不增生第二份根）
    }
    if invoke().map(|s| s.success()).unwrap_or(false) {
        entries.insert(key, digest);
        write_emit_marker(&marker_path, &entries);
    }
}

/// 投影内容摘要（sha256 十六进制）：订单目录路径 + 每行整行 JSON。
/// 行内容含 `span_id`/`parent_span_name`/attrs（计数等），任一变化都使摘要
/// 变——「内容未变不重发」的判定面对齐发射脚本实际会发的东西。
fn projection_digest(dir: &Path, rows: &[SpanRow]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(dir.display().to_string().as_bytes());
    hasher.update(b"\n");
    for row in rows {
        hasher.update(serde_json::to_string(row).unwrap_or_default().as_bytes());
        hasher.update(b"\n");
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 发射幂等标记件路径：`AF_OTEL_EMIT_MARKER` 覆盖，缺省系统临时目录。
fn emit_marker_path() -> PathBuf {
    match std::env::var(ENV_EMIT_MARKER) {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => std::env::temp_dir().join("af-otel-emit-marker.json"),
    }
}

/// 读标记件（`订单目录 → 摘要`）。缺席/坏 JSON ⇒ 空表（不拦首发射）。
fn read_emit_marker(path: &Path) -> BTreeMap<String, String> {
    let Ok(bytes) = fs::read(path) else {
        return BTreeMap::new();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// 原子写标记件（tmp + rename，同状态面写口径）。写失败静默——非关键车道，
/// 最坏后果是下次重发（回到加闸前行为），不阻断订单。
fn write_emit_marker(path: &Path, entries: &BTreeMap<String, String>) {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let Ok(body) = serde_json::to_vec(entries) else {
        return;
    };
    let tmp = match path.file_name() {
        Some(name) => {
            let mut tmp_name = name.to_os_string();
            tmp_name.push(".tmp");
            path.with_file_name(tmp_name)
        }
        None => return,
    };
    let wrote = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&tmp)
        .and_then(|mut f| {
            f.write_all(&body)?;
            f.flush()?;
            f.sync_data()
        });
    if wrote.is_err() {
        let _ = fs::remove_file(&tmp);
        return;
    }
    let _ = fs::rename(&tmp, path);
}

/// 投影行里「根 span」计数：`parent_span_name` 缺席或空串（对应 OTLP
/// `parentSpanId` 全零）。一订单 = 一 trace 恰一根 span 的生产者面判据
/// （断言器 S1 的本地面），由 `tests/span_produce.rs` 钉桩。
pub fn root_span_count(rows: &[SpanRow]) -> usize {
    rows.iter()
        .filter(|r| r.parent_span_name.as_deref().unwrap_or("").is_empty())
        .count()
}

/// 非根行的 `parent_span_name` 在本投影内找不到同名 span 的行（断链，
/// 断言器 S2 的生产者面）。返回断链行的 `span_id`。
pub fn unresolved_parent_names(rows: &[SpanRow]) -> Vec<String> {
    let names: BTreeSet<&str> = rows.iter().map(|r| r.span_name.as_str()).collect();
    rows.iter()
        .filter(|r| {
            let parent = r.parent_span_name.as_deref().unwrap_or("");
            !parent.is_empty() && !names.contains(parent)
        })
        .map(|r| r.span_id.clone())
        .collect()
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
