//! 七段 serde AST（M1：解析为结构体，构造字段语义校验留待 M2-M4）。
//! 全文 `deny_unknown_fields`。结构按提案 B §1 字段表落 Rust。

use serde::Deserialize;
use std::collections::BTreeMap;

/// YAML 面文档 = `version` + 六个集合段。types/stores/edges/policies 可缺省
/// （提案 B §5 的四条违规样例即如此；正本 §1 未定缺省性——偏差见 RESULT-M1）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub version: u32,
    pub meta: Meta,
    #[serde(default)]
    pub types: Vec<TypeDecl>,
    #[serde(default)]
    pub stores: Vec<StoreDecl>,
    pub nodes: Vec<NodeDecl>,
    #[serde(default)]
    pub edges: Vec<EdgeDecl>,
    #[serde(default)]
    pub policies: Policies,
}

/// meta：身份 + 入口 + 整图 I/O 契约（input/output 折进类型表引用）。
/// `topology_selector` 为择优裁决 §二-1 吸收项（编译期解析语义；语义校验
/// ——候选图须已声明且版本化、when 过 CEL 判定——在 `validate::m4`）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub name: String,
    pub graph_version: String,
    pub entry: String,
    /// 整图输入契约 = types 表中的类型名。
    pub input: String,
    pub output: OutputSpec,
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub topology_selector: Option<TopologySelectorDecl>,
}

/// 整图输出契约：终态工件类型与唯一发射者（from 必填，不做推断）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputSpec {
    pub r#ref: PortType,
    pub from: String,
}

/// 拓扑选择器（裁决 §二-1）：只决定编译产物采用哪个候选图，编译期解析为
/// 单一确定图，不进运行时。候选图引用为版本化图/子图名（包产物）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopologySelectorDecl {
    pub rules: Vec<TopologyRule>,
    pub fallback: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopologyRule {
    pub when: String,
    #[serde(rename = "use")]
    pub use_graph: String,
}

/// types 表条目：name doc 唯一；schema 为内联 JSON Schema 对象（唯一 schema 家）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeDecl {
    pub name: String,
    pub schema: serde_json::Value,
}

/// stores：状态槽（L0 判定对象，INV-2 单写者）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreDecl {
    pub name: String,
    pub r#type: StoreKind,
    #[serde(default)]
    pub item: Option<PortType>,
    #[serde(default)]
    pub value: Option<PortType>,
    /// 唯一写者：节点 id 或点号路径（经 loop/map 体逐层解析）。
    pub writer: String,
    pub lifecycle: Lifecycle,
    pub retention: Retention,
    #[serde(default)]
    pub eval_input: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreKind {
    MessageLog,
    Document,
    Collection,
    Kv,
    VectorIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Run,
    Iteration,
    Persistent,
}

/// 有界保留：max_items / max_bytes 至少其一（E2208）；rolling_window 是唯一
/// 具名 policy。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retention {
    #[serde(default)]
    pub policy: Option<RententionPolicy>,
    #[serde(default)]
    pub max_items: Option<usize>,
    #[serde(default)]
    pub max_bytes: Option<usize>,
    #[serde(default)]
    pub keep_system: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RententionPolicy {
    RollingWindow,
}

/// 节点：kind 四型开放 String（未注册 kind 只进诊断不拒绝）；六个正交构造
/// 字段（loop/map/human/route/escape/ops）与 kind 正交。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeDecl {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub uses: Option<UsesDecl>,
    #[serde(default)]
    pub writes: Option<Vec<String>>,
    #[serde(default)]
    pub reads: Option<Vec<String>>,
    #[serde(default)]
    pub consumes: Option<BTreeMap<String, PortType>>,
    #[serde(default)]
    pub emits: Option<BTreeMap<String, EmitSpec>>,
    #[serde(default)]
    pub context: Option<ContextDecl>,
    #[serde(default)]
    pub ops: Option<Vec<OpDecl>>,
    #[serde(default)]
    pub assert: Option<String>,
    #[serde(default)]
    pub route: Option<RouteDecl>,
    #[serde(rename = "loop")]
    pub loop_spec: Option<LoopDecl>,
    #[serde(default)]
    pub map: Option<MapDecl>,
    #[serde(default)]
    pub human: Option<HumanDecl>,
    #[serde(default)]
    pub escape: Option<EscapeDecl>,
}

/// 组件绑定：LLM/工具/评估器的分野在 uses 不在 kind（IR 直映 node_exec.components）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsesDecl {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub memory: Option<Vec<String>>,
}

/// emits 端口规格：直接类型，或从体转发（map 的 from_body 自动包 list<>）。
/// 自定义反序列化：serde untagged 变体内的 deny_unknown_fields 不生效
/// （实测 from_body+多余键被静默吞），改为显式收键，保住「全文
/// deny_unknown_fields」硬边界——多余键/对象 schema → E1003。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmitSpec {
    FromBody { from_body: String },
    Type(PortType),
}

