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
