//! route 归一化验收测试（M4，提案 B §1.5/§1.3/§6）：
//! - 归一化纯函数 `af_dsl::route::normalize` 的确定性重写产物（guard 公式
//!   逐字断言）+ 互斥性/覆盖性**性质测试**（§1.5：guard 两两不相交且并上
//!   default 为真——由构造成立，在合成赋值网格上逐格验证）；
//! - 构造校验：E2101 出边未全覆盖 · E2102 goto 悬空（规则/default 两形态）·
//!   E2103 同目标重复命中（规则-规则 + 规则-default）；
//! - default 必填（缺 → serde E1002，形状层即拒——§1.3「不进语义 pass」）；
//! - route 规则 when 的 CEL 静态判定接线（E1052/E2203/E1053）；
//! - 正例门槛：六个 golden 样例全量（M1-M4）校验干净。

use af_dsl::model::{RouteDecl, RouteRule};
use af_dsl::route::{normalize, GotoRef, RouteIssue, RouteNorm};
use af_dsl::{load_str, validate, Document, Report};
use std::collections::BTreeSet;

// ---------- 纯函数测试工具 ----------

fn route(rules: &[(&str, &str)], default: &str) -> RouteDecl {
    RouteDecl {
        default: default.to_string(),
        rules: rules
            .iter()
            .map(|(when, goto)| RouteRule {
                when: (*when).to_string(),
                goto: (*goto).to_string(),
            })
            .collect(),
    }
}

fn targets(ts: &[&str]) -> BTreeSet<String> {
    ts.iter().map(|t| t.to_string()).collect()
}

fn guards_of(norm: &RouteNorm) -> Vec<(String, String)> {
    norm.edges
        .iter()
        .map(|e| (e.to.clone(), e.guard.clone()))
        .collect()
}

// ---------- 归一化产物：guard 公式逐字（确定性重写） ----------

#[test]
fn single_rule_guards_are_when_and_negated_when() {
    let norm = normalize(&route(&[("a == 1", "x")], "y"), &targets(&["x", "y"]))
        .expect("合法 route 应归一化成功");
    assert_eq!(
        guards_of(&norm),
        vec![
            ("x".to_string(), "(a == 1)".to_string()),
            ("y".to_string(), "!((a == 1))".to_string()),
        ]
    );
    assert_eq!(norm.exit_guard, None);
}

#[test]
fn two_rules_priority_prefixes_later_guard() {
    let norm = normalize(
        &route(&[("a == 1", "x"), ("b == 2", "y")], "z"),
        &targets(&["x", "y", "z"]),
    )
    .expect("合法 route 应归一化成功");
    assert_eq!(
        guards_of(&norm),
        vec![
            ("x".to_string(), "(a == 1)".to_string()),
            ("y".to_string(), "(b == 2) && !((a == 1))".to_string()),
            ("z".to_string(), "!((a == 1) || (b == 2))".to_string()),
        ]
    );
}

#[test]
fn no_rules_default_guard_is_literal_true() {
    let norm = normalize(&route(&[], "x"), &targets(&["x"])).expect("应成功");
    assert_eq!(
        guards_of(&norm),
        vec![("x".to_string(), "true".to_string())]
    );
}

#[test]
fn exit_branches_produce_no_edge_and_union_guard() {
    // 规则 1 → exit（不产生边）；规则 2 → y；default → x。
    let norm = normalize(
        &route(&[("a == 1", "exit"), ("b == 2", "y")], "x"),
        &targets(&["x", "y"]),
    )
    .expect("合法 route 应归一化成功");
    assert_eq!(
        guards_of(&norm),
        vec![
            // 按目标名确定序（BTree）。
            ("x".to_string(), "!((a == 1) || (b == 2))".to_string()),
            ("y".to_string(), "(b == 2) && !((a == 1))".to_string()),
        ]
    );
    assert_eq!(norm.exit_guard, Some("(a == 1)".to_string()));
}

#[test]
fn multiple_exit_rules_are_benign_and_union() {
    // 两条 exit 规则：无边丢失（exit 分支不产生边，同目标重复的「不可达」
    // 缺陷只发生在出边目标上），联合守卫按文档序析取。
    let norm = normalize(
        &route(&[("a == 1", "exit"), ("b == 2", "exit")], "x"),
        &targets(&["x"]),
    )
    .expect("多条 exit 规则应放行");
    assert_eq!(
        guards_of(&norm),
        vec![("x".to_string(), "!((a == 1) || (b == 2))".to_string())]
    );
    assert_eq!(
        norm.exit_guard,
        Some("(a == 1) || (b == 2) && !((a == 1))".to_string())
    );
}

// ---------- 构造违例（一次收齐） ----------

#[test]
fn uncovered_out_edge_is_e2101() {
    // 规则 → y，default → x；z 两条路都不沾 → E2101（无重复目标混入）。
    let issues = normalize(&route(&[("a == 1", "y")], "x"), &targets(&["x", "y", "z"]))
        .expect_err("z 未覆盖应报违例");
    assert_eq!(
        issues,
        vec![RouteIssue::UncoveredTarget {
            target: "z".to_string()
        }]
    );
}

