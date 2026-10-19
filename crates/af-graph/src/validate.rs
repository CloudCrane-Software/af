//! 编译期校验器。五条规则，一次收集全部违例（编译期检查器应当一次报完）：
//!
//! ① `Cycle`                   —— 静态图必须是 DAG（循环体子图豁免：环由受控循环接管）
//! ② `Reachability`            —— 从入口可达；孤立节点报错
//! ③ `UndeclaredReference`     —— 未声明节点引用报错（边端点 / 入口 / 子图名 / 节点 id 重复 / 安全点指向）
//! ④ `LoopNotControlled`       —— 受控循环缺安全点、缺有界性、迭代上限非法 → 拒绝
//! ⑤ `MapSubgraphUnversioned`  —— map 的子图必须已版本化（graph_version 有值）

use crate::model::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    Cycle,
    Reachability,
    UndeclaredReference,
    LoopNotControlled,
    MapSubgraphUnversioned,
}

impl Rule {
    pub fn name(&self) -> &'static str {
        match self {
            Rule::Cycle => "cycle(环检测)",
            Rule::Reachability => "reachability(入口可达性)",
            Rule::UndeclaredReference => "undeclared-reference(未声明节点引用)",
            Rule::LoopNotControlled => "loop-not-controlled(受控循环缺安全点/有界性)",
            Rule::MapSubgraphUnversioned => "map-subgraph-unversioned(map子图未版本化)",
        }
    }
}

/// 稳定错误码：编译期违例的外部契约码，与内核/装载器日志对齐。
///
/// E2001–E2004 与 `verify-load` 装载器分类学（`REPORT.md` ④-3）语义一致——尤其
/// **E2001 悬空引用是跨域共享码**：装载器里是 combo→规则 id / andon→评估函数，
/// 这里同强度覆盖图模板的 节点/子图/工件(map.over)/安全点 引用。E2005–E2007 是
/// 图模板域专属扩展（配方装载器无对应项），不占用装载器已定义的码位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// E2001 悬空引用：引用的目标（节点/子图/工件/安全点）不存在 → 拒绝编译。
    DanglingReference,
    /// E2002 值域/约束违约：循环预算、携带状态有界性、安全点类型不符。
    ValueOutOfRange,
    /// E2003 id 重复：同一作用域内节点 id 重复声明。
    DuplicateId,
    /// E2004 非空违约：受控循环未声明安全点（safety_points 为空）。
    EmptyCollection,
    /// E2005 静态图存在环（graph 域专属扩展）。
    CyclicGraph,
    /// E2006 孤立节点：从入口不可达（graph 域专属扩展）。
    UnreachableNode,
    /// E2007 map 体子图未版本化（graph 域专属扩展）。
    UnversionedMapBody,
}

impl ErrorCode {
    /// 稳定错误码字符串（测试与内核日志按此断言）。
    pub fn code(&self) -> &'static str {
        match self {
            ErrorCode::DanglingReference => "E2001",
            ErrorCode::ValueOutOfRange => "E2002",
            ErrorCode::DuplicateId => "E2003",
            ErrorCode::EmptyCollection => "E2004",
            ErrorCode::CyclicGraph => "E2005",
            ErrorCode::UnreachableNode => "E2006",
            ErrorCode::UnversionedMapBody => "E2007",
        }
    }

    /// 错误大类名（结构化日志用，对齐装载器 `kind_name`）。
    pub fn name(&self) -> &'static str {
        match self {
            ErrorCode::DanglingReference => "DANGLING_REFERENCE",
            ErrorCode::ValueOutOfRange => "VALUE_OUT_OF_RANGE",
            ErrorCode::DuplicateId => "DUPLICATE_ID",
            ErrorCode::EmptyCollection => "EMPTY_COLLECTION",
            ErrorCode::CyclicGraph => "CYCLIC_GRAPH",
            ErrorCode::UnreachableNode => "UNREACHABLE_NODE",
            ErrorCode::UnversionedMapBody => "UNVERSIONED_MAP_BODY",
        }
    }
}

/// 一条违例。`scope` 标明发生在根图还是哪个子图；`error_code` 是外部契约码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: Rule,
    pub scope: String,
    pub message: String,
    pub error_code: ErrorCode,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{} {}] {}: {}",
            self.error_code.code(),
            self.error_code.name(),
            self.scope,
            self.message
        )
    }
}

/// 校验报告：全部违例的集合。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub violations: Vec<Violation>,
}

