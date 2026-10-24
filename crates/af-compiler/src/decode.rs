//! IR 拒绝式信封 decode（`ir_version != 1` / 未知键 / 形状不符 → 整份拒绝）。
//!
//! 纪律承 af-dsl `envelope.rs` 与 af-harness `decode_spec` 双先例：先查版本
//! （缺/非整数/非 1 → 单错短路，body 不解析），版本合法再收未知顶层键，最后
//! 才做 body 形状解析（deny_unknown_fields 全层生效）。全部失败路径点名，
//! 不静默降级。

use crate::ir::{IrProgram, IR_VERSION};
use std::fmt;

/// decode 失败（拒绝式信封四情形 + body 形状）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrParseError {
    /// 顶层缺 `ir_version` 键（或根不是 JSON 对象）。
    MissingVersion,
    /// `ir_version` 非整数。
    VersionNotInteger,
    /// `ir_version` 整数但非 [`IR_VERSION`]。
    VersionNotOne { found: u64 },
    /// 顶层未知键（信封只认 `ir_version` + `body`）。
    UnknownTopKey { key: String },
    /// body 形状不符（serde `deny_unknown_fields` / 类型错 / 缺字段）。
    MalformedBody { detail: String },
}

impl fmt::Display for IrParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingVersion => write!(f, "IR 信封缺 ir_version 键"),
            Self::VersionNotInteger => write!(f, "IR 信封 ir_version 非整数"),
            Self::VersionNotOne { found } => {
                write!(f, "IR 信封 ir_version={found} 非 {IR_VERSION}（拒绝式：整份拒绝）")
            }
            Self::UnknownTopKey { key } => {
                write!(f, "IR 信封未知顶层键 {key:?}（只认 ir_version + body）")
            }
            Self::MalformedBody { detail } => write!(f, "IR body 形状不符：{detail}"),
        }
    }
}

impl std::error::Error for IrParseError {}

/// 信封合法的顶层键。
pub const TOP_KEYS: [&str; 2] = ["ir_version", "body"];

/// 从 JSON 串解码 IR（拒绝式信封）。
pub fn decode_ir(src: &str) -> Result<IrProgram, IrParseError> {
    let _ = (src, IR_VERSION);
    // G2a RED：先红后绿（ADR-007）。实现于 GREEN 提交。
    unimplemented!("G2a RED：decode_ir 待实现")
}
