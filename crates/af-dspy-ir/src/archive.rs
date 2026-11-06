//! 与 DSPy 状态 JSON 的互转。
//!
//! `Module.save(..., save_program=False)` 写出的对象是
//! `{predictor 名: Predict.dump_state(), "metadata": ...}`。
//! `Predict.dump_state` 里的 `signature.fields` 只有 `prefix` 和 `description`。
//! 字段名按签名表的位置补上：先输入，后输出，与 `Signature.fields` 一致。
//! 条数不一致就拒绝，不学 DSPy `zip(..., strict=False)` 的静默截断。

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::error::{ErrorCode, IrError, IrFailure};
use crate::model::{blank, Field, LmConfig, Program, Signature};
use crate::parse::parse_program_value;

/// 把 IR 写成 DSPy 状态 JSON。
///
/// 签名字段不写 `name`。`traces` / `train` 写成空数组。不写 `metadata`，
/// 也不写 `_dspy_lm_class`（DSPy 对缺类名的旧存档按 `dspy.LM` 装回）。
/// predictor 配置和优化状态不在 DSPy 存档里，留在 IR。
pub fn to_dspy_archive(program: &Program) -> Result<Value, IrFailure> {
    program.validate()?;
    let mut root = Map::new();
    for predictor in &program.predictors {
        let Some(signature) = program.signature(&predictor.signature) else {
            return Err(IrFailure::one(IrError::new(
                ErrorCode::UnknownSignature,
                format!(
                    "predictor `{}` 引用了不存在的签名 `{}`",
                    predictor.name, predictor.signature
                ),
            )));
        };
        let mut demos = Vec::with_capacity(predictor.demos.len());
        for demo_id in &predictor.demos {
            let Some(demo) = program.demo(demo_id) else {
                return Err(IrFailure::one(IrError::new(
                    ErrorCode::UnknownDemo,
                    format!(
                        "predictor `{}` 引用了不存在的 demo `{demo_id}`",
                        predictor.name
                    ),
                )));
            };
            demos.push(Value::Object(
                demo.values
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            ));
        }
        let mut state = Map::new();
        state.insert("traces".to_string(), Value::Array(Vec::new()));
        state.insert("train".to_string(), Value::Array(Vec::new()));
        state.insert("demos".to_string(), Value::Array(demos));
        state.insert("signature".to_string(), export_signature(signature));
        state.insert("lm".to_string(), export_lm(predictor.lm.as_ref()));
        root.insert(predictor.name.clone(), Value::Object(state));
    }
    Ok(Value::Object(root))
}

