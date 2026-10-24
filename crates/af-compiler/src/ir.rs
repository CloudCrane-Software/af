//! IR schema v1（G2a）——「拓扑即 IR」（提案 B §3.2）。
//!
//! 设计纪律（提案 B §3.4 IR 改动预算表 + §3.5 确定性要点 + ADR-010 §2/§5）：
//!
//! - **自持有 schema**：本 crate 自定义 IR 类型（不经 af-graph 依赖），但拓扑
//!   四字段（node id/kind/human/loop/map）与 edge 四字段逐字对应 af-graph
//!   `model.rs` 的形状——对应关系逐结构注记。提案 §3.4 的 af-graph 改动预算
//!   （8 可选字段 + 4 新结构体 + 1 类型表）在本 schema 中全部落位，G2a 不动
//!   af-graph 源码（预算表转为「IR schema 字段清单」的对照账）。
//! - **确定性**：全部集合用 `BTreeMap`/`BTreeSet`（§3.5-1）；Vec 输出按稳定
//!   键排序（§3.5-2：nodes/edges 按 id/四元组、carried 按 store 名、
//!   safety_points 按节点路径、budgets 按 id；`stop`/`failures`/`ops`/
//!   `context.sections` 保文档序——声明序本身即语义序：拼接序/管道序）。
//!   序列化无任何 HashMap 参与迭代序。
//! - **拒绝式信封**：`IrProgram { ir_version, body }`，`ir_version != 1` 或
//!   未知键 → 整份拒绝（decode 见 [`crate::decode`]；纪律承 af-dsl
//!   `envelope.rs` / af-harness `decode_spec` 双先例）。
//! - **一等字段占位（ADR-010 §5，四者）**：`IrNode.capability_refs` /
//!   `Components.tools[].effect_class` / `ArtifactSpec.provenance` /
//!   `IrBody.trust`。v0.1 lowering 产出空默认值，但字段**总在序列化面**（不
//!   skip）——位置先占住，避免日后破坏性变更。
//! - **内联体降维**：loop/map 内联体 → 命名子图（名 = `<图名>/<构造节点点号
//!   路径>`，如 `voyager_curriculum/explore/acquire`）；`graph_version` 一律
//!   stamp 父图版本——af-graph E2007「map 体未版本化」在 DSL 面不可触发；
//!   `entry` = 体 nodes 首元素（§3.2）。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// IR 版本（拒绝式信封，`IR_VERSION != ir_version` 的输入整份拒绝）。
pub const IR_VERSION: u32 = 1;

// ============================================================
// 信封
// ============================================================

/// IR 顶层：版本信封 + body。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrProgram {
    pub ir_version: u32,
    pub body: IrBody,
}

/// IR body 七表（拓扑 / 接口 / 类型 / 槽 / 节点投影 / 策略 / 逃逸记账）+ trust 占位。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrBody {
    /// 拓扑（根图 + 命名子图）。形状对应 af-graph `GraphTemplate`（`model.rs:26-38`）。
    pub graph: GraphTemplate,
    /// 整图 I/O 契约（提案 §1.2 meta 映射表：input/output 折进接口，不开新语法段）。
    pub interface: InterfaceSpec,
    /// 类型词典（提案 §3.4：新 `TypeTable`；键 = 类型名，值 = JSON Schema）。
    pub types: BTreeMap<String, serde_json::Value>,
    /// 状态槽（L0 判定对象；键 = store 名）。
    pub stores: BTreeMap<String, StoreSpec>,
    /// 节点执行投影（落地契约 §7.1 `node_exec` 的声明面；键 = 从根的点号路径，
    /// 如 `rounds.debate_map`——体内节点的全局唯一连接键）。
    pub node_exec: BTreeMap<String, NodeExec>,
    /// 三账合一（budgets/stop/failures）。
    pub policies: IrPolicies,
    /// L4 逃逸记账（键 = 节点点号路径；与 [`IrNode::escape`] 同数据——节点上是
    /// 拓扑位，此处是记账视图，审计不必走子图）。
    pub escapes: BTreeMap<String, EscapeSpec>,
    /// **trust 注记一等占位**（ADR-010 §5）：键 = 组件/节点路径，值 = 信任边界
    /// 注记。v0.1 lowering 恒空映射（`{}`）；eval_input 信任边界当前由
    /// [`StoreSpec::eval_input`] 承载。字段总在序列化面。
    pub trust: BTreeMap<String, String>,
}

// ============================================================
// 拓扑（对应 af-graph GraphTemplate/Node/Edge/SubGraph）
// ============================================================

