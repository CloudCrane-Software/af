//! 包图清单 registry 装载器验收测试（P2-L09，失败分类账 #1-4 缺组件终裁）：
//! ① manifest 规范（JSON，serde deny_unknown_fields + 静态校验）；
//! ③ registry 装载器：扫 `af/packages/*/manifest.json` → 版本化图引用集
//!    （`pkg.{package}.{name}@{version}`）→ 注入 `ValidateOptions.declared_graphs`；
//! ④ 仓库正本 af/packages 四示例包必须覆盖 4 份被拒草稿（ADAS / AutoGen-
//!    Team-JSON / GPTSwarm / MetaGPT）引用的全部候选图；
//! 闭环正反例：不注入（默认空集）→ E2001 全拒；注入后同文档 0 违例。
//! 引用清单逐字取自 对齐草稿/ 4 份 YAML 的 topology_selector（use+fallback），
//! 与 ~/.cache/afb-g3gate/drafts-run-fix1.txt 的 E2001 明细一致。

use af_dsl::registry::{PackageManifest, Registry, RegistryError};
use af_dsl::{load_str, validate, validate_with, ValidateOptions};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn fixture(p: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(p)
}

fn repo_packages() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../af/packages")
}

// ---------- ① manifest 解析 + 静态校验 ----------

const GOOD_MANIFEST: &str = r#"{
  "package": "alpha",
  "graphs": [
    { "name": "g1", "version": "1", "source": "graphs/g1@1.afb" },
    { "name": "core.best", "version": "2.0", "source": "graphs/core/best@2.0.afb" }
  ]
}"#;

#[test]
fn parse_valid_manifest_composes_versioned_refs() {
    let m = PackageManifest::parse(GOOD_MANIFEST).expect("合法 manifest 应解析");
    assert_eq!(m.package, "alpha");
    assert_eq!(
        m.graph_refs(),
        vec![
            "pkg.alpha.g1@1".to_string(),
            "pkg.alpha.core.best@2.0".to_string(),
        ],
        "组合规则：pkg.{{package}}.{{name}}@{{version}}（点号局部名合法）"
    );
}

#[test]
fn parse_rejects_unknown_field() {
    let src = r#"{ "package": "a", "graphs": [ { "name": "g", "version": "1", "source": "s", "extra": 1 } ] }"#;
    let e = PackageManifest::parse(src).expect_err("未知字段应拒");
    assert!(
        matches!(e, RegistryError::ManifestInvalid { .. }),
        "got: {e}"
    );
    assert!(e.to_string().contains("unknown field"), "got: {e}");
}

#[test]
fn parse_rejects_missing_source_field() {
    let src = r#"{ "package": "a", "graphs": [ { "name": "g", "version": "1" } ] }"#;
    let e = PackageManifest::parse(src).expect_err("来源引用 source 必填");
    assert!(matches!(e, RegistryError::ManifestInvalid { .. }));
    assert!(e.to_string().contains("missing field"), "got: {e}");
}

#[test]
fn parse_rejects_empty_package_name() {
    let src = r#"{ "package": "", "graphs": [ { "name": "g", "version": "1", "source": "s" } ] }"#;
    let e = PackageManifest::parse(src).expect_err("空包名应拒");
    assert!(e.to_string().contains("package"), "got: {e}");
}

#[test]
fn parse_rejects_at_sign_in_name_or_version() {
    // 组合面以 '@' 分隔 name@version——字段内再含 '@' 会使组合引用歧义，静态拒。
    for bad in [
        r#"{ "package": "a", "graphs": [ { "name": "g@1", "version": "1", "source": "s" } ] }"#,
        r#"{ "package": "a", "graphs": [ { "name": "g", "version": "1@x", "source": "s" } ] }"#,
        r#"{ "package": "a@b", "graphs": [ { "name": "g", "version": "1", "source": "s" } ] }"#,
    ] {
        let e = PackageManifest::parse(bad).expect_err("字段含 '@' 应拒");
        assert!(e.to_string().contains('@'), "got: {e}");
    }
}

#[test]
fn parse_rejects_empty_graph_fields() {
    for bad in [
        r#"{ "package": "a", "graphs": [ { "name": "", "version": "1", "source": "s" } ] }"#,
        r#"{ "package": "a", "graphs": [ { "name": "g", "version": "", "source": "s" } ] }"#,
        r#"{ "package": "a", "graphs": [ { "name": "g", "version": "1", "source": "" } ] }"#,
    ] {
        assert!(PackageManifest::parse(bad).is_err(), "应拒：{bad}");
    }
}

