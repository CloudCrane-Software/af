//! adjudicate —— 裁决环：裁决单生成（af-order 侧）+ steward 认领与签字闭环。
//!
//! v0.3 T2 终裁的触发链在本 crate 的落点（不在 af-engine 造事件总线）：
//!
//! ```text
//! af-execline complete → MES 事件 → (event-router 消费腿，PoC-D 面)
//!   → 本模块 generate_adjudication_doc   （af-order 生成裁决单）
//!   → af-station Station::enqueue        （队列，幂等键 UNIQUE）
//!   → Station::claim                     （steward 原子认领 + 租约）
//!   → DecisionArtifact::seal             （af-workorder 真签路径，不另造签名）
//!   → crate::admit::admit                （OR21xx 校验链接收，receipt 落账）
//!   → Station::complete                  （任务闭环 done）
//! ```
//!
//! # 裁决单文书（②）
//!
//! 裁决单是**配方层文书**：`doc_kind="裁决单"`、`template_id=
//! "tpl-adjudication-v1"`，不进任何 Rust OrderType 枚举（硬约束）。模板正文
//! 正本 = [`TEMPLATE_BODY`]（YAML 形文本常量，配方层留档；模板正文若需独立
//! 文件，走配方层目录——本批无既有配方模板目录约定，暂以常量 + 摘要留档，
//! 见模块边界说明）。正文摘要进 [`AdjudicationTemplate::body_sha256`]，
//! 与生成/受理两侧同源。
//!
//! 生成物 [`AdjudicationDoc`] 可序列化落盘、可复算：除 `doc_sha256` 外全部
//! 字段的 canonical JSON 字节 SHA-256 = `doc_sha256`（[`verify_doc`] 独立重
//! 算）。内容含：源单 stable_id、裁决事项（subject）、basis 列表（全部来自
//! 源单实际工件/凭证的重算摘要，[`resolved_basis`]）、next_orders（计划面，
//! [`crate::admit::canonical_next_orders_digest`] 同口径）、expires_at。
//!
//! # steward 闭环（③）
//!
//! [`run_steward_loop`] 全程只接线既有件，零新机制：
//! - **入队幂等**：af-station `idempotency_key = {源单 stable_id}/{裁决单 id}`
//!   （UNIQUE，重复入队返回 `Duplicate`，不产生第二行）；
//! - **认领**：`Station::claim(worker_id, "steward", lease_ms)` 带守卫 UPDATE
//!   原子认领；角色不匹配领不到（af-station 角色路由）；
//! - **真签**：复用 af-workorder `DecisionArtifact::seal`（ed25519，四扩展字
//!   段进被签字节），payload = `WorkOrder::admission_decision_payload`；
//! - **角色纪律（不造权限系统）**：Role 阶梯与 Decision 所需权限全部交给
//!   af-workorder/af-engine 既有四层校验——`seal` 取角色默认权限集，引擎第
//!   ②③层拒绝越权（Reviser 无 Approve、Reader 无任何裁决权、Arbitrate 仅
//!   Arbiter 有 [`af_workorder::Permission::Arbitrate`]，只有 Arbiter 能
//!   pivot）；越权签发在 admit 第③步 OR2106 类型化拒绝；
//! - **受理**：`crate::admit::admit`（OR2101-2112 全链）；
//! - **continue/rework 闭环**：Decision=Approve/Arbitrate → 原单继续，receipt
//!   留档即留痕；Decision=Revise → rework：按被签计划面 `next_orders` 逐一生
//!   成后继 rework 工单（`WorkOrder::successor`，supersedes=源单，账本落
//!   create_order 行，Created 态 = 待修订）；Decision=Reject → abort：源单
//!   cancel + 账本落 cancel 行（Closed/Cancelled 终态无 abort 边，如实报
//!   [`AdjudicateError::MissingEdge`]，不假装迁移）。
//!
//! # 跨库意图账（开放风险 #5：跨库无原子事务的 prepared/committed 补偿）
//!
//! 一次闭环横跨四类持久面：裁决单文书（文件）、af-station lease 库（sqlite：
//! enqueue/claim/complete）、订单受理账（`admissions.jsonl`：receipt）、WIP 面
//! （rework 工单账本 / 源单 cancel 行）——没有任何跨库原子事务。此前的失败
//! 语义只有「claimed 留检 + 租约到期回炉」，而这在**提交点之后**的崩溃窗
//! （admit 已落账、`station.complete` 未落）里会双写：任务回炉被再认领 →
//! 重签 seq+1 → 同一裁决单二次受理（见 poc/poc-cross-db-atomicity 改前读数）。
//!
//! 补偿面（本模块落地，最小两阶段语义）：
//! - **prepared**：跨库提交点（admit）**之前**落 [`PreparedIntent`]——
//!   「我将要受理什么」，含回执幂等键；
//! - **committed**：提交点之后落 [`CommittedFact`]——分支结算事实
//!   （rework 清单 / 是否 abort）；
//! - **completed**：任务 complete 落账后收口；
//! - **重放幂等臂**：认领到的裁决单若已过提交点（committed 线在，或
//!   prepared 的回执键在受理账——盖住「admit 落账 ↔ committed 线之间」的
//!   崩溃窗），不重签不重受理，按意图账收敛剩余面后返回原回执；
//! - **[`reconcile_adjudications`]**：扫描全部意图账，检出在飞残留
//!   （prepared 无回执 = 留检面；有回执 = 分支幂等结算 + 线补缺），任务面
//!   最终 complete 由重放臂在下一次认领时落。
//!
//! 意图账是既有 adjudications 目录里的 append-only JSONL（fsync 追加，
//! 与 `append_receipt`/`OrderLedger` 同款纪律），**不是另一套库**。

use std::fs;
use std::path::{Path, PathBuf};

use af_execline::{EngineClient, LedgerRecord};
use af_station::{Enqueued, NewTask, Station};
use af_workorder::{
    sha256_hex, Decision, DecisionArtifact, DecisionAttributes, Ed25519Keypair, Priority, Role,
    Sla, StableId, TrustedActorRegistry, WorkOrder,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::admit::{
    admit, budget_voucher_digest, canonical_next_orders_digest, load_receipts,
    AdjudicationTemplate, AdmissionReceipt, AdmissionRequest, GateListSpec, GateSpec,
    ResourceClaim, ResourceProfileSpec,
};
use crate::state::{StateDir, LEDGER_FILE};

/// 裁决单文书种类（T1 终裁文书模板最小集内的配方层面，非 Rust 枚举）。
pub const ADJUDICATION_DOC_KIND: &str = "裁决单";
/// 裁决单文书模板 id（admit.rs 配方模板最小集既有约定值）。
pub const ADJUDICATION_TEMPLATE_ID: &str = "tpl-adjudication-v1";
/// 裁决单文书模板版本。
pub const ADJUDICATION_TEMPLATE_VERSION: &str = "1.0";
/// steward 认领 af-station 队列任务所用角色名（required_role 精确匹配）。
pub const STEWARD_ROLE: &str = "steward";
/// 裁决单落盘目录名（state 根下；再按源单 order_id 分目录）。
pub const ADJUDICATIONS_DIR: &str = "adjudications";

/// 裁决单文书模板正文正本（配方层 YAML 形留档；本批不新增模板文件，见模块
/// 文档）。`AdjudicationTemplate::body_sha256` = 本正本的 SHA-256，生成与受
/// 理两侧同源同值。
pub const TEMPLATE_BODY: &str = r#"# tpl-adjudication-v1 —— 裁决单文书模板（配方层正本）
doc_kind: 裁决单
fields:
  adjudication_id: 裁决单号（adj-<order_id>-<n>，幂等/队列键组成）
  source_order_id: 源单订单号
  source_stable_id: 源单稳定身份
  subject: 裁决事项
  basis: 决策依据（每条 <kind>:<entity_id>:<sha256>，可独立重算）
  next_orders: 下一步订单集合（计划面，digest 输入）
  next_orders_digest: 计划面 SHA-256（af-order/next-orders/v1 规范化）
  expires_at: 决策有效期（RFC3339）
digest: doc_sha256 = 除本摘要外全部字段 canonical JSON 的 SHA-256
"#;

/// 裁决单模板正文摘要（生成/受理两侧共用的唯一取值点）。
pub fn template_body_sha256() -> String {
    sha256_hex(TEMPLATE_BODY.as_bytes())
}

/// adjudicate 模块的 PoC 配方缺省资源画像（schema 面；生产面由配方层供给，
/// 同 examples/admit_seal 缺省样例纪律——schema 校验需要非空声明）。
pub fn loop_resource_profile() -> ResourceProfileSpec {
    ResourceProfileSpec {
        profile_id: "rp-adjudicate".into(),
        profile_version: "1.0".into(),
        claims: vec![ResourceClaim {
            resource: "glm-cp".into(),
            units: 1,
        }],
    }
}

/// adjudicate 模块的 PoC 配方缺省门清单（schema 面；同上纪律）。
pub fn loop_gate_list() -> GateListSpec {
    GateListSpec {
        gates: vec![GateSpec {
            gate_id: "F1".into(),
            kind: "fast".into(),
            required: true,
        }],
    }
}

/// 裁决单文书（可序列化、可复算；线格式 = JSON，deny_unknown_fields）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdjudicationDoc {
    /// 文书种类：恒为 [`ADJUDICATION_DOC_KIND`]（"裁决单"）。
    pub doc_kind: String,
    /// 文书模板 id：恒为 [`ADJUDICATION_TEMPLATE_ID`]。
    pub template_id: String,
    pub template_version: String,
    /// 裁决单号：`adj-{source_order_id}-{n}`（n = 该源单第 n 份裁决单），
    /// 同时是 af-station 任务 id 与幂等键组成面。
    pub adjudication_id: String,
    /// 源单订单号（state/orders/ 目录名，路径安全）。
    pub source_order_id: String,
    /// 源单稳定身份（生成时从账本快照取，队列幂等键组成面）。
    pub source_stable_id: String,
    /// 裁决事项。
    pub subject: String,
    /// 决策依据（[`resolved_basis`] 产物：全部条目可被 admit OR2109 独立重算）。
    pub basis: Vec<String>,
    /// 下一步订单集合（计划面；[`crate::admit::canonical_next_orders_digest`]
    /// 的输入，Revise 分支据此物化 rework 工单）。
    pub next_orders: Vec<String>,
    /// 计划面摘要（与工件被签字节 `next_orders_digest` 同值）。
    pub next_orders_digest: String,
    /// 决策有效期（RFC3339）。
    pub expires_at: DateTime<Utc>,
    /// 文书摘要：除本字段外全部字段 canonical JSON 字节的 SHA-256。
    pub doc_sha256: String,
}

