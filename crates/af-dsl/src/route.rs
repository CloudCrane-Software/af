//! `route` 归一化（提案 B §1.5）：有序规则 + 必填 default 的**确定性重写**，
//! 互斥性与覆盖性由构造成立——无需任何图分析。
//!
//! ```text
//! guard_i = when_i ∧ ¬(when_1 ∨ … ∨ when_{i-1})    （优先级前缀：排除所有前序）
//! guard_d = ¬(when_1 ∨ … ∨ when_n)                 （default 取全否）
//! 边 N→t 携带命中 t 的第一条分支的 guard；goto: exit 的分支不产生边。
//! ```
//!
//! guard 的 CEL 重构形（确定性，逐字可测）：每条 when 恒加括号 `({when})`；
//! 前序析取 `({w0}) || ({w1}) …`；guard_0 = `({w0})`；guard_i =
//! `({wi}) && !({前序析取})`；guard_d = `!({全序析取})`（无规则时 `true`）。
//!
//! 构造校验（静态、一次收齐，§1.5 + §6）：E2101 出边未全覆盖 · E2102 goto
//! 非 out 边目标也非 exit · E2103 同一出边目标被多条分支命中（后中者不可达，
//! 请合并为 `A || B`）。E2103 的 §1.5 文面是「多条规则」；规则与 default 同
//! 目标在 first-hit 语义下同样丢失 default 分支的路由贡献——对称缺陷同码
//! （RESULT-M4 偏差表登记）。多条 exit 规则**良性**：exit 分支不产生边、无
//! 丢失可言，联合守卫按文档序析取。
//!
//! 本模块为纯函数（不触索引/文档）——归一化产物可独立单测，validate 管道
//! （`validate/m4.rs`）负责接线与违例收集。

use crate::model::RouteDecl;
use std::collections::{BTreeMap, BTreeSet};

/// exit 关键字（goto 的唯一非出边目标合法值）。
pub const EXIT: &str = "exit";

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

impl std::fmt::Display for GotoRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
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
/// 全部构造校验通过 → Ok；任一违例 → Err（违例一次收齐，序：E2102（规则序
/// → default）→ E2103（目标名序）→ E2101（目标名序，且在 E2102/E2103 在场
/// 时整体抑制——覆盖面不可信，不产噪声级联）。
pub fn normalize(
    route: &RouteDecl,
    out_targets: &BTreeSet<String>,
) -> Result<RouteNorm, Vec<RouteIssue>> {
    let mut issues = Vec::new();
    let valid = |g: &str| g == EXIT || out_targets.contains(g);

    // E2102：goto 悬空（规则文档序 → default）。
    for (i, r) in route.rules.iter().enumerate() {
        if !valid(&r.goto) {
            issues.push(RouteIssue::UnknownGoto {
                target: r.goto.clone(),
                what: GotoRef::Rule(i),
            });
        }
    }
    if !valid(&route.default) {
        issues.push(RouteIssue::UnknownGoto {
            target: route.default.clone(),
            what: GotoRef::Default,
        });
    }

    // E2103：同一出边目标（非 exit）被多条分支命中——首个命中者胜，后续
    // 分支对该边无贡献（后中者不可达）。
    let mut first_hit: BTreeMap<&str, GotoRef> = BTreeMap::new();
    for (i, r) in route.rules.iter().enumerate() {
        if r.goto != EXIT && out_targets.contains(&r.goto) {
            match first_hit.entry(r.goto.as_str()) {
                std::collections::btree_map::Entry::Vacant(e) => {
                    e.insert(GotoRef::Rule(i));
                }
                std::collections::btree_map::Entry::Occupied(e) => {
                    issues.push(RouteIssue::DuplicateTarget {
                        target: r.goto.clone(),
                        first: *e.get(),
                        later: GotoRef::Rule(i),
                    })
                }
            }
        }
    }
    if route.default != EXIT && out_targets.contains(&route.default) {
        if let Some(first) = first_hit.get(route.default.as_str()) {
            issues.push(RouteIssue::DuplicateTarget {
                target: route.default.clone(),
                first: *first,
                later: GotoRef::Default,
            });
        }
    }

    if !issues.is_empty() {
        return Err(issues);
    }

    // E2101：出边目标未被任何分支覆盖。抑制规则（承 RESULT-M1 偏差⑨纪律）：
    // 存在 E2102/E2103 时跳过——goto 悬空/重复已定罪，覆盖面不可信，不重复
    // 定罪产噪声级联。
    for t in out_targets {
        let covered = route.default == *t || route.rules.iter().any(|r| r.goto == *t);
        if !covered {
            issues.push(RouteIssue::UncoveredTarget { target: t.clone() });
        }
    }
    if !issues.is_empty() {
        return Err(issues);
    }

    // 分支 guard（规则 i 与 default；exit 分支同公式，只是不产边）。
    let disjunction = |k: usize| -> String {
        route.rules[..k]
            .iter()
            .map(|r| format!("({})", r.when))
            .collect::<Vec<_>>()
            .join(" || ")
    };
    let rule_guard = |i: usize| -> String {
        if i == 0 {
            format!("({})", route.rules[0].when)
        } else {
            format!("({}) && !({})", route.rules[i].when, disjunction(i))
        }
    };
    let default_guard = if route.rules.is_empty() {
        "true".to_string()
    } else {
        format!("!({})", disjunction(route.rules.len()))
    };

    // 边守卫：命中 t 的第一条规则的 guard_i，否则 default 的 guard_d。
    let mut edges = Vec::new();
    for t in out_targets {
        let guard = route
            .rules
            .iter()
            .position(|r| r.goto == *t)
            .map(rule_guard)
            .unwrap_or_else(|| default_guard.clone());
        edges.push(EdgeGuard {
            to: t.clone(),
            guard,
        });
    }

    // exit 分支联合守卫（文档序析取）。
    let exit_guard = {
        let exits: Vec<String> = route
            .rules
            .iter()
            .enumerate()
            .filter(|(_, r)| r.goto == EXIT)
            .map(|(i, _)| rule_guard(i))
            .collect();
        (!exits.is_empty()).then(|| exits.join(" || "))
    };

    Ok(RouteNorm { edges, exit_guard })
}
