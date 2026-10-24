//! INV-3 生成器-验证器分离的编译期判定（提案 B §5，M3，v0.1 口径 §8-4）：
//!
//! 验证面 = kind ∈ {check, gate} ∪ human 标记节点（`NodeDecl.human` 在场）。
//! 验证节点的任一条入边的生产者不得是它自己（产出即验收 = 自验证）→ E2321。
//! 直接自环判定 = 所在体作用域内 `from == to == 本节点 id` 的边；间接自验证
//! （check → transform → check 的数据流回环）需要可达性分析，v0.1 不查
//! （提案 §8-4 开放问题，留后续——正例 `indirect_self_verification_not_
//! flagged_v01` 钉住该边界）。
//!
//! E2322（loop.safety_points 每节点必须是验证面）不在 M3 范围：P2-L04 切分
//! 仅列直接自环；实现 E2322 将使 M1 已交付夹具断言（恰 [E2312]）翻红——
//! 见 RESULT-M3 遗留表。

use super::Index;
use crate::error::{ErrorCode, Report};
use crate::model::NodeDecl;

/// 是否验证面节点（§5：kind ∈ {check, gate} ∪ human 标记节点）。
/// kind 是开放词表（M1 决策），check/gate 字面量判定与 mod.rs 内建
/// `passed` 端口的判定口径一致。
pub(crate) fn is_verifier(node: &NodeDecl) -> bool {
    matches!(node.kind.as_str(), "check" | "gate") || node.human.is_some()
}

pub(crate) fn check(ix: &Index<'_>, report: &mut Report) {
    for nref in ix.nodes.values() {
        if !is_verifier(nref.node) {
            continue;
        }
        // 本节点所在体作用域内的边（from/to 为本地 id，直接自环 = 两本地
        // id 相等且等于本节点）。
        for e in ix.edges.get(&nref.scope).copied().unwrap_or(&[]) {
            if e.to == nref.node.id && e.from == nref.node.id {
                report.push(
                    ErrorCode::SelfVerification,
                    &nref.scope,
                    format!(
                        "verifier node '{}' consumes port '{}' produced by itself（生成与验证必须不同节点）",
                        nref.path, e.input
                    ),
                );
            }
        }
    }
}
