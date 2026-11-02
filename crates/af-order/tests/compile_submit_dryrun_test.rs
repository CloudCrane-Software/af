//! E1 缺口①端到端：编译 → 提交一跳干跑（`--dry-run true`）留证。
//!
//! 链路（与生产同一入口，零替身）：
//! 1. af-compiler 四段全链（装载 → 校验 → lower → bind → plan）编译建设任务
//!    图（build 单节点 + deliverable 类型）——产物即 `<task>/plan.json` 提交形态；
//! 2. 组装任务包 `<data_root>/<task>/{task.json, plan.json}`（task.json 只带
//!    通用字段 task_id/task_book——E1 实证的建设任务面）；
//! 3. `af-order submit --unit graph --dry-run true` 走提交前链（材料面装配 +
//!    planbind 装订预检 + 档案预检），stdout 留证；
//! 4. 断言零副作用：state 根不落任何文件（无订单目录/凭证/账本）——不真下单。
//!
//! 反向用例：任务包计划面被改（编译后偷换 plan.json）→ 干跑在装订预检
//! fail-loud（E9004 注册表 digest/信封 decode 拒），同样零残留——W2-C55
//! 防偷换语义在提交前链的实证面。

use std::fs;
use std::path::PathBuf;

use af_compiler::components::ComponentRegistry;
use af_compiler::{bind, generate_plan, lower};
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
            "af-order-e2e-dryrun-{}-{}",
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
        // 任务包：计划 = 编译产物直落（非 afb 源形）；task.json 只带通用字段。
        fs::write(task_dir.join("plan.json"), compile_construction_plan()).unwrap();
        fs::write(
            task_dir.join("task.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "task_id": "E2G-demo",
                "task_book": "建设 tools/factory-stat.py：读 状态观测/quota.json 与 kb/kb.db 汇总一行 JSON；附 unittest（夹具数据，不碰真库）与 README 段。",
                // D-48：缺省注册表根是本机死路径，任务包必须显式声明（E1 交接
                // 侦察结论第 1 条——registry_root 取 task.json 声明）。
                "registry_root": repo_packages_root(),
            }))
            .unwrap(),
        )
        .unwrap();
        let spec = root.join("order.json");
        fs::write(
            &spec,
            r#"{"order_id":"E2G-DRY-1","line":{"line_id":"line-A","line_version":"1.0"},"budget":300}"#,
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

    fn residue_count(dir: &std::path::Path) -> usize {
        let Ok(entries) = fs::read_dir(dir) else {
            return 0;
        };
        entries
            .flatten()
            .map(|entry| {
                if entry.path().is_dir() {
                    1 + Self::residue_count(&entry.path())
                } else {
                    1
                }
            })
            .sum()
    }
}

#[test]
fn compile_to_submit_one_hop_dryrun_leaves_evidence_without_side_effects() {
    let fx = Fixture::new();

    let out = af_order::cli::run(&fx.dryrun_args()).unwrap_or_else(|e| panic!("干跑应成功：{e}"));
    let evidence: Value = serde_json::from_str(&out).expect("干跑 stdout 应是 JSON 留证");

    // 留证面：干跑标记 + 全链检查点名 + 摘要钉死 + 计划身份。
    assert_eq!(evidence["dry_run"], serde_json::json!(true));
    assert_eq!(evidence["order_id"], serde_json::json!("E2G-DRY-1"));
    let checks = evidence["checks"].as_array().expect("checks 应成数组");
    for needle in [
        "parse_spec",
        "resolve_unit",
        "pin_task_digest",
        "pin_plan_digest",
        "graph_task_package_layout",
        "planbind_preflight",
        "profile_resolvable",
    ] {
        assert!(
            checks.iter().any(|c| c == needle),
            "干跑 checks 应含 {needle}：{checks:?}"
        );
    }
    // 编译产物的摘要被钉死（提交面留证——正是 W2-C55 的语义材料）。
    assert!(evidence["task_sha256"].is_string(), "task.json 摘要钉死");
    assert!(evidence["plan_sha256"].is_string(), "plan.json 摘要钉死");
    // 装订预检取回的计划身份 = 编译产物身份。
    assert!(
        evidence["unit_detail"]["plan_id"]
            .as_str()
            .unwrap_or_default()
            .starts_with("plan.factory_construction_build@1.0+"),
        "计划身份应是编译产物派生形：{}",
        evidence["unit_detail"]["plan_id"]
    );
    assert_eq!(
        evidence["unit_detail"]["judge_wired"],
        serde_json::json!(true),
        "生产判分锚应接线（与正式 submit 同一装配）"
    );

    // 零副作用：state 根零文件（不真下单——无订单目录/凭证/账本）。
    assert_eq!(
        Fixture::residue_count(&fx.state),
        0,
        "干跑档零残留：state 根不应存在任何文件"
    );
    assert!(!fx.state.exists(), "干跑不应创建 state 目录");

    let _ = fs::remove_dir_all(&fx.root);
}

