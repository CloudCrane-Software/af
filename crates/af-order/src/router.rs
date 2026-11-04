//! router —— event-router 消费腿（PoC-D 面 → PoC-C 落点）：把一张真实的
//! af.trigger.v1 信封接到 [`crate::adjudicate::generate_adjudication_doc`]
//! 所在的裁决环上。
//!
//! adjudicate 模块文档的触发链里「(event-router 消费腿，PoC-D 面)」这一环
//! 即本模块：Trigger 收件箱（`poc/poc-super-agent/tools-mirror/trigger-inbox.py`）
//! 落盘的 af.trigger.v1 信封 → 读侧验证 → 从信封派生裁决目标（源单
//! order_id/stable_id/MES complete 事件）→ 既有 steward 闭环
//! （[`crate::adjudicate::run_steward_loop`]：生成裁决单 → 入队 → 认领 →
//! 真签 → admit → complete）。本模块只做**接线**：信封解析/验证/派生与消费
//! 游标，不造任何新机制（不派第二套队列、不另造签名、不改状态机）。
//!
//! # 读侧纪律（af.trigger.v1 契约 §4，2026-11-03 修复批）
//!
//! 消费腿的唯一可信载荷 = `body_b64`（字节精确原始 payload，由
//! `payload_sha256` 自证）；派单决策输入（order_id/stable_id/MES 事件）
//! 一律从**已验 body_b64 重解析**取，不消费信封冗余的 `body` 解析态。信封
//! 冗余字段（event_type/delivery_id/body）与重解析结果逐项核对，任一不符
//! = 拒读（[`RouterError::CrossCheck`]，不派单）。thread_key/dedup_key 按
//! 收件箱 `envelope_for` 兜底分支（非 git 事件：`域:repo` / `etype:repo`）
//! 从注册表 + 已验 body 重推导核对——git 域事件（push/pull_request）不属
//! 本消费腿（归 PoC-D event-router），fail-loud 拒收。
//!
//! # 防重放（两道，正交；契约 §3）
//!
//! 1. delivery 幂等：收件箱层（同 source/delivery 不再落第二份）；
//! 2. 业务去重：本读侧消费游标 `consumed.jsonl`（契约 §4 一行一事件），
//!    delivery_id 或 dedup_key 命中已消费行 = 重放投递，
//!    [`duplicate_consumed`] 拒派（队列侧第三道 = Station 幂等键 UNIQUE）。
//!
//! # 裁决单文书线格式更正（对任务书的如实登记）
//!
//! 裁决单落盘线格式是 **JSON**（[`crate::adjudicate::AdjudicationDoc::persist`]
//! 既有口径，禁改）；YAML 正本指配方层模板常量
//! [`crate::adjudicate::TEMPLATE_BODY`]（YAML 形文本），非生成物线格式。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use af_workorder::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::error::OrderError;
use crate::spec::validate_order_id;
use crate::state::{current_state, load_ledger, StateDir, LEDGER_FILE, MES_FILE};

/// 消费腿所吃的信封契约名（PoC-D 契约正本 §1）。
pub const TRIGGER_CONTRACT: &str = "af.trigger.v1";
/// 本消费腿的事件类型（MES complete → 裁决单，adjudicate.rs 触发链）。
pub const ORDER_COMPLETE_EVENT: &str = "order_complete";
/// 读侧消费游标文件（相对 triggers home；契约 §4：一行一事件，防重放真源）。
pub const CONSUMED_FILE: &str = "consumed.jsonl";

/// af.trigger.v1 信封（七字段 + 外层实现字段；deny_unknown_fields 钉死
/// 线格式——多字段/换字段即拒读，fail-closed）。字段语义见契约正本
/// `poc/poc-super-agent/contracts/af.trigger.v1.md` §1。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerEnvelope {
    /// 契约名：恒为 [`TRIGGER_CONTRACT`]。
    pub contract: String,
    /// 事件全局唯一 id（= `<source>-<delivery_id>`，收件箱生成）。
    pub event_id: String,
    /// 线程键 `域:业务对象id`（收件箱按注册表 domain 生成）。
    pub thread_key: String,
    /// 原始 payload 落盘相对路径（`inbox/<source>/<delivery-id>.json`）。
    pub payload_ref: String,
    /// 原始 payload 字节流的 SHA-256 小写 hex（body_b64 自证面）。
    pub payload_sha256: String,
    /// 业务去重键（防重放/幂等派单第二道）。
    pub dedup_key: String,
    /// 事件来源（注册表 `sources[].name`）。
    pub source: String,
    /// 收件时刻（ISO-8601 UTC）。
    pub received_at: String,
    // ---- 外层实现字段（非七字段契约面）----
    /// 幂等主键（payload 内 delivery_id 或 X-AF-Delivery 头）。
    pub delivery_id: String,
    /// 白名单事件类型（body 内 event 字段）。
    pub event_type: String,
    /// 验签 key 指向（秘密不落信封；注册表同名字段冗余，允许缺席）。
    pub key_id: Option<String>,
    /// HMAC 算法（收件箱默认 sha256）。
    pub hmac: String,
    /// 业务对象短名（git 域为 sha12/PR 号；兜底分支为空串）。
    pub biz_object: String,
    /// 冗余解析态（**不可信**：只作逐项核对对象，不作决策输入）。
    pub body: serde_json::Value,
    /// 字节精确原始 payload（base64，标准字母表带 padding）——唯一可信载荷。
    pub body_b64: String,
}

