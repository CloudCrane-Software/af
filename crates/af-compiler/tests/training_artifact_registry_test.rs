//! 训练轨演化产物的注册回注接口——机器校验（wave-02 卡
//! `s17-training-artifact-registry`，债号 D-56；设计稿
//! `30-工件/strategies17-供给面设计-20261106.md` §七.3）。
//!
//! 三层正本：
//! - 映射规约（prose 正本）：`af/packages/training-artifact-registry.md`；
//! - 登记册（数据正本）：`af/packages/training-artifacts.json`；
//! - 本文件（可执行正本）：R1–R6 逐条断言 + 正反两向夹具——登记册为空集
//!   （训练轨未派单，depends_on 未满足）时盘面对话空转通过，规则正确性由
//!   仓库正本夹具与合成注册表夹具（tests/fixtures/training-artifact-registry/）
//!   证明，不 vacuous。
//!
//! fail-closed 语义：映射条目指向的声明必须已在 `af/packages` 组件清单注册
//! （R3/R4）；`impl_ref` 必须与声明逐字相等且为执行面 builtin 形态、其目标
//! 组件名自身已注册（R5——锚已接线实现，禁 P2-L12 式预留包路径）。

use af_compiler::components::{ComponentClass, ComponentRegistry};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// 登记册 schema 钉（升版须同步本常量与规约文档 §二）。
const REGISTER_SCHEMA: &str = "af.training-artifact-registry/1";

/// 仓库正本包根（与 `registry_decl_test.rs` / `bind_test.rs` 同口径）。
fn repo_packages_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../af/packages")
}

/// 合成注册表夹具根（负例专用：盘上无对应形态的声明可作 R5 各分支靶点）。
fn fixture_registry_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/training-artifact-registry")
}

fn repo_registry() -> ComponentRegistry {
    ComponentRegistry::load(repo_packages_root())
        .unwrap_or_else(|e| panic!("仓库正本 af/packages 组件清单应装载：{e}"))
}

fn fixture_registry() -> ComponentRegistry {
    let root = fixture_registry_root();
    ComponentRegistry::load(&root).unwrap_or_else(|e| panic!("夹具注册表应装载 {root:?}：{e}"))
}

/// 产物形态闭集（R2）：训练轨只演化工具面与判定单元两类产物。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ArtifactKind {
    /// 工具面演化产物 → class = tool（DSL `uses.tools` 面）。
    ToolWrapper,
    /// 判定单元演化产物 → class = unit（DSL `uses.unit` 面）。
    UnitJudge,
}

impl ArtifactKind {
    /// R2：形态 → 期望组件类（册内不独立存 class，由 kind 推导后与声明核对）。
    fn expected_class(self) -> ComponentClass {
        match self {
            Self::ToolWrapper => ComponentClass::Tool,
            Self::UnitJudge => ComponentClass::Unit,
        }
    }
}

/// 登记册单条映射（全文 `deny_unknown_fields`——未知字段即拒，fail-closed）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrainingArtifact {
    /// 产物 id：训练轨全限定名（`<轨>/<run>/<形态>/<名>` 形），不含 `@`。
    artifact: String,
    /// 产物形态（闭集）。
    kind: ArtifactKind,
    /// 回注目标声明名（`uses.*` 引用串去 `@版本` 段）。
    declaration: String,
    /// 回注目标声明版本。
    version: String,
    /// 实现定位串——必须与声明的 `impl_ref` 逐字相等（R5）。
    impl_ref: String,
    /// 读数定位串（poc 证据目录 / 训练轨读数坐标；必填非空，R6）。
    evidence: String,
}

/// 登记册文件正本（data 面：`note` 自述用途，装载面只校验不消费）。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisterFile {
    schema: String,
    #[serde(default)]
    note: Option<String>,
    artifacts: Vec<TrainingArtifact>,
}

/// 读登记册 JSON 正本（路径由调用方给根）。
fn read_register(root: &Path) -> RegisterFile {
    let path = root.join("training-artifacts.json");
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 {path:?}：{e}"));
    serde_json::from_str(&src).unwrap_or_else(|e| panic!("登记册 JSON 非法 {path:?}：{e}"))
}

fn repo_register() -> RegisterFile {
    read_register(&repo_packages_root())
}

