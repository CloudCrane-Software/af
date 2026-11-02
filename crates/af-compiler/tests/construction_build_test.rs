//! E1 缺口①：建设任务（build 单节点 + deliverable 类型）的四段全链编译
//! 产物 = 可提交 plan 形态（计划形态缺口销项）。
//!
//! 契约（E1-报告 2026-10-30 五条缺口清单之①）：
//! - af-compiler 全链入口（装载 → 校验 → lower → bind → plan）对建设图
//!   产出 `{v:1, body:…}` 拒绝式信封计划——即 `<data_root>/<task>/plan.json`
//!   提交形态，非 afb 源形；
//! - 图输入 = construction_task_input（通用字段 task_id/task_book，必填仅
//!   这两项）——调用方不手填 SWE 字段（instance_id/repo/problem_statement
//!   不在必填面，镜像 image 可缺省由执行器补默认）；
//! - 输出 = deliverable 类型（files + tests_passed）从 build 节点发射；
//! - budgets/stops/failures 随包进计划 budget/recovery 块；
//! - 双跑逐位一致（编译链确定性纪律，提案 B §3.5）。

use af_compiler::components::ComponentRegistry;
use af_compiler::{bind, decode_plan, generate_plan, lower, plan_json_pretty};

fn repo_packages_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../af/packages")
}

fn construction_doc() -> af_dsl::Document {
    // 注意：本样例不入 G3 golden 闭集（C10 登记 ceremony 不属于本缺口）——
    // 放 tests/fixtures，由本测试直接消费。
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/construction_build.yaml");
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 {path:?}：{e}"));
    af_dsl::load_str(&src).unwrap_or_else(|e| panic!("construction_build 装载失败：{e}"))
}

fn construction_plan() -> af_compiler::ExecutablePlan {
    let doc = construction_doc();
    let report = af_dsl::validate(&doc);
    assert!(report.is_ok(), "建设图应过语义校验：{report}");
    let ir = lower(&doc).unwrap();
    let registry = ComponentRegistry::load(repo_packages_root())
        .unwrap_or_else(|e| panic!("af/packages 组件清单装载失败：{e}"));
    let report = bind(&ir, &registry);
    assert!(
        report.is_ok(),
        "建设图绑定应零违例：{:?}",
        report.violations
    );
    generate_plan(&ir, &report).unwrap()
}

#[test]
fn construction_build_compiles_to_submittable_plan_envelope() {
    let plan = construction_plan();
    // 提交形态 = 拒绝式信封（执行面 planbind/decode_plan 的同一消费面）。
    assert_eq!(plan.version, af_compiler::PLAN_VERSION);
    let body = &plan.body;

    // plan_id 确定性派生形（图名@图版本+digest 前 12 位）。
    assert!(
        body.plan_id
            .starts_with("plan.factory_construction_build@1.0+"),
        "plan_id 派生形：{}",
        body.plan_id
    );
    assert_eq!(body.compiled_from.graph_version, "1.0");

    // 入口/输入输出面：build 单节点、construction_task_input → deliverable。
    assert_eq!(body.graph.entry, "build");
    assert_eq!(body.graph.nodes.len(), 1, "build 单节点");
    assert_eq!(body.interface.input_type, "construction_task_input");
    assert_eq!(body.interface.output_type, "deliverable");
    assert_eq!(body.interface.output_from, "build");

    // node_exec 投影：单节点 build，deliverable 端口 is_final 且带类型。
    assert_eq!(body.node_exec.len(), 1);
    let node = &body.node_exec[0];
    assert_eq!(node.node_id, "build");
    assert!(node.components.model_route.is_some(), "uses.model 已绑定");
    assert_eq!(node.artifact_contract.len(), 1);
    assert_eq!(node.artifact_contract[0].name, "deliverable");
    assert!(node.artifact_contract[0].is_final);
    assert_eq!(
        node.artifact_contract[0].r#type.as_deref(),
        Some("deliverable")
    );

    // 预算/停止/失败账随包进计划（提交后不可经重放放宽——W2-C55 语义面）。
    assert!(
        body.budget
            .budgets
            .iter()
            .any(|b| b.id == "cost_cap" && b.cap == 300),
        "cost_cap 随包：{:?}",
        body.budget.budgets
    );
    assert_eq!(body.budget.stop.len(), 2);
    assert_eq!(body.budget.failures.len(), 3);

    // 信封 decode 回环 = 生成产物（提交面消费契约）。
    let json = serde_json::to_string(&plan).unwrap();
    let decoded = decode_plan(&json).expect("decode_plan 应接受编译产物");
    assert_eq!(decoded, plan);
}

#[test]
fn construction_input_type_requires_no_swe_fields() {
    let plan = construction_plan();
    let schema = plan
        .body
        .types
        .get("construction_task_input")
        .expect("类型词典应带 construction_task_input");
    let required = schema["required"]
        .as_array()
        .expect("schema 应有 required 面")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    // 必填面 = 通用字段两项（缺口③编译面：不手填 SWE 字段）。
    assert_eq!(
        required,
        vec!["task_id".to_string(), "task_book".to_string()]
    );
    for swe_field in ["instance_id", "repo", "problem_statement"] {
        assert!(
            !required.iter().any(|r| r == swe_field),
            "SWE 字段 {swe_field} 不得在通用建设输入的必填面"
        );
    }
    // image/test_command 声明为可选（执行器容器面与 check 节点判据的缺省补位）。
    let props = schema["properties"]
        .as_object()
        .expect("schema 应有 properties");
    assert!(props.contains_key("image"));
    assert!(props.contains_key("test_command"));

    // 最小通用任务包（仅通用字段）满足编译产物的输入 schema 必填面。
    let minimal = serde_json::json!({
        "task_id": "E2G-demo",
        "task_book": "建 tools/factory-stat.py……",
    });
    for key in &required {
        assert!(minimal.get(key).is_some(), "最小任务包应携带必填键 {key}");
    }
}

#[test]
fn construction_build_compilation_is_bitwise_deterministic() {
    let doc = construction_doc();
    let registry = ComponentRegistry::load(repo_packages_root()).unwrap();
    let first = plan_json_pretty(&doc, &registry).unwrap();
    let second = plan_json_pretty(&doc, &registry).unwrap();
    assert_eq!(first, second, "同一输入两次全链编译应逐位一致");

    // CLI 入口（af-compiler-plan 的库内同体）产物 = 步进式产物。
    let ir = lower(&doc).unwrap();
    let report = bind(&ir, &registry);
    let plan = generate_plan(&ir, &report).unwrap();
    assert_eq!(serde_json::to_string_pretty(&plan).unwrap(), first);
}
