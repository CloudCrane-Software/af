//! 校验器判定断言：3 个合法样例应通过；非法样例各踩一条规则且不踩别的。

use af_graph::{
    validate, CarriedState, Edge, GraphTemplate, Node, Rule, SafetyPoint, SafetyPointKind,
};
use std::path::Path;

fn load(name: &str) -> GraphTemplate {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(format!("{name}.json"));
    let src = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取 {p:?}: {e}"));
    serde_json::from_str(&src).unwrap_or_else(|e| panic!("解析 {name}.json: {e}"))
}

const LEGAL: [&str; 3] = ["legal-1-pipeline-human", "legal-2-loop", "legal-3-full"];

// ---------- 合法样例：应通过 ----------

#[test]
fn legal_templates_pass() {
    for name in LEGAL {
        let r = validate(&load(name));
        assert!(r.is_ok(), "{name} 应通过校验，实际违例:\n{r}");
    }
}

// ---------- 非法样例：各踩一条规则 ----------

#[test]
fn illegal_1_trips_only_cycle() {
    let r = validate(&load("illegal-1-cycle"));
    assert!(r.only_rule(Rule::Cycle), "应只踩规则①cycle，实际:\n{r}");
    let msg = &r.of_rule(Rule::Cycle)[0].message;
    assert!(
        msg.contains("draft -> review_gate -> revise -> qc -> draft"),
        "环路径应完整报出，实际: {msg}"
    );
}

#[test]
fn illegal_2_trips_only_reachability() {
    let r = validate(&load("illegal-2-orphan"));
    assert!(
        r.only_rule(Rule::Reachability),
        "应只踩规则②reachability，实际:\n{r}"
    );
    assert!(
        r.violations[0].message.contains("legacy_export"),
        "孤立节点名应点名，实际: {}",
        r.violations[0].message
    );
}

#[test]
fn illegal_3_trips_only_undeclared_reference() {
    let r = validate(&load("illegal-3-undeclared-ref"));
    assert!(
        r.only_rule(Rule::UndeclaredReference),
        "应只踩规则③undeclared-reference，实际:\n{r}"
    );
    assert!(
        r.violations[0].message.contains("notify_slack"),
        "未声明节点名应点名，实际: {}",
        r.violations[0].message
    );
}

#[test]
fn illegal_4_trips_only_loop_not_controlled() {
    let r = validate(&load("illegal-4-loop-no-safety"));
    assert!(
        r.only_rule(Rule::LoopNotControlled),
        "应只踩规则④loop-not-controlled（缺安全点），实际:\n{r}"
    );
    assert!(
        r.violations[0].message.contains("未声明安全点"),
        "应点名缺安全点，实际: {}",
        r.violations[0].message
    );
}

#[test]
fn illegal_5_trips_only_map_unversioned() {
    let r = validate(&load("illegal-5-map-unversioned"));
    assert!(
        r.only_rule(Rule::MapSubgraphUnversioned),
        "应只踩规则⑤map-subgraph-unversioned，实际:\n{r}"
    );
}

// ---------- 受控循环语义边界（规则④的其余侧面） ----------

#[test]
fn loop_body_cycle_exempt_but_map_body_cycle_rejected() {
    // 合法样例 2 的循环体内部有环（review→fix→review），豁免 DAG 检查——合法样例测试已覆盖。
    assert!(validate(&load("legal-2-loop")).is_ok());

    // 但 map 体（每次只跑一遍）内出现环 = 无界循环，必须拒绝（规则①）。
    let mut t = load("legal-3-full");
    t.subgraphs
        .get_mut("map_body")
        .unwrap()
        .edges
        .push(Edge {
            from: "self_check".into(),
            output: "redo".into(),
            to: "summarize".into(),
            input: "candidate".into(),
        });
    let r = validate(&t);
    assert!(r.only_rule(Rule::Cycle), "map 体内的环应触发规则①，实际:\n{r}");
    assert_eq!(r.violations[0].scope, "subgraph:map_body");
}

#[test]
fn loop_unbounded_carried_state_rejected() {
    let mut t = load("legal-2-loop");
    t.nodes[1].loop_spec.as_mut().unwrap().carried_state = CarriedState {
        name: "manuscript".into(),
        max_items: None,
        max_bytes: None,
    };
    let r = validate(&t);
    assert!(
        r.only_rule(Rule::LoopNotControlled),
        "携带状态无界应触发规则④，实际:\n{r}"
    );
}