impl<'de> Deserialize<'de> for EmitSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_yaml::Value::deserialize(d)?;
        match v {
            serde_yaml::Value::Mapping(m) => {
                let key = serde_yaml::Value::String("from_body".into());
                if m.len() == 1 && m.contains_key(&key) {
                    let path = match m.get(&key) {
                        Some(serde_yaml::Value::String(s)) => s,
                        _ => {
                            return Err(serde::de::Error::custom(
                                "invalid type: from_body 的值必须是字符串路径（节点.端口）",
                            ))
                        }
                    };
                    Ok(EmitSpec::FromBody {
                        from_body: path.clone(),
                    })
                } else {
                    Err(serde::de::Error::custom(
                        "invalid type: emits 端口映射值只允许 {from_body: 节点.端口}",
                    ))
                }
            }
            other => PortType::try_from_value(other)
                .map(EmitSpec::Type)
                .map_err(serde::de::Error::custom),
        }
    }
}

/// 端口/store 元素类型：类型名 | list<·> | envelope<·> | 内联原始类型 | signal。
/// 内联只收原始四型——对象 schema 必须住 types 表（E1003）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortType {
    Named(TypeRef),
    Inline(InlinePrimitive),
}

impl PortType {
    /// 从已解析的 YAML 值构造；错误信息以 `invalid type:` 起头（→ E1003）。
    fn try_from_value(v: serde_yaml::Value) -> Result<Self, String> {
        match v {
            serde_yaml::Value::String(s) => parse_type_expr(&s)
                .map(|e| PortType::Named(TypeRef(e)))
                .ok_or_else(|| {
                    format!(
                        "invalid type: 类型表达式 {s:?} 不合法（合法形：name | list<name> | envelope<name> | signal）"
                    )
                }),
            serde_yaml::Value::Mapping(m) => {
                let type_key = serde_yaml::Value::String("type".into());
                if m.len() == 1 && m.contains_key(&type_key) {
                    match m.get(&type_key) {
                        Some(serde_yaml::Value::String(t)) => match t.as_str() {
                            "string" => Ok(PortType::Inline(InlinePrimitive {
                                r#type: PrimitiveKind::String,
                            })),
                            "integer" => Ok(PortType::Inline(InlinePrimitive {
                                r#type: PrimitiveKind::Integer,
                            })),
                            "boolean" => Ok(PortType::Inline(InlinePrimitive {
                                r#type: PrimitiveKind::Boolean,
                            })),
                            "number" => Ok(PortType::Inline(InlinePrimitive {
                                r#type: PrimitiveKind::Number,
                            })),
                            other => Err(format!(
                                "invalid type: 端口内联类型 {other:?} 不在原始四型 string|integer|boolean|number（对象 schema 必须住 types 表）"
                            )),
                        },
                        _ => Err("invalid type: 端口内联 {type: ·} 的值必须是字符串".to_string()),
                    }
                } else {
                    Err(
                        "invalid type: 端口映射值只允许 {type: string|integer|boolean|number}（对象 schema 必须住 types 表）"
                            .to_string(),
                    )
                }
            }
            other => Err(format!(
                "invalid type: 端口类型必须是字符串或 {{type: 原始四型}}，实为 {}",
                yaml_kind_name(&other)
            )),
        }
    }
}

impl<'de> Deserialize<'de> for PortType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_yaml::Value::deserialize(d)?;
        PortType::try_from_value(v).map_err(serde::de::Error::custom)
    }
}

fn yaml_kind_name(v: &serde_yaml::Value) -> &'static str {
    match v {
        serde_yaml::Value::Null => "null",
        serde_yaml::Value::Bool(_) => "布尔",
        serde_yaml::Value::Number(_) => "数字",
        serde_yaml::Value::String(_) => "字符串",
        serde_yaml::Value::Sequence(_) => "序列",
        serde_yaml::Value::Mapping(_) => "映射",
        serde_yaml::Value::Tagged(_) => "带标签值",
    }
}

impl PortType {
    /// 展示名（违例信息用）。
    pub fn display(&self) -> String {
        match self {
            Self::Named(t) => t.display(),
            Self::Inline(p) => format!("{{type:{}}}", p.r#type.display()),
        }
    }

    pub(crate) fn signal() -> Self {
        Self::Named(TypeRef(TypeExpr::Signal))
    }

    pub(crate) fn boolean() -> Self {
        Self::Inline(InlinePrimitive {
            r#type: PrimitiveKind::Boolean,
        })
    }

    /// 包一层 list<>（from_body 经 map 构造转发时用）。
    pub(crate) fn list_of(inner: TypeExpr) -> Self {
        Self::Named(TypeRef(TypeExpr::List(Box::new(inner))))
    }
}

/// 字符串类型表达式：`name` | `list<name>` | `envelope<name>` | `signal`。
/// 嵌套泛型（list<list<·>>）不在 YAML 面语法内 → 解析期 E1003。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef(pub TypeExpr);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    Signal,
    Named(String),
    List(Box<TypeExpr>),
    Envelope(Box<TypeExpr>),
}

