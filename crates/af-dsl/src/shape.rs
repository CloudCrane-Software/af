//! 形状层：serde 错误 → 稳定码分类（E1001-E1004）。
//! serde 天然 fail-fast（首错即止）；「一次收齐」在语义层（Report 范式）。

use crate::error::{ErrorCode, LoadError, Report};
use crate::model::Document;

/// 信封通过后的 body 反序列化；serde 错误按信息前缀归类到 E1001-E1004。
pub fn deserialize_document(value: &serde_yaml::Value) -> Result<Document, LoadError> {
    let doc: Document = serde_yaml::from_value(value.clone())
        .map_err(|e| LoadError::Shape(classify_serde_error(&e)))?;
    Ok(doc)
}

/// serde_yaml 错误信息 → 稳定码。映射表（RESULT-M1 登记）：
/// `unknown field` → E1001；`missing field` → E1002；
/// `unknown variant` / `invalid value` → E1004；其余（`invalid type`、
/// `data did not match any variant`、自定义类型表达式语法错等）→ E1003。
pub fn classify_serde_error(err: &serde_yaml::Error) -> Report {
    todo!("M1: serde 错误分类")
}

/// 手写闭环词表检查（平铺联合体承载的枚举面）：ops 算子名、section 来源名。
/// 归入语义层 Report（码位属形状域 E1004，由 validate 一并收齐）。
pub fn check_vocab(doc: &Document) -> Vec<crate::error::Violation> {
    todo!("M1: 手写词表检查")
}
