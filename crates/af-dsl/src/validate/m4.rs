//! M4 语义 pass：CEL 保守子集静态判定的**全位点接线** + route 归一化构造
//! 校验 + topology_selector 编译期解析语义（提案 B §1.4/§1.5/§3.1 + 裁决
//! §二-1）。管道序（一次收齐）：CEL 位点判定 → route 归一化 → selector
//! 候选图核对。
//!
//! CEL 位点与位置词表（§1.4 词汇表 × 作用域）：
//! - `policies.stop[].when` / `meta.topology_selector.rules[].when`：全局五根
//!   （steps/budget/last_tool/last_message/input）——文档级无节点作用域链，
//!   节点 id 根不在其词表（窄解释，RESULT-M4 偏差表）。
//! - `route.rules[].when`：节点作用域链（内层优先，含节点自身）+ 验证面节点
//!   可写裸 `passed` + resume（链上首个 human 构造节点）。
//! - `loop.exit_when`：体根作用域链 + attempt（体内验证面内建 passed 与显式
//!   emits）+ item（链上 map 体的元素）+ resume。
//! - `check/gate assert`：节点作用域链 + inputs（本节点入边端口集，按生产者
//!   emit 类型建模）+ resume。
//!
//! 词表名（steps/budget/last_tool/last_message/input/resume/attempt/item/
//! inputs/passed）在可见位点上是保留字：词汇表绑定后写覆盖同名节点 id。
//!
//! `failures[]` 按 §1.2 无 CEL 字段（route 为目标串，E2204 已在 M1 判）。
//!
//! 抑制规则：route 的 E2101 在 E2102/E2103 在场时整体抑制（`route::normalize`
//! 内实现，承偏差⑨不重复定罪纪律）；求值错误不判（`cel` 模块级注记）。

use super::{Index, NodeRef, ValidateOptions};
use crate::cel::{self, CelEnv};
use crate::error::{ErrorCode, Report};
use crate::model::{Document, EmitSpec, PortType, PrimitiveKind, TypeExpr};
use cel_interpreter::Value;
use std::collections::{BTreeSet, HashMap};

/// §1.4 全局绑定四根的闭环字段（steps/budget 维度族 × BudgetDimension 词表）。
const STEPS_FIELDS: [&str; 4] = ["turns", "tool_free", "tool_calls_last_turn", "iterations"];
const BUDGET_DIMENSIONS: [&str; 8] = [
    "turns",
    "model_calls",
    "tokens_in",
    "tokens_out",
    "tokens_total",
    "tool_calls",
    "cents",
    "clock_ms",
];

/// emit/类型递归上限（与 refs 层一致）。
const TYPE_DEPTH: u8 = 32;

pub(crate) fn check(doc: &Document, ix: &Index<'_>, opts: &ValidateOptions, report: &mut Report) {
    cel_sites(doc, ix, report);
    routes(ix, report);
    selector_candidates(doc, opts, report);
}

// ---------- CEL 位点判定 ----------

fn cel_sites(doc: &Document, ix: &Index<'_>, report: &mut Report) {
    for nref in ix.nodes.values() {
        let node = nref.node;
        if let Some(assert) = &node.assert {
            let env = assert_env(doc, ix, nref);
            judge(
                assert,
                &env,
                &nref.scope,
                &format!("node '{}' assert", nref.path),
                report,
            );
        }
        if let Some(route) = &node.route {
            for (i, rule) in route.rules.iter().enumerate() {
                let env = route_env(doc, ix, nref);
                judge(
                    &rule.when,
                    &env,
                    &nref.scope,
                    &format!("node '{}' route rules[{i}].when", nref.path),
                    report,
                );
            }
        }
        if let Some(lp) = &node.loop_spec {
            if let Some(exit_when) = &lp.exit_when {
                let env = exit_when_env(doc, ix, nref);
                judge(
                    exit_when,
                    &env,
                    &nref.scope,
                    &format!("loop '{}' exit_when", nref.path),
                    report,
                );
            }
        }
    }

    // 文档级位点：policies.stop[].when 与 selector when——全局词表（无作用域链）。
    let global = global_env(doc);
    for (i, s) in doc.policies.stop.iter().enumerate() {
        judge(
            &s.when,
            &global,
            "policies.stop",
            &format!("policies.stop[{i}].when"),
            report,
        );
    }
    if let Some(sel) = &doc.meta.topology_selector {
        for (i, r) in sel.rules.iter().enumerate() {
            judge(
                &r.when,
                &global,
                "meta",
                &format!("meta.topology_selector rules[{i}].when"),
                report,
            );
        }
    }
}