#[test]
fn loop_zero_iterations_rejected() {
    let mut t = load("legal-2-loop");
    t.nodes[1].loop_spec.as_mut().unwrap().max_iterations = 0;
    let r = validate(&t);
    assert!(r.only_rule(Rule::LoopNotControlled), "max_iterations=0 应触发规则④，实际:\n{r}");
}

#[test]
fn safety_point_outside_body_is_undeclared_reference() {
    let mut t = load("legal-2-loop");
    t.nodes[1].loop_spec.as_mut().unwrap().safety_points = vec![SafetyPoint {
        node: "publish".into(), // 在根图，不在循环体子图里
        kind: SafetyPointKind::Human,
    }];
    let r = validate(&t);
    assert!(
        r.only_rule(Rule::UndeclaredReference),
        "安全点指向体外节点应触发规则③，实际:\n{r}"
    );
}

#[test]
fn safety_point_kind_mismatch_rejected() {
    let mut t = load("legal-2-loop");
    // human_review 是人工 gate 节点，却声明为 check 安全点。
    t.nodes[1].loop_spec.as_mut().unwrap().safety_points = vec![SafetyPoint {
        node: "human_review".into(),
        kind: SafetyPointKind::Check,
    }];
    let r = validate(&t);
    assert!(
        r.only_rule(Rule::LoopNotControlled),
        "安全点类型不符应触发规则④，实际:\n{r}"
    );
}

// ---------- schema 层（serde）与开放性 ----------

#[test]
fn serde_roundtrip_preserves_validity() {
    let t = load("legal-3-full");
    let s = serde_json::to_string(&t).expect("序列化");
    let t2: GraphTemplate = serde_json::from_str(&s).expect("反序列化");
    assert!(validate(&t2).is_ok(), "round-trip 后仍应通过校验");
}

#[test]
fn unknown_node_kind_is_open_extension() {
    // kind 开放为 String：未注册的新类型不拒绝（将来注册扩展的前提）。
    let t = GraphTemplate {
        graph_version: "1".into(),
        name: "ext-kind".into(),
        entry: "a".into(),
        nodes: vec![
            Node { id: "a".into(), kind: "rerank_v2".into(), human: None, loop_spec: None, map_spec: None },
            Node { id: "b".into(), kind: "rerank_v3".into(), human: None, loop_spec: None, map_spec: None },
        ],
        edges: vec![Edge { from: "a".into(), output: "o".into(), to: "b".into(), input: "i".into() }],
        subgraphs: Default::default(),
    };
    assert!(validate(&t).is_ok(), "未知 kind 不应被拒绝");
}

#[test]
fn undeclared_entry_is_reference_error() {
    let mut t = load("legal-1-pipeline-human");
    t.entry = "ghost".into();
    let r = validate(&t);
    assert!(
        r.only_rule(Rule::UndeclaredReference),
        "入口未声明应触发规则③，实际:\n{r}"
    );
}

#[test]
fn missing_required_field_rejected_by_serde_layer() {
    // 两层校验分工：schema 结构由 serde 管（缺 graph_version 直接解析失败），
    // 语义规则由校验器管。
    let bad = r#"{"name":"x","entry":"a","nodes":[],"edges":[]}"#;
    let parsed: Result<GraphTemplate, _> = serde_json::from_str(bad);
    assert!(parsed.is_err(), "缺 graph_version 应在 serde 层被拒");
}

#[test]
fn loop_body_must_be_declared_subgraph() {
    let mut t = load("legal-2-loop");
    t.nodes[1].loop_spec.as_mut().unwrap().body = "no_such_subgraph".into();
    let r = validate(&t);
    // 注意：这里断言"规则③必须报"而非 only_rule——body 被改指不存在的子图后，
    // revise_loop 不再被认定为循环体，其内部环会同时触发规则①（两个错误都真实）。
    // "只踩一条规则"的强断言只用于精选的非法 fixtures。
    let refs = r.of_rule(Rule::UndeclaredReference);
    assert!(
        !refs.is_empty() && refs.iter().any(|v| v.message.contains("循环体子图未声明")),
        "循环体子图未声明应触发规则③，实际:\n{r}"
    );
}
