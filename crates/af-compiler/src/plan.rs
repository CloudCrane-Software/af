//! Stage 4 · 计划生成（`plan`）：绑定图 → 可执行计划 JSON。
//!
//! 顶层结构 = 落地契约 §7.1 草案：拒绝式信封 `{"v":1,"body":…}`（本 crate
//! 自持有 schema，全文 `deny_unknown_fields`）。七块正位 + 计划自含增补：
//!
//! | 块 | 数据源 | 偏差登记 |
//! |---|---|---|
//! | `plan_id`/`compiled_from` | 图名/版本 + `dsl_digest`（IR 规范序列化 SHA-256）+ 编译器版本 | §7.1 草案未定派生式——本版取确定性派生（无时钟无随机） |
//! | `line` | —（af-workorder `bind_line` 执行面事项，§2.2） | 三槽 null 占位 |
//! | `order` | 工单字段落位骨架（`WorkOrderWire` 全键在场） | 仅 `graph_version`/`state` 是编译期实数据，其余执行面补齐（§5 顺序） |
//! | `graph` | IR `GraphTemplate` 原样（af-graph 原形——G2a 字段级对应） | 无 |
//! | `node_exec` | [`BindReport::bindings`] 投影（组件解析+回填后） | unit/components 面对齐 §7.1；tools 为对象数组（effect_class 回填可见） |
//! | `budget`/`resources`/`observation`/`recovery` | IR 三账 / DSL 无资源面（空）/ §7.1 默认骨架 / 失败账+安全点 | 见 RESULT-G2b 偏差表 |
//! | `stores`/`types`/`interface`/`escapes` | IR 原样（**计划自含**——§7.2-1「单文件即编排真相」；草案缺此四块，增补） | 增补块 |
//!
//! 确定性同 G2a 纪律（提案 §3.5）：全 BTree、Vec 按稳定键排序、无时钟/
//! 随机/网络——同一输入两次全链编译产出**逐位一致**的计划 JSON。
//!
//! 逃逸注入（G2b 注入点）：`BindReport::escapes` → 计划 `escapes` 块
//! （`impl_ref` 与记账字段透传——执行面按包内实现装载，编译器不解析）。

use crate::bind::{BindReport, BindViolation};
use crate::ir::{
    BudgetDimension, BudgetEntry, EffectClass, EscapeSpec, FailureEntry, GraphTemplate,
    InterfaceSpec, IrProgram, ProvenanceSpec, StopEntry, StoreSpec,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// 计划版本（拒绝式信封，`v != 1` 整份拒绝）。
pub const PLAN_VERSION: u32 = 1;

// ============================================================
// 信封与顶层
// ============================================================

/// 可执行计划（信封 + body；serde 键 `v`——契约 §7.1 草案原形）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutablePlan {
    #[serde(rename = "v")]
    pub version: u32,
    pub body: PlanBody,
}

/// 计划 body：七块正位 + 自含增补四块（模块注释对照表）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanBody {
    /// 确定性派生：`plan.<图名>@<图版本>+<dsl_digest 前 12 位>`。
    pub plan_id: String,
    pub compiled_from: CompiledFrom,
    pub line: LineSlot,
    pub order: OrderSlot,
    pub graph: GraphTemplate,
    pub interface: InterfaceSpec,
    pub types: BTreeMap<String, serde_json::Value>,
    pub stores: BTreeMap<String, StoreSpec>,
    pub node_exec: Vec<PlanNodeExec>,
    pub budget: PlanBudget,
    pub resources: ResourcesSlot,
    pub observation: ObservationSlot,
    pub recovery: RecoverySlot,
    /// L4 逃逸记账（注入点产物；§7.1 草案缺此块——计划自含增补）。
    pub escapes: BTreeMap<String, EscapeSpec>,
}

/// 编译溯源（与 `WorkOrder.graph_version` 不可变绑定——order.rs:19-31）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledFrom {
    /// DSL→IR 规范序列化（compact）的 SHA-256。
    pub dsl_digest: String,
    /// af-compiler 版本（`CARGO_PKG_VERSION`）。
    pub compiler_version: String,
    pub graph_version: String,
}

/// 产线绑定槽（af-workorder `bind_line` 执行面补齐——编译期无来源，§2.2）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineSlot {
    pub line_id: Option<String>,
    pub line_version: Option<String>,
    /// 领域配置包摘要（§2.2 domain pack）。
    pub domain_pack_digest: Option<String>,
}

// ============================================================
// order 骨架（WorkOrderWire 全键在场——值是执行面落位）
// ============================================================