fn judge(source: &str, env: &CelEnv, scope: &str, site: &str, report: &mut Report) {
    if let Err(e) = cel::check(source, env) {
        report.push(e.code, scope, format!("{site} CEL {}", e.message()));
    }
}

// ---------- 合成绑定上下文 ----------

/// 全局五根（§1.4：steps/budget/last_tool/last_message/input，全局可见）。
fn global_env(doc: &Document) -> CelEnv {
    let mut env = CelEnv::new();
    env.add(
        "steps",
        cel::vmap(STEPS_FIELDS.iter().map(|f| ((*f).to_string(), Value::Int(0)))),
    );
    env.add(
        "budget",
        cel::vmap(BUDGET_DIMENSIONS.iter().flat_map(|d| {
            [
                (format!("{d}_spent"), Value::Int(0)),
                (format!("{d}_cap"), Value::Int(1)),
            ]
        })),
    );
    env.add(
        "last_tool",
        cel::vmap([
            ("name".to_string(), Value::String(String::new().into())),
            ("first_line".to_string(), Value::String(String::new().into())),
            ("ok".to_string(), Value::Bool(true)),
        ]),
    );
    env.add(
        "last_message",
        cel::vmap([("has_tool_calls".to_string(), Value::Bool(false))]),
    );
    // input = meta.input schema 展开（类型未登记 → Null，E2205 域已另行定罪）。
    let input = match type_schema(doc, &doc.meta.input) {
        Some(s) => cel::synth_from_schema(s),
        None => Value::Null,
    };
    env.add("input", input);
    env
}

/// 节点作用域链（内层优先 → 根）上的节点 id 根（§1.4：内层优先；同名校内
/// 层胜——外层同名不再覆盖）。词汇表绑定由调用方随后覆盖（保留字纪律）。
fn add_scope_chain_roots(doc: &Document, ix: &Index<'_>, env: &mut CelEnv, from_scope: &str) {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for label in scope_chain(ix, from_scope) {
        let Some(scope) = ix.scopes.get(&label) else {
            continue;
        };
        for (id, path) in &scope.ids {
            if seen.insert(id.clone()) {
                if let Some(nref) = ix.nodes.get(path) {
                    env.add(id, node_value(doc, ix, nref));
                }
            }
        }
    }
}

/// resume：从 from_scope 向外第一个带 human 构造的节点（同域多 human 取
/// 确定序首个——BTree id 序，v0.1 无二义样例，RESULT-M4 登记）。
fn add_resume(ix: &Index<'_>, env: &mut CelEnv, from_scope: &str) {
    for label in scope_chain(ix, from_scope) {
        let Some(scope) = ix.scopes.get(&label) else {
            continue;
        };
        let hit = scope
            .ids
            .values()
            .filter_map(|p| ix.nodes.get(p))
            .find(|n| n.node.human.is_some());
        if let Some(h) = hit {
            let schema = &h.node.human.as_ref().expect("checked").resume.schema;
            env.add("resume", cel::synth_from_schema(schema));
            return;
        }
    }
}

