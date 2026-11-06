//! af-graph — 图模板（Task Graph Template）schema 与编译期校验器原型。
//!
//! 设计依据：《Agent工厂设计_v2.md》§2.1.3（Task Graph + map 原语）、修正案 7
//! （graph_version 一等键）、§五（人工工位一等节点）、P0 清单（图模板 schema +
//! 编译期检查器，蓝本 Flyte workflow_compiler）。
//!
//! 拓扑纪律：**静态图必须是 DAG**。迭代不靠环，靠一等"受控循环"构造
//! （循环体=独立子图 + 安全点 + 有界）；动态 fan-out 靠一等"map"构造
//! （遍历工件集合，运行时定基数，子图必须已版本化）。

pub mod model;
pub mod predicate;
pub mod validate;

pub use model::*;
pub use predicate::{Predicate, PredicateError};
pub use validate::{
    format_validate_stdout, validate, ErrorCode, Report, Rule, ValidateDecision,
    ValidateDecisionViolation, Violation,
};