#[test]
fn dryrun_rejects_broken_plan_package_without_side_effects() {
    let fx = Fixture::new();
    // 计划面破损（非编译产物形）：装订预检的信封 decode 应 fail-loud。
    // 注：「同形偷换预算帽」不在本链检出面——planbind 只核注册表 digest/
    // 信封/一致性；内容级防偷换 = 缺口②的任务包 manifest 链（见
    // package_manifest 测试），那才是 W2-C55 语义内置编译链的落点。
    fs::write(fx.task_dir.join("plan.json"), b"{\"v\":1,\"body\":broken").unwrap();

    let err = af_order::cli::run(&fx.dryrun_args()).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("OR1001") && msg.contains("装订"),
        "破损计划应在干跑装订预检 fail-loud：{msg}"
    );
    // 失败路径同样零残留（W3-C24 纪律覆盖干跑档）。
    assert_eq!(Fixture::residue_count(&fx.state), 0, "失败干跑零残留");
    assert!(!fx.state.exists());

    let _ = fs::remove_dir_all(&fx.root);
}

#[test]
fn dryrun_flag_parse_is_strict() {
    let fx = Fixture::new();
    let mut bad = fx.dryrun_args();
    let pos = bad.iter().position(|a| a == "true").unwrap();
    bad[pos] = "yes".into();
    let err = af_order::cli::run(&bad).unwrap_err();
    assert!(
        err.to_string().contains("OR1001") && err.to_string().contains("--dry-run"),
        "非法布尔值应 OR1001：{err}"
    );
    let _ = fs::remove_dir_all(&fx.root);
}

/// E1 缺口④端到端：编译器组包（任务书内嵌 declare/envelope 协议教学段 +
/// 内容 hash 留档）→ 提交干跑一跳——带教学任务书的通用建设任务包照过
/// 提交前链，checks 点名 package_manifest（缺口②）与 planbind_preflight。
#[test]
fn compiler_assembled_teaching_package_passes_dryrun_one_hop() {
    let fx = Fixture::new();
    // 用编译器组包正本覆盖夹具手装包（教学任务书 + manifest 一并落盘）。
    let spec = af_compiler::ConstructionSpec {
        task_id: "E2G-demo".into(),
        statement: "建设 tools/factory-stat.py：读 quota.json 与 kb.db 汇总一行 JSON。".into(),
        acceptance: "unittest 全过（夹具数据）+ README 段。".into(),
        image: None,
        test_command: None,
        registry_root: repo_packages_root(),
    };
    af_compiler::assemble_construction_package(&fx.task_dir, &spec).unwrap();

    let out = af_order::cli::run(&fx.dryrun_args()).unwrap_or_else(|e| panic!("干跑应成功：{e}"));
    let evidence: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(evidence["dry_run"], serde_json::json!(true));
    let checks = evidence["checks"].as_array().unwrap();
    for needle in [
        "package_manifest",
        "planbind_preflight",
        "profile_resolvable",
    ] {
        assert!(checks.iter().any(|c| c == needle), "checks 应含 {needle}");
    }
    // 教学任务书确实在包里（缺口④语义随包进提交链）。
    let task: Value =
        serde_json::from_str(&fs::read_to_string(fx.task_dir.join("task.json")).unwrap()).unwrap();
    assert!(
        task["task_book"]
            .as_str()
            .unwrap_or_default()
            .contains(af_compiler::PROTOCOL_TEACHING),
        "组包任务书应内嵌协议教学段"
    );
    assert_eq!(Fixture::residue_count(&fx.state), 0, "干跑零残留");
    let _ = fs::remove_dir_all(&fx.root);
}
