//! INV-2 写入串行化的编译期判定（提案 B §5）：
//!
//! ① 每 store 恰有一个写者——`store.writer` 声明与全部节点 `writes[]` 引用
//!    （点号路径解析后）交叉核对。E2311：实际写者 ≥2，或唯一实际写者 ≠ 声明值。
//!    （零写者不报：非串行化违例——声明值与 writes 的一致性归 E2xxx 后续模块，
//!    见 RESULT-M1 偏差表。写者路径悬空已由 refs 层 E2001 报，此处跳过该 store。）
//! ② map 体节点不得写体外声明的 store（N 并行实例 = N 写者）→ E2312。
//!    （v0.1 体只能是内联 nodes+edges，store 一律体外声明——提案 B §8-3。）
//! ③ loop 的 carried 必须覆盖「体内（含嵌套体）写入的、体外声明的全部 store」
//!    → E2211。体内节点按路径前缀 "{owner}." 判定（嵌套体递归包含）。

use super::Index;
use crate::error::{ErrorCode, Report};
use std::collections::BTreeSet;

pub(crate) fn check(ix: &Index<'_>, report: &mut Report) {
    // ① + ②：写者闭包交叉核对。
    for (name, store) in &ix.stores {
        // 写者路径悬空已由 refs 层 E2001 报，此处跳过（无法核对声明值）。
        let Some(declared) = ix.resolve_root_path(&store.writer) else {
            continue;
        };
        let actual: Vec<&super::NodeRef<'_>> = ix
            .nodes
            .values()
            .filter(|n| n.node.writes.iter().flatten().any(|w| w == name))
            .collect();
        // ② map 体节点写共享槽（与 ① 独立判定，提案 B §5 inv2 样例即单码触发）。
        for w in &actual {
            if w.in_map {
                report.push(
                    ErrorCode::MapSharedStoreWrite,
                    &w.scope,
                    format!(
                        "map 体节点 {:?} 写体外声明的 store {name:?}（N 并行实例将并发写；改由汇聚节点单写）",
                        w.path
                    ),
                );
            }
        }
        // ① 写者集合势 = 1 且等于声明值。文案按违例实况分叉（红队 W1-C26）：
        // 多写者 = 「不唯一」；恰一写者但不匹配 = 「与声明不符」（该分支实际
        // 写者集恰一个，断言「不唯一」与事实不符、误导排障方向）。
        let paths: Vec<&str> = actual.iter().map(|n| n.path.as_str()).collect();
        if paths.len() > 1 {
            report.push(
                ErrorCode::StoreMultiWriter,
                "stores",
                format!(
                    "store {name:?} 写者不唯一：声明 {:?}，实际写者 {paths:?}",
                    declared.path
                ),
            );
        } else if paths.len() == 1 && paths[0] != declared.path {
            report.push(
                ErrorCode::StoreMultiWriter,
                "stores",
                format!(
                    "store {name:?} 写者与声明不符：声明 {:?}，实际写者 {paths:?}",
                    declared.path
                ),
            );
        }
    }
    // ③ carried 覆盖核对。
    for site in &ix.loops {
        let mut written_inside: BTreeSet<&str> = BTreeSet::new();
        let prefix = format!("{}.", site.owner_path);
        for n in ix.nodes.values() {
            if !n.path.starts_with(&prefix) {
                continue;
            }
            for w in n.node.writes.iter().flatten() {
                if ix.stores.contains_key(w) {
                    written_inside.insert(w.as_str());
                }
            }
        }
        let carried: BTreeSet<&str> = site
            .decl
            .carried
            .iter()
            .flatten()
            .map(|c| c.store.as_str())
            .collect();
        for s in written_inside {
            if !carried.contains(s) {
                report.push(
                    ErrorCode::CarriedNotCovering,
                    format!("{}.loop_body", site.owner_path),
                    format!(
                        "loop {:?} 体内（含嵌套体）写入体外声明的 store {s:?}，但 carried 未登记",
                        site.owner_path
                    ),
                );
            }
        }
    }
}