/// 工单字段落位骨架：仅 `graph_version` 与 `state: "Created"` 是编译期实
/// 数据；id/stable_id/run_id 等由执行面签发（契约 §7.1 草案注释「release
/// 前材料由执行面按 §5 顺序补齐」的同语义占位）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderSlot {
    pub id: String,
    pub stable_id: String,
    pub run_id: String,
    pub state: String,
    pub graph_version: String,
    pub line: Option<OrderLine>,
    pub budget_voucher: Option<serde_json::Value>,
    pub artifacts: Vec<String>,
    pub sla: SlaSlot,
    pub supersedes: Option<String>,
    pub history: Vec<String>,
}

/// `order.line`（null 占位——LineSlot 的工单侧镜像）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderLine {
    pub line_id: String,
    pub line_version: String,
}

/// SLA 槽（deadline 执行面落位；priority v0.1 词表仅 Normal）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlaSlot {
    pub deadline: Option<String>,
    pub priority: PlanPriority,
}

/// 工单优先级（v0.1 词表 = 仅 `Normal`——执行面扩展位，deny 未知值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanPriority {
    #[serde(rename = "Normal")]
    Normal,
}

// ============================================================
// node_exec 投影
// ============================================================

/// 计划节点投影（§7.1 `node_exec[]` 面 + 绑定解析细节；全节点含体内，
/// 按点号路径字典序）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanNodeExec {
    /// 点号路径（全局唯一连接键；`node_id` 局部名在 graph 内解析）。
    pub node_id: String,
    /// 判定单元绑定（`uses.unit` 解析产物）。
    pub unit: Option<UnitBinding>,
    pub components: PlanComponents,
    /// 该节点的预算（IR budgets 中 `scope == "node:<路径>"` 者，按 id 序）。
    pub budget: Vec<NodeBudget>,
    pub artifact_contract: Vec<PlanArtifact>,
    /// 节点级重试（失败分类账是类级非节点级——v0.1 恒 None，位占住）。
    pub retry: Option<PlanRetry>,
}

/// 判定单元绑定（§7.1 `unit` 面：`unit_id`/`unit_version`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitBinding {
    pub unit_id: String,
    pub unit_version: String,
}

/// 组件引用集（§7.1 `components` 面；tools 带 effect_class 回填——ADR-010
/// 一等字段在计划面可见）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanComponents {
    pub model_route: Option<String>,
    /// v0.1 恒 None（uses 无声明面）。
    pub loop_policy: Option<String>,
    pub memory: Vec<String>,
    pub tools: Vec<PlanToolRef>,
    /// v0.1 恒 None（uses 无声明面）。
    pub context_compaction: Option<String>,
}

/// 工具引用（解析后版本化引用 + effect_class 回填）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanToolRef {
    pub reference: String,
    pub effect_class: EffectClass,
}

/// 节点预算条目（IR BudgetEntry 的节点级投影）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeBudget {
    pub id: String,
    pub dimension: BudgetDimension,
    pub cap: u64,
}

/// 工件契约（§7.1 `artifact_contract[]` 面 + provenance 一等位）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanArtifact {
    pub name: String,
    pub is_final: bool,
    /// close 门材料（`node:<check id>`——v0.1 无静态推导源，恒 None 位占住）。
    pub qualified_by: Option<String>,
    /// ADR-010 provenance（envelope_of 已回填；eval_sha256 运行期落）。
    pub provenance: Option<ProvenanceSpec>,
}

/// 重试预案（§7.1 `retry` 面；v0.1 恒 None——类级账在 recovery 块）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRetry {
    pub max_attempts: u32,
    pub requeue_delay_s: u64,
}

// ============================================================
// budget / resources / observation / recovery
// ============================================================

/// 计划预算段：IR 三账原样（budgets 按 id 序；stop/failures 文档序——§5.2
/// 的 voucher/dimensions 面是执行面签发物，编译期无来源，偏差登记）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanBudget {
    pub budgets: Vec<BudgetEntry>,
    pub stop: Vec<StopEntry>,
    pub failures: Vec<FailureEntry>,
}

/// 资源声明（§3.2 gates+limits 面）：DSL v0.1 无资源声明面（并行度在
/// graph 的 `max_fanout`/`max_iterations`）——两槽恒空，位占住。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourcesSlot {
    pub gates: Vec<serde_json::Value>,
    pub limits: Vec<serde_json::Value>,
}