#[test]
fn unknown_goto_is_e2102_for_rule_and_default() {
    let issues =
        normalize(&route(&[("a == 1", "ghost")], "x"), &targets(&["x"])).expect_err("ghost 悬空");
    assert_eq!(
        issues,
        vec![RouteIssue::UnknownGoto {
            target: "ghost".to_string(),
            what: GotoRef::Rule(0),
        }]
    );
    let issues =
        normalize(&route(&[("a == 1", "x")], "ghost"), &targets(&["x"])).expect_err("default 悬空");
    assert_eq!(
        issues,
        vec![RouteIssue::UnknownGoto {
            target: "ghost".to_string(),
            what: GotoRef::Default,
        }]
    );
}

#[test]
fn duplicate_target_is_e2103_for_rule_rule_and_rule_default() {
    let issues = normalize(
        &route(&[("a == 1", "x"), ("b == 2", "x")], "y"),
        &targets(&["x", "y"]),
    )
    .expect_err("x 被两条规则命中");
    assert_eq!(
        issues,
        vec![RouteIssue::DuplicateTarget {
            target: "x".to_string(),
            first: GotoRef::Rule(0),
            later: GotoRef::Rule(1),
        }]
    );
    // 规则与 default 同目标：first-hit 语义下 default 的路由贡献丢失——
    // 同一「后中者不可达」缺陷（§1.5 文本「多条规则」，规则-default 命中是
    // 其对称面，见 RESULT-M4 偏差表）。
    let issues = normalize(&route(&[("a == 1", "x")], "x"), &targets(&["x"]))
        .expect_err("x 被规则与 default 命中");
    assert_eq!(
        issues,
        vec![RouteIssue::DuplicateTarget {
            target: "x".to_string(),
            first: GotoRef::Rule(0),
            later: GotoRef::Default,
        }]
    );
}

// ---------- 性质测试：互斥 ∧ 覆盖（§1.5 免费正确性） ----------

/// 在 a×b 的布尔赋值网格上逐格验证：全部分支 guard（规则 + default + exit）
/// 两两不相交，且析取为永真（覆盖性——default 取全否）。
#[test]
fn branch_guards_are_mutually_exclusive_and_collectively_exhaustive() {
    use cel_interpreter::{Context, Program, Value};

    fn eval_true(expr: &str, a: bool, b: bool) -> bool {
        let prog = Program::compile(expr).expect("guard 应可编译");
        let mut ctx = Context::default();
        ctx.add_variable_from_value("a", a);
        ctx.add_variable_from_value("b", b);
        matches!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| prog.execute(&ctx))),
            Ok(Ok(Value::Bool(true)))
        )
    }

    // 两条规则 + default；规则 1 → x，规则 2 → exit，default → y。
    let norm = normalize(
        &route(&[("a", "x"), ("b", "exit")], "y"),
        &targets(&["x", "y"]),
    )
    .expect("合法 route");
    let mut branch_guards: Vec<String> = norm.edges.iter().map(|e| e.guard.clone()).collect();
    branch_guards.extend(norm.exit_guard.iter().cloned());
    assert_eq!(branch_guards.len(), 3, "规则×2 + default 恰 3 分支");

    for a in [false, true] {
        for b in [false, true] {
            let hits: Vec<bool> = branch_guards.iter().map(|g| eval_true(g, a, b)).collect();
            assert!(
                hits.iter().filter(|h| **h).count() <= 1,
                "赋值 a={a},b={b} 命中多分支：{hits:?}（互斥性破坏）"
            );
            assert!(
                hits.iter().any(|h| *h),
                "赋值 a={a},b={b} 无分支命中（覆盖性破坏）"
            );
        }
    }
}

// ---------- 文档级接线 ----------

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

/// 最小路由文档骨架：router（task，emits answer）→ n2/n3；types 含 in_t/a。
fn doc(node_extra: &str, edges: &str) -> String {
    format!(
        "version: 1\n\
meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: router\n  input: in_t\n  output: {{ ref: a, from: router }}\n\
types:\n  - {{ name: in_t, schema: {{ type: object, required: [q], properties: {{ q: {{ type: string }} }} }} }}\n  - {{ name: a, schema: {{ type: object }} }}\n\
nodes:\n  - id: router\n    kind: task\n    emits: {{ answer: a }}\n{node_extra}
  - {{ id: n2, kind: task, consumes: {{ answer: a }}, emits: {{ answer: a }} }}\n
  - {{ id: n3, kind: task, consumes: {{ answer: a }}, emits: {{ answer: a }} }}\n\
edges: {edges}\n"
    )
}

fn route_yaml(default: &str, rules: &[(&str, &str)]) -> String {
    if rules.is_empty() {
        return format!("    route:\n      default: {default}\n      rules: []\n");
    }
    let mut s = format!("    route:\n      default: {default}\n      rules:\n");
    for (when, goto) in rules {
        s.push_str(&format!("        - {{ when: \"{when}\", goto: {goto} }}\n"));
    }
    s
}

