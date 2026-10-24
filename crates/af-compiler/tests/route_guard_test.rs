//! route 归一化落 IR（提案 §1.5 → §3.2「route 归一化为 Edge.guard；goto: exit
//! 不产生边」）：guard 公式逐字断言（确定性重构形）、exit 守卫记节点、
//! 非路由边无守卫、CelRef（roots/digest）与 CEL 词汇表一致性。

mod common;

use af_compiler::ir::*;

fn find_edge<'a>(edges: &'a [IrEdge], from: &str, to: &str) -> &'a IrEdge {
    edges
        .iter()
        .find(|e| e.from == from && e.to == to)
        .unwrap_or_else(|| panic!("边 {from}->*->{to} 不存在"))
}

fn assert_cel(cel: &CelRef, source: &str, roots: &[&str]) {
    assert_eq!(cel.source, source, "guard 源串应为 §1.5 确定性重构形");
    let want: std::collections::BTreeSet<String> = roots.iter().map(|s| s.to_string()).collect();
    assert_eq!(cel.roots, want, "roots 应为表达式实际引用的根绑定集");
    assert_eq!(cel.digest.len(), 64, "digest = SHA-256 十六进制 64 字符");
    assert!(
        cel.digest
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "digest 应为小写十六进制"
    );
}

/// 样例 5：default 命中边（规则全否守卫）+ goto:exit 规则的 exit 守卫 +
/// 规则命中边（首规则守卫）+ default:exit 不产边。
#[test]
fn sample5_route_guards() {
    let (doc, _src) = common::sample("sample5.yaml");
    let p = af_compiler::lower(&doc).expect("lowering 失败");
    let sg = &p.body.graph.subgraphs["langgraph_subset/react_ring"];

    // format_check：rules=[goto exit]，default=agent → 出边带 default 守卫
    let e = find_edge(&sg.edges, "format_check", "agent");
    assert_cel(
        e.guard.as_ref().expect("route 节点出边必带守卫"),
        "!((format_check.passed == false))",
        &["format_check"],
    );
    let fc = &sg.nodes.iter().find(|n| n.id == "format_check").unwrap();
    assert_cel(
        fc.route_exit_guard
            .as_ref()
            .expect("goto:exit 分支守卫记在节点"),
        "(format_check.passed == false)",
        &["format_check"],
    );

    // approve：rules=[goto agent]，default=exit → 出边带首规则守卫；
    // default:exit 无规则型 exit 分支 → 无节点 exit 守卫（结构性收束，
    // 由 loop exit_when "resume.approved == true" 承担早退语义）
    let e = find_edge(&sg.edges, "approve", "agent");
    assert_cel(
        e.guard.as_ref().expect("route 节点出边必带守卫"),
        "(resume.approved == false)",
        &["resume"],
    );
    let approve = &sg.nodes.iter().find(|n| n.id == "approve").unwrap();
    assert_eq!(approve.route_exit_guard, None);

    // 非路由边无守卫
    assert_eq!(find_edge(&sg.edges, "agent", "tools").guard, None);
    // goto: exit 不产生边：全图无 to == "exit" 的边
    for e in &sg.edges {
        assert_ne!(e.to, "exit", "goto:exit 不得产生边");
    }
    for e in &p.body.graph.edges {
        assert_ne!(e.to, "exit");
    }

    // gate assert → node_exec 投影
    let fc_exec = &p.body.node_exec["react_ring.format_check"];
    assert_cel(
        fc_exec.assert.as_ref().expect("gate 的 assert 应投影"),
        "inputs.observation.ok == true",
        &["inputs"],
    );
    // loop exit_when → CelRef（roots = budget 词汇表根）
    let ring = &p
        .body
        .graph
        .nodes
        .iter()
        .find(|n| n.id == "react_ring")
        .unwrap();
    assert_cel(
        ring.loop_spec.as_ref().unwrap().exit_when.as_ref().unwrap(),
        "resume.approved == true",
        &["resume"],
    );
}

/// 样例 3：多出边路由——规则命中边与 default 命中边守卫互斥（构造性）。
#[test]
fn sample3_route_guards_mutual_exclusion() {
    let (doc, _src) = common::sample("sample3.yaml");
    let p = af_compiler::lower(&doc).expect("lowering 失败");
    let g = &p.body.graph;

    let rule_src = "(vote.stats.top_count * 2 > vote.stats.answers)";
    let to_finalize = find_edge(&g.edges, "vote", "finalize");
    assert_cel(to_finalize.guard.as_ref().unwrap(), rule_src, &["vote"]);
    let to_adjudicate = find_edge(&g.edges, "vote", "adjudicate");
    assert_cel(
        to_adjudicate.guard.as_ref().unwrap(),
        &format!("!({rule_src})"),
        &["vote"],
    );
    let vote = &g.nodes.iter().find(|n| n.id == "vote").unwrap();
    assert_eq!(vote.route_exit_guard, None);
    // 非路由边（adjudicate→finalize）无守卫
    assert_eq!(find_edge(&g.edges, "adjudicate", "finalize").guard, None);
}