/// 观测骨架（§7.1 草案默认值——数据源是执行面配置，编译期以草案默认占位；
/// af-trace TraceConfig 同形）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationSlot {
    pub span_tree: Vec<String>,
    pub ctx_propagation: String,
    pub otel: OtelSlot,
    pub events: EventsSlot,
    pub mes: MesSlot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OtelSlot {
    pub enabled: bool,
    pub endpoint: Option<String>,
    pub batch_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventsSlot {
    pub root_env: String,
    /// 产线名（执行面 bind_line 落位——编译期空串）。
    pub line: String,
    pub index: bool,
    pub manifest: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MesSlot {
    pub endpoint_ref: String,
    pub database: String,
    pub line_id: String,
    pub station_id: String,
}

/// 恢复预案（§6.2 面 + 声明面安全点）：失败分类账原样 + loop 安全点全集 +
/// 三账本恢复输入（§6.2 总则 1）。`node_retry` 节点级预案 v0.1 恒 None。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoverySlot {
    pub node_retry: Option<PlanRetry>,
    pub resume_source: Vec<String>,
    pub safety_points: Vec<PlanSafetyPoint>,
    pub failures: Vec<FailureEntry>,
}

/// 安全点条目（loop 声明面收集：`node` 是体内相对路径——IR SafetyPoint
/// 原样，全局寻址 = `<loop 路径>.<node>`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanSafetyPoint {
    /// loop 构造节点的点号路径。
    pub loop_node: String,
    pub node: String,
    pub kind: String,
}

// ============================================================
// 生成入口
// ============================================================

/// 计划生成失败（fail-loud）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// 绑定未过（E900x 违例非空——stage 4 拒绝消费不可用绑定）。
    Unbound { violations: Vec<BindViolation> },
    /// 序列化不可达错误（serde_json 对本 schema 理论不可达）。
    Serialize(String),
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unbound { violations } => write!(
                f,
                "绑定未过（{} 条 E900x 违例）：{}",
                violations.len(),
                violations.iter().map(|v| v.to_string()).collect::<Vec<_>>().join("；")
            ),
            Self::Serialize(detail) => write!(f, "计划序列化失败：{detail}"),
        }
    }
}

impl std::error::Error for PlanError {}

/// `IR × 绑定报告 → 可执行计划`（纯函数）。绑定违例非空 →
/// [`PlanError::Unbound`]（一次点名全部违例）。
pub fn generate_plan(ir: &IrProgram, report: &BindReport) -> Result<ExecutablePlan, PlanError> {
    let _ = (ir, report);
    todo!("G2b RED 桩：plan.rs 计划生成未实现")
}

/// `lower + bind + plan + serialize`（compact）——确定性计划快照。
pub fn plan_json(doc: &af_dsl::Document, registry: &crate::components::ComponentRegistry) -> Result<String, PlanError> {
    let _ = (doc, registry);
    todo!("G2b RED 桩：plan_json 未实现")
}

/// 同 [`plan_json`]，pretty（缩进 2）——golden/人读形态，同样确定。
pub fn plan_json_pretty(doc: &af_dsl::Document, registry: &crate::components::ComponentRegistry) -> Result<String, PlanError> {
    let _ = (doc, registry);
    todo!("G2b RED 桩：plan_json_pretty 未实现")
}

// ============================================================
// 拒绝式信封 decode（纪律承 decode.rs / af-harness decode_spec）
// ============================================================

/// 计划 decode 失败（信封四情形 + body 形状）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanParseError {
    MissingVersion,
    VersionNotInteger,
    VersionNotOne { found: u64 },
    UnknownTopKey { key: String },
    MalformedBody { detail: String },
}

impl fmt::Display for PlanParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingVersion => write!(f, "计划信封缺 v 键"),
            Self::VersionNotInteger => write!(f, "计划信封 v 非整数"),
            Self::VersionNotOne { found } => {
                write!(f, "计划信封 v={found} 非 {PLAN_VERSION}（拒绝式：整份拒绝）")
            }
            Self::UnknownTopKey { key } => {
                write!(f, "计划信封未知顶层键 {key:?}（只认 v + body）")
            }
            Self::MalformedBody { detail } => write!(f, "计划 body 形状不符：{detail}"),
        }
    }
}

impl std::error::Error for PlanParseError {}

/// 信封合法的顶层键。
pub const PLAN_TOP_KEYS: [&str; 2] = ["v", "body"];

/// 从 JSON 串解码计划（拒绝式信封；执行面 digest/组件核对归执行面）。
pub fn decode_plan(src: &str) -> Result<ExecutablePlan, PlanParseError> {
    let _ = src;
    todo!("G2b RED 桩：decode_plan 未实现")
}