/// 根图模板。对应 af-graph `GraphTemplate`，差异：`subgraphs` 不 skip（IR 面
/// 无子图即空映射，恒可观测）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphTemplate {
    pub graph_version: String,
    pub name: String,
    /// 入口节点 id（根图 = `meta.entry`）。
    pub entry: String,
    /// 按 id 字典序（§3.5-2）。
    pub nodes: Vec<IrNode>,
    /// 按 `(from, output, to, input)` 字典序。
    pub edges: Vec<IrEdge>,
    /// 命名子图（内联体降维产物；键 = `<图名>/<构造节点点号路径>`）。
    pub subgraphs: BTreeMap<String, SubGraph>,
}

/// 图节点。对应 af-graph `Node` + 预算表 4 项扩展：`escape`（E2210）、
/// `route_exit_guard`（§1.5 归一化的 exit 分支守卫，不产边）、
/// `capability_refs`（ADR-010 §5 一等占位）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrNode {
    pub id: String,
    /// 开放 String（未注册 kind 只进诊断不拒绝，承 af-graph `kind` 模块）。
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human: Option<HumanSpec>,
    /// 字段名承 af-graph（`loop_spec`）；DSL 面 `loop`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_spec: Option<ControlledLoop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_spec: Option<MapSpec>,
    /// L4 逃逸（预算表：af-graph `Node.escape` 新增可选字段）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escape: Option<EscapeSpec>,
    /// route 归一化的 exit 分支联合守卫（`goto: exit` 不产生边，守卫记在节点
    /// 上供执行面判定「本轮提前收束」；§1.5）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_exit_guard: Option<CelRef>,
    /// **capability 引用一等占位**（ADR-010 §5：af-credential Cedar/biscuit
    /// 衰减凭证的登记位）。v0.1 lowering 恒空集；字段总在序列化面。
    pub capability_refs: BTreeSet<String>,
}

/// 类型化工件边。对应 af-graph `Edge` 四字段 + 预算表 2 项扩展：
/// `guard`（route 归一化产物，E1051/E2203 码族）、`checkpoint`（LangGraph
/// checkpointer 声明位）。无 `when` 字段——条件路由全部走 `route`（B-R3）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrEdge {
    pub from: String,
    pub output: String,
    pub to: String,
    pub input: String,
    pub checkpoint: bool,
    /// route 归一化守卫：仅 route 节点的出边携带（互斥且覆盖由构造成立，§1.5）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard: Option<CelRef>,
}

/// 命名子图（内联体降维产物）。对应 af-graph `SubGraph`，差异：
/// `graph_version` 恒 stamp 父图版本（非 Option——E2007 在 DSL 面不可触发）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubGraph {
    pub graph_version: String,
    /// 体 nodes 首元素（§3.2；声明序，非排序后）。
    pub entry: String,
    /// 按 id 字典序。
    pub nodes: Vec<IrNode>,
    /// 按 `(from, output, to, input)` 字典序。
    pub edges: Vec<IrEdge>,
}

/// 人工工位规格。对应 af-graph `HumanSpec` + 预算表 1 项扩展：`resume`
/// （类型化恢复值，LangGraph `interrupt(response_schema)` 对应物）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanSpec {
    pub station: String,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quorum: Option<u32>,
    pub resume: ResumeSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeSpec {
    /// JSON Schema 对象（类型化恢复值契约）。
    pub schema: serde_json::Value,
}

/// 受控循环。对应 af-graph `ControlledLoop` + 预算表 4 项扩展：`args` /
/// `exit_when` / `on_exhausted` / `carried`（Vec 形——af-graph 原为单个
/// `carried_state`，DSL 面是多条携带登记，E2211 判定对象）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlledLoop {
    /// 循环体子图名（内联体降维产物）。
    pub body: String,
    pub max_iterations: u32,
    /// 跨迭代携带状态，按 store 名排序（§3.5-2）。
    pub carried: Vec<CarriedState>,
    /// 安全点，按节点路径排序（§3.5-2；路径从体根解析，可穿嵌套体）。
    pub safety_points: Vec<SafetyPoint>,
    /// 体形参绑定（形参名 → `节点.端口[.字段]` / `input.字段`；同时构成隐式
    /// 依赖边）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_when: Option<CelRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_exhausted: Option<OnExhausted>,
}

/// 携带登记（对应 af-graph `CarriedState`，`name` → `store` 更名对齐 DSL 词）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarriedState {
    pub store: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
}

