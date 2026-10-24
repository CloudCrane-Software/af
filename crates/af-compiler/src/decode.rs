//! IR 拒绝式信封 decode（`ir_version != 1` / 未知键 / 形状不符 → 整份拒绝）。
//!
//! 纪律承 af-dsl `envelope.rs` 与 af-harness `decode_spec` 双先例：先查版本
//! （缺/非整数/非 1 → 单错短路，body 不解析），版本合法再收未知顶层键，最后
//! 才做 body 形状解析（deny_unknown_fields 全层生效）。全部失败路径点名，
//! 不静默降级。无效 JSON 连文档都不是 → 归并为「缺 ir_version」的退化情形
//! （承 af-dsl `load_str` 对 YAML 语法错误的 E0001 归并，偏差在 RESULT-G2a
//! 登记）。

use crate::ir::{IrProgram, IR_VERSION};
use std::fmt;

/// decode 失败（拒绝式信封四情形 + body 形状）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrParseError {
    /// 顶层缺 `ir_version` 键（或根不是 JSON 对象 / 无效 JSON 的退化归并）。
    MissingVersion,
    /// `ir_version` 非整数。
    VersionNotInteger,
    /// `ir_version` 整数但非 [`IR_VERSION`]。
    VersionNotOne { found: u64 },
    /// 顶层未知键（信封只认 `ir_version` + `body`；字典序首个）。
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
    // ① 根必须是 JSON 对象且含 ir_version（无效 JSON → 退化归并 MissingVersion）。
    let value: serde_json::Value = serde_json::from_str(src).map_err(|_| IrParseError::MissingVersion)?;
    let map = value.as_object().ok_or(IrParseError::MissingVersion)?;

    // ② 版本三查：缺 → 非整数 → 非 1。任一命中即整份拒绝（body 不解析）。
    let version = match map.get("ir_version") {
        None => return Err(IrParseError::MissingVersion),
        Some(v) => v.as_u64().ok_or(IrParseError::VersionNotInteger)?,
    };
    if version != u64::from(IR_VERSION) {
        return Err(IrParseError::VersionNotOne { found: version });
    }

    // ③ 未知顶层键（BTreeMap 迭代 = 字典序，首个未知键点名）。
    for key in map.keys() {
        if !TOP_KEYS.contains(&key.as_str()) {
            return Err(IrParseError::UnknownTopKey { key: key.clone() });
        }
    }

    // ④ body 形状（deny_unknown_fields 全层生效）。
    let body = map.get("body").ok_or_else(|| IrParseError::MalformedBody {
        detail: "缺 body 键".to_string(),
    })?;
    let body: crate::ir::IrBody = serde_json::from_value(body.clone())
        .map_err(|e| IrParseError::MalformedBody { detail: e.to_string() })?;
    Ok(IrProgram { ir_version: IR_VERSION, body })
}
