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

/// 图执行器 B 席嫁接（合流批）：emits 类型表达式投影 `artifact_contract[].type`
/// 与入边消费契约投影 `node_exec[].consumes`（端口名 → 类型表达式串）。
/// 载体 = 仓库正本 sop_full@1.1.afb（planner emits task_plan；coder emits
/// list&lt;code_file&gt;；coder·reviewer·tester 各带 consumes）。两者均
/// serde(default) + skip_serializing_if——旧计划（无此键）仍可 decode，与
/// context 投影同一向后兼容方针。
#[test]
fn plan_projects_emits_types_and_consumes_contracts() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../af/packages/sweng/graphs/sop_full@1.1.afb"),
    )
    .expect("sop_full@1.1.afb 仓库正本应在盘");
    let doc = af_dsl::load_str(&src).expect("sop_full 应装载");
    assert!(af_dsl::validate(&doc).is_ok());
    let ir = lower(&doc).unwrap();
    let report = bind(&ir, &repo_registry());
    assert!(report.is_ok());
    let plan = generate_plan(&ir, &report).unwrap();
    let node = |id: &str| {
        plan.body
            .node_exec
            .iter()
            .find(|n| n.node_id == id)
            .unwrap_or_else(|| panic!("node_exec 应含 {id}"))
    };

    // 上游面：emits 类型表达式（含 list<T> 形）。
    let planner = node("planner");
    assert_eq!(
        planner
            .artifact_contract
            .iter()
            .find(|a| a.name == "plan")
            .and_then(|a| a.r#type.as_deref()),
        Some("task_plan"),
        "planner emits plan: task_plan"
    );
    let coder = node("coder");
    assert_eq!(
        coder
            .artifact_contract
            .iter()
            .find(|a| a.name == "files")
            .and_then(|a| a.r#type.as_deref()),
        Some("list<code_file>"),
        "coder emits files: list<code_file>"
    );

    // 下游面：consumes 契约（端口名 → 类型表达式）。
    assert_eq!(
        coder.consumes.get("plan").map(String::as_str),
        Some("task_plan")
    );
    let reviewer = node("reviewer");
    assert_eq!(
        reviewer.consumes.get("plan").map(String::as_str),
        Some("task_plan")
    );
    assert_eq!(
        reviewer.consumes.get("files").map(String::as_str),
        Some("list<code_file>")
    );
    let tester = node("tester");
    assert_eq!(
        tester.consumes.get("files").map(String::as_str),
        Some("list<code_file>")
    );
    assert_eq!(
        tester.consumes.get("review").map(String::as_str),
        Some("review_report")
    );

    // 向后兼容：删掉 type/consumes 键的旧计划 JSON 仍可 decode（缺省空）。
    let mut legacy: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&plan).unwrap()).unwrap();
    for node in legacy["body"]["node_exec"].as_array_mut().unwrap() {
        node.as_object_mut().unwrap().remove("consumes");
        for artifact in node["artifact_contract"].as_array_mut().unwrap() {
            artifact.as_object_mut().unwrap().remove("type");
        }
    }
    let decoded =
        decode_plan(&legacy.to_string()).expect("旧计划（无 type/consumes 键）应可 decode");
    assert!(decoded.body.node_exec.iter().all(|n| n.consumes.is_empty()));
    assert!(decoded
        .body
        .node_exec
        .iter()
        .flat_map(|n| n.artifact_contract.iter())
        .all(|a| a.r#type.is_none()));
}

/// 预算校准批（F-第四单续二，2026-10-25）：F4C 实测主单 EX4007 熔断于
/// planner 1,909 + coder 512,516 = 514,425 tok（tokens_total 口径含
/// cache_read；af-state ledger seq6 逐位闭合），31/60 turns 处 400k 帽先于
/// turns 帽压制真实编码负载。裁决：sop_full 版本化 1.0→1.1，plan 级
/// token_cap 400,000→1,000,000（planner+coder 实测 + reviewer 预估 +
/// 余量）；cost_cap 600¢ 与 coder/reviewer turns 帽不动。
#[test]
fn sop_full_v1_1_token_cap_calibrated_to_f4c_measurement() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../af/packages/sweng/graphs/sop_full@1.1.afb"),
    )
    .expect("sop_full@1.1.afb 仓库正本应在盘（预算校准批版本化 1.0→1.1）");
    let doc = af_dsl::load_str(&src).expect("sop_full@1.1 应装载");
    assert!(af_dsl::validate(&doc).is_ok(), "sop_full@1.1 应零违例");
    let ir = lower(&doc).unwrap();
    let report = bind(&ir, &repo_registry());
    assert!(
        report.is_ok(),
        "sop_full@1.1 绑定应零违例: {:?}",
        report.violations
    );
    let plan = generate_plan(&ir, &report).unwrap();

    // 版本化面：plan_id 携带 @1.1（新 dsl_digest 随之）。
    assert!(
        plan.body.plan_id.starts_with("plan.sweng_sop_full@1.1+"),
        "plan_id 应为 @1.1 版本化产物，got {}",
        plan.body.plan_id
    );

    // 注册面：仓库正本声明 @1.1，@1.0 退场（旧引用清零的声明面证据）。
    let reg = af_dsl::registry::Registry::load(repo_packages_root()).unwrap();
    let declared = reg.declared_graphs();
    assert!(
        declared.contains(&"pkg.sweng.sop_full@1.1".to_string()),
        "af/packages 应声明 pkg.sweng.sop_full@1.1"
    );
    assert!(
        !declared.contains(&"pkg.sweng.sop_full@1.0".to_string()),
        "pkg.sweng.sop_full@1.0 应已退场（版本化不留双版本）"
    );

    // 预算面（plan body 投影）：token_cap 校准 1,000,000（scope/dimension
    // 口径不变）；其余三条逐项不动。
    let entry = |id: &str| {
        plan.body
            .budget
            .budgets
            .iter()
            .find(|b| b.id == id)
            .unwrap_or_else(|| panic!("预算条目 {id} 应存在"))
    };
    let token = entry("token_cap");
    assert_eq!(
        (token.cap, token.scope.as_str(), token.dimension),
        (
            1_000_000,
            "plan",
            af_compiler::ir::BudgetDimension::TokensTotal
        ),
        "token_cap：F4C 实测 514,425 tok 熔断 → 1,000,000（planner 1,909 + coder \
         512,516@31/60 turns + reviewer 预估 + 余量），口径仍 plan/tokens_total"
    );
    let cost = entry("cost_cap");
    assert_eq!(
        (cost.cap, cost.scope.as_str(), cost.dimension),
        (600, "plan", af_compiler::ir::BudgetDimension::Cents),
        "cost_cap 不动：F4C coder 实测 32¢，600¢ 帽余量充足"
    );
    let coder_turns = entry("coder_turns");
    assert_eq!(
        (
            coder_turns.cap,
            coder_turns.scope.as_str(),
            coder_turns.dimension
        ),
        (60, "node:coder", af_compiler::ir::BudgetDimension::Turns),
        "coder_turns 不动：F4C 实测 31/60 未触"
    );
    let reviewer_turns = entry("reviewer_turns");
    assert_eq!(
        (
            reviewer_turns.cap,
            reviewer_turns.scope.as_str(),
            reviewer_turns.dimension
        ),
        (12, "node:reviewer", af_compiler::ir::BudgetDimension::Turns),
        "reviewer_turns 不动"
    );
}