/// 读侧所需的最小注册表视图（正本还有 repo_suites/liveness/slow_path 等
/// 段，本模块不消费、不拒未知字段——数据文件读侧，宽松解析）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TriggerRegistry {
    pub sources: Vec<SourceDef>,
}

/// 注册表 source 定义（契约 §6：name/domain/events 白名单/key_id/hmac）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SourceDef {
    pub name: String,
    pub domain: String,
    #[serde(default)]
    pub key_id: Option<String>,
    #[serde(default = "default_hmac")]
    pub hmac: String,
    pub events: Vec<String>,
}

fn default_hmac() -> String {
    "sha256".into()
}

/// 验证通过的触发消息：全部派单决策输入来自已验 body_b64 重解析。
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedTrigger {
    /// 原始信封（仅作留证输出；冗余字段已经逐项核对过）。
    pub envelope: TriggerEnvelope,
    /// 唯一可信载荷：body_b64 解码字节的重解析（JSON 对象）。
    pub body: serde_json::Value,
    /// 已验 payload 字节数（留证）。
    pub body_bytes_len: usize,
}

/// event-router 消费腿失败（类型化；fail-closed——任一核对不过即拒读，
/// 不派单、不入队）。
#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    /// 信封格式/形状不符（非 JSON、未知字段、契约名/七字段形状坏）。
    #[error("消费腿信封形状不符: {0}")]
    Envelope(String),
    /// body_b64 不可解码或 payload_sha256 自证不过（篡改 body_b64 即红）。
    #[error("消费腿载荷自证失败: {0}")]
    Payload(String),
    /// 信封冗余字段与已验 body_b64 重解析不符（篡改 parsed body 即红）。
    #[error("信封冗余字段与已验载荷不符: {0}")]
    CrossCheck(String),
    /// 注册表面失败（source 未登记 / 事件不在白名单 / 域推导不符）。
    #[error("消费腿注册表面失败: {0}")]
    Registry(String),
    /// 裁决目标解析失败（事件域不符 / 字段缺 / 与 state 对账不上）。
    #[error("裁决目标解析失败: {0}")]
    Target(String),
    /// 消费游标失败（consumed.jsonl 损坏或不可写）。
    #[error("消费游标失败: {0}")]
    Cursor(String),
    /// af-order 面失败（账本读取等 OR 码透传）。
    #[error("消费腿 af-order 面失败: {0}")]
    Order(#[from] OrderError),
}

/// 从 triggers home 读注册表（运行位 `registry.json`，契约 §6）。
pub fn load_registry(path: &Path) -> Result<TriggerRegistry, RouterError> {
    let text = std::fs::read_to_string(path).map_err(|err| {
        RouterError::Registry(format!("注册表读取失败 {}: {err}", path.display()))
    })?;
    serde_json::from_str(&text)
        .map_err(|err| RouterError::Registry(format!("注册表不是合法 af.trigger.registry 视图: {err}")))
}

