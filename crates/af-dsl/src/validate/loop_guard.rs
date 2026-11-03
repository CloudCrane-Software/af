//! 受控循环守卫（fix2，P2-L08）：G2a 独立验证发现的三处校验缺口。
//!
//! 码位（提案 B §1.3 正本 + §6 码表）：
//! - E2002 max_iterations 必须 >0（值域——与 af-graph 共享码位，同族
//!   `af-graph/src/validate.rs` 「迭代上限必须为正」）；
//! - E2004 safety_points 必须非空（空集合——与 af-graph 共享码位；
//!   「缺安全点=拒绝编译」）；
//! - E2002 carried 每项至少一个上界（max_items/max_bytes，承 af-graph
//!   `CarriedState::is_bounded` 口径：至少其一，Some(0) 视为已声明）。
//!   与 E2211（carried 覆盖性）互补不重复：值域 vs 覆盖义务。
//!
//! E2322（安全点指向验证面节点）在 inv3（INV-3 族，`is_verifier` 所在）。
//! 一次收齐：按 Index 遍历序（文档序）逐 loop 三查，信息含节点路径定位。

use super::Index;
use crate::error::{ErrorCode, Report};

pub(crate) fn check(ix: &Index<'_>, report: &mut Report) {
    for site in &ix.loops {
        // 重复 id 时首声明者胜（E2003 已报），被跳过的节点不再定罪。
        let Some(owner) = ix.nodes.get(&site.owner_path) else {
            continue;
        };
        let decl = site.decl;
        let path = &site.owner_path;
        if decl.max_iterations == 0 {
            report.push(
                ErrorCode::ValueOutOfRange,
                &owner.scope,
                format!("loop {path} 的 max_iterations = 0（迭代上限必须 >0）"),
            );
        }
        if decl.safety_points.is_empty() {
            report.push(
                ErrorCode::EmptyCollection,
                &owner.scope,
                format!("loop {path} 未声明任何安全点（safety_points 为空，拒绝编译）"),
            );
        }
        for c in decl.carried.iter().flatten() {
            if c.max_items.is_none() && c.max_bytes.is_none() {
                report.push(
                    ErrorCode::ValueOutOfRange,
                    &owner.scope,
                    format!(
                        "loop {path} carried 条目 store {:?} 无上界（max_items/max_bytes 至少其一）",
                        c.store
                    ),
                );
            }
        }
    }
}