impl Report {
    pub fn is_ok(&self) -> bool {
        self.violations.is_empty()
    }
    pub fn of_rule(&self, r: Rule) -> Vec<&Violation> {
        self.violations.iter().filter(|v| v.rule == r).collect()
    }
    /// 非空且全部违例都属于同一条规则（用于"非法样例各踩一条规则"的断言）。
    pub fn only_rule(&self, r: Rule) -> bool {
        !self.violations.is_empty() && self.violations.iter().all(|v| v.rule == r)
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_ok() {
            write!(f, "OK")
        } else {
            for v in &self.violations {
                writeln!(f, "{v}")?;
            }
            Ok(())
        }
    }
}

/// 一个图作用域（根图或子图）做 DAG/可达性/引用检查所需的视图。
struct ScopeView<'a> {
    /// 作用域路径，如 "graph" / "subgraph:revise_loop"。
    path: String,
    entry: &'a str,
    nodes: &'a [Node],
    edges: &'a [Edge],
    /// 是否要求 DAG。循环体子图为 false（环由循环构造接管）。
    dag_required: bool,
}

/// 校验整个图模板，返回全部违例。
pub fn validate(t: &GraphTemplate) -> Report {
    let mut out = Vec::new();

    // 循环体子图集合：任何作用域内任何节点 loop_spec.body 指到的子图名。
    let loop_bodies: BTreeSet<&str> = all_nodes(t)
        .filter_map(|n| n.loop_spec.as_ref().map(|l| l.body.as_str()))
        .collect();

    // 作用域视图：根图 + 每个子图。循环体子图豁免 DAG 检查。
    let mut scopes = vec![ScopeView {
        path: "graph".to_string(),
        entry: &t.entry,
        nodes: &t.nodes,
        edges: &t.edges,
        dag_required: true,
    }];
    for (name, sg) in &t.subgraphs {
        scopes.push(ScopeView {
            path: format!("subgraph:{name}"),
            entry: &sg.entry,
            nodes: &sg.nodes,
            edges: &sg.edges,
            dag_required: !loop_bodies.contains(name.as_str()),
        });
    }

    for s in &scopes {
        check_scope(s, &mut out);
    }
    check_loops(t, &mut out);
    check_maps(t, &mut out);

    Report { violations: out }
}

/// 根图与全部子图中的所有节点（作用域扁平化，供规则④⑤扫描）。
fn all_nodes(t: &GraphTemplate) -> impl Iterator<Item = &Node> {
    t.nodes
        .iter()
        .chain(t.subgraphs.values().flat_map(|sg| sg.nodes.iter()))
}

