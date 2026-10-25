//! G2b stage 4 测试：可执行计划生成（落地契约 §7.1 草案顶层结构）+ IR/计划
//! 双层确定性（双跑逐位一致）+ 逃逸注入计划 + 拒绝式信封 decode + 未绑定
//! 拒绝生成。G2 门自证的库内面（CLI 实跑面见 RESULT-G2b 验收表）。
//!
//! 门样例三份：sample2 / sample3（裸校验链）+ GPTSwarm（对齐稿——
//! registry 注入链，topology_selector 候选图依赖 af/packages 图清单）。

use af_compiler::components::ComponentRegistry;
use af_compiler::ir::EffectClass;
use af_compiler::{
    bind, decode_plan, generate_plan, lower, plan_json_pretty, PlanError, PlanParseError,
    PLAN_VERSION,
};

fn repo_packages_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../af/packages")
}

fn corpus_src(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata/src")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 {path:?}：{e}"))
}

/// 门样例三份 + 各自的校验链（GPTSwarm 走注入，其余裸校验）。
fn gate_sample(name: &str) -> af_compiler::IrProgram {
    let src = corpus_src(name);
    let doc = af_dsl::load_str(&src).unwrap_or_else(|e| panic!("{name} 装载失败：{e}"));
    let report = if name == "GPTSwarm.yaml" {
        let reg = af_dsl::registry::Registry::load(repo_packages_root()).unwrap();
        af_dsl::validate_with(&doc, &reg.options())
    } else {
        af_dsl::validate(&doc)
    };
    assert!(report.is_ok(), "{name} 应过校验：{report}");
    lower(&doc).unwrap()
}

fn repo_registry() -> ComponentRegistry {
    ComponentRegistry::load(repo_packages_root())
        .unwrap_or_else(|e| panic!("af/packages 组件清单装载失败：{e}"))
}

/// 全链（load→validate→lower→bind→plan→pretty 序列化）——确定性测试的
/// 「第二次编译」用独立入口，覆盖解析层。
fn full_chain_plan(name: &str) -> String {
    let ir = gate_sample(name);
    let report = bind(&ir, &repo_registry());
    assert!(
        report.is_ok(),
        "{name} 绑定应零违例：{:?}",
        report.violations
    );
    let plan = generate_plan(&ir, &report).unwrap_or_else(|e| panic!("{name} 计划生成失败：{e}"));
    serde_json::to_string_pretty(&plan).unwrap()
}

// ============================================================
// 计划形状（§7.1 草案顶层结构）
// ============================================================