impl AdjudicationDoc {
    /// 复算输入：除 `doc_sha256` 外全部字段的 canonical JSON 字节。
    fn body_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut value = serde_json::to_value(self)?;
        value
            .as_object_mut()
            .expect("AdjudicationDoc 是 JSON 对象")
            .remove("doc_sha256");
        serde_json::to_vec(&value)
    }

    /// 复算文书摘要（生成与 steward 侧共用同一口径）。
    pub fn recompute_sha256(&self) -> Result<String, serde_json::Error> {
        Ok(sha256_hex(&self.body_bytes()?))
    }

    /// 落盘路径：`<state>/adjudications/<source_order_id>/<adjudication_id>.json`。
    pub fn path(&self, state: &StateDir) -> PathBuf {
        state
            .root()
            .join(ADJUDICATIONS_DIR)
            .join(&self.source_order_id)
            .join(format!("{}.json", self.adjudication_id))
    }

    /// 序列化落盘（父目录按需创建；返回路径）。
    pub fn persist(&self, state: &StateDir) -> Result<PathBuf, AdjudicateError> {
        let path = self.path(state);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| AdjudicateError::Doc(format!("创建 {}: {err}", parent.display())))?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|err| AdjudicateError::Doc(format!("裁决单序列化失败: {err}")))?;
        fs::write(&path, format!("{text}\n")).map_err(|err| {
            AdjudicateError::Doc(format!("裁决单落盘失败 {}: {err}", path.display()))
        })?;
        Ok(path)
    }
}

/// 独立复算一份裁决单（steward 认领后的第一道门）：字段集形状 + doc_sha256
/// 重算一致。文书被掉包任何内容字段都红。
pub fn verify_doc(doc: &AdjudicationDoc) -> Result<(), AdjudicateError> {
    if doc.doc_kind != ADJUDICATION_DOC_KIND || doc.template_id != ADJUDICATION_TEMPLATE_ID {
        return Err(AdjudicateError::DocVerify(format!(
            "doc_kind/template_id 不在裁决单模板最小集（{} / {}）",
            doc.doc_kind, doc.template_id
        )));
    }
    let actual = doc
        .recompute_sha256()
        .map_err(|err| AdjudicateError::DocVerify(format!("文书序列化失败: {err}")))?;
    if actual != doc.doc_sha256 {
        return Err(AdjudicateError::DocVerify(format!(
            "文书摘要不符：声称 {}，复算 {actual}",
            doc.doc_sha256
        )));
    }
    Ok(())
}

/// 从盘上读回并复算一份裁决单。
pub fn load_doc(path: &Path) -> Result<AdjudicationDoc, AdjudicateError> {
    let text = fs::read_to_string(path)
        .map_err(|err| AdjudicateError::Doc(format!("裁决单读取失败 {}: {err}", path.display())))?;
    let doc: AdjudicationDoc = serde_json::from_str(&text).map_err(|err| {
        AdjudicateError::DocVerify(format!("裁决单不是合法 JSON（{}）: {err}", path.display()))
    })?;
    verify_doc(&doc)?;
    Ok(doc)
}

// ============================================================
// 跨库意图账（开放风险 #5：prepared/committed 补偿）
// ============================================================

/// 意图账文件后缀（`<state>/adjudications/<order_id>/<裁决单号>.journal.jsonl`）。
/// `.jsonl` 结尾不进 [`next_adjudication_id`] 的 `*.json` 计数。
pub const JOURNAL_SUFFIX: &str = ".journal.jsonl";

/// prepared 意图：跨库提交点（admit 落受理账）**之前**落盘的「我将要受理
/// 什么」。有了这行，崩溃后的任何重放都能按 `receipt_idempotency_key` 对
/// 账出「这笔提交到底落没落」，不靠猜。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedIntent {
    pub adjudication_id: String,
    /// 认领到的 af-station 任务 id（与裁决单号同源；留键面供 reconcile 对账）。
    pub task_id: String,
    pub source_order_id: String,
    pub source_stable_id: String,
    pub decision: Decision,
    pub decision_seq: u64,
    pub decision_digest: String,
    /// admit 回执幂等键（`{stable}/admit/{seq}/{digest}`，与受理账同键）。
    pub receipt_idempotency_key: String,
    /// 计划面（Revise 分支幂等物化的输入面）。
    pub next_orders: Vec<String>,
    pub ts_ms: i64,
}

/// committed 事实：跨库提交点之后、任务 complete 之前，各库面实际发生的
/// 分支结算结果——重放臂据此跳过分支，不凭空推断。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommittedFact {
    pub receipt_idempotency_key: String,
    /// Revise 分支物化的 rework 工单清单（其余分支为空）。
    pub rework_order_ids: Vec<String>,
    /// Reject 分支是否执行了 abort。
    pub aborted: bool,
    pub ts_ms: i64,
}

/// 意图账一行（append-only；最新到某相位即代表当前相位，重试可产生多条
/// prepared——意图在账，历史不抹）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum JournalLine {
    Prepared(PreparedIntent),
    Committed(CommittedFact),
    Completed { ts_ms: i64 },
}

/// 一份裁决单的跨库意图账句柄（打开/追加/相位判读）。
#[derive(Debug)]
pub struct AdjudicationJournal {
    path: PathBuf,
    lines: Vec<JournalLine>,
}

impl AdjudicationJournal {
    /// 意图账路径（与裁决单文书同目录）。
    pub fn path(state: &StateDir, source_order_id: &str, adjudication_id: &str) -> PathBuf {
        state
            .root()
            .join(ADJUDICATIONS_DIR)
            .join(source_order_id)
            .join(format!("{adjudication_id}{JOURNAL_SUFFIX}"))
    }

