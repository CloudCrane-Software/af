//! 引用完整性验收测试（M1）：类型引用 / 节点引用 / store 引用 / 端口引用，
//! 稳定错误码精确断言（每例断言精确码集，防无关码噪声）。
//!
//! 码位：E2001 悬空 / E2003 重复 id / E2104 over 非 list / E2105 args·over 悬空 /
//! E2106 from_body 悬空 / E2107 safety_point 悬空 / E2201 边端口与类型 /
//! E2202 output.from 不发射 / E2204 failures 路由悬空 / E2205 未知类型名 /
//! E2207 预算 cap·作用域 / E2208 retention 无界。

use af_dsl::{load_str, validate, Document};

const DEF_META: &str = "meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: a, from: n1 }\n";
const DEF_TYPES: &str = "types:\n  - { name: in_t, schema: { type: object, required: [q], properties: { q: { type: string } } } }\n  - { name: a, schema: { type: object, required: [text], properties: { text: { type: string } } } }\n";
const DEF_POLICIES: &str = "policies:\n  budgets: []\n  stop: []\n  failures: []\n";

fn build(
    meta: &str,
    types: &str,
    stores: &str,
    nodes: &str,
    edges: &str,
    policies: &str,
) -> String {
    let stores = if stores.is_empty() {
        String::new()
    } else {
        format!("stores:\n{stores}")
    };
    let edges = if edges.is_empty() {
        "edges: []\n".to_string()
    } else {
        format!("edges:\n{edges}")
    };
    format!("version: 1\n{meta}{types}{stores}nodes:\n{nodes}{edges}{policies}")
}

fn base() -> String {
    build(
        DEF_META,
        DEF_TYPES,
        "",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\n",
        "",
        DEF_POLICIES,
    )
}

fn check(src: &str) -> Vec<&'static str> {
    let doc: Document = load_str(src).unwrap_or_else(|e| panic!("解析失败：{e}"));
    validate(&doc).codes()
}

// ---------- 基线正例 ----------

#[test]
fn minimal_base_doc_validates_clean() {
    assert_eq!(check(&base()), Vec::<&str>::new());
}

