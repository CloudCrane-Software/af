//! Stage 3 · 组件绑定（`bind`）：IR → 绑定图（提案 B §3.3）。
//!
//! 纯函数：[`bind`] 消费 [`IrProgram`]（stage 2 产物——**绑定从 IR 出发**，
//! 不回读 DSL）与 [`ComponentRegistry`]（stage 3 对照面），对每个
//! `node_exec` 节点解析 `uses.*` 引用：
//!
//! - `uses.model` → model_route 类；`uses.tool/tools` → tool 类；
//!   `uses.unit` → unit 类；`uses.memory` → memory 类；
//! - 引用串按最后一个 `'@'` 切分 `name@version`（无 `@` = 未版本化引用，
//!   绑定到该 name 的唯一声明版本）；
//! - 产出 [`BoundNodeExec`] 投影：`components/model_route/tools/unit/memory`
//!   解析为版本化引用 + **ADR-010 四一等字段的回填位**——`effect_class`
//!   （tool 类，注册表回填）与 `provenance.envelope_of`（工件类型声明
//!   `envelope<T>` 之 T）真实回填；`capability_refs`/`trust` v0.1 无数据源，
//!   保持空占位（见 RESULT-G2b 遗留）。
//!
//! **错误模型（E900x 段，提案 §码表 1572 行：部署期 4 条）**——一次收齐
//! （承 af-dsl Report 范式），逐条点名节点路径与引用串，fail-loud：
//!
//! | 码 | 语义 | 触发 |
//! |---|---|---|
//! | E9001 | 组件引用缺失 | 引用名未注册，或引用串不可解析（空段/内嵌 `'@'`） |
//! | E9002 | 版本不符 | 名已注册但请求版本未声明；或未版本化引用命中多版本声明 |
//! | E9003 | 组件类别不符 | 名+版本已注册但类别与 `uses` 面不符（unit 当 tool 用等） |
//! | E9004 | registry digest 不匹配 | **编译期无生产者**——digest 由本阶段产出并随计划携带，核对归执行面装载（§2.2）；码位保留 |
//!
//! **逃逸注入点**：IR `escapes` 记账（键 = 节点点号路径）原样透传进
//! [`BindReport::escapes`]——`impl_ref`/`kind`/`reason`/`owner`/
//! `share_budget_pct` 全字段保留（stage 4 落进计划对象的 escapes 块；
//! 实现本体在包内，编译器只记账不解析，ADR-001 §3）。
//!
//! **AgentSpec 四元组的可装载面**：`uses.model` 节点附带
//! [`BoundAgent::endpoint`]（model_route 声明的端点原样——af-harness
//! `AgentSpec.endpoint` 直通位）。四元组的 `context_template`（需模板组件
//! 注册面）与 `policy`（需 loop_policy 组件）v0.1 无 `uses` 声明面，不伪造
//! ——见 RESULT-G2b 遗留。

use crate::components::{ComponentClass, ComponentRegistry, EndpointSpec};
use crate::ir::{
    ArtifactSpec, CelRef, EffectClass, EscapeSpec, IrProgram, OpSpec, ProvenanceSpec, SectionSpec,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

// ============================================================
// E900x 违例
// ============================================================

/// 部署期错误码（E900x 段——提案 §码表 4 条中的编译期生产者 3 条；
/// E9004 无编译期生产者，码位保留执行面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindCode {
    /// E9001 组件引用缺失。
    RefMissing,
    /// E9002 版本不符。
    VersionMismatch,
    /// E9003 组件类别不符。
    ClassMismatch,
}

impl BindCode {
    /// 稳定码串（ADR-007 §3：稳定码 + 精确信息）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RefMissing => "E9001",
            Self::VersionMismatch => "E9002",
            Self::ClassMismatch => "E9003",
        }
    }
}

impl fmt::Display for BindCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 一条绑定违例：码 + 节点点号路径 + 引用串 + 信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindViolation {
    pub code: BindCode,
    pub node: String,
    pub reference: String,
    pub message: String,
}

impl fmt::Display for BindViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} 节点 {:?} 引用 {:?}：{}",
            self.code, self.node, self.reference, self.message
        )
    }
}

// ============================================================
// 绑定产物
// ============================================================

/// 解析后的组件引用（版本化组合引用 + 类别 + 回填位）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedComponent {
    /// 组件名（引用串去版本段——`name@version` 之 name）。
    pub name: String,
    /// 解析后版本化引用（`name@version`——未版本化 DSL 引用在此升格）。
    pub reference: String,
    pub class: ComponentClass,
    pub version: String,
    /// ADR-010 effect_class 回填（tool 类恒 `Some`——装载即校验）。
    pub effect_class: Option<EffectClass>,
    /// 实现定位串（声明原样）。
    pub impl_ref: Option<String>,
    /// 端点（model_route 类恒 `Some`——af-harness 四元组直通位）。
    pub endpoint: Option<EndpointSpec>,
}

