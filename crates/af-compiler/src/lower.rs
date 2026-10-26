//! 确定性 lowering：`af-dsl` AST（**已过全量校验的** `Document`）→ IR v1。
//!
//! 纯函数、无 I/O、无时钟/随机/网络（§3.5-4）。同一份 `Document` 两次
//! lowering 产出逐位相同的序列化 IR（确定性测试逐字断言）。
//!
//! 输入契约：`Document` 已过 `af_dsl::validate` 全量校验（M1-M4）。lowering
//! 不做二次校验，只对「不一致即不可能出现在已校验文档中」的情形返回
//! [`LowerError`]（绝不 panic）。
//!
//! **不入 IR 的两个 DSL 面**（有意的，非丢失）：
//! - `Document.version`：DSL 侧信封（E0003 已判 =1）；IR 自带 `ir_version`。
//! - `meta.topology_selector`：编译期解析语义（裁决 §二-1）——只决定编译产物
//!   采用哪个候选图，解析为单一确定图后**不进运行时**，v0.1 单文档无包产物
//!   声明面，故无 IR 落点。
//!
//! 实现结构：[`Lowering`] 携带图名/版本/输出契约与三张累积表（子图/节点
//! 投影/逃逸记账），[`Lowering::scope`] 按作用域递归（根图与每个 loop/map
//! 体各一次）；`route` 归一化复用 `af_dsl::route::normalize`（§1.5 纯文本
//! 重写，无搜索）。

use crate::ir::*;
use af_dsl::model as m;
use cel_interpreter::Program;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// lowering 失败：输入违反「已过全量校验」前提（stage 1 的 E21xx/E1007 面）
/// 或序列化不可达错误。已校验文档恒 `Ok`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LowerError {
    /// 构造体为空（`entry = 体 nodes 首元素` 无从取）。af-dsl M1 不做空体
    /// 检查（E1007 留待构造形状模块，`model.rs:450` 注记）——lowering 以错误
    /// 报告代替 panic。
    EmptyBody { construct: String },
    /// route 归一化违例（E2101/E2102/E2103 在 stage 1 已判——命中即说明输入
    /// 未过校验）。`issues` 为逐条违例的调试串。
    RouteUnnormalized { node: String, issues: Vec<String> },
    /// CEL 编译失败（E1051 已判面；含 cel-parser 内部 panic 的 catch_unwind
    /// 兜底归一——承 af-graph `predicate.rs:11-14` 纪律）。
    CelCompile { source: String, detail: String },
    /// IR 序列化失败（serde_json 对本 schema 理论不可达）。
    Serialize(String),
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyBody { construct } => {
                write!(f, "构造 {construct:?} 的体为空（entry 无从取；E1007 面）")
            }
            Self::RouteUnnormalized { node, issues } => {
                write!(
                    f,
                    "节点 {node:?} route 归一化违例（输入未过 stage 1 校验）：{issues:?}"
                )
            }
            Self::CelCompile { source, detail } => {
                write!(
                    f,
                    "CEL 编译失败（输入未过 stage 1 校验）{source:?}：{detail}"
                )
            }
            Self::Serialize(detail) => write!(f, "IR 序列化失败：{detail}"),
        }
    }
}

impl std::error::Error for LowerError {}

/// `Document` → `IrProgram`（纯函数）。
/// 类型表达式规范串（红队 W3-C20）：内联原始型规范化为**原始名**
/// （string/integer/number/boolean），而非 `PortType::display()` 的
/// `"{type:string}"` sentinel——后者不在计划类型表达式文法
/// （name | list<> | envelope<> | signal）内、plan.types 无此基名：
/// 执行面 check_typed 对有人边载荷的该端口硬拒「基名不在 plan.types」、
/// materialize_schema 静默返 None（端口零形状信息）。规范化后内联型经
/// `inline_prims` 并入 plan.types（独立原始型条目 `{type: <prim>}`），
/// 边核对面/物化面/执行面同可解析（golden Multi-Agent-Debate 的
/// `consumes.item="{type:string}"` 即该 sentinel 的实证携带者）。
fn norm_type_expr(t: &m::PortType) -> String {
    match t {
        m::PortType::Named(r) => r.display(),
        m::PortType::Inline(p) => p.r#type.display().to_string(),
    }
}

