//! 语义层校验编排（M1）：手写词表（E1004）→ 重复 id（E2003，建索引时收）
//! → 引用闭包（refs）→ INV-2（inv2），一次收齐。
//! 顺序固定以保证违例输出确定性（承提案 B §3.1 顺序说明的 M1 前缀）。

mod refs;

use crate::error::{ErrorCode, Report};
use crate::model::{Document, EdgeDecl, LoopDecl, NodeDecl, PortType, StoreDecl};
use std::collections::{BTreeMap, BTreeSet};

/// 节点在作用域索引中的条目。
pub(crate) struct NodeRef<'a> {
    pub node: &'a NodeDecl,
    /// 所在体的 scope 标签（"root" / "<路径>.loop_body" / "<路径>.map_body"）。
    pub scope: String,
    /// 从根的点号路径（只含构造节点 id 段，如 "rounds.debate_map.debater"）。
    pub path: String,
    /// 是否处于任何 map 体内（INV-2 的 E2312 判定用）。
    pub in_map: bool,
}

/// 一个体作用域：本地 id → 节点全路径。
#[derive(Default)]
pub(crate) struct ScopeInfo {
    pub ids: BTreeMap<String, String>,
}

/// loop 构造位置（INV-2 carried 覆盖核对用）。
pub(crate) struct LoopSite<'a> {
    /// 拥有 loop 构造的节点路径（体内节点路径均以 "{owner}." 起头）。
    pub owner_path: String,
    pub decl: &'a LoopDecl,
}

/// 文档的作用域索引：全部节点（含体内嵌）、全部体、类型表、store 表。
pub(crate) struct Index<'a> {
    pub nodes: BTreeMap<String, NodeRef<'a>>,
    pub scopes: BTreeMap<String, ScopeInfo>,
    pub edges: BTreeMap<String, &'a [EdgeDecl]>,
    pub loops: Vec<LoopSite<'a>>,
    pub type_names: BTreeSet<String>,
    pub stores: BTreeMap<String, &'a StoreDecl>,
}

pub fn validate(doc: &Document) -> Report {
    let mut report = Report::default();
    report.violations.extend(crate::shape::check_vocab(doc));
    let index = Index::build(doc, &mut report);
    refs::check(doc, &index, &mut report);
    // INV-2（E2311/E2312/E2211）在 M1 末段（C8）接入。
    report
}

impl<'a> Index<'a> {
    pub(crate) fn build(doc: &'a Document, report: &mut Report) -> Self {
        let mut ix = Self {
            nodes: BTreeMap::new(),
            scopes: BTreeMap::new(),
            edges: BTreeMap::new(),
            loops: Vec::new(),
            type_names: BTreeSet::new(),
            stores: BTreeMap::new(),
        };
        for t in &doc.types {
            if !ix.type_names.insert(t.name.clone()) {
                report.push(
                    ErrorCode::DuplicateId,
                    "types",
                    format!("类型名 {:?} 重复登记", t.name),
                );
            }
        }
        for s in &doc.stores {
            if ix.stores.insert(s.name.clone(), s).is_some() {
                report.push(
                    ErrorCode::DuplicateId,
                    "stores",
                    format!("store 名 {:?} 重复声明", s.name),
                );
            }
        }
        ix.walk(&doc.nodes, &doc.edges, "", "root", false, report);
        ix
    }

    fn walk(
        &mut self,
        nodes: &'a [NodeDecl],
        edges: &'a [EdgeDecl],
        parent_path: &str,
        label: &str,
        in_map: bool,
        report: &mut Report,
    ) {
        self.edges.insert(label.to_string(), edges);
        for n in nodes {
            let path = if parent_path.is_empty() {
                n.id.clone()
            } else {
                format!("{parent_path}.{}", n.id)
            };
            let dup = self
                .scopes
                .entry(label.to_string())
                .or_default()
                .ids
                .insert(n.id.clone(), path.clone())
                .is_some();
            if dup {
                report.push(
                    ErrorCode::DuplicateId,
                    label,
                    format!("节点 id {:?} 在作用域 {label:?} 内重复", n.id),
                );
            }
            if let Some(lp) = &n.loop_spec {
                let child = format!("{path}.loop_body");
                self.loops.push(LoopSite {
                    owner_path: path.clone(),
                    decl: lp,
                });
                self.walk(&lp.body.nodes, &lp.body.edges, &path, &child, in_map, report);
            }
            if let Some(mp) = &n.map {
                let child = format!("{path}.map_body");
                self.walk(&mp.body.nodes, &mp.body.edges, &path, &child, true, report);
            }
            // 重复 id：首个声明者胜（后续同名节点不覆盖索引，避免噪声级联）。
            if !dup {
                self.nodes.insert(
                    path.clone(),
                    NodeRef {
                        node: n,
                        scope: label.to_string(),
                        path,
                        in_map,
                    },
                );
            }
        }
    }

