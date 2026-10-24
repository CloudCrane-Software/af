//! 内联体降维（提案 §3.2）：loop/map 内联体 → 命名子图（名 = `<图名>/<构造
//! 节点点号路径>`），嵌套构造递归降维；`entry` = 体 nodes 首元素；
//! `graph_version` 一律 stamp 父图版本（E2007 在 DSL 面不可触发）；节点/边
//! 总量守恒（降维不改拓扑，只改命名空间）。

mod common;

use af_compiler::ir::*;

/// DSL 侧节点/边总量（递归穿 loop/map 体）。
fn count_dsl(
    nodes: &[af_dsl::model::NodeDecl],
    edges: &[af_dsl::model::EdgeDecl],
) -> (usize, usize) {
    let mut n = nodes.len();
    let mut e = edges.len();
    for node in nodes {
        if let Some(lp) = &node.loop_spec {
            let (cn, ce) = count_dsl(&lp.body.nodes, &lp.body.edges);
            n += cn;
            e += ce;
        }
        if let Some(mp) = &node.map {
            let (cn, ce) = count_dsl(&mp.body.nodes, &mp.body.edges);
            n += cn;
            e += ce;
        }
    }
    (n, e)
}

/// IR 侧节点/边总量（根图 + 全部子图）。
fn count_ir(p: &IrProgram) -> (usize, usize) {
    let mut n = p.body.graph.nodes.len();
    let mut e = p.body.graph.edges.len();
    for sg in p.body.graph.subgraphs.values() {
        n += sg.nodes.len();
        e += sg.edges.len();
    }
    (n, e)
}

/// 降维等价：六样例节点/边总量守恒 + 子图版本 stamp + 全部子图 entry 命中体内。
#[test]
fn inline_body_dimensional_reduction_conserves_nodes_and_edges() {
    for name in common::SAMPLES {
        let (doc, _src) = common::sample(name);
        let p = af_compiler::lower(&doc).expect("{name}: lowering 失败");
        let (dn, de) = count_dsl(&doc.nodes, &doc.edges);
        let (in_, ie) = count_ir(&p);
        assert_eq!(dn, in_, "{name}: 节点总量不守恒（DSL {dn} vs IR {in_}）");
        assert_eq!(de, ie, "{name}: 边总量不守恒（DSL {de} vs IR {ie}）");
        // 版本 stamp：全部子图 graph_version == 父图版本（非 Option——E2007 面消除）
        for (sg_name, sg) in &p.body.graph.subgraphs {
            assert_eq!(
                sg.graph_version, doc.meta.graph_version,
                "{name}: 子图 {sg_name} 未 stamp 父图版本"
            );
        }
    }
}

/// 样例 3：loop 体 + 嵌套 map 体；命名 = `<图名>/<构造节点路径>`；
/// entry = 体 nodes 首元素；loop_spec.body / map_spec.body 反指子图名。
#[test]
fn sample3_subgraph_naming_and_loop_map_specs() {
    let (doc, _src) = common::sample("sample3.yaml");
    let p = af_compiler::lower(&doc).expect("lowering 失败");
    let g = &p.body.graph;

    let mut names: Vec<&str> = g.subgraphs.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(names, ["debate_kv/rounds", "debate_kv/rounds/debate_map"]);

    // 根节点 rounds：loop_spec 反指 + 扩展四字段（args/exit_when/on_exhausted/carried）
    let rounds = &g.nodes.iter().find(|n| n.id == "rounds").unwrap();
    let lp = rounds.loop_spec.as_ref().unwrap();
    assert_eq!(lp.body, "debate_kv/rounds");
    assert_eq!(lp.max_iterations, 3);
    assert_eq!(
        lp.carried,
        vec![CarriedState {
            store: "transcript".into(),
            max_items: Some(64),
            max_bytes: None
        }]
    );
    assert_eq!(
        lp.safety_points,
        vec![SafetyPoint {
            node: "converge_gate".into(),
            kind: SafetyPointKind::Gate
        }]
    );
    assert!(lp.args.is_empty());
    assert_eq!(
        lp.exit_when.as_ref().unwrap().source,
        "tally.result.top_share_milli >= 700"
    );
    assert_eq!(lp.on_exhausted, Some(OnExhausted::Continue));

    // 子图 debate_kv/rounds：entry = 首元素 debate_map；nodes 按 id 排序；3 边
    let sg = &g.subgraphs["debate_kv/rounds"];
    assert_eq!(sg.entry, "debate_map");
    let ids: Vec<&str> = sg.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["collector", "converge_gate", "debate_map", "tally"]);
    assert_eq!(sg.edges.len(), 3);
    assert_eq!(sg.graph_version, doc.meta.graph_version);

    // 体内 map 构造：map_spec 反指嵌套子图
    let debate_map = &sg.nodes.iter().find(|n| n.id == "debate_map").unwrap();
    let mp = debate_map.map_spec.as_ref().unwrap();
    assert_eq!(mp.body, "debate_kv/rounds/debate_map");
    assert_eq!(mp.over, "input.personas");
    assert_eq!(mp.max_fanout, None);

    let inner = &g.subgraphs["debate_kv/rounds/debate_map"];
    assert_eq!(inner.entry, "debater");
    assert_eq!(inner.nodes.len(), 1);
    assert!(inner.edges.is_empty());
}

