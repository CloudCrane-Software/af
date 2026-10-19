//! v1 字段清单、拒绝式信封、引用校验、删除和 DSPy 存档互转。

use std::collections::{BTreeMap, BTreeSet};

use af_dspy_ir::{
    parse_program, to_json, Demo, ErrorCode, Field, IrFailure, LmConfig, OptimizationState,
    OptimizationStatus, Predictor, PredictorConfig, Program, Signature, IR_VERSION, V1_FIELDS,
};
use serde_json::{json, Value};

fn canonical_json() -> Value {
    json!({
        "version": 1,
        "program_id": "qa-cot",
        "signatures": [{
            "name": "BasicQA",
            "instructions": "Answer questions with short factoid answers.",
            "inputs": [{
                "name": "question",
                "prefix": "Question:",
                "description": "${question}",
                "type_name": "str"
            }],
            "outputs": [{
                "name": "answer",
                "prefix": "Answer:",
                "description": "often between 1 and 5 words",
                "type_name": "str"
            }]
        }],
        "demos": [{
            "id": "demo-1",
            "signature": "BasicQA",
            "values": {
                "question": "What is 2+2?",
                "answer": "4"
            }
        }],
        "predictors": [{
            "name": "generate_answer",
            "signature": "BasicQA",
            "demos": ["demo-1"],
            "config": {
                "temperature_millis": 700,
                "rollout_id": 1,
                "n": 1
            },
            "lm": {
                "model": "openai/gpt-4o-mini",
                "model_type": "chat",
                "cache": true,
                "num_retries": 3
            }
        }],
        "optimization": {
            "optimizer": "BootstrapFewShot",
            "status": "optimized",
            "metric": "exact_match",
            "score_millis": 875,
            "compiled": true
        }
    })
}

fn expected_program() -> Program {
    let mut values = BTreeMap::new();
    values.insert("question".to_string(), json!("What is 2+2?"));
    values.insert("answer".to_string(), json!("4"));
    Program {
        version: IR_VERSION,
        program_id: "qa-cot".to_string(),
        signatures: vec![Signature {
            name: "BasicQA".to_string(),
            instructions: "Answer questions with short factoid answers.".to_string(),
            inputs: vec![Field {
                name: "question".to_string(),
                prefix: "Question:".to_string(),
                description: "${question}".to_string(),
                type_name: "str".to_string(),
            }],
            outputs: vec![Field {
                name: "answer".to_string(),
                prefix: "Answer:".to_string(),
                description: "often between 1 and 5 words".to_string(),
                type_name: "str".to_string(),
            }],
        }],
        demos: vec![Demo {
            id: "demo-1".to_string(),
            signature: "BasicQA".to_string(),
            values,
        }],
        predictors: vec![Predictor {
            name: "generate_answer".to_string(),
            signature: "BasicQA".to_string(),
            demos: vec!["demo-1".to_string()],
            config: PredictorConfig {
                temperature_millis: Some(700),
                rollout_id: Some(1),
                n: Some(1),
            },
            lm: Some(LmConfig {
                model: "openai/gpt-4o-mini".to_string(),
                model_type: "chat".to_string(),
                cache: true,
                num_retries: 3,
            }),
        }],
        optimization: OptimizationState {
            optimizer: Some("BootstrapFewShot".to_string()),
            status: OptimizationStatus::Optimized,
            metric: Some("exact_match".to_string()),
            score_millis: Some(875),
            compiled: true,
        },
    }
}

fn must_parse() -> Program {
    parse_program(&canonical_json().to_string()).expect("canonical IR")
}

fn parse_failure(text: &str) -> IrFailure {
    parse_program(text).expect_err("document should be rejected")
}

fn mutate_failure(edit: impl FnOnce(&mut Value)) -> IrFailure {
    let mut doc = canonical_json();
    edit(&mut doc);
    parse_program(&doc.to_string()).expect_err("mutated document should be rejected")
}

