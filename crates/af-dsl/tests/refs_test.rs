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
fn builtin_only_terminal_is_rejected_e2202() {
    // 红队 W3-C19（原 W1-C22 两例翻转）：无 emits 的构造/判定节点不得作
    // meta.output.from——内建端口（loop/map done、check/gate passed）服务边
    // 模型，但 lower 的 artifact_contract 仅由 n.emits 构造，计划面无其投影
    // 位；W1-C22 放行后形成「af-dsl 过、编译 exit 0、执行面
    // final_artifact_port 找不到 is_final 工件」的修复面新缺口。现 emits 缺席
    // 即 E2202（闭环：编译产物不存在不可执行的终态计划）。
    let meta_done = "meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: signal, from: l1 }\n";
    let src = build(
        meta_done,
        DEF_TYPES,
        "",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\n  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      safety_points: [ { node: g, kind: gate } ]\n",
        "",
        DEF_POLICIES,
    );
    assert_eq!(check(&src), ["E2202"]);

    let meta_passed = "meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: { type: boolean }, from: c1 }\n";
    let src = build(
        meta_passed,
        DEF_TYPES,
        "",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\n  - id: c1\n    kind: gate\n    assert: \"true\"\n",
        "",
        DEF_POLICIES,
    );
    assert_eq!(check(&src), ["E2202"]);

    // 对照：自带 emits 的 gate 节点作终态仍合法（sop_full tester 同款形态）。
    let src = build(
        meta_passed,
        DEF_TYPES,
        "",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\n  - id: c1\n    kind: gate\n    assert: \"true\"\n    emits: { passed: { type: boolean } }\n",
        "",
        DEF_POLICIES,
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn builtin_done_port_still_feeds_edges() {
    // 边模型不变：loop 的 done:signal 仍可合法接边（内建端口服务边核对面，
    // W1-C22 该半不受终态收紧影响）。
    let meta = "meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: a, from: n1 }\n";
    let src = build(
        meta,
        DEF_TYPES,
        "",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\n  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      safety_points: [ { node: g, kind: gate } ]\n  - id: n2\n    kind: transform\n    ops: [ { op: project, port: p, into: q } ]\n",
        "  - { from: l1, output: done, to: n2, input: done }\n",
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

// ---------- 红队修复批（W2）：内建端口显式声明不重复计数 ----------

#[test]
fn explicit_builtin_done_declaration_is_not_double_counted() {
    // 红队 W2-C23：loop 节点显式 `emits: {done: signal}` 与内建 done 同
    // 名同型——emits 循环与 builtin_ports 循环各 push 一次即假拒合法
    // 文档（违例文案自证 ["done","done"]）。端口名去重后应零违例。
    let meta = "meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: signal, from: l1 }\n";
    let src = build(
        meta,
        DEF_TYPES,
        "",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\n  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      safety_points: [ { node: g, kind: gate } ]\n    emits: { done: signal }\n",
        "",
        DEF_POLICIES,
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn explicit_builtin_passed_declaration_is_not_double_counted() {
    // 同上去重：gate 节点显式 emits passed:boolean + 内建 passed。
    let meta = "meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: { type: boolean }, from: c1 }\n";
    let src = build(
        meta,
        DEF_TYPES,
        "",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\n  - id: c1\n    kind: gate\n    assert: \"true\"\n    emits: { passed: { type: boolean } }\n",
        "",
        DEF_POLICIES,
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn two_distinct_same_type_ports_still_reject_as_e2202() {
    // 反向钉：去重不得放松「恰一端口」——两个不同名同型端口仍 E2202。
    let meta = "meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: a, from: n1 }\n";
    let src = build(
        meta,
        DEF_TYPES,
        "",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a, shadow: a }\n",
        "",
        DEF_POLICIES,
    );
    assert_eq!(check(&src), ["E2202"]);
}

// ---------- 红队 W3-C17：内建端口显式声明与内建固定类型冲突 fail-loud ----------

#[test]
fn builtin_passed_conflicting_declaration_is_rejected() {
    // emit_type/port_exists 对内建端口（check/gate passed、loop/map done）
    // 先于 emits 表判定——显式声明 passed: a（命名对象型）被静默遮蔽：
    // E2202 按声明类型比对、边核对面按内建 boolean，同一次校验给出两个
    // 互相矛盾的类型答案（af-compiler is_final 经 derived_emit_type 继承
    // 内建优先，第三口径）。冲突声明必须 fail-loud 拒绝（E1003 类型域），
    // 使存活文档的声明面/内建面恒一致。
    let meta = "meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: a, from: n1 }\n";
    let src = build(
        meta,
        DEF_TYPES,
        "",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\n  - id: c1\n    kind: gate\n    assert: \"true\"\n    emits: { passed: a }\n",
        "",
        DEF_POLICIES,
    );
    assert!(
        check(&src).contains(&"E1003"),
        "gate 显式声明 passed 与内建 boolean 冲突应 fail-loud（E1003；W3-C17）"
    );
}

#[test]
fn builtin_done_conflicting_declaration_is_rejected() {
    // 同上：loop 节点显式声明 done 为非 signal 型。
    let meta = "meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: a, from: n1 }\n";
    let src = build(
        meta,
        DEF_TYPES,
        "",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\n  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      safety_points: [ { node: g, kind: gate } ]\n    emits: { done: a }\n",
        "",
        DEF_POLICIES,
    );
    assert!(
        check(&src).contains(&"E1003"),
        "loop 显式声明 done 与内建 signal 冲突应 fail-loud（E1003；W3-C17）"
    );
}

#[test]
fn builtin_port_from_body_spec_is_rejected_as_shadowed() {
    // 内建端口名的 from_body 声明同样被 emit_type 内建短路静默忽略（声明
    // 的转发通道永不生效）——一并 fail-loud，不允许装饰性死声明。
    let meta = "meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: a, from: n1 }\n";
    let src = build(
        meta,
        DEF_TYPES,
        "",
        "  - id: n1\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\n  - id: c1\n    kind: gate\n    assert: \"true\"\n    emits: { passed: { from_body: inner.ok } }\n  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - { id: inner, kind: task, uses: { model: route.main@1 }, emits: { ok: a } }\n          - { id: c1, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      safety_points: [ { node: c1, kind: gate } ]\n",
        "",
        DEF_POLICIES,
    );
    assert!(
        check(&src).contains(&"E1003"),
        "内建端口名的 from_body 声明应 fail-loud（声明被内建短路永不生效；W3-C17）"
    );
}

// ---------- 红队 W3-C20：map.from_body 转发内联原始型不可表达 ----------

#[test]
fn map_frombody_inline_primitive_is_rejected() {
    // emit_type 对 map 构造的 from_body 只对 Named 内层包 list<>，内联原始
    // 型恒 None ⇒ E2202 判 undecidable 放行 + lower is_final=false ⇒ 计划
    // 零终态工件 ⇒ 执行面 final_artifact_port 必拒（编译过执行拒）。类型
    // 表达式文法无 list<原始型>（泛型载荷须已登记名或 signal），该形状不
    // 可表达——af-dsl 阶段显式拒绝（E2213，红队 W3-C20），禁止编译出
    // 无 is_final 工件的计划。
    // over: input.items 需要 in_t 有 items 数组字段（DEF_TYPES 无），专用类型表。
    let meta = "meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: { type: boolean }, from: n1 }\n";
    let types = "types:\n  - { name: in_t, schema: { type: object, required: [items], properties: { items: { type: array, items: { type: string } } } } }\n  - { name: a, schema: { type: object } }\n";
    let src = build(meta, types, "", "  - id: n1\n    kind: task\n    map:\n      over: input.items\n      body:\n        nodes:\n          - { id: chk, kind: check, assert: \"true\" }\n        edges: []\n    emits: { final: { from_body: chk.passed } }\n", "", DEF_POLICIES);
    let codes = check(&src);
    assert!(
        codes.contains(&"E2213"),
        "map.from_body 转发内建 passed:boolean（内联原始型）应显式拒绝（E2213；W3-C20），实得 {codes:?}"
    );
}

#[test]
fn loop_frombody_builtin_passed_still_passes() {
    // 反向钉（W3-C20 不误伤）：loop 构造不包 list<>，from_body 转发内建
    // passed:boolean 是合法同型转发（W3-C19 的终态路径），零违例。
    let meta = "meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: { type: boolean }, from: n1 }\n";
    let src = build(
        meta,
        DEF_TYPES,
        "",
        "  - id: n1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - { id: chk, kind: check, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      safety_points: [ { node: chk, kind: check } ]\n    emits: { final: { from_body: chk.passed } }\n",
        "",
        DEF_POLICIES,
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}