/// 对单个作用域做规则③（引用完整性）→①（DAG）→②（入口可达性）。
fn check_scope(s: &ScopeView, out: &mut Vec<Violation>) {
    // ---- 规则③：未声明节点引用 ----
    let mut ids: BTreeSet<&str> = BTreeSet::new();
    for n in s.nodes {
        if !ids.insert(n.id.as_str()) {
            out.push(Violation {
                rule: Rule::UndeclaredReference,
                scope: s.path.clone(),
                error_code: ErrorCode::DuplicateId,
                message: format!("节点 id 重复声明: {}", n.id),
            });
        }
    }
    let entry_declared = ids.contains(s.entry);
    if !entry_declared {
        out.push(Violation {
            rule: Rule::UndeclaredReference,
            scope: s.path.clone(),
            error_code: ErrorCode::DanglingReference,
            message: format!("入口节点未声明: {}", s.entry),
        });
    }
    // 只拿两端都声明的边建邻接表——悬空边不污染 DAG/可达性判断。
    let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for e in s.edges {
        if !ids.contains(e.from.as_str()) || !ids.contains(e.to.as_str()) {
            let bad = if ids.contains(e.from.as_str()) {
                &e.to
            } else {
                &e.from
            };
            out.push(Violation {
                rule: Rule::UndeclaredReference,
                scope: s.path.clone(),
                error_code: ErrorCode::DanglingReference,
                message: format!("边 {} 引用了未声明节点: {}", e, bad),
            });
            continue;
        }
        adj.entry(e.from.as_str()).or_default().push(e.to.as_str());
    }

    // ---- 规则③（工件名级）：map.over 的 "节点.工件" 前缀节点必须已声明 ----
    // 收口 RESULT.md ⑤ 遗留"工件名级引用完整性未做"里可判定的那一半：edge.output/input
    // 的工件存在性需要"节点产出/消费工件表"（模型未建，仍遗留），但 map.over 的前缀是
    // 对某节点产出的引用——前缀节点必须存在于本作用域，否则 E2001 拒绝。
    for n in s.nodes {
        let Some(mp) = &n.map_spec else { continue };
        match mp.over.split_once('.') {
            Some((node, artifact)) if !node.is_empty() && !artifact.is_empty() => {
                if !ids.contains(node) {
                    out.push(Violation {
                        rule: Rule::UndeclaredReference,
                        scope: s.path.clone(),
                        error_code: ErrorCode::DanglingReference,
                        message: format!(
                            "map.over `{}` 引用的节点 `{}` 未声明（应为 '节点.工件名'，前缀节点须在本作用域）",
                            mp.over, node
                        ),
                    });
                }
            }
            _ => out.push(Violation {
                rule: Rule::UndeclaredReference,
                scope: s.path.clone(),
                error_code: ErrorCode::DanglingReference,
                message: format!(
                    "map.over `{}` 格式非法：必须形如 '节点.工件名'（节点与工件名均非空）",
                    mp.over
                ),
            }),
        }
    }

    // ---- 规则①：环检测（循环体子图豁免）----
    if s.dag_required {
        if let Some(cycle) = find_cycle(&ids, &adj) {
            out.push(Violation {
                rule: Rule::Cycle,
                scope: s.path.clone(),
                error_code: ErrorCode::CyclicGraph,
                message: format!("静态图存在环: {}", cycle.join(" -> ")),
            });
        }
    }

    // ---- 规则②：从入口的可达性 ----
    if entry_declared {
        let reached = reachable(s.entry, &adj);
        for id in &ids {
            if !reached.contains(*id) {
                out.push(Violation {
                    rule: Rule::Reachability,
                    scope: s.path.clone(),
                    error_code: ErrorCode::UnreachableNode,
                    message: format!("孤立节点（从入口不可达）: {id}"),
                });
            }
        }
    }
}

/// 规则④：受控循环缺安全点/有界性 → 拒绝。
fn check_loops(t: &GraphTemplate, out: &mut Vec<Violation>) {
    for (scope, n) in scoped_nodes(t) {
        let Some(lp) = &n.loop_spec else { continue };
        // 循环体子图必须已声明（归入规则③的"未声明引用" = E2001）。
        if !t.subgraphs.contains_key(&lp.body) {
            out.push(Violation {
                rule: Rule::UndeclaredReference,
                scope: scope.clone(),
                error_code: ErrorCode::DanglingReference,
                message: format!("循环体子图未声明: {}", lp.body),
            });
            continue; // 体都没有，安全点无从查起
        }
        // 迭代上限必须为正。
        if lp.max_iterations == 0 {
            out.push(Violation {
                rule: Rule::LoopNotControlled,
                scope: scope.clone(),
                error_code: ErrorCode::ValueOutOfRange,
                message: "循环 max_iterations 必须为正，当前为 0".to_string(),
            });
        }
        // 携带状态必须有界。
        if !lp.carried_state.is_bounded() {
            out.push(Violation {
                rule: Rule::LoopNotControlled,
                scope: scope.clone(),
                error_code: ErrorCode::ValueOutOfRange,
                message: format!(
                    "循环携带状态 `{}` 无界：max_items 与 max_bytes 至少声明一个",
                    lp.carried_state.name
                ),
            });
        }
        // 必须声明安全点。
        if lp.safety_points.is_empty() {
            out.push(Violation {
                rule: Rule::LoopNotControlled,
                scope: scope.clone(),
                error_code: ErrorCode::EmptyCollection,
                message: "受控循环未声明安全点（safety_points 为空），拒绝编译".to_string(),
            });
        }
        // 每个安全点必须真实落在循环体子图里，且类型对得上。
        let body = &t.subgraphs[&lp.body];
        let body_ids: BTreeSet<&str> = body.nodes.iter().map(|b| b.id.as_str()).collect();
        for sp in &lp.safety_points {
            let Some(bn) = body.nodes.iter().find(|b| b.id == sp.node) else {
                out.push(Violation {
                    rule: Rule::UndeclaredReference,
                    scope: scope.clone(),
                    error_code: ErrorCode::DanglingReference,
                    message: format!(
                        "安全点 {} 指向的节点不在循环体子图 `{}` 中（体中节点: {:?}）",
                        sp.node,
                        lp.body,
                        body_ids.iter().collect::<Vec<_>>()
                    ),
                });
                continue;
            };
            let kind_ok = match sp.kind {
                SafetyPointKind::Check => bn.kind == kind::CHECK,
                SafetyPointKind::Gate => bn.kind == kind::GATE,
                SafetyPointKind::Human => bn.human.is_some(),
            };
            if !kind_ok {
                out.push(Violation {
                    rule: Rule::LoopNotControlled,
                    scope: scope.clone(),
                    error_code: ErrorCode::ValueOutOfRange,
                    message: format!(
                        "安全点 {} 类型不符：要求 {:?}，但体中该节点 kind=`{}` 且 human 标记={}",
                        sp.node,
                        sp.kind,
                        bn.kind,
                        bn.human.is_some()
                    ),
                });
            }
        }
    }
}