#[test]
fn parse_rejects_empty_graphs_list() {
    let src = r#"{ "package": "a", "graphs": [] }"#;
    let e = PackageManifest::parse(src).expect_err("空图清单应拒（manifest 唯一职责=声明图）");
    assert!(e.to_string().contains("graphs"), "got: {e}");
}

#[test]
fn parse_rejects_duplicate_ref_within_manifest() {
    let src = r#"{ "package": "a", "graphs": [
        { "name": "g", "version": "1", "source": "s1" },
        { "name": "g", "version": "1", "source": "s2" }
    ] }"#;
    let e = PackageManifest::parse(src).expect_err("同清单重复图引用应拒");
    assert!(e.to_string().contains("pkg.a.g@1"), "got: {e}");
}

// ---------- ③ registry 目录扫描 ----------

#[test]
fn scan_builds_sorted_ref_set_from_fixture_packages() {
    let r = Registry::load(fixture("packages")).expect("fixture 应装载");
    let refs: Vec<&str> = r.declared_graphs().iter().map(|s| s.as_str()).collect();
    assert_eq!(
        refs,
        vec![
            "pkg.alpha.core.best@2.0",
            "pkg.alpha.g1@1",
            "pkg.beta.router@1"
        ],
        "BTreeSet 确定性排序；跨包合并"
    );
    assert_eq!(r.package_count(), 2);
}

#[test]
fn scan_missing_root_is_error() {
    let e = Registry::load(fixture("packages-nonexistent")).expect_err("缺根应拒");
    assert!(matches!(e, RegistryError::RootMissing(_)), "got: {e}");
}

#[test]
fn scan_root_not_dir_is_error() {
    // 指向一个文件当根 → RootNotDir（不是静默空集）。
    let e = Registry::load(fixture("packages/alpha/manifest.json")).expect_err("文件当根应拒");
    assert!(matches!(e, RegistryError::RootNotDir(_)), "got: {e}");
}

#[test]
fn scan_package_dir_without_manifest_is_error() {
    let e = Registry::load(fixture("packages-bad-nomanifest")).expect_err("缺 manifest 应拒");
    assert!(matches!(e, RegistryError::ManifestMissing(_)), "got: {e}");
    assert!(e.to_string().contains("stray"), "got: {e}");
}

#[test]
fn scan_invalid_manifest_is_error() {
    let e = Registry::load(fixture("packages-bad-manifest")).expect_err("非法 manifest 应拒");
    assert!(
        matches!(e, RegistryError::ManifestInvalid { .. }),
        "got: {e}"
    );
}

#[test]
fn scan_cross_package_duplicate_ref_is_error() {
    let e = Registry::load(fixture("packages-bad-dup")).expect_err("跨包重复应拒");
    match &e {
        RegistryError::DuplicateGraphRef { reference, .. } => {
            assert_eq!(reference, "pkg.d.x@1");
        }
        other => panic!("期望 DuplicateGraphRef，得到：{other}"),
    }
}

#[test]
fn scan_empty_root_yields_empty_registry_fail_closed() {
    // 空根 = 空集 = 一切候选引用仍拒（与不注入同效；fail-closed，不静默放行）。
    let r = Registry::load(fixture("packages-empty")).expect("空根合法（零包）");
    assert!(r.declared_graphs().is_empty());
    assert_eq!(r.package_count(), 0);
    assert!(r.options().declared_graphs.is_empty());
}

#[test]
fn scan_skips_non_package_files() {
    // 目录即包候选：非目录条目（.gitkeep/README.md）与隐藏目录不是包，跳过不报错。
    let r = Registry::load(fixture("packages-empty")).expect("含 .gitkeep 的空根应装载");
    assert_eq!(r.package_count(), 0);
}

// ---------- 闭环：注入后 E2001 消失（正反例） ----------