impl TypeExpr {
    fn display(&self) -> String {
        match self {
            Self::Signal => "signal".to_string(),
            Self::Named(n) => n.clone(),
            Self::List(inner) => format!("list<{}>", inner.display()),
            Self::Envelope(inner) => format!("envelope<{}>", inner.display()),
        }
    }
}

impl TypeRef {
    pub fn display(&self) -> String {
        self.0.display()
    }
}

/// 类型表达式文法：一层泛型、载荷为名或 signal。返回 None = 语法不合法。
fn parse_type_expr(s: &str) -> Option<TypeExpr> {
    if s == "signal" {
        return Some(TypeExpr::Signal);
    }
    if let Some(inner) = s.strip_prefix("list<").and_then(|r| r.strip_suffix('>')) {
        return parse_type_payload(inner).map(|p| TypeExpr::List(Box::new(p)));
    }
    if let Some(inner) = s
        .strip_prefix("envelope<")
        .and_then(|r| r.strip_suffix('>'))
    {
        return parse_type_payload(inner).map(|p| TypeExpr::Envelope(Box::new(p)));
    }
    if is_type_name(s) {
        return Some(TypeExpr::Named(s.to_string()));
    }
    None
}

/// 泛型载荷：名或 signal，不允许再嵌套。
fn parse_type_payload(s: &str) -> Option<TypeExpr> {
    if s == "signal" {
        Some(TypeExpr::Signal)
    } else if is_type_name(s) {
        Some(TypeExpr::Named(s.to_string()))
    } else {
        None
    }
}

/// 类型名字符集：字母数字与 `_.:-`，非空（承 ref 名先例 af-harness is_ref_name）。
fn is_type_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
}

/// 内联原始类型：{type: string|integer|boolean|number}（对象 schema 不收 → E1003）。
/// 由 `PortType::try_from_value` 显式构造（不经 derive，见该处说明）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlinePrimitive {
    pub r#type: PrimitiveKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveKind {
    String,
    Integer,
    Boolean,
    Number,
}

impl PrimitiveKind {
    pub fn display(&self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Integer => "integer",
            Self::Boolean => "boolean",
            Self::Number => "number",
        }
    }
}

/// edges：类型化工件边（四字段与 af-graph Edge 逐字一致 + 可选 checkpoint；无 when）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeDecl {
    pub from: String,
    pub output: String,
    pub to: String,
    pub input: String,
    #[serde(default)]
    pub checkpoint: Option<bool>,
}

/// loop 构造（承 af-graph ControlledLoop，三扩展；体为内联 nodes+edges）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopDecl {
    pub body: Body,
    pub max_iterations: u32,
    #[serde(default)]
    pub carried: Option<Vec<CarriedDecl>>,
    pub safety_points: Vec<SafetyPointDecl>,
    #[serde(default)]
    pub args: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub exit_when: Option<String>,
    #[serde(default)]
    pub on_exhausted: Option<OnExhausted>,
}

/// 内联体：entry = nodes 首元素。edges 可缺省（golden 样例 6 的 explore 体）。
/// M1 不做空体检查（E1007 留待构造形状模块）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Body {
    pub nodes: Vec<NodeDecl>,
    #[serde(default)]
    pub edges: Vec<EdgeDecl>,
}

/// 携带登记：必须覆盖体内（含嵌套体）写入的、体外声明的 store（E2211）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarriedDecl {
    pub store: String,
    #[serde(default)]
    pub max_items: Option<usize>,
    #[serde(default)]
    pub max_bytes: Option<usize>,
}

/// 安全点：路径从体根解析，可穿嵌套体；kind 三值承 af-graph SafetyPointKind。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyPointDecl {
    pub node: String,
    pub kind: SafetyPointKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyPointKind {
    Check,
    Gate,
    Human,
}

impl SafetyPointKind {
    /// 词面（serde snake_case 同形；E2322 信息定位用）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Gate => "gate",
            Self::Human => "human",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnExhausted {
    Continue,
    Fail,
    KeepBest,
}

/// map 构造（承 af-graph MapSpec）：over 运行时定基数、体经隐式形参 item 收元素。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapDecl {
    pub over: String,
    pub body: Body,
    #[serde(default)]
    pub max_fanout: Option<u32>,
}