/// 规则⑤：map 的子图必须已版本化。
fn check_maps(t: &GraphTemplate, out: &mut Vec<Violation>) {
    for (scope, n) in scoped_nodes(t) {
        let Some(mp) = &n.map_spec else { continue };
        match t.subgraphs.get(&mp.body) {
            None => out.push(Violation {
                rule: Rule::UndeclaredReference,
                scope: scope.clone(),
                error_code: ErrorCode::DanglingReference,
                message: format!("map 体子图未声明: {}", mp.body),
            }),
            Some(sg) if sg.graph_version.is_none() => out.push(Violation {
                rule: Rule::MapSubgraphUnversioned,
                scope: scope.clone(),
                error_code: ErrorCode::UnversionedMapBody,
                message: format!(
                    "map 体子图 `{}` 未版本化（graph_version 缺失）：运行时实例化 N 份等于现场改拓扑，必须先审定",
                    mp.body
                ),
            }),
            Some(_) => {}
        }
    }
}

/// (作用域, 节点) 对，供规则④⑤扫描根图与所有子图。
fn scoped_nodes(t: &GraphTemplate) -> impl Iterator<Item = (String, &Node)> {
    let root = t.nodes.iter().map(|n| ("graph".to_string(), n));
    let subs = t.subgraphs.iter().flat_map(|(name, sg)| {
        sg.nodes
            .iter()
            .map(move |n| (format!("subgraph:{name}"), n))
    });
    root.chain(subs)
}

/// 三色 DFS 找环，返回环路径（如 [a, b, c, a]）。图规模为模板级（几十节点），递归足够。
fn find_cycle(ids: &BTreeSet<&str>, adj: &BTreeMap<&str, Vec<&str>>) -> Option<Vec<String>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }
    let mut color: BTreeMap<&str, Color> = ids.iter().map(|&i| (i, Color::White)).collect();
    let mut stack: Vec<&str> = Vec::new();

    fn visit<'a>(
        u: &'a str,
        color: &mut BTreeMap<&'a str, Color>,
        adj: &BTreeMap<&'a str, Vec<&'a str>>,
        stack: &mut Vec<&'a str>,
    ) -> Option<Vec<String>> {
        color.insert(u, Color::Gray);
        stack.push(u);
        for &v in adj.get(u).into_iter().flatten() {
            match color.get(v).copied().unwrap_or(Color::White) {
                Color::Gray => {
                    // 找到环：从栈中 v 的位置截到栈顶，再回到 v。
                    let start = stack.iter().position(|&x| x == v).unwrap_or(0);
                    let mut cyc: Vec<String> =
                        stack[start..].iter().map(|s| s.to_string()).collect();
                    cyc.push(v.to_string());
                    return Some(cyc);
                }
                Color::White => {
                    if let Some(c) = visit(v, color, adj, stack) {
                        return Some(c);
                    }
                }
                Color::Black => {}
            }
        }
        stack.pop();
        color.insert(u, Color::Black);
        None
    }

    for &id in ids.iter() {
        if color[id] == Color::White {
            stack.clear();
            if let Some(c) = visit(id, &mut color, adj, &mut stack) {
                return Some(c);
            }
        }
    }
    None
}

/// 从入口 BFS，返回可达节点集合。
fn reachable<'a>(entry: &'a str, adj: &BTreeMap<&'a str, Vec<&'a str>>) -> BTreeSet<&'a str> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut q = VecDeque::new();
    q.push_back(entry);
    seen.insert(entry);
    while let Some(u) = q.pop_front() {
        for &v in adj.get(u).into_iter().flatten() {
            if seen.insert(v) {
                q.push_back(v);
            }
        }
    }
    seen
}