    /// 打开意图账：文件缺席 = 空账（首走之前本来就没有意图）；存在则重载
    /// 校验（坏行 fail-loud，不静默跳过）。
    pub fn open(
        state: &StateDir,
        source_order_id: &str,
        adjudication_id: &str,
    ) -> Result<Self, AdjudicateError> {
        let path = Self::path(state, source_order_id, adjudication_id);
        let mut lines = Vec::new();
        if path.is_file() {
            let text = fs::read_to_string(&path).map_err(|err| {
                AdjudicateError::Doc(format!("意图账读取失败 {}: {err}", path.display()))
            })?;
            for (index, line) in text.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let parsed: JournalLine = serde_json::from_str(trimmed).map_err(|err| {
                    AdjudicateError::Doc(format!(
                        "意图账第 {} 行不是合法记录（{}）: {err}",
                        index + 1,
                        path.display()
                    ))
                })?;
                lines.push(parsed);
            }
        }
        Ok(Self { path, lines })
    }

    /// 追加一行：fsync 落盘后才更新内存视图——崩溃时内存不领先于盘
    /// （与 `append_receipt`/`OrderLedger::append` 同款纪律）。
    fn append(&mut self, line: JournalLine) -> Result<(), AdjudicateError> {
        use std::io::Write as _;
        let text = serde_json::to_string(&line)
            .map_err(|err| AdjudicateError::Doc(format!("意图账序列化失败: {err}")))?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| AdjudicateError::Doc(format!("创建 {}: {err}", parent.display())))?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|err| {
                AdjudicateError::Doc(format!("意图账打开失败 {}: {err}", self.path.display()))
            })?;
        file.write_all(text.as_bytes())
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.sync_all())
            .map_err(|err| {
                AdjudicateError::Doc(format!("意图账落盘失败 {}: {err}", self.path.display()))
            })?;
        self.lines.push(line);
        Ok(())
    }

    /// 落 prepared 意图（跨库提交点之前）。
    pub fn append_prepared(&mut self, intent: PreparedIntent) -> Result<(), AdjudicateError> {
        self.append(JournalLine::Prepared(intent))
    }

    /// 落 committed 事实（提交点之后、complete 之前）。
    pub fn append_committed(&mut self, fact: CommittedFact) -> Result<(), AdjudicateError> {
        self.append(JournalLine::Committed(fact))
    }

    /// 落 completed 收口线（任务 complete 落账之后）。
    pub fn append_completed(&mut self) -> Result<(), AdjudicateError> {
        let ts_ms = Utc::now().timestamp_millis();
        self.append(JournalLine::Completed { ts_ms })
    }

    /// 全部历史行（审计面；相位判读用下方三个取值器）。
    pub fn lines(&self) -> &[JournalLine] {
        &self.lines
    }

    /// 最新一条 prepared 意图（无 = 从未走到提交点前）。
    pub fn latest_prepared(&self) -> Option<&PreparedIntent> {
        self.lines.iter().rev().find_map(|line| match line {
            JournalLine::Prepared(intent) => Some(intent),
            _ => None,
        })
    }

    /// 最新一条 committed 事实。
    pub fn latest_committed(&self) -> Option<&CommittedFact> {
        self.lines.iter().rev().find_map(|line| match line {
            JournalLine::Committed(fact) => Some(fact),
            _ => None,
        })
    }

    /// 是否已有 completed 线（跨库事务全部面已收敛）。
    pub fn is_completed(&self) -> bool {
        self.lines
            .iter()
            .any(|line| matches!(line, JournalLine::Completed { .. }))
    }
}

/// 从源单实际状态解析"决策依据"清单（生成侧；每条都能过 admit OR2109）：
/// 源单每个工件一条 `artifact:<id>:<content_sha256>`，绑定预算凭证再加一条
/// `voucher:<id>:<canonical 摘要>`。账本行不作默认依据（生成面只列实质实体；
/// admit 侧 `ledger:` 文法保留，运营者可显式引用）。
pub fn resolved_basis(order: &WorkOrder) -> Vec<String> {
    let mut basis = Vec::new();
    for artifact in order.artifacts() {
        basis.push(format!(
            "artifact:{}:{}",
            artifact.artifact_id,
            artifact.content_sha256()
        ));
    }
    if let Some(voucher) = order.budget_voucher() {
        basis.push(format!(
            "voucher:{}:{}",
            voucher.voucher_id,
            budget_voucher_digest(voucher)
        ));
    }
    basis
}

/// 裁决环失败（类型化；admit/state 面的 OR21xx/OR20xx 由 [`AdjudicateError::Order`]
/// 原样透传，不吞码）。
#[derive(Debug, thiserror::Error)]
pub enum AdjudicateError {
    /// af-order 面（源单恢复 / admit 校验链 / 状态读写）失败，OR 码透传。
    #[error("裁决环 af-order 面失败: {0}")]
    Order(#[from] crate::error::OrderError),
    /// 裁决单生成/落盘失败。
    #[error("裁决单生成失败: {0}")]
    Doc(String),
    /// 裁决单文书复算不过（被掉包/损坏/文书面不符）。
    #[error("裁决单文书复算未过: {0}")]
    DocVerify(String),
    /// af-station 队列面失败。
    #[error("裁决环队列面失败: {0}")]
    Station(#[from] af_station::Error),
    /// 认领/完成等队列语义失败（非 IO）。
    #[error("裁决环队列语义失败: {0}")]
    Queue(String),
    /// 状态机没有所要求的边（如 Closed→Cancelled 的 abort）——如实拒绝，
    /// 不假装迁移。
    #[error("状态机无此边: {0}")]
    MissingEdge(String),
}

/// 生成一份裁决单文书（②）：对已落账源单（判据面 = 账本最近快照）生成
/// doc_kind="裁决单" 文书。裁决事项 subject 从源单事实拼出；basis =
/// [`resolved_basis`]；计划面 next_orders 由调用方声明（空 = 无后继订单），
/// digest 按受理侧同一规范化现算。落盘并返回（文书、源单快照）。
pub fn generate_adjudication_doc(
    state: &StateDir,
    source_order_id: &str,
    next_orders: Vec<String>,
    expires_at: DateTime<Utc>,
) -> Result<(AdjudicationDoc, WorkOrder), AdjudicateError> {
    let (order, _records) = crate::admit::recover_source_order(state, source_order_id)?;
    let basis = resolved_basis(&order);
    if basis.is_empty() {
        return Err(AdjudicateError::Doc(format!(
            "源单 {source_order_id} 无可引用依据实体（无工件且未绑预算凭证），拒绝生成空依据裁决单"
        )));
    }
    let digest = canonical_next_orders_digest(&next_orders)
        .map_err(|reason| AdjudicateError::Doc(format!("计划面 next_orders 不合法: {reason}")))?;
    let stable_id = order.stable_id().as_str().to_string();
    let subject = format!(
        "对源单 {}（stable_id={stable_id}，state={}）闭环后的裁决受理",
        source_order_id,
        serde_json::to_value(order.state())
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| format!("{:?}", order.state())),
    );
    let doc = AdjudicationDoc {
        doc_kind: ADJUDICATION_DOC_KIND.into(),
        template_id: ADJUDICATION_TEMPLATE_ID.into(),
        template_version: ADJUDICATION_TEMPLATE_VERSION.into(),
        adjudication_id: next_adjudication_id(state, source_order_id),
        source_order_id: source_order_id.to_string(),
        source_stable_id: stable_id,
        subject,
        basis,
        next_orders_digest: digest,
        next_orders,
        expires_at,
        doc_sha256: String::new(),
    };
    // 摘要字段最后自举：对去摘要 canonical 体现算。
    let mut doc = doc;
    doc.doc_sha256 = doc
        .recompute_sha256()
        .map_err(|err| AdjudicateError::Doc(format!("裁决单摘要自举失败: {err}")))?;
    verify_doc(&doc)?;
    Ok((doc, order))
}

/// 该源单下一份裁决单号：`adj-{order_id}-{已落盘份数+1}`（确定性；重跑同
/// 源单生成新号新轮次，同号重入队由队列幂等键接住）。
fn next_adjudication_id(state: &StateDir, source_order_id: &str) -> String {
    let dir = state.root().join(ADJUDICATIONS_DIR).join(source_order_id);
    let count = fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
                .count()
        })
        .unwrap_or(0);
    format!("adj-{source_order_id}-{}", count + 1)
}

