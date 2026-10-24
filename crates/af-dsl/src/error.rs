//! 错误分类学与报告范式（承 poc-cue `loader/src/error.rs` 四层分类学、
//! af-graph `validate.rs` 一次收齐 Report、ADR-007 §3 稳定错误码契约）。
//!
//! 四层：E0 信封（装载入口，body 解析之前）/ E1 形状（serde + 构造词汇表）/
//! E2 语义（引用闭包 + L0 不变量）/ E9 部署期（af-compiler stage 3，本 crate 不产出）。
//! 码表来源：《DSL-语法提案-B》§6（正本）+ 择优裁决 2026-10-24 §二。

/// 稳定错误码。`code()` 是外部契约（测试与日志按此断言），`name()` 是规则名
/// （沿用提案 B §5 预期错误风格的命名，如 `map_shared_store_write`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorCode {
    // ---- E0xxx 信封（整份拒绝，不解析 body）----
    /// E0001 缺 version 键（含根不是映射 / YAML 语法错误两种退化情形）。
    MissingVersion,
    /// E0002 version 非整数（字符串 / 浮点 / 布尔等）。
    VersionNotInteger,
    /// E0003 version 整数但非 1。
    VersionNotOne,
    /// E0004 信封未知键（顶层只认七段 + version）。
    EnvelopeUnknownKey,

    // ---- E1xxx 形状（serde deny_unknown_fields 派生 + 手写词汇表）----
    /// E1001 未知字段（serde `unknown field`）。
    UnknownField,
    /// E1002 缺必填字段（serde `missing field`）。
    MissingField,
    /// E1003 类型不符（serde `invalid type` / 类型表达式语法错 / untagged 失配 /
    /// 端口内联对象 schema——对象 schema 必须住 types 表）。
    TypeMismatch,
    /// E1004 枚举值非法（serde `unknown variant` + 手写闭环词表：ops 算子名、
    /// section 来源名等）。
    InvalidEnumValue,
    /// E1051 CEL 解析失败（含 cel-parser 0.10.1 内部 panic 的 catch_unwind
    /// 兜底归一——尾部运算符/空输入触发 antlr4rust 错误恢复 panic，承 af-graph
    /// `predicate.rs:11-14` 实测注记）。
    CelParse,
    /// E1052 CEL 非保守子集：非算子函数/方法调用（白名单 15 算子，`in` 登记
    /// 为 `@in`——提案 §4.7 实测修正）。
    CelNonSubset,
    /// E1053 CEL 谓词求值非布尔（承 af-graph `PredicateError::NotBoolean`）。
    CelNotBoolean,

    // ---- E2xxx 语义：引用闭包（M1）----
    /// E2001 悬空引用（与 af-graph 共享码位）：节点 / store / 入口 / 写者路径 /
    /// carried.store / writes/reads。
    DanglingReference,
    /// E2002 值域（与 af-graph 共享码位，M1 显式不触发、fix2 起实装）：
    /// loop.max_iterations 非正 / loop.carried 条目无上界（提案 §1.3
    /// 「迭代上限 >0（E2002）」「至少一个上界（E2002）」）。
    ValueOutOfRange,
    /// E2003 重复 id（同作用域节点 id / 类型名 / store 名）。
    DuplicateId,
    /// E2004 空集合（与 af-graph 共享码位，fix2 起实装）：受控循环未声明
    /// 安全点（提案 §1.3「缺安全点=拒绝编译，承 E2004」）。
    EmptyCollection,
    /// E2101 route 出边目标未被任何规则/default 覆盖。
    RouteUncovered,
    /// E2102 route 规则/default 的 goto 不是出边目标也不是 exit。
    RouteGotoDangling,
    /// E2103 route 同一出边目标被多条分支命中（后中者不可达——请合并为
    /// `A || B`）。
    RouteUnreachable,
    /// E2104 map.over 引用可解析但不是 list。
    MapOverNotList,
    /// E2105 loop.args / map.over 的 `节点.端口` / `input.字段` 引用悬空。
    ArgOverRefDangling,
    /// E2106 emits.from_body 路径悬空（体内节点或端口不存在）。
    FromBodyDangling,
    /// E2107 safety_points 路径悬空（从体根逐层解析失败）。
    SafetyPointDangling,
    /// E2201 边端口不存在或两端类型不相等。
    EdgePortTypeMismatch,
    /// E2202 meta.output.from 节点不发射 output.ref 所指类型。
    OutputNotEmitted,
    /// E2203 CEL 未知根绑定（根 ∉ 位置词表——按位点作用域链 + §1.4 词汇表
    /// 建模）。
    CelUnknownRoot,
    /// E2204 failures.route / fallback_route 目标既非关键词也非已声明节点。
    FailureRouteDangling,
    /// E2205 未知类型名（类型名 / list<·> / envelope<·> 的载荷名不在 types 表，
    /// 内建 `signal` 除外）。
    UnknownTypeName,
    /// E2207 预算 cap 非正或作用域目标悬空（plan / node:<id> / loop:<id> / map:<id>）。
    BudgetCapOrScopeInvalid,
    /// E2208 store retention 无上界（max_items / max_bytes 至少其一）。
    StoreRetentionUnbounded,
    /// E2211 loop.carried 未覆盖「体内（含嵌套体）写入的、体外声明的 store」。
    CarriedNotCovering,

    // ---- E2xxx 语义：context.sections 装配 + L0 不变量 INV-1（M2）----
    /// E2301 上下文来源不可解析：input.fields ∉ meta.input schema、edge.port ∉
    /// 本节点入边端口集、store 名不在 stores 表、各 from 缺必配来源字段
    /// （template.template_ref / input.fields / edge.port / store.store / arg.name）。
    ContextSourceUnresolvable,
    /// E2302 配额非正：quota_tokens = 0（例外：store 段 policy:none 记 0 合法）；
    /// 可配置总量上限超限同码（信息注明「总量」，§6 无独立码位——见 RESULT-M2）。
    QuotaNotPositive,
    /// E2303 LLM-bound 节点（uses.model 在场）声明 reads[]（store 只能经
    /// context.sections 进入 agent 上下文）。
    AgentDirectStoreRead,
    /// E2304 入边端口未被 section 恰一个引用（零引用 = 实际消费未声明；
    /// 多引用 = 违反「恰被一个」）。
    InputPortUnreferenced,
    /// E2401 store 视图策略域：store 段缺策略/策略不在词表/参数不闭合
    /// （latest_k·retrieve 须 args.k 正整数；retrieve·summarize 须 component）；
    /// 非 store 段带策略或带不适用策略亦归此码。
    StoreViewPolicyUnknown,
    /// E2402 arg 段 name 不在所在体形参表（沿作用域链向外收集：loop args +
    /// map 隐式 item）。
    ArgNotInParams,
    /// E2403 template 段 vars 引用悬空（input.字段 ∉ meta.input schema、
    /// arg.名 ∉ 可见形参、前缀后空名）。
    TemplateVarsDangling,

    // ---- E2xxx 语义：L0 不变量 INV-2（M1）----
    /// E2311 store 多写者 / 写者集合与声明不符（INV-2 串行化判定）。
    StoreMultiWriter,
    /// E2312 map 体节点写体外声明的 store（N 并行实例 = N 写者）。
    MapSharedStoreWrite,

    // ---- E2xxx 语义：L0 不变量 INV-3/INV-4（M3）----
    /// E2321 自验证：验证面节点（kind ∈ {check, gate} ∪ human 标记）任一入边
    /// 的生产者是它自己（直接自环；间接自验证 v0.1 不查——提案 §8-4）。
    SelfVerification,
    /// E2322 安全点非验证面（§6 已分配码位，fix2 起实装）：loop.safety_points
    /// 的 kind 三值承 af-graph `validate.rs` 语义原样——check→kind=check 节点、
    /// gate→kind=gate 节点、human→human 标记；不符即拒（提案 §5 inv2 夹具）。
    SafetyPointNotVerifier,
    /// E2331 评估输入对候选可读：eval_input store 的读者（reads /
    /// context.sections from:store）∉ {writer, 绑定 uses.unit 的 check 节点}。
    EvalInputVisible,
}

