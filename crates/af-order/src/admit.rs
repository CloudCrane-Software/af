//! admit —— 裁决受理：把一份已签 DecisionArtifact 按固定校验链接收入单。
//!
//! 公共 API：[`admit`]（`AdmissionRequest` → `AdmissionReceipt` | [`OrderError`]）+
//! CLI 子命令 `af-order admit --request <admission.json> --state <dir>`（见
//! [`crate::cli`]）。PoC-A2 骨架；决策校验链细则按任务分工留给 PoC-C（各占位
//! 处写明交接注释与保留错误码）。
//!
//! # 固定校验顺序（钉死，不可重排）
//!
//! 1. **源单恢复与终态校验**：从 `<state>/orders/<id>/ledger.jsonl` 恢复源单
//!    最近快照；查无账本/无快照 = OR2101；源单已 Cancelled（abort 封存，
//!    准入不再有意义）= OR2102。Closed 单受理——闭环单的裁决留档正是本链
//!    的用途之一（receipt.source_state 如实记录受理时状态）。
//! 2. **模板/资源画像/门清单 schema 校验**（OR2103/OR2104/OR2105）：三类配方
//!    文书只验线格式 schema（必填非空、类型、deny_unknown_fields、门 id 不重复），
//!    语义核对（模板与决策种类匹配、资源实际可用、门单两查）留 PoC-C。
//! 3. **DecisionArtifact 四层校验**（OR2106）：必经真 af-engine 子进程
//!    `admit_decision` op——payload 绑定 stable_id+graph_version
//!    （`WorkOrder::admission_decision_payload`）+ hash→角色上限→决策权限→
//!    注册表真 ed25519 验签。门面不在进程内复刻校验。
//! 4. **seq/expiry/basis/digest 复核**（OR2107/…/OR2110）：幂等窗查账
//!    （同键同摘要 → 回执原 receipt；同 seq 异摘要 → OR2107）+ 决策有效期
//!    （validated_at ≥ expires_at → OR2108）；basis 逐项重算（OR2109）与
//!    next_orders_digest 重算（OR2110）为 PoC-C 占位（接口位+保留码）。
//! 5. **凭证求交**（OR2111，PoC-C 占位）：actor 凭证与订单凭证面求交的接口位。
//! 6. **全过后才写状态**：把 receipt 追加落盘
//!    `<state>/orders/<id>/admissions.jsonl`（OR2112）。任一步失败 = 类型化
//!    拒绝且**零外部效应**（不落 receipt、不动账本、不起别的进程）。
//!
//! # Receipt 幂等键
//!
//! `{source_stable_id}/admit/{decision_seq}/{decision_digest}`（digest = 工件
//! canonical 字节的 SHA-256，见 [`decision_digest`]）。同键同摘要重放返回
//! 原 receipt（`replayed=true`，零写入）；同 seq 异摘要 = OR2107 拒绝
//! （seq 单调性细则 = PoC-C，见 [`AdmitReject::SeqConflict`] 交接注释）。

use std::fs;

use af_execline::{EngineClient, EngineOutcome, LedgerRecord, OrderLedger};
use af_workorder::{sha256_hex, DecisionArtifact, OrderState, TrustedActorRegistry, WorkOrder};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::OrderError;
use crate::state::{StateDir, LEDGER_FILE};

/// 准入受理记录文件名（订单目录内，append-only）。
pub const ADMISSIONS_FILE: &str = "admissions.jsonl";

/// admit 校验链通过的检查点名（receipt.checks 线格式，顺序即链序）。
pub const CHECK_SOURCE_RECOVERY: &str = "source_recovery";
pub const CHECK_TERMINAL_STATE: &str = "terminal_state";
pub const CHECK_TEMPLATE_SCHEMA: &str = "template_schema";
pub const CHECK_RESOURCE_PROFILE_SCHEMA: &str = "resource_profile_schema";
pub const CHECK_GATE_LIST_SCHEMA: &str = "gate_list_schema";
pub const CHECK_DECISION_FOUR_LAYER: &str = "decision_four_layer";
pub const CHECK_SEQ_WINDOW: &str = "seq_window";
pub const CHECK_EXPIRY: &str = "expiry";
pub const CHECK_BASIS_RECHECK: &str = "basis_recheck";
pub const CHECK_DIGEST_RECHECK: &str = "digest_recheck";
pub const CHECK_CREDENTIAL_INTERSECTION: &str = "credential_intersection";

// ---------- 请求 ----------

