//! P2-L12 注册行为测试：G3 全链闸门 §三.1 缺组件处置的声明式注册
//! （G2G3-放行裁决：50 个未注册组件名补声明，**声明非实现**）。
//!
//! 对照面 = 仓库正本 `af/packages`（闸门 CLI 注入的同一注册表）。
//! 覆盖清单来源：`G3-全链闸门-L1L4-20261024.md` §⑤——15 稿 `uses.*` 引用
//! 103 处、命中 46、缺 57（E9001 全量）、去重 50 名（model_route 5 /
//! tool 28 / unit 13 / memory 4）；类归属经 15 份草稿 uses 面逐名核实
//! （`uses.tool` 与 `uses.tools` 同归 tool 类——bind.rs §四面）。
//!
//! TDD：本文件先红（声明未写时 50 名皆不可达 / 计数 7≠57 / E9003 探针
//! 名未注册走 E9001 而非 E9003），声明落盘后全绿。

use af_compiler::bind::BindCode;
use af_compiler::components::{ComponentClass, ComponentRegistry};

/// 仓库正本包根（与 bind_test.rs 同口径）。
fn repo_packages_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../af/packages")
}

fn repo_registry() -> ComponentRegistry {
    ComponentRegistry::load(repo_packages_root())
        .unwrap_or_else(|e| panic!("仓库正本 af/packages 组件清单应装载：{e}"))
}

/// P2-L12 扩容 50 名（名, 版本, 类）——闸门 §⑤ 清款逐条登记。
/// 版本对齐草稿引用串（`@1` 或未版本化引用的单版本 `"1"`）。
const DECLS_50: &[(&str, &str, ComponentClass)] = &[
    // ---- model_route ×5（根级，uses.model 面）----
    ("route.plan", "1", ComponentClass::ModelRoute),
    ("route.manager", "1", ComponentClass::ModelRoute),
    ("route.alt", "1", ComponentClass::ModelRoute),
    ("route.judge", "1", ComponentClass::ModelRoute),
    ("route.reflect", "1", ComponentClass::ModelRoute),
    // ---- tool ×28（uses.tool / uses.tools 面）----
    ("pkg.adaskit.task_loader", "1", ComponentClass::Tool),
    ("pkg.aflowkit.task_loader", "1", ComponentClass::Tool),
    ("pkg.agensh.inspect", "1", ComponentClass::Tool),
    ("pkg.agensh.local_edit", "1", ComponentClass::Tool),
    ("pkg.agensh.local_run", "1", ComponentClass::Tool),
    ("pkg.agensh.publish_update", "1", ComponentClass::Tool),
    ("pkg.agensh.board_append", "1", ComponentClass::Tool),
    ("pkg.agensh.workspace_commit", "1", ComponentClass::Tool),
    ("pkg.autogen.tool.search", "1", ComponentClass::Tool),
    ("pkg.autogen.tool.calc", "1", ComponentClass::Tool),
    ("pkg.crewkit.kickoff_inputs", "1", ComponentClass::Tool),
    ("pkg.crewkit.tool.web_search", "1", ComponentClass::Tool),
    ("pkg.crewkit.tool.fetch", "1", ComponentClass::Tool),
    ("pkg.crewkit.tool.doc_render", "1", ComponentClass::Tool),
    ("pkg.debatekit.search", "1", ComponentClass::Tool),
    ("pkg.reactkit.hopqa_api", "1", ComponentClass::Tool),
    ("pkg.reactkit.lookup", "1", ComponentClass::Tool),
    ("pkg.reflexkit.env_exec", "1", ComponentClass::Tool),
    ("pkg.reflexkit.search", "1", ComponentClass::Tool),
    ("pkg.sweagent.aci.search", "1", ComponentClass::Tool),
    ("pkg.sweagent.aci.open", "1", ComponentClass::Tool),
    ("pkg.sweagent.aci.edit", "1", ComponentClass::Tool),
    ("pkg.sweagent.aci.create", "1", ComponentClass::Tool),
    ("pkg.sweagent.aci.run_test", "1", ComponentClass::Tool),
    ("pkg.sweagent.aci.submit", "1", ComponentClass::Tool),
    ("pkg.sweagent.env", "1", ComponentClass::Tool),
    ("pkg.openworld.env", "1", ComponentClass::Tool),
    ("pkg.sweng.publish_task_finished", "1", ComponentClass::Tool),
    // ---- unit ×13（uses.unit 面）----
    ("pkg.adaskit.verifier", "1", ComponentClass::Unit),
    ("pkg.aflowkit.evaluate", "1", ComponentClass::Unit),
    ("pkg.agensh.acceptance_gate", "1", ComponentClass::Unit),
    ("pkg.agensh.fail_scan", "1", ComponentClass::Unit),
    ("pkg.autogen.team.stall_unit", "1", ComponentClass::Unit),
    ("pkg.autogen.team.term_unit", "1", ComponentClass::Unit),
    ("pkg.camelkit.social_eval", "1", ComponentClass::Unit),
    (
        "pkg.crewkit.guardrail.structural",
        "1",
        ComponentClass::Unit,
    ),
    ("pkg.langgraph.verify", "1", ComponentClass::Unit),
    ("pkg.reflexkit.evaluator", "1", ComponentClass::Unit),
    ("pkg.sweagent.aci_schema", "1", ComponentClass::Unit),
    ("pkg.sweagent.human_gate", "1", ComponentClass::Unit),
    ("pkg.sweagent.swebench_eval", "1", ComponentClass::Unit),
    // ---- memory ×4（uses.memory 面）----
    ("pkg.autogen.mem.list", "1", ComponentClass::Memory),
    ("pkg.crewkit.mem.longterm", "1", ComponentClass::Memory),
    ("pkg.crewkit.mem.entity", "1", ComponentClass::Memory),
    ("pkg.debatekit.belief", "1", ComponentClass::Memory),
];