/// 绑定后组件投影（落地契约 §7.1 `node_exec.components` 面 + 解析细节）。
/// `loop_policy`/`context_compaction` v0.1 的 `uses` 无声明面，恒 `None`/
/// 空（字段位先占住——§2.2 六类组件对齐）。
#[derive(Debug, Clone, PartialEq)]
pub struct BoundComponents {
    pub model_route: Option<ResolvedComponent>,
    pub loop_policy: Option<ResolvedComponent>,
    pub memory: Vec<ResolvedComponent>,
    pub tools: Vec<ResolvedComponent>,
    pub context_compaction: Option<ResolvedComponent>,
    pub unit: Option<ResolvedComponent>,
}

/// task 节点（`uses.model` 绑定者）的 af-harness 四元组**可装载面**：
/// `endpoint` 实数据（model_route 声明原样）；`context_template`/`policy`
/// v0.1 无声明面不伪造（RESULT-G2b 遗留——G2c/执行面补齐）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundAgent {
    pub endpoint: EndpointSpec,
}

/// 绑定后节点执行投影（IR `NodeExec` 的组件解析 + 一等字段回填形态；
/// 其余字段——reads/writes/consumes/context/ops/assert——原样透传）。
#[derive(Debug, Clone, PartialEq)]
pub struct BoundNodeExec {
    pub node_id: String,
    pub components: BoundComponents,
    /// `uses.model` 节点的四元组可装载面（无 model 引用 = None）。
    pub agent: Option<BoundAgent>,
    pub reads: BTreeSet<String>,
    pub writes: BTreeSet<String>,
    pub consumes: BTreeMap<String, String>,
    /// 工件契约（`provenance.envelope_of` 已按 `envelope<T>` 声明回填）。
    pub artifacts: BTreeMap<String, ArtifactSpec>,
    pub context: Vec<SectionSpec>,
    pub ops: Vec<OpSpec>,
    pub assert: Option<CelRef>,
}

/// 绑定报告（提案 §3.3 `BindReport`）：全部绑定投影 + 逃逸透传 + 注册表
/// digest + 一次收齐的违例集。
#[derive(Debug, Clone, PartialEq)]
pub struct BindReport {
    /// 键 = 节点点号路径（与 IR `node_exec` 键同集）；含违例节点的可解析部分。
    pub bindings: BTreeMap<String, BoundNodeExec>,
    /// L4 逃逸记账透传（注入点；stage 4 落进计划 escapes 块）。
    pub escapes: BTreeMap<String, EscapeSpec>,
    /// 注册表 digest（§2.2——执行面核对面，E9004）。
    pub registry_digest: String,
    /// E900x 违例（一次收齐；非空 = 绑定不可用，fail-loud）。
    pub violations: Vec<BindViolation>,
}

impl BindReport {
    /// 零违例（绑定可用于 stage 4）。
    pub fn is_ok(&self) -> bool {
        self.violations.is_empty()
    }
}

/// `IrProgram` × `ComponentRegistry` → 绑定图（纯函数，无 I/O）。
pub fn bind(ir: &IrProgram, registry: &ComponentRegistry) -> BindReport {
    let mut violations: Vec<BindViolation> = Vec::new();
    let mut bindings: BTreeMap<String, BoundNodeExec> = BTreeMap::new();
    for (path, exec) in &ir.body.node_exec {
        let bound = bound_node_exec(exec, path, registry, &mut violations);
        bindings.insert(path.clone(), bound);
    }
    BindReport {
        bindings,
        // 逃逸注入点：IR 记账视图原样透传（impl_ref 保留、字段不加工）。
        escapes: ir.body.escapes.clone(),
        registry_digest: registry.digest().to_string(),
        violations,
    }
}

