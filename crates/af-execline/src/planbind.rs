//! planbind —— 计划装订器（E9004 执行面消费的**唯一正本**，裁决9）。
//!
//! 装载期四核对（任一不符拒启动——fail-loud，不降级不静默换计划）：
//! 1. **E9004**（→EX4004）：`ComponentRegistry::load(<本地包根>).digest()`
//!    ≠ `plan.compiled_from.registry_digest`——组件注册表与编译期漂移
//!    （plan.rs CompiledFrom 注释「执行面装载时对本地注册表重算核对，不符
//!    即 E9004 拒启动」的兑现；sweng/reportkit 两单元与后续包共用本装订器，
//!    报告单元**不得**另写第二套 digest 核对——裁决9）。
//! 2. **信封**：`v == 1` + 拒绝式 decode（deny_unknown_fields）。
//! 3. **一致性**：graph_version（compiled_from vs order 槽 vs 可选外部期待）、
//!    line 绑定（计划携带 line_id 时与期待核对——编译期常 None，执行面
//!    bind_line 后可复验）、node_exec 与 graph.nodes 按 id 对齐。
//! 4. **模板清单**（裁决10 统一机制，解析器一套 = [`crate::pkg::prompts`]）：
//!    装订器收口单元 spec 声明的 template_ref 集，逐件装载 + MANIFEST.sha256
//!    核对（EX4001-4003 透传）。
//!
//! 依赖方向（裁决9 随销项）：af-execline path 依赖 af-compiler（复用
//! `ExecutablePlan` decode + `ComponentRegistry::load`）；af-plan 独立建位
//! （类型搬迁去重）仍列开放问题（软件工程包设计 §7-2）。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use af_compiler::plan::{decode_plan, PLAN_VERSION};
use af_compiler::{ComponentRegistry, ExecutablePlan};

use crate::error::ExeclineError;
use crate::pkg::prompts::{self, PromptAsset};

/// 装订期待（可选外部核对值：驱动层 bind_line 的 line、WorkOrder 一次性
/// 绑定的 graph_version——driver.rs:42-43 语义面；None = 不核对该项）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BindExpectations {
    pub line_id: Option<String>,
    pub graph_version: Option<String>,
}

/// 装订产物：核对通过的计划 + 已装载模板集（key = template_ref 原串）。
#[derive(Debug, Clone)]
pub struct PlanBind {
    pub plan: ExecutablePlan,
    pub templates: BTreeMap<String, PromptAsset>,
}

