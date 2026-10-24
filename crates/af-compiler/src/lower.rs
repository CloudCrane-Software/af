//! 确定性 lowering：`af-dsl` AST（**已过全量校验的** `Document`）→ IR v1。
//!
//! 纯函数、无 I/O、无时钟/随机/网络（§3.5-4）。同一份 `Document` 两次
//! lowering 产出逐位相同的序列化 IR（确定性测试逐字断言）。
//!
//! 输入契约：`Document` 已过 `af_dsl::validate` 全量校验（M1-M4）。lowering
//! 不做二次校验，只对「不一致即不可能出现在已校验文档中」的情形返回
//! [`LowerError`]（绝不 panic）。
//!
//! **不入 IR 的两个 DSL 面**（有意的，非丢失）：
//! - `Document.version`：DSL 侧信封（E0003 已判 =1）；IR 自带 `ir_version`。
//! - `meta.topology_selector`：编译期解析语义（裁决 §二-1）——只决定编译产物
//!   采用哪个候选图，解析为单一确定图后**不进运行时**，v0.1 单文档无包产物
//!   声明面，故无 IR 落点。

use crate::ir::IrProgram;
use std::fmt;

/// lowering 失败：输入违反「已过全量校验」前提（stage 1 的 E21xx/E1007 面）
/// 或序列化不可达错误。已校验文档恒 `Ok`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LowerError {
    /// 构造体为空（`entry = 体 nodes 首元素` 无从取）。af-dsl M1 不做空体
    /// 检查（E1007 留待构造形状模块，`model.rs:450` 注记）——lowering 以错误
    /// 报告代替 panic。
    EmptyBody { construct: String },
    /// route 归一化违例（E2101/E2102/E2103 在 stage 1 已判——命中即说明输入
    /// 未过校验）。`issues` 为逐条违例的显示串。
    RouteUnnormalized { node: String, issues: Vec<String> },
    /// CEL 编译失败（E1051 已判面；含 cel-parser 内部 panic 的 catch_unwind
    /// 兜底归一——承 af-graph `predicate.rs:11-14` 纪律）。
    CelCompile { source: String, detail: String },
    /// IR 序列化失败（serde_json 对本 schema 理论不可达）。
    Serialize(String),
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyBody { construct } => {
                write!(f, "构造 {construct:?} 的体为空（entry 无从取；E1007 面）")
            }
            Self::RouteUnnormalized { node, issues } => {
                write!(f, "节点 {node:?} route 归一化违例（输入未过 stage 1 校验）：{issues:?}")
            }
            Self::CelCompile { source, detail } => {
                write!(f, "CEL 编译失败（输入未过 stage 1 校验）{source:?}：{detail}")
            }
            Self::Serialize(detail) => write!(f, "IR 序列化失败：{detail}"),
        }
    }
}

impl std::error::Error for LowerError {}

/// `Document` → `IrProgram`（纯函数）。
pub fn lower(doc: &af_dsl::Document) -> Result<IrProgram, LowerError> {
    let _ = doc;
    // G2a RED：先红后绿（ADR-007）。实现于 GREEN 提交。
    unimplemented!("G2a RED：lowering 待实现")
}
