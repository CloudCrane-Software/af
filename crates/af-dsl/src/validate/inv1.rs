//! context.sections 语义校验 + INV-1 上下文完整性的编译期判定（提案 B §1.2/§5/§6，M2）：
//!
//! ① 来源可解析：input.fields ∈ meta.input schema、edge.port ∈ 本节点入边端口集、
//!    store 名 ∈ stores 表（v0.1 体内不声明 store，作用域链 = 顶层表）、各 from 的
//!    必配来源字段在场（template.template_ref / input.fields / edge.port /
//!    store.store / arg.name）→ E2301。
//! ② 视图策略域：store 段必填 full|latest_k|retrieve|summarize|none 且参数闭合
//!    （latest_k/retrieve 须 args.k 正整数、retrieve/summarize 须 component）；
//!    input/arg/edge 段策略缺省合法、出现须恰为 project；template 段不得带策略
//!    → E2401。
//! ③ 配额：每段 quota_tokens 为正（例外：store 段 policy:none 记 0——提案 §7-5
//!    词法变通）→ E2302；可选总量上限（ValidateOptions，默认不核对——§5 无总量
//!    规则）超限同码 E2302、信息注明「总量」（§6 无独立码位，偏差登记 RESULT-M2）。
//! ④ arg ∈ 所在体形参表：沿作用域链向外收集（loop args + map 隐式 item）→ E2402。
//! ⑤ template vars 引用：input.字段 / arg.名 可解析（字面量不判）→ E2403。
//! ⑥ INV-1：LLM-bound 节点（uses.model 在场）禁 reads[] → E2303；入边端口恰被
//!    一个 from:edge 段引用（零引用 = 实际消费未声明；多引用违「恰一个」）→ E2304，
//!    判定对象 = 仅 LLM-bound 节点（提案 B §5 主语；非 LLM 节点入边是程序性
//!    数据载荷，不作上下文装配核对——G3 复核 2026-10-24 收窄）。
//!
//! 抑制规则（承 RESULT-M1 偏差⑨纪律）：from 不在五词表内时 M1 check_vocab 已报
//! E1004，本层整段跳过（不产噪声级联）；meta.input schema 无 properties 时
//! input 字段核对跳过（E2205 域已报）；reads 引用未声明 store（refs 层 E2001
//! 已定罪）时 E2303 只对已声明项判定。

use super::refs::{input_has_properties, input_property};
use super::{Index, NodeRef, ValidateOptions};
use crate::error::{ErrorCode, Report};
use crate::model::{Document, SectionDecl};
use std::collections::{BTreeMap, BTreeSet};

/// store 视图策略闭环词表（提案 B §1.2 context.sections 表）。
pub(crate) const STORE_VIEW_POLICIES: [&str; 5] =
    ["full", "latest_k", "retrieve", "summarize", "none"];

pub(crate) fn check(doc: &Document, ix: &Index<'_>, opts: &ValidateOptions, report: &mut Report) {
    for nref in ix.nodes.values() {
        check_node(doc, ix, nref, opts, report);
    }
}

