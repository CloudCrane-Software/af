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
    assert_eq!(tool_names, ["pkg.hopqa.lookup", "pkg.hopqa.search"]);
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
    assert_eq!(pol.budgets[0].scope, "node:solver"); // 样例 1 三预算均为节点级
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

// ---------------------------------------------------------------------------
// G3 golden 入库回归（宪法 C10 基线）。以下全部为追加测试，不改上文任何
// 既有测试的语义。
//
// 语料 = 11 份对齐草稿 + af-dsl 六样例，共 17 份，源 YAML 入
// `testdata/src/`、IR 快照入 `testdata/golden/`。核验口径与 CLI
// `af-compiler-snapshot` 完全一致：load → 全量校验 → lower → pretty 序列化
// + 换行（writeln 的产物），golden 文件即 CLI stdout 的逐字节落盘。
// ---------------------------------------------------------------------------

/// G3 入库清单（闭集）：文件名与 `testdata/src/` 一致。C10 变更纪律——
/// 快照或清单的任何变化只允许两种解释：有意识演进（ADR + 此处登记）或
/// 回归（修复）。新增/移除条目必须改这里，不允许静默漂移。
const G3_CORPUS: [&str; 17] = [
    // 11 份编译干净的对齐草稿（selector 拒绝稿不入库）
    "ReAct.yaml",
    "Reflexion.yaml",
    "Voyager.yaml",
    "SWE-agent.yaml",
    "MemGPT.yaml",
    "CAMEL.yaml",
    "Multi-Agent-Debate.yaml",
    "AFlow.yaml",
    "LangGraph-StateGraph.yaml",
    "CrewAI-YAML.yaml",
    "Agensh-DeLM.yaml", // 已修订版
    // af-dsl 提案 B §4 六样例
    "sample1.yaml",
    "sample2.yaml",
    "sample3.yaml",
    "sample4.yaml",
    "sample5.yaml",
    "sample6.yaml",
];

/// 篡改检测的目标样例（清单中最小的一份，加速测试）。
const TAMPER_TARGET: &str = "sample1.yaml";

fn corpus_src_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/src").join(name)
}

fn corpus_golden_path(name: &str) -> std::path::PathBuf {
    let stem = name.strip_suffix(".yaml").expect("语料文件名应以 .yaml 结尾");
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata/golden")
        .join(format!("{stem}.ir.json"))
}

/// 单份核验（与 CLI 同口径）：重新 load + 全量校验 + lower + pretty 序列化
/// + 换行，与 `testdata/golden/<stem>.ir.json` **逐字节**比对。任何一位
/// 不同即 panic——这是篡改检测测试借用的同一把牙齿。
fn assert_golden_byte_exact(name: &str) {
    let src_path = corpus_src_path(name);
    let src = std::fs::read_to_string(&src_path)
        .unwrap_or_else(|e| panic!("读取 {src_path:?}：{e}"));
    let doc = af_dsl::load_str(&src).unwrap_or_else(|e| panic!("{name} 装载失败：{e}"));
    let report = af_dsl::validate(&doc);
    assert!(report.is_ok(), "{name} 应过 af-dsl 全量校验：{report}");
    let json = af_compiler::snapshot_json_pretty(&doc)
        .unwrap_or_else(|e| panic!("{name} lowering 失败：{e}"));
    // CLI 末尾 writeln! 一个换行——golden 即 CLI stdout 逐字节落盘
    let expected: Vec<u8> = json.as_bytes().iter().copied().chain(std::iter::once(b'\n')).collect();

    let golden_path = corpus_golden_path(name);
    let actual = std::fs::read(&golden_path)
        .unwrap_or_else(|e| panic!("读取 golden {golden_path:?}：{e}"));

    if expected != actual {
        // 报告首个差异位与上下文，避免整个文件倾倒进断言消息
        let (el, al) = (expected.len(), actual.len());
        let pos = expected
            .iter()
            .zip(actual.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(el.min(al));
        let ctx = |v: &[u8]| {
            let s = pos.saturating_sub(20);
            String::from_utf8_lossy(&v[s..(pos + 20).min(v.len())]).escape_debug().to_string()
        };
        panic!(
            "{name}: golden 逐字节不一致——期望 {el} 字节 / 实际 {al} 字节，\
             首个差异位 @{pos}（期望 …{}… / 实际 …{}…）",
            ctx(&expected),
            ctx(&actual)
        );
    }
}

/// G3 主回归：17 份语料全部重新 lower+serialize，与 golden 逐字节相等。
#[test]
fn g3_golden_corpus_byte_exact() {
    for name in G3_CORPUS {
        assert_golden_byte_exact(name);
    }
}

/// 清单闭集守卫：`testdata/src` 与 `testdata/golden` 的文件集合恰等于
/// G3_CORPUS（src 一份不多不少，golden 一份不多不少）。防未登记文件混入。
#[test]
fn g3_golden_corpus_manifest_closed() {
    let manifest: std::collections::BTreeSet<&str> = G3_CORPUS.into_iter().collect();

    let src_files: std::collections::BTreeSet<String> =
        std::fs::read_dir(corpus_src_path(G3_CORPUS[0]).parent().unwrap())
            .expect("读 testdata/src 失败")
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".yaml"))
            .collect();
    let expected_src: std::collections::BTreeSet<String> =
        manifest.iter().map(|n| n.to_string()).collect();
    assert_eq!(src_files, expected_src, "testdata/src 与 G3 清单不一致（C10：须登记后变更）");

    let golden_files: std::collections::BTreeSet<String> =
        std::fs::read_dir(corpus_golden_path(G3_CORPUS[0]).parent().unwrap())
            .expect("读 testdata/golden 失败")
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".ir.json"))
            .collect();
    let expected_golden: std::collections::BTreeSet<String> = manifest
        .iter()
        .map(|n| format!("{}.ir.json", n.strip_suffix(".yaml").unwrap()))
        .collect();
    assert_eq!(
        golden_files, expected_golden,
        "testdata/golden 与 G3 清单不一致（C10：须登记后变更）"
    );
}

/// 篡改检测（证明测试有牙齿）：临时篡改 `TAMPER_TARGET` 的 golden 一个
/// 字符（首字节 '{' → ' '），断言逐字节核验**失败**；随后恢复原字节并
/// 复核通过。任何一步异常都会留下失败测试或（被恢复的）原样文件。
#[test]
fn g3_golden_tamper_detection() {
    let golden_path = corpus_golden_path(TAMPER_TARGET);
    let original = std::fs::read(&golden_path)
        .unwrap_or_else(|e| panic!("读取 {golden_path:?}：{e}"));
    assert_eq!(original.first(), Some(&b'{'), "golden 应以 '{{' 开头，才有可篡改的字符");

    // 篡改一个字符并落盘
    let mut tampered = original.clone();
    tampered[0] = b' ';
    std::fs::write(&golden_path, &tampered).expect("写入篡改 golden 失败");

    // 核验必须失败（panic 被捕获）；临时静音 panic 钩子避免噪音
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let caught =
        std::panic::catch_unwind(|| assert_golden_byte_exact(TAMPER_TARGET));
    std::panic::set_hook(prev_hook);
    assert!(
        caught.is_err(),
        "篡改 golden 一个字符后逐字节核验仍未失败——测试没有牙齿"
    );

    // 恢复原字节并复核
    std::fs::write(&golden_path, &original).expect("恢复 golden 失败");
    assert_golden_byte_exact(TAMPER_TARGET);
}