/// steward 闭环的裁决种子（一次裁决 = 一份种子）。
pub struct AdjudicationSeed<'a> {
    /// 源单订单号。
    pub order_id: &'a str,
    /// 裁决结论（Approve/Reject/Revise/Arbitrate）。
    pub decision: Decision,
    /// 计划面（进文书与被签字节；Revise 分支据此物化 rework 工单）。
    pub next_orders: Vec<String>,
    /// 决策有效期。
    pub expires_at: DateTime<Utc>,
    /// 签字 actor（须在受信注册表）。
    pub actor_id: &'a str,
    /// 签字角色（角色默认权限集即能力上限；越权由引擎四层拒绝）。
    pub actor_role: Role,
    /// 签字私钥（af-workorder 真签路径）。
    pub actor_keypair: &'a Ed25519Keypair,
    /// af-station 认领方 worker id。
    pub worker_id: &'a str,
    /// 认领租约毫秒。
    pub lease_ms: u64,
}

/// steward 闭环的外部设施面（state/队列/引擎/注册表）。
pub struct LoopFacilities<'a> {
    pub state: &'a StateDir,
    pub station: &'a Station,
    pub client: &'a EngineClient,
    pub registry: &'a TrustedActorRegistry,
    /// 验签/受理时刻（admit `now`）。
    pub now: DateTime<Utc>,
}

/// steward 闭环的产物（全部落点，供留证与断言）。
#[derive(Debug)]
pub struct StewardOutcome {
    pub adjudication_id: String,
    pub doc: AdjudicationDoc,
    pub doc_path: PathBuf,
    pub enqueue: Enqueued,
    pub task_id: String,
    pub receipt: AdmissionReceipt,
    /// Revise 分支物化的 rework 工单 id（其余分支为空）。
    pub rework_order_ids: Vec<String>,
    /// Reject 分支的 abort 是否执行（终态源单无 abort 边时会先报错，此字段
    /// 只在成功路径出现）。
    pub aborted: bool,
}

/// 一次 steward 闭环（③）：生成裁决单 → 入队 → 认领 → 复算文书 →
/// （重放幂等臂：已过提交点则按意图账收敛，返回原回执）→ 真签 → prepared
/// 意图落账 → admit → 决策分支幂等结算 → committed 线 → complete 任务 →
/// completed 线。
///
/// 任何一步失败即向上传播类型化错误；认领后的任务**不**自动回滚（租约到期
/// 由 af-station 惰性回收），失败面留给运营者检查，不静默吞任务——**但**
/// 提交点（admit 落账）之后的崩溃不再靠人肉分辨：意图账记下 prepared/
/// committed/completed 三相，租约回炉后的再认领由重放幂等臂收敛（不二次
/// 受理），无人值守扫描见 [`reconcile_adjudications`]。
pub fn run_steward_loop(
    seed: &AdjudicationSeed<'_>,
    f: &LoopFacilities<'_>,
) -> Result<StewardOutcome, AdjudicateError> {
    // ── ② 裁决单生成 ──────────────────────────────────────────
    let (doc, _source) = generate_adjudication_doc(
        f.state,
        seed.order_id,
        seed.next_orders.clone(),
        seed.expires_at,
    )?;
    let doc_path = doc.persist(f.state)?;

    // ── ③ 入队（幂等键 = 源单 stable_id + 裁决单 id，UNIQUE 防重复入队）──
    let idempotency_key = format!("{}/{}", doc.source_stable_id, doc.adjudication_id);
    let enqueue = f.station.enqueue(NewTask {
        id: &doc.adjudication_id,
        payload_ref: doc_path.to_string_lossy().as_ref(),
        required_role: STEWARD_ROLE,
        idempotency_key: Some(&idempotency_key),
        deadline: None,
    })?;

    // ── 认领（带守卫 UPDATE 原子认领；角色不匹配领不到）──────────
    let Some(task) = f
        .station
        .claim(seed.worker_id, STEWARD_ROLE, seed.lease_ms)?
    else {
        return Err(AdjudicateError::Queue(format!(
            "steward {} 认领不到 {STEWARD_ROLE} 任务（队列空或角色不匹配）",
            seed.worker_id
        )));
    };
    let task_id = task.id.clone();

    // ── 复算文书（认领后第一道门：payload_ref 指向的落盘文书独立重算）──
    let claimed_path = PathBuf::from(&task.payload_ref);
    let claimed = load_doc(&claimed_path)?;

    // ── 重放幂等臂（风险#5）：认领到的裁决单若已过跨库提交点，不再重签/
    //    重受理——那是同一裁决单的二次受理（双写）。按意图账收敛剩余面后
    //    返回原回执。提交点判据见 [`committed_receipt`]。──────────────
    let mut journal =
        AdjudicationJournal::open(f.state, &claimed.source_order_id, &claimed.adjudication_id)?;
    if let Some(receipt) = committed_receipt(f.state, &journal, &claimed)? {
        return steward_replay(
            f,
            seed.worker_id,
            claimed,
            claimed_path,
            &mut journal,
            ReplayCarry {
                enqueue,
                task_id,
                receipt,
            },
        );
    }

    // ── 真签（af-workorder 既有路径；四扩展字段进被签字节）────────
    let (mut source, _records) =
        crate::admit::recover_source_order(f.state, &claimed.source_order_id)?;
    let seq = next_decision_seq(f.state, &claimed.source_order_id)?;
    let payload = source.admission_decision_payload();
    let attributes = DecisionAttributes::new(
        seq,
        claimed.basis.clone(),
        claimed.next_orders_digest.clone(),
        claimed.expires_at,
    );
    let decision = DecisionArtifact::seal(
        seed.actor_id,
        seed.actor_role,
        seed.decision,
        &payload,
        seed.actor_keypair,
        attributes,
    );

    // ── prepared 意图落账（风险#5：跨库提交点之前，先落「将受理什么」）──
    let digest = crate::admit::decision_digest(&decision);
    let stable_id = source.stable_id().as_str().to_string();
    let receipt_idempotency_key = crate::admit::idempotency_key(&stable_id, seq, &digest);
    journal.append_prepared(PreparedIntent {
        adjudication_id: claimed.adjudication_id.clone(),
        task_id: task_id.clone(),
        source_order_id: claimed.source_order_id.clone(),
        source_stable_id: stable_id,
        decision: seed.decision,
        decision_seq: seq,
        decision_digest: digest,
        receipt_idempotency_key: receipt_idempotency_key.clone(),
        next_orders: claimed.next_orders.clone(),
        ts_ms: Utc::now().timestamp_millis(),
    })?;

    // ── admit 受理（OR21xx 全链；越权角色在此 OR2106 类型化拒绝）────
    let held_voucher = source
        .budget_voucher()
        .map(|voucher| voucher.voucher_id.clone())
        .ok_or_else(|| {
            AdjudicateError::Queue(format!(
                "源单 {} 未绑预算凭证，steward 无法呈报受理所需凭证",
                claimed.source_order_id
            ))
        })?;
    let request = AdmissionRequest {
        order_id: claimed.source_order_id.clone(),
        decision,
        actor_registry: f.registry.clone(),
        template: AdjudicationTemplate {
            template_id: claimed.template_id.clone(),
            template_version: claimed.template_version.clone(),
            doc_kind: claimed.doc_kind.clone(),
            body_sha256: template_body_sha256(),
        },
        resource_profile: loop_resource_profile(),
        gate_list: loop_gate_list(),
        next_orders: claimed.next_orders.clone(),
        required_credentials: vec![held_voucher],
        validated_at: Some(f.now),
    };
    let receipt = admit(&request, f.state, f.client, f.now)?;

    // ── continue / rework / abort 分支（幂等结算，风险#5）+ committed 线 ──
    let (rework_order_ids, aborted) =
        settle_decision_branches(f.state, &mut source, seed.decision, &claimed.next_orders)?;
    journal.append_committed(CommittedFact {
        receipt_idempotency_key: receipt.idempotency_key.clone(),
        rework_order_ids: rework_order_ids.clone(),
        aborted,
        ts_ms: Utc::now().timestamp_millis(),
    })?;

    // ── complete 任务（只有现任 claimant 能置 done）──────────────
    if !f.station.complete(&task_id, seed.worker_id)? {
        return Err(AdjudicateError::Queue(format!(
            "任务 {task_id} 完成失败（非现任 claimant 或已被接管）"
        )));
    }

    // ── completed 线：跨库事务全部面（文书/lease 库/受理账/WIP 面）收敛 ──
    journal.append_completed()?;

    Ok(StewardOutcome {
        adjudication_id: claimed.adjudication_id.clone(),
        doc: claimed,
        doc_path,
        enqueue,
        task_id,
        receipt,
        rework_order_ids,
        aborted,
    })
}