impl ErrorCode {
    /// 稳定错误码字符串（外部契约，禁止改号）。
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingVersion => "E0001",
            Self::VersionNotInteger => "E0002",
            Self::VersionNotOne => "E0003",
            Self::EnvelopeUnknownKey => "E0004",
            Self::UnknownField => "E1001",
            Self::MissingField => "E1002",
            Self::TypeMismatch => "E1003",
            Self::InvalidEnumValue => "E1004",
            Self::CelParse => "E1051",
            Self::CelNonSubset => "E1052",
            Self::CelNotBoolean => "E1053",
            Self::DanglingReference => "E2001",
            Self::ValueOutOfRange => "E2002",
            Self::DuplicateId => "E2003",
            Self::EmptyCollection => "E2004",
            Self::RouteUncovered => "E2101",
            Self::RouteGotoDangling => "E2102",
            Self::RouteUnreachable => "E2103",
            Self::MapOverNotList => "E2104",
            Self::ArgOverRefDangling => "E2105",
            Self::FromBodyDangling => "E2106",
            Self::SafetyPointDangling => "E2107",
            Self::EdgePortTypeMismatch => "E2201",
            Self::OutputNotEmitted => "E2202",
            Self::CelUnknownRoot => "E2203",
            Self::FailureRouteDangling => "E2204",
            Self::UnknownTypeName => "E2205",
            Self::BudgetCapOrScopeInvalid => "E2207",
            Self::StoreRetentionUnbounded => "E2208",
            Self::CarriedNotCovering => "E2211",
            Self::ContextSourceUnresolvable => "E2301",
            Self::QuotaNotPositive => "E2302",
            Self::AgentDirectStoreRead => "E2303",
            Self::InputPortUnreferenced => "E2304",
            Self::StoreViewPolicyUnknown => "E2401",
            Self::ArgNotInParams => "E2402",
            Self::TemplateVarsDangling => "E2403",
            Self::StoreMultiWriter => "E2311",
            Self::MapSharedStoreWrite => "E2312",
            Self::SelfVerification => "E2321",
            Self::SafetyPointNotVerifier => "E2322",
            Self::EvalInputVisible => "E2331",
        }
    }

    /// 规则名（snake_case，与提案 B §5 预期错误风格对齐）。
    pub fn name(&self) -> &'static str {
        match self {
            Self::MissingVersion => "missing_version",
            Self::VersionNotInteger => "version_not_integer",
            Self::VersionNotOne => "version_not_1",
            Self::EnvelopeUnknownKey => "envelope_unknown_key",
            Self::UnknownField => "unknown_field",
            Self::MissingField => "missing_field",
            Self::TypeMismatch => "type_mismatch",
            Self::InvalidEnumValue => "invalid_enum_value",
            Self::CelParse => "cel_parse",
            Self::CelNonSubset => "cel_non_subset",
            Self::CelNotBoolean => "cel_not_boolean",
            Self::DanglingReference => "dangling_reference",
            Self::ValueOutOfRange => "value_out_of_range",
            Self::DuplicateId => "duplicate_id",
            Self::EmptyCollection => "empty_collection",
            Self::RouteUncovered => "route_edge_uncovered",
            Self::RouteGotoDangling => "route_goto_dangling",
            Self::RouteUnreachable => "route_rule_unreachable",
            Self::MapOverNotList => "map_over_not_list",
            Self::ArgOverRefDangling => "arg_over_ref_dangling",
            Self::FromBodyDangling => "from_body_dangling",
            Self::SafetyPointDangling => "safety_point_dangling",
            Self::EdgePortTypeMismatch => "edge_port_type_mismatch",
            Self::OutputNotEmitted => "output_not_emitted",
            Self::CelUnknownRoot => "cel_unknown_root",
            Self::FailureRouteDangling => "failure_route_dangling",
            Self::UnknownTypeName => "unknown_type_name",
            Self::BudgetCapOrScopeInvalid => "budget_cap_or_scope_invalid",
            Self::StoreRetentionUnbounded => "store_retention_unbounded",
            Self::CarriedNotCovering => "carried_not_covering",
            Self::ContextSourceUnresolvable => "context_source_unresolvable",
            Self::QuotaNotPositive => "quota_not_positive",
            Self::AgentDirectStoreRead => "agent_direct_store_read",
            Self::InputPortUnreferenced => "input_port_unreferenced",
            Self::StoreViewPolicyUnknown => "store_view_policy_unknown",
            Self::ArgNotInParams => "arg_not_in_params",
            Self::TemplateVarsDangling => "template_vars_dangling",
            Self::StoreMultiWriter => "store_multi_writer",
            Self::MapSharedStoreWrite => "map_shared_store_write",
            Self::SelfVerification => "self_verification",
            Self::SafetyPointNotVerifier => "safety_point_not_verifier",
            Self::EvalInputVisible => "eval_input_visible",
        }
    }
}