fn collect_pointers(value: &Value) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    fn walk(value: &Value, path: &str, found: &mut BTreeSet<String>) {
        let Value::Object(map) = value else {
            return;
        };
        for (key, child) in map {
            let child_path = format!("{path}/{key}");
            found.insert(child_path.clone());
            // demo 值的键是签名字段名，不是 schema 键。
            if key == "values" {
                continue;
            }
            match child {
                Value::Object(_) => walk(child, &child_path, found),
                Value::Array(items) => {
                    for (index, item) in items.iter().enumerate() {
                        walk(item, &format!("{child_path}/{index}"), found);
                    }
                }
                _ => {}
            }
        }
    }
    walk(value, "", &mut found);
    found
}

fn set_pointer(value: &mut Value, pointer: &str, new_value: Value) {
    let parts: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    fn walk(current: &mut Value, parts: &[&str], new_value: Value) {
        if parts.len() == 1 {
            let Value::Object(map) = current else {
                panic!("pointer does not land on an object: {}", parts[0]);
            };
            assert!(
                map.contains_key(parts[0]),
                "pointer key {} was already absent",
                parts[0]
            );
            map.insert(parts[0].to_string(), new_value);
            return;
        }
        let next = match current {
            Value::Object(map) => map
                .get_mut(parts[0])
                .unwrap_or_else(|| panic!("missing object key {}", parts[0])),
            Value::Array(items) => {
                let index: usize = parts[0].parse().expect("array index");
                items
                    .get_mut(index)
                    .unwrap_or_else(|| panic!("missing index {}", parts[0]))
            }
            other => panic!("cannot walk into {other} at {}", parts[0]),
        };
        walk(next, &parts[1..], new_value);
    }
    walk(value, &parts, new_value);
}

/// Option 字段的反例。字符串字段用数字，数字和对象字段用字符串。
fn wrong_type_for(pointer: &str) -> Value {
    match pointer {
        "/optimization/optimizer" | "/optimization/metric" => json!(1),
        _ => json!("nope"),
    }
}

fn remove_pointer(value: &mut Value, pointer: &str) {
    let parts: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    fn walk(current: &mut Value, parts: &[&str]) {
        if parts.len() == 1 {
            let Value::Object(map) = current else {
                panic!("pointer does not land on an object: {}", parts[0]);
            };
            assert!(
                map.remove(parts[0]).is_some(),
                "pointer key {} was already absent",
                parts[0]
            );
            return;
        }
        let next = match current {
            Value::Object(map) => map
                .get_mut(parts[0])
                .unwrap_or_else(|| panic!("missing object key {}", parts[0])),
            Value::Array(items) => {
                let index: usize = parts[0].parse().expect("array index");
                items
                    .get_mut(index)
                    .unwrap_or_else(|| panic!("missing index {}", parts[0]))
            }
            other => panic!("cannot walk into {other} at {}", parts[0]),
        };
        walk(next, &parts[1..]);
    }
    walk(value, &parts);
}

#[test]
fn error_codes_are_contiguous_from_di1001() {
    assert_eq!(ErrorCode::all()[0].as_str(), "DI1001");
    for (index, code) in ErrorCode::all().iter().enumerate() {
        assert_eq!(code.as_str(), format!("DI{}", 1001 + index));
        assert!(!code.name().is_empty());
        assert_eq!(code.as_str(), code.to_string());
    }
}

