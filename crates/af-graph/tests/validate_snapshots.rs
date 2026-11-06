//! afg validate 的违规清单（文本）与决策工件（YAML）快照。
//!
//! 快照文件在 `tests/snapshots/*.snap`，直接入库。生成或更新时用
//! `INSTA_UPDATE=always`，不走 `cargo insta review`。
//!
//! 三条覆盖：干净产线零违规、E2001 引用断裂、未知节点类型。
//! 未知 kind 是开放扩展（RESULT.md：不设白名单），单独出现为零违规；
//! 它撞上既有安全点类型约束时，违例码是 E2002，不是一条新的"未知类型"码。

use af_graph::{format_validate_stdout, validate, GraphTemplate, ValidateDecision};
use std::path::Path;
use std::process::Command;

fn load(name: &str) -> GraphTemplate {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(format!("{name}.json"));
    let src = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取 {p:?}: {e}"));
    serde_json::from_str(&src).unwrap_or_else(|e| panic!("解析 {name}.json: {e}"))
}

/// 跑 `afg validate <fixtures/{name}.json>`，并核对标准输出与库内格式化函数一致。
fn run_afg(name: &str) -> (i32, String, ValidateDecision) {
    let rel = format!("fixtures/{name}.json");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(env!("CARGO_BIN_EXE_afg"))
        .current_dir(root)
        .args(["validate", &rel])
        .output()
        .unwrap_or_else(|e| panic!("启动 afg: {e}"));
    let stderr =
        String::from_utf8(output.stderr).unwrap_or_else(|e| panic!("stderr 非 utf-8: {e}"));
    assert!(stderr.is_empty(), "stderr 应为空，实际:\n{stderr}");
    let stdout =
        String::from_utf8(output.stdout).unwrap_or_else(|e| panic!("stdout 非 utf-8: {e}"));
    let code = output.status.code().unwrap_or(-1);

    let template = load(name);
    let report = validate(&template);
    assert_eq!(
        stdout,
        format_validate_stdout(&rel, &template, &report),
        "CLI 违规清单应与 format_validate_stdout 一致"
    );
    let decision = ValidateDecision::from_report(&template, &report);
    assert_eq!(decision.violation_count, report.violations.len());
    assert_eq!(
        decision.verdict,
        if report.is_ok() { "accept" } else { "reject" }
    );
    (code, stdout, decision)
}

#[test]
fn clean_pipeline_zero_violations() {
    let (code, stdout, decision) = run_afg("legal-1-pipeline-human");
    assert_eq!(code, 0);
    assert!(
        stdout.starts_with("PASS "),
        "干净产线应 PASS，实际:\n{stdout}"
    );
    assert!(
        !stdout.contains("E200"),
        "零违规清单不应出现错误码:\n{stdout}"
    );
    assert_eq!(decision.verdict, "accept");
    assert_eq!(decision.violation_count, 0);
    assert!(decision.violations.is_empty());
    assert!(decision.unknown_kinds.is_empty());
    insta::assert_snapshot!("clean_pipeline_violation_list", stdout);
    insta::assert_yaml_snapshot!("clean_pipeline_decision", decision);
}

#[test]
fn e2001_dangling_reference() {
    let (code, stdout, decision) = run_afg("illegal-3-undeclared-ref");
    assert_eq!(code, 1);
    assert!(stdout.contains("E2001"), "应报 E2001，实际:\n{stdout}");
    assert!(
        stdout.contains("DANGLING_REFERENCE"),
        "应带错误类名，实际:\n{stdout}"
    );
    assert!(
        stdout.contains("notify_slack"),
        "应点名悬空节点，实际:\n{stdout}"
    );
    assert_eq!(decision.violations.len(), 1);
    assert_eq!(decision.violations[0].code, "E2001");
    assert_eq!(decision.violations[0].name, "DANGLING_REFERENCE");
    assert!(decision.unknown_kinds.is_empty());
    insta::assert_snapshot!("e2001_violation_list", stdout);
    insta::assert_yaml_snapshot!("e2001_decision", decision);
}

#[test]
fn unknown_kind_open_extension_zero_violations() {
    // 未知 kind（rerank）不是违例：与 unknown_node_kind_is_open_extension 同一结论。
    let (code, stdout, decision) = run_afg("unknown-kind-accepted");
    assert_eq!(code, 0);
    assert!(
        !stdout.contains("E200"),
        "未知 kind 单独出现应为零违规:\n{stdout}"
    );
    assert_eq!(decision.verdict, "accept");
    assert_eq!(decision.violation_count, 0);
    assert_eq!(decision.unknown_kinds, vec!["rerank".to_string()]);
    insta::assert_snapshot!("unknown_kind_accepted_violation_list", stdout);
    insta::assert_yaml_snapshot!("unknown_kind_accepted_decision", decision);
}

#[test]
fn unknown_kind_type_mismatch_e2002() {
    // 未知 kind 撞上安全点类型约束：既有规则④，错误码 E2002，不是新的未知类型码。
    let (code, stdout, decision) = run_afg("unknown-kind-type-mismatch");
    assert_eq!(code, 1);
    assert!(
        stdout.contains("E2002"),
        "类型不符应是 E2002，实际:\n{stdout}"
    );
    assert!(
        stdout.contains("VALUE_OUT_OF_RANGE"),
        "应带错误类名，实际:\n{stdout}"
    );
    assert!(
        stdout.contains("rerank"),
        "应点名未知 kind，实际:\n{stdout}"
    );
    assert!(
        stdout.contains("类型不符"),
        "应说明类型不符，实际:\n{stdout}"
    );
    assert_eq!(decision.violations.len(), 1);
    assert_eq!(decision.violations[0].code, "E2002");
    assert_eq!(decision.unknown_kinds, vec!["rerank".to_string()]);
    insta::assert_snapshot!("unknown_kind_type_mismatch_violation_list", stdout);
    insta::assert_yaml_snapshot!("unknown_kind_type_mismatch_decision", decision);
}