pub fn lower(doc: &m::Document) -> Result<IrProgram, LowerError> {
    let mut lw = Lowering {
        doc,
        inline_prims: BTreeSet::new(),
        graph_name: doc.meta.name.clone(),
        graph_version: doc.meta.graph_version.clone(),
        output_from: doc.meta.output.from.clone(),
        output_type: norm_type_expr(&doc.meta.output.r#ref),
        subgraphs: BTreeMap::new(),
        node_exec: BTreeMap::new(),
        escapes: BTreeMap::new(),
    };
    let (nodes, edges) = lw.scope(&doc.nodes, &doc.edges, "")?;
    let body = IrBody {
        graph: GraphTemplate {
            graph_version: doc.meta.graph_version.clone(),
            name: doc.meta.name.clone(),
            entry: doc.meta.entry.clone(),
            nodes,
            edges,
            subgraphs: lw.subgraphs,
        },
        interface: InterfaceSpec {
            input_type: doc.meta.input.clone(),
            output_type: norm_type_expr(&doc.meta.output.r#ref),
            output_from: doc.meta.output.from.clone(),
            intent: doc.meta.intent.clone(),
        },
        // 类型词典（重复名 = stage 1 E2003 面，last-wins 机械收集）；
        // schema 为透传 Value，经 canonical_value 规范键序（见 ir.rs 契约护栏）
        // 类型词典 = 声明 types + 内联原始型条目（红队 W3-C20：规范串
        // string/integer/... 的执行面解析位——声明面无同名时补
        // `{type: <prim>}`；声明面已有同名（极少见）时声明优先，不覆写）。
        types: {
            let mut types: BTreeMap<String, serde_json::Value> = doc
                .types
                .iter()
                .map(|t| (t.name.clone(), canonical_value(&t.schema)))
                .collect();
            for prim in &lw.inline_prims {
                types
                    .entry(prim.clone())
                    .or_insert_with(|| serde_json::json!({ "type": prim }));
            }
            types
        },
        stores: doc
            .stores
            .iter()
            .map(|s| (s.name.clone(), store_spec(s)))
            .collect(),
        node_exec: lw.node_exec,
        policies: policies(doc)?,
        escapes: lw.escapes,
        // trust 注记一等占位（ADR-010 §5）：v0.1 恒空
        trust: BTreeMap::new(),
    };
    Ok(IrProgram {
        ir_version: IR_VERSION,
        body,
    })
}

/// 逐作用域降维状态（根图 + 嵌套体共用）。
struct Lowering<'a> {
    /// 全量文档（红队 W2-C26：from_body 端口的 is_final 类型推导经
    /// `af_dsl::derived_emit_type` 同规则求值，不另复制推导器）。
    doc: &'a m::Document,
    graph_name: String,
    graph_version: String,
    /// `meta.output.from`（终态工件发射者，is_final 判定用）。
    output_from: String,
    /// `meta.output.ref` 展示串。
    output_type: String,
    /// 全部命名子图（键 = `<图名>/<构造节点路径>`，全局唯一由路径构造保证）。
    subgraphs: BTreeMap<String, SubGraph>,
    /// 节点执行投影（键 = 点号路径）。
    node_exec: BTreeMap<String, NodeExec>,
    /// L4 逃逸记账（键 = 点号路径）。
    escapes: BTreeMap<String, EscapeSpec>,
    /// 内联原始型名收集（红队 W3-C20：emits/consumes/interface 引用到的
    /// 原始型并入 plan.types，使规范串在执行面可解析）。
    inline_prims: BTreeSet<String>,
}

impl<'a> Lowering<'a> {
    /// 子图名：`<图名>/<构造节点路径>`（路径中 `.` → `/`；§3.2 降维命名
    /// 确定性——机械拼接，不依赖哈希或计数器状态）。
    fn subgraph_name(&self, owner_path: &str) -> String {
        format!("{}/{}", self.graph_name, owner_path.replace('.', "/"))
    }

    /// 一个作用域（根图或某构造体）：节点投影 + 体递归 + 边守卫归一化。
    fn scope(
        &mut self,
        nodes: &[m::NodeDecl],
        edges: &[m::EdgeDecl],
        parent: &str,
    ) -> Result<(Vec<IrNode>, Vec<IrEdge>), LowerError> {
        // 边投影 + 排序（稳定键四元组，§3.5-2）。
        let mut ir_edges: Vec<IrEdge> = edges
            .iter()
            .map(|e| IrEdge {
                from: e.from.clone(),
                output: e.output.clone(),
                to: e.to.clone(),
                input: e.input.clone(),
                checkpoint: e.checkpoint.unwrap_or(false),
                guard: None,
            })
            .collect();
        ir_edges.sort_by(|a, b| {
            (&a.from, &a.output, &a.to, &a.input).cmp(&(&b.from, &b.output, &b.to, &b.input))
        });

        // 节点投影 + 体递归降维。
        let mut ir_nodes: Vec<IrNode> = Vec::with_capacity(nodes.len());
        let mut index: BTreeMap<&str, usize> = BTreeMap::new();
        for n in nodes {
            let path = if parent.is_empty() {
                n.id.clone()
            } else {
                format!("{parent}.{}", n.id)
            };
            let node_projection = self.node_exec(n, &path)?;
            self.node_exec.insert(path.clone(), node_projection);

            let loop_spec = match &n.loop_spec {
                Some(lp) => Some(self.loop_spec(lp, &path)?),
                None => None,
            };
            let map_spec = match &n.map {
                Some(mp) => Some(self.map_spec(mp, &path)?),
                None => None,
            };
            let escape = n.escape.as_ref().map(escape_spec);
            if let Some(esc) = &escape {
                self.escapes.insert(path.clone(), esc.clone());
            }
            // 同作用域重复 id = stage 1 E2003 面；首个声明者胜（与 af-dsl
            // Index 同款纪律，避免噪声级联）。
            index.entry(n.id.as_str()).or_insert(ir_nodes.len());
            ir_nodes.push(IrNode {
                id: n.id.clone(),
                kind: n.kind.clone(),
                human: n.human.as_ref().map(human_spec),
                loop_spec,
                map_spec,
                escape,
                route_exit_guard: None,
                // capability 引用一等占位（ADR-010 §5）：v0.1 恒空集
                capability_refs: BTreeSet::new(),
            });
        }

        // route 归一化（§1.5）：出边按目标携带守卫；goto:exit 不产边，联合
        // 守卫记节点。复用 af-dsl 纯函数——同一算法同一重构形，无第二实现。
        for n in nodes {
            let Some(route) = &n.route else { continue };
            let targets: BTreeSet<String> = edges
                .iter()
                .filter(|e| e.from == n.id)
                .map(|e| e.to.clone())
                .collect();
            let norm = af_dsl::route::normalize(route, &targets).map_err(|issues| {
                LowerError::RouteUnnormalized {
                    node: n.id.clone(),
                    issues: issues.iter().map(|i| format!("{i:?}")).collect(),
                }
            })?;
            // 目标 → 守卫（同名目标多条边共享该目标守卫——路由按目标不按端口）。
            let guard_by_target: BTreeMap<&str, CelRef> = norm
                .edges
                .iter()
                .map(|g| Ok((g.to.as_str(), cel_ref(&g.guard)?)))
                .collect::<Result<BTreeMap<_, _>, LowerError>>()?;
            for e in &mut ir_edges {
                if e.from == n.id {
                    if let Some(c) = guard_by_target.get(e.to.as_str()) {
                        e.guard = Some(c.clone());
                    }
                }
            }
            let exit = match &norm.exit_guard {
                Some(g) => Some(cel_ref(g)?),
                None => None,
            };
            let idx = index[n.id.as_str()];
            ir_nodes[idx].route_exit_guard = exit;
        }

        ir_nodes.sort_by(|a, b| a.id.cmp(&b.id));
        Ok((ir_nodes, ir_edges))
    }

    /// 节点执行投影（uses/consumes/emits/context/ops/assert/reads/writes）。
    fn node_exec(&mut self, n: &m::NodeDecl, path: &str) -> Result<NodeExec, LowerError> {
        let mut components = Components {
            model: None,
            tools: BTreeMap::new(),
            unit: None,
            memory: BTreeSet::new(),
        };
        if let Some(uses) = &n.uses {
            components.model = uses.model.clone();
            components.unit = uses.unit.clone();
            // tool（单）与 tools（列表）合并为引用集；effect_class 一等占位
            // 恒 None（stage 3 由组件注册表回填——ADR-010 §5）。
            if let Some(tool) = &uses.tool {
                components
                    .tools
                    .insert(tool.clone(), ToolSpec { effect_class: None });
            }
            if let Some(tools) = &uses.tools {
                for t in tools {
                    components
                        .tools
                        .entry(t.clone())
                        .or_insert(ToolSpec { effect_class: None });
                }
            }
            if let Some(mem) = &uses.memory {
                components.memory.extend(mem.iter().cloned());
            }
        }
        // 终态工件判定：输出发射者的端口（类型串相等或 from_body 转发——
        // from_body 的推导类型由执行面按 af-dsl 同规则求值，IR 不复制推导器）。
        let artifacts: BTreeMap<String, ArtifactSpec> = n
            .emits
            .as_ref()
            .map(|em| {
                em.iter()
                    .map(|(port, spec)| {
                        let (ty, fb) = match spec {
                            m::EmitSpec::Type(t) => {
                                let norm = norm_type_expr(t);
                                if let m::PortType::Inline(p) = t {
                                    self.inline_prims.insert(p.r#type.display().to_string());
                                }
                                (Some(norm), None)
                            }
                            m::EmitSpec::FromBody { from_body } => (None, Some(from_body.clone())),
                        };
                        // 终态判定（红队 W2-C26 收紧）：显式类型端口按
                        // 类型串相等（既有口径）；from_body 端口必须经
                        // af-dsl emit_type **同规则推导**且推导类型等于
                        // meta.output.ref 才置 is_final——此前
                        // `fb.is_some()` 析取项对 from_body 不看类型，
                        // 非同型 from_body 端口被误标终态（执行面
                        // final_artifact_port 取首个 is_final 即选错工件，
                        // 与 af-dsl E2202「恰一端口匹配」不闭环）。推导
                        // 不出（不可解析）= 不可证明，不置（fail-closed）。
                        let is_final = path == self.output_from
                            && match (&ty, &fb) {
                                (Some(t), _) => t == &self.output_type,
                                (None, Some(_)) => af_dsl::derived_emit_type(self.doc, path, port)
                                    .is_some_and(|derived| derived.display() == self.output_type),
                                (None, None) => false,
                            };
                        (
                            port.clone(),
                            ArtifactSpec {
                                r#type: ty,
                                from_body: fb,
                                is_final,
                                provenance: None,
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(NodeExec {
            node_id: n.id.clone(),
            components,
            reads: set_of(&n.reads),
            writes: set_of(&n.writes),
            consumes: n
                .consumes
                .as_ref()
                .map(|c| {
                    c.iter()
                        .map(|(k, v)| {
                            if let m::PortType::Inline(p) = v {
                                self.inline_prims.insert(p.r#type.display().to_string());
                            }
                            (k.clone(), norm_type_expr(v))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            artifacts,
            context: n
                .context
                .as_ref()
                .map(|c| c.sections.iter().map(section_spec).collect())
                .unwrap_or_default(),
            ops: n
                .ops
                .as_ref()
                .map(|ops| ops.iter().map(op_spec).collect())
                .unwrap_or_default(),
            assert: n.assert.as_deref().map(cel_ref).transpose()?,
        })
    }

    /// loop 构造 → ControlledLoop + 内联体降维子图。
    fn loop_spec(
        &mut self,
        lp: &m::LoopDecl,
        owner_path: &str,
    ) -> Result<ControlledLoop, LowerError> {
        let name = self.subgraph_name(owner_path);
        let entry =
            lp.body
                .nodes
                .first()
                .map(|n| n.id.clone())
                .ok_or_else(|| LowerError::EmptyBody {
                    construct: format!("loop@{owner_path}"),
                })?;
        let (nodes, edges) = self.scope(&lp.body.nodes, &lp.body.edges, owner_path)?;
        // graph_version 一律 stamp 父图版本（E2007 在 DSL 面不可触发）
        self.subgraphs.insert(
            name.clone(),
            SubGraph {
                graph_version: self.graph_version.clone(),
                entry,
                nodes,
                edges,
            },
        );
        let mut carried: Vec<CarriedState> = lp
            .carried
            .as_ref()
            .map(|cs| {
                cs.iter()
                    .map(|c| CarriedState {
                        store: c.store.clone(),
                        max_items: c.max_items,
                        max_bytes: c.max_bytes,
                    })
                    .collect()
            })
            .unwrap_or_default();
        carried.sort_by(|a, b| a.store.cmp(&b.store));
        let mut safety_points: Vec<SafetyPoint> = lp
            .safety_points
            .iter()
            .map(|s| SafetyPoint {
                node: s.node.clone(),
                kind: safety_kind(s.kind),
            })
            .collect();
        safety_points.sort_by(|a, b| a.node.cmp(&b.node));
        Ok(ControlledLoop {
            body: name,
            max_iterations: lp.max_iterations,
            carried,
            safety_points,
            args: lp.args.clone().unwrap_or_default(),
            exit_when: lp.exit_when.as_deref().map(cel_ref).transpose()?,
            on_exhausted: lp.on_exhausted.map(on_exhausted),
        })
    }

    /// map 构造 → MapSpec + 内联体降维子图。
    fn map_spec(&mut self, mp: &m::MapDecl, owner_path: &str) -> Result<MapSpec, LowerError> {
        let name = self.subgraph_name(owner_path);
        let entry =
            mp.body
                .nodes
                .first()
                .map(|n| n.id.clone())
                .ok_or_else(|| LowerError::EmptyBody {
                    construct: format!("map@{owner_path}"),
                })?;
        let (nodes, edges) = self.scope(&mp.body.nodes, &mp.body.edges, owner_path)?;
        self.subgraphs.insert(
            name.clone(),
            SubGraph {
                graph_version: self.graph_version.clone(),
                entry,
                nodes,
                edges,
            },
        );
        Ok(MapSpec {
            body: name,
            over: mp.over.clone(),
            max_fanout: mp.max_fanout,
        })
    }
}

// ============================================================
// CEL 引用
// ============================================================

/// CEL 源串 → [`CelRef`]（roots 经 cel-interpreter 提取；digest = SHA-256 于
/// 规范 JSON `{"roots":[…],"source":"…"}`——serde_json Map 默认 BTreeMap 序，
/// 键序=字典序，确定性成立）。cel-parser 0.10.1 对畸形串的 antlr4rust 错误
/// 恢复 panic 以 catch_unwind 兜底归一（承 af-graph `predicate.rs` 纪律）。
fn cel_ref(source: &str) -> Result<CelRef, LowerError> {
    let compile = || Program::compile(source);
    let program = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(compile)) {
        Ok(Ok(p)) => p,
        Ok(Err(e)) => {
            return Err(LowerError::CelCompile {
                source: source.into(),
                detail: e.to_string(),
            })
        }
        Err(_) => {
            return Err(LowerError::CelCompile {
                source: source.into(),
                detail: "cel-parser 内部 panic（antlr4rust 错误恢复路径）".into(),
            })
        }
    };
    let roots: BTreeSet<String> = program
        .references()
        .variables()
        .into_iter()
        .map(str::to_owned)
        .collect();
    let canonical = serde_json::json!({ "roots": roots, "source": source });
    let text = serde_json::to_string(&canonical)
        .map_err(|e| LowerError::Serialize(format!("CelRef 规范序列化失败：{e}")))?;
    Ok(CelRef {
        source: source.to_string(),
        roots,
        digest: hex(&Sha256::digest(text.as_bytes())),
    })
}

/// SHA-256 摘要 → 小写十六进制串（64 字符）。
pub(crate) fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

/// 字节面的 SHA-256 十六进制摘要（G2b 起的 crate 内共享入口——
/// `CelRef.digest`/`dsl_digest`/`registry_digest` 同一确定性口径）。
pub(crate) fn digest_of(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

// ============================================================
// 策略三账
// ============================================================

fn policies(doc: &m::Document) -> Result<IrPolicies, LowerError> {
    let mut budgets: Vec<BudgetEntry> = doc
        .policies
        .budgets
        .iter()
        .map(|b| BudgetEntry {
            id: b.id.clone(),
            dimension: budget_dimension(b.dimension),
            scope: b.scope.clone(),
            cap: b.cap,
        })
        .collect();
    budgets.sort_by(|a, b| a.id.cmp(&b.id));
    let stop = doc
        .policies
        .stop
        .iter()
        .map(|s| {
            Ok(StopEntry {
                when: cel_ref(&s.when)?,
                outcome: stop_outcome(s.outcome),
            })
        })
        .collect::<Result<Vec<_>, LowerError>>()?;
    let failures = doc.policies.failures.iter().map(failure_entry).collect();
    Ok(IrPolicies {
        budgets,
        stop,
        failures,
    })
}

fn failure_entry(f: &m::FailureDecl) -> FailureEntry {
    FailureEntry {
        class: failure_class(f.class),
        retry: f.retry.as_ref().map(|r| RetrySpec {
            max: r.max,
            backoff_ms: r.backoff_ms,
        }),
        route: f.route.clone(),
        feedback: f.feedback.clone(),
        fallback_route: f.fallback_route.clone(),
        terminal: f.terminal,
        outcome: f.outcome.map(stop_outcome),
    }
}

// ============================================================
// 逐结构镜像（DSL model → IR；枚举一一对应，显式 match 可审计）
// ============================================================

fn set_of(v: &Option<Vec<String>>) -> BTreeSet<String> {
    v.as_ref()
        .map(|xs| xs.iter().cloned().collect())
        .unwrap_or_default()
}

fn store_spec(s: &m::StoreDecl) -> StoreSpec {
    StoreSpec {
        name: s.name.clone(),
        kind: match s.r#type {
            m::StoreKind::MessageLog => StoreKind::MessageLog,
            m::StoreKind::Document => StoreKind::Document,
            m::StoreKind::Collection => StoreKind::Collection,
            m::StoreKind::Kv => StoreKind::Kv,
            m::StoreKind::VectorIndex => StoreKind::VectorIndex,
        },
        item: s.item.as_ref().map(|p| p.display()),
        value: s.value.as_ref().map(|p| p.display()),
        writer: s.writer.clone(),
        lifecycle: match s.lifecycle {
            m::Lifecycle::Run => Lifecycle::Run,
            m::Lifecycle::Iteration => Lifecycle::Iteration,
            m::Lifecycle::Persistent => Lifecycle::Persistent,
        },
        retention: Retention {
            policy: s.retention.policy.map(|p| match p {
                m::RententionPolicy::RollingWindow => RententionPolicy::RollingWindow,
            }),
            max_items: s.retention.max_items,
            max_bytes: s.retention.max_bytes,
            keep_system: s.retention.keep_system,
        },
        eval_input: s.eval_input.unwrap_or(false),
    }
}

fn human_spec(h: &m::HumanDecl) -> HumanSpec {
    HumanSpec {
        station: h.station.clone(),
        role: h.role.clone(),
        quorum: h.quorum,
        resume: ResumeSpec {
            // 透传 JSON Schema，键序规范化（ir.rs 契约护栏）
            schema: canonical_value(&h.resume.schema),
        },
    }
}

fn escape_spec(e: &m::EscapeDecl) -> EscapeSpec {
    EscapeSpec {
        kind: e.kind.map(|k| match k {
            m::EscapeKind::DynamicToolRegistry => EscapeKind::DynamicToolRegistry,
            m::EscapeKind::ProgramSynthesis => EscapeKind::ProgramSynthesis,
            m::EscapeKind::MetaLoop => EscapeKind::MetaLoop,
            m::EscapeKind::RuntimeContextMutation => EscapeKind::RuntimeContextMutation,
            m::EscapeKind::EnvAdapter => EscapeKind::EnvAdapter,
            m::EscapeKind::FreeTextParser => EscapeKind::FreeTextParser,
            m::EscapeKind::LlmPredicate => EscapeKind::LlmPredicate,
        }),
        impl_ref: e.impl_ref.clone(),
        reason: e.reason.clone(),
        owner: e.owner.clone(),
        share_budget_pct: e.share_budget_pct,
    }
}

fn safety_kind(k: m::SafetyPointKind) -> SafetyPointKind {
    match k {
        m::SafetyPointKind::Check => SafetyPointKind::Check,
        m::SafetyPointKind::Gate => SafetyPointKind::Gate,
        m::SafetyPointKind::Human => SafetyPointKind::Human,
    }
}

fn on_exhausted(o: m::OnExhausted) -> OnExhausted {
    match o {
        m::OnExhausted::Continue => OnExhausted::Continue,
        m::OnExhausted::Fail => OnExhausted::Fail,
        m::OnExhausted::KeepBest => OnExhausted::KeepBest,
    }
}

fn section_spec(s: &m::SectionDecl) -> SectionSpec {
    SectionSpec {
        from: s.from.clone(),
        quota_tokens: s.quota_tokens,
        template_ref: s.template_ref.clone(),
        vars: s.vars.clone().unwrap_or_default(),
        fields: s.fields.clone().unwrap_or_default(),
        name: s.name.clone(),
        port: s.port.clone(),
        store: s.store.clone(),
        policy: s.policy.clone(),
        // 透传 Value 表，值侧键序规范化（ir.rs 契约护栏）
        args: s.args.as_ref().map(canonical_value_map).unwrap_or_default(),
        component: s.component.clone(),
        optional: s.optional,
    }
}

fn op_spec(o: &m::OpDecl) -> OpSpec {
    OpSpec {
        op: o.op.clone(),
        port: o.port.clone(),
        ports: o.ports.clone().unwrap_or_default(),
        into: o.into.clone(),
        fields: o.fields.clone().unwrap_or_default(),
        k: o.k,
        drop: o.drop.clone().unwrap_or_default(),
        by: o.by.clone(),
        prefer: o.prefer.clone(),
        seed_ref: o.seed_ref.clone(),
        max_tokens_each: o.max_tokens_each,
    }
}

fn budget_dimension(d: m::BudgetDimension) -> BudgetDimension {
    match d {
        m::BudgetDimension::Turns => BudgetDimension::Turns,
        m::BudgetDimension::ModelCalls => BudgetDimension::ModelCalls,
        m::BudgetDimension::TokensIn => BudgetDimension::TokensIn,
        m::BudgetDimension::TokensOut => BudgetDimension::TokensOut,
        m::BudgetDimension::TokensTotal => BudgetDimension::TokensTotal,
        m::BudgetDimension::ToolCalls => BudgetDimension::ToolCalls,
        m::BudgetDimension::Cents => BudgetDimension::Cents,
        m::BudgetDimension::ClockMs => BudgetDimension::ClockMs,
    }
}

fn stop_outcome(o: m::StopOutcome) -> StopOutcome {
    match o {
        m::StopOutcome::Completed => StopOutcome::Completed,
        m::StopOutcome::Failed => StopOutcome::Failed,
        m::StopOutcome::EscalatedHuman => StopOutcome::EscalatedHuman,
    }
}

fn failure_class(c: m::FailureClass) -> FailureClass {
    match c {
        m::FailureClass::TransportTransient => FailureClass::TransportTransient,
        m::FailureClass::EnvironmentFailure => FailureClass::EnvironmentFailure,
        m::FailureClass::SchemaInvalid => FailureClass::SchemaInvalid,
        m::FailureClass::ToolError => FailureClass::ToolError,
        m::FailureClass::Timeout => FailureClass::Timeout,
        m::FailureClass::GateRefused => FailureClass::GateRefused,
        m::FailureClass::BudgetExhausted => FailureClass::BudgetExhausted,
        m::FailureClass::PolicyStopped => FailureClass::PolicyStopped,
        m::FailureClass::Unauthorized => FailureClass::Unauthorized,
    }
}
