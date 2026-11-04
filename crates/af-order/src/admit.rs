//! admit —— 裁决受理：把一份已签 DecisionArtifact 按固定校验链接收入单。
//!
//! 公共 API：[`admit`]（`AdmissionRequest` → `AdmissionReceipt` | [`OrderError`]）+
//! CLI 子命令 `af-order admit --request <admission.json> --state <dir>`（见
//! [`crate::cli`]）。PoC-A2 骨架 + PoC-C 落地：OR2109/2110/2111 与 seq 单调门
//! 已按下方口径实现（各函数 docstring 即判据）；模板语义匹配/资源可用性
//! 等其余语义核对仍留后续批次（接口位与保留码不动）。
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
//!    （同键同摘要 → 回执原 receipt；同 seq 异摘要 → OR2107；新 seq 回退
//!    （低于本源单已受理最大 seq）→ OR2107 单调门）+ 决策有效期
//!    （validated_at ≥ expires_at → OR2108）；basis 逐项重算（OR2109）与
//!    next_orders_digest 重算（OR2110）（PoC-C 落地，见各自 docstring）。
//! 5. **凭证求交**（OR2111，PoC-C 落地）：请求声明的受理所需凭证与源单持有
//!    凭证面求交，交集为空即拒。
//! 6. **全过后才写状态**：把 receipt 追加落盘
//!    `<state>/orders/<id>/admissions.jsonl`（OR2112）。任一步失败 = 类型化
//!    拒绝且**零外部效应**（不落 receipt、不动账本、不起别的进程）。
//!
//! # Receipt 幂等键
//!
//! `{source_stable_id}/admit/{decision_seq}/{decision_digest}`（digest = 工件
//! canonical 字节的 SHA-256，见 [`decision_digest`]）。同键同摘要重放返回
//! 原 receipt（`replayed=true`，零写入）；同 seq 异摘要与 seq 回退（低于本
//! 源单已受理最大 seq）= OR2107 拒绝（单调门细则见 [`AdmitReject::SeqConflict`]）。

use std::collections::BTreeSet;
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
    /// 计划面：本次准入声称的下一步订单集合（OR2110 重算 digest 的输入面；
    /// 与工件内 `next_orders_digest` 必须一致，规范化口径见
    /// [`canonical_next_orders_digest`]）。空清单 = 无后继订单（合法）。
    pub next_orders: Vec<String>,
    /// 凭证面：本次受理所需凭证 id 清单（OR2111 求交的请求侧；与源单持有
    /// 凭证面求交，交集为空即拒）。至少须呈报一项源单确持有的凭证。
    pub required_credentials: Vec<String>,
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
    "next_orders",
    "order_id",
    "required_credentials",
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
        "next_orders",
        "required_credentials",
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

/// next_orders 规范化线的域分离前缀（配方层文档口径的一部分，不另设文件）。
pub const NEXT_ORDERS_DOMAIN: &str = "af-order/next-orders/v1";