/// R1–R6 单条校验（错误信息带规则号——负例夹具按子串断言）。
fn validate_entry(entry: &TrainingArtifact, reg: &ComponentRegistry) -> Result<(), String> {
    // R1 产物 id：非空、不含 `@`（`@` 为 name@version 组合引用的保留段）。
    if entry.artifact.is_empty() {
        return Err("R1 产物 id 为空".to_string());
    }
    if entry.artifact.contains('@') {
        return Err(format!("R1 产物 id 含保留段 '@'：{}", entry.artifact));
    }
    // R3 声明名：非空、不含 `@`（版本独立成字段）。
    if entry.declaration.is_empty() {
        return Err("R3 声明名为空".to_string());
    }
    if entry.declaration.contains('@') {
        return Err(format!("R3 声明名含 '@'：{}", entry.declaration));
    }
    // R3/R4 回注目标必须已注册、版本必须已声明（先落声明、后登记映射）。
    let versions = reg
        .versions_of(&entry.declaration)
        .ok_or_else(|| format!("R3 回注目标未注册：{}", entry.declaration))?;
    let decl = versions
        .get(&entry.version)
        .ok_or_else(|| format!("R4 版本未声明：{}@{}", entry.declaration, entry.version))?;
    // R2 形态 ↔ 声明类一致。
    let expected = entry.kind.expected_class();
    if decl.class != expected {
        return Err(format!(
            "R2 形态与声明类不符：{:?} 期望 {:?}，声明实为 {:?}",
            entry.kind, expected, decl.class
        ));
    }
    // R5 impl_ref：声明侧必填、册值与声明逐字相等、执行面 builtin 形态、
    // builtin 目标组件名自身已注册。
    let decl_impl = decl.impl_ref.as_deref().ok_or_else(|| {
        format!(
            "R5 声明无 impl_ref：{}@{}",
            entry.declaration, entry.version
        )
    })?;
    if entry.impl_ref != decl_impl {
        return Err(format!(
            "R5 impl_ref 与声明不一致：册 {} vs 声明 {}",
            entry.impl_ref, decl_impl
        ));
    }
    let builtin = decl_impl
        .strip_prefix("executor:builtin/")
        .ok_or_else(|| format!("R5 impl_ref 非执行面 builtin 形态：{decl_impl}"))?;
    let builtin_name = builtin
        .rsplit_once('@')
        .map(|(name, _crate)| name)
        .ok_or_else(|| format!("R5 impl_ref 缺 @crate 段：{decl_impl}"))?;
    if reg.versions_of(builtin_name).is_none() {
        return Err(format!("R5 builtin 目标未注册：{builtin_name}"));
    }
    // R6 读数定位串必填非空。
    if entry.evidence.is_empty() {
        return Err("R6 读数定位串为空".to_string());
    }
    Ok(())
}

/// R1 册内唯一性 + schema 钉 + 逐条 R1–R6。
fn validate_register(file: &RegisterFile, reg: &ComponentRegistry) -> Result<(), String> {
    if file.schema != REGISTER_SCHEMA {
        return Err(format!("schema 钉不符：{}", file.schema));
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for (i, entry) in file.artifacts.iter().enumerate() {
        if !seen.insert(entry.artifact.as_str()) {
            return Err(format!("R1 产物 id 册内重复：{}", entry.artifact));
        }
        validate_entry(entry, reg)
            .map_err(|e| format!("artifacts[{i}]（{}）：{e}", entry.artifact))?;
    }
    Ok(())
}

// ---------------------------------------------------------------
// 盘面正本（af/packages/training-artifacts.json）
// ---------------------------------------------------------------

/// 仓库正本登记册合规：schema 钉 + note 自述在场 + 逐条映射规则。
/// 空集时循环体空转（训练轨未派单如实登记）——规则由下述夹具测试证明。
#[test]
fn repo_register_conforms_to_mapping_rules() {
    let reg = repo_registry();
    let file = repo_register();
    validate_register(&file, &reg).unwrap_or_else(|e| panic!("登记册应合规：{e}"));
}

/// schema/note 钉：静默升版或丢失自述注记即红。
#[test]
fn repo_register_schema_and_note_are_pinned() {
    let file = repo_register();
    assert_eq!(file.schema, REGISTER_SCHEMA, "schema 钉不符");
    let note = file.note.expect("登记册应带 note 注记（自述正本用途）");
    assert!(!note.trim().is_empty(), "note 注记不得为空");
}

/// 装载对账：注册表装载计数 == 盘面 components.json 逐文件声明数之和——
/// 计数钉（`registry_decl_test.rs` / `bind_test.rs` 字面量）的独立读数面，
/// 防装载器静默漏文件；与字面量钉互不替代。
#[test]
fn repo_registry_loader_count_matches_manifest_files() {
    let reg = repo_registry();
    let root = repo_packages_root();
    let mut files: Vec<PathBuf> = Vec::new();
    let root_file = root.join("components.json");
    if root_file.is_file() {
        files.push(root_file);
    }
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("读包根 {root:?}：{e}"))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let f = dir.join("components.json");
        if f.is_file() {
            files.push(f);
        }
    }
    let mut on_disk = 0usize;
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap_or_else(|e| panic!("读 {f:?}：{e}"));
        #[derive(Deserialize)]
        struct CountFile {
            components: Vec<serde_json::Value>,
        }
        let parsed: CountFile =
            serde_json::from_str(&src).unwrap_or_else(|e| panic!("解析 {f:?}：{e}"));
        on_disk += parsed.components.len();
    }
    assert_eq!(
        on_disk,
        reg.component_count(),
        "装载计数应与盘面 components.json 声明数之和一致（{} 份清单）",
        files.len()
    );
}