/// 文书模板（裁决单面配方文书）线格式。schema 只验形状；语义匹配留 PoC-C。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdjudicationTemplate {
    pub template_id: String,
    pub template_version: String,
    /// 文书种类：任务单 | 门单 | 裁决单（T1 终裁文书模板最小集）。
    pub doc_kind: String,
    /// 模板正文摘要（配方层留档，admit 只核存在与形状；正文核对留 PoC-C）。
    pub body_sha256: String,
}

impl AdjudicationTemplate {
    /// schema 校验（第②步前半）：非空、doc_kind 在最小集内、body_sha256 为
    /// 64 位小写 SHA-256 hex。语义（模板与本次决策种类是否匹配）留 PoC-C。
    fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("template_id", &self.template_id),
            ("template_version", &self.template_version),
            ("doc_kind", &self.doc_kind),
        ] {
            if value.trim().is_empty() {
                return Err(format!("{field} 为空或含空白"));
            }
        }
        if !matches!(self.doc_kind.as_str(), "任务单" | "门单" | "裁决单") {
            return Err(format!(
                "doc_kind={} 不在文书模板最小集（任务单|门单|裁决单）",
                self.doc_kind
            ));
        }
        if !is_sha256_hex(&self.body_sha256) {
            return Err("body_sha256 必须是 64 位小写 SHA-256 hex".into());
        }
        Ok(())
    }
}

/// 资源画像（静态声明半；动态读数在运行时账本，不进配方——T4 终裁）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceProfileSpec {
    pub profile_id: String,
    pub profile_version: String,
    /// 静态声明清单（声明余量等语义核对留 PoC-C）。
    pub claims: Vec<ResourceClaim>,
}

/// 一条资源声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceClaim {
    pub resource: String,
    pub units: u64,
}

impl ResourceProfileSpec {
    /// schema 校验（第②步中段）：id/version 非空；声明 resource 非空、
    /// units > 0。实际可用性（动态读数联防）留 PoC-C/运行时账本。
    fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("profile_id", &self.profile_id),
            ("profile_version", &self.profile_version),
        ] {
            if value.trim().is_empty() {
                return Err(format!("{field} 为空或含空白"));
            }
        }
        if self.claims.is_empty() {
            return Err("claims 为空：资源画像至少声明一项".into());
        }
        for (index, claim) in self.claims.iter().enumerate() {
            if claim.resource.trim().is_empty() {
                return Err(format!("claims[{index}].resource 为空或含空白"));
            }
            if claim.units == 0 {
                return Err(format!("claims[{index}].units 必须 > 0"));
            }
        }
        Ok(())
    }
}

/// 门清单（F 门单两查所在；首版只上 schema，两查语义=PoC-F/C 接口位）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateListSpec {
    pub gates: Vec<GateSpec>,
}

/// 一道门的声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateSpec {
    pub gate_id: String,
    /// 门种类（如 F1/W1/G10；种类语义表在配方层，admit 只核非空）。
    pub kind: String,
    pub required: bool,
}