/// 样例 2：QA 回环——goto engineer 规则守卫 + default:exit 无边；
/// 样例 6：体内 gate 的 exit 守卫（嵌套子图内路由）。
#[test]
fn sample2_sample6_route_guards() {
    let (doc2, _) = common::sample("sample2.yaml");
    let p2 = af_compiler::lower(&doc2).expect("lowering 失败");
    let sg2 = &p2.body.graph.subgraphs["metagpt_sop/build"];
    let e = find_edge(&sg2.edges, "qa", "engineer");
    assert_cel(e.guard.as_ref().unwrap(), "(qa.passed == false)", &["qa"]);
    let qa = &sg2.nodes.iter().find(|n| n.id == "qa").unwrap();
    assert_eq!(qa.route_exit_guard, None);

    let (doc6, _) = common::sample("sample6.yaml");
    let p6 = af_compiler::lower(&doc6).expect("lowering 失败");
    let inner = &p6.body.graph.subgraphs["voyager_curriculum/explore/acquire"];
    let e = find_edge(&inner.edges, "admit", "curator");
    assert_cel(
        e.guard.as_ref().unwrap(),
        "!((admit.passed == false))",
        &["admit"],
    );
    let admit = &inner.nodes.iter().find(|n| n.id == "admit").unwrap();
    assert_cel(
        admit.route_exit_guard.as_ref().unwrap(),
        "(admit.passed == false)",
        &["admit"],
    );
    // 根图（无边路由样例）守卫恒空
    for e in &p6.body.graph.edges {
        assert_eq!(e.guard, None);
    }
}

/// CelRef.digest 稳定性：同一源串跨样例/跨次编译同 digest（确定性 §3.5-4：
/// 规范序列化键序=字典序后取 SHA-256）。
#[test]
fn cel_digest_stable_across_compiles() {
    for name in common::SAMPLES {
        let (doc, _) = common::sample(name);
        let p1 = af_compiler::lower(&doc).expect("lowering 失败");
        let p2 = af_compiler::lower(&doc).expect("lowering 失败");
        let j1 = af_compiler::snapshot_json(&doc).unwrap();
        let j2 = af_compiler::snapshot_json(&doc).unwrap();
        assert_eq!(j1, j2);
        // 全部 CelRef 的 digest 由 (source, roots) 唯一决定：两次编译逐字段相等
        let collect = |p: &af_compiler::ir::IrProgram| -> Vec<String> {
            let mut out = Vec::new();
            let mut push =
                |c: &CelRef| out.push(format!("{}|{:?}|{}", c.source, c.roots, c.digest));
            for n in &p.body.graph.nodes {
                if let Some(c) = &n.route_exit_guard {
                    push(c);
                }
                if let Some(lp) = &n.loop_spec {
                    if let Some(c) = &lp.exit_when {
                        push(c);
                    }
                }
            }
            for sg in p.body.graph.subgraphs.values() {
                for n in &sg.nodes {
                    if let Some(c) = &n.route_exit_guard {
                        push(c);
                    }
                    if let Some(lp) = &n.loop_spec {
                        if let Some(c) = &lp.exit_when {
                            push(c);
                        }
                    }
                }
                for e in &sg.edges {
                    if let Some(c) = &e.guard {
                        push(c);
                    }
                }
            }
            for e in &p.body.graph.edges {
                if let Some(c) = &e.guard {
                    push(c);
                }
            }
            for ex in p.body.node_exec.values() {
                if let Some(c) = &ex.assert {
                    push(c);
                }
            }
            for s in &p.body.policies.stop {
                out.push(format!(
                    "{}|{:?}|{}",
                    s.when.source, s.when.roots, s.when.digest
                ));
            }
            out.sort_unstable();
            out
        };
        assert_eq!(
            collect(&p1),
            collect(&p2),
            "{name}: CelRef 集两次编译不一致"
        );
        assert!(
            !collect(&p1).is_empty(),
            "{name}: 样例应含至少一条 CEL 位点"
        );
    }
}
