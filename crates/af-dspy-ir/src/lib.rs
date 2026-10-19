//! af-dspy-ir — DSPy 程序的 IR schema v1。
//!
//! 这是 L2 适配器侧的 schema 件，不是内核抽象。
//!
//! DSPy `Signature.dump_state` 的 `fields` 只有 `prefix` 和 `description`，没有字段名；
//! `load_state` 按位置套回调用方手里的签名。`Signature.delete` 对不存在的名字是
//! `fields.pop(name, None)`，不报错。本 crate 把带字段名的 IR 当作真相源：
//! 缺字段名、坏引用、删除不存在的字段都显式失败。
//!
//! 文档是拒绝式信封。没有 `version`，或 `version` 不是整数 `1`，整份拒绝，不默认成 v1。
//! 字段清单见 [`V1_FIELDS`]。

mod error;
mod fields;
mod model;
mod parse;
mod validate;

pub use error::{ErrorCode, IrError, IrFailure};
pub use fields::{FieldSpec, V1_FIELDS};
pub use model::{
    Demo, Field, LmConfig, OptimizationState, OptimizationStatus, Predictor, PredictorConfig,
    Program, Signature,
};
pub use parse::{parse_program, to_json};

/// 当前 IR 文档版本。信封里的 `version` 只能是这个数。
pub const IR_VERSION: u32 = 1;
