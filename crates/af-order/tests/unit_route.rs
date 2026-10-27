//! G4 裁决7：`--unit swev|report|stub` 单元路由验收（TDD 先红后绿）。
//!
//! 契约（裁决7 + 报告包设计 §6.2-②）：
//! - 缺省行为**不变**：不给 `--unit` 时三件套在=Swev、三件套缺=stub 冒烟
//!   （exec_contract.rs 既有断言零破——本文件另钉两条缺省臂）；
//! - `--unit swev|report` 需要三件套（否则 OR1001）；`--unit stub` 与三件套
//!   互斥（显式冒烟不带执行参数）；
//! - `--unit report` 的单元材料面：任务包目录 `<data_root>/<task>/` 携
//!   task.json + plan.json（v1 约定：计划随任务包——submit 级 `--plan` 档
//!   形态=开放问题 8，不预判），registry_root 取 task.json 声明（缺省机器
//!   本地常量）。

use std::path::PathBuf;

use af_order::cli::{parse_args, Command};
use af_order::exec::{parse_unit_choice, report_unit_for, resolve_unit, RealExec, UnitChoice};

fn trio() -> Vec<String> {
    vec![
        "--spec".into(),
        "/tmp/s.json".into(),
        "--state".into(),
        "/tmp/st".into(),
        "--task".into(),
        "paper-task-1".into(),
        "--profile".into(),
        "glm-cp".into(),
        "--data-root".into(),
        "/tmp/pkgroot".into(),
    ]
}

#[test]
fn unit_choice_parses_three_values_and_rejects_garbage() {
    assert_eq!(parse_unit_choice("swev").unwrap(), UnitChoice::Swev);
    assert_eq!(parse_unit_choice("report").unwrap(), UnitChoice::Report);
    assert_eq!(parse_unit_choice("stub").unwrap(), UnitChoice::Stub);
    for bad in ["mono", "swev-x", ""] {
        let err = parse_unit_choice(bad).unwrap_err();
        assert!(
            err.to_string().contains("OR1001") || err.to_string().contains("--unit"),
            "{bad}: {err}"
        );
    }
}

#[test]
fn resolve_unit_default_behavior_is_unchanged() {
    // 缺省（Auto）：三件套在 → Swev；缺 → stub。
    let exec = RealExec {
        task: "django__django-11099".into(),
        profile: "glm-cp".into(),
        data_root: PathBuf::from("/tmp/swev-s1"),
        host: "anolis".into(),
        max_turns: Some(60),
        spend_cap_cents: Some(100),
    };
    assert_eq!(
        resolve_unit(UnitChoice::Auto, Some(&exec)).unwrap(),
        af_order::exec::SelectedUnit::Swev
    );
    assert_eq!(
        resolve_unit(UnitChoice::Auto, None).unwrap(),
        af_order::exec::SelectedUnit::Stub
    );
}

#[test]
fn resolve_unit_matrix_strict_sides() {
    let exec = RealExec {
        task: "paper-task-1".into(),
        profile: "glm-cp".into(),
        data_root: PathBuf::from("/tmp/pkgroot"),
        host: "local".into(),
        max_turns: Some(60),
        spend_cap_cents: Some(300),
    };
    // swev/report 缺三件套 → OR1001。
    for u in [UnitChoice::Swev, UnitChoice::Report] {
        let err = resolve_unit(u, None).unwrap_err();
        assert!(
            err.to_string().contains("--task") || err.to_string().contains("三件套"),
            "{u:?}: {err}"
        );
    }
    // stub 与三件套互斥。
    let err = resolve_unit(UnitChoice::Stub, Some(&exec)).unwrap_err();
    assert!(
        err.to_string().contains("互斥") || err.to_string().contains("stub"),
        "{err}"
    );
    // 显式选择成功臂。
    assert_eq!(
        resolve_unit(UnitChoice::Swev, Some(&exec)).unwrap(),
        af_order::exec::SelectedUnit::Swev
    );
    assert_eq!(
        resolve_unit(UnitChoice::Report, Some(&exec)).unwrap(),
        af_order::exec::SelectedUnit::Report
    );
    assert_eq!(
        resolve_unit(UnitChoice::Stub, None).unwrap(),
        af_order::exec::SelectedUnit::Stub
    );
}

#[test]
fn cli_accepts_unit_flag_and_keeps_default_absent() {
    // --unit report 进 Command；缺省缺如（Auto）。
    let mut args = trio();
    args.insert(0, "submit".into());
    args.extend(["--unit".into(), "report".into()]);
    let cmd = parse_args(&args).unwrap();
    match cmd {
        Command::Submit { unit, exec, .. } => {
            assert_eq!(unit, UnitChoice::Report);
            assert!(exec.is_some());
        }
        other => panic!("应解析为 Submit：{other:?}"),
    }
    // 缺省提交（无执行参数）unit == Auto。
    let cmd = parse_args(&[
        "submit".to_string(),
        "--spec".to_string(),
        "/tmp/s.json".into(),
        "--state".into(),
        "/tmp/st".into(),
    ])
    .unwrap();
    match cmd {
        Command::Submit { unit, exec, .. } => {
            assert_eq!(unit, UnitChoice::Auto);
            assert!(exec.is_none());
        }
        other => panic!("{other:?}"),
    }
    // 非法值 → 用法错。
    let mut args = trio();
    args.insert(0, "submit".into());
    args.extend(["--unit".into(), "mono".into()]);
    assert!(parse_args(&args).is_err());
}

#[test]
fn report_unit_materials_come_from_task_package() {
    // 任务包布局（v1 约定）：<data_root>/<task>/{task.json, plan.json}；
    // registry_root 取 task.json 声明，缺省机器本地常量。
    let base = std::env::temp_dir().join(format!("g4r-route-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    let task_dir = base.join("paper-task-1");
    std::fs::create_dir_all(&task_dir).unwrap();
    std::fs::write(
        task_dir.join("task.json"),
        serde_json::json!({
            "paper_url": "file:///tmp/paper.txt",
            "focus": "机制",
            "registry_root": "/tmp/some-packages",
            "rubric_params": { "min_citations": 3 }
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(task_dir.join("plan.json"), "{\"v\":1}").unwrap();

    let exec = RealExec {
        task: "paper-task-1".into(),
        profile: "glm-cp".into(),
        data_root: base.clone(),
        host: "local".into(),
        max_turns: Some(60),
        spend_cap_cents: Some(300),
    };
    let spec = report_unit_for(&exec).expect("任务包齐全应产 spec");
    assert_eq!(spec.profile, "glm-cp");
    assert_eq!(spec.registry_root, PathBuf::from("/tmp/some-packages"));
    assert_eq!(spec.reader_template_ref, "pkg.reportkit.reader_system");

    // 缺 plan.json → fail-loud（v1 约定文案点名）。
    std::fs::remove_file(task_dir.join("plan.json")).unwrap();
    let err = report_unit_for(&exec).unwrap_err();
    assert!(
        err.to_string().contains("plan.json") || err.to_string().contains("计划随任务包"),
        "{err}"
    );
    std::fs::remove_dir_all(&base).ok();
}