fn selector_doc(use_g: &str, fallback: &str) -> String {
    format!(
        "version: 1\n\
meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: {{ ref: a, from: n1 }}\n
  topology_selector:\n    rules:\n      - {{ when: \"input.q == 'x'\", use: {use_g} }}\n    fallback: {fallback}\n\
types:\n  - {{ name: in_t, schema: {{ type: object, required: [q], properties: {{ q: {{ type: string }} }} }} }}\n  - {{ name: a, schema: {{ type: object }} }}\n\
nodes:\n  - {{ id: n1, kind: task, uses: {{ model: route.main@1 }}, emits: {{ answer: a }} }}\n\
edges: []\npolicies: {{ budgets: [], stop: [], failures: [] }}\n"
    )
}

#[test]
fn injection_closes_e2001_but_default_still_rejects() {
    let src = selector_doc("pkg.alpha.g1@1", "pkg.beta.router@1");
    let doc = load_str(&src).expect("草稿形应解析");

    // 反例：不注入（默认空集）→ use+fallback 双 E2001（现状=全拒，无隐式放行）。
    assert_eq!(validate(&doc).codes(), ["E2001", "E2001"]);

    // 正例：装载 registry 注入 → 同一文档 0 违例。
    let r = Registry::load(fixture("packages")).expect("fixture 应装载");
    let rep = validate_with(&doc, &r.options());
    assert!(rep.is_ok(), "注入后应全绿：\n{rep}");
}

#[test]
fn registry_options_only_touches_declared_graphs() {
    // 注入面=ValidateOptions 的 declared_graphs 字段；其余可配置项维持默认。
    let r = Registry::load(fixture("packages")).unwrap();
    let opts = r.options();
    assert_eq!(opts.context_quota_cap, None);
    assert_eq!(
        opts.declared_graphs,
        r.declared_graphs().clone(),
        "options() 是纯注入，不改其他项"
    );
    // 反向：ValidateOptions::default() 仍空集（不变式：默认=全拒）。
    assert!(ValidateOptions::default().declared_graphs.is_empty());
}

// ---------- ④ 仓库正本 af/packages：覆盖 4 份被拒草稿的全部候选图 ----------

/// 4 份草稿 topology_selector（use+fallback 去重）引用的全部候选图，逐字
/// 取自 对齐草稿/{ADAS,AutoGen-Team-JSON,GPTSwarm,MetaGPT}.yaml。
const DRAFT_REFS: &[&str] = &[
    // ADAS.yaml:12-14
    "pkg.adaskit.discovered_maze@1",
    "pkg.adaskit.discovered_wordle@1",
    "pkg.adaskit.mas_search@1",
    // AutoGen-Team-JSON.yaml:14-19（fallback selector 与 rules[1] 同名，去重）
    "pkg.autogen.team.round_robin@1.0",
    "pkg.autogen.team.selector@1.0",
    "pkg.autogen.team.magentic_one@1.0",
    "pkg.autogen.team.swarm@1.0",
    "pkg.autogen.team.graph_flow@1.0",
    // GPTSwarm.yaml:11-13
    "pkg.swarmkit.graph.plan_execute_review@2.1",
    "pkg.swarmkit.graph.fanout_vote@1.4",
    "pkg.swarmkit.graph.react_baseline@1.0",
    // MetaGPT.yaml:23-25
    "pkg.sweng.sop_full@1.0",
    "pkg.sweng.sop_no_qa@1.0",
    "pkg.sweng.sop_minimal@1.0",
];

/// G4 任务图白名单（裁决§二：14 图闭包测试扩展为「草稿引用 ∪ G4 任务图
/// 白名单」常量形式——注册表卫生语义保留：清单仍不得夹带闭包外图，只是
/// 闭包定义从「草稿引用」扩为「草稿引用 ∪ G4 落地任务图」）。Phase 1 =
/// sweng fix_swe_task；Phase 2 reportkit paper_report 随其包落地同 commit
/// 追加于此。
const G4_TASK_GRAPH_WHITELIST: &[&str] = &[
    // sweng/manifest.json:9（G4 软件工程包 E2E 图，设计 §4.2）
    "pkg.sweng.fix_swe_task@1.0",
];

#[test]
fn repo_packages_registry_covers_all_four_draft_refs() {
    let r = Registry::load(repo_packages()).expect("仓库正本 af/packages 应装载（4 示例包）");
    assert_eq!(r.package_count(), 4, "adaskit/autogen/swarmkit/sweng");
    for rf in DRAFT_REFS {
        assert!(
            r.declared_graphs().contains(*rf),
            "草稿候选图 {rf} 未被 af/packages 声明——失败分类账 #1-4 无法闭环"
        );
    }
}

#[test]
fn repo_packages_registry_strictly_matches_draft_refs() {
    // 清单闭包（G4 裁决§二扩展）：草稿引用 ∪ G4 任务图白名单——除两者外
    // 不夹带未引用图（注册表卫生语义保留；计数同步与包落地同 commit）。
    let r = Registry::load(repo_packages()).unwrap();
    let expected: BTreeSet<&str> = DRAFT_REFS
        .iter()
        .chain(G4_TASK_GRAPH_WHITELIST)
        .copied()
        .collect();
    let declared: BTreeSet<&str> = r.declared_graphs().iter().map(|s| s.as_str()).collect();
    assert_eq!(
        declared,
        expected,
        "清单应恰为草稿闭包（14 条）∪ G4 任务图白名单（{} 条）",
        G4_TASK_GRAPH_WHITELIST.len()
    );
    assert_eq!(declared.len(), 14 + G4_TASK_GRAPH_WHITELIST.len());
}