/// 安全点（对应 af-graph `SafetyPoint` + `SafetyPointKind` 三值原样）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyPoint {
    /// 体内节点点号路径（可穿嵌套体）。
    pub node: String,
    pub kind: SafetyPointKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyPointKind {
    Check,
    Gate,
    Human,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnExhausted {
    Continue,
    Fail,
    KeepBest,
}

/// map 原语。对应 af-graph `MapSpec` 三字段原样（体为降维子图名）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapSpec {
    /// map 体子图名。
    pub body: String,
    /// 被遍历工件集合引用（`节点.端口` / `input.字段`；运行时定基数）。
    pub over: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_fanout: Option<u32>,
}

/// L4 逃逸记账（提案 §3.4：新 `EscapeSpec`；ADR-001 §3 四字段 + M1 吸收的
/// `kind` 闭环词表 7 类）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EscapeSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<EscapeKind>,
    /// 包内实现路径（Python 实现住在包里，不进语法层）。
    pub impl_ref: String,
    pub reason: String,
    pub owner: String,
    pub share_budget_pct: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscapeKind {
    DynamicToolRegistry,
    ProgramSynthesis,
    MetaLoop,
    RuntimeContextMutation,
    EnvAdapter,
    FreeTextParser,
    LlmPredicate,
}

// ============================================================
// 接口与类型表
// ============================================================

/// 整图 I/O 契约（提案 §3.4：新 `InterfaceSpec` 3 字段 + `intent` 留位——
/// stage-4 计划元数据的输入，不在 lowering 中丢失）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterfaceSpec {
    /// `meta.input`（types 表类型名）。
    pub input_type: String,
    /// `meta.output.ref`（类型表达式串，如 `list<skill_entry>`）。
    pub output_type: String,
    /// `meta.output.from`（终态工件唯一发射者）。
    pub output_from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
}

// ============================================================
// 状态槽（提案 §3.4：新 `StoreSpec`）
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreSpec {
    pub name: String,
    pub kind: StoreKind,
    /// collection/vector_index 元素类型（类型表达式串）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    /// kv 值类型（类型表达式串）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// 唯一写者（INV-2）：节点 id 或点号路径（声明原样，语义解析在 stage 1）。
    pub writer: String,
    pub lifecycle: Lifecycle,
    pub retention: Retention,
    /// 评估 ground truth 槽（INV-4 判定对象 / trust 边界）。
    pub eval_input: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreKind {
    MessageLog,
    Document,
    Collection,
    Kv,
    VectorIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Run,
    Iteration,
    Persistent,
}

/// 有界保留（max_items / max_bytes 至少其一——E2208 在 stage 1 判）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retention {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<RententionPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_system: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RententionPolicy {
    RollingWindow,
}

// ============================================================
// 节点执行投影（落地契约 §7.1 `node_exec` 的声明面）
// ============================================================

/// 单节点执行投影。组件解析（unit/components 的注册表引用）是 stage 3
/// （G2b）的产物；G2a 携带 DSL 声明面——LLM/工具/评估器分野在 `uses` 不在
/// `kind`（提案 §1.2 判别表），全部落 `components`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeExec {
    /// 局部 id（体作用域内；全局键 = `node_exec` 的 map 键 = 点号路径）。
    pub node_id: String,
    pub components: Components,
    /// 直读 store 名集（仅非 LLM 节点；LLM 节点声明即 E2303——stage 1 已判）。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub reads: BTreeSet<String>,
    /// 可写 store 名集。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub writes: BTreeSet<String>,
    /// 入边端口消费契约：端口名 → 类型表达式串。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub consumes: BTreeMap<String, String>,
    /// 出边端口工件契约（emits；含 provenance 一等占位）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub artifacts: BTreeMap<String, ArtifactSpec>,
    /// context.sections 装配声明（INV-1 判定对象；文档序 = 拼接序）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<SectionSpec>,
    /// transform 算子（文档序 = 管道求值序；参数闭包 E2209 在 stage 1 判）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ops: Vec<OpSpec>,
    /// gate 的 assert 谓词（内建 `passed: boolean` 端口的求值源）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assert: Option<CelRef>,
}

/// `uses` 投影：组件引用（v0.1 = DSL 声明串原样；stage 3 解析为注册表引用 +
/// 版本核对，E9001-E9004）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Components {
    /// 模型路由引用（`uses.model`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 工具引用集（`uses.tools` ∪ `uses.tool`；键 = 引用串）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tools: BTreeMap<String, ToolSpec>,
    /// 判定单元引用（`uses.unit`；check 节点的评估器外部化）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// 记忆组件引用集（`uses.memory`）。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub memory: BTreeSet<String>,
}

