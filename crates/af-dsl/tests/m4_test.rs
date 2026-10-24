//! M4 全位点 CEL 静态判定接线测试（提案 B §1.4 词汇表 + §3.1 校验序）：
//! 位点面 = `policies.stop[].when`（全局词表）· `loop.exit_when`（+ attempt +
//! 作用域链节点根）· `gate/check assert`（+ inputs）· `route.rules[].when`
//! （+ 节点自身/裸 passed，见 route_test）· `meta.topology_selector.rules[].when`
//! （见 selector_test）。`failures[]` 按 §1.2 无 CEL 字段（route 是目标串，
//! E2204 已在 M1 判）——见 RESULT-M4 偏差表。
//! 正例门槛复用六个 golden 样例（route_test 已含），此处补位点级正反例。

use af_dsl::{load_str, validate, Document, Report};

fn run(src: &str) -> Report {
    let doc: Document = load_str(src).unwrap_or_else(|e| panic!("解析失败：{e}"));
    validate(&doc)
}

fn codes(src: &str) -> Vec<&'static str> {
    run(src).codes()
}

fn msgs(src: &str) -> Vec<(&'static str, String)> {
    run(src)
        .violations
        .iter()
        .map(|v| (v.code.code(), v.message.clone()))
        .collect()
}

/// 最小骨架：n0（task，emits answer:a）→ n1（entry，task，emits answer:a）。
fn doc(nodes_tail: &str, edges: &str, policies: &str) -> String {
    format!(
        "version: 1\n\
meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: {{ ref: a, from: n1 }}\n\
types:\n  - {{ name: in_t, schema: {{ type: object, required: [q], properties: {{ q: {{ type: string }} }} }} }}\n  - {{ name: a, schema: {{ type: object, required: [ok], properties: {{ ok: {{ type: boolean }} }} }} }}\n\
nodes:\n  - {{ id: n0, kind: task, emits: {{ answer: a }} }}\n  - id: n1\n    kind: task\n    emits: {{ answer: a }}\n{nodes_tail}\
edges: {edges}\n{policies}"
    )
}

const EMPTY_POLICIES: &str = "policies: { budgets: [], stop: [], failures: [] }\n";

// ---------- policies.stop[].when：全局词表 ----------

#[test]
fn stop_when_non_boolean_is_e1053() {
    let src = doc(
        "  - { id: n2, kind: task, consumes: { answer: a }, emits: { answer: a } }\n",
        "[]",
        "policies:\n  budgets: []\n  stop:\n    - { when: \"steps.turns + 1\", outcome: completed }\n  failures: []\n",
    );
    assert_eq!(codes(&src), ["E1053"]);
    assert_eq!(
        msgs(&src),
        [("E1053", "policies.stop[0].when CEL 谓词求值非布尔：结果为 int".to_string())]
    );
}

#[test]
fn stop_when_unknown_root_is_e2203() {
    let src = doc(
        "  - { id: n2, kind: task, consumes: { answer: a }, emits: { answer: a } }\n",
        "[]",
        "policies:\n  budgets: []\n  stop:\n    - { when: \"ghost.x == 1\", outcome: completed }\n  failures: []\n",
    );
    assert_eq!(codes(&src), ["E2203"]);
}

#[test]
fn stop_when_valid_global_vocabulary_passes() {
    // 样例 1 同面：last_tool / steps / input 三根全过。
    let src = doc(
        "  - { id: n2, kind: task, consumes: { answer: a }, emits: { answer: a } }\n",
        "[]",
        "policies:\n  budgets: []\n  stop:\n    - { when: \"last_tool.first_line in ['FINISH', 'FINAL ANSWER']\", outcome: completed }\n    - { when: \"steps.tool_free >= 2\", outcome: failed }\n    - { when: \"input.q == 'x'\", outcome: completed }\n  failures: []\n",
    );
    assert!(codes(&src).is_empty(), "全局词表合法表达式应放行：\n{}", run(&src));
}

#[test]
fn stop_when_node_id_root_not_in_global_vocabulary() {
    // 文档级 policies 无节点作用域链：节点 id 根不在位置词表（窄解释，见
    // RESULT-M4 偏差表）。
    let src = doc(
        "  - { id: n2, kind: task, consumes: { answer: a }, emits: { answer: a } }\n",
        "[]",
        "policies:\n  budgets: []\n  stop:\n    - { when: \"n1.answer.ok == true\", outcome: completed }\n  failures: []\n",
    );
    assert_eq!(codes(&src), ["E2203"]);
}