impl GateListSpec {
    /// schema 校验（第②步后半）：gate_id 非空且不重复；kind 非空。
    /// 门单两查（RequiredInputMissing/InputGateNotGreen）是 PoC-F 职责，
    /// 此处只把清单形状钉死。
    fn validate(&self) -> Result<(), String> {
        if self.gates.is_empty() {
            return Err("gates 为空：门清单至少一道门".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for (index, gate) in self.gates.iter().enumerate() {
            if gate.gate_id.trim().is_empty() {
                return Err(format!("gates[{index}].gate_id 为空或含空白"));
            }
            if !seen.insert(gate.gate_id.clone()) {
                return Err(format!("gates[{index}].gate_id 重复: {}", gate.gate_id));
            }
            if gate.kind.trim().is_empty() {
                return Err(format!("gates[{index}].kind 为空或含空白"));
            }
        }
        Ok(())
    }
}

/// 准入请求（admission.json 线格式，`deny_unknown_fields`）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionRequest {
    /// 源单订单号（同时是 state/orders/ 目录名，过路径安全门）。
    pub order_id: String,
    /// 被准入的决策工件（v2 真签形态；四层校验在第③步）。
    pub decision: DecisionArtifact,
    /// 受信 actor 注册表（编排从受信凭证存储装载；工件只带 key_id）。
    pub actor_registry: TrustedActorRegistry,
    /// 文书模板（第②步 schema）。
    pub template: AdjudicationTemplate,
    /// 资源画像（第②步 schema）。
    pub resource_profile: ResourceProfileSpec,
    /// 门清单（第②步 schema）。
    pub gate_list: GateListSpec,
    /// 验签/受理时刻（RFC3339；缺省 = 系统钟）。调用方注入，不读工件自带时间。
    #[serde(default)]
    pub validated_at: Option<DateTime<Utc>>,
}

/// 请求解析错误（Display 即原因；CLI 映射 OR2007）。
#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error("不是合法 JSON: {0}")]
    Malformed(String),
    #[error("缺少必填字段: {0}")]
    MissingField(String),
    #[error("不认识的字段: {0}")]
    UnknownFields(String),
    #[error("字段 {field} 不合法: {reason}")]
    Invalid { field: &'static str, reason: String },
}

/// 请求认识的顶层字段（字典序，供未知字段名单对拍）。
const REQUEST_FIELDS: &[&str] = &[
    "actor_registry",
    "decision",
    "gate_list",
    "order_id",
    "resource_profile",
    "template",
    "validated_at",
];

/// 解析 admission 请求文本：JSON → 未知字段 → 必填 → 类型/语义预检 → 反序列化。
pub fn parse_request(text: &str) -> Result<AdmissionRequest, RequestError> {
    let value: Value =
        serde_json::from_str(text).map_err(|err| RequestError::Malformed(err.to_string()))?;
    let obj = value
        .as_object()
        .ok_or_else(|| RequestError::Malformed("admission request 必须是 JSON 对象".into()))?;
    let mut unknown: Vec<&str> = obj
        .keys()
        .map(String::as_str)
        .filter(|key| !REQUEST_FIELDS.contains(key))
        .collect();
    if !unknown.is_empty() {
        unknown.sort_unstable();
        return Err(RequestError::UnknownFields(unknown.join(", ")));
    }
    for field in [
        "order_id",
        "decision",
        "actor_registry",
        "template",
        "resource_profile",
        "gate_list",
    ] {
        if !obj.contains_key(field) {
            return Err(RequestError::MissingField(field.to_string()));
        }
    }
    match obj.get("order_id") {
        Some(Value::String(_)) => {}
        Some(other) => {
            return Err(RequestError::Invalid {
                field: "order_id",
                reason: format!("必须是字符串，实际是 {other}"),
            })
        }
        None => unreachable!("必填已点名"),
    }
    if let Some(raw) = obj.get("validated_at") {
        let text = raw.as_str().ok_or_else(|| RequestError::Invalid {
            field: "validated_at",
            reason: format!("必须是 RFC3339 字符串，实际是 {raw}"),
        })?;
        DateTime::parse_from_rfc3339(text).map_err(|err| RequestError::Invalid {
            field: "validated_at",
            reason: format!("不是合法 RFC3339: {err}"),
        })?;
    }
    let request: AdmissionRequest =
        serde_json::from_value(value).map_err(|err| RequestError::Malformed(err.to_string()))?;
    crate::spec::validate_order_id(&request.order_id).map_err(|err| RequestError::Invalid {
        field: "order_id",
        reason: err.to_string(),
    })?;
    Ok(request)
}

// ---------- Receipt ----------

/// 准入回执（admissions.jsonl 行格式 + CLI stdout 面）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionReceipt {
    /// 幂等键：`{source_stable_id}/admit/{decision_seq}/{decision_digest}`。
    pub idempotency_key: String,
    pub order_id: String,
    pub source_stable_id: String,
    pub decision_seq: u64,
    pub decision_digest: String,
    /// 受理时源单状态（serde 名：Created/…）。
    pub source_state: String,
    pub actor_id: String,
    /// 决策结论（serde 名：Approve/Reject/Revise/Arbitrate）。
    pub decision: String,
    /// 受理时刻（= 本次 validated_at，RFC3339）。
    pub admitted_at: DateTime<Utc>,
    /// 第③步引擎版本（审计面：哪版引擎做的四层校验）。
    pub engine_version: String,
    /// 通过的校验链（顺序固定，见模块文档）。
    pub checks: Vec<String>,
    /// 本回执是否经同键重放返回（盘上落盘的行恒为 false）。
    #[serde(default)]
    pub replayed: bool,
}

/// 决策工件 digest：canonical 序列化字节（serde 字段序，跨进程确定）的 SHA-256。
pub fn decision_digest(decision: &DecisionArtifact) -> String {
    let bytes = serde_json::to_vec(decision).expect("DecisionArtifact 只含可序列化字段");
    sha256_hex(&bytes)
}