    /// scope 内本地 id → 节点。
    pub(crate) fn scope_id(&self, scope: &str, id: &str) -> Option<&NodeRef<'a>> {
        let path = self.scopes.get(scope)?.ids.get(id)?;
        self.nodes.get(path)
    }

    /// 父节点的 loop/map 体中找子节点。
    pub(crate) fn child_node(&self, parent: &NodeRef<'a>, id: &str) -> Option<&NodeRef<'a>> {
        for suffix in ["loop_body", "map_body"] {
            let label = format!("{}.{}", parent.path, suffix);
            if let Some(n) = self.scope_id(&label, id) {
                return Some(n);
            }
        }
        None
    }

    /// 从 base_scope 的体根逐段解析点号路径（可穿嵌套体）。
    pub(crate) fn resolve_path(&self, base_scope: &str, dotted: &str) -> Option<&NodeRef<'a>> {
        let mut segs = dotted.split('.');
        let first = segs.next()?;
        let mut cur = self.scope_id(base_scope, first)?;
        for seg in segs {
            cur = self.child_node(cur, seg)?;
        }
        Some(cur)
    }

    /// 从根图解析点号路径（store.writer 的基准）。
    pub(crate) fn resolve_root_path(&self, dotted: &str) -> Option<&NodeRef<'a>> {
        self.resolve_path("root", dotted)
    }

    /// 端口是否存在：emits 键 ∪ 内建端口（loop/map 的 done:signal、check/gate 的
    /// passed:boolean）。
    pub(crate) fn port_exists(&self, node: &NodeRef<'a>, port: &str) -> bool {
        if (node.node.loop_spec.is_some() || node.node.map.is_some()) && port == "done" {
            return true;
        }
        if matches!(node.node.kind.as_str(), "check" | "gate") && port == "passed" {
            return true;
        }
        node.node
            .emits
            .as_ref()
            .is_some_and(|e| e.contains_key(port))
    }

    /// 端口类型推导：直接类型，或 from_body 递归（map 构造自动包 list<>，loop 不包）。
    /// depth 防	from_body 环；返回 None = 不可判定（悬空或环）。
    pub(crate) fn emit_type(&self, node: &NodeRef<'a>, port: &str, depth: u8) -> Option<PortType> {
        if depth == 0 {
            return None;
        }
        if (node.node.loop_spec.is_some() || node.node.map.is_some()) && port == "done" {
            return Some(PortType::signal());
        }
        if matches!(node.node.kind.as_str(), "check" | "gate") && port == "passed" {
            return Some(PortType::boolean());
        }
        let spec = node.node.emits.as_ref()?.get(port)?;
        match spec {
            crate::model::EmitSpec::Type(t) => Some(t.clone()),
            crate::model::EmitSpec::FromBody { from_body } => {
                let body_scope = construct_body_scope(node)?;
                let (target_id, tport) = from_body.rsplit_once('.')?;
                let target = self.scope_id(&body_scope, target_id)?;
                let inner = self.emit_type(target, tport, depth - 1)?;
                if node.node.map.is_some() {
                    match inner {
                        PortType::Named(t) => Some(PortType::list_of(t.0)),
                        _ => None,
                    }
                } else {
                    Some(inner)
                }
            }
        }
    }
}

/// 构造节点的体 scope 标签（loop/map 皆有则取 loop 体）。
pub(crate) fn construct_body_scope(node: &NodeRef<'_>) -> Option<String> {
    if node.node.loop_spec.is_some() {
        Some(format!("{}.loop_body", node.path))
    } else if node.node.map.is_some() {
        Some(format!("{}.map_body", node.path))
    } else {
        None
    }
}
