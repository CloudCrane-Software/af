//! G6 端到端：compiler_version 从只溯源到被核对——任务包（manifest 面 +
//! 计划面）携版本不符时，提交门（`af_order::cli::run` submit --dry-run true
//! ——与生产 main 同一入口）拒于一切副作用之前，且状态树零写入。
//!
//! 与改前探针（poc/poc-g6-compiler-version/probe-before-*.rs/.txt）对照：
//! 改前场景 A（manifest rogue）与场景 B（计划 rogue + 摘要自洽）真门**照过**
//! ——本文件钉死改后行为：A 在 manifest preflight 拒、B 在装订预检拒。
//!
//! 缺字段口径（G6 前后一致，改前探针 C/D 已实证）：manifest / 计划各缺
//! compiler_version 键 = 坏形拒（schema 必填 + deny_unknown_fields），
//! 不是「显式未核对」，更不许静默放行。

use std::fs;
use std::path::PathBuf;

use af_compiler::components::ComponentRegistry;
use af_compiler::{bind, generate_plan, lower, write_package_manifest, PACKAGE_MANIFEST_FILE};
use serde_json::Value;

fn repo_packages_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../af/packages")
}

fn compile_construction_plan() -> String {
    let src_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../af-compiler/tests/fixtures/construction_build.yaml");
    let src = fs::read_to_string(&src_path).unwrap_or_else(|e| panic!("读取 {src_path:?}：{e}"));
    let doc = af_dsl::load_str(&src).expect("建设图装载失败");
    let report = af_dsl::validate(&doc);
    assert!(report.is_ok(), "建设图校验应过：{report}");
    let ir = lower(&doc).unwrap();
    let registry =
        ComponentRegistry::load(repo_packages_root()).expect("af/packages 组件清单装载失败");
    let report = bind(&ir, &registry);
    assert!(
        report.is_ok(),
        "建设图绑定应零违例：{:?}",
        report.violations
    );
    let plan = generate_plan(&ir, &report).unwrap();
    serde_json::to_string_pretty(&plan).unwrap()
}

struct Fixture {
    root: PathBuf,
    data_root: PathBuf,
    state: PathBuf,
    spec: PathBuf,
    task_dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "af-order-g6-gate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&root);
        let data_root = root.join("pkg");
        let task_dir = data_root.join("E2G-demo");
        fs::create_dir_all(&task_dir).unwrap();
        let plan_src = compile_construction_plan();
        fs::write(task_dir.join("plan.json"), &plan_src).unwrap();
        fs::write(
            task_dir.join("task.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "task_id": "E2G-demo",
                "task_book": "建设 tools/factory-stat.py：读 状态观测/quota.json 与 kb/kb.db 汇总一行 JSON；附 unittest（夹具数据，不碰真库）与 README 段。",
                "registry_root": repo_packages_root(),
            }))
            .unwrap(),
        )
        .unwrap();
        let plan: Value = serde_json::from_str(&plan_src).unwrap();
        let plan_id = plan.pointer("/body/plan_id").unwrap().as_str().unwrap();
        write_package_manifest(&task_dir, plan_id).unwrap();
        let spec = root.join("order.json");
        fs::write(
            &spec,
            r#"{"order_id":"E2G-G6-1","line":{"line_id":"line-A","line_version":"1.0"},"budget":300}"#,
        )
        .unwrap();
        Self {
            state: root.join("state"),
            root,
            data_root,
            spec,
            task_dir,
        }
    }

    fn dryrun_args(&self) -> Vec<String> {
        vec![
            "submit".into(),
            "--spec".into(),
            self.spec.display().to_string(),
            "--state".into(),
            self.state.display().to_string(),
            "--task".into(),
            "E2G-demo".into(),
            "--profile".into(),
            "glm-cp".into(),
            "--data-root".into(),
            self.data_root.display().to_string(),
            "--unit".into(),
            "graph".into(),
            "--dry-run".into(),
            "true".into(),
        ]
    }
}

fn residue_count(dir: &std::path::Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            if entry.path().is_dir() {
                1 + residue_count(&entry.path())
            } else {
                1
            }
        })
        .sum()
}

/// 相符 ⇒ 过：当前构建产任务包过真门，留证面回显当前版本
/// （改后对照组——与 A/B 的拒对照）。
#[test]
fn gate_passes_when_package_carries_current_compiler_version() {
    let fx = Fixture::new();
    let out = af_order::cli::run(&fx.dryrun_args()).unwrap_or_else(|e| panic!("应过门：{e}"));
    let ev: Value = serde_json::from_str(&out).unwrap();
    let checks = ev["checks"].as_array().unwrap();
    for needle in ["package_manifest", "planbind_preflight"] {
        assert!(
            checks.iter().any(|c| c == needle),
            "应过 {needle}：{checks:?}"
        );
    }
    assert_eq!(
        ev["package_manifest"]["compiler_version"],
        af_compiler::COMPILER_VERSION,
        "留证面回显当前构建版本"
    );
    assert_eq!(residue_count(&fx.state), 0);
    let _ = fs::remove_dir_all(&fx.root);
}