/// 幂等键：`{source_stable_id}/admit/{decision_seq}/{decision_digest}`。
pub fn idempotency_key(source_stable_id: &str, decision_seq: u64, decision_digest: &str) -> String {
    format!("{source_stable_id}/admit/{decision_seq}/{decision_digest}")
}

// ---------- 校验链 ----------

/// admit 校验链的类型化拒绝（OR21xx 保留码表；PoC-C 占位变体已钉接口位）。
#[derive(Debug, thiserror::Error)]
pub enum AdmitReject {
    /// OR2101：源单恢复失败（查无账本/账本损坏/全账本无快照）。
    #[error("OR2101: 源单恢复失败: {0}")]
    SourceRecovery(String),
    /// OR2102：源单终态校验失败（Cancelled 封存单不再受理准入；Closed 单
    /// 受理——闭环单裁决留档是本链用途之一）。
    #[error("OR2102: 源单终态校验失败: {0}")]
    TerminalSource(String),
    /// OR2103：文书模板 schema 校验失败。
    #[error("OR2103: 文书模板 schema 校验失败: {0}")]
    TemplateSchema(String),
    /// OR2104：资源画像 schema 校验失败。
    #[error("OR2104: 资源画像 schema 校验失败: {0}")]
    ResourceProfileSchema(String),
    /// OR2105：门清单 schema 校验失败。
    #[error("OR2105: 门清单 schema 校验失败: {0}")]
    GateListSchema(String),
    /// OR2106：DecisionArtifact 四层校验失败（引擎 admit_decision 拒绝；
    /// 含 payload 绑定与真 ed25519 验签各层）。
    #[error("OR2106: DecisionArtifact 四层校验失败: {0}")]
    DecisionFourLayer(String),
    /// OR2107：seq 复核失败——同 seq 异摘要（幂等冲突）。
    ///
    /// 交接 PoC-C：seq 单调性（新准入 seq 须 > 本源单已受理最大 seq）在同一
    /// 查账窗落位——admissions.jsonl 已持久全部 (seq, digest)，单调门只需在此
    /// 变体上扩判断；两个负例（同键重放回执/同 seq 异摘要拒绝）已钉死不可破。
    #[error("OR2107: seq 复核失败（同 seq 异摘要）: {0}")]
    SeqConflict(String),
    /// OR2108：决策已过有效期（validated_at ≥ expires_at，恰在时刻即拒）。
    #[error("OR2108: 决策已过有效期: {0}")]
    Expired(String),
    /// OR2109：basis 逐项重算未过（**PoC-C 落位**：逐条独立重算 basis 条目，
    /// 不信工件自报；接口位与保留码已钉，调用点在链序第④步内不得移动）。
    #[error("OR2109: basis 逐项重算未过: {0}")]
    BasisRecompute(String),
    /// OR2110：next_orders_digest 重算未过（**PoC-C 落位**：对 next_orders
    /// 计划面独立重算 digest 与工件内值比对；接口位与保留码已钉）。
    #[error("OR2110: next_orders_digest 重算未过: {0}")]
    DigestRecompute(String),
    /// OR2111：凭证求交未过（**PoC-C 落位**：actor 凭证 ∩ 订单凭证面，Cedar
    /// 扩面前的注册表+验签先行腿；接口位与保留码已钉）。
    #[error("OR2111: 凭证求交未过: {0}")]
    CredentialIntersection(String),
    /// OR2112：准入状态写入失败（admissions.jsonl 追加/落盘）。
    #[error("OR2112: 准入状态写入失败: {0}")]
    StateWrite(String),
}

