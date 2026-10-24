//! 引用完整性（M1）：类型引用 / 节点引用 / store 引用 / 端口引用，全部一次收齐。
//!
//! 码位：E2001 悬空（入口/output.from/边端点/写者路径/writes/reads/carried）·
//! E2104 over 非 list · E2105 over·args 悬空 · E2106 from_body 悬空 ·
//! E2107 safety_point 悬空 · E2201 边端口与两端类型 · E2202 output.from 不发射 ·
//! E2204 failures 路由悬空 · E2205 未知类型名 · E2207 预算 cap·作用域 ·
//! E2208 retention 无界。
//!
//! 抑制规则（避免噪声，见 RESULT-M1）：类型表里有未登记名（E2205 已报）时，
//! E2202 与 E2201 的类型相等核对跳过——比较对象本身不可信，不重复定罪。

use super::{construct_body_scope, Index, NodeRef};
use crate::error::{ErrorCode, Report};
use crate::model::{Document, EmitSpec, PortType, TypeExpr};

/// 端口类型递归上限（from_body 链防环）。
const TYPE_DEPTH: u8 = 32;

pub(crate) fn check(doc: &Document, ix: &Index<'_>, report: &mut Report) {
    check_meta(doc, ix, report);
    check_stores(ix, report);
    check_nodes(doc, ix, report);
    check_edges(ix, report);
    check_policies(doc, ix, report);
}

// ---------- meta ----------

fn check_meta(doc: &Document, ix: &Index<'_>, report: &mut Report) {
    if !ix.scopes["root"].ids.contains_key(&doc.meta.entry) {
        report.push(
            ErrorCode::DanglingReference,
            "root",
            format!("meta.entry 指向未声明节点 {:?}", doc.meta.entry),
        );
    }
    let from = match ix.resolve_root_path(&doc.meta.output.from) {
        Some(f) => f,
        None => {
            report.push(
                ErrorCode::DanglingReference,
                "root",
                format!("meta.output.from 指向未声明节点 {:?}", doc.meta.output.from),
            );
            return;
        }
    };
    if !ix.type_names.contains(&doc.meta.input) {
        report.push(
            ErrorCode::UnknownTypeName,
            "meta",
            format!("meta.input 引用未登记类型名 {:?}", doc.meta.input),
        );
    }
    // E2202：from 节点必须发射 output.ref 所指类型。
    // 抑制：ref 或 from 的任一 emit 类型未登记（E2205 域）或 from_body 不可解析
    // （E2106 域）时不判——无法证明「不发射」。
    if !type_resolves(ix, &doc.meta.output.r#ref) {
        return;
    }
    let Some(emits) = &from.node.emits else {
        report.push(
            ErrorCode::OutputNotEmitted,
            "root",
            format!(
                "meta.output.from {:?} 无任何 emits 端口，无法发射 {}",
                from.path,
                doc.meta.output.r#ref.display()
            ),
        );
        return;
    };
    let mut matched = false;
    let mut undecidable = false;
    for (port, spec) in emits {
        match spec {
            EmitSpec::Type(t) => {
                if !type_resolves(ix, t) {
                    undecidable = true;
                } else if *t == doc.meta.output.r#ref {
                    matched = true;
                }
            }
            EmitSpec::FromBody { .. } => match ix.emit_type(from, port, TYPE_DEPTH) {
                Some(t) if t == doc.meta.output.r#ref => matched = true,
                Some(_) => {}
                None => undecidable = true,
            },
        }
    }
    if !matched && !undecidable {
        report.push(
            ErrorCode::OutputNotEmitted,
            "root",
            format!(
                "meta.output.from {:?} 不发射类型 {}（其端口类型均不等）",
                from.path,
                doc.meta.output.r#ref.display()
            ),
        );
    }
}

/// 类型引用是否全部可解析（names 表内；内建 signal 与内联原始类型恒真）。
fn type_resolves(ix: &Index<'_>, t: &PortType) -> bool {
    match t {
        PortType::Inline(_) => true,
        PortType::Named(t) => expr_resolves(ix, &t.0),
    }
}