/// 装订主入口：`plan_src` = 计划 JSON 全文；`registry_root` = 本地组件包根
/// （`af/packages`）；`template_refs` = 单元 spec 声明的提示词引用集。
pub fn bind(
    plan_src: &str,
    registry_root: &Path,
    template_refs: &[&str],
    expected: &BindExpectations,
) -> Result<PlanBind, ExeclineError> {
    // ① 信封 decode（拒绝式：v != 1 / 未知字段 / 块缺失即拒）。
    let plan = decode_plan(plan_src).map_err(|err| ExeclineError::PlanBindMismatch {
        detail: format!("计划 decode 拒绝: {err}"),
    })?;
    if plan.version != PLAN_VERSION {
        return Err(ExeclineError::PlanBindMismatch {
            detail: format!("信封版本 {} != {PLAN_VERSION}", plan.version),
        });
    }
    // ② E9004：本地注册表重算 digest 对计划携带值（唯一消费者正本）。
    let registry =
        ComponentRegistry::load(registry_root).map_err(|err| ExeclineError::PlanBindMismatch {
            detail: format!("本地组件注册表装载失败（{:#}）", err),
        })?;
    let actual = registry.digest().to_string();
    if actual != plan.body.compiled_from.registry_digest {
        return Err(ExeclineError::PlanDigestMismatch {
            expected: plan.body.compiled_from.registry_digest.clone(),
            actual,
        });
    }
    // ③ 一致性：graph_version 三面（compiled_from / order 槽 / 外部期待）。
    let compiled_gv = &plan.body.compiled_from.graph_version;
    if &plan.body.order.graph_version != compiled_gv {
        return Err(ExeclineError::PlanBindMismatch {
            detail: format!(
                "graph_version 不一致：compiled_from {compiled_gv} ≠ order 槽 {}",
                plan.body.order.graph_version
            ),
        });
    }
    if let Some(want) = &expected.graph_version {
        if want != compiled_gv {
            return Err(ExeclineError::PlanBindMismatch {
                detail: format!(
                    "graph_version 不一致：期待 {want} ≠ 计划 {compiled_gv}（WorkOrder 一次性绑定）"
                ),
            });
        }
    }
    // line 绑定：计划携带 line_id（编译期常 None）时与外部期待核对。
    if let (Some(carried), Some(want)) = (&plan.body.line.line_id, &expected.line_id) {
        if carried != want {
            return Err(ExeclineError::PlanBindMismatch {
                detail: format!("line 绑定不一致：计划携带 {carried} ≠ 期待 {want}"),
            });
        }
    }
    // node_exec 与 graph.nodes 按 id 对齐：顶层节点全覆盖 + 反向归属
    // （node_exec 的点号路径须落在某顶层节点名下——体内节点带路径前缀）。
    // 重影拒绝（对抗面 2026-10-25）：任一侧 id 重复 = 编排真相二义——
    // 执行面 `find` 静默取首份投影，重复条目的契约差异被吞；先拒后对齐。
    {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for n in &plan.body.graph.nodes {
            if !seen.insert(n.id.as_str()) {
                return Err(ExeclineError::PlanBindMismatch {
                    detail: format!("graph.nodes 重复节点 id {}", n.id),
                });
            }
        }
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for n in &plan.body.node_exec {
            if !seen.insert(n.node_id.as_str()) {
                return Err(ExeclineError::PlanBindMismatch {
                    detail: format!("node_exec 重复投影 {}", n.node_id),
                });
            }
        }
    }
    let top_ids: Vec<&str> = plan
        .body
        .graph
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    let exec_ids: Vec<&str> = plan
        .body
        .node_exec
        .iter()
        .map(|n| n.node_id.as_str())
        .collect();
    for id in &top_ids {
        if !exec_ids.contains(id) {
            return Err(ExeclineError::PlanBindMismatch {
                detail: format!("graph 节点 {id} 无 node_exec 投影"),
            });
        }
    }
    for id in &exec_ids {
        let owned = top_ids
            .iter()
            .any(|top| *id == *top || id.starts_with(&format!("{top}.")));
        if !owned {
            return Err(ExeclineError::PlanBindMismatch {
                detail: format!("node_exec {id} 不归属任何 graph 顶层节点"),
            });
        }
    }
    // 红队 W1-C37：边级重影拒绝——同一 (to, input) 端口被多条边命中 = 编排
    // 真相二义（与节点 id 重影同类）：入边装配 inputs.insert 后边胜、提示词
    // 装配 .find 前边胜，「模型看到的」与「consumes 校验的」载荷静默分叉。
    {
        let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
        for e in &plan.body.graph.edges {
            if !seen.insert((e.to.clone(), e.input.clone())) {
                return Err(ExeclineError::PlanBindMismatch {
                    detail: format!(
                        "边级重影：{}.{:?} ← 多条边命中同一 (to, input) 端口（载荷二义，先拒）",
                        e.to, e.input
                    ),
                });
            }
        }
    }
    // 红队 W1-C38：节点 id / 端口名路径安全——手写 plan.json（任务包面）不经
    // af-dsl 词法集，id 含 '/'、'\' 或纯点号段（".."）即把节点目录与 envelope
    // 工件名（{node_id}.{port}.json）写出 attempt_dir 之外（同类计划内容面
    // 的敌意输入硬化）。
    {
        let bad_seg = |seg: &str| seg.is_empty() || seg == "." || seg == "..";
        for id in exec_ids.iter().chain(top_ids.iter()) {
            if id.contains('/') || id.contains('\\') || id.split('.').any(bad_seg) {
                return Err(ExeclineError::PlanBindMismatch {
                    detail: format!(
                        "节点 id {id:?} 含路径分隔符或纯点号段（节点目录/工件名拼接面，拒绝）"
                    ),
                });
            }
        }
        for n in &plan.body.node_exec {
            for port in n.artifact_contract.iter().map(|a| a.name.as_str()) {
                if port.contains('/')
                    || port.contains('\\')
                    || bad_seg(port)
                    || port.starts_with('.')
                {
                    return Err(ExeclineError::PlanBindMismatch {
                        detail: format!(
                            "节点 {} 工件端口名 {port:?} 含路径分隔符或纯点号段（工件文件名拼接面，拒绝）",
                            n.node_id
                        ),
                    });
                }
            }
            for port in n.consumes.keys() {
                if port.contains('/')
                    || port.contains('\\')
                    || bad_seg(port)
                    || port.starts_with('.')
                {
                    return Err(ExeclineError::PlanBindMismatch {
                        detail: format!(
                            "节点 {} consumes 端口名 {port:?} 含路径分隔符或纯点号段，拒绝",
                            n.node_id
                        ),
                    });
                }
            }
        }
    }
    // ④ 模板装载（裁决10 唯一机制，解析器一套）。
    let mut templates = BTreeMap::new();
    for r in template_refs {
        let asset = prompts::resolve_template(registry_root, r)?;
        templates.insert((*r).to_string(), asset);
    }
    Ok(PlanBind { plan, templates })
}