/// 50 名按类计数（闸门 §⑤ 聚合行的直接断言）。
#[test]
fn decls_table_matches_gate_coverage_counts() {
    let count = |want: ComponentClass| DECLS_50.iter().filter(|(_, _, c)| *c == want).count();
    // 50 名表是 P2-L12 历史闭包（类计数不随 G4 增量变——G4 +6 件的注册表
    // 总量在 repo_registry_total_is_57_after_p2l12_expansion 断言 63）。
    assert_eq!(count(ComponentClass::ModelRoute), 5);
    assert_eq!(count(ComponentClass::Tool), 28);
    assert_eq!(count(ComponentClass::Unit), 13);
    assert_eq!(count(ComponentClass::Memory), 4);
    assert_eq!(DECLS_50.len(), 50, "恰好 50 名，无重复漏项");
    let names: std::collections::BTreeSet<&str> = DECLS_50.iter().map(|(n, _, _)| *n).collect();
    assert_eq!(names.len(), 50, "名字唯一");
}

/// 声明可解析：50 名全部在正本注册表按预期版本+类可达（绑定层可解析）。
#[test]
fn all_50_p2l12_declarations_resolve_with_expected_version_and_class() {
    let reg = repo_registry();
    for (name, version, class) in DECLS_50 {
        let versions = reg
            .versions_of(name)
            .unwrap_or_else(|| panic!("{name} 应已注册（P2-L12 声明）"));
        let decl = versions.get(*version).unwrap_or_else(|| {
            panic!(
                "{name}@{version} 应已声明（已声明版本：{:?}）",
                versions.keys()
            )
        });
        assert_eq!(decl.class, *class, "{name}@{version} 类应为 {class:?}");
        assert_eq!(decl.name, *name);
        assert_eq!(decl.version, *version);
        // 声明形态：model_route 带端点（四元组可装载面）、tool 带 effect_class
        //（ADR-010 回填源）——装载静态校验的不变量在真实声明上复核。
        match class {
            ComponentClass::ModelRoute => assert!(
                decl.endpoint.is_some(),
                "{name}（model_route）声明应带 endpoint"
            ),
            ComponentClass::Tool => assert!(
                decl.effect_class.is_some(),
                "{name}（tool）声明应带 effect_class"
            ),
            _ => {}
        }
    }
}

/// 注册表总量：P2-L11 首批 7 + P2-L12 扩容 50 + G4 reportkit 6 = 63 条声明。
/// （测试名保留 P2-L12 历史口径——中立性预期红名单按名引用，不随计数演进
/// 改名；计数同步与包落地同 commit——裁决§二。）
#[test]
fn repo_registry_total_is_57_after_p2l12_expansion() {
    let reg = repo_registry();
    assert_eq!(
        reg.component_count(),
        63,
        "7（P2-L11 首批）+ 50（P2-L12 扩容）+ 6（G4 reportkit）"
    );
}

/// 未版本化草稿引用的 7 名恰好单版本声明（bind 解析不歧义——闸门 §③
/// ReAct 行「注册须恰好单版本，否则触发歧义拒」的直接对照）。
#[test]
fn unversioned_referenced_names_declare_exactly_one_version() {
    let reg = repo_registry();
    for name in [
        "pkg.agensh.inspect",
        "pkg.agensh.local_edit",
        "pkg.agensh.local_run",
        "pkg.agensh.publish_update",
        "pkg.reactkit.hopqa_api",
        "pkg.reactkit.lookup",
        "pkg.sweng.publish_task_finished",
    ] {
        let versions = reg
            .versions_of(name)
            .unwrap_or_else(|| panic!("{name} 应已注册"));
        assert_eq!(
            versions.len(),
            1,
            "{name} 未版本化引用须恰好单版本（当前 {:?}）",
            versions.keys()
        );
    }
}

/// 类不匹配拒绝：真实注册名（pkg.adaskit.verifier，unit 类）被 uses.tools
/// 面引用 → E9003（名+版本已注册、类别不符——注册正确不等于免检）。
#[test]
fn class_mismatch_against_repo_registry_is_rejected() {
    let src = r#"version: 1
meta:
  name: mismatch_probe
  graph_version: "1.0"
  entry: worker
  input: probe_input
  output: { ref: answer, from: worker }
  intent: "P2-L12 类不匹配探针"
types:
  - name: probe_input
    schema: { type: object, required: [x], properties: { x: { type: string } } }
  - name: answer
    schema: { type: object, required: [y], properties: { y: { type: string } } }
nodes:
  - id: worker
    kind: task
    uses: { model: route.main@1, tools: [pkg.adaskit.verifier@1] }
    emits: { answer: answer }
edges: []
policies:
  budgets:
    - { id: cap, dimension: cents, scope: plan, cap: 100 }
"#;
    let doc = af_dsl::load_str(src).unwrap_or_else(|e| panic!("探针装载失败：{e}"));
    assert!(af_dsl::validate(&doc).is_ok());
    let ir = af_compiler::lower(&doc).unwrap();
    let report = af_compiler::bind(&ir, &repo_registry());
    assert!(
        report
            .violations
            .iter()
            .any(|v| v.code == BindCode::ClassMismatch
                && v.node == "worker"
                && v.reference == "pkg.adaskit.verifier@1"),
        "unit 声明被 uses.tools 引用应 E9003：{:?}",
        report.violations
    );
    assert!(
        !report
            .violations
            .iter()
            .any(|v| v.code == BindCode::RefMissing),
        "名已注册，不应再报 E9001：{:?}",
        report.violations
    );
}
