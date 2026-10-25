//! G4 研究报告包 v1 包面验收（与包文件同 commit 落盘——裁决§二 TDD 语义）。
//!
//! 验收面：
//! - af-dsl 图清单：reportkit 第 5 包、pkg.reportkit.paper_report@1.0 在册；
//! - 组件注册表：63 条终态（57→63，+6 件逐名/类/effect 断言）；
//! - 图正本干跑（真编译器全链 in-test）：exit 0、reader sources 工件口
//!   进 artifact_contract（裁决5 记账面）、cite_check/rubric_check unit 绑定；
//! - prompts/MANIFEST（裁决10）与 citations 五字段 required（裁决4）。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use af_compiler::ir::EffectClass;
use af_compiler::{ComponentClass, ComponentRegistry};
use af_execline::pkg::prompts;

fn repo_packages() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../af/packages")
}

#[test]
fn reportkit_is_fifth_package_with_paper_report_graph() {
    let reg = af_dsl::registry::Registry::load(repo_packages())
        .expect("仓库正本 af/packages 应装载（5 包）");
    assert_eq!(reg.package_count(), 5);
    assert!(
        reg.declared_graphs()
            .contains(&"pkg.reportkit.paper_report@1.0".to_string()),
        "reportkit 图应在册"
    );
}

#[test]
fn registry_reaches_g4_terminal_sixty_three() {
    let reg = ComponentRegistry::load(repo_packages()).unwrap();
    assert_eq!(reg.component_count(), 63, "57 + reportkit 6（G4 终态）");
    // +6 件逐名/类/effect（设计 §3 全文照抄面）。
    let expect = [
        (
            "pkg.report.web_search",
            ComponentClass::Tool,
            Some(EffectClass::ExternalOnce),
        ),
        (
            "pkg.report.web_fetch",
            ComponentClass::Tool,
            Some(EffectClass::ExternalOnce),
        ),
        (
            "pkg.report.pdf_extract",
            ComponentClass::Tool,
            Some(EffectClass::Pure),
        ),
        ("pkg.report.citation_unit", ComponentClass::Unit, None),
        ("pkg.report.rubric_unit", ComponentClass::Unit, None),
        ("pkg.report.mem.notes", ComponentClass::Memory, None),
    ];
    for (name, class, effect) in expect {
        let versions = reg
            .versions_of(name)
            .unwrap_or_else(|| panic!("{name} 应已注册"));
        assert!(versions.contains_key("1"), "{name}@1");
        let decl = &versions["1"];
        assert_eq!(decl.class, class, "{name}");
        assert_eq!(decl.effect_class, effect, "{name} effect_class");
        assert!(decl.impl_ref.is_some(), "{name} impl_ref 在场");
    }
}

#[test]
fn paper_report_graph_compiles_with_sources_artifact_port() {
    // 真编译器全链干跑（判据 1：af-compiler-plan 同链 in-test）。
    let src =
        std::fs::read_to_string(repo_packages().join("reportkit/graphs/paper_report@1.0.afb"))
            .expect("图正本应在盘（设计附录 A + reader sources 工件口）");
    let doc = af_dsl::load_str(&src).expect("图应装载");
    let verdict = af_dsl::validate(&doc);
    assert!(verdict.is_ok(), "图应零违例：{verdict}");
    let reg = ComponentRegistry::load(repo_packages()).unwrap();
    let plan_src = af_compiler::plan_json(&doc, &reg).expect("图应产计划（63 条 digest）");
    // 双跑确定性。
    let plan_src2 = af_compiler::plan_json(&doc, &reg).unwrap();
    assert_eq!(plan_src, plan_src2, "双跑逐字节一致");
    let plan: serde_json::Value = serde_json::from_str(&plan_src).unwrap();
    assert!(
        plan["body"]["plan_id"]
            .as_str()
            .unwrap()
            .starts_with("plan.paper_report@1.0+"),
        "{}",
        plan["body"]["plan_id"]
    );
    assert_eq!(
        plan["body"]["compiled_from"]["registry_digest"],
        "63c3478e9321e1c58318620a592e9953ef34cee3a5d19ea4c3300a722fafe15b",
        "G4 终态 digest（63 条）"
    );
    // 裁决5：reader sources 工件口进 artifact_contract（进图记账、不进 store）。
    let reader = plan["body"]["node_exec"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["node_id"] == "study.reader")
        .expect("study.reader 应有投影");
    let artifacts: BTreeSet<&str> = reader["artifact_contract"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    assert!(
        artifacts.contains("sources"),
        "reader sources 工件契约：{artifacts:?}"
    );
    // 双验证器 unit 绑定（裁决4 面的图侧）。
    let cite = plan["body"]["node_exec"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["node_id"] == "study.cite_check")
        .unwrap();
    assert_eq!(
        cite["unit"]["unit_id"], "pkg.report.citation_unit",
        "cite_check 绑定 citation_unit"
    );
}

#[test]
fn reportkit_prompts_resolve_via_unified_mechanism() {
    let root = repo_packages();
    for r in [
        "pkg.reportkit.reader_system@1",
        "pkg.reportkit.writer_system",
    ] {
        let asset = prompts::resolve_template(&root, r)
            .unwrap_or_else(|e| panic!("{r} 应可解析（裁决10 统一机制）: {e}"));
        assert!(!asset.content.is_empty());
    }
    // 版本不符拒（EX4003）——与 sweng 面同一解析器一套。
    let err = prompts::resolve_template(&root, "pkg.reportkit.reader_system@2").unwrap_err();
    assert!(err.to_string().starts_with("EX4003"), "{err}");
}

#[test]
fn citations_type_carries_five_required_fields() {
    // 裁决4：snapshot_ref 必入 required——quote_hit/snapshot_present 可执行
    // 的地基。从图正本 types 段直接断言（schema 单一正本面之一）。
    // 计划 types 段承载图内 schema（IR 原样投影）——从计划面断言单一正本。
    let src =
        std::fs::read_to_string(repo_packages().join("reportkit/graphs/paper_report@1.0.afb"))
            .unwrap();
    let doc = af_dsl::load_str(&src).unwrap();
    let reg = ComponentRegistry::load(repo_packages()).unwrap();
    let plan: serde_json::Value =
        serde_json::from_str(&af_compiler::plan_json(&doc, &reg).unwrap()).unwrap();
    let rr = &plan["body"]["types"]["research_report"];
    assert!(rr.is_object(), "research_report 类型入计划 types：{rr}");
    // 计划 types 段存 schema 本体（非 {name, schema} 包裹）。
    let citations = &rr["properties"]["citations"]["items"];
    let required = citations["required"].as_array().unwrap();
    let set: BTreeSet<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(
        set,
        BTreeSet::from(["n", "url", "quote", "snapshot_sha256", "snapshot_ref"]),
        "citations 五字段 required（裁决4）"
    );
}
