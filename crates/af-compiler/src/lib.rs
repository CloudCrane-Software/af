//! af-compiler：AFB（Agent Flow Blueprint）四阶段编译管线（ADR-002 crate 职责）。
//!
//! - 阶段 1 · 校验：`af-dsl::validate`（G1/M1-M4，前置 crate）。
//! - 阶段 2 · IR（G2a）：schema v1（`ir`）+ 确定性 lowering（`lower`）+
//!   拒绝式信封 decode（`decode`）+ 快照入口（`snapshot_json` / CLI
//!   `af-compiler-snapshot`）。拓扑即 IR：内联 loop/map 体降维为命名子图，
//!   route 归一化为边守卫，全链路 BTreeMap 确定序。
//! - **G2b（本模块期）**：阶段 3 · 组件绑定（`components` 注册表装载 +
//!   `bind`——uses.* 引用 × 注册表对照，E900x fail-loud，逃逸注入点）与
//!   阶段 4 · 计划生成（`plan`——绑定图 → 可执行计划 JSON，落地契约 §7.1
//!   草案顶层结构，确定性同 G2a 纪律；CLI `af-compiler-plan` 全链入口）。
//!
//! 权威输入：《DSL-语法提案-B》§3（四阶段类型草图与确定性要点）+ ADR-010
//! §2/§5（保证边界与四一等字段）+ 落地契约-v0 §2/§7（组件注册与计划对象）。
//!
//! 交付文档：`docs/保证边界矩阵.md`、`docs/执行语义规范-v0.md`。

pub mod bind;
pub mod components;
pub mod decode;
pub mod ir;
pub mod lower;
pub mod plan;

pub use bind::{
    bind, BindCode, BindReport, BindViolation, BoundAgent, BoundComponents, BoundNodeExec,
    ResolvedComponent,
};
pub use components::{
    ComponentClass, ComponentDecl, ComponentRegistry, ComponentRegistryError, ComponentsFile,
    EndpointSpec,
};
pub use decode::{decode_ir, IrParseError};
pub use ir::{IrProgram, IR_VERSION};
pub use lower::{lower, LowerError};
pub use plan::{
    decode_plan, generate_plan, plan_json, plan_json_pretty, ExecutablePlan, PlanError,
    PlanParseError, PLAN_VERSION,
};

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
