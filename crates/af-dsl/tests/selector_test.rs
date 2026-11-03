//! topology_selector 语义校验验收测试（M4，裁决 2026-10-24 §二-1）：
//! 语义定为**编译期解析**——每条规则引用的候选图必须是已声明的版本化图/子图
//! （name@version 形 + 已声明集命中）、fallback 必填（缺 → serde E1002）、
//! rules[].when 过 CEL 保守子集静态判定（全局词汇表）。候选图集是包产物
//! （v0.1 单文档不自带声明面），经 `ValidateOptions.declared_graphs` 注入，
//! 默认空集 = 任何候选引用都判未声明（E2001，悬空引用共享码位——§6 码表
//! 无 selector 专用码位，不私占空号）。

use af_dsl::{load_str, validate_with, Document, LoadError, Report, ValidateOptions};
use std::collections::BTreeSet;

fn base_doc(selector: &str) -> String {
    format!(
        "version: 1\n\
meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: {{ ref: a, from: n1 }}\n{selector}\
types:\n  - {{ name: in_t, schema: {{ type: object, required: [q], properties: {{ q: {{ type: string }} }} }} }}\n  - {{ name: a, schema: {{ type: object }} }}\n\
nodes:\n  - {{ id: n1, kind: task, uses: {{ model: route.main@1 }}, emits: {{ answer: a }} }}\n\
edges: []\npolicies: {{ budgets: [], stop: [], failures: [] }}\n"
    )
}

fn selector_yaml(rules: &[(&str, &str)], fallback: &str) -> String {
    let mut s = String::from("  topology_selector:\n    rules:\n");
    for (when, use_graph) in rules {
        s.push_str(&format!(
            "      - {{ when: \"{when}\", use: {use_graph} }}\n"
        ));
    }
    s.push_str(&format!("    fallback: {fallback}\n"));
    s
}

fn run(src: &str, declared: &[&str]) -> Report {
    let doc: Document = load_str(src).unwrap_or_else(|e| panic!("解析失败：{e}"));
    let opts = ValidateOptions {
        declared_graphs: declared.iter().map(|g| g.to_string()).collect(),
        ..Default::default()
    };
    validate_with(&doc, &opts)
}

fn codes(src: &str, declared: &[&str]) -> Vec<&'static str> {
    run(src, declared).codes()
}

// ---------- fallback 必填（serde 层即拒，E1002） ----------

#[test]
fn selector_without_fallback_is_rejected_as_e1002() {
    let src = concat!(
        "version: 1\n",
        "meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: a, from: n1 }\n  topology_selector:\n    rules:\n      - { when: \"input.q == 'x'\", use: pkg.g@1 }\n",
        "types:\n  - { name: in_t, schema: { type: object, properties: { q: { type: string } } } }\n  - { name: a, schema: { type: object } }\n",
        "nodes:\n  - { id: n1, kind: task, uses: { model: route.main@1 }, emits: { answer: a } }\n",
        "edges: []\npolicies: { budgets: [], stop: [], failures: [] }\n"
    )
    .to_string();
    let err = load_str(&src).expect_err("缺 fallback 应被拒");
    assert!(matches!(err, LoadError::Shape(_)));
    assert_eq!(err.codes(), ["E1002"]);
}

// ---------- 候选缺失：默认空集 = 任何候选都未声明（E2001） ----------

#[test]
fn undeclared_rule_candidate_is_e2001_with_default_empty_registry() {
    let src = base_doc(&selector_yaml(
        &[("input.q == 'x'", "pkg.g1@1")],
        "pkg.g0@1",
    ));
    assert_eq!(codes(&src, &[]), ["E2001", "E2001"]);
}

#[test]
fn declared_versioned_candidates_pass_clean() {
    let src = base_doc(&selector_yaml(
        &[
            ("input.q == 'x'", "pkg.g1@1"),
            ("steps.turns >= 9", "pkg.g2@1"),
        ],
        "pkg.g0@1",
    ));
    assert!(
        codes(&src, &["pkg.g0@1", "pkg.g1@1", "pkg.g2@1"]).is_empty(),
        "已声明候选应放行：\n{}",
        run(&src, &["pkg.g0@1", "pkg.g1@1", "pkg.g2@1"])
    );
}