// ---------------------------------------------------------------
// 夹具：仓库正本上的正反两向（R1/R2/R3/R4/R5-分歧/R6）
// ---------------------------------------------------------------

/// 正例夹具：锚真实在册声明 unit `pkg.sweng.test_unit@1`（P2-L11 首批，
/// impl_ref 即执行面 builtin 形态）。
fn ok_fixture() -> TrainingArtifact {
    TrainingArtifact {
        artifact: "harnessadapt-rl/run-001/unit_judge/rerank_gate".to_string(),
        kind: ArtifactKind::UnitJudge,
        declaration: "pkg.sweng.test_unit".to_string(),
        version: "1".to_string(),
        impl_ref: "executor:builtin/pkg.sweng.test_unit@af-execline".to_string(),
        evidence: "poc/poc-s17-training/readings.json".to_string(),
    }
}

#[test]
fn mapping_rules_accept_conforming_entries() {
    let reg = repo_registry();
    validate_entry(&ok_fixture(), &reg).expect("unit 形态正例应过 R1–R6");
    let tool = TrainingArtifact {
        artifact: "harnessadapt-rl/run-001/tool_wrapper/grep_ranker".to_string(),
        kind: ArtifactKind::ToolWrapper,
        declaration: "pkg.sweng.locate".to_string(),
        impl_ref: "executor:builtin/pkg.sweng.locate@af-execline".to_string(),
        ..ok_fixture()
    };
    validate_entry(&tool, &reg).expect("tool 形态正例应过 R1–R6");
}

#[test]
fn mapping_rules_reject_nonconforming_entries_on_repo_registry() {
    let reg = repo_registry();
    let cases: Vec<(&str, TrainingArtifact, &str)> = vec![
        (
            "R1 产物 id 为空",
            TrainingArtifact {
                artifact: String::new(),
                ..ok_fixture()
            },
            "R1 产物 id 为空",
        ),
        (
            "R1 产物 id 含保留段 @",
            TrainingArtifact {
                artifact: "run-001@1".to_string(),
                ..ok_fixture()
            },
            "R1 产物 id 含保留段 '@'",
        ),
        (
            "R2 形态与声明类不符（unit 形态锚 tool 声明）",
            TrainingArtifact {
                declaration: "pkg.sweng.locate".to_string(),
                ..ok_fixture()
            },
            "R2 形态与声明类不符",
        ),
        (
            "R3 声明名为空",
            TrainingArtifact {
                declaration: String::new(),
                ..ok_fixture()
            },
            "R3 声明名为空",
        ),
        (
            "R3 声明名含 @ 段",
            TrainingArtifact {
                declaration: "pkg.sweng.test_unit@1".to_string(),
                ..ok_fixture()
            },
            "R3 声明名含 '@'",
        ),
        (
            "R3 回注目标未注册",
            TrainingArtifact {
                declaration: "pkg.train.not_registered".to_string(),
                ..ok_fixture()
            },
            "R3 回注目标未注册",
        ),
        (
            "R4 版本未声明",
            TrainingArtifact {
                version: "999".to_string(),
                ..ok_fixture()
            },
            "R4 版本未声明",
        ),
        (
            "R5 impl_ref 与声明不一致",
            TrainingArtifact {
                impl_ref: "executor:builtin/pkg.sweng.view@af-execline".to_string(),
                ..ok_fixture()
            },
            "R5 impl_ref 与声明不一致",
        ),
        (
            "R6 读数定位串为空",
            TrainingArtifact {
                evidence: String::new(),
                ..ok_fixture()
            },
            "R6 读数定位串为空",
        ),
    ];
    for (name, entry, expect) in cases {
        let err = match validate_entry(&entry, &reg) {
            Ok(()) => panic!("{name}：应被拒（fail-closed）"),
            Err(e) => e,
        };
        assert!(
            err.contains(expect),
            "{name}：错误串应含 {expect:?}，实得 {err:?}"
        );
    }
}