// ---------- loop.exit_when：attempt + 作用域链 ----------

fn loop_doc(exit_when: &str, body_route: Option<&str>) -> String {
    let router = body_route.unwrap_or("");
    format!(
        "version: 1\n\
meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: {{ ref: a, from: n1 }}\n\
types:\n  - {{ name: in_t, schema: {{ type: object, required: [q], properties: {{ q: {{ type: string }} }} }} }}\n  - {{ name: a, schema: {{ type: object, required: [ok], properties: {{ ok: {{ type: boolean }} }} }} }}\n\
nodes:\n  - id: n1\n    kind: task\n    loop:\n      body:\n        nodes:\n
          - {{ id: inner, kind: check, consumes: {{ x: a }}, emits: {{ report: a }} }}\n
          - id: sink\n            kind: task\n            consumes: {{ report: a }}\n            emits: {{ done: a }}\n{router}
        edges:\n          - {{ from: inner, output: report, to: sink, input: report }}\n      max_iterations: 3\n      safety_points:\n        - {{ node: inner, kind: check }}\n      exit_when: \"{exit_when}\"\n      on_exhausted: fail\n    emits: {{ answer: {{ from_body: sink.done }} }}\n\
edges: []\npolicies: {{ budgets: [], stop: [], failures: [] }}\n"
    )
}

#[test]
fn exit_when_attempt_and_scope_chain_roots_pass() {
    // attempt.passed（体内验证面内建）+ 体外根 n1（作用域链向外）+ 全局 steps。
    let src = loop_doc("attempt.passed == true && n1.answer.ok == true && steps.turns >= 1", None);
    assert!(codes(&src).is_empty(), "exit_when 词表根应放行：\n{}", run(&src));
}

#[test]
fn exit_when_non_boolean_is_e1053() {
    let src = loop_doc("steps.turns + 1", None);
    assert_eq!(codes(&src), ["E1053"]);
}

#[test]
fn exit_when_unknown_root_is_e2203() {
    let src = loop_doc("ghost.x == 1", None);
    assert_eq!(codes(&src), ["E2203"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2203",
            "loop 'n1' exit_when CEL 根绑定 \"ghost\" 不在位置词表内".to_string()
        )]
    );
}

#[test]
fn exit_when_body_node_root_resolves_through_scope_chain() {
    // 体内节点根（inner/sink）对 exit_when 可见（链自体根向外）。
    let src = loop_doc("inner.report.ok == true || sink.done.ok == true", None);
    assert!(codes(&src).is_empty(), "\n{}", run(&src));
}

// ---------- gate/check assert：inputs + 节点根 ----------

#[test]
fn assert_inputs_binding_from_incoming_edges() {
    // inputs.answer 由入边端口集建模（a.ok:boolean → 求值为布尔）。
    let src = doc(
        "  - { id: n2, kind: gate, consumes: { answer: a }, emits: { checked: a }, assert: \"inputs.answer.ok == true\" }\n",
        "[{ from: n0, output: answer, to: n2, input: answer }]",
        EMPTY_POLICIES,
    );
    assert!(codes(&src).is_empty(), "inputs 建模合法表达式应放行：\n{}", run(&src));
}

#[test]
fn assert_bare_inputs_value_is_not_boolean_e1053() {
    // inputs 绑定为 Map——整值引用非布尔 → E1053（证明 inputs 已绑定并受判定）。
    let src = doc(
        "  - { id: n2, kind: gate, consumes: { answer: a }, emits: { checked: a }, assert: \"inputs\" }\n",
        "[{ from: n0, output: answer, to: n2, input: answer }]",
        EMPTY_POLICIES,
    );
    assert_eq!(codes(&src), ["E1053"]);
}

#[test]
fn assert_unknown_root_is_e2203() {
    let src = doc(
        "  - { id: n2, kind: gate, consumes: { answer: a }, emits: { checked: a }, assert: \"ghost.x == 1\" }\n",
        "[{ from: n0, output: answer, to: n2, input: answer }]",
        EMPTY_POLICIES,
    );
    assert_eq!(codes(&src), ["E2203"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2203",
            "node 'n2' assert CEL 根绑定 \"ghost\" 不在位置词表内".to_string()
        )]
    );
}