/// 准入：固定校验顺序见模块文档。任一步失败 = 类型化拒绝且零写入；
/// 全过后才把 receipt 追加进 `admissions.jsonl`。
///
/// `now` = 本次验签/受理时刻（CLI 由 `--now` 或系统钟注入）。
pub fn admit(
    request: &AdmissionRequest,
    state: &StateDir,
    client: &EngineClient,
    now: DateTime<Utc>,
) -> Result<AdmissionReceipt, OrderError> {
    let mut checks: Vec<String> = Vec::new();

    // ── 第①步：源单恢复与终态校验 ─────────────────────────────
    let order = recover_source_order(state, &request.order_id)?;
    checks.push(CHECK_SOURCE_RECOVERY.into());
    let source_state = order.state();
    if source_state == OrderState::Cancelled {
        return Err(AdmitReject::TerminalSource(format!(
            "源单 {} 已 Cancelled（abort 封存单不再受理准入）",
            request.order_id
        ))
        .into());
    }
    checks.push(CHECK_TERMINAL_STATE.into());

    // ── 幂等窗（第④步 seq 复核的查账快道，先于第②③步：重放不得被
    //    后续校验的时变因素——注册表轮换/决策过期——误伤）────────────
    let digest = decision_digest(&request.decision);
    let idem = idempotency_key(
        order.stable_id().as_str(),
        request.decision.decision_seq,
        &digest,
    );
    let prior = load_receipts(state, &request.order_id)?;
    if let Some(original) = prior.iter().find(|receipt| receipt.idempotency_key == idem) {
        // 同键同摘要重放：返回原 receipt（replayed=true），零写入。
        let mut receipt = original.clone();
        receipt.replayed = true;
        return Ok(receipt);
    }
    if prior
        .iter()
        .any(|receipt| receipt.decision_seq == request.decision.decision_seq)
    {
        // 同 seq 异摘要：幂等冲突，拒（OR2107）。seq 单调性细则 = PoC-C。
        return Err(AdmitReject::SeqConflict(format!(
            "源单 {} 已受理过 seq={} 的决策（异摘要）：重放须同键同摘要，换内容须换 seq",
            request.order_id, request.decision.decision_seq
        ))
        .into());
    }

    // ── 第②步：模板/资源画像/门清单 schema 校验 ────────────────
    request
        .template
        .validate()
        .map_err(|detail| OrderError::from(AdmitReject::TemplateSchema(detail)))?;
    checks.push(CHECK_TEMPLATE_SCHEMA.into());
    request
        .resource_profile
        .validate()
        .map_err(|detail| OrderError::from(AdmitReject::ResourceProfileSchema(detail)))?;
    checks.push(CHECK_RESOURCE_PROFILE_SCHEMA.into());
    request
        .gate_list
        .validate()
        .map_err(|detail| OrderError::from(AdmitReject::GateListSchema(detail)))?;
    checks.push(CHECK_GATE_LIST_SCHEMA.into());

    // ── 第③步：DecisionArtifact 四层校验（真验签，必经引擎子进程）──
    let engine_version = validate_decision_via_engine(client, &order, request, now)?;
    checks.push(CHECK_DECISION_FOUR_LAYER.into());

    // ── 第④步：seq/expiry/basis/digest 复核 ───────────────────
    // seq：幂等窗已处理重放/冲突；单调性细则 = PoC-C（见 SeqConflict 交接注释）。
    checks.push(CHECK_SEQ_WINDOW.into());
    if now >= request.decision.expires_at {
        return Err(AdmitReject::Expired(format!(
            "validated_at={now} ≥ expires_at={}（恰在时刻即拒）",
            request.decision.expires_at
        ))
        .into());
    }
    checks.push(CHECK_EXPIRY.into());
    recheck_basis(&request.decision)?;
    checks.push(CHECK_BASIS_RECHECK.into());
    recheck_next_orders_digest(&request.decision)?;
    checks.push(CHECK_DIGEST_RECHECK.into());

    // ── 第⑤步：凭证求交（PoC-C 占位）──────────────────────────
    intersect_credentials(&request.decision)?;
    checks.push(CHECK_CREDENTIAL_INTERSECTION.into());

    // ── 第⑥步：全过后才写状态 ─────────────────────────────────
    let receipt = AdmissionReceipt {
        idempotency_key: idem,
        order_id: request.order_id.clone(),
        source_stable_id: order.stable_id().as_str().to_string(),
        decision_seq: request.decision.decision_seq,
        decision_digest: digest,
        source_state: serde_json::to_value(source_state)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| format!("{source_state:?}")),
        actor_id: request.decision.actor_id.clone(),
        decision: serde_json::to_value(request.decision.decision)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| format!("{:?}", request.decision.decision)),
        admitted_at: now,
        engine_version,
        checks: checks.clone(),
        replayed: false,
    };
    append_receipt(state, &request.order_id, &receipt)?;
    Ok(receipt)
}

