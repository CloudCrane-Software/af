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

/// 语义层（validate）违例码集——形状门 E1003 经 validate 一次收齐面断言。
fn codes(src: &str) -> Vec<&'static str> {
    let doc: Document = load_str(src).unwrap_or_else(|e| panic!("解析失败：{e}"));
    af_dsl::validate(&doc).codes()
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
        assert_eq!(
            parsed.nodes[0].escape.as_ref().unwrap().impl_ref,
            "p/x@1/h.py"
        );
    }
}

// ---------- E1001 未知字段（deny_unknown_fields，逐层） ----------

#[test]
fn unknown_field_in_meta_is_e1001() {
    let src = doc("  - id: n1\n    kind: task\n    emits: { answer: a }", "")
        .replace("  entry: n1", "  entry: n1\n  bogus_meta: 1");
    assert_eq!(shape_codes(&src), ["E1001"]);
}

#[test]
fn unknown_field_in_node_is_e1001() {
    let src = doc(
        "  - id: n1\n    kind: task\n    bogus_node: 1\n    emits: { answer: a }",
        "",
    );
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
fn body_without_nodes_is_e1002() {
    // 注意：body.edges 可缺省（golden 样例 6 的 explore 体无 edges 键），
    // 缺 nodes 才是 E1002。
    let src = doc(
        "  - id: n1\n    kind: task\n    map:\n      over: input.q\n      body: { edges: [] }\n    emits: { answer: a }",
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
    let src = doc(
        "  - id: n1\n    kind: task\n    emits: { answer: \"list<<\" }",
        "",
    );
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

// ---------- 红队 W3-C17：负值入无符号字段归 E2002 值域（非 E1004 枚举） ----------

#[test]
fn negative_into_unsigned_is_e2002_not_e1004() {
    // serde 对无符号域报 `invalid value: integer -1, expected u32`——类型家族
    // 对、值越界，属 E2002 值域（error.rs:62-63）；原映射把它与 unknown
    // variant 一并塞 E1004「枚举值非法」，误导排障方向。
    let cases = [
        // loop max_iterations:-1（expected u32）
        (
            "  - id: n1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: -1\n      safety_points: [ { node: g, kind: gate } ]\n    emits: { answer: a }",
            "",
        ),
        // store retention.max_items:-1（expected usize）
        (
            "  - id: n1\n    kind: task\n    emits: { answer: a }",
            "stores:\n  - { name: s, type: kv, writer: n1, lifecycle: run, retention: { max_items: -1 } }\n",
        ),
        // context quota_tokens:-5（expected u64）
        (
            "  - id: n1\n    kind: task\n    context:\n      sections:\n        - { from: root, template_ref: pkg.x.t@1, quota_tokens: -5 }\n    emits: { answer: a }",
            "",
        ),
    ];
    for (nodes, stores) in cases {
        assert_eq!(shape_codes(&doc(nodes, stores)), ["E2002"]);
    }
    // 对照组：枚举未知值仍归 E1004（词表域不受本次二分影响）。
    let src = doc(
        "  - id: n1\n    kind: task\n    emits: { answer: a }",
        "stores:\n  - { name: s, type: queue, writer: n1, lifecycle: run, retention: { max_items: 8 } }\n",
    );
    assert_eq!(shape_codes(&src), ["E1004"]);
}

// ---------- 红队 W3-C18：types[].schema 形状强制（E1003） ----------

#[test]
fn non_object_schema_is_e1003() {
    // model.rs 自述 schema 为「内联 JSON Schema 对象」，实现原为裸 Value 零
    // 校验——标量/数组/布尔/null 形状零违例过审，CEL 合成绑定退化为 Null、
    // 求值错误不判 ⇒ 布尔判定静默无证据。现形状层 fail-loud（E1003）。
    let bad_schemas = [
        ("字符串标量", "x"),
        ("数字", "42"),
        ("数组", "[1, 2]"),
        ("布尔", "true"),
        ("null", "null"),
        ("空对象（无 type）", "{}"),
        ("type 非六型", "{ type: blob }"),
        (
            "properties 子节点无 type",
            "{ type: object, properties: { r: {} } }",
        ),
    ];
    for (label, schema) in bad_schemas {
        let src = format!(
            "version: 1\nmeta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: {{ ref: a, from: n1 }}\ntypes:\n  - {{ name: in_t, schema: {{ type: object, required: [q], properties: {{ q: {{ type: string }} }} }} }}\n  - {{ name: a, schema: {schema} }}\nnodes:\n  - id: n1\n    kind: task\n    uses: {{ model: route.main@1 }}\n    emits: {{ answer: a }}\nedges: []\npolicies:\n  budgets: []\n  stop: []\n  failures: []\n"
        );
        assert_eq!(codes(&src), ["E1003"], "{label} 应被形状门拒（E1003）");
    }
    // 正例：六型与嵌套 properties/items 合法（含内联原始型与 list items）。
    let ok = "version: 1\nmeta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: a, from: n1 }\ntypes:\n  - { name: in_t, schema: { type: object, required: [q], properties: { q: { type: string } } } }\n  - { name: a, schema: { type: object, required: [items], properties: { items: { type: array, items: { type: integer } }, flag: { type: boolean } } } }\nnodes:\n  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\nedges: []\npolicies:\n  budgets: []\n  stop: []\n  failures: []\n";
    assert_eq!(codes(ok), Vec::<&str>::new(), "合法 schema 形状零违例");
}

// ---------- 红队 W2-C-24：human.resume.schema 同门（E1003） ----------

#[test]
fn human_resume_schema_shapes_are_gated_e1003() {
    // human.resume.schema 与 types[].schema 同为裸 serde_json::Value、同走
    // cel::synth_from_schema（validate/m4.rs add_resume）——W3-C18 只门了
    // types 面，标量/数组/null resume schema 仍零违例过审：resume 绑定退化为
    // Null、human 节点 route 的 CEL 布尔判定静默无证据。同规则 fail-loud。
    let bad_schemas = [
        ("字符串标量", "x"),
        ("数字", "42"),
        ("数组", "[1, 2]"),
        ("布尔", "true"),
        ("null", "null"),
        ("空对象（无 type）", "{}"),
        ("type 非六型且非声明名", "{ type: blob }"),
        (
            "properties 子节点无 type",
            "{ type: object, properties: { approved: {} } }",
        ),
    ];
    for (label, schema) in bad_schemas {
        let src = format!(
            "version: 1\nmeta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: {{ ref: a, from: n1 }}\ntypes:\n  - {{ name: in_t, schema: {{ type: object }} }}\n  - {{ name: a, schema: {{ type: object }} }}\nnodes:\n  - id: n1\n    kind: task\n    human: {{ station: s1, role: approver, resume: {{ schema: {schema} }} }}\n    emits: {{ answer: a }}\nedges: []\npolicies:\n  budgets: []\n  stop: []\n  failures: []\n"
        );
        assert_eq!(
            codes(&src),
            ["E1003"],
            "{label} 应被 human.resume.schema 形状门拒（E1003）"
        );
    }
    // 正例：合法对象（六型 + 递归 properties + 已声明类型命名引用）零违例。
    let ok = "version: 1\nmeta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: a, from: n1 }\ntypes:\n  - { name: in_t, schema: { type: object } }\n  - { name: a, schema: { type: object } }\nnodes:\n  - id: n1\n    kind: task\n    human:\n      station: s1\n      role: approver\n      resume:\n        schema: { type: object, required: [approved], properties: { approved: { type: boolean }, note: { type: in_t } } }\n    emits: { answer: a }\nedges: []\npolicies:\n  budgets: []\n  stop: []\n  failures: []\n";
    assert_eq!(
        codes(ok),
        Vec::<&str>::new(),
        "合法 human.resume.schema 零违例"
    );
}

// ---------- 红队 W3-C15：human.resume.schema 门覆盖 loop/map 体内嵌 human ----------

#[test]
fn nested_human_resume_schema_shapes_are_gated_e1003() {
    // check_schema_shape 原只遍历根层 doc.nodes——loop/map 体内的 human 节点
    // 逃逸门禁。而嵌套 human 恰是全部真实语料的形态（ADAS human_stop /
    // sample5 approve / SWE-agent human_help 均在构造体内），add_resume 对
    // 其 resume.schema 走同一 synth_from_schema——标量 schema ⇒ resume 绑定
    // 退化为 Null、该 human 的 route CEL 布尔判定静默无证据。同门必须同
    // 覆盖：体内 human 标量/无 type schema → E1003。
    for (label, schema) in [("字符串标量", "x"), ("空对象（无 type）", "{}")] {
        let src = format!(
            "version: 1\nmeta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: {{ ref: a, from: n1 }}\ntypes:\n  - {{ name: in_t, schema: {{ type: object }} }}\n  - {{ name: a, schema: {{ type: object }} }}\nnodes:\n  - id: n1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - id: approve\n            kind: task\n            human: {{ station: s1, role: approver, resume: {{ schema: {schema} }} }}\n            route:\n              default: exit\n              rules:\n                - {{ when: \"resume.approved == false\", goto: exit }}\n          - {{ id: g, kind: gate, assert: \"true\" }}\n        edges: []\n      max_iterations: 2\n      safety_points: [ {{ node: g, kind: gate }} ]\n    emits: {{ answer: a }}\nedges: []\npolicies:\n  budgets: []\n  stop: []\n  failures: []\n"
        );
        assert_eq!(
            codes(&src),
            ["E1003"],
            "loop 体内 human.resume.schema {label} 应被同门拒（E1003；W3-C15）"
        );
    }
    // map 体同门。
    let map_src = "version: 1\nmeta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: a, from: n1 }\ntypes:\n  - { name: in_t, schema: { type: object, required: [q], properties: { q: { type: array, items: { type: string } } } } }\n  - { name: a, schema: { type: object } }\nnodes:\n  - id: n1\n    kind: task\n    map:\n      over: input.q\n      body:\n        nodes:\n          - { id: approve, kind: task, human: { station: s1, role: approver, resume: { schema: 42 } } }\n        edges: []\n    emits: { answer: a }\nedges: []\npolicies:\n  budgets: []\n  stop: []\n  failures: []\n";
    assert_eq!(
        codes(map_src),
        ["E1003"],
        "map 体内 human.resume.schema 标量应被同门拒（E1003；W3-C15）"
    );
    // 正例：体内 human 合法对象 schema 零违例（真实语料形态）。
    let ok = "version: 1\nmeta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: a, from: n1 }\ntypes:\n  - { name: in_t, schema: { type: object } }\n  - { name: a, schema: { type: object } }\nnodes:\n  - id: n1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - id: approve\n            kind: task\n            human:\n              station: s1\n              role: approver\n              resume:\n                schema: { type: object, required: [approved], properties: { approved: { type: boolean } } }\n            route:\n              default: exit\n              rules:\n                - { when: \"resume.approved == false\", goto: exit }\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      safety_points: [ { node: g, kind: gate } ]\n    emits: { answer: a }\nedges: []\npolicies:\n  budgets: []\n  stop: []\n  failures: []\n";
    assert_eq!(
        codes(ok),
        Vec::<&str>::new(),
        "loop 体内 human 合法 resume.schema 零违例（W3-C15 不误伤）"
    );
}