/// 样例 6：双层嵌套 loop（外课程环 + 内习得环）；跨层安全点路径；
/// loop args 形参绑定；体无边（edges 缺省）。
#[test]
fn sample6_nested_loops_and_cross_layer_safety_points() {
    let (doc, _src) = common::sample("sample6.yaml");
    let p = af_compiler::lower(&doc).expect("lowering 失败");
    let g = &p.body.graph;

    let mut names: Vec<&str> = g.subgraphs.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "voyager_curriculum/explore",
            "voyager_curriculum/explore/acquire"
        ]
    );

    // 外层 explore：carried 按 store 名排序（curriculum < skill_library）；
    // 跨层安全点 "acquire.admit"（从体根穿嵌套体解析）
    let explore = &g.nodes.iter().find(|n| n.id == "explore").unwrap();
    let lp = explore.loop_spec.as_ref().unwrap();
    assert_eq!(lp.body, "voyager_curriculum/explore");
    assert_eq!(lp.max_iterations, 200);
    assert_eq!(lp.carried.len(), 2);
    assert_eq!(lp.carried[0].store, "curriculum");
    assert_eq!(lp.carried[1].store, "skill_library");
    assert_eq!(
        lp.safety_points,
        vec![SafetyPoint {
            node: "acquire.admit".into(),
            kind: SafetyPointKind::Gate
        }]
    );
    assert_eq!(
        lp.exit_when.as_ref().unwrap().source,
        "budget.cents_spent >= budget.cents_cap * 9 / 10"
    );
    assert_eq!(lp.on_exhausted, Some(OnExhausted::Continue));

    let outer = &g.subgraphs["voyager_curriculum/explore"];
    assert_eq!(outer.entry, "proposer");
    let ids: Vec<&str> = outer.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["acquire", "proposer"]);
    assert!(outer.edges.is_empty());

    // 内层 acquire：args 形参绑定（隐式依赖边声明）
    let acquire = &outer.nodes.iter().find(|n| n.id == "acquire").unwrap();
    let ilp = acquire.loop_spec.as_ref().unwrap();
    assert_eq!(ilp.body, "voyager_curriculum/explore/acquire");
    assert_eq!(ilp.max_iterations, 6);
    assert_eq!(
        ilp.args.get("goal_text").map(String::as_str),
        Some("proposer.goal.text")
    );
    assert_eq!(
        ilp.carried,
        vec![CarriedState {
            store: "skill_library".into(),
            max_items: Some(512),
            max_bytes: None
        }]
    );
    assert_eq!(
        ilp.exit_when.as_ref().unwrap().source,
        "attempt.passed == true"
    );
    assert_eq!(ilp.on_exhausted, Some(OnExhausted::Continue));

    let inner = &g.subgraphs["voyager_curriculum/explore/acquire"];
    assert_eq!(inner.entry, "generator");
    let ids: Vec<&str> = inner.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["admit", "code_exec", "curator", "generator"]);
    assert_eq!(inner.edges.len(), 5);

    // L4 逃逸：拓扑位 + 记账视图同数据
    let code_exec = &inner.nodes.iter().find(|n| n.id == "code_exec").unwrap();
    let esc = code_exec.escape.as_ref().unwrap();
    assert_eq!(
        esc.impl_ref,
        "packages/openworld@1.0.0/hatches/code_exec.py"
    );
    assert_eq!(esc.owner, "lang-team");
    assert_eq!(esc.share_budget_pct, 25);
    assert_eq!(p.body.escapes["explore.acquire.code_exec"], *esc);
}

/// 样例 4：loop 内嵌 map（synth→run）；样例 2：单层 loop 体。
#[test]
fn sample4_sample2_subgraph_shapes() {
    let (doc4, _) = common::sample("sample4.yaml");
    let p4 = af_compiler::lower(&doc4).expect("lowering 失败");
    let g4 = &p4.body.graph;
    let mut names: Vec<&str> = g4.subgraphs.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(names, ["aflow_search/search", "aflow_search/search/run"]);
    let search = &g4.subgraphs["aflow_search/search"];
    assert_eq!(search.entry, "synth");
    let run = &search.nodes.iter().find(|n| n.id == "run").unwrap();
    assert_eq!(
        run.map_spec.as_ref().unwrap().body,
        "aflow_search/search/run"
    );
    assert_eq!(run.map_spec.as_ref().unwrap().over, "synth.candidates");
    let run_sg = &g4.subgraphs["aflow_search/search/run"];
    assert_eq!(run_sg.entry, "gen_a");
    let ids: Vec<&str> = run_sg.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["aggregate", "gen_a", "gen_b"]);
    assert_eq!(run_sg.edges.len(), 2);

    let (doc2, _) = common::sample("sample2.yaml");
    let p2 = af_compiler::lower(&doc2).expect("lowering 失败");
    let g2 = &p2.body.graph;
    assert_eq!(g2.subgraphs.len(), 1);
    let sg = &g2.subgraphs["metagpt_sop/build"];
    assert_eq!(sg.entry, "engineer");
    let ids: Vec<&str> = sg.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["apply", "engineer", "qa"]);
    assert_eq!(sg.edges.len(), 3);
    assert_eq!(sg.graph_version, doc2.meta.graph_version);
    // 嵌套深度 1 的 loop 扩展四字段（build）
    let build = &g2.nodes.iter().find(|n| n.id == "build").unwrap();
    let lp = build.loop_spec.as_ref().unwrap();
    assert_eq!(lp.max_iterations, 3);
    assert_eq!(
        lp.exit_when.as_ref().unwrap().source,
        "attempt.passed == true"
    );
    assert_eq!(lp.on_exhausted, Some(OnExhausted::Fail));
    assert_eq!(
        lp.safety_points,
        vec![SafetyPoint {
            node: "qa".into(),
            kind: SafetyPointKind::Check
        }]
    );
}