/// 从 state 目录恢复源单最近快照（ledger 最后一条带快照行）。
fn recover_source_order(state: &StateDir, order_id: &str) -> Result<WorkOrder, OrderError> {
    // order_id 过路径安全门（与 submit/show 同门：目录名不得越出 orders/）。
    crate::spec::validate_order_id(order_id).map_err(|err| {
        OrderError::from(AdmitReject::SourceRecovery(format!(
            "order_id 不当作目录名（{err}）"
        )))
    })?;
    let ledger_path = state.order_dir(order_id).join(LEDGER_FILE);
    if !ledger_path.is_file() {
        return Err(AdmitReject::SourceRecovery(format!(
            "{} 无 ledger（源单不存在或尚未落账）",
            ledger_path.display()
        ))
        .into());
    }
    let records = OrderLedger::load(&ledger_path).map_err(|err| {
        OrderError::from(AdmitReject::SourceRecovery(format!(
            "ledger 读取失败（{}）: {err}",
            ledger_path.display()
        )))
    })?;
    latest_snapshot(&records).ok_or_else(|| {
        OrderError::from(AdmitReject::SourceRecovery(format!(
            "全账本无订单快照（{} 共 {} 行），无从恢复源单",
            ledger_path.display(),
            records.len()
        )))
    })
}

/// 账本最后一条带快照的行 = 源单现态。
fn latest_snapshot(records: &[LedgerRecord]) -> Option<WorkOrder> {
    records
        .iter()
        .rev()
        .find_map(|record| record.order_after.clone())
}

/// 第③步执行体：admit_decision 经真引擎子进程（门面不在进程内复刻四层校验）。
/// 返回引擎版本（入 receipt 审计面）。
fn validate_decision_via_engine(
    client: &EngineClient,
    order: &WorkOrder,
    request: &AdmissionRequest,
    now: DateTime<Utc>,
) -> Result<String, OrderError> {
    let order_value = serde_json::to_value(order).map_err(|err| {
        OrderError::from(AdmitReject::DecisionFourLayer(format!(
            "源单快照序列化失败: {err}"
        )))
    })?;
    let artifact_value = serde_json::to_value(&request.decision).map_err(|err| {
        OrderError::from(AdmitReject::DecisionFourLayer(format!(
            "决策工件序列化失败: {err}"
        )))
    })?;
    let registry_value = serde_json::to_value(&request.actor_registry).map_err(|err| {
        OrderError::from(AdmitReject::DecisionFourLayer(format!(
            "注册表序列化失败: {err}"
        )))
    })?;
    let engine_request = json!({
        "op": "admit_decision",
        "order": order_value,
        // payload 由门面按现成 API 现算（与引擎内部绑定值同源同值）；
        // 引擎仍会独立重算并比对，双重绑定防"签 A 批 B"。
        "payload": order.admission_decision_payload(),
        "artifact": artifact_value,
        "actor_registry": registry_value,
        "validated_at": now,
    });
    match client
        .call(&engine_request)
        .map_err(|source| OrderError::Engine { source })?
    {
        EngineOutcome::Accept(accept) => Ok(accept.engine_version),
        EngineOutcome::Reject(rejection) => Err(OrderError::from(AdmitReject::DecisionFourLayer(
            rejection.to_string(),
        ))),
    }
}

/// 第④步-basis 复核（**PoC-C 落位**，保留码 OR2109）。
///
/// 交接：逐条独立重算 basis 条目（条目引用的工件/快照以实际字节重算摘要，
/// 不信工件自报），任一不符 → [`AdmitReject::BasisRecompute`]。本批骨架不执行
/// 语义核对（占位恒过），但链位、保留码与调用点在链序第④步内已钉死不得移动。
fn recheck_basis(_decision: &DecisionArtifact) -> Result<(), AdmitReject> {
    Ok(())
}

/// 第④步-digest 复核（**PoC-C 落位**，保留码 OR2110）。
///
/// 交接：对 next_orders 计划面独立重算 digest，与工件内 `next_orders_digest`
/// 比对（防换计划面），不符 → [`AdmitReject::DigestRecompute`]。占位恒过。
fn recheck_next_orders_digest(_decision: &DecisionArtifact) -> Result<(), AdmitReject> {
    Ok(())
}

/// 第⑤步凭证求交（**PoC-C 落位**，保留码 OR2111）。
///
/// 交接：actor 凭证 ∩ 订单凭证面（voucher/credential 族的交集判定，Cedar
/// 扩面前的注册表+真验签先行腿，T6 终裁），不交 →
/// [`AdmitReject::CredentialIntersection`]。占位恒过。
fn intersect_credentials(_decision: &DecisionArtifact) -> Result<(), AdmitReject> {
    Ok(())
}

// ---------- 受理账本 ----------