/// attempt：体内验证面节点（check/gate）的内建 passed 与显式 emits 端口
/// （§1.4；「取最近一次验证输出」是运行期语义，静态合成取全集）。
fn add_attempt(doc: &Document, ix: &Index<'_>, env: &mut CelEnv, body_scope: &str) {
    let mut hm: HashMap<String, Value> = HashMap::new();
    hm.insert("passed".to_string(), Value::Bool(true));
    if let Some(scope) = ix.scopes.get(body_scope) {
        for path in scope.ids.values() {
            let Some(nref) = ix.nodes.get(path) else { continue };
            if matches!(nref.node.kind.as_str(), "check" | "gate") {
                for (port, spec) in nref.node.emits.iter().flatten() {
                    hm.insert(port.clone(), emit_port_value(doc, ix, nref, port, spec));
                }
            }
        }
    }
    env.add("attempt", cel::vmap(hm));
}

/// item：链上首个 map 体的元素（over 解析：input.字段 → schema items；
/// 节点.端口 → emit 类型剥 list<>。不可解析 → Null——E2104/E2105 域已定罪，
/// 不产 E2203 级联）。
fn add_item(doc: &Document, ix: &Index<'_>, env: &mut CelEnv, from_scope: &str) {
    for label in scope_chain(ix, from_scope) {
        if let Some(owner_path) = label.strip_suffix(".map_body") {
            let Some(map_node) = ix.nodes.get(owner_path) else { continue };
            let over = match &map_node.node.map {
                Some(m) => m.over.as_str(),
                None => continue,
            };
            env.add("item", synth_element(doc, ix, map_node, over));
            return;
        }
    }
}

/// map.over 的元素类型合成。
fn synth_element(doc: &Document, ix: &Index<'_>, map_node: &NodeRef<'_>, over: &str) -> Value {
    if let Some(field) = over.strip_prefix("input.") {
        let items = type_schema(doc, &doc.meta.input)
            .and_then(|s| s.get("properties"))
            .and_then(|p| p.get(field))
            .and_then(|prop| prop.get("items"));
        return items.map(cel::synth_from_schema).unwrap_or(Value::Null);
    }
    let Some((node_id, port)) = over.split_once('.') else {
        return Value::Null;
    };
    let element = match ix.scope_id(&map_node.scope, node_id) {
        Some(producer) => match ix.emit_type(producer, port, TYPE_DEPTH) {
            // over 必须 list（E2104 域核对）——剥一层取元素。
            Some(PortType::Named(t)) => match t.0 {
                TypeExpr::List(inner) => synth_expr(doc, &inner),
                _ => Value::Null,
            },
            _ => Value::Null,
        },
        None => Value::Null,
    };
    element
}

/// assert 位点：作用域链节点根 + 全局词表覆盖 + inputs（入边端口集）+ resume。
fn assert_env(doc: &Document, ix: &Index<'_>, nref: &NodeRef<'_>) -> CelEnv {
    let mut env = CelEnv::new();
    add_scope_chain_roots(doc, ix, &mut env, &nref.scope);
    overlay_global(doc, &mut env);
    add_resume(ix, &mut env, &nref.scope);
    add_item(doc, ix, &mut env, &nref.scope);

    // inputs：本节点入边端口集（按生产者 emit 类型建模；不可判定端口 → Null）。
    let mut hm: HashMap<String, Value> = HashMap::new();
    if let Some(edges) = ix.edges.get(&nref.scope) {
        for e in edges.iter().filter(|e| e.to == nref.node.id) {
            let v = match ix.scope_id(&nref.scope, &e.from) {
                Some(from) => match ix.emit_type(from, &e.output, TYPE_DEPTH) {
                    Some(pt) => synth_port(doc, &pt),
                    None => Value::Null,
                },
                None => Value::Null,
            };
            hm.insert(e.input.clone(), v);
        }
    }
    env.add("inputs", cel::vmap(hm));
    env
}

/// route 位点：作用域链节点根（含节点自身）+ 全局词表覆盖 + resume + 验证面
/// 节点的裸 `passed`。
fn route_env(doc: &Document, ix: &Index<'_>, nref: &NodeRef<'_>) -> CelEnv {
    let mut env = CelEnv::new();
    add_scope_chain_roots(doc, ix, &mut env, &nref.scope);
    overlay_global(doc, &mut env);
    add_resume(ix, &mut env, &nref.scope);
    add_item(doc, ix, &mut env, &nref.scope);
    if matches!(nref.node.kind.as_str(), "check" | "gate") {
        env.add("passed", Value::Bool(true));
    }
    env
}