/// 意图账判读：这份裁决单是否已过跨库提交点（受理账已有该 prepared 意图
/// 对应的回执）。三种在账证据任一即真：
/// 1. committed / completed 线在——线是提交点之后落的，受理账必含该回执
///    （对不上 = 跨库账面漂移，fail-loud 拒重放）；
/// 2. 仅 prepared 线时**以受理账为准**对账回执键——盖住「admit 已落账 ↔
///    committed 线未落」的崩溃窗（这正是意图账存在的意义）。
///
/// 返回受理账中的原回执（无 = 首走：从未到过提交点）。
fn committed_receipt(
    state: &StateDir,
    journal: &AdjudicationJournal,
    claimed: &AdjudicationDoc,
) -> Result<Option<AdmissionReceipt>, AdjudicateError> {
    let Some(prepared) = journal.latest_prepared() else {
        return Ok(None); // 无在飞意图 = 首走
    };
    let receipts = crate::admit::load_receipts(state, &claimed.source_order_id)?;
    let original = receipts
        .into_iter()
        .find(|receipt| receipt.idempotency_key == prepared.receipt_idempotency_key);
    if journal.latest_committed().is_some() || journal.is_completed() {
        return original.map(Some).ok_or_else(|| {
            AdjudicateError::Queue(format!(
                "意图账称裁决单 {} 已过提交点（回执键 {}），受理账中无此回执——\
                 跨库账面漂移，拒绝重放，留检",
                claimed.adjudication_id, prepared.receipt_idempotency_key
            ))
        });
    }
    Ok(original)
}

/// 重放臂的携带面（认领现场的既有事实，收拢参数列）。
struct ReplayCarry {
    /// 本次闭环 enqueue 步的结果（如实回传 outcome）。
    enqueue: Enqueued,
    /// 认领到的任务 id。
    task_id: String,
    /// 受理账中的原回执（[`committed_receipt`] 已判真在账）。
    receipt: AdmissionReceipt,
}

/// 重放幂等臂本体（风险#5）：跨库提交点已过——不重签、不重受理（不产生
/// 第二笔受理账）；分支按 committed 线事实复用（线缺 = 幂等重结算，盖住
/// 「admit 落账 ↔ 分支结算之间」的崩溃窗）；complete 任务后落 completed 线。
/// 返回原回执（`replayed=true`）。
fn steward_replay(
    f: &LoopFacilities<'_>,
    worker_id: &str,
    claimed: AdjudicationDoc,
    claimed_path: PathBuf,
    journal: &mut AdjudicationJournal,
    carry: ReplayCarry,
) -> Result<StewardOutcome, AdjudicateError> {
    let prepared = journal
        .latest_prepared()
        .cloned()
        .expect("committed_receipt 已保证在飞意图在账");
    // 分支结算：committed 线在 = 事实已留档，按线复用；缺线 = 幂等重结算。
    let (rework_order_ids, aborted) = if let Some(fact) = journal.latest_committed() {
        (fact.rework_order_ids.clone(), fact.aborted)
    } else {
        let (mut source, _records) =
            crate::admit::recover_source_order(f.state, &claimed.source_order_id)?;
        let settled = settle_decision_branches(
            f.state,
            &mut source,
            prepared.decision,
            &claimed.next_orders,
        )?;
        journal.append_committed(CommittedFact {
            receipt_idempotency_key: prepared.receipt_idempotency_key.clone(),
            rework_order_ids: settled.0.clone(),
            aborted: settled.1,
            ts_ms: Utc::now().timestamp_millis(),
        })?;
        settled
    };
    if !f.station.complete(&carry.task_id, worker_id)? {
        return Err(AdjudicateError::Queue(format!(
            "重放收敛：任务 {} 完成失败（非现任 claimant 或已被接管）",
            carry.task_id
        )));
    }
    journal.append_completed()?;
    let mut receipt = carry.receipt;
    receipt.replayed = true;
    Ok(StewardOutcome {
        adjudication_id: claimed.adjudication_id.clone(),
        doc: claimed,
        doc_path: claimed_path,
        enqueue: carry.enqueue,
        task_id: carry.task_id,
        receipt,
        rework_order_ids,
        aborted,
    })
}

/// 决策分支结算（continue / rework / abort），**幂等**（风险#5）：崩溃窗
/// 重放不重复物化、不重复 abort。Approve/Arbitrate 无分支动作；Revise 按
/// 计划面物化 rework 后继单（[`spawn_rework_order`] 同号已物化即跳过）；
/// Reject abort 源单（[`abort_source`] 已 Cancelled 即收敛）。返回
/// (rework 清单, 是否 abort) 供 committed 线留档与重放臂复用。
fn settle_decision_branches(
    state: &StateDir,
    source: &mut WorkOrder,
    decision: Decision,
    next_orders: &[String],
) -> Result<(Vec<String>, bool), AdjudicateError> {
    match decision {
        // Approve / Arbitrate：原单继续，receipt 留档即留痕。
        Decision::Approve | Decision::Arbitrate => Ok((Vec::new(), false)),
        // Revise：按被签计划面物化 rework 后继单（Created 态 = 待修订）。
        Decision::Revise => {
            let mut rework_order_ids = Vec::new();
            for rework_id in next_orders {
                spawn_rework_order(state, source, rework_id)?;
                rework_order_ids.push(rework_id.clone());
            }
            Ok((rework_order_ids, false))
        }
        // Reject：abort——源单 cancel + 账本落 cancel 行。
        Decision::Reject => {
            abort_source(state, source)?;
            Ok((Vec::new(), true))
        }
    }
}

/// 本源单下一笔单调 decision_seq：已受理最大 seq + 1（无受理 = 1）。
/// 与 admit 幂等窗同源同读（[`load_receipts`]），不另造第二套回执读取。
fn next_decision_seq(state: &StateDir, order_id: &str) -> Result<u64, AdjudicateError> {
    let receipts = load_receipts(state, order_id)?;
    Ok(receipts
        .iter()
        .map(|receipt| receipt.decision_seq)
        .max()
        .unwrap_or(0)
        + 1)
}

/// rework 物化：按计划面声明的 id 生成后继工单（supersedes=源单，图版本随
/// 源单，SLA 走骨架缺省）并在新订单目录落 create_order 账本行（Created 态
/// = 待修订）。既有件接线：af-workorder `WorkOrder::successor` +
/// af-execline `OrderLedger`。
///
/// 幂等（风险#5）：同号已落账**且确系本源单的后继单** = 崩溃窗重放的已结算
/// 面，跳过不重复物化；同号异源（supersedes 对不上）= 真冲突，拒绝覆盖。
fn spawn_rework_order(
    state: &StateDir,
    source: &WorkOrder,
    rework_id: &str,
) -> Result<(), AdjudicateError> {
    crate::spec::validate_order_id(rework_id)
        .map_err(|err| AdjudicateError::Queue(format!("rework 单号不合法 {rework_id:?}: {err}")))?;
    let order_dir = state.order_dir(rework_id);
    if order_dir.join(LEDGER_FILE).is_file() {
        let records =
            af_execline::OrderLedger::load(&order_dir.join(LEDGER_FILE)).map_err(|err| {
                AdjudicateError::Queue(format!(
                    "rework 单 {rework_id} 已落账但账本不可读（对不上后继关系）: {err}"
                ))
            })?;
        let existing = records
            .iter()
            .rev()
            .find_map(|record| record.order_after.clone());
        if existing
            .as_ref()
            .is_some_and(|order| order.supersedes() == Some(source.id()))
        {
            return Ok(());
        }
        return Err(AdjudicateError::Queue(format!(
            "rework 单 {rework_id} 已落账且非本源单（{}）后继，拒绝覆盖",
            source.id()
        )));
    }
    let mut rework = WorkOrder::successor(
        rework_id,
        source.graph_version().clone(),
        Sla::new(None, Priority::Normal),
        source.id(),
    );
    rework
        .attach_budget_voucher(af_workorder::BudgetVoucher {
            voucher_id: format!("bv-{rework_id}"),
            // rework 面沿用源单预算额度（待修订工单同预算顶；签发留给该单
            // 自己的 submit 面，此处只做计划面物化登记）。
            amount: source.budget_voucher().map(|v| v.amount).unwrap_or(0),
            // 未签发：Created 态登记，release 门控会如实报缺签名凭证。
            signed: false,
        })
        .map_err(|err| AdjudicateError::Queue(format!("rework 凭证登记失败: {err}")))?;
    let mut ledger = af_execline::OrderLedger::open(&order_dir.join(LEDGER_FILE))
        .map_err(|err| AdjudicateError::Queue(format!("rework 账本打开失败: {err}")))?;
    let record = LedgerRecord {
        seq: 0, // OrderLedger::append 落地时重编
        ts_ms: Utc::now().timestamp_millis(),
        order_id: rework.id().to_string(),
        stable_id: rework.stable_id().as_str().to_string(),
        op: "create_order".into(),
        idem_key: format!("{}/create_order", rework.stable_id()),
        ok: true,
        state_after: Some("Created".into()),
        error_kind: None,
        error_detail: None,
        order_after: Some(rework),
    };
    ledger
        .append(record)
        .map_err(|err| AdjudicateError::Queue(format!("rework 账本落盘失败: {err}")))?;
    Ok(())
}

