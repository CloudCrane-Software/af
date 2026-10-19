//! 语义校验。serde 负责键和类型，这里负责空名、重复、引用和取值。
//!
//! 一次收齐全部违例，不在第一条停住。

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{ErrorCode, IrError, IrFailure};
use crate::model::{blank, Program};
use crate::IR_VERSION;

impl Program {
    /// 检查这份程序能否当作 IR 真相源。
    pub fn validate(&self) -> Result<(), IrFailure> {
        let mut errors = Vec::new();
        if self.version != IR_VERSION {
            errors.push(IrError::new(
                ErrorCode::UnknownVersion,
                format!("未知 version {}，只接受 {IR_VERSION}", self.version),
            ));
        }
        if blank(&self.program_id) {
            errors.push(IrError::new(ErrorCode::EmptyIdentifier, "program_id 为空"));
        }

        let mut signature_names: BTreeSet<&str> = BTreeSet::new();
        let mut fields_by_signature: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for signature in &self.signatures {
            if blank(&signature.name) {
                errors.push(IrError::new(ErrorCode::EmptyIdentifier, "签名名为空"));
                continue;
            }
            if !signature_names.insert(signature.name.as_str()) {
                errors.push(IrError::new(
                    ErrorCode::DuplicateId,
                    format!("签名名 `{}` 重复", signature.name),
                ));
            }
            let mut field_names = BTreeSet::new();
            for field in signature.ordered_fields() {
                if blank(&field.name) {
                    errors.push(IrError::new(
                        ErrorCode::MissingFieldName,
                        format!("签名 `{}` 的字段名为空", signature.name),
                    ));
                    continue;
                }
                if !field_names.insert(field.name.as_str()) {
                    errors.push(IrError::new(
                        ErrorCode::DuplicateId,
                        format!("签名 `{}` 的字段名 `{}` 重复", signature.name, field.name),
                    ));
                }
                if blank(&field.type_name) {
                    errors.push(IrError::new(
                        ErrorCode::InvalidValue,
                        format!(
                            "签名 `{}` 的字段 `{}` 缺少 type_name",
                            signature.name, field.name
                        ),
                    ));
                }
            }
            fields_by_signature.insert(signature.name.as_str(), field_names);
        }

        let mut demo_ids: BTreeSet<&str> = BTreeSet::new();
        let mut demo_signatures: BTreeMap<&str, &str> = BTreeMap::new();
        for demo in &self.demos {
            if blank(&demo.id) {
                errors.push(IrError::new(ErrorCode::EmptyIdentifier, "demo id 为空"));
            } else if !demo_ids.insert(demo.id.as_str()) {
                errors.push(IrError::new(
                    ErrorCode::DuplicateId,
                    format!("demo id `{}` 重复", demo.id),
                ));
            } else {
                demo_signatures.insert(demo.id.as_str(), demo.signature.as_str());
            }

            if blank(&demo.signature) {
                errors.push(IrError::new(
                    ErrorCode::EmptyIdentifier,
                    format!("demo `{}` 的签名名为空", demo.id),
                ));
                continue;
            }
            let Some(field_names) = fields_by_signature.get(demo.signature.as_str()) else {
                errors.push(IrError::new(
                    ErrorCode::UnknownSignature,
                    format!("demo `{}` 引用了不存在的签名 `{}`", demo.id, demo.signature),
                ));
                continue;
            };
            let mut seen_values: BTreeSet<&str> = BTreeSet::new();
            for key in demo.values.keys() {
                if blank(key) {
                    errors.push(IrError::new(
                        ErrorCode::MissingFieldName,
                        format!("demo `{}` 的值用了空字段名", demo.id),
                    ));
                    continue;
                }
                if !field_names.contains(key.as_str()) {
                    errors.push(IrError::new(
                        ErrorCode::UnknownField,
                        format!(
                            "demo `{}` 的字段 `{key}` 不在签名 `{}` 上",
                            demo.id, demo.signature
                        ),
                    ));
                    continue;
                }
                seen_values.insert(key.as_str());
            }
            for required in field_names {
                if !seen_values.contains(required) {
                    errors.push(IrError::new(
                        ErrorCode::IncompleteDemo,
                        format!(
                            "demo `{}` 缺少签名 `{}` 的字段 `{required}`",
                            demo.id, demo.signature
                        ),
                    ));
                }
            }
        }

        let mut predictor_names: BTreeSet<&str> = BTreeSet::new();
        for predictor in &self.predictors {
            if blank(&predictor.name) {
                errors.push(IrError::new(ErrorCode::EmptyIdentifier, "predictor 名为空"));
            } else if predictor.name == "metadata" {
                errors.push(IrError::new(
                    ErrorCode::InvalidValue,
                    "predictor 名 `metadata` 与 DSPy 存档的 metadata 键冲突",
                ));
            } else if !predictor_names.insert(predictor.name.as_str()) {
                errors.push(IrError::new(
                    ErrorCode::DuplicateId,
                    format!("predictor 名 `{}` 重复", predictor.name),
                ));
            }

            let signature_ok = if blank(&predictor.signature) {
                errors.push(IrError::new(
                    ErrorCode::EmptyIdentifier,
                    format!("predictor `{}` 的签名名为空", predictor.name),
                ));
                false
            } else if !signature_names.contains(predictor.signature.as_str()) {
                errors.push(IrError::new(
                    ErrorCode::UnknownSignature,
                    format!(
                        "predictor `{}` 引用了不存在的签名 `{}`",
                        predictor.name, predictor.signature
                    ),
                ));
                false
            } else {
                true
            };

            let mut seen_demos: BTreeSet<&str> = BTreeSet::new();
            for demo_id in &predictor.demos {
                if blank(demo_id) {
                    errors.push(IrError::new(
                        ErrorCode::EmptyIdentifier,
                        format!("predictor `{}` 引用了空 demo id", predictor.name),
                    ));
                    continue;
                }
                if !seen_demos.insert(demo_id.as_str()) {
                    errors.push(IrError::new(
                        ErrorCode::DuplicateId,
                        format!("predictor `{}` 重复引用 demo `{demo_id}`", predictor.name),
                    ));
                    continue;
                }
                let Some(demo_signature) = demo_signatures.get(demo_id.as_str()) else {
                    errors.push(IrError::new(
                        ErrorCode::UnknownDemo,
                        format!(
                            "predictor `{}` 引用了不存在的 demo `{demo_id}`",
                            predictor.name
                        ),
                    ));
                    continue;
                };
                if signature_ok && *demo_signature != predictor.signature.as_str() {
                    errors.push(IrError::new(
                        ErrorCode::DemoSignatureMismatch,
                        format!(
                            "predictor `{}` 的 demo `{demo_id}` 属于签名 `{demo_signature}`，不是 `{}`",
                            predictor.name, predictor.signature
                        ),
                    ));
                }
            }

            if let Some(lm) = &predictor.lm {
                if blank(&lm.model) {
                    errors.push(IrError::new(
                        ErrorCode::InvalidValue,
                        format!("predictor `{}` 的 lm.model 为空", predictor.name),
                    ));
                }
                if blank(&lm.model_type) {
                    errors.push(IrError::new(
                        ErrorCode::InvalidValue,
                        format!("predictor `{}` 的 lm.model_type 为空", predictor.name),
                    ));
                }
            }
            if predictor.config.n == Some(0) {
                errors.push(IrError::new(
                    ErrorCode::InvalidValue,
                    format!("predictor `{}` 的 config.n 必须 ≥ 1", predictor.name),
                ));
            }
        }

        if let Some(optimizer) = &self.optimization.optimizer {
            if blank(optimizer) {
                errors.push(IrError::new(
                    ErrorCode::InvalidValue,
                    "optimization.optimizer 为空字符串",
                ));
            }
        }
        if let Some(metric) = &self.optimization.metric {
            if blank(metric) {
                errors.push(IrError::new(
                    ErrorCode::InvalidValue,
                    "optimization.metric 为空字符串",
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(IrFailure { errors })
        }
    }
}
