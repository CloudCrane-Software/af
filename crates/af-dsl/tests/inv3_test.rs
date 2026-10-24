//! INV-3 生成器-验证器分离（直接自环判定）验收测试（M3，提案 B §5/§8-4）：
//! 验证面 = kind ∈ {check, gate} ∪ human 标记节点；验证节点任一入边的生产者
//! 不得是它自己（产出即验收 = 自验证）→ E2321。间接自验证（check → transform
//! → check）v0.1 不查（提案 §8-4，留后续）。E2322（安全点 kind 白名单）不在
//! M3 范围（P2-L04 切分；实现它将打破 M1 已交付的 inv2 夹具断言，见 RESULT-M3）。
//! 正例门槛：提案 B §4 六个 golden 样例全量（M1+M2+M3）校验干净。

use af_dsl::{load_str, validate, Document, Report};

fn run(src: &str) -> Report {
    let doc: Document = load_str(src).unwrap_or_else(|e| panic!("解析失败：{e}"));
    validate(&doc)
}

fn check(src: &str) -> Vec<&'static str> {
    run(src).codes()
}

/// (code, message) 对（逐字断言用）。
fn msgs(src: &str) -> Vec<(&'static str, String)> {
    run(src)
        .violations
        .iter()
        .map(|v| (v.code.code(), v.message.clone()))
        .collect()
}

/// 最小文档骨架：meta.entry/output 指 verifier；types 含 in_t（字段 q）与 a。
fn doc(node_block: &str, edges: &str) -> String {
    format!(
        "version: 1\n\
meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: verifier\n  input: in_t\n  output: {{ ref: a, from: verifier }}\n\
types:\n  - {{ name: in_t, schema: {{ type: object, required: [q], properties: {{ q: {{ type: string }} }} }} }}\n  - {{ name: a, schema: {{ type: object }} }}\n\
nodes:\n{node_block}edges: {edges}\n"
    )
}

// ---------- golden 门槛：六样例全量（M1+M2+M3）校验干净 ----------

#[test]
fn six_golden_samples_pass_full_m3_validation() {
    for i in 1..=6 {
        let src = std::fs::read_to_string(format!("testdata/sample{i}.yaml")).unwrap();
        let doc: Document = load_str(&src).unwrap_or_else(|e| panic!("sample{i} 解析失败：{e}"));
        let r = validate(&doc);
        assert!(r.is_ok(), "sample{i} 不应违例：\n{r}");
    }
}

// ---------- 提案 B §5 的 INV-3 违规样例（机器验证过的 golden 拒绝夹具） ----------

#[test]
fn spec_inv3_fixture_triggers_exactly_e2321() {
    // 提案 B §5 INV-3 样例：check 节点 solver 消费自己产出的 patch 端口。
    // 其余全干净（无 store、无 context、uses.unit 不是 LLM-bound）。
    let src = std::fs::read_to_string("testdata/reject/inv3_e2321_self_verification.yaml").unwrap();
    assert_eq!(check(&src), ["E2321"]);
    // 错误信息逐字对齐提案 §5 预期错误风格。
    assert_eq!(
        msgs(&src),
        [(
            "E2321",
            "verifier node 'solver' consumes port 'patch' produced by itself（生成与验证必须不同节点）"
                .to_string()
        )]
    );
}

// ---------- 反例 2：gate 自环 ----------

#[test]
fn gate_self_loop_is_e2321() {
    let src = doc(
        "  - { id: verifier, kind: gate, consumes: { x: a }, emits: { answer: a } }\n",
        "[{ from: verifier, output: answer, to: verifier, input: x }]",
    );
    assert_eq!(check(&src), ["E2321"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2321",
            "verifier node 'verifier' consumes port 'x' produced by itself（生成与验证必须不同节点）"
                .to_string()
        )]
    );
}

// ---------- 反例 3：human 标记节点的验证面归属 ----------