/// abort 物化：源单 cancel（非终态）+ 账本落 cancel 行。终态（Closed）没有
/// abort 边——如实报 [`AdjudicateError::MissingEdge`]，不假装迁移。
///
/// 幂等（风险#5）：已 Cancelled = abort 已生效（崩溃窗重放直接收敛为成功），
/// 不再报错——Cancelled 只有 abort 这一条进路，重复 abort 语义上无歧义。
fn abort_source(state: &StateDir, source: &mut WorkOrder) -> Result<(), AdjudicateError> {
    let order_id = source.id().to_string();
    if source.state() == af_workorder::OrderState::Cancelled {
        return Ok(());
    }
    if source.state().is_terminal() {
        return Err(AdjudicateError::MissingEdge(format!(
            "源单 {order_id} 已终态（{}），状态机无 abort 边；拒绝结论以 receipt 留档，不改单",
            serde_json::to_value(source.state())
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| format!("{:?}", source.state())),
        )));
    }
    source.cancel().map_err(|err| {
        AdjudicateError::MissingEdge(format!("源单 {order_id} cancel 被拒: {err}"))
    })?;
    let ledger_path = state.order_dir(&order_id).join(LEDGER_FILE);
    let mut ledger = af_execline::OrderLedger::open(&ledger_path)
        .map_err(|err| AdjudicateError::Queue(format!("源单账本打开失败: {err}")))?;
    let record = LedgerRecord {
        seq: 0,
        ts_ms: Utc::now().timestamp_millis(),
        order_id: order_id.clone(),
        stable_id: source.stable_id().as_str().to_string(),
        op: "cancel".into(),
        idem_key: format!("{}/cancel", source.stable_id()),
        ok: true,
        state_after: Some("Cancelled".into()),
        error_kind: None,
        error_detail: None,
        order_after: Some(source.clone()),
    };
    ledger
        .append(record)
        .map_err(|err| AdjudicateError::Queue(format!("源单账本落盘失败: {err}")))?;
    Ok(())
}

/// stable_id（队列幂等键组成面用；公开给测试与编排对拍）。
pub fn stable_id_of(order_id: &str) -> StableId {
    StableId::generate(order_id)
}

// ============================================================
// reconcile：意图账残留扫描与无人值守收敛（风险#5 ④）
// ============================================================

/// 一份意图账经 [`reconcile_adjudications`] 处理后的收敛判读。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReconcileStatus {
    /// 无在飞意图（空账 / completed 线已落）——无需动作。
    Converged,
    /// receipt 在账、任务已 done——本轮补 completed 线收口（complete 落了、
    /// 线没落的崩溃窗）。
    ClosedByReconcile,
    /// receipt 在账、分支已结算，任务 queued——下一次 steward 认领由重放
    /// 幂等臂收敛。**不代认领**：af-station 的 claim 无任务定向，代领可能
    /// 抢到未裁决任务又无释放面（complete 需真受理在账），只会制造新残面。
    AwaitingClaim { task_id: String },
    /// receipt 在账、任务 claimed 且租约未到——在役/僵尸租约；到期回炉后
    /// 按 [`ReconcileStatus::AwaitingClaim`] 路径收敛。
    LeaseHeld {
        task_id: String,
        claimant: String,
        lease_until: i64,
    },
    /// prepared 残留（受理账无对应回执）——admit 未提交或被拒：检出留检。
    /// 无回执即无已提交事实，双写不可能；后续认领按首走重试（同 seq 同
    /// 摘要的重放由受理幂等窗接住，换 seq 换摘要是新裁决轮次，语义如旧）。
    PreparedResidue { detail: String },
}

/// 一份意图账的 reconcile 报告（留证面：谁、什么状态、怎么收敛的）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReconcileReport {
    pub source_order_id: String,
    pub adjudication_id: String,
    pub status: ReconcileStatus,
}

/// 扫描全部裁决单意图账：检出在飞残留并收敛**可无人值守收敛的面**——
/// 分支幂等结算（rework 物化补缺 / abort 补落）、committed 线补缺、
/// completed 线收口（任务已 done 时）。任务面的最终 complete 由重放幂等臂
/// 在下一次认领时落（见 [`ReconcileStatus::AwaitingClaim`] 的不代领理由）。
///
/// 幂等：对已收敛（completed 线在）的账是 no-op；重复扫描不产生新写入。
pub fn reconcile_adjudications(
    state: &StateDir,
    station: &Station,
) -> Result<Vec<ReconcileReport>, AdjudicateError> {
    let mut reports = Vec::new();
    let root = state.root().join(ADJUDICATIONS_DIR);
    if !root.is_dir() {
        return Ok(reports);
    }
    let mut order_dirs: Vec<PathBuf> = fs::read_dir(&root)
        .map_err(|err| AdjudicateError::Doc(format!("扫描 {}: {err}", root.display())))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    order_dirs.sort();
    for order_dir in order_dirs {
        let source_order_id = order_dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut journals: Vec<PathBuf> = fs::read_dir(&order_dir)
            .map_err(|err| AdjudicateError::Doc(format!("扫描 {}: {err}", order_dir.display())))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().ends_with(JOURNAL_SUFFIX))
            })
            .collect();
        journals.sort();
        for journal_path in journals {
            let Some(adjudication_id) = journal_path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(JOURNAL_SUFFIX))
            else {
                continue; // 排序过滤已保证可剥离；防御而已
            };
            reports.push(reconcile_one(
                state,
                station,
                &source_order_id,
                adjudication_id,
            )?);
        }
    }
    Ok(reports)
}

