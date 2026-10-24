//! af-dsl：AFB（Agent Flow Blueprint）DSL 前端——信封 + 七段解析 + 静态校验。
//!
//! 施工按 ADR-002 crate 职责与裁决书 §四模块切分：
//! - M1（本模块期）：拒绝式信封 E0001-E0004；七段 AST（全文
//!   `deny_unknown_fields`，构造字段解析为结构体、语义校验留待后续模块）；
//!   引用完整性（类型/节点/store/端口）；INV-2 写入串行化编译期判定；
//!   语义层一次收齐（Report 范式，承 af-graph `validate.rs`）。
//! - M2-M4（context.sections+INV-1 / INV-3+INV-4 / route 归一化+CEL+topology_selector）
//!   不在本模块期范围。
//!
//! 权威语法规范：《DSL-语法提案-B》（正本）+ 择优裁决 2026-10-24 §二。

pub mod envelope;
pub mod error;
pub mod model;
pub mod shape;
pub mod validate;

pub use error::{ErrorCode, LoadError, Report, Violation};
pub use model::Document;

/// 解析并做全部 M1 校验。信封失败 → 整份拒绝（body 不解析）；
/// 形状失败 → serde 首错；语义失败 → 一次收齐的 Report。
pub fn load_str(src: &str) -> Result<Document, LoadError> {
    let value: serde_yaml::Value = match serde_yaml::from_str(src) {
        Ok(v) => v,
        // YAML 语法错误：连文档都不是 → 视作「缺 version」的退化情形（E0001），
        // 归入信封层拒绝。码表无独立码位，此归并在 RESULT-M1 偏差表登记。
        Err(e) => {
            return Err(LoadError::Envelope({
                let mut r = Report::default();
                r.push(
                    ErrorCode::MissingVersion,
                    "envelope",
                    format!("YAML syntax error: {e}"),
                );
                r
            }))
        }
    };
    envelope::check(&value)?;
    let doc: Document = shape::deserialize_document(&value)?;
    Ok(doc)
}

/// 语义层校验（引用闭包 + INV-2），一次收齐。签名承提案 B §3.1 草图。
pub fn validate(doc: &Document) -> Report {
    validate::validate(doc)
}

/// 带可配置项的语义校验（M2 起：context 配额总量上限等）。
pub use validate::validate_with;

/// 语义校验可配置项。
pub use validate::ValidateOptions;