/// sample2 计划：七块正位 + 自含增补四块；plan_id 确定性派生；node_exec
/// 投影（工具 effect_class 可见/单元绑定/节点预算/envelope 溯源）。
#[test]
fn sample2_plan_shape_matches_contract_skeleton() {
    let ir = gate_sample("sample2.yaml");
    let report = bind(&ir, &repo_registry());
    assert!(report.is_ok());
    let plan = generate_plan(&ir, &report).unwrap();

    // 信封
    assert_eq!(plan.version, PLAN_VERSION);
    let body = &plan.body;

    // plan_id / compiled_from（确定性派生——无时钟无随机）
    assert!(
        body.plan_id.starts_with("plan.metagpt_sop@1.0+"),
        "plan_id 派生形：{}",
        body.plan_id
    );
    let tail = body.plan_id.trim_start_matches("plan.metagpt_sop@1.0+");
    assert_eq!(tail.len(), 12, "dsl_digest 前 12 位");
    assert!(tail.chars().all(|c| c.is_ascii_hexdigit()), "十六进制段");
    assert_eq!(body.compiled_from.graph_version, "1.0");
    assert_eq!(
        body.compiled_from.compiler_version,
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(body.compiled_from.dsl_digest.len(), 64);
    // 绑定快照摘要随计划携带（§2.2——执行面 E9004 核对面）
    assert_eq!(body.compiled_from.registry_digest, repo_registry().digest());

    // line / order 骨架（执行面落位，编译期占位）
    assert!(body.line.line_id.is_none() && body.line.domain_pack_digest.is_none());
    assert_eq!(body.order.state, "Created");
    assert_eq!(body.order.graph_version, "1.0");
    assert!(body.order.budget_voucher.is_none());
    assert!(body.order.artifacts.is_empty());

    // graph = af-graph 原形（IR GraphTemplate 直落）
    assert_eq!(body.graph.name, "metagpt_sop");
    assert_eq!(body.graph.entry, "pm");
    assert!(body.graph.subgraphs.contains_key("metagpt_sop/build"));

    // 计划自含增补四块（§7.2-1 单文件即编排真相）
    assert!(body.stores.contains_key("tasks_board") && body.stores.contains_key("repo"));
    assert!(body.types.contains_key("prd") && body.types.contains_key("code_file"));
    assert_eq!(body.interface.input_type, "requirement_input");
    assert!(body.escapes.is_empty(), "sample2 无逃逸：空映射在场");

    // node_exec：体内节点按点号路径收录；工具 effect_class 回填可见
    let engineer = body
        .node_exec
        .iter()
        .find(|n| n.node_id == "build.engineer")
        .expect("体内节点应入计划 node_exec");
    let tools: Vec<(&str, EffectClass)> = engineer
        .components
        .tools
        .iter()
        .map(|t| (t.reference.as_str(), t.effect_class))
        .collect();
    assert_eq!(
        tools,
        [
            ("pkg.sweng.edit@1", EffectClass::Persist),
            ("pkg.sweng.locate@1", EffectClass::Pure),
            ("pkg.sweng.view@1", EffectClass::Pure),
        ]
    );
    assert_eq!(
        engineer.components.model_route.as_deref(),
        Some("route.main@1")
    );
    // node_exec 按点号路径字典序（确定性）
    let ids: Vec<&str> = body.node_exec.iter().map(|n| n.node_id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "node_exec 按路径字典序");

    // QA 节点：判定单元绑定（§7.1 unit 面）
    let qa = body
        .node_exec
        .iter()
        .find(|n| n.node_id == "build.qa")
        .unwrap();
    let unit = qa.unit.as_ref().unwrap();
    assert_eq!(unit.unit_id, "pkg.sweng.test_unit");
    assert_eq!(unit.unit_version, "1");

    // pm 工件契约：envelope 溯源回填；终态位在 build.report（meta.output.from
    // = build——G2a is_final 规则直传，golden IR 同判）
    let pm = body.node_exec.iter().find(|n| n.node_id == "pm").unwrap();
    let prd = pm
        .artifact_contract
        .iter()
        .find(|a| a.name == "prd")
        .expect("pm.prd 工件契约应在场");
    assert!(!prd.is_final, "终态发射者是 build 非 pm");
    assert_eq!(
        prd.provenance.as_ref().unwrap().envelope_of.as_deref(),
        Some("prd")
    );
    let build = body
        .node_exec
        .iter()
        .find(|n| n.node_id == "build")
        .unwrap();
    let report = build
        .artifact_contract
        .iter()
        .find(|a| a.name == "report")
        .expect("build.report 工件契约应在场");
    assert!(report.is_final, "from_body 转发端口承终态位");

    // 预算段：三账原样（budgets 按 id 序）
    let budget_ids: Vec<&str> = body.budget.budgets.iter().map(|b| b.id.as_str()).collect();
    assert_eq!(budget_ids, ["cost_cap", "fix_turns"]);
    assert!(!body.budget.failures.is_empty());

    // 恢复预案：loop 安全点收集（build→qa:check）
    assert_eq!(body.recovery.safety_points.len(), 1);
    let sp = &body.recovery.safety_points[0];
    assert_eq!(sp.loop_node, "build");
    assert_eq!(sp.node, "qa");
    assert_eq!(sp.kind, "check");
}

/// GPTSwarm 计划（注入链）：逃逸舱注入可执行——六条 EscapeSpec 进计划，
/// impl_ref 与记账字段逐字保留。
#[test]
fn gptswarm_plan_carries_escape_specs() {
    let ir = gate_sample("GPTSwarm.yaml");
    let report = bind(&ir, &repo_registry());
    assert!(report.is_ok(), "GPTSwarm 应零违例：{:?}", report.violations);
    let plan = generate_plan(&ir, &report).unwrap();

    assert_eq!(plan.body.escapes.len(), 6, "六处 L4 逃逸全部入计划");
    let graph_edit = plan
        .body
        .escapes
        .values()
        .find(|e| e.impl_ref == "packages/swarmkit@1.0.0/hatches/graph_edit.py")
        .expect("graph_edit 逃逸应在场");
    assert_eq!(graph_edit.owner, "lang-team");
    assert_eq!(graph_edit.share_budget_pct, 10);
    assert_eq!(
        graph_edit.kind,
        Some(af_compiler::ir::EscapeKind::ProgramSynthesis)
    );
    // 注入点键 = 节点点号路径（体内可寻址）
    assert!(plan.body.escapes.contains_key("search.graph_edit"));
    // GPTSwarm 工具引用绑定（pkg.swarmkit.tools@1 → execute_map 体内）
    let tool_node = plan
        .body
        .node_exec
        .iter()
        .find(|n| {
            n.components
                .tools
                .iter()
                .any(|t| t.reference == "pkg.swarmkit.tools@1")
        })
        .expect("工具节点应在场");
    assert_eq!(
        tool_node.components.tools[0].effect_class,
        EffectClass::ExternalOnce
    );
}

// ============================================================
// 双层确定性（IR + 计划）——G2 门自证第三件
// ============================================================

/// 门样例三份：同一输入两次**独立全链**编译，IR 快照与计划 JSON 双层
/// 逐位一致（提案 §3.5-2 同款纪律，G2a 确定性面沿伸到 stage 4）。
#[test]
fn gate_samples_ir_and_plan_deterministic_bitwise() {
    for name in ["sample2.yaml", "sample3.yaml", "GPTSwarm.yaml"] {
        // IR 层
        let a = gate_sample(name);
        let b = gate_sample(name);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap(),
            "{name}: IR 层两次独立编译逐位不一致"
        );
        // 计划层（独立 load→validate→lower→bind→plan→serialize 全链）
        let p1 = full_chain_plan(name);
        let p2 = full_chain_plan(name);
        assert_eq!(p1, p2, "{name}: 计划层两次独立全链逐位不一致");
        assert!(!p1.is_empty());
    }
}