fn expr_resolves(ix: &Index<'_>, e: &TypeExpr) -> bool {
    match e {
        TypeExpr::Signal => true,
        TypeExpr::Named(n) => ix.type_names.contains(n),
        TypeExpr::List(inner) | TypeExpr::Envelope(inner) => expr_resolves(ix, inner),
    }
}

fn report_unresolved(ix: &Index<'_>, t: &PortType, report: &mut Report, scope: &str, what: &str) {
    if let PortType::Named(t) = t {
        report_unresolved_expr(ix, &t.0, report, scope, what);
    }
}

fn report_unresolved_expr(
    ix: &Index<'_>,
    e: &TypeExpr,
    report: &mut Report,
    scope: &str,
    what: &str,
) {
    match e {
        TypeExpr::Signal => {}
        TypeExpr::Named(n) => {
            if !ix.type_names.contains(n) {
                report.push(
                    ErrorCode::UnknownTypeName,
                    scope,
                    format!("{what} 引用未登记类型名 {n:?}"),
                );
            }
        }
        TypeExpr::List(inner) | TypeExpr::Envelope(inner) => {
            report_unresolved_expr(ix, inner, report, scope, what)
        }
    }
}

// ---------- stores ----------

fn check_stores(ix: &Index<'_>, report: &mut Report) {
    for (name, s) in &ix.stores {
        if s.retention.max_items.is_none() && s.retention.max_bytes.is_none() {
            report.push(
                ErrorCode::StoreRetentionUnbounded,
                "stores",
                format!("store {name:?} retention 无上界（max_items/max_bytes 至少其一）"),
            );
        }
        if let Some(item) = &s.item {
            report_unresolved(ix, item, report, "stores", &format!("store {name:?}.item"));
        }
        if let Some(v) = &s.value {
            report_unresolved(ix, v, report, "stores", &format!("store {name:?}.value"));
        }
        if ix.resolve_root_path(&s.writer).is_none() {
            report.push(
                ErrorCode::DanglingReference,
                "stores",
                format!(
                    "store {name:?} 的 writer 路径 {:?} 经 loop/map 体解析失败",
                    s.writer
                ),
            );
        }
    }
}

// ---------- nodes ----------

fn check_nodes(doc: &Document, ix: &Index<'_>, report: &mut Report) {
    for nref in ix.nodes.values() {
        let node = nref.node;
        for w in node.writes.iter().flatten() {
            if !ix.stores.contains_key(w) {
                report.push(
                    ErrorCode::DanglingReference,
                    &nref.scope,
                    format!("节点 {:?} writes 未声明的 store {:?}", nref.path, w),
                );
            }
        }
        for r in node.reads.iter().flatten() {
            if !ix.stores.contains_key(r) {
                report.push(
                    ErrorCode::DanglingReference,
                    &nref.scope,
                    format!("节点 {:?} reads 未声明的 store {:?}", nref.path, r),
                );
            }
        }
        if let Some(consumes) = &node.consumes {
            for (port, t) in consumes {
                report_unresolved(
                    ix,
                    t,
                    report,
                    &nref.scope,
                    &format!("节点 {:?} consumes.{port}", nref.path),
                );
            }
        }
        if let Some(emits) = &node.emits {
            for (port, spec) in emits {
                match spec {
                    EmitSpec::Type(t) => report_unresolved(
                        ix,
                        t,
                        report,
                        &nref.scope,
                        &format!("节点 {:?} emits.{port}", nref.path),
                    ),
                    EmitSpec::FromBody { from_body } => {
                        check_from_body(ix, nref, port, from_body, report);
                    }
                }
            }
        }
        if let Some(mp) = &node.map {
            check_over(doc, ix, nref, &mp.over, report);
        }
        if let Some(lp) = &node.loop_spec {
            for (arg, value) in lp.args.iter().flatten() {
                check_arg_ref(doc, ix, nref, arg, value, report);
            }
            for c in lp.carried.iter().flatten() {
                if !ix.stores.contains_key(&c.store) {
                    report.push(
                        ErrorCode::DanglingReference,
                        &nref.scope,
                        format!(
                            "loop {:?} carried 引用未声明的 store {:?}",
                            nref.path, c.store
                        ),
                    );
                }
            }
            let body_scope = format!("{}.loop_body", nref.path);
            for sp in &lp.safety_points {
                if ix.resolve_path(&body_scope, &sp.node).is_none() {
                    report.push(
                        ErrorCode::SafetyPointDangling,
                        &nref.scope,
                        format!(
                            "loop {:?} 安全点路径 {:?} 从体根逐层解析失败",
                            nref.path, sp.node
                        ),
                    );
                }
            }
        }
    }
}