/// 单份意图账的 reconcile（判读 + 收敛动作）。
fn reconcile_one(
    state: &StateDir,
    station: &Station,
    source_order_id: &str,
    adjudication_id: &str,
) -> Result<ReconcileReport, AdjudicateError> {
    let mut journal = AdjudicationJournal::open(state, source_order_id, adjudication_id)?;
    let report = |status| ReconcileReport {
        source_order_id: source_order_id.to_string(),
        adjudication_id: adjudication_id.to_string(),
        status,
    };
    let Some(prepared) = journal.latest_prepared().cloned() else {
        return Ok(report(ReconcileStatus::Converged)); // 空账：无在飞意图
    };
    if journal.is_completed() {
        return Ok(report(ReconcileStatus::Converged));
    }
    // 提交点判读：以受理账为准（committed/completed 线在而回执缺席 =
    // 账面漂移，fail-loud——与 committed_receipt 同一纪律）。
    let receipts = crate::admit::load_receipts(state, &prepared.source_order_id)?;
    let Some(receipt) = receipts
        .into_iter()
        .find(|receipt| receipt.idempotency_key == prepared.receipt_idempotency_key)
    else {
        return Ok(report(ReconcileStatus::PreparedResidue {
            detail: format!(
                "prepared 意图在账（seq={} 回执键 {}）、受理回执缺席——admit 未提交或被拒；\
                 无回执即无已提交事实，留检待后续认领按首走重试",
                prepared.decision_seq, prepared.receipt_idempotency_key
            ),
        }));
    };
    // receipt 在账：跨库提交点已过——结算分支（幂等）+ committed 线补缺。
    if journal.latest_committed().is_none() {
        let (mut source, _records) =
            crate::admit::recover_source_order(state, &prepared.source_order_id)?;
        let (rework_order_ids, aborted) =
            settle_decision_branches(state, &mut source, prepared.decision, &prepared.next_orders)?;
        journal.append_committed(CommittedFact {
            receipt_idempotency_key: receipt.idempotency_key.clone(),
            rework_order_ids,
            aborted,
            ts_ms: Utc::now().timestamp_millis(),
        })?;
    }
    match station.get(&prepared.task_id)? {
        None => Ok(report(ReconcileStatus::PreparedResidue {
            detail: format!(
                "receipt 在账但队列任务 {} 缺位（lease 库与意图账漂移）——留检",
                prepared.task_id
            ),
        })),
        Some(task) if task.state == af_station::State::Done => {
            journal.append_completed()?;
            Ok(report(ReconcileStatus::ClosedByReconcile))
        }
        Some(task) if task.state == af_station::State::Queued => {
            Ok(report(ReconcileStatus::AwaitingClaim {
                task_id: prepared.task_id.clone(),
            }))
        }
        Some(task) => Ok(report(ReconcileStatus::LeaseHeld {
            task_id: prepared.task_id.clone(),
            claimant: task.claimant.clone().unwrap_or_default(),
            lease_until: task.lease_until.unwrap_or(0),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_workorder::{GraphVersion, LineBinding};

    const DECISION_EXPIRY: &str = "2026-12-01T00:00:00Z";

    fn time(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc)
    }

    /// 落一张已 Closed 源单（带合格终态工件 + 绑定预算凭证）。
    fn seed_closed_order(state: &StateDir, order_id: &str) -> WorkOrder {
        let mut order = WorkOrder::new(
            order_id,
            GraphVersion::new("af-order/poc-1"),
            Sla::new(None, Priority::Normal),
        );
        order
            .bind_line(LineBinding {
                line_id: "line-poc".into(),
                line_version: "lv-1".into(),
            })
            .unwrap();
        order
            .attach_budget_voucher(af_workorder::BudgetVoucher {
                voucher_id: format!("bv-{order_id}"),
                amount: 100,
                signed: true,
            })
            .unwrap();
        order.release().unwrap();
        order.start().unwrap();
        let artifact_bytes = format!("{{\"artifact\":\"{order_id}\"}}\n");
        order
            .attach_artifact(af_workorder::WorkArtifact::from_bytes(
                "fa-1",
                true,
                true,
                artifact_bytes.as_bytes(),
            ))
            .unwrap();
        order.complete().unwrap();
        order.close().unwrap();
        let dir = state.order_dir(order_id);
        fs::create_dir_all(&dir).unwrap();
        let mut ledger = af_execline::OrderLedger::open(&dir.join(LEDGER_FILE)).unwrap();
        ledger
            .append(LedgerRecord {
                seq: 0,
                ts_ms: 1_767_225_600_000,
                order_id: order_id.to_string(),
                stable_id: order.stable_id().as_str().to_string(),
                op: "close".into(),
                idem_key: format!("{}/close", order.stable_id()),
                ok: true,
                state_after: Some("Closed".into()),
                error_kind: None,
                error_detail: None,
                order_after: Some(order.clone()),
            })
            .unwrap();
        // 工件字节落盘（artifact 条目重算的实际字节面）。
        let artifacts_dir = dir.join("artifacts");
        fs::create_dir_all(&artifacts_dir).unwrap();
        fs::write(artifacts_dir.join("fa-1.json"), &artifact_bytes).unwrap();
        order
    }

    fn scratch(tag: &str) -> StateDir {
        let dir = std::env::temp_dir().join(format!(
            "af-order-adjudicate-{}-{tag}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        StateDir::new(dir)
    }

    /// ② 生成面：doc_kind/template_id 固定；内容含 stable_id/事项/basis/
    /// next_orders/expires_at；可序列化可复算（落盘读回 verify 过）。
    #[test]
    fn adjudication_doc_is_serializable_and_recomputable() {
        let state = scratch("doc");
        seed_closed_order(&state, "wo-doc");
        let (doc, order) = generate_adjudication_doc(
            &state,
            "wo-doc",
            vec!["wo-doc-rw".into()],
            time(DECISION_EXPIRY),
        )
        .unwrap();
        assert_eq!(doc.doc_kind, "裁决单");
        assert_eq!(doc.template_id, "tpl-adjudication-v1");
        assert_eq!(doc.source_stable_id, order.stable_id().as_str());
        assert!(doc.subject.contains("wo-doc"), "{}", doc.subject);
        assert_eq!(doc.adjudication_id, "adj-wo-doc-1");
        assert_eq!(doc.next_orders, vec!["wo-doc-rw".to_string()]);
        // basis 每条都是受理侧文法可解析的真实摘要（与源单实际实体对得上）。
        assert_eq!(doc.basis.len(), 2, "一个工件 + 一个凭证");
        for entry in &doc.basis {
            let segments: Vec<&str> = entry.split(':').collect();
            assert_eq!(segments.len(), 3, "{entry}");
            assert_eq!(segments[2].len(), 64, "{entry}");
        }
        let artifact = &order.artifacts()[0];
        assert!(
            doc.basis
                .iter()
                .any(|entry| entry == &format!("artifact:fa-1:{}", artifact.content_sha256())),
            "basis 应含工件真实摘要: {:?}",
            doc.basis
        );
        // 可序列化 + 可复算：落盘 → 读回 → verify 过；掉包一字节即红。
        let path = doc.persist(&state).unwrap();
        let reloaded = load_doc(&path).unwrap();
        assert_eq!(reloaded, doc);
        assert_eq!(
            reloaded.recompute_sha256().unwrap(),
            doc.doc_sha256,
            "复算口径与生成口径同源"
        );
        let mut tampered = reloaded.clone();
        tampered.subject.push_str("（被掉包）");
        assert!(matches!(
            verify_doc(&tampered),
            Err(AdjudicateError::DocVerify(_))
        ));
        // 下一份单号递增。
        let (second, _) =
            generate_adjudication_doc(&state, "wo-doc", vec![], time(DECISION_EXPIRY)).unwrap();
        assert_eq!(second.adjudication_id, "adj-wo-doc-2");
        let _ = fs::remove_dir_all(state.root());
    }

    /// ② 负例：空依据源单（无工件未绑凭证）拒生成；计划面含空白条目拒。
    #[test]
    fn adjudication_doc_generation_rejects_empty_basis_and_bad_plan() {
        let state = scratch("docneg");
        let dir = state.order_dir("wo-bare");
        fs::create_dir_all(&dir).unwrap();
        let bare = WorkOrder::new(
            "wo-bare",
            GraphVersion::new("af-order/poc-1"),
            Sla::new(None, Priority::Normal),
        );
        let mut ledger = af_execline::OrderLedger::open(&dir.join(LEDGER_FILE)).unwrap();
        ledger
            .append(LedgerRecord {
                seq: 0,
                ts_ms: 1_767_225_600_000,
                order_id: "wo-bare".into(),
                stable_id: bare.stable_id().as_str().to_string(),
                op: "create_order".into(),
                idem_key: format!("{}/create_order", bare.stable_id()),
                ok: true,
                state_after: Some("Created".into()),
                error_kind: None,
                error_detail: None,
                order_after: Some(bare),
            })
            .unwrap();
        let err = generate_adjudication_doc(&state, "wo-bare", vec![], time(DECISION_EXPIRY))
            .unwrap_err();
        assert!(matches!(err, AdjudicateError::Doc(_)), "{err}");
        let err =
            generate_adjudication_doc(&state, "wo-does-not-exist", vec![], time(DECISION_EXPIRY))
                .unwrap_err();
        assert!(matches!(err, AdjudicateError::Order(_)), "{err}");
        let _ = fs::remove_dir_all(state.root());
    }

    /// 意图账三相追加/重载/相位判读 roundtrip（风险#5）。
    #[test]
    fn journal_phases_roundtrip_and_latest_readers() {
        let state = scratch("journal");
        let mut journal = AdjudicationJournal::open(&state, "wo-j", "adj-wo-j-1").unwrap();
        assert!(journal.latest_prepared().is_none(), "空账无在飞意图");
        assert!(!journal.is_completed());
        let intent = PreparedIntent {
            adjudication_id: "adj-wo-j-1".into(),
            task_id: "adj-wo-j-1".into(),
            source_order_id: "wo-j".into(),
            source_stable_id: "sid-wo-j".into(),
            decision: Decision::Approve,
            decision_seq: 1,
            decision_digest: "d1".into(),
            receipt_idempotency_key: "sid-wo-j/admit/1/d1".into(),
            next_orders: vec![],
            ts_ms: 1,
        };
        journal.append_prepared(intent.clone()).unwrap();
        journal
            .append_committed(CommittedFact {
                receipt_idempotency_key: "sid-wo-j/admit/1/d1".into(),
                rework_order_ids: vec![],
                aborted: false,
                ts_ms: 2,
            })
            .unwrap();
        journal.append_completed().unwrap();
        // 重载后判读一致（fsync 落盘语义由 OrderLedger/append_receipt 同款
        // 纪律保证，此处验相位面）。
        let reloaded = AdjudicationJournal::open(&state, "wo-j", "adj-wo-j-1").unwrap();
        assert_eq!(reloaded.latest_prepared(), Some(&intent));
        assert_eq!(reloaded.latest_committed().map(|f| f.aborted), Some(false));
        assert!(reloaded.is_completed());
        assert_eq!(reloaded.lines().len(), 3, "三相各一行");
        // 重试可产生第二条 prepared（意图在账，历史不抹）：最新意图取后者。
        let mut reloaded = reloaded;
        let second = PreparedIntent {
            decision_seq: 2,
            ..intent.clone()
        };
        reloaded.append_prepared(second.clone()).unwrap();
        assert_eq!(reloaded.latest_prepared(), Some(&second));
        let _ = fs::remove_dir_all(state.root());
    }

    /// 意图账坏行 fail-loud（不静默跳过）。
    #[test]
    fn journal_bad_line_fails_loud() {
        let state = scratch("journal-bad");
        let dir = state.root().join(ADJUDICATIONS_DIR).join("wo-jb");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(format!("adj-wo-jb-1{JOURNAL_SUFFIX}")),
            "not-json\n",
        )
        .unwrap();
        let err = AdjudicationJournal::open(&state, "wo-jb", "adj-wo-jb-1").unwrap_err();
        assert!(matches!(err, AdjudicateError::Doc(_)), "{err}");
        let _ = fs::remove_dir_all(state.root());
    }

    /// rework 物化幂等（风险#5）：同号同源已落账 → 跳过；同号异源 → 拒绝。
    #[test]
    fn spawn_rework_order_idempotent_and_rejects_foreign_ledger() {
        let state = scratch("rework-idem");
        let source = seed_closed_order(&state, "wo-rw-idem");
        let rework_id = "wo-rw-idem-rw";
        spawn_rework_order(&state, &source, rework_id).unwrap();
        // 崩溃窗重放：同号已物化且确系本源单后继 → 跳过（不报错、不双落）。
        spawn_rework_order(&state, &source, rework_id).unwrap();
        let records =
            af_execline::OrderLedger::load(&state.order_dir(rework_id).join(LEDGER_FILE)).unwrap();
        assert_eq!(
            records.iter().filter(|r| r.op == "create_order").count(),
            1,
            "重复物化不得双落账"
        );
        // 同号异源（后继关系对不上）→ 真冲突拒绝。
        let err = spawn_rework_order(&state, &seed_closed_order(&state, "wo-other"), rework_id)
            .unwrap_err();
        assert!(matches!(err, AdjudicateError::Queue(_)), "{err}");
        let _ = fs::remove_dir_all(state.root());
    }

    /// abort 幂等（风险#5）：已 Cancelled 源单重复 abort 直接收敛成功。
    #[test]
    fn abort_source_is_idempotent_on_cancelled() {
        let state = scratch("abort-idem");
        // 落一张已 release（非终态，有 abort 边）的源单。
        let mut source = WorkOrder::new(
            "wo-ab-idem",
            af_workorder::GraphVersion::new("af-order/poc-1"),
            Sla::new(None, Priority::Normal),
        );
        source
            .bind_line(af_workorder::LineBinding {
                line_id: "line-poc".into(),
                line_version: "lv-1".into(),
            })
            .unwrap();
        source
            .attach_budget_voucher(af_workorder::BudgetVoucher {
                voucher_id: "bv-wo-ab-idem".into(),
                amount: 100,
                signed: true,
            })
            .unwrap();
        source.release().unwrap();
        let dir = state.order_dir("wo-ab-idem");
        fs::create_dir_all(&dir).unwrap();
        af_execline::OrderLedger::open(&dir.join(LEDGER_FILE))
            .unwrap()
            .append(LedgerRecord {
                seq: 0,
                ts_ms: 1_767_225_600_000,
                order_id: "wo-ab-idem".into(),
                stable_id: source.stable_id().as_str().to_string(),
                op: "release".into(),
                idem_key: format!("{}/release", source.stable_id()),
                ok: true,
                state_after: Some("Released".into()),
                error_kind: None,
                error_detail: None,
                order_after: Some(source.clone()),
            })
            .unwrap();
        abort_source(&state, &mut source).unwrap();
        assert_eq!(source.state(), af_workorder::OrderState::Cancelled);
        // 崩溃窗重放：已 Cancelled → 收敛成功（不再 MissingEdge）。
        abort_source(&state, &mut source).unwrap();
        let records =
            af_execline::OrderLedger::load(&state.order_dir("wo-ab-idem").join(LEDGER_FILE))
                .unwrap();
        assert_eq!(
            records.iter().filter(|r| r.op == "cancel").count(),
            1,
            "重复 abort 不得双落 cancel 行"
        );
        let _ = fs::remove_dir_all(state.root());
    }

    /// ③ 队列接线面：幂等键 = stable_id + 裁决单 id，重复入队 Duplicate；
    /// 非 steward 角色认领不到；认领后任务可被现任 complete。
    #[test]
    fn queue_wiring_idempotent_enqueue_and_role_routing() {
        let state = scratch("queue");
        let order = seed_closed_order(&state, "wo-queue");
        let (doc, _) =
            generate_adjudication_doc(&state, "wo-queue", vec![], time(DECISION_EXPIRY)).unwrap();
        let doc_path = doc.persist(&state).unwrap();
        let station = Station::open_in_memory().unwrap().into_station();
        let key = format!("{}/{}", order.stable_id(), doc.adjudication_id);
        let first = station
            .enqueue(NewTask {
                id: &doc.adjudication_id,
                payload_ref: doc_path.to_string_lossy().as_ref(),
                required_role: STEWARD_ROLE,
                idempotency_key: Some(&key),
                deadline: None,
            })
            .unwrap();
        assert!(matches!(first, Enqueued::Inserted { .. }));
        // 同幂等键重入队（事件重投形态）：Duplicate，不产生第二行。
        let second = station
            .enqueue(NewTask {
                id: "adj-wo-queue-1-dup",
                payload_ref: "whatever",
                required_role: STEWARD_ROLE,
                idempotency_key: Some(&key),
                deadline: None,
            })
            .unwrap();
        assert_eq!(
            second,
            Enqueued::Duplicate {
                existing_id: doc.adjudication_id.clone()
            }
        );
        // 非 steward 角色认领不到。
        assert!(station.claim("w-other", "coder", 60_000).unwrap().is_none());
        // steward 认领后现任可 complete；非现任不可。
        let task = station
            .claim("steward-1", STEWARD_ROLE, 60_000)
            .unwrap()
            .unwrap();
        assert_eq!(task.id, doc.adjudication_id);
        assert!(!station.complete(&task.id, "someone-else").unwrap());
        assert!(station.complete(&task.id, "steward-1").unwrap());
        let _ = fs::remove_dir_all(state.root());
    }
}