// ---------- resume：从作用域向外第一个 human 构造节点 ----------

#[test]
fn resume_visible_from_human_node_route_in_same_scope() {
    // 样例 5 同面：approve（human）自身 route 引用 resume.approved。
    let src = format!(
        "version: 1\n\
meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: {{ ref: a, from: n1 }}\n\
types:\n  - {{ name: in_t, schema: {{ type: object, required: [q], properties: {{ q: {{ type: string }} }} }} }}\n  - {{ name: a, schema: {{ type: object }} }}\n\
nodes:\n  - {{ id: n0, kind: task, emits: {{ answer: a }} }}\n
  - id: approver\n    kind: task\n    consumes: {{ answer: a }}\n    emits: {{ decision: a }}\n    human:\n      station: desk\n      role: reviewer\n      resume:\n        schema: {{ type: object, required: [approved], properties: {{ approved: {{ type: boolean }} }} }}\n    route:\n      default: exit\n      rules:\n        - {{ when: \"resume.approved == false\", goto: n0 }}\n  - id: n1\n    kind: task\n    emits: {{ answer: a }}\n\
edges: [{{ from: n0, output: answer, to: approver, input: answer }}, {{ from: approver, output: decision, to: n0, input: answer }}]\n{EMPTY_POLICIES}"
    );
    assert!(codes(&src).is_empty(), "resume 词表根应放行：\n{}", run(&src));
}

// ---------- map 体 item 根 ----------

#[test]
fn item_visible_inside_map_body_assert() {
    let src = format!(
        "version: 1\n\
meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: {{ ref: list<a>, from: n1 }}\n\
types:\n  - name: in_t\n    schema:\n      type: object\n      required: [tags]\n      properties: {{ tags: {{ type: array, items: {{ type: string }} }} }}\n  - {{ name: a, schema: {{ type: object }} }}\n\
nodes:\n  - id: n1\n    kind: task\n    map:\n      over: input.tags\n      body:\n        nodes:\n          - {{ id: guard, kind: gate, emits: {{ done: a }} }}\n        edges: []\n    emits: {{ items: {{ from_body: guard.done }} }}\n\
edges: []\n{EMPTY_POLICIES}"
    );
    // guard 无 assert 字段的基线：干净。item 词表正例在 GREEN 实装后由
    // 下一条夹具验证（此处先钉基线，避免 map.over E2104 噪声混入）。
    assert!(codes(&src).is_empty(), "map 基线应干净：\n{}", run(&src));
}

#[test]
fn item_assert_inside_map_body_judged_with_item_root() {
    let src = format!(
        "version: 1\n\
meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: {{ ref: list<a>, from: n1 }}\n\
types:\n  - name: in_t\n    schema:\n      type: object\n      required: [tags]\n      properties: {{ tags: {{ type: array, items: {{ type: string }} }} }}\n  - {{ name: a, schema: {{ type: object }} }}\n\
nodes:\n  - id: n1\n    kind: task\n    map:\n      over: input.tags\n      body:\n        nodes:\n          - id: guard\n            kind: gate\n            emits: {{ done: a }}\n            assert: \"item == 'x'\"\n        edges: []\n    emits: {{ items: {{ from_body: guard.done }} }}\n\
edges: []\n{EMPTY_POLICIES}"
    );
    assert!(codes(&src).is_empty(), "item 根（string 元素）应放行：\n{}", run(&src));
}

// ---------- 判定不改变既有层行为（一次收齐、无双报） ----------

#[test]
fn cel_judgment_and_reference_checks_collect_together() {
    // CEL 违例 + 引用违例一次收齐（顺序：CEL 位点在引用闭包后、route 归一化前，
    // 见 RESULT-M4 偏差表——§3.1 草图序的实测约束化调整）。
    let src = doc(
        "  - id: n2\n    kind: task\n    emits: { answer: a }\n    route:\n      default: exit\n      rules:\n        - { when: \"size(steps.turns) > 0\", goto: ghost }\n",
        "[]",
        EMPTY_POLICIES,
    );
    assert_eq!(codes(&src), ["E1052", "E2102"]);
}
