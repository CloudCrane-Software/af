//! 七段 serde 解析验收测试（M1）：6 golden 样例 + 全文 deny_unknown_fields
//! （E1001）+ 缺必填（E1002）+ 类型不符/内联对象/类型表达式（E1003）+
//! 枚举词表（E1004，含裁决 §二-2 escape.kind 7 类闭环词表）+
//! 拓扑选择器（裁决 §二-1，M1 只解析结构）。

use af_dsl::{load_str, Document, LoadError};

/// 最小合法文档：单 task 节点，供各负例变体拼接。
fn doc(nodes: &str, extra: &str) -> String {
    format!(
        "version: 1\n\
meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: {{ ref: a, from: n1 }}\n\
types:\n  - {{ name: in_t, schema: {{ type: object }} }}\n  - {{ name: a, schema: {{ type: object }} }}\n\
nodes:\n{nodes}\n{extra}"
    )
}

fn shape_codes(src: &str) -> Vec<&'static str> {
    match load_str(src) {
        Err(LoadError::Shape(r)) => r.codes(),
        other => panic!("期望形状层拒绝，实得 {other:?}"),
    }
}

// ---------- golden 样例：解析通过 ----------

const SAMPLE_NAMES: [(&str, &str); 6] = [
    ("../testdata/sample1.yaml", "react_mono"),
    ("../testdata/sample2.yaml", "metagpt_sop"),
    ("../testdata/sample3.yaml", "debate_kv"),
    ("../testdata/sample4.yaml", "aflow_search"),
    ("../testdata/sample5.yaml", "langgraph_subset"),
    ("../testdata/sample6.yaml", "voyager_curriculum"),
];

#[test]
fn six_golden_samples_parse() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata");
    for (file, name) in SAMPLE_NAMES {
        let path = format!("{dir}/{file}");
        let src = std::fs::read_to_string(&path).unwrap();
        let doc = load_str(&src).unwrap_or_else(|e| panic!("{path} 解析失败：{e}"));
        assert_eq!(doc.meta.name, name, "{path}");
        assert_eq!(doc.version, 1);
    }
}

#[test]
fn topology_selector_parses_structurally() {
    // 裁决 §二-1：meta.topology_selector 编译期解析语义，M1 只解析结构。
    let src = doc(
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }",
        "policies: { budgets: [], stop: [], failures: [] }\n",
    )
    .replace(
        "  output: { ref: a, from: n1 }",
        "  output: { ref: a, from: n1 }\n  topology_selector:\n    rules:\n      - { when: \"input.q == 'x'\", use: pkg.graphs.g1@1 }\n    fallback: pkg.graphs.g0@1",
    );
    let doc: Document = load_str(&src).unwrap_or_else(|e| panic!("{e}"));
    let sel = doc.meta.topology_selector.expect("selector 应解析");
    assert_eq!(sel.fallback, "pkg.graphs.g0@1");
    assert_eq!(sel.rules.len(), 1);
    assert_eq!(sel.rules[0].use_graph, "pkg.graphs.g1@1");
}

#[test]
fn escape_with_adjudicated_kind_parses() {
    // 裁决 §二-2：escape.kind 闭环词表 7 类；合法值必须可解析。
    for kind in [
        "dynamic_tool_registry",
        "program_synthesis",
        "meta_loop",
        "runtime_context_mutation",
        "env_adapter",
        "free_text_parser",
        "llm_predicate",
    ] {
        let src = doc(
            &format!(
                "  - id: n1\n    kind: task\n    escape:\n      kind: {kind}\n      impl_ref: p/x@1/h.py\n      reason: r\n      owner: team\n      share_budget_pct: 10\n    emits: {{ answer: a }}"
            ),
            "policies: { budgets: [], stop: [], failures: [] }\n",
        );
        let parsed: Document = load_str(&src).unwrap_or_else(|e| panic!("{kind} 应合法：{e}"));
        assert_eq!(parsed.nodes[0].escape.as_ref().unwrap().impl_ref, "p/x@1/h.py");
    }
}

// ---------- E1001 未知字段（deny_unknown_fields，逐层） ----------

#[test]
fn unknown_field_in_meta_is_e1001() {
    let src = doc(
        "  - id: n1\n    kind: task\n    emits: { answer: a }",
        "",
    ).replace("  entry: n1", "  entry: n1\n  bogus_meta: 1");
    assert_eq!(shape_codes(&src), ["E1001"]);
}

#[test]
fn unknown_field_in_node_is_e1001() {
    let src = doc("  - id: n1\n    kind: task\n    bogus_node: 1\n    emits: { answer: a }", "");
    assert_eq!(shape_codes(&src), ["E1001"]);
}

#[test]
fn unknown_field_in_uses_is_e1001() {
    let src = doc(
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1, engine: x }\n    emits: { answer: a }",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1001"]);
}

#[test]
fn unknown_field_in_store_is_e1001() {
    let src = doc(
        "  - id: n1\n    kind: task\n    writes: [s]\n    emits: { answer: a }",
        "stores:\n  - { name: s, type: kv, value: { type: string }, writer: n1, lifecycle: run, retention: { max_items: 8 }, readers: [] }\n",
    );
    assert_eq!(shape_codes(&src), ["E1001"]);
}