#[test]
fn same_id_in_different_scopes_is_ok() {
    // id 唯一性是「所在作用域」语义（提案 B §1.2 nodes.id）。
    let src = build(
        DEF_META,
        DEF_TYPES,
        "",
        "  - id: n1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - { id: inner, kind: task, emits: { x: a } }\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      safety_points: [ { node: g, kind: gate } ]\n    emits: { answer: { from_body: inner.x } }\n  - id: inner\n    kind: task\n    emits: { y: a }\n",
        "",
        DEF_POLICIES,
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn loop_node_builtin_done_port_feeds_edge() {
    // loop/map 节点自带 done: signal 端口（提案 B §1.3）；无 consumes 的消费者不查输入侧。
    let src = build(
        DEF_META,
        DEF_TYPES,
        "",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\n  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      safety_points: [ { node: g, kind: gate } ]\n  - id: n2\n    kind: transform\n    ops: [ { op: project, port: p, into: q } ]\n",
        "  - { from: l1, output: done, to: n2, input: done }\n",
        DEF_POLICIES,
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

// ---------- E2001 悬空引用 ----------

#[test]
fn entry_ghost_is_e2001() {
    let src = base().replace("  entry: n1", "  entry: ghost");
    assert_eq!(check(&src), ["E2001"]);
}

#[test]
fn output_from_ghost_is_e2001() {
    let src = base().replace(
        "output: { ref: a, from: n1 }",
        "output: { ref: a, from: ghost }",
    );
    assert_eq!(check(&src), ["E2001"]);
}

#[test]
fn edge_endpoint_ghost_is_e2001() {
    let src = base().replace(
        "edges: []",
        "edges:\n  - { from: n1, output: answer, to: ghost, input: x }",
    );
    assert_eq!(check(&src), ["E2001"]);
}

#[test]
fn writes_unknown_store_is_e2001() {
    let src = base().replace(
        "    uses: { model: route.main@1 }",
        "    uses: { model: route.main@1 }\n    writes: [ghost_s]",
    );
    assert_eq!(check(&src), ["E2001"]);
}

#[test]
fn reads_unknown_store_is_e2001() {
    let src = base().replace(
        "    uses: { model: route.main@1 }",
        "    uses: { model: route.main@1 }\n    reads: [ghost_s]",
    );
    assert_eq!(check(&src), ["E2001"]);
}

#[test]
fn carried_unknown_store_is_e2001() {
    let src = base().replace(
        "    emits: { answer: a }",
        "    loop:\n      body:\n        nodes:\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      carried: [ { store: ghost_s, max_items: 4 } ]\n      safety_points: [ { node: g, kind: gate } ]\n    emits: { answer: a }",
    );
    assert_eq!(check(&src), ["E2001"]);
}

#[test]
fn writer_path_ghost_is_only_e2001() {
    // 写者路径悬空 → 只报 E2001；INV-2 对该 store 跳过（不产生 E2311 噪声）。
    let src = build(
        DEF_META,
        DEF_TYPES,
        "  - { name: s, type: kv, value: { type: string }, writer: ghost.x, lifecycle: run, retention: { max_items: 8 } }\n",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    writes: [s]\n    emits: { answer: a }\n",
        "",
        DEF_POLICIES,
    );
    assert_eq!(check(&src), ["E2001"]);
}

// ---------- E2003 重复 id ----------

#[test]
fn duplicate_node_ids_is_e2003() {
    let src = base().replace(
        "    emits: { answer: a }\n",
        "    emits: { answer: a }\n  - id: n1\n    kind: task\n",
    );
    assert_eq!(check(&src), ["E2003"]);
}

#[test]
fn duplicate_type_names_is_e2003() {
    let src = base().replace(
        "  - { name: a, schema: { type: object, required: [text], properties: { text: { type: string } } } }",
        "  - { name: a, schema: { type: object } }\n  - { name: a, schema: { type: object } }",
    );
    assert_eq!(check(&src), ["E2003"]);
}

#[test]
fn duplicate_store_names_is_e2003() {
    let src = build(
        DEF_META,
        DEF_TYPES,
        "  - { name: s, type: kv, value: { type: string }, writer: n1, lifecycle: run, retention: { max_items: 8 } }\n  - { name: s, type: kv, value: { type: string }, writer: n1, lifecycle: run, retention: { max_items: 8 } }\n",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\n",
        "",
        DEF_POLICIES,
    );
    assert_eq!(check(&src), ["E2003"]);
}

// ---------- E2205 未知类型名 ----------

#[test]
fn emits_unknown_type_is_e2205() {
    let src = base().replace("emits: { answer: a }", "emits: { answer: ghost_t }");
    assert_eq!(check(&src), ["E2205"]);
}

#[test]
fn list_payload_unknown_type_is_e2205() {
    let src = base().replace(
        "emits: { answer: a }",
        "emits: { answer: \"list<ghost_t>\" }",
    );
    assert_eq!(check(&src), ["E2205"]);
}

#[test]
fn envelope_payload_unknown_type_is_e2205() {
    let src = base().replace(
        "emits: { answer: a }",
        "emits: { answer: \"envelope<ghost_t>\" }",
    );
    assert_eq!(check(&src), ["E2205"]);
}

#[test]
fn store_item_unknown_type_is_e2205() {
    let src = build(
        DEF_META,
        DEF_TYPES,
        "  - { name: s, type: collection, item: ghost_t, writer: n1, lifecycle: run, retention: { max_items: 8 } }\n",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\n",
        "",
        DEF_POLICIES,
    );
    assert_eq!(check(&src), ["E2205"]);
}

#[test]
fn meta_input_unknown_type_is_e2205() {
    let src = base().replace("  input: in_t", "  input: ghost_t");
    assert_eq!(check(&src), ["E2205"]);
}

// ---------- E2201 边端口与类型 ----------

#[test]
fn edge_output_port_missing_is_e2201() {
    let src = base()
        .replace(
            "edges: []",
            "edges:\n  - { from: n1, output: nope, to: n2, input: x }",
        )
        .replace(
            "    emits: { answer: a }\n",
            "    emits: { answer: a }\n  - id: n2\n    kind: task\n",
        );
    assert_eq!(check(&src), ["E2201"]);
}

#[test]
fn edge_input_port_not_consumed_is_e2201() {
    // 消费者声明了 consumes：入边端口必须 ∈ consumes 键集。
    let src = base()
        .replace(
            "edges: []",
            "edges:\n  - { from: n1, output: answer, to: n2, input: nope }",
        )
        .replace(
            "    emits: { answer: a }\n",
            "    emits: { answer: a }\n  - id: n2\n    kind: task\n    consumes: { answer: a }\n",
        );
    assert_eq!(check(&src), ["E2201"]);
}

#[test]
fn edge_type_mismatch_is_e2201() {
    let src = base()
        .replace(
            "edges: []",
            "edges:\n  - { from: n1, output: answer, to: n2, input: x }",
        )
        .replace(
            "    emits: { answer: a }\n",
            "    emits: { answer: a }\n  - id: n2\n    kind: task\n    consumes: { x: in_t }\n",
        );
    assert_eq!(check(&src), ["E2201"]);
}

// ---------- E2106 from_body 悬空 ----------

#[test]
fn from_body_node_dangling_is_e2106() {
    let src = base().replace(
        "    emits: { answer: a }",
        "    loop:\n      body:\n        nodes:\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      safety_points: [ { node: g, kind: gate } ]\n    emits: { answer: { from_body: ghost.port } }",
    );
    assert_eq!(check(&src), ["E2106"]);
}

#[test]
fn from_body_port_dangling_is_e2106() {
    let src = base().replace(
        "    emits: { answer: a }",
        "    loop:\n      body:\n        nodes:\n          - { id: inner, kind: task, emits: { x: a } }\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      safety_points: [ { node: g, kind: gate } ]\n    emits: { answer: { from_body: inner.nope } }",
    );
    assert_eq!(check(&src), ["E2106"]);
}

// ---------- E2107 safety_point 悬空 ----------

#[test]
fn safety_point_dangling_is_e2107() {
    let src = base().replace(
        "    emits: { answer: a }",
        "    loop:\n      body:\n        nodes:\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      safety_points: [ { node: nope.deep, kind: gate } ]\n    emits: { answer: a }",
    );
    assert_eq!(check(&src), ["E2107"]);
}

// ---------- E2105 map.over / loop.args 引用悬空 ----------

#[test]
fn map_over_node_dangling_is_e2105() {
    let src = base().replace(
        "    emits: { answer: a }",
        "    map:\n      over: ghost.port\n      body: { nodes: [], edges: [] }\n    emits: { answer: a }",
    );
    assert_eq!(check(&src), ["E2105"]);
}

#[test]
fn map_over_input_field_dangling_is_e2105() {
    let src = base().replace(
        "    emits: { answer: a }",
        "    map:\n      over: input.nope\n      body: { nodes: [], edges: [] }\n    emits: { answer: a }",
    );
    assert_eq!(check(&src), ["E2105"]);
}

#[test]
fn loop_args_ref_dangling_is_e2105() {
    let src = base().replace(
        "    emits: { answer: a }",
        "    loop:\n      args: { g: ghost.port }\n      body:\n        nodes:\n          - { id: inner, kind: task, emits: { x: a } }\n          - { id: sg, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      safety_points: [ { node: sg, kind: gate } ]\n    emits: { answer: { from_body: inner.x } }",
    );
    assert_eq!(check(&src), ["E2105"]);
}

// ---------- E2104 map.over 非 list ----------

#[test]
fn map_over_port_not_list_is_e2104() {
    let src = base().replace(
        "    emits: { answer: a }",
        "    map:\n      over: n1.answer\n      body: { nodes: [], edges: [] }\n    emits: { answer: a }",
    );
    assert_eq!(check(&src), ["E2104"]);
}

#[test]
fn map_over_input_field_not_array_is_e2104() {
    // meta.input schema 的 q 是 string，不是 array。
    let src = base().replace(
        "    emits: { answer: a }",
        "    map:\n      over: input.q\n      body: { nodes: [], edges: [] }\n    emits: { answer: a }",
    );
    assert_eq!(check(&src), ["E2104"]);
}

// ---------- E2202 output.from 不发射该类型 ----------

#[test]
fn output_type_not_emitted_is_e2202() {
    // 增加已声明类型 b；n1 只发射 a，output.ref 却要 b。
    let src = base()
        .replace("  output: { ref: a, from: n1 }", "  output: { ref: b, from: n1 }")
        .replace(
            "  - { name: a, schema: { type: object, required: [text], properties: { text: { type: string } } } }",
            "  - { name: a, schema: { type: object, required: [text], properties: { text: { type: string } } } }\n  - { name: b, schema: { type: object } }",
        );
    assert_eq!(check(&src), ["E2202"]);
}

// ---------- E2204 failures 路由悬空 ----------

#[test]
fn failure_route_dangling_is_e2204() {
    let src = base().replace(
        "  failures: []",
        "  failures:\n    - { class: tool_error, route: ghost }",
    );
    assert_eq!(check(&src), ["E2204"]);
}

#[test]
fn failure_fallback_route_dangling_is_e2204() {
    let src = base().replace(
        "  failures: []",
        "  failures:\n    - { class: schema_invalid, fallback_route: ghost }",
    );
    assert_eq!(check(&src), ["E2204"]);
}

#[test]
fn failure_route_keywords_and_nodes_are_ok() {
    let src = base().replace(
        "  failures: []",
        "  failures:\n    - { class: tool_error, route: same_node }\n    - { class: gate_refused, route: next_iteration }\n    - { class: budget_exhausted, route: n1, terminal: true }",
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

// ---------- E2207 预算 cap / 作用域 ----------

#[test]
fn budget_scope_dangling_is_e2207() {
    let src = base().replace(
        "  budgets: []",
        "  budgets:\n    - { id: b, dimension: turns, scope: node:ghost, cap: 5 }",
    );
    assert_eq!(check(&src), ["E2207"]);
}

#[test]
fn budget_cap_zero_is_e2207() {
    let src = base().replace(
        "  budgets: []",
        "  budgets:\n    - { id: b, dimension: turns, scope: plan, cap: 0 }",
    );
    assert_eq!(check(&src), ["E2207"]);
}

#[test]
fn budget_loop_scope_requires_loop_construct() {
    // loop:n1 但 n1 无 loop 构造 → E2207。
    let src = base().replace(
        "  budgets: []",
        "  budgets:\n    - { id: b, dimension: turns, scope: loop:n1, cap: 5 }",
    );
    assert_eq!(check(&src), ["E2207"]);
}

// ---------- E2208 retention 无界 ----------

#[test]
fn store_retention_unbounded_is_e2208() {
    let src = build(
        DEF_META,
        DEF_TYPES,
        "  - { name: s, type: kv, value: { type: string }, writer: n1, lifecycle: run, retention: {} }\n",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    writes: [s]\n    emits: { answer: a }\n",
        "",
        DEF_POLICIES,
    );
    assert_eq!(check(&src), ["E2208"]);
}

// ---------- 红队修复批（W1）：E2202 判定面与内建端口表同源 ----------

#[test]
fn builtin_done_port_satisfies_output_ref() {
    // meta.output.from = loop 构造节点（无 emits）、ref = signal：内建
    // done:signal 端口在 E2202 判定面应被承认（与 port_exists/emit_type
    // 同一口径——此前边核对面存在、output 判定面「不存在」，合法文档假拒）。
    let meta = "meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: signal, from: l1 }\n";
    let src = build(
        meta,
        DEF_TYPES,
        "",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\n  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      safety_points: [ { node: g, kind: gate } ]\n",
        "",
        DEF_POLICIES,
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn builtin_passed_port_satisfies_output_ref() {
    // meta.output.from = gate 节点（无 emits）、ref 内联 boolean：内建
    // passed:boolean 端口同理应被承认。
    let meta = "meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: { type: boolean }, from: c1 }\n";
    let src = build(
        meta,
        DEF_TYPES,
        "",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\n  - id: c1\n    kind: gate\n    assert: \"true\"\n",
        "",
        DEF_POLICIES,
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn dual_same_type_emit_ports_ambiguate_output() {
    // 同节点 emits 两个同类型端口（answer + shadow 同为 a）：output.ref 无法
    // 单义指认终态端口 → E2202（恰一端口类型匹配收紧；多匹配即歧义）。
    let src = base().replace("emits: { answer: a }", "emits: { answer: a, shadow: a }");
    assert_eq!(check(&src), ["E2202"]);
}