// ============================================================
// 拒绝式信封与 fail-loud
// ============================================================

/// 序列化 → decode → 与生成产物相等（往返无损）。
#[test]
fn decode_plan_roundtrip_equals_generated() {
    let ir = gate_sample("sample2.yaml");
    let report = bind(&ir, &repo_registry());
    let plan = generate_plan(&ir, &report).unwrap();
    let json = serde_json::to_string(&plan).unwrap();
    let decoded = decode_plan(&json).unwrap();
    assert_eq!(decoded, plan);
    assert_eq!(decoded.version, PLAN_VERSION);
}

/// 信封纪律：版本先行（v=2 拒且 body 不解析）、未知顶层键拒、缺 v 拒、
/// body 未知字段拒（deny_unknown_fields 全层）。
#[test]
fn plan_envelope_is_rejecting() {
    let ir = gate_sample("sample2.yaml");
    let report = bind(&ir, &repo_registry());
    let plan = generate_plan(&ir, &report).unwrap();
    let json = serde_json::to_string(&plan).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();

    // v=2（body 同时注入毒字段——版本先行，不解析 body）
    value["v"] = serde_json::json!(2);
    value["body"]["poison"] = serde_json::json!(1);
    let err = decode_plan(&value.to_string()).unwrap_err();
    assert!(
        matches!(err, PlanParseError::VersionNotOne { found: 2 }),
        "got: {err}"
    );

    // 未知顶层键
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["extra"] = serde_json::json!(1);
    let err = decode_plan(&value.to_string()).unwrap_err();
    assert!(
        matches!(err, PlanParseError::UnknownTopKey { .. }),
        "got: {err}"
    );

    // 缺 v
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value.as_object_mut().unwrap().remove("v");
    let err = decode_plan(&value.to_string()).unwrap_err();
    assert!(matches!(err, PlanParseError::MissingVersion), "got: {err}");

    // body 未知字段（deny_unknown_fields）
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["body"]["mystery"] = serde_json::json!(1);
    let err = decode_plan(&value.to_string()).unwrap_err();
    assert!(
        matches!(err, PlanParseError::MalformedBody { .. }),
        "got: {err}"
    );
}

