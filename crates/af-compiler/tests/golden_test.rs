//! G2a golden 主测：六样例确定性（两次编译逐位一致）+ decode 往返 + 样例 1
//! 投影抽查（interface/stores/policies/node_exec 全字段落地）。

mod common;

use af_compiler::ir::*;

fn find_edge<'a>(edges: &'a [IrEdge], from: &str, output: &str, to: &str, input: &str) -> &'a IrEdge {
    edges
        .iter()
        .find(|e| e.from == from && e.output == output && e.to == to && e.input == input)
        .unwrap_or_else(|| panic!("边 {from}.{output}->{to}.{input} 不存在"))
}

/// `&[&str]` → `BTreeSet<String>`（writes/reads 断言用）。
fn strset(items: &[&str]) -> std::collections::BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// 完整编译管线（独立 load + validate + lower + 序列化）——确定性测试的
/// 「第二次编译」用独立入口而非复用 Document，覆盖解析层。
fn pipeline_json(src: &str) -> String {
    let doc = af_dsl::load_str(src).unwrap_or_else(|e| panic!("装载失败：{e}"));
    let report = af_dsl::validate(&doc);
    assert!(report.is_ok(), "管线内校验未过：{report}");
    af_compiler::snapshot_json(&doc).expect("lowering 失败")
}

#[test]
fn six_golden_samples_pass_full_validation() {
    for name in common::SAMPLES {
        let (_doc, _src) = common::sample(name); // sample() 内断言零违例
    }
}

/// 确定性（提案 §3.5-2）：同一输入两次编译，IR 序列化逐位一致。
/// 两条独立管线（各自 load+validate+lower+serialize）+ 同一 Document 两次
/// lowering，三种口径都逐位相等。
#[test]
fn six_golden_deterministic_bitwise() {
    for name in common::SAMPLES {
        let (doc, src) = common::sample(name);
        let a = pipeline_json(&src);
        let b = pipeline_json(&src);
        assert!(!a.is_empty(), "{name}: 快照非空");
        assert_eq!(a, b, "{name}: 两次独立管线编译逐位不一致");

        let s1 = af_compiler::snapshot_json(&doc).expect("{name}: lowering 1 失败");
        let s2 = af_compiler::snapshot_json(&doc).expect("{name}: lowering 2 失败");
        assert_eq!(s1, s2, "{name}: 同一 Document 两次 lowering 逐位不一致");
        assert_eq!(s1, a, "{name}: 管线产物与文档直降不一致");
    }
}

/// 序列化 → 拒绝式 decode → 与 lowering 产物相等（往返无损）。
#[test]
fn decode_roundtrip_equals_lowered() {
    for name in common::SAMPLES {
        let (doc, _src) = common::sample(name);
        let program = af_compiler::lower(&doc).expect("{name}: lowering 失败");
        let json = af_compiler::snapshot_json(&doc).expect("{name}: 序列化失败");
        let decoded = af_compiler::decode_ir(&json).expect("{name}: decode 失败");
        assert_eq!(decoded, program, "{name}: decode 往返不等于原 IR");
        assert_eq!(decoded.ir_version, af_compiler::IR_VERSION);
    }
}