/// 单节点投影：uses.* 四面解析 + effect_class/provenance 回填 + 其余透传。
fn bound_node_exec(
    exec: &crate::ir::NodeExec,
    path: &str,
    registry: &ComponentRegistry,
    violations: &mut Vec<BindViolation>,
) -> BoundNodeExec {
    let c = &exec.components;
    let model_route = c
        .model
        .as_deref()
        .and_then(|r| resolve(r, ComponentClass::ModelRoute, path, registry, violations));
    let agent = model_route.as_ref().map(|m| BoundAgent {
        // model_route 类装载即校验 endpoint 在场（components.rs 不变量）
        endpoint: m.endpoint.clone().expect("model_route 端点装载不变量"),
    });
    let tools = c
        .tools
        .keys()
        .filter_map(|r| resolve(r, ComponentClass::Tool, path, registry, violations))
        .collect();
    let unit = c
        .unit
        .as_deref()
        .and_then(|r| resolve(r, ComponentClass::Unit, path, registry, violations));
    let memory = c
        .memory
        .iter()
        .filter_map(|r| resolve(r, ComponentClass::Memory, path, registry, violations))
        .collect();

    // ADR-010 provenance 回填：类型声明 `envelope<T>` → envelope_of = T
    // （声明面的机械推导，无第二份类型推导器；eval_sha256 运行期落）。
    let artifacts = exec
        .artifacts
        .iter()
        .map(|(port, a)| {
            let mut a = a.clone();
            if let Some(inner) = a
                .r#type
                .as_deref()
                .and_then(|t| t.strip_prefix("envelope<"))
                .and_then(|t| t.strip_suffix('>'))
                .filter(|t| !t.is_empty())
            {
                a.provenance = Some(ProvenanceSpec {
                    envelope_of: Some(inner.to_string()),
                    eval_sha256: None,
                });
            }
            (port.clone(), a)
        })
        .collect();

    BoundNodeExec {
        node_id: exec.node_id.clone(),
        components: BoundComponents {
            model_route,
            loop_policy: None, // v0.1 uses 无声明面（占位）
            memory,
            tools,
            context_compaction: None, // v0.1 uses 无声明面（占位）
            unit,
        },
        agent,
        reads: exec.reads.clone(),
        writes: exec.writes.clone(),
        consumes: exec.consumes.clone(),
        artifacts,
        context: exec.context.clone(),
        ops: exec.ops.clone(),
        assert: exec.assert.clone(),
    }
}

/// 引用解析：切分 `name@version`（最后 `'@'`）→ 注册表对照 → 类别核对。
/// 命中 → [`ResolvedComponent`]；未命中 → 记违例返回 None（一次收齐，
/// 节点其余引用继续解析）。
fn resolve(
    reference: &str,
    expected: ComponentClass,
    node: &str,
    registry: &ComponentRegistry,
    violations: &mut Vec<BindViolation>,
) -> Option<ResolvedComponent> {
    let mut push = |code: BindCode, message: String| {
        violations.push(BindViolation {
            code,
            node: node.to_string(),
            reference: reference.to_string(),
            message,
        });
    };
    // ① 切分：name 非空、无内嵌 '@'；版本段（若有）非空。
    let (name, want_version) = match reference.rsplit_once('@') {
        Some((n, v)) if !n.is_empty() && !v.is_empty() => {
            if n.contains('@') || v.contains('@') {
                push(
                    BindCode::RefMissing,
                    "引用串不可解析（内嵌 '@'——无法对照注册表）".into(),
                );
                return None;
            }
            (n, Some(v))
        }
        Some(_) | None if !reference.is_empty() && !reference.contains('@') => (reference, None),
        Some(_) | None => {
            push(
                BindCode::RefMissing,
                "引用串不可解析（空串或内嵌 '@'）".into(),
            );
            return None;
        }
    };
    // ② 名字对照：未注册 → E9001。
    let Some(versions) = registry.versions_of(name) else {
        push(
            BindCode::RefMissing,
            format!(
                "组件 {name:?} 未注册（注册表共 {} 条声明）",
                registry.component_count()
            ),
        );
        return None;
    };
    // ③ 版本核对：请求版本未声明 / 未版本化引用命中多版本 → E9002。
    let version = match want_version {
        Some(v) if versions.contains_key(v) => v.to_string(),
        Some(v) => {
            let declared: Vec<&str> = versions.keys().map(String::as_str).collect();
            push(
                BindCode::VersionMismatch,
                format!("{name:?} 请求版本 {v:?} 未声明（已声明：{declared:?}）"),
            );
            return None;
        }
        None => {
            if versions.len() != 1 {
                let declared: Vec<&str> = versions.keys().map(String::as_str).collect();
                push(
                    BindCode::VersionMismatch,
                    format!(
                        "未版本化引用 {name:?} 命中多版本声明 {declared:?}（须带 @version 消歧）"
                    ),
                );
                return None;
            }
            versions.keys().next().expect("len==1 已判").clone()
        }
    };
    // ④ 类别核对：声明类别与 uses 面不符 → E9003。
    let decl = &versions[&version];
    if decl.class != expected {
        push(
            BindCode::ClassMismatch,
            format!(
                "{name:?}@{version} 注册类别为 {:?}，此处按 {:?} 引用",
                decl.class, expected
            ),
        );
        return None;
    }
    Some(ResolvedComponent {
        name: name.to_string(),
        reference: format!("{name}@{version}"),
        class: decl.class,
        version,
        effect_class: decl.effect_class,
        impl_ref: decl.impl_ref.clone(),
        endpoint: decl.endpoint.clone(),
    })
}
