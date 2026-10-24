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

    // ---- E2xxx 语义：引用闭包（M1）----
    /// E2001 悬空引用（与 af-graph 共享码位）：节点 / store / 入口 / 写者路径 /
    /// carried.store / writes/reads。
    DanglingReference,
    /// E2003 重复 id（同作用域节点 id / 类型名 / store 名）。
    DuplicateId,
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

    // ---- E2xxx 语义：L0 不变量 INV-2（M1）----
    /// E2311 store 多写者 / 写者集合与声明不符（INV-2 串行化判定）。
    StoreMultiWriter,
    /// E2312 map 体节点写体外声明的 store（N 并行实例 = N 写者）。
    MapSharedStoreWrite,
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
            Self::DanglingReference => "E2001",
            Self::DuplicateId => "E2003",
            Self::MapOverNotList => "E2104",
            Self::ArgOverRefDangling => "E2105",
            Self::FromBodyDangling => "E2106",
            Self::SafetyPointDangling => "E2107",
            Self::EdgePortTypeMismatch => "E2201",
            Self::OutputNotEmitted => "E2202",
            Self::FailureRouteDangling => "E2204",
            Self::UnknownTypeName => "E2205",
            Self::BudgetCapOrScopeInvalid => "E2207",
            Self::StoreRetentionUnbounded => "E2208",
            Self::CarriedNotCovering => "E2211",
            Self::StoreMultiWriter => "E2311",
            Self::MapSharedStoreWrite => "E2312",
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
            Self::DanglingReference => "dangling_reference",
            Self::DuplicateId => "duplicate_id",
            Self::MapOverNotList => "map_over_not_list",
            Self::ArgOverRefDangling => "arg_over_ref_dangling",
            Self::FromBodyDangling => "from_body_dangling",
            Self::SafetyPointDangling => "safety_point_dangling",
            Self::EdgePortTypeMismatch => "edge_port_type_mismatch",
            Self::OutputNotEmitted => "output_not_emitted",
            Self::FailureRouteDangling => "failure_route_dangling",
            Self::UnknownTypeName => "unknown_type_name",
            Self::BudgetCapOrScopeInvalid => "budget_cap_or_scope_invalid",
            Self::StoreRetentionUnbounded => "store_retention_unbounded",
            Self::CarriedNotCovering => "carried_not_covering",
            Self::StoreMultiWriter => "store_multi_writer",
            Self::MapSharedStoreWrite => "map_shared_store_write",
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
