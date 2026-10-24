//! `route` 归一化（提案 B §1.5）：有序规则 + 必填 default 的**确定性重写**，
//! 互斥性与覆盖性由构造成立——无需任何图分析。
//!
//! guard_i = when_i ∧ ¬(when_1 ∨ … ∨ when_{i-1})；guard_d = ¬(when_1 ∨ … ∨ when_n)；
//! 边 N→t 携带命中 t 的第一条分支的 guard；`goto: exit` 的分支不产生边。
//! 产物 = 边守卫集（目标 → CEL 守卫串，确定序）+ exit 分支联合守卫。
//!
//! 构造校验（静态、一次收齐）：E2101 出边未全覆盖 · E2102 goto 非 out 边目标
//! 也非 exit · E2103 同一目标被多条分支命中（后中者不可达，请合并为 `A || B`）。
//!
//! **RED 骨架**：归一化算法在 GREEN 期实装；本文件先立 API 面。

use crate::model::RouteDecl;
use std::collections::BTreeSet;

/// 一条归一化边守卫：目标节点 id + CEL 守卫串（guard 公式的确定性重构）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeGuard {
    pub to: String,
    pub guard: String,
}

/// 归一化产物：边守卫集（按目标名确定序）+ exit 分支联合守卫（无 exit 分支
/// 则 None；多条 exit 规则按文档序析取）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteNorm {
    pub edges: Vec<EdgeGuard>,
    pub exit_guard: Option<String>,
}

/// goto 来源（E2102/E2103 信息用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GotoRef {
    Rule(usize),
    Default,
}

impl GotoRef {
    pub fn display(&self) -> String {
        match self {
            Self::Rule(i) => format!("rules[{i}]"),
            Self::Default => "default".to_string(),
        }
    }
}

/// 构造违例（E2101/E2102/E2103 的结构化形态；validate 管道转 Violation）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteIssue {
    /// E2101：出边目标未被任何规则/default 覆盖。
    UncoveredTarget { target: String },
    /// E2102：goto 既非出边目标也非 exit。
    UnknownGoto { target: String, what: GotoRef },
    /// E2103：同一出边目标被多条分支命中（后中者不可达）。
    DuplicateTarget {
        target: String,
        first: GotoRef,
        later: GotoRef,
    },
}

/// 归一化：route（rules + default）× 出边目标集 → 边守卫集。
/// 全部构造校验通过 → Ok；任一违例 → Err（全部违例一次收齐）。
pub fn normalize(_route: &RouteDecl, _out_targets: &BTreeSet<String>) -> Result<RouteNorm, Vec<RouteIssue>> {
    // RED 骨架：未实装——正例恒过（空产物）、反例恒漏。
    Ok(RouteNorm {
        edges: Vec::new(),
        exit_guard: None,
    })
}