/// 读回全部已受理回执（缺文件 = 空清单；坏行 fail-loud 为状态读取失败）。
fn load_receipts(state: &StateDir, order_id: &str) -> Result<Vec<AdmissionReceipt>, OrderError> {
    let path = admissions_path(state, order_id);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&path).map_err(|err| OrderError::State {
        detail: format!("受理账本读取失败 {}: {err}", path.display()),
    })?;
    let mut receipts = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        receipts.push(
            serde_json::from_str(trimmed).map_err(|err| OrderError::State {
                detail: format!(
                    "受理账本第 {} 行不是合法回执 JSON（{}）: {err}",
                    index + 1,
                    path.display()
                ),
            })?,
        );
    }
    Ok(receipts)
}

/// 追加一笔回执（第⑥步唯一写点；失败 = OR2112，调用方收到拒绝）。
fn append_receipt(
    state: &StateDir,
    order_id: &str,
    receipt: &AdmissionReceipt,
) -> Result<(), OrderError> {
    let path = admissions_path(state, order_id);
    let line = serde_json::to_string(receipt).map_err(|err| {
        OrderError::from(AdmitReject::StateWrite(format!("回执序列化失败: {err}")))
    })?;
    use std::io::Write as _;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|err| {
            OrderError::from(AdmitReject::StateWrite(format!(
                "受理账本打开失败 {}: {err}",
                path.display()
            )))
        })?;
    file.write_all(line.as_bytes())
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
        .map_err(|err| {
            OrderError::from(AdmitReject::StateWrite(format!(
                "受理账本落盘失败 {}: {err}",
                path.display()
            )))
        })
}

/// 受理账本路径（`<state>/orders/<order_id>/admissions.jsonl`）。
pub fn admissions_path(state: &StateDir, order_id: &str) -> std::path::PathBuf {
    state.order_dir(order_id).join(ADMISSIONS_FILE)
}