/// fail-loud：绑定违例非空 → 计划生成拒绝（Unbound 一次点名全部 E900x）。
#[test]
fn unbound_report_refuses_plan_generation() {
    let ir = gate_sample("sample2.yaml");
    let report = bind(&ir, &ComponentRegistry::empty()); // 空注册表：全部引用 E9001
    assert!(!report.is_ok());
    let err = generate_plan(&ir, &report).unwrap_err();
    match err {
        PlanError::Unbound { violations } => {
            assert!(!violations.is_empty());
            assert!(
                violations.len() >= 4,
                "sample2 全部 uses 引用逐条点名：{violations:?}"
            );
        }
        other => panic!("应为 Unbound：{other}"),
    }
}

/// plan_json_pretty 全链入口与分步产物逐位一致（库面单入口的确定性）。
#[test]
fn plan_json_pretty_entry_matches_stepwise() {
    let name = "sample3.yaml";
    let src = corpus_src(name);
    let doc = af_dsl::load_str(&src).unwrap();
    assert!(af_dsl::validate(&doc).is_ok());
    let via_entry = plan_json_pretty(&doc, &repo_registry()).unwrap();
    assert_eq!(via_entry, full_chain_plan(name));
}

/// 图执行器增补投影（v1.1）：node_exec[].context = IR NodeExec.context 原样
/// （template_ref/输入字段面/边端口面——「单文件即编排真相」补齐：此前该面
/// 只在 IR，执行器无从取节点提示词材料）；旧计划（无 context 键）可 decode
/// （serde default，向后兼容）。sample2 = MetaGPT SOP（体内节点带 context）。
#[test]
fn plan_node_exec_projects_context_sections() {
    let ir = gate_sample("sample2.yaml");
    let report = bind(&ir, &repo_registry());
    assert!(report.is_ok());
    let plan = generate_plan(&ir, &report).unwrap();
    let engineer = plan
        .body
        .node_exec
        .iter()
        .find(|n| n.node_id == "build.engineer")
        .expect("体内节点应在 node_exec");
    assert!(
        !engineer.context.is_empty(),
        "engineer 的 context.sections 应投影进计划"
    );
    assert!(
        engineer
            .context
            .iter()
            .any(|s| s.from == "template" && s.template_ref.is_some()),
        "template 段（template_ref 载荷）应在投影面"
    );
    // 投影 = IR 原样（文档序 = 拼接序保持）。
    let ir_exec = ir
        .body
        .node_exec
        .get("build.engineer")
        .expect("IR node_exec 应有同名投影源");
    assert_eq!(engineer.context, ir_exec.context);

    // 向后兼容：删掉 context 键的旧计划 JSON 仍可 decode（缺省空）。
    let mut legacy: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&plan).unwrap()).unwrap();
    for node in legacy["body"]["node_exec"].as_array_mut().unwrap() {
        node.as_object_mut().unwrap().remove("context");
    }
    let decoded = decode_plan(&legacy.to_string()).expect("旧计划（无 context 键）应可 decode");
    assert!(decoded.body.node_exec.iter().all(|n| n.context.is_empty()));
}