/// 样例 1（单节点 ReAct）：全字段投影抽查——meta→interface、stores、
/// policies 三账、node_exec（components/artifacts/context/writes）。
#[test]
fn sample1_projection_facts() {
    let (doc, _src) = common::sample("sample1.yaml");
    let p = af_compiler::lower(&doc).expect("lowering 失败");

    assert_eq!(p.ir_version, 1);
    let b = &p.body;
    assert_eq!(b.graph.name, "react_mono");
    assert_eq!(b.graph.graph_version, "1.0");
    assert_eq!(b.graph.entry, "solver");
    assert_eq!(b.graph.nodes.len(), 1);
    assert_eq!(b.graph.nodes[0].id, "solver");
    assert_eq!(b.graph.nodes[0].kind, "task");
    assert!(b.graph.edges.is_empty());
    assert!(b.graph.subgraphs.is_empty());
    assert!(b.escapes.is_empty());
    assert!(b.trust.is_empty());
    assert_eq!(b.types.len(), 2);
    assert!(b.types.contains_key("react_input"));
    assert!(b.types.contains_key("answer"));

    // interface（提案 §1.2 meta 映射表）
    let i = &b.interface;
    assert_eq!(i.input_type, "react_input");
    assert_eq!(i.output_type, "answer");
    assert_eq!(i.output_from, "solver");
    assert_eq!(i.intent.as_deref(), Some("单 agent 节点：工具回灌在节点运行时内串行落账；语义终止 + 三重预算"));

    // stores
    let st = &b.stores["trajectory"];
    assert_eq!(st.kind, StoreKind::MessageLog);
    assert_eq!(st.writer, "solver");
    assert_eq!(st.lifecycle, Lifecycle::Run);
    assert_eq!(st.retention.max_items, Some(128));
    assert!(!st.eval_input);

    // node_exec：solver 全投影
    let ex = &b.node_exec["solver"];
    assert_eq!(ex.node_id, "solver");
    assert_eq!(ex.components.model.as_deref(), Some("route.main@1"));
    assert_eq!(ex.components.unit, None);
    assert!(ex.components.memory.is_empty());
    let tool_names: Vec<&str> = ex.components.tools.keys().map(String::as_str).collect();
    assert_eq!(tool_names, ["pkg.hopqa.search", "pkg.hopqa.lookup"]);
    assert!(ex.components.tools.values().all(|t| t.effect_class.is_none()));
    assert_eq!(ex.writes, strset(&["trajectory"]));
    assert!(ex.reads.is_empty());
    assert!(ex.consumes.is_empty());
    let art = &ex.artifacts["answer"];
    assert_eq!(art.r#type.as_deref(), Some("answer"));
    assert_eq!(art.from_body, None);
    assert!(art.is_final);
    assert_eq!(art.provenance, None);
    assert_eq!(ex.context.len(), 3);
    assert_eq!(ex.context[0].from, "template");
    assert_eq!(ex.context[1].from, "input");
    assert_eq!(ex.context[2].from, "store");
    assert_eq!(ex.context[2].store.as_deref(), Some("trajectory"));
    assert!(ex.ops.is_empty());
    assert_eq!(ex.assert, None);

    // policies 三账：budgets 按 id 排序；stop/failures 文档序
    let pol = &b.policies;
    let budget_ids: Vec<&str> = pol.budgets.iter().map(|x| x.id.as_str()).collect();
    assert_eq!(budget_ids, ["cost_cap", "token_cap", "turn_cap"]);
    assert_eq!(pol.budgets[0].dimension, BudgetDimension::Cents);
    assert_eq!(pol.budgets[0].scope, "plan");
    assert_eq!(pol.budgets[0].cap, 120);
    assert_eq!(pol.budgets[1].scope, "node:solver");
    assert_eq!(pol.budgets[2].cap, 12);
    assert_eq!(pol.stop.len(), 2);
    assert_eq!(
        pol.stop[0].when.source,
        "last_tool.first_line in ['FINISH', 'FINAL ANSWER']"
    );
    assert_eq!(pol.stop[0].outcome, StopOutcome::Completed);
    assert_eq!(pol.stop[1].when.source, "steps.tool_free >= 2");
    assert_eq!(pol.stop[1].outcome, StopOutcome::Failed);
    assert_eq!(pol.failures.len(), 3);
    assert_eq!(pol.failures[0].class, FailureClass::TransportTransient);
    assert_eq!(pol.failures[0].retry, Some(RetrySpec { max: 3, backoff_ms: 800 }));
    assert_eq!(pol.failures[1].class, FailureClass::ToolError);
    assert_eq!(pol.failures[1].route.as_deref(), Some("same_node"));
    assert_eq!(pol.failures[1].feedback.as_deref(), Some("error_observation"));
    assert_eq!(pol.failures[2].class, FailureClass::BudgetExhausted);
    assert_eq!(pol.failures[2].terminal, Some(true));
    assert_eq!(pol.failures[2].outcome, Some(StopOutcome::Failed));
}

/// 样例 4/5 抽查：eval_input 槽（trust 边界源数据）、check 节点 unit、
/// 非 LLM 节点直读、loop done 端口不进 artifacts、from_body 声明保真。
#[test]
fn sample4_sample5_projection_facts() {
    let (doc4, _) = common::sample("sample4.yaml");
    let p4 = af_compiler::lower(&doc4).expect("lowering 失败");
    // eval_input = true（INV-4 trust 边界在 StoreSpec 一等位）
    assert!(p4.body.stores["answers_ref"].eval_input);
    // check 节点 uses.unit 投影
    let assess = &p4.body.node_exec["search.assess"];
    assert_eq!(assess.node_id, "assess");
    assert_eq!(assess.components.unit.as_deref(), Some("pkg.searchkit.assess@1"));
    assert_eq!(assess.reads, strset(&["answers_ref"]));
    // 非输出节点工件非终态；signal 类型原样
    let loader = &p4.body.node_exec["loader"];
    assert_eq!(loader.artifacts["done"].r#type.as_deref(), Some("signal"));
    assert!(!loader.artifacts["done"].is_final);
    // from_body 声明保真（map 构造自动包 list<> 的推导不在此复制——见 ir.rs 注记）
    let run = &p4.body.node_exec["search.run"];
    assert_eq!(run.artifacts["items"].from_body.as_deref(), Some("aggregate.answer"));

    let (doc5, _) = common::sample("sample5.yaml");
    let p5 = af_compiler::lower(&doc5).expect("lowering 失败");
    // 体内 LLM 节点：工具集 + 两个 message_log 段交错读（context 6 段文档序）
    let agent = &p5.body.node_exec["react_ring.agent"];
    assert_eq!(agent.context.len(), 6);
    assert_eq!(agent.context[2].store.as_deref(), Some("agent_turns"));
    assert_eq!(agent.context[3].store.as_deref(), Some("tool_turns"));
    // 边 checkpoint 投影（LangGraph checkpointer 声明位）
    let sg = &p5.body.graph.subgraphs["langgraph_subset/react_ring"];
    assert!(find_edge(sg.edges.as_slice(), "agent", "action", "tools", "action").checkpoint);
    assert!(find_edge(sg.edges.as_slice(), "tools", "observation", "format_check", "observation").checkpoint);
    assert!(!find_edge(sg.edges.as_slice(), "format_check", "checked", "agent", "checked").checkpoint);
    // human 构造：station/role/quorum/resume.schema
    let approve = &sg.nodes.iter().find(|n| n.id == "approve").unwrap();
    let h = approve.human.as_ref().unwrap();
    assert_eq!(h.station, "review-desk");
    assert_eq!(h.role, "reviewer");
    assert_eq!(h.quorum, Some(1));
    assert_eq!(h.resume.schema["properties"]["approved"]["type"], "boolean");
    // loop 节点的 from_body 转发 + 终态位
    let ring = &p5.body.node_exec["react_ring"];
    assert_eq!(ring.artifacts["answer"].from_body.as_deref(), Some("agent.answer"));
    assert!(ring.artifacts["answer"].is_final);
}