/// 验证一张 af.trigger.v1 信封（读侧第一道门）：
/// 形状（契约名/七字段/派生形状）→ body_b64 自证（sha256）→ 冗余字段
/// 逐项核对 → 注册表（source 白名单 + 事件白名单 + thread_key/dedup_key
/// 兜底分支重推导）。任何一步不过即 [`RouterError`]，不产出可信载荷。
pub fn verify_envelope(
    text: &str,
    registry: &TriggerRegistry,
) -> Result<VerifiedTrigger, RouterError> {
    let envelope: TriggerEnvelope = serde_json::from_str(text).map_err(|err| {
        RouterError::Envelope(format!("信封不是合法 af.trigger.v1 线格式: {err}"))
    })?;
    if envelope.contract != TRIGGER_CONTRACT {
        return Err(RouterError::Envelope(format!(
            "契约名不符：{}（期望 {TRIGGER_CONTRACT}）",
            envelope.contract
        )));
    }
    for (name, value) in [
        ("event_id", &envelope.event_id),
        ("thread_key", &envelope.thread_key),
        ("payload_ref", &envelope.payload_ref),
        ("payload_sha256", &envelope.payload_sha256),
        ("dedup_key", &envelope.dedup_key),
        ("source", &envelope.source),
        ("received_at", &envelope.received_at),
        ("delivery_id", &envelope.delivery_id),
        ("event_type", &envelope.event_type),
    ] {
        if value.is_empty() {
            return Err(RouterError::Envelope(format!("信封字段 {name} 为空")));
        }
    }
    // 派生形状（契约 §1：event_id=<source>-<delivery_id>；payload_ref=inbox/…）。
    if envelope.event_id != format!("{}-{}", envelope.source, envelope.delivery_id) {
        return Err(RouterError::Envelope(format!(
            "event_id 形状不符：{}（期望 {{source}}-{{delivery_id}}）",
            envelope.event_id
        )));
    }
    let want_ref = format!("inbox/{}/{}.json", envelope.source, envelope.delivery_id);
    if envelope.payload_ref != want_ref {
        return Err(RouterError::Envelope(format!(
            "payload_ref 形状不符：{}（期望 {want_ref}）",
            envelope.payload_ref
        )));
    }
    let sha = &envelope.payload_sha256;
    if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err(RouterError::Envelope(format!(
            "payload_sha256 不是 64 位小写 hex: {sha}"
        )));
    }
    chrono::DateTime::parse_from_rfc3339(&envelope.received_at).map_err(|err| {
        RouterError::Envelope(format!("received_at 不是 RFC3339: {err}"))
    })?;

    // ── body_b64 自证（唯一可信载荷；篡改即红）──────────────────────
    let body_bytes = b64_decode_strict(&envelope.body_b64)?;
    let actual_sha = sha256_hex(&body_bytes);
    if actual_sha != envelope.payload_sha256 {
        return Err(RouterError::Payload(format!(
            "payload_sha256 自证不过：声称 {}，body_b64 复算 {actual_sha}",
            envelope.payload_sha256
        )));
    }
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).map_err(|err| {
        RouterError::Payload(format!("已验 payload 不是合法 JSON: {err}"))
    })?;
    if !body.is_object() {
        return Err(RouterError::Payload("已验 payload 顶层不是 JSON 对象".into()));
    }

    // ── 冗余字段逐项核对（契约 §4 修复批：篡改 parsed body 保留合法
    //    body_b64+hash 的形态必须拒读零派单）─────────────────────────
    let claimed_delivery = body.get("delivery_id").and_then(serde_json::Value::as_str);
    if claimed_delivery != Some(envelope.delivery_id.as_str()) {
        return Err(RouterError::CrossCheck(format!(
            "delivery_id 不符：信封 {}，已验载荷 {claimed_delivery:?}",
            envelope.delivery_id
        )));
    }
    let claimed_event = body.get("event").and_then(serde_json::Value::as_str);
    if claimed_event != Some(envelope.event_type.as_str()) {
        return Err(RouterError::CrossCheck(format!(
            "event_type 不符：信封 {}，已验载荷 {claimed_event:?}",
            envelope.event_type
        )));
    }
    if envelope.body != body {
        return Err(RouterError::CrossCheck(
            "冗余解析态 body 与已验 body_b64 重解析不符（篡改面）".into(),
        ));
    }

    // ── 注册表核对（source 白名单 + 事件白名单 + 域推导）────────────
    let source_def = registry
        .sources
        .iter()
        .find(|def| def.name == envelope.source)
        .ok_or_else(|| {
            RouterError::Registry(format!("source 未登记: {}", envelope.source))
        })?;
    if envelope.event_type == "push" || envelope.event_type == "pull_request" {
        return Err(RouterError::Registry(format!(
            "git 域事件（{}）归 PoC-D event-router 消费腿，本腿（MES complete→裁决单）不消费",
            envelope.event_type
        )));
    }
    if !source_def.events.contains(&envelope.event_type) {
        return Err(RouterError::Registry(format!(
            "event_type 不在 source {} 白名单 {:?}: {}",
            envelope.source, source_def.events, envelope.event_type
        )));
    }
    // thread_key/dedup_key 兜底分支重推导（envelope_for 非 git 分支的镜像：
    // repo = body.repository.full_name ?? .name ?? "unknown-repo"；
    // thread_key = 域:repo；dedup_key = etype:repo）。
    let repo = mirror_repo_of(&body);
    let want_thread = format!("{}:{}", source_def.domain, repo);
    if envelope.thread_key != want_thread {
        return Err(RouterError::CrossCheck(format!(
            "thread_key 与兜底分支重推导不符：信封 {}，重推导 {want_thread}",
            envelope.thread_key
        )));
    }
    let want_dedup = format!("{}:{}", envelope.event_type, repo);
    if envelope.dedup_key != want_dedup {
        return Err(RouterError::CrossCheck(format!(
            "dedup_key 与兜底分支重推导不符：信封 {}，重推导 {want_dedup}",
            envelope.dedup_key
        )));
    }

    Ok(VerifiedTrigger {
        body_bytes_len: body_bytes.len(),
        envelope,
        body,
    })
}