/// 一条违例：稳定码 + scope（根图或哪层体）+ 精确信息（ADR-007 §3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub code: ErrorCode,
    pub scope: String,
    pub message: String,
}

impl Violation {
    pub fn new(code: ErrorCode, scope: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            scope: scope.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}[{}]: {}",
            self.code.code(),
            self.code.name(),
            self.scope,
            self.message
        )
    }
}

/// 校验报告：全部违例的集合，一次收齐不停第一条（承 af-graph `validate.rs`）。
/// 插入序 = 校验器遍历序（全链路 BTree*/文档序，确定性）；不重排。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub violations: Vec<Violation>,
}

impl Report {
    pub fn is_ok(&self) -> bool {
        self.violations.is_empty()
    }

    pub fn push(&mut self, code: ErrorCode, scope: impl Into<String>, message: impl Into<String>) {
        self.violations.push(Violation::new(code, scope, message));
    }

    pub fn has_code(&self, code: ErrorCode) -> bool {
        self.violations.iter().any(|v| v.code == code)
    }

    /// 全部违例的错误码（文档序），测试按此断言「精确码集」。
    pub fn codes(&self) -> Vec<&'static str> {
        self.violations.iter().map(|v| v.code.code()).collect()
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_ok() {
            write!(f, "OK")
        } else {
            for v in &self.violations {
                writeln!(f, "{v}")?;
            }
            Ok(())
        }
    }
}