/// 场景 A（改前探针 A 照过）：manifest.compiler_version = rogue（摘要一致）
/// ⇒ manifest preflight 拒（OR1001 + 专用文案），状态树零写入。
#[test]
fn gate_rejects_manifest_from_different_compiler_version() {
    let fx = Fixture::new();
    let mpath = fx.task_dir.join(PACKAGE_MANIFEST_FILE);
    let mut m: Value = serde_json::from_str(&fs::read_to_string(&mpath).unwrap()).unwrap();
    m["compiler_version"] = serde_json::json!("0.0.1-rogue");
    fs::write(&mpath, serde_json::to_string_pretty(&m).unwrap()).unwrap();

    let err = af_order::cli::run(&fx.dryrun_args())
        .expect_err("manifest 版本不符应拒（改前探针 A：照过——破窗已实证）");
    let msg = err.to_string();
    assert!(msg.contains("OR1001"), "{msg}");
    assert!(
        msg.contains("compiler_version 不符") && msg.contains("0.0.1-rogue"),
        "应点名版本面：{msg}"
    );
    assert!(msg.contains("重新组包"), "行动指令在文案面：{msg}");
    // 状态树零写入（拒于一切副作用之前）。
    assert_eq!(residue_count(&fx.state), 0, "版本拒零残留");
    assert!(!fx.state.exists(), "版本拒不得创建 state 目录");
    let _ = fs::remove_dir_all(&fx.root);
}

/// 场景 B（改前探针 B 照过）：计划 compiled_from.compiler_version = rogue，
/// 对改后文件重组 manifest（摘要面自洽——隔离计划面核对）⇒ 装订预检拒
/// （EX4005 透传），状态树零写入。
#[test]
fn gate_rejects_plan_from_different_compiler_version() {
    let fx = Fixture::new();
    let ppath = fx.task_dir.join("plan.json");
    let mut p: Value = serde_json::from_str(&fs::read_to_string(&ppath).unwrap()).unwrap();
    p["body"]["compiled_from"]["compiler_version"] = serde_json::json!("0.0.1-rogue");
    let plan_src = serde_json::to_string_pretty(&p).unwrap();
    fs::write(&ppath, &plan_src).unwrap();
    let plan_id = p
        .pointer("/body/plan_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    write_package_manifest(&fx.task_dir, plan_id).unwrap();

    let err = af_order::cli::run(&fx.dryrun_args())
        .expect_err("计划版本不符应拒（改前探针 B：照过——破窗已实证）");
    let msg = err.to_string();
    assert!(msg.contains("OR1001") && msg.contains("装订"), "{msg}");
    assert!(
        msg.contains("compiler_version 不符") && msg.contains("0.0.1-rogue"),
        "装订预检应点名版本面：{msg}"
    );
    assert_eq!(residue_count(&fx.state), 0, "版本拒零残留");
    assert!(!fx.state.exists());
    let _ = fs::remove_dir_all(&fx.root);
}

/// 缺字段口径钉死（改前探针 C/D 已实证同行为）：manifest 缺
/// compiler_version 键 ⇒ 坏形拒——不许静默、不与「核对过」混同。
#[test]
fn gate_rejects_manifest_missing_compiler_version_field() {
    let fx = Fixture::new();
    let mpath = fx.task_dir.join(PACKAGE_MANIFEST_FILE);
    let mut m: Value = serde_json::from_str(&fs::read_to_string(&mpath).unwrap()).unwrap();
    m.as_object_mut().unwrap().remove("compiler_version");
    fs::write(&mpath, serde_json::to_string_pretty(&m).unwrap()).unwrap();

    let err = af_order::cli::run(&fx.dryrun_args()).expect_err("缺版本键应坏形拒");
    let msg = err.to_string();
    assert!(msg.contains("OR1001") && msg.contains("坏形"), "{msg}");
    assert!(
        msg.contains("compiler_version"),
        "坏形文案应点名缺失键：{msg}"
    );
    assert_eq!(residue_count(&fx.state), 0, "坏形拒零残留");
    let _ = fs::remove_dir_all(&fx.root);
}

/// 缺字段口径钉死（计划面）：compiled_from 缺 compiler_version 键（摘要
/// 自洽）⇒ 装订预检 decode 拒。
#[test]
fn gate_rejects_plan_missing_compiler_version_field() {
    let fx = Fixture::new();
    let ppath = fx.task_dir.join("plan.json");
    let mut p: Value = serde_json::from_str(&fs::read_to_string(&ppath).unwrap()).unwrap();
    p["body"]["compiled_from"]
        .as_object_mut()
        .unwrap()
        .remove("compiler_version");
    let plan_src = serde_json::to_string_pretty(&p).unwrap();
    fs::write(&ppath, &plan_src).unwrap();
    let plan_id = p
        .pointer("/body/plan_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    write_package_manifest(&fx.task_dir, plan_id).unwrap();

    let err = af_order::cli::run(&fx.dryrun_args()).expect_err("计划缺版本键应 decode 拒");
    let msg = err.to_string();
    assert!(msg.contains("OR1001") && msg.contains("装订"), "{msg}");
    assert!(
        msg.contains("missing field `compiler_version`"),
        "decode 坏形点名缺失键：{msg}"
    );
    assert_eq!(residue_count(&fx.state), 0, "decode 拒零残留");
    let _ = fs::remove_dir_all(&fx.root);
}