/// human 构造（承 af-graph HumanSpec，一扩展）：resume.schema 类型化恢复值。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanDecl {
    pub station: String,
    pub role: String,
    #[serde(default)]
    pub quorum: Option<u32>,
    pub resume: ResumeDecl,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeDecl {
    pub schema: serde_json::Value,
}

/// route 构造（§1.5 归一化算法在 `crate::route`，语义校验在 `validate::m4`）：
/// 有序 CEL 规则 + 必填 default（缺 → serde E1002，§1.3「不进语义 pass」）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteDecl {
    pub default: String,
    pub rules: Vec<RouteRule>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteRule {
    pub when: String,
    pub goto: String,
}

/// escape 构造（L4 记账，ADR-001 §3）：四字段缺一 → serde E1002（E2210 语义
/// 留待后续模块）；`kind` 为择优裁决 §二-2 吸收的闭环词表 7 类（v0.1 词表；
/// 样例 B §4 成文先于裁决、未带 kind，故 M1 收为可选、值域已闭环——见 RESULT-M1）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EscapeDecl {
    #[serde(default)]
    pub kind: Option<EscapeKind>,
    pub impl_ref: String,
    pub reason: String,
    pub owner: String,
    pub share_budget_pct: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
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

/// policies：三账合一（budgets/stop/failures；同构记录表 + CEL 谓词 + 闭环词表）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policies {
    #[serde(default)]
    pub budgets: Vec<BudgetDecl>,
    #[serde(default)]
    pub stop: Vec<StopDecl>,
    #[serde(default)]
    pub failures: Vec<FailureDecl>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetDecl {
    pub id: String,
    pub dimension: BudgetDimension,
    /// plan | node:<id> | loop:<id> | map:<id>。
    pub scope: String,
    pub cap: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopDecl {
    pub when: String,
    pub outcome: StopOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopOutcome {
    Completed,
    Failed,
    EscalatedHuman,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureDecl {
    pub class: FailureClass,
    #[serde(default)]
    pub retry: Option<RetryDecl>,
    #[serde(default)]
    pub route: Option<String>,
    #[serde(default)]
    pub feedback: Option<String>,
    #[serde(default)]
    pub fallback_route: Option<String>,
    #[serde(default)]
    pub terminal: Option<bool>,
    #[serde(default)]
    pub outcome: Option<StopOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryDecl {
    pub max: u32,
    pub backoff_ms: u64,
}

/// context（sections 语义校验 = INV-1，`validate/inv1.rs`）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextDecl {
    pub sections: Vec<SectionDecl>,
}

/// section 平铺联合体：from ∈ {template,input,arg,edge,store}（M1 查词汇表），
/// 各 from 形的字段闭包与来源可解析性在 M2（E2301/E2302/E2401/E2402/E2403，
/// `validate/inv1.rs`）。quota_tokens 必填（M1 serde 层）；配额正数核对在 M2
///（E2302，policy:none 记 0 合法）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionDecl {
    pub from: String,
    pub quota_tokens: u64,
    #[serde(default)]
    pub template_ref: Option<String>,
    #[serde(default)]
    pub vars: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub fields: Option<Vec<String>>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub port: Option<String>,
    #[serde(default)]
    pub store: Option<String>,
    #[serde(default)]
    pub policy: Option<String>,
    #[serde(default)]
    pub args: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(default)]
    pub component: Option<String>,
    #[serde(default)]
    pub optional: Option<bool>,
}

/// ops（transform 算子，M1 只解析结构 + 闭环算子名词表；参数闭包 E2209 在后续模块）。
/// 平铺联合体而非内部标签枚举：serde 内部标签枚举不支持 deny_unknown_fields，
/// 平铺保住「全文 deny_unknown_fields」的硬边界（偏差见 RESULT-M1）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpDecl {
    pub op: String,
    #[serde(default)]
    pub port: Option<String>,
    #[serde(default)]
    pub ports: Option<Vec<String>>,
    #[serde(default)]
    pub into: Option<String>,
    #[serde(default)]
    pub fields: Option<Vec<String>>,
    #[serde(default)]
    pub k: Option<u64>,
    #[serde(default)]
    pub drop: Option<Vec<String>>,
    #[serde(default)]
    pub by: Option<String>,
    #[serde(default)]
    pub prefer: Option<String>,
    #[serde(default)]
    pub seed_ref: Option<String>,
    #[serde(default)]
    pub max_tokens_each: Option<u64>,
}

/// ops 闭环算子词表（提案 B §1.3）。
pub const OP_KINDS: [&str; 8] = [
    "project",
    "merge",
    "slice",
    "anonymize",
    "shuffle_seeded",
    "truncate",
    "tally",
    "select_best",
];

/// section 来源闭环词表（提案 B §1.2 context.sections）。
pub const SECTION_FROMS: [&str; 5] = ["template", "input", "arg", "edge", "store"];