#[test]
fn canonical_document_matches_every_v1_field() {
    let doc = canonical_json();
    let pointers = collect_pointers(&doc);
    let expected: BTreeSet<String> = V1_FIELDS
        .iter()
        .map(|spec| spec.pointer.to_string())
        .collect();
    assert_eq!(pointers, expected, "v1 字段清单与规范样例的键不一致");
    assert_eq!(V1_FIELDS.len(), 38);

    let program = must_parse();
    assert_eq!(program, expected_program());
    assert_eq!(program.version, 1);
    assert_eq!(program.program_id, "qa-cot");
    assert_eq!(program.signatures[0].name, "BasicQA");
    assert_eq!(
        program.signatures[0].instructions,
        "Answer questions with short factoid answers."
    );
    assert_eq!(program.signatures[0].inputs[0].name, "question");
    assert_eq!(program.signatures[0].inputs[0].prefix, "Question:");
    assert_eq!(program.signatures[0].inputs[0].description, "${question}");
    assert_eq!(program.signatures[0].inputs[0].type_name, "str");
    assert_eq!(program.signatures[0].outputs[0].name, "answer");
    assert_eq!(program.signatures[0].outputs[0].prefix, "Answer:");
    assert_eq!(
        program.signatures[0].outputs[0].description,
        "often between 1 and 5 words"
    );
    assert_eq!(program.signatures[0].outputs[0].type_name, "str");
    assert_eq!(program.demos[0].id, "demo-1");
    assert_eq!(program.demos[0].signature, "BasicQA");
    assert_eq!(program.demos[0].values["question"], json!("What is 2+2?"));
    assert_eq!(program.demos[0].values["answer"], json!("4"));
    assert_eq!(program.predictors[0].name, "generate_answer");
    assert_eq!(program.predictors[0].signature, "BasicQA");
    assert_eq!(program.predictors[0].demos, vec!["demo-1".to_string()]);
    assert_eq!(program.predictors[0].config.temperature_millis, Some(700));
    assert_eq!(program.predictors[0].config.rollout_id, Some(1));
    assert_eq!(program.predictors[0].config.n, Some(1));
    let lm = program.predictors[0].lm.as_ref().expect("lm");
    assert_eq!(lm.model, "openai/gpt-4o-mini");
    assert_eq!(lm.model_type, "chat");
    assert!(lm.cache);
    assert_eq!(lm.num_retries, 3);
    assert_eq!(
        program.optimization.optimizer.as_deref(),
        Some("BootstrapFewShot")
    );
    assert_eq!(program.optimization.status, OptimizationStatus::Optimized);
    assert_eq!(program.optimization.metric.as_deref(), Some("exact_match"));
    assert_eq!(program.optimization.score_millis, Some(875));
    assert!(program.optimization.compiled);
}

#[test]
fn each_v1_field_has_a_rejecting_counterexample() {
    for spec in V1_FIELDS {
        let mut doc = canonical_json();
        if spec.optional {
            // serde 把缺席的 Option 读成 None，删键不是反例。
            remove_pointer(&mut doc, spec.pointer);
            parse_program(&doc.to_string()).unwrap_or_else(|err| {
                panic!("可选字段 {} 缺键应视为未设置，实际 {err}", spec.name)
            });
            let mut doc = canonical_json();
            set_pointer(&mut doc, spec.pointer, wrong_type_for(spec.pointer));
            let err = parse_program(&doc.to_string()).expect_err(spec.name);
            assert!(
                err.contains(ErrorCode::Schema),
                "{} 的错误类型应是 DI1014，实际 {err}",
                spec.name
            );
            continue;
        }
        remove_pointer(&mut doc, spec.pointer);
        let err = parse_program(&doc.to_string()).expect_err(spec.name);
        let code = if spec.pointer == "/version" {
            ErrorCode::MissingVersion
        } else {
            ErrorCode::MissingField
        };
        assert!(
            err.contains(code),
            "删掉 {} ({}) 应是 {code}，实际 {err}",
            spec.name,
            spec.pointer
        );
    }
}

#[test]
fn ir_json_roundtrip_keeps_every_field() {
    let program = must_parse();
    let text = to_json(&program).expect("serialize");
    let again = parse_program(&text).expect("parse");
    assert_eq!(again, program);
}

#[test]
fn missing_or_unknown_version_is_rejected() {
    let mut without_version = canonical_json();
    without_version
        .as_object_mut()
        .expect("object")
        .remove("version");
    let missing = parse_failure(&without_version.to_string());
    assert!(missing.contains(ErrorCode::MissingVersion), "{missing}");

    let empty = parse_failure("{}");
    assert!(empty.contains(ErrorCode::MissingVersion), "{empty}");

    for text in [
        r#"{"version":2}"#,
        r#"{"version":0}"#,
        r#"{"version":"1"}"#,
        r#"{"version":1.5}"#,
        r#"{"version":-1}"#,
        r#"{"version":null}"#,
    ] {
        let err = parse_failure(text);
        assert!(err.contains(ErrorCode::UnknownVersion), "{text} -> {err}");
    }
}

