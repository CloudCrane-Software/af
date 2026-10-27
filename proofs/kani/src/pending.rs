//! 受阻 harness（默认不编入；`cargo kani --features blocked-harnesses` 启用）。
//!
//! 保留原样给 FV-1：一旦 kani-compiler 修掉 ICE（chrono/cel 可达链，
//! intrinsics.rs:243）或 CBMC 对 serde_yaml::Value 的代价降到可接受，
//! 这些 harness 即为 INV-2 + 信封的正式验证入口。设计说明见 lib.rs 顶部
//! 与 EVIDENCE.md。

use af_dsl::envelope;
use af_dsl::error::{ErrorCode, LoadError};
use serde_yaml::Value;

// ───────── 信封（编译通过；CBMC 下 serde_yaml::Value 构造病态，>4min 未完）─────────

/// E0001：缺 version 键 → 整份拒绝，单违例短路。
#[kani::proof]
fn envelope_missing_version_is_rejected_whole() {
    let k: u8 = kani::any();
    kani::assume(k < 2);
    let mut m = serde_yaml::Mapping::new();
    if k == 1 {
        m.insert(Value::String("meta".into()), Value::Null);
    }
    match envelope::check(&Value::Mapping(m)) {
        Ok(()) => assert!(false),
        Err(LoadError::Envelope(rep)) => {
            assert_eq!(rep.violations.len(), 1);
            assert!(matches!(rep.violations[0].code, ErrorCode::MissingVersion));
        }
        Err(_) => assert!(false),
    }
}

/// E0002：version 非整数（串/布尔/浮点）→ 整份拒绝。
#[kani::proof]
fn envelope_non_integer_version_is_rejected() {
    let k: u8 = kani::any();
    kani::assume(k < 3);
    let v = match k {
        0 => Value::String("1".into()),
        1 => Value::Bool(true),
        _ => Value::Number(1.5f64.into()),
    };
    let mut m = serde_yaml::Mapping::new();
    m.insert(Value::String("version".into()), v);
    match envelope::check(&Value::Mapping(m)) {
        Ok(()) => assert!(false),
        Err(LoadError::Envelope(rep)) => {
            assert!(matches!(rep.violations[0].code, ErrorCode::VersionNotInteger));
        }
        Err(_) => assert!(false),
    }
}

/// E0003：version=1 唯一放行；v ∈ {-1,0,2} 整份拒绝（VersionNotOne 单违例）。
#[kani::proof]
fn envelope_version_gate_is_exactly_one() {
    let sel: u8 = kani::any();
    kani::assume(sel < 4);
    let v: i64 = match sel {
        0 => -1,
        1 => 0,
        2 => 1,
        _ => 2,
    };
    let mut m = serde_yaml::Mapping::new();
    m.insert(Value::String("version".into()), Value::Number(v.into()));
    match envelope::check(&Value::Mapping(m)) {
        Ok(()) => assert!(v == 1),
        Err(LoadError::Envelope(rep)) => {
            assert!(v != 1);
            assert_eq!(rep.violations.len(), 1);
            assert!(matches!(rep.violations[0].code, ErrorCode::VersionNotOne));
        }
        Err(_) => assert!(false),
    }
}

/// E0004：六段合法键放行；未知键逐键一条拒绝。
#[kani::proof]
fn envelope_unknown_key_detected_iff_illegal() {
    let k: u8 = kani::any();
    kani::assume(k < 7);
    let extra = match k {
        0 => "meta",
        1 => "types",
        2 => "stores",
        3 => "nodes",
        4 => "edges",
        5 => "policies",
        _ => "zzz_unknown",
    };
    let mut m = serde_yaml::Mapping::new();
    m.insert(Value::String("version".into()), Value::Number(1i64.into()));
    m.insert(Value::String(extra.into()), Value::Null);
    match envelope::check(&Value::Mapping(m)) {
        Ok(()) => assert!(extra != "zzz_unknown"),
        Err(LoadError::Envelope(rep)) => {
            assert!(extra == "zzz_unknown");
            assert!(matches!(rep.violations[0].code, ErrorCode::EnvelopeUnknownKey));
        }
        Err(_) => assert!(false),
    }
}

// ───────── INV-2（kani-compiler 0.68.0 ICE：validate 可达链 chrono/cel）─────────

fn codes(src: &str) -> Vec<&'static str> {
    match af_dsl::load_str(src) {
        Ok(doc) => af_dsl::validate(&doc).codes(),
        Err(_) => {
            assert!(false);
            Vec::new()
        }
    }
}

fn doc_with(writer: &str, nodes: &str) -> String {
    let entry = if nodes.contains("id: l1") { "l1" } else { "n1" };
    format!(
        "version: 1\nmeta:\n  name: t\n  graph_version: \"1.0\"\n  entry: {entry}\n  input: in_t\n  output: {{ ref: a, from: {entry} }}\ntypes:\n  - {{ name: in_t, schema: {{ type: object, required: [q], properties: {{ q: {{ type: string }} }} }} }}\n  - {{ name: a, schema: {{ type: object }} }}\nstores:\n  - {{ name: s, type: kv, value: {{ type: string }}, writer: {writer}, lifecycle: run, retention: {{ max_items: 8 }} }}\nnodes:\n{nodes}edges: []\npolicies:\n  budgets: []\n  stop: []\n  failures: []\n"
    )
}

