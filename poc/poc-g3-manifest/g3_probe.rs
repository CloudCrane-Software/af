//! G3 探针（改前/改后行为对照取材）：不承载断言，只把「同一场景」下的
//! 真实行为面打印到 stdout（--nocapture），由外层 shell 落盘对照。
//!
//! 场景（全部 graph 档 + RealExec 三件套 = 要求核对的路径）：
//!  E1 manifest 缺席 + 干跑 —— 破窗本体：改前应「成功但没核对」；
//!  E2 manifest 缺席 + 正式 submit（无引擎环境）—— 改前应越过 preflight
//!     死在更晚的引擎定位（state 有残留）；改后应拒于 preflight（零残留）；
//!  E3 manifest 在场 + plan.json 被改（偷预算帽模型）+ 干跑 —— 判据不动面：
//!     改前改后都应 ContentTampered（清态换号重提）；
//!  E4 manifest 在场 + 一致 + 干跑 —— 判据不动面：改前改后都应通过且
//!     checks 点名 package_manifest。
//!
//! 探针文件本身不入库（poc 留档副本在 poc/poc-g3-manifest/）。

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
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "g3-probe-{tag}-{}-{}",
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
            r#"{"order_id":"E2G-G3-1","line":{"line_id":"line-A","line_version":"1.0"},"budget":300}"#,
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
        let mut v = vec![
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
        ];
        if dry_run {
            v.push("--dry-run".into());
            v.push("true".into());
        }
        v
    }

    fn residue(&self) -> usize {
        fn count(dir: &std::path::Path) -> usize {
            let Ok(entries) = fs::read_dir(dir) else {
                return 0;
            };
            entries
                .flatten()
                .map(|e| if e.path().is_dir() { 1 + count(&e.path()) } else { 1 })
                .sum()
        }
        count(&self.state)
    }
}

fn report(tag: &str, outcome: &str, detail: String) {
    println!("### {tag} :: {outcome}\n{detail}\n");
}

#[test]
fn g3_probe_scenarios() {
    // E1 缺席 + 干跑。
    let fx = Fx::new("e1");
    match af_order::cli::run(&fx.args(true)) {
        Ok(out) => {
            let ev: Value = serde_json::from_str(&out).unwrap();
            let checks = ev["checks"].as_array().unwrap().clone();
            report(
                "E1-absent-dryrun",
                "Ok（干跑放行）",
                format!(
                    "package_manifest 字段 = {}\nchecks 含 package_manifest = {}\nstate 残留 = {}",
                    ev["package_manifest"],
                    checks.iter().any(|c| c == "package_manifest"),
                    fx.residue()
                ),
            );
        }
        Err(err) => report(
            "E1-absent-dryrun",
            "Err（干跑拒绝）",
            format!("报错：{err}\nstate 残留 = {}", fx.residue()),
        ),
    }
    let _ = fs::remove_dir_all(&fx.root);

    // E2 缺席 + 正式（无引擎环境：如实记录死点与残留）。
    let fx = Fx::new("e2");
    match af_order::cli::run(&fx.args(false)) {
        Ok(out) => report("E2-absent-submit", "Ok（?!）", out),
        Err(err) => report(
            "E2-absent-submit",
            "Err",
            format!(
                "报错：{err}\nstate 残留 = {}（>0 = 越过 preflight 后才死）",
                fx.residue()
            ),
        ),
    }
    let _ = fs::remove_dir_all(&fx.root);

    // E3 在场 + 被改（plan 偷删 cost_cap）+ 干跑。
    let fx = Fx::new("e3");
    write_package_manifest(&fx.task_dir, "plan.factory_construction_build@1.0+x").unwrap();
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
    match af_order::cli::run(&fx.args(true)) {
        Ok(out) => report("E3-tampered-dryrun", "Ok（?!）", out),
        Err(err) => report(
            "E3-tampered-dryrun",
            "Err（应拒）",
            format!("报错：{err}\nstate 残留 = {}", fx.residue()),
        ),
    }
    let _ = fs::remove_dir_all(&fx.root);

    // E4 在场 + 一致 + 干跑。
    let fx = Fx::new("e4");
    write_package_manifest(&fx.task_dir, "plan.factory_construction_build@1.0+x").unwrap();
    match af_order::cli::run(&fx.args(true)) {
        Ok(out) => {
            let ev: Value = serde_json::from_str(&out).unwrap();
            let checks = ev["checks"].as_array().unwrap().clone();
            report(
                "E4-present-dryrun",
                "Ok（应过）",
                format!(
                    "package_manifest.plan_id = {}\nchecks 含 package_manifest = {}\nstate 残留 = {}",
                    ev["package_manifest"]["plan_id"],
                    checks.iter().any(|c| c == "package_manifest"),
                    fx.residue()
                ),
            );
        }
        Err(err) => report("E4-present-dryrun", "Err（?!）", err.to_string()),
    }
    let _ = fs::remove_dir_all(&fx.root);
}