#[test]
fn human_marked_node_self_loop_is_e2321() {
    // kind=task 但带 human 构造 → 仍属验证面（§5：check/gate ∪ human 标记节点）。
    let src = doc(
        "  - id: verifier\n    kind: task\n    human: { station: s1, role: approver, resume: { schema: { type: object } } }\n    consumes: { x: a }\n    emits: { answer: a }\n",
        "[{ from: verifier, output: answer, to: verifier, input: x }]",
    );
    assert_eq!(check(&src), ["E2321"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2321",
            "verifier node 'verifier' consumes port 'x' produced by itself（生成与验证必须不同节点）"
                .to_string()
        )]
    );
}

// ---------- 反例 4：loop 体内的 check 自环（非根作用域） ----------

#[test]
fn loop_body_check_self_loop_is_e2321() {
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
        "nodes:",
        "  - id: l1",
        "    kind: task",
        "    loop:",
        "      body:",
        "        nodes:",
        "          - { id: verifier, kind: check, consumes: { x: a }, emits: { answer: a } }",
        "        edges:",
        "          - { from: verifier, output: answer, to: verifier, input: x }",
        "      max_iterations: 2",
        "      safety_points: [ { node: verifier, kind: check } ]",
        "    emits: { answer: { from_body: verifier.answer } }",
        "edges: []",
    ]
    .join("\n")
        + "\n";
    assert_eq!(check(&src), ["E2321"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2321",
            "verifier node 'l1.verifier' consumes port 'x' produced by itself（生成与验证必须不同节点）"
                .to_string()
        )]
    );
}

// ---------- 反例 5：两条自环边各报一条（一次收齐） ----------

#[test]
fn two_self_edges_report_two_e2321() {
    let src = doc(
        "  - { id: verifier, kind: check, consumes: { x: a, y: a }, emits: { p: a, q: a } }\n",
        "[{ from: verifier, output: p, to: verifier, input: x }, { from: verifier, output: q, to: verifier, input: y }]",
    );
    assert_eq!(check(&src), ["E2321", "E2321"]);
}

// ---------- 正例：不误伤 ----------

#[test]
fn check_consuming_other_producer_is_clean() {
    // 验证节点消费他人产出（生成与验证不同节点）→ 干净。
    let src = doc(
        "  - { id: maker, kind: task, emits: { work: a } }\n  - { id: verifier, kind: check, consumes: { x: a }, emits: { answer: a } }\n",
        "[{ from: maker, output: work, to: verifier, input: x }]",
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn task_self_loop_is_not_e2321() {
    // 非验证面节点的自环不属 INV-3 判定对象（v0.1 验证面限定；根图裸环的
    // E2005 归 af-graph 域，本 crate 未实现——RESULT-M1 已登记）。
    let src = doc(
        "  - { id: verifier, kind: task, consumes: { x: a }, emits: { answer: a } }\n",
        "[{ from: verifier, output: answer, to: verifier, input: x }]",
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn indirect_self_verification_not_flagged_v01() {
    // 提案 §8-4：间接自验证（check → transform → check 回环）v0.1 不查。
    // solver 消费 fixer 的 fixed 端口，而 fixer 消费 solver 的 patch——
    // solver 实际参与生成自己验收的工件，但无直接自环 → 不报 E2321。
    let src = doc(
        "  - { id: verifier, kind: check, consumes: { fixed: a }, emits: { patch: a, answer: a } }\n  - { id: fixer, kind: transform, consumes: { raw: a }, emits: { fixed: a } }\n",
        "[{ from: verifier, output: patch, to: fixer, input: raw }, { from: fixer, output: fixed, to: verifier, input: fixed }]",
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn verifier_consuming_builtin_passed_of_other_check_is_clean() {
    // check 的内建 passed 端口可被另一 check 消费（跨节点，非自环）。
    let src = doc(
        "  - { id: maker, kind: check, emits: { patch: a } }\n  - { id: verifier, kind: gate, consumes: { ok: { type: boolean } }, emits: { answer: a } }\n",
        "[{ from: maker, output: passed, to: verifier, input: ok }]",
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}