fn check_node(
    doc: &Document,
    ix: &Index<'_>,
    nref: &NodeRef<'_>,
    opts: &ValidateOptions,
    report: &mut Report,
) {
    let node = nref.node;
    let llm_bound = node.uses.as_ref().is_some_and(|u| u.model.is_some());

    // 本节点入边端口（所在体作用域内 to == 本节点 id 的边的 input；去重保序）。
    let in_ports: BTreeSet<&str> = ix
        .edges
        .get(&nref.scope)
        .copied()
        .unwrap_or(&[])
        .iter()
        .filter(|e| e.to == node.id)
        .map(|e| e.input.as_str())
        .collect();
    // 入边端口 → from:edge 段引用计数（E2304 判定用；仅记真实入边端口）。
    let mut port_refs: BTreeMap<&str, usize> = in_ports.iter().map(|p| (*p, 0)).collect();

    if let Some(ctx) = &node.context {
        let formals = visible_formals(ix, &nref.scope);
        let sec = Sec {
            doc,
            ix,
            nref,
            in_ports: &in_ports,
            formals: &formals,
        };
        for (i, s) in ctx.sections.iter().enumerate() {
            check_section(&sec, i, s, &mut port_refs, report);
        }
        // 可配置总量上限（默认不核对；policy:none 段按 0 计入）。
        if let Some(cap) = opts.context_quota_cap {
            let total: u64 = ctx
                .sections
                .iter()
                .map(|s| {
                    if s.from == "store" && s.policy.as_deref() == Some("none") {
                        0
                    } else {
                        s.quota_tokens
                    }
                })
                .fold(0u64, u64::saturating_add);
            if total > cap {
                report.push(
                    ErrorCode::QuotaNotPositive,
                    &nref.scope,
                    format!(
                        "node '{}' context 配额总量 {total} 超可配置上限 {cap}（policy:none 段按 0 计入）",
                        nref.path
                    ),
                );
            }
        }
    }

    // INV-1：LLM-bound 节点不得声明 reads（store 只能经 sections 进上下文）。
    // 抑制：reads 中引用未声明 store 的项已由 refs 层 E2001 定罪，E2303 只对
    // 已声明的项判定（承 RESULT-M1 偏差⑨「不重复定罪」纪律——纯未知引用的
    // 文档已被 E2001 拒绝，双码只产噪声）。
    if llm_bound && node.reads.as_ref().is_some_and(|r| !r.is_empty()) {
        let declared: Vec<&String> = node
            .reads
            .as_ref()
            .unwrap()
            .iter()
            .filter(|r| ix.stores.contains_key(*r))
            .collect();
        if !declared.is_empty() {
            let list = format!(
                "[{}]",
                declared
                    .iter()
                    .map(|s| format!("'{s}'"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            report.push(
                ErrorCode::AgentDirectStoreRead,
                &nref.scope,
                format!(
                    "LLM-bound node '{}' declares reads {list}（store 只能经 context.sections 进入 agent 上下文）",
                    nref.path
                ),
            );
        }
    }

    // INV-1：入边端口恰被一个 section 引用。判定对象 = 仅 LLM-bound 节点
    //（LLM-bound 且无 context 时，入边即「实际消费但未声明」）。提案 B §5
    // 规则主语 = LLM-bound task 节点；§1 节点表 context「仅 LLM-bound task
    // 可用（INV-1 判定对象）」。非 LLM 节点（check/gate/transform 等）的
    // 入边端口是程序性数据载荷，不是上下文装配面，不判 E2304（G3 复核
    // 2026-10-24 收窄，原「带 context ∪ LLM-bound」过宽——见 RESULT-M2
    // 偏差 5 修订）。
    if llm_bound {
        for (port, refs) in &port_refs {
            if *refs == 0 {
                report.push(
                    ErrorCode::InputPortUnreferenced,
                    &nref.scope,
                    format!(
                        "node '{}' 入边端口 '{port}' 未被任何 context.sections 段引用（上下文 = 且仅 = sections 声明，无隐式注入）",
                        nref.path
                    ),
                );
            } else if *refs > 1 {
                report.push(
                    ErrorCode::InputPortUnreferenced,
                    &nref.scope,
                    format!(
                        "node '{}' 入边端口 '{port}' 被 {refs} 个 section 引用（须恰被一个）",
                        nref.path
                    ),
                );
            }
        }
    }
}

/// 单段校验的只读上下文（节点级共享）。
struct Sec<'a> {
    doc: &'a Document,
    ix: &'a Index<'a>,
    nref: &'a NodeRef<'a>,
    /// 本节点入边端口集。
    in_ports: &'a BTreeSet<&'a str>,
    /// 所在体形参可见集（沿作用域链向外收集）。
    formals: &'a BTreeSet<String>,
}

/// 单段校验：来源可解析（E2301）→ 策略域（E2401）→ 配额（E2302）→
/// arg 形参（E2402）→ vars 引用（E2403）。from 不在词表内整段跳过（E1004 已报）。
fn check_section(
    sec: &Sec<'_>,
    i: usize,
    s: &SectionDecl,
    port_refs: &mut BTreeMap<&str, usize>,
    report: &mut Report,
) {
    let doc = sec.doc;
    let ix = sec.ix;
    let nref = sec.nref;
    let in_ports = sec.in_ports;
    let formals = sec.formals;
    match s.from.as_str() {
        "template" => {
            if s.template_ref.as_deref().is_none_or(|t| t.is_empty()) {
                report.push(
                    ErrorCode::ContextSourceUnresolvable,
                    &nref.scope,
                    format!(
                        "node '{}' section[{i}] from 'template' 缺来源字段 template_ref",
                        nref.path
                    ),
                );
            }
            if let Some(p) = &s.policy {
                report.push(
                    ErrorCode::StoreViewPolicyUnknown,
                    &nref.scope,
                    format!(
                        "node '{}' section[{i}] policy '{p}' 不适用于 from 'template'（template 段无 policy）",
                        nref.path
                    ),
                );
            }
            if let Some(vars) = &s.vars {
                for (var, val) in vars {
                    check_var_ref(doc, nref, i, var, val, formals, report);
                }
            }
        }
        "input" => {
            match &s.fields {
                None => report.push(
                    ErrorCode::ContextSourceUnresolvable,
                    &nref.scope,
                    format!(
                        "node '{}' section[{i}] from 'input' 缺来源字段 fields[]",
                        nref.path
                    ),
                ),
                Some(fields) => {
                    // 抑制：meta.input schema 无 properties（E2205 域已报）时不判。
                    if input_has_properties(doc) {
                        for f in fields {
                            if input_property(doc, f).is_none() {
                                report.push(
                                    ErrorCode::ContextSourceUnresolvable,
                                    &nref.scope,
                                    format!(
                                        "node '{}' section[{i}] field '{f}' not in meta.input schema ({})",
                                        nref.path, doc.meta.input
                                    ),
                                );
                            }
                        }
                    }
                }
            }
            check_projection_policy(nref, i, s, report);
        }
        "arg" => {
            match &s.name {
                None => report.push(
                    ErrorCode::ContextSourceUnresolvable,
                    &nref.scope,
                    format!(
                        "node '{}' section[{i}] from 'arg' 缺来源字段 name",
                        nref.path
                    ),
                ),
                Some(name) => {
                    if !formals.contains(name) {
                        report.push(
                            ErrorCode::ArgNotInParams,
                            &nref.scope,
                            format!(
                                "node '{}' section[{i}] arg '{name}' 不在所在体形参表（可见形参：[{}]）",
                                nref.path,
                                formals_labels(formals)
                            ),
                        );
                    }
                }
            }
            check_projection_policy(nref, i, s, report);
        }
        "edge" => {
            match &s.port {
                None => report.push(
                    ErrorCode::ContextSourceUnresolvable,
                    &nref.scope,
                    format!(
                        "node '{}' section[{i}] from 'edge' 缺来源字段 port",
                        nref.path
                    ),
                ),
                Some(port) => {
                    if !in_ports.contains(port.as_str()) {
                        let list = in_ports
                            .iter()
                            .map(|p| format!("'{p}'"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        report.push(
                            ErrorCode::ContextSourceUnresolvable,
                            &nref.scope,
                            format!(
                                "node '{}' section[{i}] port '{port}' not in incoming edge ports [{list}]",
                                nref.path
                            ),
                        );
                    } else if let Some(c) = port_refs.get_mut(port.as_str()) {
                        *c += 1;
                    }
                }
            }
            check_projection_policy(nref, i, s, report);
        }
        "store" => {
            match &s.store {
                None => report.push(
                    ErrorCode::ContextSourceUnresolvable,
                    &nref.scope,
                    format!(
                        "node '{}' section[{i}] from 'store' 缺来源字段 store",
                        nref.path
                    ),
                ),
                Some(store) => {
                    if !ix.stores.contains_key(store) {
                        report.push(
                            ErrorCode::ContextSourceUnresolvable,
                            &nref.scope,
                            format!(
                                "node '{}' section[{i}] store '{store}' not declared in stores（store 名经作用域链解析：v0.1 = 顶层 stores 表）",
                                nref.path
                            ),
                        );
                    }
                }
            }
            check_store_policy(nref, i, s, report);
        }
        _ => {} // from 不在五词表：M1 check_vocab 已报 E1004，整段跳过。
    }

    // 每段 quota_tokens 为正；例外仅 store 段 policy:none 记 0（提案 §7-5）。
    if s.quota_tokens == 0 && !(s.from == "store" && s.policy.as_deref() == Some("none")) {
        report.push(
            ErrorCode::QuotaNotPositive,
            &nref.scope,
            format!(
                "node '{}' section[{i}] quota_tokens = 0（须为正；例外仅 store 段 policy:none 记 0）",
                nref.path
            ),
        );
    }
}

/// input/arg/edge 段策略：缺省合法；出现须恰为 project → E2401。
fn check_projection_policy(nref: &NodeRef<'_>, i: usize, s: &SectionDecl, report: &mut Report) {
    if let Some(p) = &s.policy {
        if p != "project" {
            report.push(
                ErrorCode::StoreViewPolicyUnknown,
                &nref.scope,
                format!(
                    "node '{}' section[{i}] policy '{p}' 不适用于 from '{}'（仅 project 或缺省）",
                    nref.path, s.from
                ),
            );
        }
    }
}

/// store 段策略域：必填五词表 + 参数闭合 → E2401。
fn check_store_policy(nref: &NodeRef<'_>, i: usize, s: &SectionDecl, report: &mut Report) {
    let Some(p) = s.policy.as_deref() else {
        report.push(
            ErrorCode::StoreViewPolicyUnknown,
            &nref.scope,
            format!(
                "node '{}' section[{i}] from 'store' 缺视图策略（必填：{}）",
                nref.path,
                STORE_VIEW_POLICIES.join("|")
            ),
        );
        return;
    };
    if !STORE_VIEW_POLICIES.contains(&p) {
        report.push(
            ErrorCode::StoreViewPolicyUnknown,
            &nref.scope,
            format!(
                "node '{}' section[{i}] policy '{p}' 不在 store 视图策略词表（{}）",
                nref.path,
                STORE_VIEW_POLICIES.join("|")
            ),
        );
        return;
    }
    match p {
        "latest_k" => require_args_k(nref, i, s, report),
        "retrieve" => {
            if s.component.as_deref().is_none_or(|c| c.is_empty()) {
                report.push(
                    ErrorCode::StoreViewPolicyUnknown,
                    &nref.scope,
                    format!(
                        "node '{}' section[{i}] policy 'retrieve' 缺 component（组件引用）",
                        nref.path
                    ),
                );
            }
            require_args_k(nref, i, s, report);
        }
        "summarize" if s.component.as_deref().is_none_or(|c| c.is_empty()) => {
            report.push(
                ErrorCode::StoreViewPolicyUnknown,
                &nref.scope,
                format!(
                    "node '{}' section[{i}] policy 'summarize' 缺 component（组件引用）",
                    nref.path
                ),
            );
        }
        _ => {}
    }
}

/// latest_k / retrieve 的 args.k：须为正整数。
fn require_args_k(nref: &NodeRef<'_>, i: usize, s: &SectionDecl, report: &mut Report) {
    let ok = s
        .args
        .as_ref()
        .and_then(|a| a.get("k"))
        .and_then(|v| v.as_u64())
        .is_some_and(|k| k > 0);
    if !ok {
        report.push(
            ErrorCode::StoreViewPolicyUnknown,
            &nref.scope,
            format!(
                "node '{}' section[{i}] policy '{}' 参数 args.k 缺失或非正整数",
                nref.path,
                s.policy.as_deref().unwrap_or_default()
            ),
        );
    }
}

/// template vars 值：字面量（无 input./arg. 前缀）不判；input.字段 ∈ meta.input
/// schema；arg.名 ∈ 可见形参 → E2403。
fn check_var_ref(
    doc: &Document,
    nref: &NodeRef<'_>,
    i: usize,
    var: &str,
    val: &str,
    formals: &BTreeSet<String>,
    report: &mut Report,
) {
    if let Some(rest) = val.strip_prefix("input.") {
        let field = rest.split('.').next().unwrap_or(rest);
        if field.is_empty() {
            report.push(
                ErrorCode::TemplateVarsDangling,
                &nref.scope,
                format!(
                    "node '{}' section[{i}] vars.{var} 引用 '{val}' 悬空（input. 后须跟字段名）",
                    nref.path
                ),
            );
        } else if input_has_properties(doc) && input_property(doc, field).is_none() {
            report.push(
                ErrorCode::TemplateVarsDangling,
                &nref.scope,
                format!(
                    "node '{}' section[{i}] vars.{var} 引用 '{val}' 悬空：字段 '{field}' 不在 meta.input schema（{}）",
                    nref.path, doc.meta.input
                ),
            );
        }
    } else if let Some(rest) = val.strip_prefix("arg.") {
        let name = rest.split('.').next().unwrap_or(rest);
        if name.is_empty() {
            report.push(
                ErrorCode::TemplateVarsDangling,
                &nref.scope,
                format!(
                    "node '{}' section[{i}] vars.{var} 引用 '{val}' 悬空（arg. 后须跟形参名）",
                    nref.path
                ),
            );
        } else if !formals.contains(name) {
            report.push(
                ErrorCode::TemplateVarsDangling,
                &nref.scope,
                format!(
                    "node '{}' section[{i}] vars.{var} 引用 '{val}' 悬空：形参 '{name}' 不在所在体形参表（可见形参：[{}]）",
                    nref.path,
                    formals_labels(formals)
                ),
            );
        }
    }
}

/// 可见形参标签（BTreeSet 有序，消息确定性）。
fn formals_labels(formals: &BTreeSet<String>) -> String {
    formals.iter().cloned().collect::<Vec<_>>().join(", ")
}

/// 体形参可见集：从 scope 沿作用域链向外收集——loop 体贡献其 args 键、
/// map 体贡献隐式 item；根图无体形参（input 经 from:input 段进入）。
pub(crate) fn visible_formals(ix: &Index<'_>, scope: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut cur = scope.to_string();
    while cur != "root" {
        let Some((construct, kind)) = cur
            .strip_suffix(".loop_body")
            .map(|p| (p, "loop"))
            .or_else(|| cur.strip_suffix(".map_body").map(|p| (p, "map")))
        else {
            break;
        };
        let Some(nref) = ix.nodes.get(construct) else {
            break;
        };
        match kind {
            "loop" => {
                if let Some(lp) = &nref.node.loop_spec {
                    if let Some(args) = &lp.args {
                        out.extend(args.keys().cloned());
                    }
                }
            }
            _ => {
                out.insert("item".to_string());
            }
        }
        cur = nref.scope.clone();
    }
    out
}
