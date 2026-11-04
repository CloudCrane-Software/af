//! G6 改前探针（临时文件，跑完归档到 poc/ 后删除）：
//! compiler_version 不符的任务包/计划过消费侧真门（af_order::cli::run
//! submit --dry-run true——与生产 main 同一入口）的**当前行为**实证。
//!
//! 场景 A（manifest 面）：任务包正常组包后，只把 package_manifest.json 的
//! compiler_version 改成 0.0.1-rogue（摘要保持一致）——真门现在应照过。
//! 场景 B（计划面）：plan.json 的 compiled_from.compiler_version 改成
//! 0.0.1-rogue 后对改后文件重组 manifest（摘要面自洽）——真门现在应照过。
//! 场景 C/D（缺字段口径）：manifest / plan 各缺 compiler_version 键——
//! 记录当前处置（预期：坏形拒，fail-closed）。

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
            "af-order-g6-probe-{}-{}",
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
            r#"{"order_id":"E2G-PROBE-1","line":{"line_id":"line-A","line_version":"1.0"},"budget":300}"#,
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

/// 场景 A：manifest.compiler_version = 0.0.1-rogue（摘要不动）。
/// 改前预期：真门**照过**（破窗）。
#[test]
fn probe_a_manifest_rogue_version_passes_before_fix() {
    let fx = Fixture::new();
    let mpath = fx.task_dir.join(PACKAGE_MANIFEST_FILE);
    let mut m: Value = serde_json::from_str(&fs::read_to_string(&mpath).unwrap()).unwrap();
    m["compiler_version"] = serde_json::json!("0.0.1-rogue");
    fs::write(&mpath, serde_json::to_string_pretty(&m).unwrap()).unwrap();

    match af_order::cli::run(&fx.dryrun_args()) {
        Ok(out) => {
            let ev: Value = serde_json::from_str(&out).unwrap();
            println!("PROBE-A: PASS（破窗实证）dry_run={} checks={:?}",
                ev["dry_run"], ev["checks"]);
            println!("PROBE-A: package_manifest.compiler_version = {}",
                ev["package_manifest"]["compiler_version"]);
            assert_eq!(ev["package_manifest"]["compiler_version"], "0.0.1-rogue");
        }
        Err(err) => panic!("PROBE-A: 改前应照过但被拒：{err}"),
    }
    assert_eq!(residue_count(&fx.state), 0);
    let _ = fs::remove_dir_all(&fx.root);
}

/// 场景 B：plan.compiled_from.compiler_version = 0.0.1-rogue，对改后文件
/// 重组 manifest（摘要面自洽）。改前预期：真门**照过**（破窗）。
#[test]
fn probe_b_plan_rogue_version_passes_before_fix() {
    let fx = Fixture::new();
    let ppath = fx.task_dir.join("plan.json");
    let mut p: Value = serde_json::from_str(&fs::read_to_string(&ppath).unwrap()).unwrap();
    p["body"]["compiled_from"]["compiler_version"] = serde_json::json!("0.0.1-rogue");
    let plan_src = serde_json::to_string_pretty(&p).unwrap();
    fs::write(&ppath, &plan_src).unwrap();
    let plan_id = p.pointer("/body/plan_id").unwrap().as_str().unwrap().to_string();
    write_package_manifest(&fx.task_dir, plan_id).unwrap();

    match af_order::cli::run(&fx.dryrun_args()) {
        Ok(out) => {
            let ev: Value = serde_json::from_str(&out).unwrap();
            println!("PROBE-B: PASS（破窗实证）dry_run={} checks={:?}",
                ev["dry_run"], ev["checks"]);
            println!("PROBE-B: unit_detail.plan_id = {}", ev["unit_detail"]["plan_id"]);
        }
        Err(err) => panic!("PROBE-B: 改前应照过但被拒：{err}"),
    }
    assert_eq!(residue_count(&fx.state), 0);
    let _ = fs::remove_dir_all(&fx.root);
}

/// 场景 C：manifest 缺 compiler_version 键。记录当前处置。
#[test]
fn probe_c_manifest_missing_version_field_current_behavior() {
    let fx = Fixture::new();
    let mpath = fx.task_dir.join(PACKAGE_MANIFEST_FILE);
    let mut m: Value = serde_json::from_str(&fs::read_to_string(&mpath).unwrap()).unwrap();
    let obj = m.as_object_mut().unwrap();
    obj.remove("compiler_version");
    fs::write(&mpath, serde_json::to_string_pretty(&m).unwrap()).unwrap();

    match af_order::cli::run(&fx.dryrun_args()) {
        Ok(_) => println!("PROBE-C: PASS（缺字段照过）"),
        Err(err) => println!("PROBE-C: REJECT：{err}"),
    }
    println!("PROBE-C: state residue = {}", residue_count(&fx.state));
    let _ = fs::remove_dir_all(&fx.root);
}

/// 场景 D：plan 的 compiled_from 缺 compiler_version 键，重组 manifest。
/// 记录当前处置。
#[test]
fn probe_d_plan_missing_version_field_current_behavior() {
    let fx = Fixture::new();
    let ppath = fx.task_dir.join("plan.json");
    let mut p: Value = serde_json::from_str(&fs::read_to_string(&ppath).unwrap()).unwrap();
    p["body"]["compiled_from"]
        .as_object_mut()
        .unwrap()
        .remove("compiler_version");
    let plan_src = serde_json::to_string_pretty(&p).unwrap();
    fs::write(&ppath, &plan_src).unwrap();
    let plan_id = p.pointer("/body/plan_id").unwrap().as_str().unwrap().to_string();
    write_package_manifest(&fx.task_dir, plan_id).unwrap();

    match af_order::cli::run(&fx.dryrun_args()) {
        Ok(_) => println!("PROBE-D: PASS（缺字段照过）"),
        Err(err) => println!("PROBE-D: REJECT：{err}"),
    }
    println!("PROBE-D: state residue = {}", residue_count(&fx.state));
    let _ = fs::remove_dir_all(&fx.root);
}