/// emits.from_body：体内 节点.端口 必须可解析（E2106）。
fn check_from_body(
    ix: &Index<'_>,
    owner: &NodeRef<'_>,
    port: &str,
    from_body: &str,
    report: &mut Report,
) {
    let Some(body_scope) = construct_body_scope(owner) else {
        report.push(
            ErrorCode::FromBodyDangling,
            &owner.scope,
            format!(
                "节点 {:?} 的 emits.{port} 带 from_body，但该节点无 loop/map 构造体",
                owner.path
            ),
        );
        return;
    };
    let Some((target_id, tport)) = from_body.rsplit_once('.') else {
        report.push(
            ErrorCode::FromBodyDangling,
            &owner.scope,
            format!(
                "节点 {:?} 的 emits.{port} 的 from_body {:?} 不是 节点.端口 形",
                owner.path, from_body
            ),
        );
        return;
    };
    match ix.scope_id(&body_scope, target_id) {
        None => report.push(
            ErrorCode::FromBodyDangling,
            &owner.scope,
            format!(
                "节点 {:?} 的 emits.{port} 的 from_body 目标 {:?} 不在体内",
                owner.path, target_id
            ),
        ),
        Some(target) => {
            if !ix.port_exists(target, tport) {
                report.push(
                    ErrorCode::FromBodyDangling,
                    &owner.scope,
                    format!(
                        "节点 {:?} 的 emits.{port} 的 from_body 目标端口 {:?}.{tport:?} 不存在",
                        owner.path, target_id
                    ),
                );
            }
        }
    }
}

/// map.over：`input.字段` 或 同作用域 `节点.端口`（E2105），且必须是 list（E2104）。
fn check_over(
    doc: &Document,
    ix: &Index<'_>,
    map_node: &NodeRef<'_>,
    over: &str,
    report: &mut Report,
) {
    if let Some(field) = over.strip_prefix("input.") {
        match input_property(doc, field) {
            // schema 无 properties 可查 → 不判定（E2205 域已另行报 meta.input 问题）。
            None if !input_has_properties(doc) => {}
            None => report.push(
                ErrorCode::ArgOverRefDangling,
                &map_node.scope,
                format!(
                    "map {:?} 的 over {over:?}：meta.input schema 无字段 {field:?}",
                    map_node.path
                ),
            ),
            Some(prop) => {
                if let Some(t) = prop.get("type").and_then(|t| t.as_str()) {
                    if t != "array" {
                        report.push(
                            ErrorCode::MapOverNotList,
                            &map_node.scope,
                            format!(
                                "map {:?} 的 over {over:?} 解析到 schema 字段类型 {t:?}，不是 array",
                                map_node.path
                            ),
                        );
                    }
                }
            }
        }
        return;
    }
    let Some((node_id, port)) = over.split_once('.') else {
        report.push(
            ErrorCode::ArgOverRefDangling,
            &map_node.scope,
            format!(
                "map {:?} 的 over {:?} 必须是 节点.端口 或 input.字段 形",
                map_node.path, over
            ),
        );
        return;
    };
    match ix.scope_id(&map_node.scope, node_id) {
        None => report.push(
            ErrorCode::ArgOverRefDangling,
            &map_node.scope,
            format!(
                "map {:?} 的 over 引用同作用域未声明节点 {:?}",
                map_node.path, node_id
            ),
        ),
        Some(producer) => {
            if !ix.port_exists(producer, port) {
                report.push(
                    ErrorCode::ArgOverRefDangling,
                    &map_node.scope,
                    format!(
                        "map {:?} 的 over 引用节点 {:?} 的不存在端口 {:?}",
                        map_node.path, node_id, port
                    ),
                );
                return;
            }
            match ix.emit_type(producer, port, TYPE_DEPTH) {
                Some(PortType::Named(t)) if matches!(t.0, TypeExpr::List(_)) => {}
                Some(t) => report.push(
                    ErrorCode::MapOverNotList,
                    &map_node.scope,
                    format!(
                        "map {:?} 的 over {over:?} 端口类型 {} 不是 list",
                        map_node.path,
                        t.display()
                    ),
                ),
                None => {} // 不可判定（from_body 悬空已由 E2106 报）
            }
        }
    }
}