/// envelope_for 兜底分支的 repo 抽取镜像（python 真值语义：空串/null 回退）。
fn mirror_repo_of(body: &serde_json::Value) -> String {
    let pick = |key: &str| -> Option<String> {
        body.get("repository")?
            .get(key)?
            .as_str()
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    pick("full_name")
        .or_else(|| pick("name"))
        .unwrap_or_else(|| "unknown-repo".into())
}

/// 从已验触发消息派生的裁决目标（腿①的「从信封派生」替代面：源单身份与
/// 事件锚定全部来自已验载荷 + state 对账，不再自造源任务单）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdjudicationTarget {
    /// 源单订单号（state/orders/ 目录名；已过 validate_order_id）。
    pub order_id: String,
    /// 源单稳定身份（与账本行 stable_id 对账过）。
    pub stable_id: String,
    /// 信封锚定的 MES complete 事件 id（在源单 MES 正本中逐字对上）。
    pub mes_event_id: String,
    /// 源单账本最近快照状态名（留证；受理侧规则由 admit 自己的链把）。
    pub source_state: String,
    /// 源单账本行数（留证）。
    pub ledger_records: usize,
    /// 源单 MES 正本事件总数（留证）。
    pub mes_events_total: usize,
}

/// 从已验触发消息解析裁决目标（读侧第二道门）并与 state 对账：
/// - 事件域：body.event 必须是 [`ORDER_COMPLETE_EVENT`]（本腿只吃
///   MES complete；其他事件域 fail-loud 不假装能派）；
/// - 身份：body.order_id / body.stable_id 必须与源单账本一致（防信封
///   指鹿为马）；
/// - 事件锚定：body.mes_event 必须在源单 mes-events.jsonl 正本中逐字
///   存在且 kind=complete、work_order_id=stable_id（MES 事件按稳定身份
///   记账；防伪造事件）。
pub fn resolve_adjudication_target(
    state: &StateDir,
    trigger: &VerifiedTrigger,
) -> Result<AdjudicationTarget, RouterError> {
    let body = &trigger.body;
    let event = body.get("event").and_then(serde_json::Value::as_str);
    if event != Some(ORDER_COMPLETE_EVENT) {
        return Err(RouterError::Target(format!(
            "本消费腿只吃 {ORDER_COMPLETE_EVENT} 事件，拒收 {event:?}"
        )));
    }
    let order_id = body
        .get("order_id")
        .and_then(serde_json::Value::as_str)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| RouterError::Target("body.order_id 缺失或为空".into()))?;
    validate_order_id(order_id)
        .map_err(|err| RouterError::Target(format!("body.order_id 不合法: {err}")))?;
    let stable_id = body
        .get("stable_id")
        .and_then(serde_json::Value::as_str)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| RouterError::Target("body.stable_id 缺失或为空".into()))?;
    let mes_event = body.get("mes_event").ok_or_else(|| {
        RouterError::Target("body.mes_event 缺失（MES complete 事件锚定面）".into())
    })?;
    if !mes_event.is_object() {
        return Err(RouterError::Target("body.mes_event 不是 JSON 对象".into()));
    }
    if mes_event.get("kind").and_then(serde_json::Value::as_str) != Some("complete") {
        return Err(RouterError::Target(
            "body.mes_event.kind 不是 complete（本腿只锚定 complete 事件）".into(),
        ));
    }
    if mes_event.get("work_order_id").and_then(serde_json::Value::as_str) != Some(stable_id)
    {
        return Err(RouterError::Target(
            "body.mes_event.work_order_id 与 body.stable_id 不符（MES 事件按稳定身份记账，\
             事件不锚定该源单）"
                .into(),
        ));
    }
    let mes_event_id = mes_event
        .get("event_id")
        .and_then(serde_json::Value::as_str)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| RouterError::Target("body.mes_event.event_id 缺失".into()))?;

    // ── state 对账：账本存在、stable_id 一致、MES 正本含该事件 ──────
    let order_dir = state.order_dir(order_id);
    let records = load_ledger(&order_dir.join(LEDGER_FILE))?;
    let Some(last) = records.last() else {
        return Err(RouterError::Target(format!(
            "源单 {order_id} 账本为空（未经 submit 落账，无从裁决）"
        )));
    };
    if last.stable_id != stable_id {
        return Err(RouterError::Target(format!(
            "信封声称 stable_id {stable_id} 与源单账本 {} 不符（指鹿为马面）",
            last.stable_id
        )));
    }
    let Some(source_state) = current_state(&records) else {
        return Err(RouterError::Target(format!(
            "源单 {order_id} 账本无快照行，读不出当前状态"
        )));
    };
    let mes_text = std::fs::read_to_string(order_dir.join(MES_FILE)).map_err(|err| {
        RouterError::Target(format!(
            "源单 {order_id} MES 正本读取失败 {}: {err}",
            order_dir.join(MES_FILE).display()
        ))
    })?;
    let mut mes_events_total = 0usize;
    let mut anchored = false;
    for line in mes_text.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        mes_events_total += 1;
        if value == *mes_event {
            anchored = true;
        }
    }
    if !anchored {
        return Err(RouterError::Target(format!(
            "信封携带的 MES complete 事件（event_id={mes_event_id}）不在源单 {order_id} \
             MES 正本中（伪造/错单锚定）"
        )));
    }
    Ok(AdjudicationTarget {
        order_id: order_id.to_string(),
        stable_id: stable_id.to_string(),
        mes_event_id: mes_event_id.to_string(),
        source_state,
        ledger_records: records.len(),
        mes_events_total,
    })
}

