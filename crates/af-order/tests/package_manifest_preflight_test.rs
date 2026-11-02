//! E1 缺口②：提交前任务包内容 hash 一致性（防重放语义内置提交链）。
//!
//! 链：af-compiler 组包写 manifest（task.json/plan.json 内容 sha256 留档）
//! → af-order submit（正式 + 干跑两路）在一切副作用之前重算核对——
//! 不符即 OR1001 拒、文案点名「清态换号重提」、零残留。
//! 缺席 = 不核对（旧任务包同口径，见 manifest_absent_passes_through）。

use std::fs;
use std::path::PathBuf;

use af_compiler::components::ComponentRegistry;
use af_compiler::{bind, generate_plan, lower, write_package_manifest};
use serde_json::Value;

fn repo_packages_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../af/packages")
}

fn compile_construction_plan() -> String {
    let src_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../af-compiler/tests/fixtures/construction_build.yaml");
    let src = fs::read_to_string(&src_path).unwrap();
    let doc = af_dsl::load_str(&src).unwrap();
    assert!(af_dsl::validate(&doc).is_ok());
    let ir = lower(&doc).unwrap();
    let registry = ComponentRegistry::load(repo_packages_root()).unwrap();
    let report = bind(&ir, &registry);
    assert!(report.is_ok());
    serde_json::to_string_pretty(&generate_plan(&ir, &report).unwrap()).unwrap()
}

struct Fx {
    root: PathBuf,
    data_root: PathBuf,
    state: PathBuf,
    spec: PathBuf,
    task_dir: PathBuf,
}

impl Fx {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "af-order-pkgmanifest-{}-{}",
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
        fs::write(task_dir.join("plan.json"), compile_construction_plan()).unwrap();
        fs::write(
            task_dir.join("task.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "task_id": "E2G-demo",
                "task_book": "建设 tools/factory-stat.py……",
                "registry_root": repo_packages_root(),
            }))
            .unwrap(),
        )
        .unwrap();
        let spec = root.join("order.json");
        fs::write(
            &spec,
            r#"{"order_id":"E2G-PKG-1","line":{"line_id":"line-A","line_version":"1.0"},"budget":300}"#,
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

    fn args(&self, dry_run: bool) -> Vec<String> {
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
            dry_run.to_string(),
        ]
    }

    fn residue(dir: &std::path::Path) -> usize {
        let Ok(entries) = fs::read_dir(dir) else {
            return 0;
        };
        entries
            .flatten()
            .map(|e| {
                if e.path().is_dir() {
                    1 + Self::residue(&e.path())
                } else {
                    1
                }
            })
            .sum()
    }
}

#[test]
fn dryrun_passes_with_manifest_and_records_it_in_evidence() {
    let fx = Fx::new();
    let manifest =
        write_package_manifest(&fx.task_dir, "plan.factory_construction_build@1.0+x").unwrap();

    let out = af_order::cli::run(&fx.args(true)).unwrap();
    let evidence: Value = serde_json::from_str(&out).unwrap();
    let checks = evidence["checks"].as_array().unwrap();
    assert!(
        checks.iter().any(|c| c == "package_manifest"),
        "manifest 核对应点名在 checks：{checks:?}"
    );
    assert_eq!(
        evidence["package_manifest"]["plan_id"],
        serde_json::json!(manifest.plan_id)
    );
    assert_eq!(Fx::residue(&fx.state), 0, "干跑零残留");
    let _ = fs::remove_dir_all(&fx.root);
}

#[test]
fn submit_preflight_rejects_tampered_package_before_any_side_effect() {
    let fx = Fx::new();
    write_package_manifest(&fx.task_dir, "plan.factory_construction_build@1.0+x").unwrap();
    // 编译后偷换计划面（预算帽偷换模型——E2G2 实证面）：
    let mut plan: Value =
        serde_json::from_str(&fs::read_to_string(fx.task_dir.join("plan.json")).unwrap()).unwrap();
    plan["body"]["budget"]["budgets"]
        .as_array_mut()
        .unwrap()
        .retain(|b| b["id"] != "cost_cap");
    fs::write(
        fx.task_dir.join("plan.json"),
        serde_json::to_string_pretty(&plan).unwrap(),
    )
    .unwrap();

    // 正式提交路（--dry-run 缺席）：拒于状态目录创建之前。
    let err = af_order::cli::run(&fx.args(false)).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("OR1001"), "应 OR1001：{msg}");
    assert!(msg.contains("plan.json"), "应点名被改文件：{msg}");
    assert!(
        msg.contains("清态换号重提"),
        "防重放契约行动指令必须在文案面：{msg}"
    );
    assert_eq!(Fx::residue(&fx.state), 0, "拒于一切副作用之前：零残留");
    assert!(!fx.state.exists());

    // 干跑路同点同拒。
    let err2 = af_order::cli::run(&fx.args(true)).unwrap_err();
    assert!(err2.to_string().contains("清态换号重提"), "{err2}");
    assert_eq!(Fx::residue(&fx.state), 0);
    let _ = fs::remove_dir_all(&fx.root);
}

#[test]
fn manifest_absent_passes_through() {
    let fx = Fx::new();
    // 旧式任务包（无 manifest）：不核对，干跑其余预检照过。
    let out = af_order::cli::run(&fx.args(true)).unwrap();
    let evidence: Value = serde_json::from_str(&out).unwrap();
    let checks = evidence["checks"].as_array().unwrap();
    assert!(
        !checks.iter().any(|c| c == "package_manifest"),
        "manifest 缺席 = 不核对（checks 不点名）：{checks:?}"
    );
    assert_eq!(evidence["package_manifest"], serde_json::Value::Null);
    assert_eq!(Fx::residue(&fx.state), 0);
    let _ = fs::remove_dir_all(&fx.root);
}

#[test]
fn manifest_tampered_itself_is_rejected() {
    let fx = Fx::new();
    write_package_manifest(&fx.task_dir, "plan.factory_construction_build@1.0+x").unwrap();
    // manifest 留档被改（task.json 未动——换真值源模型）：判据面摘要不符。
    let mut m: Value = serde_json::from_str(
        &fs::read_to_string(fx.task_dir.join("package_manifest.json")).unwrap(),
    )
    .unwrap();
    m["task_sha256"] = serde_json::json!("0".repeat(64));
    fs::write(
        fx.task_dir.join("package_manifest.json"),
        serde_json::to_string_pretty(&m).unwrap(),
    )
    .unwrap();

    let err = af_order::cli::run(&fx.args(true)).unwrap_err();
    assert!(err.to_string().contains("清态换号重提"), "{err}");
    assert_eq!(Fx::residue(&fx.state), 0);
    let _ = fs::remove_dir_all(&fx.root);
}