/// exit_when 位点：体根作用域链 + 全局词表覆盖 + attempt + item + resume。
fn exit_when_env(doc: &Document, ix: &Index<'_>, loop_node: &NodeRef<'_>) -> CelEnv {
    let body_scope = format!("{}.loop_body", loop_node.path);
    let mut env = CelEnv::new();
    add_scope_chain_roots(doc, ix, &mut env, &body_scope);
    overlay_global(doc, &mut env);
    add_attempt(doc, ix, &mut env, &body_scope);
    add_item(doc, ix, &mut env, &body_scope);
    add_resume(ix, &mut env, &body_scope);
    env
}

/// 全局五根覆盖（保留字纪律：词汇表绑定覆盖同名节点 id）。
fn overlay_global(doc: &Document, env: &mut CelEnv) {
    let global = global_env(doc);
    for (name, value) in global.into_entries() {
        env.add(&name, value);
    }
}

/// 作用域标签链（from → … → root；体标签 → 拥有者节点所在作用域）。
fn scope_chain(ix: &Index<'_>, from: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = Some(from.to_string());
    while let Some(label) = cur {
        cur = parent_scope(ix, &label);
        out.push(label);
    }
    out
}

fn parent_scope(ix: &Index<'_>, label: &str) -> Option<String> {
    if label == "root" {
        return None;
    }
    let owner = label
        .strip_suffix(".loop_body")
        .or_else(|| label.strip_suffix(".map_body"))?;
    ix.nodes.get(owner).map(|n| n.scope.clone())
}

// ---------- 值合成 ----------

/// 节点根的合成值：内建端口（验证面 passed / 构造 done）+ emits 端口。
fn node_value(doc: &Document, ix: &Index<'_>, nref: &NodeRef<'_>) -> Value {
    let mut hm: HashMap<String, Value> = HashMap::new();
    if matches!(nref.node.kind.as_str(), "check" | "gate") {
        hm.insert("passed".to_string(), Value::Bool(true));
    }
    if nref.node.loop_spec.is_some() || nref.node.map.is_some() {
        hm.insert("done".to_string(), Value::Bool(true));
    }
    for (port, spec) in nref.node.emits.iter().flatten() {
        hm.insert(port.clone(), emit_port_value(doc, ix, nref, port, spec));
    }
    cel::vmap(hm)
}

fn emit_port_value(
    doc: &Document,
    ix: &Index<'_>,
    nref: &NodeRef<'_>,
    port: &str,
    spec: &EmitSpec,
) -> Value {
    match spec {
        EmitSpec::Type(t) => synth_port(doc, t),
        // from_body：经索引推导（map 包 list 的语义在 emit_type 内）；不可判
        // 定（E2106 域已定罪）→ Null。
        EmitSpec::FromBody { .. } => match ix.emit_type(nref, port, TYPE_DEPTH) {
            Some(pt) => synth_port(doc, &pt),
            None => Value::Null,
        },
    }
}

/// 端口类型 → 合成值（Named 类型经 types 表 schema；signal → Bool）。
fn synth_port(doc: &Document, pt: &PortType) -> Value {
    match pt {
        PortType::Inline(p) => match p.r#type {
            PrimitiveKind::String => Value::String(String::new().into()),
            PrimitiveKind::Integer => Value::Int(0),
            PrimitiveKind::Boolean => Value::Bool(true),
            PrimitiveKind::Number => Value::Float(0.0),
        },
        PortType::Named(t) => synth_expr(doc, &t.0),
    }
}