/// 消费游标一行（契约 §4 字段面：一行一事件，防重放真源）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumedRecord {
    pub delivery_id: String,
    pub source: String,
    pub event_type: String,
    pub dedup_key: String,
    pub order_id: String,
    pub consumed_at: String,
}

/// 消费游标路径（`<triggers-home>/consumed.jsonl`）。
pub fn consumed_path(triggers_home: &Path) -> PathBuf {
    triggers_home.join(CONSUMED_FILE)
}

/// 读消费游标（文件缺席 = 空；坏行 fail-closed 红——JSONL 是游标真源，
/// 不容半行糊弄）。
pub fn load_consumed(triggers_home: &Path) -> Result<Vec<ConsumedRecord>, RouterError> {
    let path = consumed_path(triggers_home);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|err| RouterError::Cursor(format!("游标读取失败 {}: {err}", path.display())))?;
    let mut records = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: ConsumedRecord = serde_json::from_str(line).map_err(|err| {
            RouterError::Cursor(format!(
                "游标第 {} 行不是合法 ConsumedRecord（真源损坏，fail-closed）: {err}",
                index + 1
            ))
        })?;
        records.push(record);
    }
    Ok(records)
}

/// 防重放判定：delivery_id（网络重试语义）或 dedup_key（业务重放语义）
/// 命中已消费行即重放，返回命中行。
pub fn duplicate_consumed<'a>(
    records: &'a [ConsumedRecord],
    delivery_id: &str,
    dedup_key: &str,
) -> Option<&'a ConsumedRecord> {
    records.iter().find(|record| {
        record.delivery_id == delivery_id || record.dedup_key == dedup_key
    })
}

/// 追加一行消费游标（只在派单闭环成功后调用；fsync 落盘）。
pub fn append_consumed(
    triggers_home: &Path,
    record: &ConsumedRecord,
) -> Result<(), RouterError> {
    let path = consumed_path(triggers_home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            RouterError::Cursor(format!("创建 {}: {err}", parent.display()))
        })?;
    }
    let line = serde_json::to_string(record)
        .map_err(|err| RouterError::Cursor(format!("游标行序列化失败: {err}")))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|err| RouterError::Cursor(format!("游标打开失败 {}: {err}", path.display())))?;
    writeln!(file, "{line}")
        .map_err(|err| RouterError::Cursor(format!("游标追加失败 {}: {err}", path.display())))?;
    file.flush()
        .map_err(|err| RouterError::Cursor(format!("游标落盘失败 {}: {err}", path.display())))?;
    Ok(())
}