/// loop.args 值：`节点.端口[.字段…]` 或 `input.字段[…]`（E2105；字段尾部不判）。
fn check_arg_ref(
    doc: &Document,
    ix: &Index<'_>,
    owner: &NodeRef<'_>,
    arg: &str,
    value: &str,
    report: &mut Report,
) {
    if let Some(field) = value.strip_prefix("input.") {
        let field = field.split('.').next().unwrap_or(field);
        if input_has_properties(doc) && input_property(doc, field).is_none() {
            report.push(
                ErrorCode::ArgOverRefDangling,
                &owner.scope,
                format!(
                    "loop {:?} 的 args.{arg} 引用 meta.input 不存在的字段 {field:?}",
                    owner.path
                ),
            );
        }
        return;
    }
    let mut segs = value.split('.');
    let (node_id, port) = match (segs.next(), segs.next()) {
        (Some(n), Some(p)) => (n, p),
        _ => {
            report.push(
                ErrorCode::ArgOverRefDangling,
                &owner.scope,
                format!(
                    "loop {:?} 的 args.{arg} 值 {value:?} 必须是 节点.端口[…] 或 input.字段[…] 形",
                    owner.path
                ),
            );
            return;
        }
    };
    match ix.scope_id(&owner.scope, node_id) {
        None => report.push(
            ErrorCode::ArgOverRefDangling,
            &owner.scope,
            format!(
                "loop {:?} 的 args.{arg} 引用同作用域未声明节点 {:?}",
                owner.path, node_id
            ),
        ),
        Some(producer) => {
            if !ix.port_exists(producer, port) {
                report.push(
                    ErrorCode::ArgOverRefDangling,
                    &owner.scope,
                    format!(
                        "loop {:?} 的 args.{arg} 引用节点 {:?} 的不存在端口 {port:?}",
                        owner.path, node_id
                    ),
                );
            }
        }
    }
}

/// meta.input schema 的字段（无 properties/字段缺失 → None）。pub(crate)：
/// inv1 层的 input.fields/vars 引用核对共用。
pub(crate) fn input_property<'a>(doc: &'a Document, field: &str) -> Option<&'a serde_json::Value> {
    doc.types
        .iter()
        .find(|t| t.name == doc.meta.input)?
        .schema
        .get("properties")?
        .get(field)
}

pub(crate) fn input_has_properties(doc: &Document) -> bool {
    doc.types
        .iter()
        .find(|t| t.name == doc.meta.input)
        .is_some_and(|t| t.schema.get("properties").is_some())
}

// ---------- edges ----------