#[test]
fn non_object_and_unknown_key_are_schema_errors() {
    assert!(parse_failure("[]").contains(ErrorCode::Schema));
    assert!(parse_failure("\"qa\"").contains(ErrorCode::Schema));
    assert!(parse_failure("{").contains(ErrorCode::Schema));

    let err = mutate_failure(|doc| {
        doc.as_object_mut()
            .expect("object")
            .insert("extra".to_string(), json!(1));
    });
    assert!(err.contains(ErrorCode::Schema), "{err}");

    let err = mutate_failure(|doc| {
        doc["predictors"][0]["config"]["extra"] = json!(true);
    });
    assert!(err.contains(ErrorCode::Schema), "{err}");

    let err = mutate_failure(|doc| {
        doc["predictors"][0]["config"]["temperature_millis"] = json!("hot");
    });
    assert!(err.contains(ErrorCode::Schema), "{err}");

    let err = mutate_failure(|doc| {
        doc["optimization"]["score_millis"] = json!(0.875);
    });
    assert!(err.contains(ErrorCode::Schema), "{err}");

    let err = mutate_failure(|doc| {
        doc["optimization"]["status"] = json!("bogus");
    });
    assert!(err.contains(ErrorCode::Schema), "{err}");
}

#[test]
fn optional_nulls_and_each_optimization_status_parse() {
    let mut doc = canonical_json();
    doc["predictors"][0]["config"]["temperature_millis"] = Value::Null;
    doc["predictors"][0]["config"]["rollout_id"] = Value::Null;
    doc["predictors"][0]["config"]["n"] = Value::Null;
    doc["predictors"][0]["lm"] = Value::Null;
    doc["optimization"]["optimizer"] = Value::Null;
    doc["optimization"]["metric"] = Value::Null;
    doc["optimization"]["score_millis"] = Value::Null;
    let program = parse_program(&doc.to_string()).expect("nulls are unset, not missing");
    assert_eq!(program.predictors[0].config.temperature_millis, None);
    assert_eq!(program.predictors[0].config.rollout_id, None);
    assert_eq!(program.predictors[0].config.n, None);
    assert!(program.predictors[0].lm.is_none());
    assert!(program.optimization.optimizer.is_none());
    assert!(program.optimization.metric.is_none());
    assert!(program.optimization.score_millis.is_none());

    for status in ["unoptimized", "optimized", "failed"] {
        let mut doc = canonical_json();
        doc["optimization"]["status"] = json!(status);
        parse_program(&doc.to_string()).expect(status);
    }
}

#[test]
fn blank_field_name_is_di1001() {
    let err = mutate_failure(|doc| {
        doc["signatures"][0]["inputs"][0]["name"] = json!("");
    });
    assert!(err.contains(ErrorCode::MissingFieldName), "{err}");

    let err = mutate_failure(|doc| {
        doc["signatures"][0]["outputs"][0]["name"] = json!("  ");
    });
    assert!(err.contains(ErrorCode::MissingFieldName), "{err}");

    let err = mutate_failure(|doc| {
        let values = doc["demos"][0]["values"].as_object_mut().expect("values");
        let answer = values.remove("answer").expect("answer");
        values.insert("".to_string(), answer);
    });
    assert!(err.contains(ErrorCode::MissingFieldName), "{err}");
}

#[test]
fn bad_signature_and_demo_references_are_rejected() {
    let err = mutate_failure(|doc| {
        doc["demos"][0]["signature"] = json!("MissingSig");
    });
    assert!(err.contains(ErrorCode::UnknownSignature), "{err}");

    let err = mutate_failure(|doc| {
        doc["predictors"][0]["signature"] = json!("MissingSig");
    });
    assert!(err.contains(ErrorCode::UnknownSignature), "{err}");

    let err = mutate_failure(|doc| {
        doc["predictors"][0]["demos"] = json!(["missing-demo"]);
    });
    assert!(err.contains(ErrorCode::UnknownDemo), "{err}");
}