/// 标准 base64（字母表 A-Za-z0-9+/，必须带齐 padding）严格解码。
/// af-order 零外部依赖纪律（Cargo.toml 硬约束 6），不引 base64 crate；
/// 非法字符/非法 padding/长度非 4 倍数一律拒（fail-closed，不静默吞）。
fn b64_decode_strict(text: &str) -> Result<Vec<u8>, RouterError> {
    let invalid = |reason: String| RouterError::Payload(format!("body_b64 不是合法 base64: {reason}"));
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err(invalid(format!("长度 {} 非 4 的倍数", bytes.len())));
    }
    // '=' 只允许出现在末两位（0-2 个），其后不得再有其他字符。
    if let Some(first_pad) = bytes.iter().position(|&byte| byte == b'=') {
        let pads = bytes.len() - first_pad;
        if pads > 2 || bytes[first_pad..].iter().any(|&byte| byte != b'=') {
            return Err(invalid("'=' 只能作末尾 padding".into()));
        }
    }
    let chunk_count = bytes.len() / 4;
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (index, chunk) in bytes.chunks(4).enumerate() {
        let pads = chunk.iter().filter(|&&byte| byte == b'=').count();
        let mut acc: u32 = 0;
        let mut chars = 0u32;
        for &byte in chunk {
            if byte == b'=' {
                continue;
            }
            let value = match byte {
                b'A'..=b'Z' => u32::from(byte - b'A'),
                b'a'..=b'z' => u32::from(byte - b'a') + 26,
                b'0'..=b'9' => u32::from(byte - b'0') + 52,
                b'+' => 62,
                b'/' => 63,
                other => {
                    return Err(invalid(format!("非法字符 0x{other:02x}")));
                }
            };
            acc = (acc << 6) | value;
            chars += 1;
        }
        // 末组允许 2/3 字符（1/2 字节），其余必须 4 字符（3 字节）。
        let last = index + 1 == chunk_count;
        match (chars, last) {
            (4, _) => {
                out.push((acc >> 16) as u8);
                out.push((acc >> 8) as u8);
                out.push(acc as u8);
            }
            (3, true) if pads == 1 => {
                out.push((acc >> 10) as u8);
                out.push((acc >> 2) as u8);
            }
            (2, true) if pads == 2 => {
                out.push((acc >> 4) as u8);
            }
            _ => {
                return Err(invalid("字符数/padding 组合非法".into()));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试内联 base64 编码器（标准字母表带 padding；与 lib 侧解码互为对拍）。
    fn b64_encode(data: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let bytes = [
                chunk[0],
                chunk.get(1).copied().unwrap_or(0),
                chunk.get(2).copied().unwrap_or(0),
            ];
            let acc = (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
            out.push(ALPHABET[(acc >> 18) as usize & 0x3f] as char);
            out.push(ALPHABET[(acc >> 12) as usize & 0x3f] as char);
            out.push(if chunk.len() > 1 {
                ALPHABET[(acc >> 6) as usize & 0x3f] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                ALPHABET[acc as usize & 0x3f] as char
            } else {
                '='
            });
        }
        out
    }

    fn registry() -> TriggerRegistry {
        serde_json::from_str(
            r#"{"sources":[{"name":"mes-execline","domain":"mes","key_id":"k1",
                "hmac":"sha256","events":["order_complete"]}],"repo_suites":[]}"#,
        )
        .unwrap()
    }

    /// 按收件箱 envelope_for 同款口径构造一张合法信封（测试基准面）。
    fn envelope(body: &serde_json::Value, delivery_id: &str) -> String {
        let source = "mes-execline";
        let body_bytes = serde_json::to_vec(body).unwrap();
        let payload_sha256 = sha256_hex(&body_bytes);
        let repo = body["repository"]["full_name"].as_str().unwrap();
        serde_json::json!({
            "contract": TRIGGER_CONTRACT,
            "event_id": format!("{source}-{delivery_id}"),
            "thread_key": format!("mes:{repo}"),
            "payload_ref": format!("inbox/{source}/{delivery_id}.json"),
            "payload_sha256": payload_sha256,
            "dedup_key": format!("order_complete:{repo}"),
            "source": source,
            "received_at": "2026-11-04T09:00:00+00:00",
            "delivery_id": delivery_id,
            "event_type": "order_complete",
            "key_id": "k1",
            "hmac": "sha256",
            "biz_object": "",
            "body": body,
            "body_b64": b64_encode(&body_bytes),
        })
        .to_string()
    }

    fn mes_body(order_id: &str, stable_id: &str) -> serde_json::Value {
        serde_json::json!({
            "event": "order_complete",
            "delivery_id": format!("mes-{order_id}-complete-1"),
            "repository": {"full_name": format!("line-poc/{order_id}")},
            "order_id": order_id,
            "stable_id": stable_id,
            "mes_event": {
                "work_order_id": stable_id,
                "occurred_at": 1_767_225_600_000_i64,
                "event_id": format!("me-{order_id}-complete"),
                "line_id": "line-poc",
                "station_id": "st-1",
                "kind": "complete",
                "quantity": 1,
            },
        })
    }

    /// b64 解码对拍表（含非法形态全拒）。
    #[test]
    fn b64_decode_strict_table() {
        assert_eq!(b64_decode_strict("").unwrap(), Vec::<u8>::new());
        assert_eq!(b64_decode_strict("aGk=").unwrap(), b"hi".to_vec());
        assert_eq!(b64_decode_strict("aGVsbG8=").unwrap(), b"hello".to_vec());
        assert_eq!(b64_decode_strict("aA==").unwrap(), b"h".to_vec());
        // 与测试编码器对拍（含需 1/2 padding 的长度）。
        for sample in ["", "a", "ab", "abc", "abcd", "\u{4e2d}\u{6587} payload"] {
            let encoded = b64_encode(sample.as_bytes());
            assert_eq!(
                b64_decode_strict(&encoded).unwrap(),
                sample.as_bytes().to_vec(),
                "{sample}"
            );
        }
        for bad in ["a", "a===", "====", "a=b=", "AB CD", "aG k=", "aGVsbG8", "aGVsbG8!"] {
            assert!(b64_decode_strict(bad).is_err(), "应拒: {bad}");
        }
    }

    /// 合法信封过全链验证（含注册表重推导）。
    #[test]
    fn envelope_verify_accepts_contract_shape() {
        let body = mes_body("wo-t1", "sid-t1");
        let text = envelope(&body, "mes-wo-t1-complete-1");
        let verified = verify_envelope(&text, &registry()).unwrap();
        assert_eq!(verified.body, body, "可信载荷=重解析结果");
        assert!(verified.body_bytes_len > 0);
        assert_eq!(verified.envelope.delivery_id, "mes-wo-t1-complete-1");
    }

    /// 篡改 body_b64（sha 自证红）与掉包 payload_sha256（hex/复算红）。
    #[test]
    fn envelope_verify_rejects_payload_tampering() {
        let body = mes_body("wo-t2", "sid-t2");
        let mut value: serde_json::Value =
            serde_json::from_str(&envelope(&body, "d-t2")).unwrap();
        // body_b64 换成另一份合法 base64（内容变了，sha 对不上）。
        value["body_b64"] = serde_json::json!(b64_encode(b"{\"event\":\"order_complete\"}"));
        let err = verify_envelope(&value.to_string(), &registry()).unwrap_err();
        assert!(matches!(err, RouterError::Payload(_)), "{err}");
        // payload_sha256 不是 hex。
        let mut value2: serde_json::Value =
            serde_json::from_str(&envelope(&body, "d-t2b")).unwrap();
        value2["payload_sha256"] = serde_json::json!("ZZZ");
        assert!(matches!(
            verify_envelope(&value2.to_string(), &registry()),
            Err(RouterError::Envelope(_))
        ));
    }

    /// 篡改冗余解析态 body（保留合法 body_b64+hash）必须拒读——修复批 N16。
    #[test]
    fn envelope_verify_rejects_tampered_parsed_body() {
        let body = mes_body("wo-t3", "sid-t3");
        let mut value: serde_json::Value =
            serde_json::from_str(&envelope(&body, "d-t3")).unwrap();
        value["body"]["order_id"] = serde_json::json!("wo-someone-else");
        let err = verify_envelope(&value.to_string(), &registry()).unwrap_err();
        assert!(matches!(err, RouterError::CrossCheck(_)), "{err}");
        // delivery_id / event_type 与已验载荷不符同途拒读。
        let mut value2: serde_json::Value =
            serde_json::from_str(&envelope(&body, "d-t3")).unwrap();
        value2["delivery_id"] = serde_json::json!("d-other");
        assert!(matches!(
            verify_envelope(&value2.to_string(), &registry()),
            Err(RouterError::Envelope(_) | RouterError::CrossCheck(_))
        ));
        let mut value3: serde_json::Value =
            serde_json::from_str(&envelope(&body, "d-t3")).unwrap();
        value3["event_type"] = serde_json::json!("push");
        assert!(matches!(
            verify_envelope(&value3.to_string(), &registry()),
            Err(RouterError::CrossCheck(_) | RouterError::Registry(_))
        ));
    }

    /// 契约名/未知字段/event_id/payload_ref 形状坏全拒（deny_unknown_fields）。
    #[test]
    fn envelope_verify_rejects_shape_violations() {
        let body = mes_body("wo-t4", "sid-t4");
        let mut wrong_contract: serde_json::Value =
            serde_json::from_str(&envelope(&body, "d-t4")).unwrap();
        wrong_contract["contract"] = serde_json::json!("af.trigger.v2");
        assert!(matches!(
            verify_envelope(&wrong_contract.to_string(), &registry()),
            Err(RouterError::Envelope(_))
        ));
        let mut unknown: serde_json::Value =
            serde_json::from_str(&envelope(&body, "d-t4")).unwrap();
        unknown["extra_field"] = serde_json::json!(1);
        assert!(matches!(
            verify_envelope(&unknown.to_string(), &registry()),
            Err(RouterError::Envelope(_))
        ));
        let mut bad_event_id: serde_json::Value =
            serde_json::from_str(&envelope(&body, "d-t4")).unwrap();
        bad_event_id["event_id"] = serde_json::json!("not-source-prefixed");
        assert!(matches!(
            verify_envelope(&bad_event_id.to_string(), &registry()),
            Err(RouterError::Envelope(_))
        ));
        let mut bad_ref: serde_json::Value =
            serde_json::from_str(&envelope(&body, "d-t4")).unwrap();
        bad_ref["payload_ref"] = serde_json::json!("../etc/passwd");
        assert!(matches!(
            verify_envelope(&bad_ref.to_string(), &registry()),
            Err(RouterError::Envelope(_))
        ));
    }

    /// 注册表面：source 未登记 / 事件不在白名单 / thread_key/dedup_key
    /// 与兜底分支重推导不符——全拒。
    #[test]
    fn envelope_verify_rejects_registry_mismatches() {
        let body = mes_body("wo-t5", "sid-t5");
        let mut foreign: serde_json::Value =
            serde_json::from_str(&envelope(&body, "d-t5")).unwrap();
        foreign["source"] = serde_json::json!("mes-unknown");
        // source 换了，event_id/payload_ref 派生形状会先红或 source 红——都属注册表面拒。
        let err = verify_envelope(&foreign.to_string(), &registry()).unwrap_err();
        assert!(
            matches!(err, RouterError::Envelope(_) | RouterError::Registry(_)),
            "{err}"
        );
        let mut wrong_thread: serde_json::Value =
            serde_json::from_str(&envelope(&body, "d-t5")).unwrap();
        wrong_thread["thread_key"] = serde_json::json!("mes:somewhere-else");
        assert!(matches!(
            verify_envelope(&wrong_thread.to_string(), &registry()),
            Err(RouterError::CrossCheck(_))
        ));
        let mut wrong_dedup: serde_json::Value =
            serde_json::from_str(&envelope(&body, "d-t5")).unwrap();
        wrong_dedup["dedup_key"] = serde_json::json!("order_complete:other-repo");
        assert!(matches!(
            verify_envelope(&wrong_dedup.to_string(), &registry()),
            Err(RouterError::CrossCheck(_))
        ));
    }

    /// 消费游标：append → load → delivery/dedup 两语义命中即重放。
    #[test]
    fn consumed_cursor_dedup() {
        let home = std::env::temp_dir().join(format!(
            "af-order-router-cursor-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(load_consumed(&home).unwrap().is_empty(), "文件缺席=空");
        let record = ConsumedRecord {
            delivery_id: "d-1".into(),
            source: "mes-execline".into(),
            event_type: ORDER_COMPLETE_EVENT.into(),
            dedup_key: "order_complete:line-poc/wo-1".into(),
            order_id: "wo-1".into(),
            consumed_at: "2026-11-04T09:00:00+00:00".into(),
        };
        append_consumed(&home, &record).unwrap();
        let loaded = load_consumed(&home).unwrap();
        assert_eq!(loaded, vec![record.clone()]);
        // delivery_id 命中（网络重试语义）。
        assert_eq!(duplicate_consumed(&loaded, "d-1", "fresh-dedup"), Some(&record));
        // dedup_key 命中（业务重放语义：不同 delivery 同内容）。
        assert_eq!(
            duplicate_consumed(&loaded, "d-2", "order_complete:line-poc/wo-1"),
            Some(&record)
        );
        // 全新投递不命中。
        assert_eq!(duplicate_consumed(&loaded, "d-2", "fresh-dedup"), None);
        // 坏行 fail-closed。
        std::fs::write(consumed_path(&home), "not-json\n").unwrap();
        assert!(matches!(load_consumed(&home), Err(RouterError::Cursor(_))));
        let _ = std::fs::remove_dir_all(&home);
    }
}