const BOTH_EDGES: &str = "[{ from: router, output: answer, to: n2, input: answer }, { from: router, output: answer, to: n3, input: answer }]";

// ---------- golden 门槛 ----------

#[test]
fn six_golden_samples_pass_full_m4_validation() {
    for i in 1..=6 {
        let src = std::fs::read_to_string(format!("testdata/sample{i}.yaml")).unwrap();
        let doc: Document = load_str(&src).unwrap_or_else(|e| panic!("sample{i} 解析失败：{e}"));
        let r = validate(&doc);
        assert!(r.is_ok(), "sample{i} 不应违例：\n{r}");
    }
}

// ---------- default 必填（§1.3：serde 层即拒，E1002） ----------

#[test]
fn route_without_default_is_rejected_as_e1002() {
    let src = concat!(
        "version: 1\n",
        "meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: router\n  input: in_t\n  output: { ref: a, from: router }\n",
        "types:\n  - { name: in_t, schema: { type: object, properties: { q: { type: string } } } }\n  - { name: a, schema: { type: object } }\n",
        "nodes:\n  - id: router\n    kind: task\n    emits: { answer: a }\n    route:\n      rules:\n        - { when: \"steps.tool_free >= 2\", goto: n2 }\n",
        "  - { id: n2, kind: task, consumes: { answer: a }, emits: { answer: a } }\n",
        "edges: [{ from: router, output: answer, to: n2, input: answer }]\n"
    )
    .to_string();
    let err = load_str(&src).expect_err("route 缺 default 应被拒");
    assert_eq!(err.codes(), ["E1002"]);
}

// ---------- E2102：goto 悬空（文档级） ----------

#[test]
fn route_goto_unknown_target_is_exactly_e2102() {
    let src = doc(
        &route_yaml("n3", &[("steps.tool_free >= 2", "ghost")]),
        BOTH_EDGES,
    );
    assert_eq!(codes(&src), ["E2102"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2102",
            "node 'router' route rules[0] goto \"ghost\" 不是其出边目标也不是 exit".to_string()
        )]
    );
}

// ---------- E2101：出边未全覆盖（文档级） ----------

#[test]
fn route_uncovered_out_edge_is_exactly_e2101() {
    let src = doc(
        &route_yaml("exit", &[("steps.tool_free >= 2", "n2")]),
        BOTH_EDGES,
    );
    assert_eq!(codes(&src), ["E2101"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2101",
            "node 'router' route 出边目标 \"n3\" 未被任何规则或 default 覆盖".to_string()
        )]
    );
}

// ---------- E2103：同目标重复命中（文档级） ----------

#[test]
fn route_duplicate_target_is_exactly_e2103() {
    let src = doc(
        &route_yaml(
            "n3",
            &[("steps.tool_free >= 2", "n2"), ("steps.turns >= 5", "n2")],
        ),
        BOTH_EDGES,
    );
    assert_eq!(codes(&src), ["E2103"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2103",
            "node 'router' route 目标 \"n2\" 被 rules[0] 与 rules[1] 重复命中（后中者不可达，请合并为 A || B）"
                .to_string()
        )]
    );
}

// ---------- route when 的 CEL 静态判定接线（文档级） ----------

#[test]
fn route_when_non_subset_function_is_e1052() {
    let src = doc(
        &route_yaml("n3", &[("size(steps.turns) > 0", "n2")]),
        BOTH_EDGES,
    );
    assert_eq!(codes(&src), ["E1052"]);
    assert_eq!(
        msgs(&src),
        [(
            "E1052",
            "node 'router' route rules[0].when CEL 非保守子集：非算子函数/方法调用 \"size\"（函数与方法调用一律拒绝）"
                .to_string()
        )]
    );
}

#[test]
fn route_when_unknown_root_is_e2203() {
    let src = doc(&route_yaml("n3", &[("ghost.x == 1", "n2")]), BOTH_EDGES);
    assert_eq!(codes(&src), ["E2203"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2203",
            "node 'router' route rules[0].when CEL 根绑定 \"ghost\" 不在位置词表内".to_string()
        )]
    );
}

#[test]
fn route_when_non_boolean_is_e1053() {
    let src = doc(&route_yaml("n3", &[("steps.turns + 1", "n2")]), BOTH_EDGES);
    assert_eq!(codes(&src), ["E1053"]);
}

// ---------- 裸 passed：验证面节点 route 可写（§1.4 词汇表） ----------

#[test]
fn bare_passed_is_legal_on_verification_node_route() {
    // gate 节点 router：route 规则可写裸 passed（§1.4：亦可写 router.passed）。
    let src = doc(
        "    route:\n      default: exit\n      rules:\n        - { when: \"passed == false\", goto: n2 }\n",
        "[{ from: router, output: answer, to: n2, input: answer }]",
    )
    .replacen("kind: task", "kind: gate", 1);
    assert!(
        codes(&src).is_empty(),
        "裸 passed 在验证面节点 route 应放行：\n{}",
        run(&src)
    );
}