#[test]
fn duplicate_unknown_and_incomplete_fields_are_rejected() {
    let err = mutate_failure(|doc| {
        let signature = doc["signatures"][0].clone();
        doc["signatures"]
            .as_array_mut()
            .expect("sigs")
            .push(signature);
    });
    assert!(err.contains(ErrorCode::DuplicateId), "{err}");

    let err = mutate_failure(|doc| {
        doc["signatures"][0]["outputs"][0]["name"] = json!("question");
        doc["demos"][0]["values"] = json!({"question": "What is 2+2?"});
    });
    assert!(err.contains(ErrorCode::DuplicateId), "{err}");

    let err = mutate_failure(|doc| {
        doc["demos"][0]["values"]["extra"] = json!("nope");
    });
    assert!(err.contains(ErrorCode::UnknownField), "{err}");

    let err = mutate_failure(|doc| {
        doc["demos"][0]["values"]
            .as_object_mut()
            .expect("values")
            .remove("answer");
    });
    assert!(err.contains(ErrorCode::IncompleteDemo), "{err}");

    let err = mutate_failure(|doc| {
        doc["predictors"][0]["config"]["n"] = json!(0);
    });
    assert!(err.contains(ErrorCode::InvalidValue), "{err}");

    let err = mutate_failure(|doc| {
        doc["signatures"][0]["inputs"][0]["type_name"] = json!("");
    });
    assert!(err.contains(ErrorCode::InvalidValue), "{err}");

    let err = mutate_failure(|doc| {
        doc["program_id"] = json!("");
    });
    assert!(err.contains(ErrorCode::EmptyIdentifier), "{err}");

    let err = mutate_failure(|doc| {
        doc["predictors"][0]["name"] = json!("metadata");
    });
    assert!(err.contains(ErrorCode::InvalidValue), "{err}");
}

#[test]
fn demo_bound_to_another_signature_is_rejected() {
    let mut program = must_parse();
    program.signatures.push(Signature {
        name: "Grade".to_string(),
        instructions: "Grade the answer.".to_string(),
        inputs: vec![Field {
            name: "question".to_string(),
            prefix: "Question:".to_string(),
            description: String::new(),
            type_name: "str".to_string(),
        }],
        outputs: vec![Field {
            name: "grade".to_string(),
            prefix: "Grade:".to_string(),
            description: String::new(),
            type_name: "str".to_string(),
        }],
    });
    let mut values = BTreeMap::new();
    values.insert("question".to_string(), json!("What is 2+2?"));
    values.insert("grade".to_string(), json!("A"));
    program.demos.push(Demo {
        id: "demo-2".to_string(),
        signature: "Grade".to_string(),
        values,
    });
    program.predictors[0].demos = vec!["demo-2".to_string()];
    let err = program.validate().expect_err("cross-signature demo");
    assert!(err.contains(ErrorCode::DemoSignatureMismatch), "{err}");

    program.predictors[0].demos = vec!["demo-1".to_string(), "demo-1".to_string()];
    let err = program.validate().expect_err("duplicate demo ref");
    assert!(err.contains(ErrorCode::DuplicateId), "{err}");
}

#[test]
fn delete_missing_field_is_explicit_and_does_not_change_the_program() {
    let mut program = must_parse();
    let before = program.clone();

    let missing = program
        .delete_field("BasicQA", "nope")
        .expect_err("missing");
    assert_eq!(missing.code, ErrorCode::FieldNotFound);
    assert_eq!(missing.code_str(), "DI1004");
    assert_eq!(program, before);

    let unknown = program
        .delete_field("MissingSig", "question")
        .expect_err("signature");
    assert_eq!(unknown.code, ErrorCode::UnknownSignature);
    assert_eq!(program, before);

    let blank = program.delete_field("BasicQA", " ").expect_err("blank");
    assert_eq!(blank.code, ErrorCode::MissingFieldName);
    assert_eq!(program, before);

    program
        .delete_field("BasicQA", "question")
        .expect("delete input");
    assert!(program.signatures[0].inputs.is_empty());
    assert!(!program.demos[0].values.contains_key("question"));
    assert_eq!(program.demos[0].values["answer"], json!("4"));
    program.validate().expect("still valid");
    let text = to_json(&program).expect("json");
    assert_eq!(parse_program(&text).expect("parse"), program);

    let mut again = must_parse();
    again
        .delete_field("BasicQA", "answer")
        .expect("delete output");
    assert!(again.signatures[0].outputs.is_empty());
    assert!(!again.demos[0].values.contains_key("answer"));
    again.validate().expect("output removed");
}

#[test]
fn constructed_program_with_unknown_version_cannot_be_written() {
    let mut program = must_parse();
    program.version = 2;
    let err = to_json(&program).expect_err("version");
    assert!(err.contains(ErrorCode::UnknownVersion), "{err}");
}
