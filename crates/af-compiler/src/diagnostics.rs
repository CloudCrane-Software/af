//! 机读诊断产物面（schema `af.diagnostics.v1`，G2）。
//!
//! **缺口背景**：编译器诊断（af-dsl E0xxx/E1xxx/E2xxx 码族 + 本 crate
//! E900x 绑定码族）此前只出现在人类可读的文本/错误链里——CI、编辑器、
//! 外部对账器拿不到结构化判定（退出码/文本不可单独作判据，与 all-gates
//! 侧「反馈梯度」批解决的是同一个病）。本模块只是**多一个出口**：
//! 把既有诊断投影成稳定 schema 的 JSON 产物落盘。
//!
//! **纪律（逐条对应任务书硬约束）**：
//!
//! - **码从哪来**：`Diagnostic::code` 逐字取自既有稳定码方法——af-dsl
//!   [`ErrorCode::code`](af_dsl::ErrorCode::code)（E0001-E0004 / E1xxx /
//!   E2xxx 码族）与本 crate
//!   [`BindCode::as_str`](crate::BindCode::as_str)（E9001-E9003）。
//!   本模块**零新码**：无既有码的失败面（组件清单装载失败
//!   `ComponentRegistryError`、lowering 内部拒绝 `LowerError`、序列化
//!   不可达错误）**不入产物**，仍走 stderr + 退出码（码位是否补登记
//!   是人类裁决项，见 PoC README 边界节）。
//! - **判定语义零弱化**：诊断的产生条件、拒绝语义、码值、message 逐字
//!   不动——本模块只做投影，不做任何判定。
//! - **不许编造位置**：`location` 能给就给（v0.1：CLI 面已知源文档路径）；
//!   行/列在 af-dsl 校验器**无 span 记账**的前提下恒 `null`（明确缺省，
//!   非省略键）——不造位置。
//! - **确定性**：条目序 = 上游产生序（af-dsl 校验器文档序 / bind 的
//!   BTreeMap 确定序）；同输入同产物逐位一致。
//!
//! **出口**：CLI `af-compiler-plan --diagnostics-out <path>`（见
//! `src/bin/af-compiler-plan.rs`）；库面 [`DiagnosticsFile::to_json`]
//! 供测试与嵌入方复用。

use crate::bind::BindViolation;
use af_dsl::{LoadError, Report};
use serde::{Deserialize, Serialize};

/// 产物 schema 标识（顶层 `schema` 字段值；向后兼容的演进须升版本号）。
pub const DIAGNOSTICS_SCHEMA: &str = "af.diagnostics.v1";

// ============================================================
// 枚举面
// ============================================================

/// 诊断严重度。v0.1 既有码族全部是 fail-loud 拒绝语义 → 唯一值 `error`
/// （字段位先占住：将来若有 warning 级诊断**加值不改名**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// 拒绝性诊断（产生即管线拒绝，退出码非零）。
    Error,
}

/// 诊断产生阶段（管线位面）。与 `af-compiler-plan` 的失败路径逐面对应：
/// `load` = 装载拒绝（信封 E0xxx / 形状 E1xxx，`af_dsl::load_str`）；
/// `validate` = 语义校验违例（E2xxx 族，`af_dsl::validate[_with]`）；
/// `bind` = 组件绑定违例（E900x 族，stage 3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// stage 1 装载：信封/形状拒绝（E0001-E0004 / E1xxx）。
    Load,
    /// stage 1 校验：语义层一次收齐的违例（E2xxx 族）。
    Validate,
    /// stage 3 绑定：uses.* × 注册表对照违例（E900x 族）。
    Bind,
}

// ============================================================
// 产物结构
// ============================================================

/// 诊断位置。**不许编造**：只有真实已知的定位字段才非 `null`——v0.1
/// `file` = 源文档路径（CLI 面已知）；`line`/`column` 在 af-dsl 校验器
/// 无 span 记账的前提下恒 `null`（明确缺省，键恒在场）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagLocation {
    /// 源文档路径（调用方给出的输入路径原样）。
    pub file: String,
    /// 行号；无 span 源 → `null`（不编造）。
    pub line: Option<u32>,
    /// 列号；无 span 源 → `null`（不编造）。
    pub column: Option<u32>,
}