// ---------------------------------------------------------------
// 夹具：合成注册表上的非空盘面对话（R5 各分支 / R1 册内重复 / schema 漂移）
// ---------------------------------------------------------------

/// 合成正例：锚夹具注册表内 unit `pkg.train.judge@1`（自锚 builtin）。
fn fixture_ok_entry() -> TrainingArtifact {
    TrainingArtifact {
        artifact: "trainkit/run-9/unit_judge/rerank_gate".to_string(),
        kind: ArtifactKind::UnitJudge,
        declaration: "pkg.train.judge".to_string(),
        version: "1".to_string(),
        impl_ref: "executor:builtin/pkg.train.judge@af-execline".to_string(),
        evidence: "fixtures/training-artifact-registry/readings.json".to_string(),
    }
}

fn fixture_register_file(artifacts: Vec<TrainingArtifact>) -> RegisterFile {
    RegisterFile {
        schema: REGISTER_SCHEMA.to_string(),
        note: Some("夹具登记册".to_string()),
        artifacts,
    }
}

/// 非空册盘面对话：合成注册表 + 合成登记册（1 条正例）实跑通过。
#[test]
fn fixture_register_conforms_on_nonempty_register() {
    let reg = fixture_registry();
    let file = read_register(&fixture_registry_root());
    validate_register(&file, &reg).expect("夹具登记册应合规");
    assert_eq!(file.artifacts.len(), 1, "夹具册应恰 1 条（盘面对话非空）");
}

#[test]
fn fixture_registry_rejects_unwired_reserved_and_malformed_impl_refs() {
    let reg = fixture_registry();
    let cases: Vec<(&str, TrainingArtifact, &str)> = vec![
        (
            "R5 builtin 目标未注册",
            TrainingArtifact {
                kind: ArtifactKind::ToolWrapper,
                declaration: "pkg.train.wrapper".to_string(),
                impl_ref: "executor:builtin/pkg.train.unwired@af-execline".to_string(),
                ..fixture_ok_entry()
            },
            "R5 builtin 目标未注册",
        ),
        (
            "R5 预留包路径形态",
            TrainingArtifact {
                kind: ArtifactKind::ToolWrapper,
                declaration: "pkg.train.reserved".to_string(),
                impl_ref: "packages/trainkit@1/impl".to_string(),
                ..fixture_ok_entry()
            },
            "R5 impl_ref 非执行面 builtin 形态",
        ),
        (
            "R5 声明无 impl_ref",
            TrainingArtifact {
                kind: ArtifactKind::ToolWrapper,
                declaration: "pkg.train.bare".to_string(),
                impl_ref: "executor:builtin/pkg.train.bare@af-execline".to_string(),
                ..fixture_ok_entry()
            },
            "R5 声明无 impl_ref",
        ),
        (
            "R5 缺 @crate 段",
            TrainingArtifact {
                kind: ArtifactKind::ToolWrapper,
                declaration: "pkg.train.nocrate".to_string(),
                impl_ref: "executor:builtin/pkg.train.nocrate".to_string(),
                ..fixture_ok_entry()
            },
            "R5 impl_ref 缺 @crate 段",
        ),
    ];
    for (name, entry, expect) in cases {
        let err = match validate_entry(&entry, &reg) {
            Ok(()) => panic!("{name}：应被拒（fail-closed）"),
            Err(e) => e,
        };
        assert!(
            err.contains(expect),
            "{name}：错误串应含 {expect:?}，实得 {err:?}"
        );
    }
}

#[test]
fn fixture_register_rejects_duplicate_artifact_ids() {
    let reg = fixture_registry();
    let dup = TrainingArtifact {
        artifact: "trainkit/run-9/unit_judge/rerank_gate".to_string(),
        ..fixture_ok_entry()
    };
    let file = fixture_register_file(vec![fixture_ok_entry(), dup]);
    let err = validate_register(&file, &reg).unwrap_err();
    assert!(
        err.contains("R1 产物 id 册内重复"),
        "错误串应含 R1 册内重复，实得 {err:?}"
    );
}

#[test]
fn fixture_register_rejects_schema_drift() {
    let reg = fixture_registry();
    let mut file = fixture_register_file(vec![fixture_ok_entry()]);
    file.schema = "af.training-artifact-registry/2".to_string();
    let err = validate_register(&file, &reg).unwrap_err();
    assert!(err.contains("schema 钉不符"), "实得 {err:?}");
}