/// 用存档覆盖 `base` 里 DSPy 会保存的那一部分，字段名仍取签名表。
///
/// 保留 `base` 的程序 id、字段类型、demo id、predictor 配置和优化状态。
/// 存档更新指令、前缀、说明、demo 值和 LM 四个稳定键。
pub fn apply_dspy_archive(base: &Program, archive: &Value) -> Result<Program, IrFailure> {
    base.validate()?;
    let obj = archive.as_object().ok_or_else(|| {
        IrFailure::one(IrError::new(ErrorCode::Schema, "DSPy 存档必须是 JSON 对象"))
    })?;

    let mut errors = Vec::new();
    let mut seen_predictors = BTreeSet::<&str>::new();
    for key in obj.keys() {
        if key == "metadata" {
            continue;
        }
        if base
            .predictors
            .iter()
            .any(|predictor| predictor.name == *key)
        {
            seen_predictors.insert(key.as_str());
        } else {
            errors.push(IrError::new(
                ErrorCode::UnknownPredictor,
                format!("存档引用了不存在的 predictor `{key}`"),
            ));
        }
    }
    for predictor in &base.predictors {
        if !seen_predictors.contains(predictor.name.as_str()) {
            errors.push(IrError::new(
                ErrorCode::ArchivePredictorMissing,
                format!("存档缺少 predictor `{}`", predictor.name),
            ));
        }
    }
    if !errors.is_empty() {
        return Err(IrFailure { errors });
    }

    let mut program = base.clone();
    let mut applied: BTreeMap<String, AppliedSignature> = BTreeMap::new();
    let mut written_demos: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    let predictor_plans: Vec<(String, String)> = base
        .predictors
        .iter()
        .map(|predictor| (predictor.name.clone(), predictor.signature.clone()))
        .collect();

    for (predictor_name, signature_name) in &predictor_plans {
        let state = &obj[predictor_name];
        if let Err(error) = require_object_state(state, predictor_name) {
            errors.push(error);
            continue;
        }
        if let Err(error) = require_array_if_present(state, "traces", predictor_name) {
            errors.push(error);
        }
        if let Err(error) = require_array_if_present(state, "train", predictor_name) {
            errors.push(error);
        }
        let signature = program
            .signature(signature_name)
            .expect("validate 已确认签名存在");
        match read_applied_signature(signature, state, predictor_name) {
            Ok(next) => {
                if let Some(previous) = applied.get(signature_name) {
                    if previous != &next {
                        errors.push(IrError::new(
                            ErrorCode::ArchiveSignatureConflict,
                            format!(
                                "签名 `{signature_name}` 被多个 predictor 写成了不同的指令或字段"
                            ),
                        ));
                    }
                } else {
                    applied.insert(signature_name.clone(), next);
                }
            }
            Err(failure) => errors.extend(failure.errors),
        }
        let index = predictor_index(&program, predictor_name);
        if let Err(failure) = apply_demos(&mut program, index, state, &mut written_demos) {
            errors.extend(failure.errors);
        }
        if let Err(failure) = apply_lm(&mut program, predictor_name, state) {
            errors.extend(failure.errors);
        }
    }

    if !errors.is_empty() {
        return Err(IrFailure { errors });
    }

    for (name, applied_signature) in applied {
        let signature = program
            .signatures
            .iter_mut()
            .find(|signature| signature.name == name)
            .expect("签名名来自已校验的 predictor");
        signature.instructions = applied_signature.instructions;
        signature.inputs = applied_signature.inputs;
        signature.outputs = applied_signature.outputs;
    }

    // 走与 parse 相同的语义校验，拒绝补名之后仍不完整的 demo。
    let value = serde_json::to_value(&program).map_err(|err| {
        IrFailure::one(IrError::new(
            ErrorCode::Schema,
            format!("互转结果无法序列化: {err}"),
        ))
    })?;
    parse_program_value(&value)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AppliedSignature {
    instructions: String,
    inputs: Vec<Field>,
    outputs: Vec<Field>,
}

fn export_signature(signature: &Signature) -> Value {
    let fields: Vec<Value> = signature
        .ordered_fields()
        .map(|field| {
            serde_json::json!({
                "prefix": field.prefix,
                "description": field.description,
            })
        })
        .collect();
    serde_json::json!({
        "instructions": signature.instructions,
        "fields": fields,
    })
}

fn export_lm(lm: Option<&LmConfig>) -> Value {
    match lm {
        None => Value::Null,
        Some(lm) => serde_json::json!({
            "model": lm.model,
            "model_type": lm.model_type,
            "cache": lm.cache,
            "num_retries": lm.num_retries,
        }),
    }
}

fn predictor_index(program: &Program, name: &str) -> usize {
    program
        .predictors
        .iter()
        .position(|predictor| predictor.name == name)
        .expect("predictor 名来自存档键与 IR 的交集")
}

fn read_applied_signature(
    signature: &Signature,
    state: &Value,
    predictor: &str,
) -> Result<AppliedSignature, IrFailure> {
    let Some(sig_value) = state.get("signature") else {
        return Err(IrFailure::one(IrError::new(
            ErrorCode::Schema,
            format!("predictor `{predictor}` 的存档缺少 signature"),
        )));
    };
    let Some(sig_obj) = sig_value.as_object() else {
        return Err(IrFailure::one(IrError::new(
            ErrorCode::Schema,
            format!("predictor `{predictor}` 的 signature 必须是对象"),
        )));
    };
    let Some(instructions) = sig_obj.get("instructions").and_then(Value::as_str) else {
        return Err(IrFailure::one(IrError::new(
            ErrorCode::Schema,
            format!("predictor `{predictor}` 的 signature.instructions 必须是字符串"),
        )));
    };
    let Some(fields) = sig_obj.get("fields").and_then(Value::as_array) else {
        return Err(IrFailure::one(IrError::new(
            ErrorCode::Schema,
            format!("predictor `{predictor}` 的 signature.fields 必须是数组"),
        )));
    };
    let table: Vec<&Field> = signature.ordered_fields().collect();
    if fields.len() != table.len() {
        return Err(IrFailure::one(IrError::new(
            ErrorCode::ArchiveFieldMismatch,
            format!(
                "predictor `{predictor}` 的存档有 {} 个字段，签名 `{}` 有 {} 个，无法按位置补字段名",
                fields.len(),
                signature.name,
                table.len()
            ),
        )));
    }

    let mut errors = Vec::new();
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for (index, (archived, current)) in fields.iter().zip(table.iter()).enumerate() {
        match read_archive_field(archived, current, predictor, index) {
            Ok(field) => {
                if index < signature.inputs.len() {
                    inputs.push(field);
                } else {
                    outputs.push(field);
                }
            }
            Err(error) => errors.push(error),
        }
    }
    if !errors.is_empty() {
        return Err(IrFailure { errors });
    }
    Ok(AppliedSignature {
        instructions: instructions.to_string(),
        inputs,
        outputs,
    })
}

fn read_archive_field(
    archived: &Value,
    current: &Field,
    predictor: &str,
    index: usize,
) -> Result<Field, IrError> {
    let Some(obj) = archived.as_object() else {
        return Err(IrError::new(
            ErrorCode::Schema,
            format!("predictor `{predictor}` 的第 {index} 个存档字段不是对象"),
        ));
    };
    if let Some(name) = obj.get("name") {
        let Some(text) = name.as_str() else {
            return Err(IrError::new(
                ErrorCode::Schema,
                format!("predictor `{predictor}` 的第 {index} 个存档字段的 name 不是字符串"),
            ));
        };
        if blank(text) {
            return Err(IrError::new(
                ErrorCode::MissingFieldName,
                format!("predictor `{predictor}` 的第 {index} 个存档字段名为空"),
            ));
        }
        if text != current.name {
            return Err(IrError::new(
                ErrorCode::ArchiveFieldNameConflict,
                format!(
                    "predictor `{predictor}` 的第 {index} 个存档字段名是 `{text}`，签名表是 `{}`",
                    current.name
                ),
            ));
        }
    }
    let Some(prefix) = obj.get("prefix").and_then(Value::as_str) else {
        return Err(IrError::new(
            ErrorCode::Schema,
            format!("predictor `{predictor}` 的第 {index} 个存档字段缺少 prefix"),
        ));
    };
    let Some(description) = obj.get("description").and_then(Value::as_str) else {
        return Err(IrError::new(
            ErrorCode::Schema,
            format!("predictor `{predictor}` 的第 {index} 个存档字段缺少 description"),
        ));
    };
    Ok(Field {
        name: current.name.clone(),
        prefix: prefix.to_string(),
        description: description.to_string(),
        type_name: current.type_name.clone(),
    })
}

fn apply_demos(
    program: &mut Program,
    predictor_index: usize,
    state: &Value,
    written: &mut BTreeMap<String, BTreeMap<String, Value>>,
) -> Result<(), IrFailure> {
    let predictor_name = program.predictors[predictor_index].name.clone();
    let Some(demos) = state.get("demos").and_then(Value::as_array) else {
        return Err(IrFailure::one(IrError::new(
            ErrorCode::Schema,
            format!("predictor `{predictor_name}` 的 demos 必须是数组"),
        )));
    };
    let refs = program.predictors[predictor_index].demos.clone();
    if demos.len() != refs.len() {
        return Err(IrFailure::one(IrError::new(
            ErrorCode::ArchiveDemoCount,
            format!(
                "predictor `{predictor_name}` 的存档有 {} 条 demo，IR 引用了 {} 条",
                demos.len(),
                refs.len()
            ),
        )));
    }
    let mut errors = Vec::new();
    for (demo_value, demo_id) in demos.iter().zip(refs) {
        let Some(obj) = demo_value.as_object() else {
            errors.push(IrError::new(
                ErrorCode::Schema,
                format!("predictor `{predictor_name}` 的 demo `{demo_id}` 必须是对象"),
            ));
            continue;
        };
        let mut values = BTreeMap::new();
        for (key, value) in obj {
            if blank(key) {
                errors.push(IrError::new(
                    ErrorCode::MissingFieldName,
                    format!("predictor `{predictor_name}` 的 demo `{demo_id}` 有空字段名"),
                ));
                continue;
            }
            values.insert(key.clone(), value.clone());
        }
        if let Some(previous) = written.get(&demo_id) {
            if previous != &values {
                errors.push(IrError::new(
                    ErrorCode::ArchiveDemoConflict,
                    format!("demo `{demo_id}` 被多个 predictor 写成了不同的值"),
                ));
                continue;
            }
        } else {
            written.insert(demo_id.clone(), values.clone());
        }
        let Some(demo) = program.demos.iter_mut().find(|demo| demo.id == demo_id) else {
            errors.push(IrError::new(
                ErrorCode::UnknownDemo,
                format!("predictor `{predictor_name}` 引用了不存在的 demo `{demo_id}`"),
            ));
            continue;
        };
        demo.values = values;
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(IrFailure { errors })
    }
}

fn require_object_state(state: &Value, predictor: &str) -> Result<(), IrError> {
    if state.is_object() {
        Ok(())
    } else {
        Err(IrError::new(
            ErrorCode::Schema,
            format!("predictor `{predictor}` 的存档必须是对象"),
        ))
    }
}

fn require_array_if_present(state: &Value, key: &str, predictor: &str) -> Result<(), IrError> {
    match state.get(key) {
        None | Some(Value::Array(_)) => Ok(()),
        Some(_) => Err(IrError::new(
            ErrorCode::Schema,
            format!("predictor `{predictor}` 的 `{key}` 必须是数组"),
        )),
    }
}

fn apply_lm(program: &mut Program, predictor_name: &str, state: &Value) -> Result<(), IrFailure> {
    let Some(lm_value) = state.get("lm") else {
        return Err(IrFailure::one(IrError::new(
            ErrorCode::Schema,
            format!("predictor `{predictor_name}` 的存档缺少 lm"),
        )));
    };
    let lm = if lm_value.is_null() {
        None
    } else {
        let Some(obj) = lm_value.as_object() else {
            return Err(IrFailure::one(IrError::new(
                ErrorCode::Schema,
                format!("predictor `{predictor_name}` 的 lm 必须是对象或 null"),
            )));
        };
        let model = required_string(obj, "model", predictor_name)?;
        let model_type = required_string(obj, "model_type", predictor_name)?;
        let Some(cache) = obj.get("cache").and_then(Value::as_bool) else {
            return Err(IrFailure::one(IrError::new(
                ErrorCode::Schema,
                format!("predictor `{predictor_name}` 的 lm.cache 必须是布尔值"),
            )));
        };
        let Some(num_retries) = obj.get("num_retries").and_then(Value::as_u64) else {
            return Err(IrFailure::one(IrError::new(
                ErrorCode::Schema,
                format!("predictor `{predictor_name}` 的 lm.num_retries 必须是非负整数"),
            )));
        };
        let Ok(num_retries) = u32::try_from(num_retries) else {
            return Err(IrFailure::one(IrError::new(
                ErrorCode::InvalidValue,
                format!("predictor `{predictor_name}` 的 lm.num_retries 超出 u32"),
            )));
        };
        Some(LmConfig {
            model,
            model_type,
            cache,
            num_retries,
        })
    };
    let predictor = program
        .predictors
        .iter_mut()
        .find(|predictor| predictor.name == predictor_name)
        .expect("predictor 名来自 IR");
    predictor.lm = lm;
    Ok(())
}

fn required_string(
    obj: &Map<String, Value>,
    key: &str,
    predictor: &str,
) -> Result<String, IrFailure> {
    match obj.get(key).and_then(Value::as_str) {
        Some(text) if !blank(text) => Ok(text.to_string()),
        Some(_) => Err(IrFailure::one(IrError::new(
            ErrorCode::InvalidValue,
            format!("predictor `{predictor}` 的 lm.{key} 为空"),
        ))),
        None => Err(IrFailure::one(IrError::new(
            ErrorCode::Schema,
            format!("predictor `{predictor}` 的 lm.{key} 必须是字符串"),
        ))),
    }
}