fn is_sha256_hex(text: &str) -> bool {
    text.len() == af_workorder::SHA256_HEX_LEN
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_workorder::{Decision, Ed25519Keypair, Role, TrustedActor};

    const NOW: &str = "2026-11-03T00:00:00Z";
    const KEY_EXPIRY: &str = "2027-01-01T00:00:00Z";
    const DECISION_EXPIRY: &str = "2026-11-04T00:00:00Z";
    const SEED: [u8; 32] = [0x31; 32];

    fn time(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn keypair() -> Ed25519Keypair {
        Ed25519Keypair::from_seed(&SEED)
    }

    fn registry() -> TrustedActorRegistry {
        TrustedActorRegistry::new(vec![TrustedActor::new(
            "user-a",
            Role::Approver,
            &keypair().public_key(),
            time(KEY_EXPIRY),
        )])
        .unwrap()
    }

    fn valid_request(order_id: &str) -> AdmissionRequest {
        let order = WorkOrder::new(
            order_id,
            af_workorder::GraphVersion::new("g-1"),
            af_workorder::Sla::new(None, af_workorder::Priority::Normal),
        );
        let attributes = af_workorder::DecisionAttributes::new(
            7,
            vec!["artifact:fa-1:qualified".into()],
            "a".repeat(64),
            time(DECISION_EXPIRY),
        );
        // 按 admission payload 真签（admit 链第③步绑定的是
        // `WorkOrder::admission_decision_payload`，不是引擎 approve_decision
        // 的 graph-only subject）。
        let decision = {
            let payload = order.admission_decision_payload();
            DecisionArtifact::seal(
                "user-a",
                Role::Approver,
                Decision::Approve,
                &payload,
                &keypair(),
                attributes,
            )
        };
        AdmissionRequest {
            order_id: order_id.to_string(),
            decision,
            actor_registry: registry(),
            template: AdjudicationTemplate {
                template_id: "tpl-adjudication-v1".into(),
                template_version: "1.0".into(),
                doc_kind: "裁决单".into(),
                body_sha256: "c".repeat(64),
            },
            resource_profile: ResourceProfileSpec {
                profile_id: "rp-poc".into(),
                profile_version: "1.0".into(),
                claims: vec![ResourceClaim {
                    resource: "glm-cp".into(),
                    units: 1,
                }],
            },
            gate_list: GateListSpec {
                gates: vec![GateSpec {
                    gate_id: "F1".into(),
                    kind: "fast".into(),
                    required: true,
                }],
            },
            validated_at: Some(time(NOW)),
        }
    }

    #[test]
    fn template_schema_validation_is_strict() {
        let mut request = valid_request("wo-t");
        request.template.doc_kind = "派单".into();
        assert!(request
            .template
            .validate()
            .unwrap_err()
            .contains("doc_kind"));
        request.template.doc_kind = "裁决单".into();
        request.template.body_sha256 = "XYZ".repeat(20);
        assert!(request
            .template
            .validate()
            .unwrap_err()
            .contains("body_sha256"));
        request.template.body_sha256 = "c".repeat(64);
        request.template.template_id = "  ".into();
        assert!(request
            .template
            .validate()
            .unwrap_err()
            .contains("template_id"));
        request.template.template_id = "tpl-adjudication-v1".into();
        assert!(request.template.validate().is_ok());
    }

    #[test]
    fn resource_profile_schema_validation_is_strict() {
        let mut request = valid_request("wo-r");
        request.resource_profile.claims[0].units = 0;
        assert!(request
            .resource_profile
            .validate()
            .unwrap_err()
            .contains("units"));
        request.resource_profile.claims[0].units = 1;
        request.resource_profile.claims[0].resource = "".into();
        assert!(request
            .resource_profile
            .validate()
            .unwrap_err()
            .contains("resource"));
        request.resource_profile.claims[0].resource = "glm-cp".into();
        assert!(request.resource_profile.validate().is_ok());
    }

    #[test]
    fn gate_list_schema_validation_rejects_duplicates() {
        let mut request = valid_request("wo-g");
        request.gate_list.gates.push(af_order_gate_spec("F1"));
        assert!(request.gate_list.validate().unwrap_err().contains("重复"));
        request.gate_list.gates.pop();
        request.gate_list.gates[0].kind = " ".into();
        assert!(request.gate_list.validate().unwrap_err().contains("kind"));
        request.gate_list.gates[0].kind = "fast".into();
        assert!(request.gate_list.validate().is_ok());
    }

    fn af_order_gate_spec(gate_id: &str) -> GateSpec {
        GateSpec {
            gate_id: gate_id.to_string(),
            kind: "fast".into(),
            required: true,
        }
    }

    #[test]
    fn request_parse_names_missing_unknown_and_bad_fields() {
        let base = serde_json::json!({
            "order_id": "wo-p",
            "decision": serde_json::to_value(valid_request("x").decision).unwrap(),
            "actor_registry": serde_json::to_value(registry()).unwrap(),
            "template": serde_json::to_value(valid_request("x").template).unwrap(),
            "resource_profile": serde_json::to_value(valid_request("x").resource_profile).unwrap(),
            "gate_list": serde_json::to_value(valid_request("x").gate_list).unwrap(),
        });
        let text = serde_json::to_string(&base).unwrap();
        let parsed = parse_request(&text).expect("合法请求应可解析");
        assert_eq!(parsed.order_id, "wo-p");

        // 缺字段点名
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove("template");
        assert!(matches!(
            parse_request(&missing.to_string()).unwrap_err(),
            RequestError::MissingField(field) if field == "template"
        ));

        // 未知字段全列（字典序）
        let mut unknown = base.clone();
        unknown
            .as_object_mut()
            .unwrap()
            .insert("zz".into(), 1.into());
        unknown
            .as_object_mut()
            .unwrap()
            .insert("aa".into(), 2.into());
        assert!(matches!(
            parse_request(&unknown.to_string()).unwrap_err(),
            RequestError::UnknownFields(names) if names == "aa, zz"
        ));

        // 坏 validated_at / 路径不安全 order_id
        let mut bad_time = base.clone();
        bad_time
            .as_object_mut()
            .unwrap()
            .insert("validated_at".into(), serde_json::json!("not-a-time"));
        assert!(matches!(
            parse_request(&bad_time.to_string()).unwrap_err(),
            RequestError::Invalid {
                field: "validated_at",
                ..
            }
        ));
        let mut bad_id = base.clone();
        bad_id.as_object_mut().unwrap()["order_id"] = serde_json::json!("../escape");
        assert!(matches!(
            parse_request(&bad_id.to_string()).unwrap_err(),
            RequestError::Invalid {
                field: "order_id",
                ..
            }
        ));
    }

    #[test]
    fn decision_digest_and_idempotency_key_are_stable() {
        let request = valid_request("wo-d");
        let digest = decision_digest(&request.decision);
        assert_eq!(digest, decision_digest(&request.decision));
        assert_eq!(digest.len(), 64);
        let key = idempotency_key("sid-abc", 7, &digest);
        assert_eq!(key, format!("sid-abc/admit/7/{digest}"));
    }
}