/// 一条机读诊断（schema `af.diagnostics.v1` 条目）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// 稳定码——逐字取自既有 `code()`/`as_str()`（E0001-E0004 / E1xxx /
    /// E2xxx / E9001-E9003）。本面零新码。
    pub code: String,
    /// 严重度（v0.1 恒 `error`——既有码族全部 fail-loud）。
    pub severity: Severity,
    /// 精确信息——上游 `Violation::message` / `BindViolation::message`
    /// 逐字（人类可读出口的 message 段同一份，不改写）。
    pub message: String,
    /// 产生阶段（load / validate / bind）。
    pub phase: Phase,
    /// 位置（能定位就给；给不了的字段明确 `null`，不编造）。
    pub location: Option<DiagLocation>,
    /// 相关对象标识——诊断命中的图内主体：E900x = 节点点号路径（如
    /// `worker`）；af-dsl = `Violation::scope`（`root` /
    /// `<路径>.loop_body` / `<路径>.map_body`，与 validate 索引一致）。
    /// 非图内主体的诊断（无此标识）→ `null`。
    pub node: Option<String>,
    /// 相关对象标识（E900x）——命中的组件引用串原样（如
    /// `pkg.kit.hammer@1`）。非绑定面诊断 → `null`。
    pub reference: Option<String>,
    /// 规则名（af-dsl `ErrorCode::name()`——snake_case 规则名，**非码**，
    /// 与 `code` 并列登机读面方便检索）。非 af-dsl 诊断 → `null`。
    pub rule: Option<String>,
}

/// 产物顶层：`{"schema": "af.diagnostics.v1", "diagnostics": [...]}`。
/// 空数组 = 本次运行无（已编码的）诊断——**不等于运行成功**：无码的
/// 操作性失败（注册表装载失败等）同样落空数组，成败仍以退出码为准。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsFile {
    /// 恒 [`DIAGNOSTICS_SCHEMA`]。
    pub schema: String,
    /// 诊断条目（产生序，确定性）。
    pub diagnostics: Vec<Diagnostic>,
}

impl DiagnosticsFile {
    /// 空产物（成功路径 / 无已编码诊断的路径）。
    pub fn empty() -> Self {
        Self {
            schema: DIAGNOSTICS_SCHEMA.to_string(),
            diagnostics: Vec::new(),
        }
    }

    /// 规范序列化（pretty，缩进 2，尾随换行）——确定性：同产物逐位一致。
    pub fn to_json(&self) -> String {
        let mut s = serde_json::to_string_pretty(self).expect("产物 schema 恒可序列化");
        s.push('\n');
        s
    }

    /// 落盘（父目录须已存在；整份覆盖写）。CLI `--diagnostics-out` 的
    /// 出口本体。
    pub fn write_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        std::fs::write(path, self.to_json())
    }
}

// ============================================================
// 既有诊断面 → 机读投影（纯函数，零判定）
// ============================================================

/// af-dsl 装载拒绝（信封 E0xxx / 形状 E1xxx / 语义 E2xxx——三个变体
/// 同出 `load_str` 一个口）→ 机读条目，`phase = load`。
/// 码/信息逐字：`code()` 与 `Violation::message` 原样；`rule` 登
/// `name()`；`node` = scope；`reference` 恒 `None`（非绑定面）。
pub fn from_load_error(err: &LoadError) -> Vec<Diagnostic> {
    from_report(err.report(), Phase::Load)
}

/// af-dsl 校验报告（`validate` / `validate_with` 一次收齐的 E2xxx 族）
/// → 机读条目，`phase = validate`。同上逐字投影。
pub fn from_validate_report(report: &Report) -> Vec<Diagnostic> {
    from_report(report, Phase::Validate)
}

/// 报告投影共核：`code()` / `name()` / `message` / `scope` 原样进结构化
/// 字段（人类可读出口的 `Display` 组装段拆开登位，不改写任何一段）。
fn from_report(report: &Report, phase: Phase) -> Vec<Diagnostic> {
    report
        .violations
        .iter()
        .map(|v| Diagnostic {
            code: v.code.code().to_string(),
            severity: Severity::Error,
            message: v.message.clone(),
            phase,
            location: None,
            node: Some(v.scope.clone()),
            reference: None,
            rule: Some(v.code.name().to_string()),
        })
        .collect()
}

/// E900x 绑定违例（`BindReport::violations`，或 `PlanError::Unbound`
/// 携带的同一份）→ 机读条目，`phase = bind`。码/信息逐字：
/// `BindCode::as_str()` 与 `BindViolation::message` 原样；`node` =
/// 节点点号路径、`reference` = 引用串（相关对象标识齐两面）。
pub fn from_bind_violations(violations: &[BindViolation]) -> Vec<Diagnostic> {
    violations
        .iter()
        .map(|v| Diagnostic {
            code: v.code.as_str().to_string(),
            severity: Severity::Error,
            message: v.message.clone(),
            phase: Phase::Bind,
            location: None,
            node: Some(v.node.clone()),
            reference: Some(v.reference.clone()),
            rule: None,
        })
        .collect()
}

/// 给条目盖源文档位置（CLI 面已知输入路径；行/列无 span 源 → `null`）。
/// 无位置条目（`location = None`）盖成 `file` 已知、行/列缺省——不编造。
pub fn stamp_source_file(diags: &mut [Diagnostic], file: &str) {
    for d in diags.iter_mut() {
        if d.location.is_none() {
            d.location = Some(DiagLocation {
                file: file.to_string(),
                line: None,
                column: None,
            });
        }
    }
}