fn synth_expr(doc: &Document, e: &TypeExpr) -> Value {
    match e {
        TypeExpr::Signal => Value::Bool(true),
        TypeExpr::Named(n) => match type_schema(doc, n) {
            Some(s) => cel::synth_from_schema(s),
            None => Value::Null,
        },
        // list<T> → 单元素 list（成员选择与 @in 可求值）。
        TypeExpr::List(inner) => Value::List(vec![synth_expr(doc, inner)].into()),
        // envelope<T> → 载荷形（envelope 元数据层不在布尔判定的求值面内）。
        TypeExpr::Envelope(inner) => synth_expr(doc, inner),
    }
}

fn type_schema<'a>(doc: &'a Document, name: &str) -> Option<&'a serde_json::Value> {
    doc.types
        .iter()
        .find(|t| t.name == name)
        .map(|t| &t.schema)
}

// ---------- route 归一化接线 ----------

fn routes(ix: &Index<'_>, report: &mut Report) {
    for nref in ix.nodes.values() {
        let Some(route) = &nref.node.route else {
            continue;
        };
        let mut targets: BTreeSet<String> = BTreeSet::new();
        if let Some(edges) = ix.edges.get(&nref.scope) {
            for e in edges.iter().filter(|e| e.from == nref.node.id) {
                targets.insert(e.to.clone());
            }
        }
        if let Err(issues) = crate::route::normalize(route, &targets) {
            for issue in issues {
                let (code, message) = match &issue {
                    crate::route::RouteIssue::UncoveredTarget { target } => (
                        ErrorCode::RouteUncovered,
                        format!(
                            "node '{}' route 出边目标 {target:?} 未被任何规则或 default 覆盖",
                            nref.path
                        ),
                    ),
                    crate::route::RouteIssue::UnknownGoto { target, what } => (
                        ErrorCode::RouteGotoDangling,
                        format!(
                            "node '{}' route {what} goto {target:?} 不是其出边目标也不是 exit",
                            nref.path
                        ),
                    ),
                    crate::route::RouteIssue::DuplicateTarget { target, first, later } => (
                        ErrorCode::RouteUnreachable,
                        format!(
                            "node '{}' route 目标 {target:?} 被 {first} 与 {later} 重复命中（后中者不可达，请合并为 A || B）",
                            nref.path
                        ),
                    ),
                };
                report.push(code, &nref.scope, message);
            }
        }
    }
}

// ---------- topology_selector：编译期解析语义（裁决 §二-1） ----------

/// 候选图核对：每条规则 use 与 fallback 都必须是**版本化 name@version 形**
/// 且在已声明图集内（`ValidateOptions.declared_graphs`，包产物注入；默认
/// 空集 = 任何候选引用都判未声明）。码位：E2001 悬空引用共享码（§6 码表
/// 无 selector 专用码位，不私占空号）。
fn selector_candidates(doc: &Document, opts: &ValidateOptions, report: &mut Report) {
    let Some(sel) = &doc.meta.topology_selector else {
        return;
    };
    let mut refs: Vec<(String, &str)> = sel
        .rules
        .iter()
        .enumerate()
        .map(|(i, r)| (format!("rules[{i}].use"), r.use_graph.as_str()))
        .collect();
    refs.push(("fallback".to_string(), sel.fallback.as_str()));
    for (what, name) in refs {
        if !is_versioned(name) {
            report.push(
                ErrorCode::DanglingReference,
                "meta",
                format!("topology_selector {what} 候选图引用 {name:?} 不是版本化 name@version 形"),
            );
        } else if !opts.declared_graphs.contains(name) {
            report.push(
                ErrorCode::DanglingReference,
                "meta",
                format!("topology_selector {what} 候选图 {name:?} 未在已声明的版本化图/子图集内"),
            );
        }
    }
}

/// 版本化形：`名@版本`，两侧非空（版本字符集不收紧——meta.graph_version
/// 同为自由串，如 "1.0"）。
fn is_versioned(name: &str) -> bool {
    match name.split_once('@') {
        Some((n, v)) => !n.is_empty() && !v.is_empty(),
        None => false,
    }
}
