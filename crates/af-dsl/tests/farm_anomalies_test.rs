//! 农田异常样本回归（FV-1 第一波裁决 §三；语料 = `testdata/farm-anomalies/`
//! 36 份，逐样本根因/裁定见同目录 ANOMALIES.md 副本）。
//!
//! 期望裁定按裁决书 §三 的修复分项组织；本文件随修复逐项增长（TDD：每项
//! 先 RED 后 GREEN）。文档化边界样本（B-5/B-6 类，裁决「入档不立案」）钉
//! 现行为不变，防修复批误伤。

use af_dsl::{load_str, validate, Report};
use std::path::PathBuf;

/// 装载并校验一份农田样本，返回 Report（load 失败即 panic——这些样本都
/// 是信封/形状合法、语义层或崩溃面的案子）。
fn farm(name: &str) -> Report {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata/farm-anomalies")
        .join(name);
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let doc = load_str(&src).unwrap_or_else(|e| panic!("load_str {name}: {e}"));
    validate(&doc)
}

fn codes(r: &Report) -> Vec<&'static str> {
    r.codes()
}

/// 深嵌套过闸样本的判定须在大栈线程上跑：antlr4rust 递归下降每层括号吃
/// 数 KB 栈（农田实测 8MB 主栈深 1000 过/2000 溢出），测试线程栈不定，
/// 32MB 显式栈把「过闸即真过」的判定与环境解耦（RESULT-农田修复 §阈值）。
fn run_on_big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(f)
        .expect("spawn 256MB-stack thread")
        .join()
        .expect("big-stack thread panicked")
}

// ---------- P0-PANIC：nodes:[] 不得 panic，应报 E2001（裁决 §三-P0①） ----------

/// B60（`nodes: []` + 其余合法最小文档）：validate 契约「违例一次收齐」
/// （lib.rs `validate(&Document) -> Report`）——零节点时 `meta.entry`/`meta.
/// output.from` 悬空应各报一条 E2001，而非 `refs.rs` `scopes["root"]` 索引
/// panic（ANOMALIES.md A-1：`no entry found for key`）。
#[test]
fn b60_empty_nodes_reports_e2001_not_panic() {
    let report = farm("B60_nodes_empty.yaml");
    assert_eq!(
        codes(&report),
        vec!["E2001", "E2001"],
        "零节点文档：entry 与 output.from 双悬空各一条 E2001，一次收齐：\n{report}"
    );
}

// ---------- P0-CEL：深嵌套/超长 CEL 前置闸（裁决 §三-P0②） ----------

/// 4 份 CRASH 样本（ANOMALIES.md A-2：括号深 2000/100000、20 万段加链、
/// 90 万段加链——antlr4rust 栈溢出 SIGABRT，`catch_unwind` 兜不住）必须
/// 转正规错误码 E1054（表达式段 E1xxx 前置闸：嵌套深度/字节长双闸），
/// 不再 abort 进程。
#[test]
fn cel_crashers_report_e1054_not_sigabrt() {
    for name in [
        "C_CE15_parens_2000.yaml",
        "C_CE16_parens_100000.yaml",
        "C_CE19_plus_chain_200k.yaml",
        "B89_cel_900kb_chain.yaml",
    ] {
        let report = farm(name);
        assert_eq!(
            codes(&report),
            vec!["E1054"],
            "{name} 应以 E1054 前置闸拒绝（深/长超限），而非栈溢出 abort：\n{report}"
        );
    }
}

/// 阈值回归钉（ANOMALIES.md A-2 实测：深 500/深 1000 优雅 PASS）：前置闸
/// 不得误伤过闸样本——在 256MB 大栈线程上完整走 parse→判定。
#[test]
fn cel_threshold_passers_still_pass() {
    for name in ["C_CE28_parens_500.yaml", "C_CE29_parens_1000.yaml"] {
        let ok = run_on_big_stack(|| {
            let report = farm(name);
            (report.is_ok(), format!("{report}"))
        });
        assert!(ok.0, "{name} 深 500/1000 在阈值内，应保持 PASS：\n{}", ok.1);
    }
}