/// E2311 写者唯一性：{n1 写?, n2 写?} × 声明 ∈ {n1, n2} = 16 例穷尽；精确码集。
#[kani::proof]
fn inv2_writer_uniqueness_bounded_exhaustive() {
    let sel: u8 = kani::any();
    kani::assume(sel < 16);
    let w1 = sel & 1 != 0;
    let w2 = sel & 2 != 0;
    let decl_n2 = sel & 4 != 0;
    let n1 = if w1 {
        "  - { id: n1, kind: task, uses: { model: route.main@1 }, writes: [s], emits: { answer: a } }\n"
    } else {
        "  - { id: n1, kind: task, uses: { model: route.main@1 }, emits: { answer: a } }\n"
    };
    let n2 = if w2 {
        "  - { id: n2, kind: task, writes: [s] }\n"
    } else {
        "  - { id: n2, kind: task }\n"
    };
    let writer = if decl_n2 { "n2" } else { "n1" };
    let cs = codes(&doc_with(writer, &format!("{n1}{n2}")));
    let writers = (w1 as u8) + (w2 as u8);
    let violation = writers > 1 || (writers == 1 && (w1 && decl_n2 || w2 && !decl_n2));
    if violation {
        assert_eq!(cs, vec!["E2311"]);
    } else {
        assert!(cs.is_empty());
    }
}

/// E2312 map 体写共享槽：{写, 不写} 穷尽（外层 carried 已登记以隔离 E2211）。
#[kani::proof]
fn inv2_map_shared_write_bounded_exhaustive() {
    let sel: u8 = kani::any();
    kani::assume(sel < 2);
    let writes = sel == 0;
    let d = if writes {
        "                  - { id: d, kind: task, writes: [s], emits: { x: a } }\n"
    } else {
        "                  - { id: d, kind: task, emits: { x: a } }\n"
    };
    let src = [
        "version: 1",
        "meta:",
        "  name: t",
        "  graph_version: \"1.0\"",
        "  entry: l1",
        "  input: in_t",
        "  output: { ref: a, from: l1 }",
        "types:",
        "  - { name: in_t, schema: { type: object, required: [arr], properties: { arr: { type: array, items: { type: string } } } } }",
        "  - { name: a, schema: { type: object } }",
        "stores:",
        "  - { name: s, type: kv, value: { type: string }, writer: l1.mp.d, lifecycle: run, retention: { max_items: 8 } }",
        "nodes:",
        "  - id: l1",
        "    kind: task",
        "    loop:",
        "      body:",
        "        nodes:",
        "          - id: mp",
        "            kind: task",
        "            map:",
        "              over: input.arr",
        "              body:",
        "                nodes:",
    ]
    .join("\n")
        + d
        + "                edges: []\n          - { id: b2, kind: task, emits: { y: a } }\n        edges: []\n      max_iterations: 2\n      carried: [ { store: s, max_items: 8 } ]\n      safety_points: []\n    emits: { answer: { from_body: b2.y } }\nedges: []\npolicies:\n  budgets: []\n  stop: []\n  failures: []\n";
    let cs = codes(&src);
    if writes {
        assert_eq!(cs, vec!["E2312"]);
    } else {
        assert!(cs.is_empty());
    }
}

/// E2211 carried 覆盖：{登记, 未登记} 穷尽。
#[kani::proof]
fn inv2_carried_coverage_bounded_exhaustive() {
    let sel: u8 = kani::any();
    kani::assume(sel < 2);
    let covered = sel == 0;
    let carried = if covered {
        "      carried: [ { store: s, max_items: 4 } ]\n"
    } else {
        "      carried: []\n"
    };
    let src = [
        "version: 1",
        "meta:",
        "  name: t",
        "  graph_version: \"1.0\"",
        "  entry: l1",
        "  input: in_t",
        "  output: { ref: a, from: l1 }",
        "types:",
        "  - { name: in_t, schema: { type: object, required: [q], properties: { q: { type: string } } } }",
        "  - { name: a, schema: { type: object } }",
        "stores:",
        "  - { name: s, type: kv, value: { type: string }, writer: l1.b1, lifecycle: run, retention: { max_items: 8 } }",
        "nodes:",
        "  - id: l1",
        "    kind: task",
        "    loop:",
        "      body:",
        "        nodes:",
        "          - { id: b1, kind: task, writes: [s], emits: { x: a } }",
        "        edges: []",
        "      max_iterations: 2",
        "      safety_points: []",
        "    emits: { answer: { from_body: b1.x } }",
        "edges: []",
        "policies:",
        "  budgets: []",
        "  stop: []",
        "  failures: []",
    ]
    .join("\n")
        .replace("      safety_points: []\n", &format!("      safety_points: []\n{carried}"));
    let cs = codes(&src);
    if covered {
        assert!(cs.is_empty());
    } else {
        assert_eq!(cs, vec!["E2211"]);
    }
}

/// 正控制：golden sample1（2025B，含短 CEL `when` 与 route）整链全绿。
#[kani::proof]
fn golden_sample1_full_chain_passes() {
    const SAMPLE1: &str = include_str!("sample1.yaml");
    match af_dsl::load_str(SAMPLE1) {
        Ok(doc) => assert!(af_dsl::validate(&doc).is_ok()),
        Err(_) => assert!(false),
    }
}
