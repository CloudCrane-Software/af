//! 拒绝式版本信封。
//!
//! 先看 `version`，不是整数 1 就不反序列化其余字段。缺键是 DI1005，
//! 其他写法（字符串、小数、0、2、负数）是 DI1006。

use crate::error::{ErrorCode, IrError, IrFailure};
use crate::model::Program;
use crate::IR_VERSION;
use serde_json::Value;

/// 解析 IR JSON。结构通过之后还会做语义校验。
pub fn parse_program(text: &str) -> Result<Program, IrFailure> {
    let value: Value = serde_json::from_str(text).map_err(|err| {
        IrFailure::one(IrError::new(
            ErrorCode::Schema,
            format!("IR 不是 JSON: {err}"),
        ))
    })?;
    parse_program_value(&value)
}

pub(crate) fn parse_program_value(value: &Value) -> Result<Program, IrFailure> {
    read_version(value).map_err(IrFailure::one)?;
    let program: Program = serde_json::from_value(value.clone()).map_err(map_serde)?;
    program.validate()?;
    Ok(program)
}

/// 校验后写出 IR JSON。非法程序不落成文档。
pub fn to_json(program: &Program) -> Result<String, IrFailure> {
    program.validate()?;
    serde_json::to_string(program).map_err(|err| {
        IrFailure::one(IrError::new(
            ErrorCode::Schema,
            format!("IR 无法序列化: {err}"),
        ))
    })
}

fn read_version(value: &Value) -> Result<u32, IrError> {
    let Some(obj) = value.as_object() else {
        return Err(IrError::new(ErrorCode::Schema, "IR 文档必须是 JSON 对象"));
    };
    let Some(raw) = obj.get("version") else {
        return Err(IrError::new(ErrorCode::MissingVersion, "缺少 version 字段"));
    };
    match raw {
        Value::Number(number) => {
            let Some(wide) = number.as_u64() else {
                return Err(IrError::new(
                    ErrorCode::UnknownVersion,
                    format!("无法识别的 version: {raw}"),
                ));
            };
            let Ok(version) = u32::try_from(wide) else {
                return Err(IrError::new(
                    ErrorCode::UnknownVersion,
                    format!("无法识别的 version: {raw}"),
                ));
            };
            if version != IR_VERSION {
                return Err(IrError::new(
                    ErrorCode::UnknownVersion,
                    format!("未知 version {version}，只接受 {IR_VERSION}"),
                ));
            }
            Ok(version)
        }
        other => Err(IrError::new(
            ErrorCode::UnknownVersion,
            format!("无法识别的 version: {other}"),
        )),
    }
}

fn map_serde(err: serde_json::Error) -> IrFailure {
    let message = err.to_string();
    let code = if message.contains("missing field") {
        ErrorCode::MissingField
    } else {
        ErrorCode::Schema
    };
    IrFailure::one(IrError::new(code, message))
}