fn check_edges(ix: &Index<'_>, report: &mut Report) {
    for (label, edges) in &ix.edges {
        let edges = *edges;
        for e in edges {
            let from = ix.scope_id(label, &e.from);
            let to = ix.scope_id(label, &e.to);
            if from.is_none() {
                report.push(
                    ErrorCode::DanglingReference,
                    label,
                    format!("边引用未声明节点 from={:?}", e.from),
                );
            }
            if to.is_none() {
                report.push(
                    ErrorCode::DanglingReference,
                    label,
                    format!("边引用未声明节点 to={:?}", e.to),
                );
            }
            let (Some(from), Some(to)) = (from, to) else {
                continue;
            };
            if !ix.port_exists(from, &e.output) {
                report.push(
                    ErrorCode::EdgePortTypeMismatch,
                    label,
                    format!(
                        "边 {} → {}：生产者 {:?} 无输出端口 {:?}（含内建 done/passed）",
                        e.from, e.to, from.path, e.output
                    ),
                );
                continue;
            }
            let Some(consumes) = &to.node.consumes else {
                continue;
            };
            let Some(ct) = consumes.get(&e.input) else {
                report.push(
                    ErrorCode::EdgePortTypeMismatch,
                    label,
                    format!(
                        "边 {} → {}：消费者 {:?} 声明了 consumes，但入边端口 {:?} 不在其中",
                        e.from, e.to, to.path, e.input
                    ),
                );
                continue;
            };
            if let Some(pt) = ix.emit_type(from, &e.output, TYPE_DEPTH) {
                // 抑制：类型未登记（E2205 域）时跳过相等核对。
                if type_resolves(ix, ct) && type_resolves(ix, &pt) && pt != *ct {
                    report.push(
                        ErrorCode::EdgePortTypeMismatch,
                        label,
                        format!(
                            "边 {}.{:?} → {}.{:?}：端口类型 {} 与消费者声明 {} 不相等",
                            e.from,
                            e.output,
                            e.to,
                            e.input,
                            pt.display(),
                            ct.display()
                        ),
                    );
                }
            }
        }
    }
}

// ---------- policies ----------

fn check_policies(doc: &Document, ix: &Index<'_>, report: &mut Report) {
    for b in &doc.policies.budgets {
        if b.cap == 0 {
            report.push(
                ErrorCode::BudgetCapOrScopeInvalid,
                "policies.budgets",
                format!("预算 {:?} cap = 0（必须为正）", b.id),
            );
        }
        let scope_ok = match b.scope.strip_prefix("node:") {
            Some(id) => ix.resolve_root_path(id).is_some(),
            None => match b.scope.strip_prefix("loop:") {
                Some(id) => ix
                    .resolve_root_path(id)
                    .is_some_and(|n| n.node.loop_spec.is_some()),
                None => match b.scope.strip_prefix("map:") {
                    Some(id) => ix
                        .resolve_root_path(id)
                        .is_some_and(|n| n.node.map.is_some()),
                    None => b.scope == "plan",
                },
            },
        };
        if !scope_ok {
            report.push(
                ErrorCode::BudgetCapOrScopeInvalid,
                "policies.budgets",
                format!(
                    "预算 {:?} 作用域 {:?} 悬空或非法（合法：plan | node:<id> | loop:<id>（须有 loop 构造）| map:<id>（须有 map 构造））",
                    b.id, b.scope
                ),
            );
        }
    }
    const ROUTE_KEYWORDS: [&str; 2] = ["same_node", "next_iteration"];
    for f in &doc.policies.failures {
        for (kind, target) in [("route", &f.route), ("fallback_route", &f.fallback_route)] {
            if let Some(t) = target {
                if !ROUTE_KEYWORDS.contains(&t.as_str()) && !ix.nodes.contains_key(t) {
                    report.push(
                        ErrorCode::FailureRouteDangling,
                        "policies.failures",
                        format!(
                            "失败类 {:?} 的 {kind} 目标 {t:?} 既非关键词 {ROUTE_KEYWORDS:?} 也非已声明节点",
                            f.class
                        ),
                    );
                }
            }
        }
    }
}