/// 工具绑定：**effect_class 一等占位**（ADR-010 §5：af-harness
/// `ToolDecl.effect_class` 直通位，词表同源 Pure/Persist/ExternalOnce）。
/// v0.1 lowering 恒 `None`（绑定期由组件注册表回填）；字段总在序列化面。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolSpec {
    #[serde(default)]
    pub effect_class: Option<EffectClass>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectClass {
    /// 纯函数。
    Pure,
    /// 需落盘。
    Persist,
    /// 一次性外部操作。
    ExternalOnce,
}

/// 出边端口工件契约（emits 投影 + ADR-010 §5 provenance 一等占位）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactSpec {
    /// 直接声明的类型表达式（`emits: {端口: 类型}`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    /// 从体转发的声明（`emits: {端口: {from_body: 节点.端口}}`；map 构造自动
    /// 包 `list<>`——推导规则与 af-dsl validate 同源，G2a 保真声明面不复制
    /// 推导器）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_body: Option<String>,
    /// 终态工件位（`meta.output` 发射者端口 = true；判定规则见 `lower`）。
    pub is_final: bool,
    /// **provenance 一等占位**（ADR-010 §5：`envelope<T>` 血缘 + eval_sha256
    /// 登记位）。v0.1 lowering 恒 `None`；字段总在序列化面。
    #[serde(default)]
    pub provenance: Option<ProvenanceSpec>,
}

/// 血缘登记占位：envelope 泛型载荷与判分/证据摘要的登记槽（v0.1 空值）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceSpec {
    /// 血缘载荷类型（`envelope<T>` 之 T）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope_of: Option<String>,
    /// 证据/判分摘要（eval_sha256 同位）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eval_sha256: Option<String>,
}

/// context 单段（SectionDecl 镜像；来源词汇表/策略域/配额正数——E23xx 在
/// stage 1 判）。文档序 = 拼接序。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionSpec {
    pub from: String,
    pub quota_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_ref: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
}

/// transform 算子（OpDecl 镜像；闭环词表 8 算子——E1004 在 stage 1 判）。
/// 文档序 = 管道求值序。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpSpec {
    pub op: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub into: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub k: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drop: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens_each: Option<u64>,
}

// ============================================================
// 策略三账（提案 §3.4：`FailureEntry` 新结构体 + BudgetRef/StopCondition 位）
// ============================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrPolicies {
    /// 按 id 字典序（§3.5-2）。scope 原样（`plan` / `node:<id>` / `loop:<id>` /
    /// `map:<id>`）；节点级投影在 stage 4 按 scope 连接。
    pub budgets: Vec<BudgetEntry>,
    /// 文档序（声明序即判定序；CEL 谓词运行时求值）。
    pub stop: Vec<StopEntry>,
    /// 文档序。
    pub failures: Vec<FailureEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetEntry {
    pub id: String,
    pub dimension: BudgetDimension,
    pub scope: String,
    pub cap: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetDimension {
    Turns,
    ModelCalls,
    TokensIn,
    TokensOut,
    TokensTotal,
    ToolCalls,
    Cents,
    ClockMs,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopEntry {
    pub when: CelRef,
    pub outcome: StopOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopOutcome {
    Completed,
    Failed,
    EscalatedHuman,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureEntry {
    pub class: FailureClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetrySpec>,
    /// 处置路由：`same_node` / `next_iteration` / 节点 id（E2204 在 stage 1 判）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<StopOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    TransportTransient,
    EnvironmentFailure,
    SchemaInvalid,
    ToolError,
    Timeout,
    GateRefused,
    BudgetExhausted,
    PolicyStopped,
    Unauthorized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrySpec {
    pub max: u32,
    pub backoff_ms: u64,
}

// ============================================================
// CEL 引用（提案 §3.2：compile 一次 eval 多次）
// ============================================================

/// 已编译 CEL 引用：执行面按 `digest` 缓存编译产物、按 `roots` 核对绑定，
/// 求值走运行时通道（§3.5-5：编译期只编译不求值）。`digest` =
/// SHA-256(规范 JSON `{"roots":[…],"source":"…"}`)——serde_json Map 默认
/// BTreeMap 序（键序=字典序），无 preserve_order feature，确定性成立。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CelRef {
    /// CEL 源串（守卫/断言/终止条件原文；route 守卫为 §1.5 确定性重构形）。
    pub source: String,
    /// 表达式实际引用的根绑定名集（cel-interpreter `references().variables()`，
    /// 字典序）。
    pub roots: BTreeSet<String>,
    pub digest: String,
}