#[test]
fn undeclared_fallback_is_e2001_even_when_rules_declared() {
    let src = base_doc(&selector_yaml(
        &[("input.q == 'x'", "pkg.g1@1")],
        "pkg.g0@1",
    ));
    let r = run(&src, &["pkg.g1@1"]);
    let msgs: Vec<(String, String)> = r
        .violations
        .iter()
        .map(|v| (v.code.code().to_string(), v.message.clone()))
        .collect();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].0, "E2001");
    assert_eq!(
        msgs[0].1,
        "topology_selector fallback 候选图 \"pkg.g0@1\" 未在已声明的版本化图/子图集内"
    );
}

// ---------- 版本化形：name@version 缺版本 → E2001 ----------

#[test]
fn unversioned_candidate_form_is_e2001() {
    let src = base_doc(&selector_yaml(&[("input.q == 'x'", "g1")], "pkg.g0@1"));
    let r = run(&src, &["g1", "pkg.g0@1"]);
    assert_eq!(r.codes(), ["E2001"]);
    assert_eq!(
        r.violations[0].message,
        "topology_selector rules[0].use 候选图引用 \"g1\" 不是版本化 name@version 形"
    );
}

// ---------- when 的 CEL 静态判定（全局词汇表） ----------

#[test]
fn selector_when_non_subset_function_is_e1052() {
    let src = base_doc(&selector_yaml(
        &[("size(input.q) > 0", "pkg.g1@1")],
        "pkg.g0@1",
    ));
    assert_eq!(codes(&src, &["pkg.g0@1", "pkg.g1@1"]), ["E1052"]);
    assert_eq!(
        run(&src, &["pkg.g0@1", "pkg.g1@1"]).violations[0].message,
        "meta.topology_selector rules[0].when CEL 非保守子集：非算子函数/方法调用 \"size\"（函数与方法调用一律拒绝）"
    );
}

#[test]
fn selector_when_unknown_root_is_e2203() {
    let src = base_doc(&selector_yaml(&[("ghost.x == 1", "pkg.g1@1")], "pkg.g0@1"));
    assert_eq!(codes(&src, &["pkg.g0@1", "pkg.g1@1"]), ["E2203"]);
}

#[test]
fn selector_when_node_id_root_is_not_in_global_vocabulary() {
    // 选择器在 meta 层（无节点作用域链）：节点 id 根不在其位置词表 → E2203。
    let src = base_doc(&selector_yaml(
        &[("n1.answer == a", "pkg.g1@1")],
        "pkg.g0@1",
    ));
    assert_eq!(codes(&src, &["pkg.g0@1", "pkg.g1@1"]), ["E2203"]);
}

// ---------- 已声明集含未版本化名：不因此放行未版本化引用 ----------

#[test]
fn declared_set_membership_does_not_excuse_unversioned_form() {
    // 裁决文本：候选必须是「已声明的**版本化**图/子图」——形与集两面都查。
    let src = base_doc(&selector_yaml(&[("input.q == 'x'", "g1")], "g0"));
    assert_eq!(codes(&src, &["g0", "g1"]), ["E2001", "E2001"]);
}

// ---------- BTreeSet 声明集确定性（重复注入无副作用） ----------

#[test]
fn declared_set_is_a_set() {
    let src = base_doc(&selector_yaml(
        &[("input.q == 'x'", "pkg.g1@1")],
        "pkg.g0@1",
    ));
    let mut declared = BTreeSet::new();
    declared.insert("pkg.g0@1".to_string());
    declared.insert("pkg.g0@1".to_string());
    declared.insert("pkg.g1@1".to_string());
    let doc: Document = load_str(&src).unwrap();
    let r = validate_with(
        &doc,
        &ValidateOptions {
            declared_graphs: declared,
            ..Default::default()
        },
    );
    assert!(r.is_ok(), "重复注入声明不产生重复违例：\n{r}");
}
