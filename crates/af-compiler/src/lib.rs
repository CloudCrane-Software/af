//! af-compiler：AFB（Agent Flow Blueprint）四阶段编译管线（ADR-002 crate 职责）。
//!
//! - **G2a（本模块期）**：阶段 2 · IR——schema v1（`ir`）+ 确定性 lowering
//!   （`lower`）+ 拒绝式信封 decode（`decode`）+ 快照入口（本文件
//!   `snapshot_json` / CLI `af-compiler-snapshot`）。拓扑即 IR：内联 loop/map
//!   体降维为命名子图，route 归一化为边守卫，全链路 BTreeMap 确定序。
//! - 阶段 3 · 组件绑定（`bind`）与阶段 4 · 计划生成（`plan`）：G2b，未建。
//!
//! 权威输入：《DSL-语法提案-B》§3（四阶段类型草图与确定性要点）+ ADR-010
//! §2/§5（保证边界与四一等字段）+ 落地契约-v0 §7（IR 为 stage3/4 留位）。
//!
//! 交付文档：`docs/保证边界矩阵.md`、`docs/执行语义规范-v0.md`。

pub mod decode;
pub mod ir;
pub mod lower;

pub use decode::{decode_ir, IrParseError};
pub use ir::{IrProgram, IR_VERSION};
pub use lower::{lower, LowerError};

/// `lower + serialize`：IR 规范序列化（compact，键序=BTree 序）——确定性
/// 快照，供 G3 golden 入库与逐位一致性断言。
pub fn snapshot_json(doc: &af_dsl::Document) -> Result<String, LowerError> {
    let program = lower(doc)?;
    serde_json::to_string(&program).map_err(|e| LowerError::Serialize(e.to_string()))
}

/// `lower + serialize`（pretty，缩进 2）——golden 入库的人读形态；与
/// compact 形态同样确定（同输入逐位一致）。
pub fn snapshot_json_pretty(doc: &af_dsl::Document) -> Result<String, LowerError> {
    let program = lower(doc)?;
    serde_json::to_string_pretty(&program).map_err(|e| LowerError::Serialize(e.to_string()))
}
