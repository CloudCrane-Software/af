//! `--unit graph` 单元路由验收（图执行器 v1 接入；TDD 先红后绿——
//! `UnitChoice::Graph` 缺席时编译红）。
//!
//! 契约（F-第四单执行长缺口销项）：
//! - `--unit graph` 需要三件套（与 swev/report 同矩阵），缺 → OR1001；
//! - 缺省行为**不变**：Auto 三件套在=Swev、缺=stub（unit_route.rs 既有断言零破）；
//! - 图单元材料面 = report 同款任务包约定 `<data_root>/<task>/plan.json`
//!   （submit 级 `--plan` 档形态 = 开放问题 8 已裁留 v0.2，不预判）。

use std::path::PathBuf;

use af_order::cli::{parse_args, Command};
use af_order::exec::{graph_unit_for, parse_unit_choice, resolve_unit, RealExec, UnitChoice};

fn trio() -> Vec<String> {
    vec![
        "--spec".into(),
        "/tmp/s.json".into(),
        "--state".into(),
        "/tmp/st".into(),
        "--task".into(),
        "graph-task-1".into(),
        "--profile".into(),
        "glm-cp".into(),
        "--data-root".into(),
        "/tmp/pkgroot".into(),
    ]
}

fn graph_exec(data_root: &str) -> RealExec {
    RealExec {
        task: "graph-task-1".into(),
        profile: "glm-cp".into(),
        data_root: PathBuf::from(data_root),
        host: "anolis".into(),
        max_turns: 60,
        spend_cap_cents: 300,
    }
}

#[test]
fn unit_choice_parses_graph_and_keeps_matrix_strict() {
    assert_eq!(parse_unit_choice("graph").unwrap(), UnitChoice::Graph);
    // graph 缺三件套 → OR1001。
    let err = resolve_unit(UnitChoice::Graph, None).unwrap_err();
    assert!(
        err.to_string().contains("三件套") || err.to_string().contains("--task"),
        "{err}"
    );
    // graph 携三件套 → Graph。
    assert_eq!(
        resolve_unit(UnitChoice::Graph, Some(&graph_exec("/tmp/x"))).unwrap(),
        af_order::exec::SelectedUnit::Graph
    );
    // 缺省行为不变：Auto 与 stub 不因新增臂漂移。
    assert_eq!(
        resolve_unit(UnitChoice::Auto, Some(&graph_exec("/tmp/x"))).unwrap(),
        af_order::exec::SelectedUnit::Swev
    );
    assert_eq!(
        resolve_unit(UnitChoice::Auto, None).unwrap(),
        af_order::exec::SelectedUnit::Stub
    );
    // 非法值仍拒。
    assert!(parse_unit_choice("plan").is_err());
}

#[test]
fn cli_accepts_unit_graph_flag() {
    let mut args = trio();
    args.insert(0, "submit".into());
    args.extend(["--unit".into(), "graph".into()]);
    match parse_args(&args).unwrap() {
        Command::Submit { unit, exec, .. } => {
            assert_eq!(unit, UnitChoice::Graph);
            assert!(exec.is_some());
        }
        other => panic!("应解析为 Submit：{other:?}"),
    }
}

#[test]
fn graph_unit_materials_require_plan_json_in_task_package() {
    // 任务包布局（v1 约定）：<data_root>/<task>/{plan.json, task.json}；
    // registry_root 取 task.json 声明，缺省机器本地常量（report 同款）。
    let base = std::env::temp_dir().join(format!("graph-route-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    let task_dir = base.join("graph-task-1");
    std::fs::create_dir_all(&task_dir).unwrap();
    std::fs::write(
        task_dir.join("task.json"),
        serde_json::json!({
            "repo": "django/django",
            "instance_id": "graph-task-1",
            "problem_statement": "fix",
            "image": "img",
            "registry_root": "/tmp/some-packages"
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(task_dir.join("plan.json"), "{\"v\":1}").unwrap();

    let spec = graph_unit_for(&graph_exec(base.to_str().unwrap())).expect("材料齐应产 spec");
    assert_eq!(spec.profile, "glm-cp");
    assert_eq!(
        spec.registry_root,
        PathBuf::from("/tmp/some-packages"),
        "registry_root 取 task.json 声明"
    );

    // 缺 plan.json → fail-loud（v1 约定文案点名）。
    std::fs::remove_file(task_dir.join("plan.json")).unwrap();
    let err = graph_unit_for(&graph_exec(base.to_str().unwrap())).unwrap_err();
    assert!(
        err.to_string().contains("plan.json"),
        "缺计划应点名约定路径：{err}"
    );
    std::fs::remove_dir_all(&base).ok();
}