#[test]
fn unknown_field_in_edge_is_e1001() {
    let src = doc(
        "  - id: n1\n    kind: task\n    emits: { answer: a }\n  - id: n2\n    kind: task",
        "edges:\n  - { from: n1, output: answer, to: n2, input: x, when: \"true\" }\n",
    );
    assert_eq!(shape_codes(&src), ["E1001"]);
}

#[test]
fn unknown_field_in_loop_is_e1001() {
    let src = doc(
        "  - id: n1\n    kind: task\n    loop:\n      body: { nodes: [], edges: [] }\n      max_iterations: 3\n      safety_points: []\n      until: true\n    emits: { answer: a }",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1001"]);
}

#[test]
fn unknown_field_in_carried_is_e1001() {
    let src = doc(
        "  - id: n1\n    kind: task\n    loop:\n      body: { nodes: [], edges: [] }\n      max_iterations: 3\n      carried: [ { store: s, limit: 4 } ]\n      safety_points: []\n    emits: { answer: a }",
        "stores:\n  - { name: s, type: kv, value: { type: string }, writer: n1, lifecycle: run, retention: { max_items: 8 } }\n",
    );
    assert_eq!(shape_codes(&src), ["E1001"]);
}

#[test]
fn unknown_field_in_safety_point_is_e1001() {
    let src = doc(
        "  - id: n1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 3\n      safety_points: [ { node: g, kind: gate, strict: true } ]\n    emits: { answer: a }",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1001"]);
}

#[test]
fn unknown_field_in_route_is_e1001() {
    let src = doc(
        "  - id: n1\n    kind: task\n    route:\n      default: n2\n      rules: [ { when: \"true\", goto: n2 } ]\n      exhaustive: true\n    emits: { answer: a }\n  - id: n2\n    kind: task",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1001"]);
}

#[test]
fn unknown_field_in_human_is_e1001() {
    let src = doc(
        "  - id: n1\n    kind: task\n    human:\n      station: desk\n      role: reviewer\n      resume: { schema: { type: object } }\n      timeout_ms: 1000\n    emits: { answer: a }",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1001"]);
}

#[test]
fn unknown_field_in_escape_is_e1001() {
    let src = doc(
        "  - id: n1\n    kind: task\n    escape:\n      impl_ref: p\n      reason: r\n      owner: o\n      share_budget_pct: 5\n      sandbox: true\n    emits: { answer: a }",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1001"]);
}

#[test]
fn unknown_field_in_section_is_e1001() {
    let src = doc(
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    context:\n      sections:\n        - { from: template, template_ref: pkg.t, quota_tokens: 100, merge: by_step }\n    emits: { answer: a }",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1001"]);
}

#[test]
fn unknown_field_in_op_is_e1001() {
    let src = doc(
        "  - id: n1\n    kind: transform\n    ops: [ { op: project, port: p, into: q, bogus: 1 } ]\n    emits: { answer: a }",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1001"]);
}

#[test]
fn unknown_field_in_budget_is_e1001() {
    let src = doc(
        "  - id: n1\n    kind: task\n    emits: { answer: a }",
        "policies:\n  budgets: [ { id: b, dimension: turns, scope: plan, cap: 5, currency: usd } ]\n  stop: []\n  failures: []\n",
    );
    assert_eq!(shape_codes(&src), ["E1001"]);
}

#[test]
fn unknown_field_in_topology_selector_is_e1001() {
    let src = doc(
        "  - id: n1\n    kind: task\n    emits: { answer: a }",
        "",
    ).replace(
        "  entry: n1",
        "  entry: n1\n  topology_selector:\n    rules: [ { when: \"true\", use: g1 } ]\n    fallback: g0\n    dry_run: true",
    );
    assert_eq!(shape_codes(&src), ["E1001"]);
}

// ---------- E1002 缺必填字段 ----------

#[test]
fn route_without_default_is_e1002() {
    // 提案 B §1.3：default 必填，缺 → E1002（serde 层即拒）。
    let src = doc(
        "  - id: n1\n    kind: task\n    route:\n      rules: [ { when: \"true\", goto: n2 } ]\n    emits: { answer: a }\n  - id: n2\n    kind: task",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1002"]);
}

#[test]
fn node_without_id_is_e1002() {
    let src = doc("  - kind: task\n    emits: { answer: a }", "");
    assert_eq!(shape_codes(&src), ["E1002"]);
}

#[test]
fn escape_without_accounting_field_is_e1002() {
    let src = doc(
        "  - id: n1\n    kind: task\n    escape:\n      impl_ref: p\n      reason: r\n      owner: o\n    emits: { answer: a }",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1002"]);
}

#[test]
fn edge_without_from_is_e1002() {
    let src = doc(
        "  - id: n1\n    kind: task\n    emits: { answer: a }",
        "edges:\n  - { output: answer, to: n1, input: x }\n",
    );
    assert_eq!(shape_codes(&src), ["E1002"]);
}

#[test]
fn body_without_edges_is_e1002() {
    let src = doc(
        "  - id: n1\n    kind: task\n    map:\n      over: input.q\n      body: { nodes: [] }\n    emits: { answer: a }",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1002"]);
}

// ---------- E1003 类型不符 / 内联对象 / 类型表达式 ----------

#[test]
fn non_numeric_max_iterations_is_e1003() {
    let src = doc(
        "  - id: n1\n    kind: task\n    loop:\n      body: { nodes: [], edges: [] }\n      max_iterations: three\n      safety_points: []\n    emits: { answer: a }",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1003"]);
}

#[test]
fn inline_object_schema_on_port_is_e1003() {
    // 对象 schema 必须住 types 表（提案 B §1.2）。
    let src = doc(
        "  - id: n1\n    kind: task\n    emits: { answer: { type: object, properties: {} } }",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1003"]);
}

#[test]
fn from_body_with_extra_key_is_rejected() {
    // untagged FromBody 变体 deny_unknown_fields：多余键不得静默通过。
    let src = doc(
        "  - id: n1\n    kind: task\n    map:\n      over: input.q\n      body:\n        nodes:\n          - { id: m, kind: task, emits: { x: a } }\n        edges: []\n    emits: { out: { from_body: m.x, bogus: 1 } }",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1003"]);
}

#[test]
fn malformed_type_expression_is_e1003() {
    let src = doc("  - id: n1\n    kind: task\n    emits: { answer: \"list<<\" }", "");
    assert_eq!(shape_codes(&src), ["E1003"]);
}

#[test]
fn nested_generic_type_expression_is_e1003() {
    // YAML 面只允许一层泛型。
    let src = doc(
        "  - id: n1\n    kind: task\n    emits: { answer: \"list<list<a>>\" }",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1003"]);
}

#[test]
fn non_type_port_value_is_e1003() {
    let src = doc("  - id: n1\n    kind: task\n    emits: { answer: 5 }", "");
    assert_eq!(shape_codes(&src), ["E1003"]);
}

// ---------- E1004 枚举值非法（serde 词表） ----------

#[test]
fn unknown_store_type_is_e1004() {
    let src = doc(
        "  - id: n1\n    kind: task\n    emits: { answer: a }",
        "stores:\n  - { name: s, type: queue, writer: n1, lifecycle: run, retention: { max_items: 8 } }\n",
    );
    assert_eq!(shape_codes(&src), ["E1004"]);
}

#[test]
fn unknown_lifecycle_is_e1004() {
    let src = doc(
        "  - id: n1\n    kind: task\n    emits: { answer: a }",
        "stores:\n  - { name: s, type: kv, writer: n1, lifecycle: daily, retention: { max_items: 8 } }\n",
    );
    assert_eq!(shape_codes(&src), ["E1004"]);
}

#[test]
fn safety_point_kind_evaluator_is_e1004() {
    // 裁决 §三-5：safety_points kind 保持 af-graph 三值原样（evaluator 不收）。
    let src = doc(
        "  - id: n1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 3\n      safety_points: [ { node: g, kind: evaluator } ]\n    emits: { answer: a }",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1004"]);
}

#[test]
fn escape_kind_outside_seven_is_e1004() {
    let src = doc(
        "  - id: n1\n    kind: task\n    escape:\n      kind: arbitrary_python\n      impl_ref: p\n      reason: r\n      owner: o\n      share_budget_pct: 5\n    emits: { answer: a }",
        "",
    );
    assert_eq!(shape_codes(&src), ["E1004"]);
}

#[test]
fn unknown_budget_dimension_is_e1004() {
    let src = doc(
        "  - id: n1\n    kind: task\n    emits: { answer: a }",
        "policies:\n  budgets: [ { id: b, dimension: dollars, scope: plan, cap: 5 } ]\n  stop: []\n  failures: []\n",
    );
    assert_eq!(shape_codes(&src), ["E1004"]);
}

#[test]
fn unknown_stop_outcome_is_e1004() {
    let src = doc(
        "  - id: n1\n    kind: task\n    emits: { answer: a }",
        "policies:\n  budgets: []\n  stop: [ { when: \"true\", outcome: paused } ]\n  failures: []\n",
    );
    assert_eq!(shape_codes(&src), ["E1004"]);
}

#[test]
fn unknown_failure_class_is_e1004() {
    let src = doc(
        "  - id: n1\n    kind: task\n    emits: { answer: a }",
        "policies:\n  budgets: []\n  stop: []\n  failures: [ { class: unknown_error } ]\n",
    );
    assert_eq!(shape_codes(&src), ["E1004"]);
}

#[test]
fn unknown_retention_policy_is_e1004() {
    let src = doc(
        "  - id: n1\n    kind: task\n    emits: { answer: a }",
        "stores:\n  - { name: s, type: kv, writer: n1, lifecycle: run, retention: { policy: fifo } }\n",
    );
    assert_eq!(shape_codes(&src), ["E1004"]);
}
