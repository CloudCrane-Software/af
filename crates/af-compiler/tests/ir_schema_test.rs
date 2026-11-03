//! IR schema v1 面：拒绝式信封（版本/未知键/未知字段）+ 四一等字段完备性
//! （ADR-010 §5：node.capability_refs / tool.effect_class / artifact.provenance
//! / trust 注记——v0.1 空默认值但字段必须在序列化面）。

mod common;

use af_compiler::decode::IrParseError;
use serde_json::{json, Value};

fn sample1_json() -> String {
    let (doc, _) = common::sample("sample1.yaml");
    af_compiler::snapshot_json(&doc).expect("lowering 失败")
}

fn as_obj(v: &Value) -> &serde_json::Map<String, Value> {
    v.as_object().expect("应为 JSON 对象")
}

// ------------------------------------------------------------
// 拒绝式信封
// ------------------------------------------------------------

#[test]
fn envelope_rejects_version_not_one() {
    let mut v: Value = serde_json::from_str(&sample1_json()).unwrap();
    v["ir_version"] = json!(2);
    assert_eq!(
        af_compiler::decode_ir(&v.to_string()),
        Err(IrParseError::VersionNotOne { found: 2 })
    );
    // 版本错 → body 不解析：body 篡改也不掩盖版本拒绝
    v["body"]["graph"]["name"] = json!(null);
    assert_eq!(
        af_compiler::decode_ir(&v.to_string()),
        Err(IrParseError::VersionNotOne { found: 2 })
    );
}

#[test]
fn envelope_rejects_missing_version() {
    let mut v: Value = serde_json::from_str(&sample1_json()).unwrap();
    v.as_object_mut().unwrap().remove("ir_version").unwrap();
    assert_eq!(
        af_compiler::decode_ir(&v.to_string()),
        Err(IrParseError::MissingVersion)
    );
    // 根不是对象（无 ir_version 的退化情形）
    assert_eq!(
        af_compiler::decode_ir("[]"),
        Err(IrParseError::MissingVersion)
    );
}

#[test]
fn envelope_rejects_non_integer_version() {
    let mut v: Value = serde_json::from_str(&sample1_json()).unwrap();
    v["ir_version"] = json!("1");
    assert_eq!(
        af_compiler::decode_ir(&v.to_string()),
        Err(IrParseError::VersionNotInteger)
    );
}

#[test]
fn envelope_rejects_unknown_top_key() {
    let mut v: Value = serde_json::from_str(&sample1_json()).unwrap();
    v["extra"] = json!(1);
    assert_eq!(
        af_compiler::decode_ir(&v.to_string()),
        Err(IrParseError::UnknownTopKey {
            key: "extra".into()
        })
    );
}

#[test]
fn envelope_rejects_unknown_body_field_and_nested_field() {
    let base = sample1_json();
    // body 顶层未知字段
    let mut v: Value = serde_json::from_str(&base).unwrap();
    v["body"]["agents_view"] = json!(1);
    assert!(matches!(
        af_compiler::decode_ir(&v.to_string()),
        Err(IrParseError::MalformedBody { .. })
    ));
    // 嵌套节点未知字段（deny_unknown_fields 全层生效）
    let mut v: Value = serde_json::from_str(&base).unwrap();
    v["body"]["graph"]["nodes"][0]["mystery"] = json!(1);
    assert!(matches!(
        af_compiler::decode_ir(&v.to_string()),
        Err(IrParseError::MalformedBody { .. })
    ));
    // 嵌套 CelRef 未知字段
    let mut v: Value = serde_json::from_str(&base).unwrap();
    v["body"]["policies"]["stop"][0]["when"]["extra"] = json!(1);
    assert!(matches!(
        af_compiler::decode_ir(&v.to_string()),
        Err(IrParseError::MalformedBody { .. })
    ));
}

#[test]
fn envelope_version_is_one() {
    let v: Value = serde_json::from_str(&sample1_json()).unwrap();
    assert_eq!(v["ir_version"], json!(1));
}

// ------------------------------------------------------------
// 四一等字段完备性（ADR-010 §5）
// ------------------------------------------------------------

/// 六样例：四一等字段在序列化面全数在场（空默认值可见）。
#[test]
fn first_class_fields_present_in_all_samples() {
    for name in common::SAMPLES {
        let (doc, _) = common::sample(name);
        let p = af_compiler::lower(&doc).expect("{name}: lowering 失败");
        let j = af_compiler::snapshot_json(&doc).expect("{name}: 序列化失败");
        let v: Value = serde_json::from_str(&j).unwrap();
        let body = &v["body"];

        // ① trust 注记（body 一等位，恒空对象 `{}`）
        assert!(
            body.get("trust").is_some_and(Value::is_object),
            "{name}: body.trust 缺失"
        );
        assert!(p.body.trust.is_empty());

        // ② node.capability_refs：根图 + 全部子图的每个节点
        let graph = &body["graph"];
        for n in graph["nodes"].as_array().expect("nodes 应为数组") {
            assert!(
                n.get("capability_refs").is_some_and(Value::is_array),
                "{name}: 根图节点 {} 缺 capability_refs",
                n["id"]
            );
        }
        for (sg, sgv) in as_obj(&graph["subgraphs"]) {
            for n in sgv["nodes"].as_array().expect("子图 nodes 应为数组") {
                assert!(
                    n.get("capability_refs").is_some_and(Value::is_array),
                    "{name}: 子图 {sg} 节点 {} 缺 capability_refs",
                    n["id"]
                );
            }
        }

        // ③ tool.effect_class / ④ artifact.provenance：node_exec 投影
        for (path, exec) in as_obj(&body["node_exec"]) {
            if let Some(tools) = exec["components"]["tools"].as_object() {
                for (tname, tool) in tools {
                    assert!(
                        tool.get("effect_class").is_some(),
                        "{name}: node_exec[{path}] 工具 {tname} 缺 effect_class"
                    );
                }
            }
            if let Some(arts) = exec["artifacts"].as_object() {
                for (aname, art) in arts {
                    assert!(
                        art.get("provenance").is_some(),
                        "{name}: node_exec[{path}] 工件 {aname} 缺 provenance"
                    );
                }
            }
        }
    }
}

/// 占位值断言（v0.1 空默认）：capability_refs=[]、effect_class=null、
/// provenance=null、trust={}——字段在、值为空。
#[test]
fn first_class_placeholder_values_are_empty_defaults() {
    let (doc, _) = common::sample("sample1.yaml");
    let p = af_compiler::lower(&doc).expect("lowering 失败");
    let solver = &p.body.graph.nodes[0];
    assert_eq!(solver.capability_refs, Default::default());
    let j: Value = serde_json::from_str(&af_compiler::snapshot_json(&doc).unwrap()).unwrap();
    assert_eq!(j["body"]["graph"]["nodes"][0]["capability_refs"], json!([]));
    assert_eq!(j["body"]["trust"], json!({}));
    let ex = &p.body.node_exec["solver"];
    for t in ex.components.tools.values() {
        assert_eq!(t.effect_class, None);
    }
    assert_eq!(ex.artifacts["answer"].provenance, None);
    let tools_json = &j["body"]["node_exec"]["solver"]["components"]["tools"];
    for t in as_obj(tools_json).values() {
        assert_eq!(t["effect_class"], Value::Null);
    }
    assert_eq!(
        j["body"]["node_exec"]["solver"]["artifacts"]["answer"]["provenance"],
        Value::Null
    );
}