/// 装载失败的三层出口（承提案 B §3.1 `LoadError` 草图；E9 部署期由
/// af-compiler 产出，本 crate 永远不会产出 Deploy 变体）。
/// 信封失败 = 整份拒绝、body 未解析；形状失败 = serde 首错（serde 天然
/// fail-fast，只有语义层做一次收齐——见 RESULT-M1 偏差表）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// E0001-E0004：拒绝式版本信封，body 不解析。
    Envelope(Report),
    /// E1001-E1004：serde 形状（deny_unknown_fields / 枚举词表 / 类型表达式）。
    Shape(Report),
    /// E2xxx：语义层（引用闭包 + INV-2，一次收齐）。
    Semantic(Report),
}

impl LoadError {
    pub fn report(&self) -> &Report {
        match self {
            Self::Envelope(r) | Self::Shape(r) | Self::Semantic(r) => r,
        }
    }

    pub fn codes(&self) -> Vec<&'static str> {
        self.report().codes()
    }
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Envelope(_) => write!(f, "envelope rejected:{}", self.report()),
            Self::Shape(_) => write!(f, "shape rejected:{}", self.report()),
            Self::Semantic(_) => write!(f, "semantic rejected:{}", self.report()),
        }
    }
}

impl std::error::Error for LoadError {}