/// next_orders 计划面的确定规范化 + SHA-256（OR2110 的唯一重算口径）。
///
/// # 口径（PoC-C 落地，测试钉死）
///
/// 1. **逐项校验**：条目不得为空、不得含任何空白字符（下一步订单引用是
///    单个 token，不是自由文本）——违反即 `Err`。
/// 2. **确定顺序规范化**：条目按字典序排序去重（[`BTreeSet`]），消灭提交方
///    列表顺序/重复项的漂移——`[a,b]` 与 `[b,a]`、`[a,a,b]` 规范化同线。
/// 3. **域分离线格式**：`"{NEXT_ORDERS_DOMAIN}\n{条数}\n{逐行条目}"`，
///    条数取去重后的行数；空清单 = 条数 0、无条目行。
/// 4. digest = 规范化线 UTF-8 字节的 SHA-256 小写 hex。
///
/// 跨进程确定：同样输入必得同样 digest（生成侧 [`crate::adjudicate`] 与
/// 受理侧本函数同用此口径，无第二套 hash）。
pub fn canonical_next_orders_digest(next_orders: &[String]) -> Result<String, String> {
    for entry in next_orders {
        if entry.is_empty() || entry.chars().any(char::is_whitespace) {
            return Err(format!("next_orders 条目不得为空或含空白: {entry:?}"));
        }
    }
    let unique: BTreeSet<&str> = next_orders.iter().map(String::as_str).collect();
    let mut canonical = format!("{NEXT_ORDERS_DOMAIN}\n{}\n", unique.len());
    for (index, entry) in unique.iter().enumerate() {
        if index > 0 {
            canonical.push('\n');
        }
        canonical.push_str(entry);
    }
    Ok(sha256_hex(canonical.as_bytes()))
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
    /// OR2107：seq 复核失败——同 seq 异摘要（幂等冲突），或新 seq 回退
    /// （低于本源单已受理最大 seq；PoC-C 单调门已落地在同一查账窗：
    /// admissions.jsonl 已持久全部 (seq, digest)，单调门只在此变体上扩判断，
    /// PoC-A 钉死的两负例——同键重放回执 / 同 seq 异摘要拒绝——不破）。
    #[error("OR2107: seq 复核失败（同 seq 异摘要或 seq 回退）: {0}")]
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
    let (order, records) = recover_source_order(state, &request.order_id)?;
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
        // 同 seq 异摘要：幂等冲突，拒（OR2107）。
        return Err(AdmitReject::SeqConflict(format!(
            "源单 {} 已受理过 seq={} 的决策（异摘要）：重放须同键同摘要，换内容须换 seq",
            request.order_id, request.decision.decision_seq
        ))
        .into());
    }
    // seq 单调门（OR2107，PoC-C 落地）：新准入 seq 必须 > 本源单已受理最大
    // seq。同 seq 已被上方两臂接住（同键重放回执 / 异摘要拒绝），此处只剩
    // 严格更小（或不可能的越序重放）——裁决被回退重放意味着旧结论盖新章，
    // 一律拒。admissions.jsonl 已持久全部 (seq, digest)，查账即真值。
    if let Some(max_seen) = prior.iter().map(|receipt| receipt.decision_seq).max() {
        if request.decision.decision_seq < max_seen {
            return Err(AdmitReject::SeqConflict(format!(
                "源单 {} 的 decision_seq={} 回退（已受理最大 seq={}）：新 seq 必须单调递增",
                request.order_id, request.decision.decision_seq, max_seen
            ))
            .into());
        }
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

    // ── 第②步-c：F 门单两查（gate_admit::validate_required_gate_inputs）──
    // PoC-F 纯函数已收卷合入（af_workorder::gate_admit，合流批改名消歧）。
    // **只接两查这一层，数据面 PoC-C 落位**：required = 配方声明的必需前级门
    // 工件（RequiredGateInput），provided = 按 input_id 从 CAS 解出
    // af.gate_result.v1 后回填的原样 payload.verdict（ProvidedGateInput，不
    // 推断默认、不把红/黄门映射成绿）。两面的装载（配方→请求、CAS→verdict）
    // 尚无管道，属 PoC-C 范围；本骨架调用占位恒过，链位、错误映射（门清单
    // 步骤占位码 OR2105）已钉死不得移动。落位时随数据面一起补 checks 点名
    // （本骨架不新增点名——回执 checks 面由验收钉死）。
    gate_input_two_check(request)?;

    // ── 第③步：DecisionArtifact 四层校验（真验签，必经引擎子进程）──
    let engine_version = validate_decision_via_engine(client, &order, request, now)?;
    checks.push(CHECK_DECISION_FOUR_LAYER.into());

    // ── 第④步：seq/expiry/basis/digest 复核 ───────────────────
    // seq：幂等窗已处理重放/冲突/单调回退（见上方单调门）。
    checks.push(CHECK_SEQ_WINDOW.into());
    if now >= request.decision.expires_at {
        return Err(AdmitReject::Expired(format!(
            "validated_at={now} ≥ expires_at={}（恰在时刻即拒）",
            request.decision.expires_at
        ))
        .into());
    }
    checks.push(CHECK_EXPIRY.into());
    recheck_basis(
        &request.decision,
        &order,
        &records,
        state,
        &request.order_id,
    )?;
    checks.push(CHECK_BASIS_RECHECK.into());
    recheck_next_orders_digest(&request.decision, &request.next_orders)?;
    checks.push(CHECK_DIGEST_RECHECK.into());

    // ── 第⑤步：凭证求交（PoC-C 落地）──────────────────────────
    intersect_credentials(
        &request.required_credentials,
        &order,
        state,
        &request.order_id,
    )?;
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

/// 从 state 目录恢复源单最近快照与全量账本行（ledger 最后一条带快照行）。
/// 账本行随快照一并返回：basis 的 `ledger:` 族条目按行重算摘要用（只读，
/// 不重复落盘）。pub(crate)：裁决环 [`crate::adjudicate`] 的生成/闭环腿与
/// admit 链同源恢复，不另造第二套恢复口径。
pub(crate) fn recover_source_order(
    state: &StateDir,
    order_id: &str,
) -> Result<(WorkOrder, Vec<LedgerRecord>), OrderError> {
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
    let order = latest_snapshot(&records).ok_or_else(|| {
        OrderError::from(AdmitReject::SourceRecovery(format!(
            "全账本无订单快照（{} 共 {} 行），无从恢复源单",
            ledger_path.display(),
            records.len()
        )))
    })?;
    Ok((order, records))
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

// ---------- 第④⑤步重算细则（PoC-C 落地） ----------

/// basis 条目文法的实体种类（`<kind>:<entity_id>:<sha256>` 的 kind 段）。
const BASIS_KIND_ARTIFACT: &str = "artifact";
const BASIS_KIND_VOUCHER: &str = "voucher";
const BASIS_KIND_LEDGER: &str = "ledger";

/// 源单工件实际字节所在目录内的文件名约定（af-execline 执行单元落盘形态：
/// `<order_dir>/artifacts/<artifact_id>.json`；见 af-execline exec.rs stub 单元）。
fn artifact_file_path(state: &StateDir, order_id: &str, artifact_id: &str) -> std::path::PathBuf {
    state
        .order_dir(order_id)
        .join("artifacts")
        .join(format!("{artifact_id}.json"))
}

/// BudgetVoucher 的内容摘要（canonical JSON 字节 SHA-256，字段序即 serde
/// 声明序，跨进程确定）。`voucher:` 族 basis 与生成侧共用同一口径。
pub fn budget_voucher_digest(voucher: &af_workorder::BudgetVoucher) -> String {
    let bytes = serde_json::to_vec(voucher).expect("BudgetVoucher 只含可序列化字段");
    sha256_hex(&bytes)
}

/// 账本行的内容摘要（canonical JSON 字节 SHA-256，行序字段即 serde 声明序，
/// 跨进程确定）。`ledger:` 族 basis 与生成侧共用同一口径。
pub fn ledger_record_digest(record: &LedgerRecord) -> String {
    let bytes = serde_json::to_vec(record).expect("LedgerRecord 只含可序列化字段");
    sha256_hex(&bytes)
}

/// 第④步-basis 复核（**PoC-C 落地**，保留码 OR2109）。
///
/// # 口径（docstring 即判据，测试钉死）
///
/// basis 是"决策依据"清单，每条必须独立重算通过，任一不符即拒：
///
/// 1. **非空**：basis 为空清单 → 拒（没有依据的决策不受理）。
/// 2. **文法**：每条 = `<kind>:<entity_id>:<sha256>`，恰好三段（':' 分隔），
///    三段皆非空，末段是 64 位小写 hex；kind ∈ {artifact, voucher, ledger}。
///    文法不符 → 拒（伪造条目）。
/// 3. **实体可解析**（源单工件/凭证/账本三处逐一查）：
///    - `artifact:<artifact_id>:<digest>`：实体 = 源单快照上同 id 工件，查无 → 拒；
///    - `voucher:<voucher_id>:<digest>`：实体 = 源单快照绑定的预算凭证，
///      未绑凭证或 id 不符 → 拒；
///    - `ledger:<seq>:<digest>`：实体 = 源单账本 seq 号匹配的行，查无 → 拒。
/// 4. **摘要重算（不信声称）**：
///    - artifact：先比快照 `content_sha256`（登记面）；盘上
///      `artifacts/<artifact_id>.json` 在场时再对**实际字节**重算 SHA-256，
///      字节摘要与登记面漂移 = 账面/实物不一致，一并拒——登记摘要只能由
///      实际字节算出（af-workorder `WorkArtifact::from_bytes` 同纪律）；
///    - voucher：比 [`budget_voucher_digest`]（canonical JSON）；
///    - ledger：比 [`ledger_record_digest`]（canonical JSON）。
///
/// 链位、保留码与调用点在链序第④步内已钉死不得移动（PoC-A 交接约束）。
fn recheck_basis(
    decision: &DecisionArtifact,
    order: &WorkOrder,
    records: &[LedgerRecord],
    state: &StateDir,
    order_id: &str,
) -> Result<(), AdmitReject> {
    if decision.basis.is_empty() {
        return Err(AdmitReject::BasisRecompute(
            "basis 为空：决策必须逐条引用可解析的依据实体".into(),
        ));
    }
    for entry in &decision.basis {
        recheck_basis_entry(entry, order, records, state, order_id)?;
    }
    Ok(())
}

/// 单条 basis 的独立重算（见 [`recheck_basis`] 口径 2-4；逐条互不影响）。
fn recheck_basis_entry(
    entry: &str,
    order: &WorkOrder,
    records: &[LedgerRecord],
    state: &StateDir,
    order_id: &str,
) -> Result<(), AdmitReject> {
    let reject = |reason: String| AdmitReject::BasisRecompute(format!("条目 {entry:?}: {reason}"));
    let segments: Vec<&str> = entry.split(':').collect();
    let &[kind, entity_id, claimed] = segments.as_slice() else {
        return Err(reject(format!(
            "文法不符：须为 <kind>:<entity_id>:<sha256>，实得 {} 段",
            segments.len()
        )));
    };
    if kind.is_empty() || entity_id.is_empty() {
        return Err(reject("kind/entity_id 段为空".into()));
    }
    if !is_sha256_hex(claimed) {
        return Err(reject("digest 段不是 64 位小写 SHA-256 hex".into()));
    }
    match kind {
        BASIS_KIND_ARTIFACT => {
            let artifact = order
                .artifacts()
                .iter()
                .find(|artifact| artifact.artifact_id == entity_id)
                .ok_or_else(|| {
                    reject(format!(
                        "实体不存在：源单 {} 无工件 {entity_id:?}",
                        order.id()
                    ))
                })?;
            let registered = artifact.content_sha256();
            // 盘上实际字节在场时以字节为准重算；缺席时账本快照即恢复源
            // （ADR-003：账本自成恢复源，登记摘要本就出自 attach 时实际字节）。
            let artifact_path = artifact_file_path(state, order_id, entity_id);
            if artifact_path.is_file() {
                let bytes = fs::read(&artifact_path).map_err(|err| {
                    reject(format!(
                        "工件字节读取失败 {}: {err}",
                        artifact_path.display()
                    ))
                })?;
                let actual = sha256_hex(&bytes);
                if actual != registered {
                    return Err(reject(format!(
                        "工件 {entity_id} 账面/字节漂移：登记 {registered}，实算 {actual}"
                    )));
                }
            }
            if claimed != registered {
                return Err(reject(format!(
                    "摘要不符：声称 {claimed}，实体 {registered}"
                )));
            }
            Ok(())
        }
        BASIS_KIND_VOUCHER => {
            let voucher = order
                .budget_voucher()
                .ok_or_else(|| reject(format!("实体不存在：源单 {} 未绑定预算凭证", order.id())))?;
            if voucher.voucher_id != entity_id {
                return Err(reject(format!(
                    "实体不存在：源单绑定凭证 id={}，条目指向 {entity_id:?}",
                    voucher.voucher_id
                )));
            }
            let actual = budget_voucher_digest(voucher);
            if claimed != actual {
                return Err(reject(format!("摘要不符：声称 {claimed}，实体 {actual}")));
            }
            Ok(())
        }
        BASIS_KIND_LEDGER => {
            let seq: u64 = entity_id.parse().map_err(|_| {
                reject("ledger 条目的 entity_id 须是账本 seq（十进制 u64）".to_string())
            })?;
            let record = records
                .iter()
                .find(|record| record.seq == seq)
                .ok_or_else(|| {
                    reject(format!(
                        "实体不存在：账本无 seq={seq} 行（共 {} 行）",
                        records.len()
                    ))
                })?;
            let actual = ledger_record_digest(record);
            if claimed != actual {
                return Err(reject(format!("摘要不符：声称 {claimed}，实体 {actual}")));
            }
            Ok(())
        }
        other => Err(reject(format!(
            "未知实体种类 {other:?}（artifact|voucher|ledger）"
        ))),
    }
}

/// 第④步-digest 复核（**PoC-C 落地**，保留码 OR2110）。
///
/// 口径：对请求的计划面 [`AdmissionRequest::next_orders`] 按
/// [`canonical_next_orders_digest`] 的确定规范化独立重算 SHA-256，与工件
/// 被签字节里的 `next_orders_digest` 严格比对（大小写敏感）；不符 → 拒
/// （防换计划面：签字的是 A 计划面，提交的却是 B 计划面）。
fn recheck_next_orders_digest(
    decision: &DecisionArtifact,
    next_orders: &[String],
) -> Result<(), AdmitReject> {
    let actual = canonical_next_orders_digest(next_orders)
        .map_err(|reason| AdmitReject::DigestRecompute(format!("计划面规范化失败: {reason}")))?;
    if decision.next_orders_digest != actual {
        return Err(AdmitReject::DigestRecompute(format!(
            "计划面摘要不符：工件声称 {}，请求 next_orders 实算 {actual}",
            decision.next_orders_digest
        )));
    }
    Ok(())
}

/// 第⑤步凭证求交（**PoC-C 落地**，保留码 OR2111）。
///
/// # 口径
///
/// - **源单持有凭证面**：① 源单快照绑定的预算凭证 id（账本恢复源，必在）；
///   ② 盘上 `voucher.json` 的凭证 id（submit 签发材料；缺席 = 不计入，
///   在场但损坏 = fail-closed 拒绝，坏凭证材料不得被静默略过）。
/// - **受理所需凭证**：[`AdmissionRequest::required_credentials`] 声明的 id
///   清单（受理方呈报的凭证授权；空白条目 = 拒）。
/// - **判定**：两集合按凭证 id 精确匹配求交；交集为空（含请求侧空清单）
///   → 拒——受理必须落在源单自己的凭证链上，呈报一套与源单无关的凭证
///   视为未获授权。
fn intersect_credentials(
    required: &[String],
    order: &WorkOrder,
    state: &StateDir,
    order_id: &str,
) -> Result<(), AdmitReject> {
    let mut held: Vec<String> = Vec::new();
    if let Some(voucher) = order.budget_voucher() {
        held.push(voucher.voucher_id.clone());
    }
    let voucher_path = state.order_dir(order_id).join(crate::voucher::VOUCHER_FILE);
    if voucher_path.is_file() {
        let text = fs::read_to_string(&voucher_path).map_err(|err| {
            AdmitReject::CredentialIntersection(format!(
                "源单凭证材料读取失败 {}: {err}",
                voucher_path.display()
            ))
        })?;
        let record: crate::voucher::VoucherRecord = serde_json::from_str(&text).map_err(|err| {
            AdmitReject::CredentialIntersection(format!(
                "源单凭证材料损坏 {}（fail-closed）: {err}",
                voucher_path.display()
            ))
        })?;
        held.push(record.voucher_id);
    }
    if required.is_empty() {
        return Err(AdmitReject::CredentialIntersection(format!(
            "受理所需凭证为空清单：须呈报至少一项源单持有的凭证（持有面 = {held:?}）"
        )));
    }
    for entry in required {
        if entry.trim().is_empty() {
            return Err(AdmitReject::CredentialIntersection(
                "受理所需凭证含空白条目".into(),
            ));
        }
    }
    let matched: Vec<&str> = required
        .iter()
        .map(String::as_str)
        .filter(|entry| held.iter().any(|id| id == entry))
        .collect();
    if matched.is_empty() {
        return Err(AdmitReject::CredentialIntersection(format!(
            "凭证交集为空：呈报 {required:?}，源单持有面 {held:?}"
        )));
    }
    Ok(())
}

/// 第②步-c 门单两查执行体：真调 PoC-F 的
/// [`af_workorder::gate_admit::validate_required_gate_inputs`]（存在性一查 +
/// verdict 严格等于 "pass" 二查，fail-closed）。
///
/// **占位恒过（数据面 PoC-C 落位）**：`required`/`provided` 两侧目前以空清单
/// 调用——
/// - `required`：配方面声明的必需前级门工件。admission 请求线格式
///   （[`AdmissionRequest`]，deny_unknown_fields）尚未携带配方装载的
///   `RequiredGateInput` 清单，扩线格式属 PoC-C；
/// - `provided`：按 `input_id` 从 CAS 解出 `af.gate_result.v1`，取其
///   `payload.verdict` 原字符串。admission 链无 CAS 根参数，CAS 对账（同
///   input_id 多份工件、digest 与 basis 引用一致性）属 PoC-C 的 basis 重算
///   （OR2109）邻接面，不在本骨架实现。
///
/// 交接（PoC-C）：落位时两侧装载 + 拒绝映射保持本函数形状
/// （`GateAdmitRejection` → [`AdmitReject::GateListSchema`]，门清单步骤占位
/// 码，细则可在 PoC-C 拆独立保留码）+ 补 checks 点名（回执 checks 面当前由
/// 验收钉死 11 项，本骨架不新增）。链位在 schema 校验之后、引擎四层之前，
/// 不得移动。
fn gate_input_two_check(_request: &AdmissionRequest) -> Result<(), AdmitReject> {
    af_workorder::gate_admit::validate_required_gate_inputs(&[], &[]).map_err(|rejection| {
        AdmitReject::GateListSchema(format!("门单两查未过（PoC-C 落位前不触发）: {rejection}"))
    })
}

// ---------- 受理账本 ----------

/// 读回全部已受理回执（缺文件 = 空清单；坏行 fail-loud 为状态读取失败）。
/// 公开给 [`crate::adjudicate`]：裁决环取"本源单已受理最大 seq"定下一笔
/// 单调 seq，与 admit 链幂等窗同源同读，不另造第二套回执读取。
pub fn load_receipts(
    state: &StateDir,
    order_id: &str,
) -> Result<Vec<AdmissionReceipt>, OrderError> {
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
        // PoC-C 口径：计划面 digest 由同一规范化函数现算，不再手抄占位 hex。
        let next_orders = vec![format!("{order_id}-rw")];
        let attributes = af_workorder::DecisionAttributes::new(
            7,
            vec![format!("artifact:fa-1:{}", "a".repeat(64))],
            canonical_next_orders_digest(&next_orders).unwrap(),
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
            next_orders,
            required_credentials: vec![format!("bv-{order_id}")],
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
            "next_orders": ["wo-p-rw"],
            "required_credentials": ["bv-wo-p"],
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

    /// OR2110 口径单元面：规范化确定（排序去重）、域分离、空白条目拒绝。
    #[test]
    fn canonical_next_orders_digest_is_sorted_deduped_and_domain_separated() {
        let a = canonical_next_orders_digest(&["wo-b".into(), "wo-a".into()]).unwrap();
        let b =
            canonical_next_orders_digest(&["wo-a".into(), "wo-b".into(), "wo-b".into()]).unwrap();
        assert_eq!(a, b, "排序去重后同集合同 digest");
        let empty = canonical_next_orders_digest(&[]).unwrap();
        assert_eq!(empty.len(), 64);
        assert_ne!(empty, a, "空计划面与非空计划面摘要必不同");
        // 域分离：线里含 domain 前缀与条数（对拍同线重算）。
        let line = format!("{NEXT_ORDERS_DOMAIN}\n2\nwo-a\nwo-b");
        assert_eq!(a, sha256_hex(line.as_bytes()));
        assert!(canonical_next_orders_digest(&["含 空格".into()]).is_err());
        assert!(canonical_next_orders_digest(&["".into()]).is_err());
    }

    /// OR2109 口径单元面：文法三段式 + 未知 kind + 非 hex digest。
    #[test]
    fn basis_entry_grammar_is_rejected_before_entity_lookup() {
        let order = WorkOrder::new(
            "wo-basis",
            af_workorder::GraphVersion::new("g-1"),
            af_workorder::Sla::new(None, af_workorder::Priority::Normal),
        );
        let recheck = |entry: &str| {
            recheck_basis_entry(
                entry,
                &order,
                &[],
                &StateDir::new("/nonexistent"),
                "wo-basis",
            )
            .err()
            .map(|err| err.to_string())
        };
        let text = recheck("没有分隔符").unwrap();
        assert!(text.contains("OR2109") && text.contains("文法"), "{text}");
        let text = recheck("artifact:fa-1").unwrap();
        assert!(text.contains("文法"), "{text}");
        let text =
            recheck("magic:fa-1:2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae")
                .unwrap();
        assert!(text.contains("未知实体种类"), "{text}");
        let text = recheck("artifact:fa-1:NOTHEX").unwrap();
        assert!(text.contains("hex"), "{text}");
        // 实体不存在（源单无工件/未绑凭证/空账本）。
        let text = recheck(
            "artifact:fa-1:2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae",
        )
        .unwrap();
        assert!(text.contains("实体不存在"), "{text}");
        let text = recheck(
            "voucher:bv-x:2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae",
        )
        .unwrap();
        assert!(text.contains("实体不存在"), "{text}");
        let text =
            recheck("ledger:0:2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae")
                .unwrap();
        assert!(text.contains("实体不存在"), "{text}");
    }

    /// OR2109 正例单元面：voucher 条目摘要 = canonical JSON 摘要；
    /// ledger 条目摘要 = 账本行 canonical JSON 摘要（换行字节即红）。
    #[test]
    fn basis_voucher_and_ledger_entries_resolve_and_recompute() {
        let mut order = WorkOrder::new(
            "wo-basis-ok",
            af_workorder::GraphVersion::new("g-1"),
            af_workorder::Sla::new(None, af_workorder::Priority::Normal),
        );
        let voucher = af_workorder::BudgetVoucher {
            voucher_id: "bv-ok".into(),
            amount: 42,
            signed: true,
        };
        order.attach_budget_voucher(voucher.clone()).unwrap();
        let record = LedgerRecord {
            seq: 0,
            ts_ms: 1_767_225_600_000,
            order_id: "wo-basis-ok".into(),
            stable_id: order.stable_id().as_str().to_string(),
            op: "create_order".into(),
            idem_key: format!("{}/create_order", order.stable_id()),
            ok: true,
            state_after: Some("Created".into()),
            error_kind: None,
            error_detail: None,
            order_after: None,
        };
        let records = vec![record.clone()];
        let voucher_entry = format!("voucher:bv-ok:{}", budget_voucher_digest(&voucher));
        recheck_basis_entry(
            &voucher_entry,
            &order,
            &records,
            &StateDir::new("/nonexistent"),
            "wo-basis-ok",
        )
        .unwrap_or_else(|err| panic!("凭证条目应过: {err}"));
        let ledger_entry = format!("ledger:0:{}", ledger_record_digest(&record));
        recheck_basis_entry(
            &ledger_entry,
            &order,
            &records,
            &StateDir::new("/nonexistent"),
            "wo-basis-ok",
        )
        .unwrap_or_else(|err| panic!("账本条目应过: {err}"));
        // 摘要换一字节即红（重算口径真的在比内容）。
        let bad_ledger = format!("ledger:0:{}", "0".repeat(64));
        let text = recheck_basis_entry(
            &bad_ledger,
            &order,
            &records,
            &StateDir::new("/nonexistent"),
            "wo-basis-ok",
        )
        .unwrap_err()
        .to_string();
        assert!(text.contains("摘要不符"), "{text}");
    }
}
