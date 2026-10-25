//! 图执行器 v1——可执行计划的节点级消费（F-第四单执行长缺口销项）。
//!
//! 一句话：planbind 四核对（EX4004 口径；核对清单=信封 decode/E9004
//! digest/一致性/模板装载，承 planbind.rs 模块头先例术语）通过后，把
//! `ExecutablePlan` 按
//! 拓扑序逐节点驱动到真实执行面——**v1 范围 = sop_full 线性链形态**（可
//! 序贯拓扑序的 DAG；loop/map 构造节点与环拓扑 → EX4008 登记后续）。
//!
//! 节点分派（ask 口径）：
//! - **task 节点** = 模型节点运行时环（复用 swev 环模式）：
//!   - system 提示词来自 `context.sections[].template_ref` 的包 prompts
//!     装载正文（裁决10 唯一机制，planbind ④ 装订）；
//!   - `uses.model` 档位 = 注册表 model_route 声明的端点（af-compiler
//!     `EndpointSpec` 镜像 → af-harness 字段级转换；key_cmd 按 key_ref
//!     反查 profile 表，未匹配回退基座档案）；
//!   - 节点级工具面 = `uses.tools` 逐件 builtin 实现（`pkg::sweng`），
//!     声明 = `sweng::tool_decl`、执行 = 注册表闭包（经 `AgentLaunch.tools`）；
//!   - 按节点 turns 预算帽 = node_exec[].budget（dimension=turns），
//!     未声明取 spec 缺省；
//! - **check 节点** = unit 直调：`uses.unit` 经注册表 impl 解析
//!   （`executor:builtin/<名>@af-execline` 锚，裁决13）→ 进程内调用
//!   （v1 已接线：`pkg.sweng.test_unit`）。
//!
//! typed envelope 交接（EX4006）：节点产物按 plan edges/types 校验后入
//! 下游输入——
//! - 图输入先过 `types[interface.input_type]` schema 校验；
//! - task 节点终态消息必须是 JSON 对象且键集 = artifact_contract 端口名
//!   （envelope<T>：端口载荷 = 对象值；多端口天然支持）；
//! - 每条边的 `(from, output)` 必须有已产 envelope（无产者拒）；
//! - 终态工件（`interface.output_from` 的 is_final 端口）过
//!   `types[interface.output_type]` schema 校验；
//! - 违例 fail-loud：EX4006，不落工件面、不驱动下游。
//!
//! 预算面（EX4007）：plan 级 `cost_cap`（cents）/`token_cap`（tokens_total）
//! 由执行器强制——llm.cents 口径 1 次模型调用 = 1¢（[`crate::agent::
//! llm_cents_policy`] 的 per_turn 记账面），节点完成后累账，超限即熔断
//! 拒起下一节点；单节点 launch 的 spend_cap 同步收敛到剩余额度。
//!
//! 订单生命周期零改动（约束 e）：bind/voucher/release/start 在订单面
//! （`OrderLifecycleDriver`），本单元只在 `execute()` 内驱动图。
//!
//! 范围外（登记后续，EX4008 拒驱动）：loop/map 构造节点、环拓扑、route
//! 边守卫（CEL）、stop 条件求值、stores 读写运行时、节点级 retry、
//! 非 builtin 的 unit impl_ref、多 attempt 图内重试。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use af_compiler::ir::{BudgetDimension, SectionSpec};
use af_compiler::{ComponentRegistry, ExecutablePlan};
use af_harness::{AgentSpec, ContextTemplate, ToolDecl};
use af_workorder::WorkArtifact;
use serde_json::Value;

use crate::agent::{
    AgentLaunch, AgentRunFailure, AgentRunOutcome, AgentRunner, ToolBinding, ToolFn,
};
use crate::error::ExeclineError;
use crate::exec::{ExecContext, ExecOutput, ExecuteUnit};
use crate::hostbridge::HostBridge;
use crate::pkg::sweng;
use crate::planbind::{self, BindExpectations, PlanBind};
use crate::suite::swev;
use crate::suite::tb::{ContainerGuard, TbEnv};

/// 节点未声明 turns 帽时的缺省（SOP 代际口径：sop_full coder 60 / reviewer
/// 12 / sop_minimal repair 60 的主工作节点档）。
pub const DEFAULT_NODE_TURNS: u32 = 60;
/// 完成信号词与连续无工具轮上限（swev 环模式原样，swev_unit.rs:94-96）。
pub const COMPLETION_SIGNAL: &str = "TASK_COMPLETE";
pub const TOOL_FREE_TURN_LIMIT: u32 = 3;
/// 单节点 launch 缺省成本硬顶（美分；plan cost_cap 在场时收敛到剩余额度）。
pub const DEFAULT_NODE_SPEND_CAP_CENTS: i64 = 300;

// ============================================================
// 单元声明
// ============================================================

/// 图执行单元静态声明（对齐契约 §1.2 unit.json 的装载面）。
#[derive(Debug, Clone)]
pub struct GraphUnitSpec {
    /// 组件注册表根（`af/packages`）——planbind digest 核对 + model_route/
    /// unit impl 解析的同一注册表。
    pub registry_root: PathBuf,
    /// 基座档案名（容器命名、key_cmd 反查未匹配时的回退）。
    pub profile: String,
    /// 节点未声明 turns 帽的缺省。
    pub default_node_turns: u32,
    /// 单节点 launch 成本硬顶缺省。
    pub spend_cap_cents: i64,
    /// 工具单次开销（swev 环同款记账面）。
    pub tool_spend_cents: i64,
    /// swebench_eval 判定单元的判分锚（B 席嫁接，合流批；None = 该单元
    /// 分派时 fail-loud 拒载——不静默跳判）。装配面（swev 环 spec.judge
    /// 同源）：订单层 wiring 或测试注入。
    pub judge: Option<crate::judge::JudgeLocator>,
    /// 单元版本。
    pub unit_version: &'static str,
}

impl GraphUnitSpec {
    /// v1 缺省构造。
    pub fn v1(profile: impl Into<String>, registry_root: impl Into<PathBuf>) -> Self {
        Self {
            registry_root: registry_root.into(),
            profile: profile.into(),
            default_node_turns: DEFAULT_NODE_TURNS,
            spend_cap_cents: DEFAULT_NODE_SPEND_CAP_CENTS,
            tool_spend_cents: 1,
            judge: None,
            unit_version: "0.1.0",
        }
    }
}

/// 图执行单元：两个可注入缝（HostBridge 容器宿主 / AgentRunner 模型执行），
/// 消费 `<data_root>/<task>/{plan.json, task.json}` 任务包。
pub struct GraphExecuteUnit {
    spec: GraphUnitSpec,
    bridge: Arc<dyn HostBridge + Send + Sync>,
    runner: Arc<dyn AgentRunner>,
}

impl std::fmt::Debug for GraphExecuteUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphExecuteUnit")
            .field("unit_id", &self.unit_id())
            .field("profile", &self.spec.profile)
            .finish_non_exhaustive()
    }
}

impl GraphExecuteUnit {
    /// 装载：基座 profile 可解析（fail-loud，拒在装订/起容器之前）。
    pub fn new(
        spec: GraphUnitSpec,
        bridge: Arc<dyn HostBridge + Send + Sync>,
        runner: Arc<dyn AgentRunner>,
    ) -> Result<Self, ExeclineError> {
        crate::profile::profile(&spec.profile).map_err(|err| ExeclineError::UnitFailed {
            detail: format!("装载失败：{err}"),
        })?;
        Ok(Self {
            spec,
            bridge,
            runner,
        })
    }

    fn fail(&self, detail: impl Into<String>) -> ExeclineError {
        ExeclineError::UnitFailed {
            detail: detail.into(),
        }
    }

    fn envelope_err(detail: impl Into<String>) -> ExeclineError {
        ExeclineError::EnvelopeMismatch {
            detail: detail.into(),
        }
    }
}

impl ExecuteUnit for GraphExecuteUnit {
    fn unit_id(&self) -> &str {
        "graph-unit"
    }

    fn execute(&self, ctx: &ExecContext<'_>) -> Result<ExecOutput, ExeclineError> {
        // ---- setup 相①：任务包 + 计划装订四核对（planbind 唯一正本，先例术语）----
        let task_dir = ctx.data_root.join(ctx.task_id);
        let plan_src = std::fs::read_to_string(task_dir.join("plan.json")).map_err(|err| {
            self.fail(format!(
                "读计划失败（v1 约定：计划随任务包 <data_root>/<task>/plan.json；submit 级 --plan 档=开放问题 8 留 v0.2）: {err}"
            ))
        })?;
        let input: Value = serde_json::from_str(
            &std::fs::read_to_string(task_dir.join("task.json"))
                .map_err(|err| self.fail(format!("读任务包 task.json 失败: {err}")))?,
        )
        .map_err(|err| self.fail(format!("task.json 不是合法 JSON: {err}")))?;
        // 模板引用先收口（装订器④的装载清单）：plan JSON 通用遍历
        // node_exec[].context[].template_ref。
        let template_refs = collect_template_refs(&plan_src);
        let refs: Vec<&str> = template_refs.iter().map(String::as_str).collect();
        let bound = planbind::bind(
            &plan_src,
            &self.spec.registry_root,
            &refs,
            &BindExpectations::default(),
        )?;
        let plan = &bound.plan;

        // ---- setup 相②：图输入 envelope 校验 + 线性化 + 预算面提取 ----
        let input_schema = plan
            .body
            .types
            .get(&plan.body.interface.input_type)
            .cloned()
            .ok_or_else(|| {
                Self::envelope_err(format!(
                    "图输入类型 {} 不在 types 词典",
                    plan.body.interface.input_type
                ))
            })?;
        if let Err(detail) = validate_schema(&input, &input_schema) {
            return Err(Self::envelope_err(format!(
                "图输入不符 {} schema：{}",
                plan.body.interface.input_type, detail
            )));
        }
        let order = linearize(plan)?;
        let budget = PlanBudgets::of(plan)?;

        // ---- setup 相③：任务材料 + 容器（swev 环模式复用）----
        let task = GraphTask::parse(&input)?;
        let attempt_dir = ctx.attempt_dir;
        std::fs::create_dir_all(attempt_dir)
            .map_err(|err| self.fail(format!("建 run 目录失败: {err}")))?;
        let artifacts_dir = attempt_dir.join("artifacts");
        std::fs::create_dir_all(&artifacts_dir)
            .map_err(|err| self.fail(format!("建工件面失败: {err}")))?;
        let env = Arc::new(
            swev::setup(
                self.bridge.as_ref(),
                ctx.host_alias,
                &task.as_swev_task(),
                &self.spec.profile,
            )
            .map_err(|err| self.fail(format!("起容器失败: {err}")))?,
        );
        let _guard = ContainerGuard {
            bridge: self.bridge.clone(),
            env: Some(env.clone()),
        };

        // ---- 驱动相：按拓扑序逐节点 ----
        let registry = ComponentRegistry::load(&self.spec.registry_root).map_err(|err| {
            ExeclineError::PlanBindMismatch {
                detail: format!("组件注册表装载失败（{:#}）", err),
            }
        })?;
        let mut stage = Drive::default();
        for node_id in &order {
            budget.gate(&stage)?;
            let node = plan
                .body
                .node_exec
                .iter()
                .find(|n| &n.node_id == node_id)
                .expect("线性化序内节点必有 node_exec 投影（planbind ③ 已对齐）");
            let kind = plan
                .body
                .graph
                .nodes
                .iter()
                .find(|n| &n.id == node_id)
                .expect("node_exec 投影必归属 graph 节点（planbind ③ 已对齐）")
                .kind
                .clone();
            // 入边工件集齐 + consumes 契约复核（下游面，B 席嫁接）：节点起动前
            // 对入边载荷按 node.consumes 类型表达式过同一 check_typed 正门——
            // 与上游 emissions 面互不替代（双声明各自独立核）。无 consumes
            // 声明 = 不核（如实口径；旧计划缺省空）。
            let mut inputs: BTreeMap<String, Value> = BTreeMap::new();
            for edge in &plan.body.graph.edges {
                if &edge.to == node_id {
                    let payload = stage.payload(&edge.from, &edge.output).ok_or_else(|| {
                        Self::envelope_err(format!(
                            "节点 {node_id} 入边端口 {} 无上游工件（拓扑序断裂或产者缺席）",
                            edge.input
                        ))
                    })?;
                    inputs.insert(edge.input.clone(), payload);
                }
            }
            for (port, type_expr) in &node.consumes {
                if let Some(value) = inputs.get(port) {
                    check_typed(&plan.body.types, node_id, port, type_expr, value)?;
                }
            }
            let envelopes = match kind.as_str() {
                "task" => self.drive_task_node(
                    ctx,
                    plan,
                    &bound,
                    &registry,
                    node,
                    &input,
                    &mut stage,
                    env.clone(),
                    &budget,
                )?,
                "check" => {
                    self.drive_check_node(ctx, &task, &registry, node, env.clone(), &inputs)?
                }
                other => {
                    return Err(ExeclineError::GraphShapeUnsupported {
                        detail: format!("节点 {node_id} kind={other} 超v1分派面（task|check）"),
                    })
                }
            };
            // envelope 结构校验 + 落工件面 + 入交接账。
            stage.commit(plan, node_id, node, envelopes, &artifacts_dir)?;
        }

        // ---- 收尾相：终态工件 schema 校验 + 订单面产出 ----
        let final_port = final_artifact_port(plan)?;
        let final_payload = stage
            .payload(&plan.body.interface.output_from, &final_port)
            .ok_or_else(|| {
                Self::envelope_err(format!(
                    "终态工件 {}.{} 无 envelope 产出",
                    plan.body.interface.output_from, final_port
                ))
            })?;
        // 终态工件 typed 正门（对抗面 2026-10-25）：output_type 经 [`check_typed`]
        // ——与 emissions（上游面）/consumes（下游面）同一校验器（命名类型解析 +
        // list<T> 拆包，sop_no_qa 的 `list<code_file>` 表达式形态同覆盖）。此前
        // `types.get(output_type)` 缺席即**静默跳过**终态校验——ghost 类型/非对象
        // schema/非字符串 type 段均可溜过（EX4006 点名收口）。
        check_typed(
            &plan.body.types,
            &plan.body.interface.output_from,
            &final_port,
            &plan.body.interface.output_type,
            &final_payload,
        )?;
        // 判定语义：判定型终态（含 boolean pass 字段，如 qa_report）取其值；
        // 非判定型（如 patch）产出即合格——envelope 已过结构校验。
        let verdict = final_payload
            .get("pass")
            .and_then(Value::as_bool)
            .or(Some(true));
        let artifact_id = format!("graph-{}-{}", plan.body.interface.output_from, final_port);
        let row = serde_json::json!({
            "suite": "graph",
            "plan_id": plan.body.plan_id,
            "order_id": ctx.order_id,
            "task_id": ctx.task_id,
            "profile": self.spec.profile,
            "nodes": order.len(),
            "cents_used": stage.cents_used,
            "tokens_used": stage.tokens_used,
            "verdict": verdict,
            "unit": self.unit_id(),
            "unit_version": self.spec.unit_version,
        });
        ctx.record_sink
            .append_v1(&row.to_string())
            .map_err(|err| self.fail(format!("写 records 失败: {err}")))?;
        let mut outputs = vec![task_dir.join("plan.json"), task_dir.join("task.json")];
        outputs.extend(stage.envelope_files.values().cloned());
        Ok(ExecOutput {
            artifacts: vec![WorkArtifact {
                artifact_id,
                is_final: true,
                qualified: verdict == Some(true),
            }],
            outputs,
            note: format!(
                "图执行器驱动 {} 节点（{} → …），判定 {:?}",
                order.len(),
                order.first().map(String::as_str).unwrap_or(""),
                verdict
            ),
            verdict,
            record: Some(row),
            evidence: stage.envelope_files.values().cloned().collect(),
        })
    }
}

impl GraphExecuteUnit {
    /// task 节点 = 模型节点运行时环（swev 环模式）。
    #[allow(clippy::too_many_arguments)]
    fn drive_task_node(
        &self,
        ctx: &ExecContext<'_>,
        plan: &ExecutablePlan,
        bound: &PlanBind,
        registry: &ComponentRegistry,
        node: &af_compiler::plan::PlanNodeExec,
        input: &Value,
        stage: &mut Drive,
        env: Arc<TbEnv>,
        budget: &PlanBudgets,
    ) -> Result<NodeEnvelopes, ExeclineError> {
        // uses.model 档位：注册表 model_route 端点（无声明 → 环缺模型面，拒）。
        let route_ref =
            node.components.model_route.as_deref().ok_or_else(|| {
                self.fail(format!("task 节点 {} 无 uses.model 声明", node.node_id))
            })?;
        let endpoint = resolve_route_endpoint(registry, route_ref, &self.spec.profile)
            .map_err(|err| self.fail(format!("模型路由 {route_ref} 解析失败: {err}")))?;
        let key_cmd = crate::profile::resolve_key_cmd(&endpoint.key_ref).unwrap_or_else(|| {
            crate::profile::profile(&self.spec.profile)
                .expect("装载期已保证基座档案可解析")
                .key_cmd
        });
        // 节点 turns 帽：声明值 > spec 缺省。u64→u32 **饱和**换算（对抗面
        // 2026-10-25）：`as u32` 截断曾把 cap=2^32 静默折成 0 帽——帽越大约束
        // 越紧的语义倒置；饱和语义 = 巨帽≈无帽（fail-open 方向不变）。
        let max_turns = node
            .budget
            .iter()
            .find(|b| b.dimension == BudgetDimension::Turns)
            .map(|b| u32::try_from(b.cap).unwrap_or(u32::MAX))
            .unwrap_or(self.spec.default_node_turns);
        // 节点工具面：uses.tools 逐件 builtin（声明 + 执行闭包同源）。
        // pkg.sweagent.env 的模型面名 = "bash"（T1 先例）——由下方 bash 专缝
        // 承载（HarnessRunner 固定注册 "bash"），tools 面跳过防重名。
        // A 席合流迁移：工具通道统一走主线 report-ring 的 ToolBinding/extra_tools
        // 缝（ToolFn = &Value→Result<Value,String>），根凭证自动覆盖工具名。
        let mut tools: Vec<ToolBinding> = Vec::new();
        for tool in &node.components.tools {
            let decl = sweng::tool_decl(&tool.reference, env.workdir()).ok_or_else(|| {
                self.fail(format!("工具 {} 无模型面声明（15 件表外）", tool.reference))
            })?;
            if decl.name == "bash" {
                continue;
            }
            let f = node_tool_fn(&tool.reference, self.bridge.clone(), env.clone()).ok_or_else(
                || {
                    self.fail(format!(
                        "工具 {} 无 v1 builtin 执行面（登记后续）",
                        tool.reference
                    ))
                },
            )?;
            tools.push(ToolBinding {
                name: decl.name.clone(),
                handler: f,
            });
        }
        let toolset: Vec<ToolDecl> = node
            .components
            .tools
            .iter()
            .filter_map(|t| sweng::tool_decl(&t.reference, env.workdir()))
            .collect();
        // bash 面（swev 环模式）：env 工具（pkg.sweagent.env）经此执行；
        // 未声明 env 的节点模型看不见 bash（toolset 无声明），闭包不生效。
        let env_bash = Arc::new(sweng::EnvBash::new(self.bridge.clone(), env.clone()));
        let bash: crate::agent::BashFn =
            Arc::new(move |command: &str| env_bash.call(command, None));
        // 提示词：system = template 段装载正文；user = input 字段段 + edge 端口段。
        let (system, user) =
            compose_prompts(bound, &node.context, input, plan, stage, &node.node_id)
                .map_err(|err| self.fail(format!("节点 {} 提示词装配失败: {err}", node.node_id)))?;
        let ports: Vec<&str> = node
            .artifact_contract
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        // 输出契约**前置**（三跑真模型判例：声明轮消息面被 20-60 轮 bash 工作
        // 惯性稳定淹没——模型以 prose 总结 + `echo TASK_COMPLETE` 收尾）。
        // 收敛机制 = **声明工具**（`pkg.sweagent.aci.submit` 同款完成载体先例）：
        // 模型调 `declare({<端口>: <载荷>})`——工具输出首行 TASK_COMPLETE 即环
        // 完成信号，参数即 envelope 声明（优先源）；终态消息 JSON 为回退源
        // （两源同一契约，EX4006 对双源违例如实拒收）。
        // 终态端口（is_final）的载荷 schema 计划可推导（interface.output_type）
        // ——一并写进契约；非终态端口类型不在计划投影，v0.2 登记。
        let final_schema_note = {
            let note = node
                .artifact_contract
                .iter()
                .filter(|a| a.is_final)
                .filter_map(|a| {
                    plan.body
                        .types
                        .get(&plan.body.interface.output_type)
                        .map(|schema| (a.name.clone(), schema.clone()))
                })
                .map(|(port, schema)| format!("    {port:?}: {schema}"))
                .collect::<Vec<_>>()
                .join(",\n");
            if note.is_empty() {
                String::new()
            } else {
                format!("The value of each key must match:\n{note}")
            }
        };
        let contract = format!(
            "[OUTPUT CONTRACT — your work is NOT complete until you do this]\n\
             1. Do the task below with bash tool calls.\n\
             2. When done, call the `declare` tool with ONE argument — a JSON object keyed by \
             artifact port name: {}. Calling `declare` ends the run (its tool output is the \
             completion signal).\n{}",
            serde_json::to_string(&ports).unwrap_or_default(),
            if final_schema_note.is_empty() {
                String::new()
            } else {
                format!("   {final_schema_note}\n")
            },
        );
        let user = format!("{contract}\n\n[task]\n{user}");
        // declare 工具（环组装面）：声明落共享单元 + 返回完成信号。
        let (declare_decl, declare_fn, declaration) = declare_tool(&ports);
        let spend_cap = budget.node_spend_cap(self.spec.spend_cap_cents, stage.cents_used);
        let node_dir = ctx.attempt_dir.join("nodes").join(&node.node_id);
        std::fs::create_dir_all(&node_dir)
            .map_err(|err| self.fail(format!("建节点目录失败: {err}")))?;
        let launch = AgentLaunch {
            attempt_dir: &node_dir,
            run_id: format!(
                "{}-{}",
                input["instance_id"].as_str().unwrap_or(ctx.task_id),
                node.node_id
            ),
            user_message: user,
            spec: AgentSpec {
                endpoint,
                context_template: ContextTemplate {
                    // 环组装面附加声明纪律（模板正文不动字节——包资产是正本，
                    // 环拥有的装配段另起一段）。
                    system: format!(
                        "{system}\n\n[artifact protocol] Your finished work is delivered by \
                         calling the `declare` tool with one JSON object keyed by artifact port \
                         name(s); that call also ends the run. The user message carries the \
                         exact keys and schemas."
                    ),
                    slots: Vec::new(),
                },
                toolset: {
                    let mut set = toolset;
                    set.push(declare_decl);
                    set
                },
                policy: crate::agent::llm_cents_policy(
                    max_turns,
                    COMPLETION_SIGNAL,
                    TOOL_FREE_TURN_LIMIT,
                ),
            },
            bash,
            extra_tools: {
                let mut set = tools;
                set.push(ToolBinding {
                    name: "declare".to_string(),
                    handler: declare_fn,
                });
                set
            },
            key_cmd,
            spend_cap_cents: spend_cap,
            tool_spend_cents: self.spec.tool_spend_cents,
            budget_ledger_cents: spend_cap,
        };
        let report = match self.runner.run(launch) {
            Ok(AgentRunOutcome { report, .. }) => report,
            Err(AgentRunFailure { message, .. }) => {
                return Err(self.fail(format!("节点 {} 模型环失败: {message}", node.node_id)))
            }
        };
        stage.account_node(&report);
        // harness 侧预算耗尽归并（B 席嫁接）：RunStatus::BudgetExhausted 不当
        // 成功静默——EX4007 结构化熔断（launch 硬顶 = plan cost_cap 收敛面）。
        if let af_harness::RunStatus::BudgetExhausted { .. } = &report.status {
            return Err(ExeclineError::BudgetCapExceeded {
                budget_id: "llm.cents".into(),
                dimension: "cents".into(),
                spent: stage.cents_used,
                cap: spend_cap.max(0) as u64,
            });
        }
        // envelope 声明三源（同一契约）：①declare 工具参数（端口键控对象，
        // 优先）；②裸 declare（参数空）→ 端口 schema 全 required 可推导
        //（diff/files，ground truth 容器取证）时按推导交付；③终态 assistant
        // 消息 JSON（回退——FakeRunner/守规模型面）。端口键集校验在 commit 相。
        let declared = declaration
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(value) = declared {
            let bare = value.as_object().is_some_and(|obj| obj.is_empty());
            if !bare {
                let obj = value.as_object().ok_or_else(|| {
                    Self::envelope_err(format!(
                        "节点 {} declare 参数不是 JSON 对象（须按端口名键控）",
                        node.node_id
                    ))
                })?;
                return Ok(NodeEnvelopes {
                    ports: obj.clone().into_iter().collect(),
                    derived: Vec::new(),
                });
            }
            // 裸 declare = 提交工作区：唯一工件端口且 required ⊆ {diff, files}
            // 才可推导（其余形状裸调 = 声明缺失，EX4006）。
            if node.artifact_contract.len() == 1 {
                let port = &node.artifact_contract[0].name;
                let schema = plan
                    .body
                    .types
                    .get(&plan.body.interface.output_type)
                    .cloned();
                if schema.as_ref().is_some_and(derivable_port_fields) {
                    let diff =
                        swev::capture_diff(self.bridge.as_ref(), ctx.host_alias, &env, &node_dir)
                            .map_err(|err| {
                            self.fail(format!(
                                "节点 {} 裸 declare 工作区取证失败: {err}",
                                node.node_id
                            ))
                        })?;
                    let payload =
                        serde_json::json!({ "diff": diff, "files": files_of_diff(&diff) });
                    let mut ports = BTreeMap::new();
                    ports.insert(port.clone(), payload);
                    return Ok(NodeEnvelopes {
                        ports,
                        derived: vec!["diff".into(), "files".into()],
                    });
                }
            }
            return Err(Self::envelope_err(format!(
                "节点 {} 裸 declare 但端口形状不可 ground-truth 推导（须端口键控参数声明）",
                node.node_id
            )));
        }
        let text = final_assistant_text(&report).ok_or_else(|| {
            Self::envelope_err(format!(
                "节点 {} 无 declare 调用且无终态 assistant 声明消息（envelope<T> 契约）",
                node.node_id
            ))
        })?;
        let parsed = parse_declaration(&text).map_err(|err| {
            Self::envelope_err(format!(
                "节点 {} 声明缺失：declare 未调用，终态消息也非合法 JSON 声明：{err}",
                node.node_id
            ))
        })?;
        let obj = parsed
            .as_object()
            .ok_or_else(|| {
                Self::envelope_err(format!(
                    "节点 {} 声明不是 JSON 对象（须按端口名键控）",
                    node.node_id
                ))
            })?
            .clone();
        Ok(NodeEnvelopes {
            ports: obj.into_iter().collect(),
            derived: Vec::new(),
        })
    }

    /// check 节点 = unit 直调（uses.unit 经注册表 impl 解析）。分发表 v1.1
    /// （B 席嫁接，合流批）：pkg.sweng.test_unit 之外接线
    /// pkg.sweagent.aci_schema（入边工件结构校验纯函数）、
    /// pkg.sweagent.human_gate（fail-closed 恒 ask:false）、
    /// pkg.sweagent.swebench_eval（判分链；spec.judge 缺席 fail-loud 拒载）。
    #[allow(clippy::too_many_arguments)]
    fn drive_check_node(
        &self,
        ctx: &ExecContext<'_>,
        task: &GraphTask,
        registry: &ComponentRegistry,
        node: &af_compiler::plan::PlanNodeExec,
        env: Arc<TbEnv>,
        inputs: &BTreeMap<String, Value>,
    ) -> Result<NodeEnvelopes, ExeclineError> {
        let unit = node
            .unit
            .as_ref()
            .ok_or_else(|| self.fail(format!("check 节点 {} 无 uses.unit 声明", node.node_id)))?;
        let decl = registry
            .versions_of(&unit.unit_id)
            .and_then(|v| v.get(&unit.unit_version))
            .ok_or_else(|| {
                self.fail(format!(
                    "uses.unit {}@{} 不在注册表",
                    unit.unit_id, unit.unit_version
                ))
            })?;
        let impl_ref = decl.impl_ref.as_deref().unwrap_or_default();
        if !sweng::is_builtin_impl(impl_ref) {
            return Err(self.fail(format!(
                "uses.unit {} impl_ref {impl_ref:?} 非 builtin 锚（包路径实现=登记后续）",
                unit.unit_id
            )));
        }
        let (name, host) = sweng::builtin_target(impl_ref).expect("is_builtin_impl 已判别");
        if host != sweng::BUILTIN_HOST_CRATE {
            return Err(self.fail(format!(
                "uses.unit {} builtin 宿主 {host} 非 {}（跨 crate 装载=登记后续）",
                unit.unit_id,
                sweng::BUILTIN_HOST_CRATE
            )));
        }
        // v1.1 分发表：四件 builtin 判定单元（B 席嫁接面）。
        let value = match name.as_str() {
            n if n == sweng::name_of(sweng::REF_SWENG_TEST_UNIT) => sweng::test_unit(
                self.bridge.as_ref(),
                &env,
                task.test_command.as_deref().unwrap_or(DEFAULT_TEST_COMMAND),
                None,
            ),
            "pkg.sweagent.aci_schema" => {
                let Some(action) = inputs.values().next() else {
                    return Err(self.fail(format!(
                        "节点 {} aci_schema 需要入边工件（agent_action）",
                        node.node_id
                    )));
                };
                sweng::aci_schema(action)
            }
            "pkg.sweagent.human_gate" => {
                sweng::human_gate(&sweng::HumanGateConfig::default(), &Value::Null)
            }
            "pkg.sweagent.swebench_eval" => {
                let Some(judge) = self.spec.judge.as_ref() else {
                    return Err(self.fail(format!(
                        "节点 {} swebench_eval 需要判分锚（spec.judge 未配置，fail-loud 拒载——不静默跳判）",
                        node.node_id
                    )));
                };
                let swev_task = task.as_swev_task();
                let work_dir = ctx.attempt_dir.join("nodes").join(&node.node_id);
                let task_json = ctx.data_root.join(ctx.task_id).join("task.json");
                std::fs::create_dir_all(&work_dir)
                    .map_err(|err| self.fail(format!("建节点目录失败: {err}")))?;
                sweng::swebench_eval(
                    self.bridge.as_ref(),
                    &env,
                    judge,
                    &swev_task,
                    &work_dir,
                    &task_json,
                )
                .verdict
            }
            other => {
                return Err(self.fail(format!(
                    "uses.unit {other} 无直调面（分发表内：pkg.sweng.test_unit + \
                     pkg.sweagent.aci_schema/human_gate/swebench_eval）"
                )))
            }
        };
        let port = node
            .artifact_contract
            .first()
            .map(|a| a.name.clone())
            .ok_or_else(|| {
                Self::envelope_err(format!(
                    "check 节点 {} 无 artifact_contract 端口",
                    node.node_id
                ))
            })?;
        let mut ports = BTreeMap::new();
        ports.insert(port, value);
        Ok(NodeEnvelopes {
            ports,
            derived: Vec::new(),
        })
    }
}

/// check 节点测试命令缺省（task.json test_command 未声明时；fail-closed 面在
/// test_unit 自身——命令为空即 pass=false 观察）。
pub const DEFAULT_TEST_COMMAND: &str = "python -m pytest -q";

// ============================================================
// 图输入任务包
// ============================================================

/// 图输入工件（swe_task_input 面 + check 节点 test_command 声明位）。
#[derive(Debug, Clone)]
struct GraphTask {
    instance_id: String,
    #[allow(dead_code)]
    repo: String,
    #[allow(dead_code)]
    problem_statement: String,
    image: String,
    test_command: Option<String>,
}

impl GraphTask {
    fn parse(input: &Value) -> Result<Self, ExeclineError> {
        let str_field = |name: &str| {
            input
                .get(name)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| ExeclineError::UnitFailed {
                    detail: format!("图输入缺 {name} 字段（swe_task_input 面）"),
                })
        };
        Ok(Self {
            instance_id: str_field("instance_id")?,
            repo: str_field("repo")?,
            problem_statement: str_field("problem_statement")?,
            image: str_field("image")?,
            test_command: input
                .get("test_command")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }

    /// 容器 setup 材料（swev::setup 只消费 instance_id/image；判分面字段空置
    /// ——图执行器 v1 判定走 check 节点，不走冻结判分链）。
    fn as_swev_task(&self) -> swev::SwevTask {
        swev::SwevTask {
            instance_id: self.instance_id.clone(),
            repo: self.repo.clone(),
            problem_statement: self.problem_statement.clone(),
            image: self.image.clone(),
            eval_script: String::new(),
            hints_text: String::new(),
        }
    }
}

// ============================================================
// 线性化（v1 拓扑序）
// ============================================================

/// 线性化：Kahn 拓扑序（就绪集 BTree 最小 id 先出——确定序）。v1 拒：
/// loop/map 构造节点（登记后续）与环拓扑。形状前置守卫（对抗面
/// 2026-10-25）：空图 / 入口节点悬空 / 悬边端点——先于 Kahn 判别（悬边
/// 端点此前被「out.len() != ids.len()」误诊为环拓扑）。
pub fn linearize(plan: &ExecutablePlan) -> Result<Vec<String>, ExeclineError> {
    let ids: BTreeSet<&str> = plan
        .body
        .graph
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    if ids.is_empty() {
        return Err(ExeclineError::GraphShapeUnsupported {
            detail: "空图（graph.nodes 零节点）——无终态发射者，v1 拒驱动".into(),
        });
    }
    if !ids.contains(plan.body.graph.entry.as_str()) {
        return Err(ExeclineError::GraphShapeUnsupported {
            detail: format!(
                "入口节点 entry={} 不在节点集（悬空入口）",
                plan.body.graph.entry
            ),
        });
    }
    for edge in &plan.body.graph.edges {
        let edge_desc = format!(
            "{}.{} -> {}.{}",
            edge.from, edge.output, edge.to, edge.input
        );
        if !ids.contains(edge.from.as_str()) {
            return Err(ExeclineError::GraphShapeUnsupported {
                detail: format!(
                    "边 {edge_desc} 的端点 from={} 不在节点集（悬边）",
                    edge.from
                ),
            });
        }
        if !ids.contains(edge.to.as_str()) {
            return Err(ExeclineError::GraphShapeUnsupported {
                detail: format!("边 {edge_desc} 的端点 to={} 不在节点集（悬边）", edge.to),
            });
        }
    }
    for node in &plan.body.graph.nodes {
        if node.loop_spec.is_some() || node.map_spec.is_some() {
            return Err(ExeclineError::GraphShapeUnsupported {
                detail: format!(
                    "节点 {} 是 loop/map 构造节点（v1 线性链范围外，登记后续）",
                    node.id
                ),
            });
        }
    }
    let mut indegree: BTreeMap<&str, usize> = ids.iter().map(|id| (*id, 0)).collect();
    for edge in &plan.body.graph.edges {
        *indegree.entry(edge.to.as_str()).or_default() += 1;
    }
    let mut ready: BTreeSet<&str> = indegree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut out = Vec::with_capacity(ids.len());
    while let Some(id) = ready.pop_first() {
        out.push(id.to_string());
        for edge in &plan.body.graph.edges {
            if edge.from == id {
                let d = indegree
                    .get_mut(edge.to.as_str())
                    .expect("边端点必在节点集");
                *d -= 1;
                if *d == 0 {
                    ready.insert(edge.to.as_str());
                }
            }
        }
    }
    if out.len() != ids.len() {
        return Err(ExeclineError::GraphShapeUnsupported {
            detail: "图含环拓扑（v1 线性链范围外，登记后续）".into(),
        });
    }
    Ok(out)
}

// ============================================================
// 预算面
// ============================================================

/// plan 级预算条目提取 + 熔断门。
#[derive(Debug, Clone, Default)]
struct PlanBudgets {
    cost_cap: Option<u64>,
    token_cap: Option<u64>,
}

impl PlanBudgets {
    /// 提取 plan 级预算条目。**正典 id 门**（对抗面 2026-10-25）：plan 级
    /// cents/tokens_total 条目的 id 必须是 `cost_cap`/`token_cap`——执行面
    /// 只认这两个正典 id，改名条目此前被静默跳过（= 手改计划改名即**静默
    /// 免帽**）；非正典 id fail-loud 拒收（EX4005），不静默降级为无预算。
    fn of(plan: &ExecutablePlan) -> Result<Self, ExeclineError> {
        let mut b = Self::default();
        for entry in &plan.body.budget.budgets {
            if entry.scope != "plan" {
                continue;
            }
            match entry.dimension {
                BudgetDimension::Cents => {
                    if entry.id != "cost_cap" {
                        return Err(ExeclineError::PlanBindMismatch {
                            detail: format!(
                                "plan 级 cents 预算 id {} 非正典 cost_cap（执行面只认 \
                                 cost_cap——改名即静默免帽，拒收）",
                                entry.id
                            ),
                        });
                    }
                    b.cost_cap = Some(entry.cap);
                }
                BudgetDimension::TokensTotal => {
                    if entry.id != "token_cap" {
                        return Err(ExeclineError::PlanBindMismatch {
                            detail: format!(
                                "plan 级 tokens_total 预算 id {} 非正典 token_cap（执行面\
                                 只认 token_cap——改名即静默免帽，拒收）",
                                entry.id
                            ),
                        });
                    }
                    b.token_cap = Some(entry.cap);
                }
                _ => {}
            }
        }
        Ok(b)
    }

    /// 熔断门：节点起动前对账（llm.cents 口径 1 模型调用 = 1¢）。B 席嫁接
    /// （合流批）：EX4007 结构化字段（budget_id/dimension/spent/cap）——
    /// tokens_total 口径计 cache_read（account_node 面）。
    fn gate(&self, stage: &Drive) -> Result<(), ExeclineError> {
        if let Some(cap) = self.cost_cap {
            if stage.cents_used >= cap {
                return Err(ExeclineError::BudgetCapExceeded {
                    budget_id: "cost_cap".into(),
                    dimension: "cents".into(),
                    spent: stage.cents_used,
                    cap,
                });
            }
        }
        if let Some(cap) = self.token_cap {
            if stage.tokens_used >= cap {
                return Err(ExeclineError::BudgetCapExceeded {
                    budget_id: "token_cap".into(),
                    dimension: "tokens_total".into(),
                    spent: stage.tokens_used,
                    cap,
                });
            }
        }
        Ok(())
    }

    /// 单节点 launch 硬顶：plan cost_cap 在场时收敛到剩余额度。
    fn node_spend_cap(&self, default_cap: i64, cents_used: u64) -> i64 {
        match self.cost_cap {
            Some(cap) => (cap as i64 - cents_used as i64).max(1),
            None => default_cap,
        }
    }
}

// ============================================================
// 驱动账（envelope 交接 + 预算累账 + 工件面）
// ============================================================

/// 一次图驱动的交接账。
#[derive(Debug, Default)]
struct Drive {
    /// `(节点, 端口) → 载荷`。
    payloads: BTreeMap<(String, String), Value>,
    /// `(节点, 端口) → 工件文件`。
    envelope_files: BTreeMap<(String, String), PathBuf>,
    cents_used: u64,
    tokens_used: u64,
    model_calls: u64,
}

impl Drive {
    fn payload(&self, node: &str, port: &str) -> Option<Value> {
        self.payloads
            .get(&(node.to_string(), port.to_string()))
            .cloned()
    }

    /// 模型环报告累账（llm.cents 口径：1 次模型调用 = 1¢；token = in+out+
    /// cache_read——tokens_total 口径计 cache_read，B 席嫁接）。
    fn account_node(&mut self, report: &af_harness::RunReport) {
        self.model_calls += report.model_calls;
        self.cents_used += report.model_calls;
        self.tokens_used += report.tokens.input + report.tokens.output + report.tokens.cache_read;
    }

    /// envelope 结构校验（键集=端口名 + emits 类型正门）+ 落工件面 + 入账。
    /// 上游面（B 席嫁接，合流批）：artifact_contract 带 r#type 的端口逐件
    /// check_typed——无类型声明 = 不核（如实口径；旧计划缺省 None）。
    fn commit(
        &mut self,
        plan: &ExecutablePlan,
        node_id: &str,
        node: &af_compiler::plan::PlanNodeExec,
        envelopes: NodeEnvelopes,
        artifacts_dir: &Path,
    ) -> Result<(), ExeclineError> {
        let ports: BTreeSet<&str> = node
            .artifact_contract
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        let produced: BTreeSet<&str> = envelopes.ports.keys().map(String::as_str).collect();
        if ports != produced {
            return Err(ExeclineError::EnvelopeMismatch {
                detail: format!(
                    "节点 {node_id} envelope 端口不符：契约 {ports:?} ≠ 产出 {produced:?}"
                ),
            });
        }
        for artifact in node.artifact_contract.iter().filter(|a| a.r#type.is_some()) {
            let port = artifact.name.as_str();
            if let Some(payload) = envelopes.ports.get(port) {
                check_typed(
                    &plan.body.types,
                    node_id,
                    port,
                    artifact.r#type.as_deref().unwrap_or_default(),
                    payload,
                )?;
            }
        }
        for (port, payload) in envelopes.ports {
            let file = artifacts_dir.join(format!("{node_id}.{port}.json"));
            let envelope = if envelopes.derived.is_empty() {
                serde_json::json!({
                    "envelope": "v1",
                    "from": node_id,
                    "port": port,
                    "payload": payload,
                })
            } else {
                serde_json::json!({
                    "envelope": "v1",
                    "from": node_id,
                    "port": port,
                    "payload": payload,
                    // 披露：裸 declare 推导面（ground truth），不冒充模型声明。
                    "derived": envelopes.derived,
                })
            };
            std::fs::write(
                &file,
                serde_json::to_string(&envelope).map_err(|err| ExeclineError::UnitFailed {
                    detail: format!("envelope 序列化失败: {err}"),
                })?,
            )
            .map_err(|err| ExeclineError::UnitFailed {
                detail: format!("写 envelope 工件失败 {}: {err}", file.display()),
            })?;
            self.payloads
                .insert((node_id.to_string(), port.clone()), payload);
            self.envelope_files
                .insert((node_id.to_string(), port), file);
        }
        Ok(())
    }
}

// ============================================================
// 提示词装配 / 模型路由 / 工具面 / schema 校验
// ============================================================

/// 节点 envelope 产出（端口载荷 + 裸 declare 推导面披露）。
#[derive(Debug, Default)]
struct NodeEnvelopes {
    ports: BTreeMap<String, Value>,
    /// ground-truth 推导字段（裸 declare 路径；envelope 落盘披露，不冒充模型声明）。
    derived: Vec<String>,
}

/// 声明共享单元（declare 工具闭包 → 驱动面读取）。
type DeclarationCell = Arc<std::sync::Mutex<Option<Value>>>;

/// declare 工具（环组装面，`pkg.sweagent.aci.submit` 完成载体先例）：
/// 模型以工具调用交付 envelope 声明——参数 = 端口键控 JSON 对象，工具输出
/// 首行 = TASK_COMPLETE 即环完成信号。返回 (声明, 执行闭包, 共享单元)。
fn declare_tool(ports: &[&str]) -> (ToolDecl, ToolFn, DeclarationCell) {
    let cell: DeclarationCell = Arc::new(std::sync::Mutex::new(None));
    let sink = cell.clone();
    // A 席合流迁移：签名随主线 ToolFn 缝（&Value→Result）——参数对象原样
    // 入共享单元（harness 已保证对象形状，不再二次 parse）。
    let on_call: ToolFn = Arc::new(move |args: &Value| {
        *sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(args.clone());
        Ok(Value::String("TASK_COMPLETE".to_string()))
    });
    // harness 面不设 required（裸 declare 必须能到达闭包——四跑真模型判例：
    // 模型把 declare 当完成按钮按，参数重述被稳定拒绝；裸调语义=提交工作区，
    // 由驱动面按端口 schema 判可推导性）。端口键集校验在 commit 相（EX4006）。
    let decl = serde_json::from_value::<ToolDecl>(serde_json::json!({
        "name": "declare",
        "description": format!(
            "Deliver your finished artifact and end the run. Preferred: ONE argument — a JSON \
             object keyed by artifact port name ({ports:?}), each value that port's payload. \
             For patch-shaped ports you may call it bare ({{}}): the workspace diff and touched \
             files are then captured from the container automatically. This is the only \
             completion path."
        ),
        "parameters": {
            "type": "object",
        },
        "effect": "pure",
    }))
    .expect("declare ToolDecl 静态构造必须合法（af-harness spec.rs 契约）");
    (decl, on_call, cell)
}

/// 裸 declare 的可推导判定：端口 schema 的 required 字段 ⊆ {diff(string),
/// files(array<string>)}——diff ← 容器工作区取证（swev::capture_diff 先例），
/// files ← diff 解析（`diff --git a/x b/y` 路径集）。两源皆 ground truth，
/// 与模型声明互不混用（envelope 记 derived 面，披露不冒充）。
fn derivable_port_fields(schema: &Value) -> bool {
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if required.is_empty() {
        return false;
    }
    let ok_shape = |name: &str| match name {
        "diff" => {
            schema
                .get("properties")
                .and_then(|p| p.get("diff"))
                .and_then(|d| d.get("type"))
                .and_then(Value::as_str)
                == Some("string")
        }
        "files" => {
            schema
                .get("properties")
                .and_then(|p| p.get("files"))
                .and_then(|d| d.get("type"))
                .and_then(Value::as_str)
                == Some("array")
        }
        _ => false,
    };
    required.iter().all(|f| ok_shape(f))
}

/// 从 diff 文本解析触碰文件集（`diff --git a/<path> b/<path>` 行）。
fn files_of_diff(diff: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git a/") {
            if let Some((path, _)) = rest.split_once(" b/") {
                let path = path.trim();
                if !path.is_empty() && !files.iter().any(|f: &String| f == path) {
                    files.push(path.to_string());
                }
            }
        }
    }
    files
}

/// 声明轮消息解析：整体 JSON；容忍首尾 markdown 围栏（```json … ```）——
/// 围栏内仍是同一声明载荷，非替代源。
fn parse_declaration(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    let body = if trimmed.starts_with("```") {
        let inner = trimmed
            .strip_prefix("```")
            .and_then(|r| r.split_once('\n'))
            .map(|(_, body)| body)
            .unwrap_or(trimmed);
        inner.trim().strip_suffix("```").unwrap_or(inner).trim()
    } else {
        trimmed
    };
    serde_json::from_str(body).map_err(|err| err.to_string())
}

/// 终态 assistant 文本：transcript 自后向前首条**非空** assistant 消息。
/// （完成信号语义 = 工具输出首行命中——完成轮的 assistant 消息可能只带
/// 工具调用、text 为空；envelope 声明在其前一轮。）
fn final_assistant_text(report: &af_harness::RunReport) -> Option<String> {
    report.transcript.iter().rev().find_map(|item| match item {
        af_harness::TranscriptItem::Assistant { text, .. } if !text.trim().is_empty() => {
            Some(text.clone())
        }
        _ => None,
    })
}

/// plan JSON 通用遍历收集 template_ref（装订器④装载清单）。
fn collect_template_refs(plan_src: &str) -> Vec<String> {
    let mut refs = BTreeSet::new();
    if let Ok(Value::Object(plan)) = serde_json::from_str::<Value>(plan_src) {
        if let Some(nodes) = plan
            .get("body")
            .and_then(|b| b.get("node_exec"))
            .and_then(Value::as_array)
        {
            for node in nodes {
                if let Some(sections) = node.get("context").and_then(Value::as_array) {
                    for section in sections {
                        if let Some(r) = section.get("template_ref").and_then(Value::as_str) {
                            refs.insert(r.to_string());
                        }
                    }
                }
            }
        }
    }
    refs.into_iter().collect()
}

/// quota_tokens 的粗字符帽（1 token ≈ 4 chars，v1 粗口径——精确分词器不在面）。
/// **饱和乘**（对抗面 2026-10-25）：u64::MAX × 4 在 debug 构建溢出 panic——
/// 合法大配额（quota_tokens 无上界约束）不得炸环；饱和后帽巨大 = 不截断，
/// 语义与「配额大于全文」一致。
fn quota_chars(quota_tokens: u64) -> usize {
    quota_tokens.max(1).saturating_mul(4) as usize
}

fn truncate_chars(text: &str, cap: usize) -> String {
    if text.chars().count() <= cap {
        text.to_string()
    } else {
        let cut: String = text.chars().take(cap).collect();
        format!("{cut}\n…[truncated at {cap} chars]")
    }
}

/// 节点提示词装配：system = template 段正文（无模板段回退 swev 环 system）；
/// user = input 字段段 + edge 端口段（上游 envelope pretty JSON，quota 截断）。
fn compose_prompts(
    bound: &PlanBind,
    sections: &[SectionSpec],
    input: &Value,
    plan: &ExecutablePlan,
    stage: &Drive,
    node_id: &str,
) -> Result<(String, String), String> {
    let mut system_parts = Vec::new();
    let mut user_parts = Vec::new();
    for section in sections {
        match section.from.as_str() {
            "template" => {
                let r = section
                    .template_ref
                    .as_deref()
                    .ok_or("template 段缺 template_ref")?;
                let asset = bound
                    .templates
                    .get(r)
                    .ok_or_else(|| format!("模板 {r} 未在装订清单（内部不一致）"))?;
                system_parts.push(truncate_chars(
                    &asset.content,
                    quota_chars(section.quota_tokens),
                ));
            }
            "input" => {
                let mut lines = Vec::new();
                for field in &section.fields {
                    let value = input.get(field.as_str()).cloned().unwrap_or(Value::Null);
                    lines.push(format!("{field}: {value}"));
                }
                let text = lines.join("\n");
                user_parts.push(truncate_chars(&text, quota_chars(section.quota_tokens)));
            }
            "edge" => {
                let port = section.port.as_deref().ok_or("edge 段缺 port")?;
                // 上游 envelope：按边表反查 (from, output) → 本节点 input 端口。
                let edge = plan
                    .body
                    .graph
                    .edges
                    .iter()
                    .find(|e| e.to == node_id && e.input == port)
                    .ok_or_else(|| format!("edge 段端口 {port} 无入边（声明面与拓扑脱节）"))?;
                let payload = stage.payload(&edge.from, &edge.output).ok_or_else(|| {
                    format!(
                        "上游 {}.{} 尚无 envelope（拓扑序违例或产者缺席）",
                        edge.from, edge.output
                    )
                })?;
                let text = serde_json::to_string_pretty(&payload).unwrap_or_default();
                user_parts.push(truncate_chars(&text, quota_chars(section.quota_tokens)));
            }
            other => return Err(format!("context 段 from={other} 无 v1 装配面")),
        }
    }
    let system = if system_parts.is_empty() {
        swev::SWEV_SYSTEM_PROMPT.to_string()
    } else {
        system_parts.join("\n\n")
    };
    Ok((system, user_parts.join("\n\n")))
}

/// 注册表 model_route 引用 → af-harness 端点（字段级转换 + swev 环采样覆写）。
fn resolve_route_endpoint(
    registry: &ComponentRegistry,
    route_ref: &str,
    _base_profile: &str,
) -> Result<af_harness::EndpointSpec, String> {
    let (name, version) = route_ref.split_once('@').unwrap_or((route_ref, ""));
    let versions = registry
        .versions_of(name)
        .ok_or_else(|| format!("model_route {name} 不在注册表"))?;
    let decl = if version.is_empty() {
        versions
            .iter()
            .next()
            .ok_or_else(|| format!("model_route {name} 无版本声明"))?
            .1
    } else {
        versions
            .get(version)
            .ok_or_else(|| format!("model_route {route_ref} 版本不在注册表"))?
    };
    let src = decl
        .endpoint
        .as_ref()
        .ok_or_else(|| format!("model_route {route_ref} 声明缺 endpoint"))?;
    let mut endpoint = af_harness::EndpointSpec {
        protocol: match src.protocol {
            af_compiler::components::Protocol::OpenaiChatCompletions => {
                af_harness::Protocol::OpenaiChatCompletions
            }
            af_compiler::components::Protocol::AnthropicMessages => {
                af_harness::Protocol::AnthropicMessages
            }
        },
        base_url: src.base_url.clone(),
        model: src.model.clone(),
        key_ref: src.key_ref.clone(),
        reasoning_tier: match src.reasoning_tier {
            af_compiler::components::ReasoningTier::Off => af_harness::ReasoningTier::Off,
            af_compiler::components::ReasoningTier::Low => af_harness::ReasoningTier::Low,
            af_compiler::components::ReasoningTier::Medium => af_harness::ReasoningTier::Medium,
            af_compiler::components::ReasoningTier::High => af_harness::ReasoningTier::High,
        },
        stream: src.stream,
        auth: src.auth.map(|a| match a {
            af_compiler::components::AuthStyle::Bearer => af_harness::AuthStyle::Bearer,
            af_compiler::components::AuthStyle::XApiKey => af_harness::AuthStyle::XApiKey,
        }),
        max_tokens: src.max_tokens,
        temperature_milli: src.temperature_milli,
        top_p_milli: src.top_p_milli,
    };
    // swev 环采样覆写（fix2 口径：温度 0/top_p 1/max_tokens 32000 显式面）。
    endpoint.temperature_milli = Some(0);
    endpoint.top_p_milli = Some(1_000);
    endpoint.max_tokens = Some(32_000);
    Ok(endpoint)
}

/// 节点工具执行闭包（15 件 builtin 表内已接线的只读/编辑/bash 面）。
/// A 席合流迁移：签名随主线 ToolFn 缝——参数对象直入（不再 parse 文本），
/// 观察文本以 Ok(Value::String) 回给模型，Err = 工具失败文案。
fn node_tool_fn(
    reference: &str,
    bridge: Arc<dyn HostBridge + Send + Sync>,
    env: Arc<TbEnv>,
) -> Option<ToolFn> {
    match sweng::name_of(reference) {
        "pkg.sweagent.env" => {
            let bash = Arc::new(sweng::EnvBash::new(bridge, env));
            Some(Arc::new(move |args: &Value| {
                Ok(Value::String(
                    bash.call(
                        args.get("command").and_then(Value::as_str).unwrap_or(""),
                        args.get("timeout_sec")
                            .and_then(Value::as_u64)
                            .map(|t| t as u32),
                    ),
                ))
            }))
        }
        "pkg.sweng.locate" | "pkg.sweagent.aci.search" => Some(Arc::new(move |args: &Value| {
            let s = |k: &str| args.get(k).and_then(Value::as_str).map(str::to_string);
            let n = |k: &str| args.get(k).and_then(Value::as_u64).map(|x| x as u32);
            Ok(Value::String(sweng::aci_search(
                bridge.as_ref(),
                &env,
                s("query").or(s("pattern")).as_deref().unwrap_or(""),
                s("path").as_deref(),
                n("max_hits"),
            )))
        })),
        "pkg.sweng.view" | "pkg.sweagent.aci.open" => Some(Arc::new(move |args: &Value| {
            let s = |k: &str| args.get(k).and_then(Value::as_str).map(str::to_string);
            let n = |k: &str| args.get(k).and_then(Value::as_u64).map(|x| x as u32);
            Ok(Value::String(sweng::aci_open(
                bridge.as_ref(),
                &env,
                s("path").as_deref().unwrap_or(""),
                n("start_line").or(n("start")),
                n("end_line").or(n("end")),
            )))
        })),
        "pkg.sweng.edit" | "pkg.sweagent.aci.edit" => Some(Arc::new(move |args: &Value| {
            let s = |k: &str| args.get(k).and_then(Value::as_str).map(str::to_string);
            Ok(Value::String(sweng::aci_edit(
                bridge.as_ref(),
                &env,
                s("path").as_deref().unwrap_or(""),
                s("old_text").as_deref().unwrap_or(""),
                s("new_text").as_deref().unwrap_or(""),
            )))
        })),
        _ => None,
    }
}

/// 终态工件端口（interface.output_from 的 is_final 端口；缺 → 线性链缺终态）。
fn final_artifact_port(plan: &ExecutablePlan) -> Result<String, ExeclineError> {
    plan.body
        .node_exec
        .iter()
        .find(|n| n.node_id == plan.body.interface.output_from)
        .and_then(|n| {
            n.artifact_contract
                .iter()
                .find(|a| a.is_final)
                .map(|a| a.name.clone())
        })
        .ok_or_else(|| ExeclineError::EnvelopeMismatch {
            detail: format!(
                "终态发射者 {} 无 is_final 工件端口",
                plan.body.interface.output_from
            ),
        })
}

/// JSON Schema 子集校验（sop 图 types 词典实际用到的面：type/required/
/// properties/items；不足即报——不静默放行未知关键字之外的形状）。
/// 形状硬门（对抗面 2026-10-25）：schema 非对象（如串/数）此前被
/// `Value::get` 全 None **静默当通过**——现 fail-loud 报错；`type` 段在场
/// 但非字符串（如 `{"type":123}`）同理。
fn validate_schema(value: &Value, schema: &Value) -> Result<(), String> {
    let Some(schema_obj) = schema.as_object() else {
        return Err(format!("schema 不是对象：{schema}"));
    };
    if let Some(type_value) = schema_obj.get("type") {
        let Some(expected) = type_value.as_str() else {
            return Err(format!("schema type 段非字符串：{type_value}"));
        };
        let ok = match (expected, value) {
            ("object", Value::Object(_)) => true,
            ("array", Value::Array(_)) => true,
            ("string", Value::String(_)) => true,
            ("boolean", Value::Bool(_)) => true,
            ("integer", Value::Number(n)) => n.is_i64() || n.is_u64(),
            ("number", Value::Number(_)) => true,
            _ => false,
        };
        if !ok {
            return Err(format!("type 应为 {expected}，实为 {value}"));
        }
    }
    if let (Value::Object(map), Some(required)) = (value, schema_obj.get("required")) {
        if let Some(required) = required.as_array() {
            for key in required {
                if let Some(key) = key.as_str() {
                    if !map.contains_key(key) {
                        return Err(format!("缺 required 键 {key}"));
                    }
                }
            }
        }
    }
    if let (Value::Object(map), Some(properties)) = (
        value,
        schema_obj.get("properties").and_then(Value::as_object),
    ) {
        for (key, sub) in properties {
            if let Some(item) = map.get(key) {
                validate_schema(item, sub)?;
            }
        }
    }
    if let (Value::Array(items), Some(item_schema)) =
        (value, schema_obj.get("items").filter(|s| s.is_object()))
    {
        for item in items {
            validate_schema(item, item_schema)?;
        }
    }
    Ok(())
}

// ============================================================
// typed envelope 校验（B 席整件嫁接，合流批）：逐边/逐端口类型正门
// ============================================================

/// 最小 JSON Schema 校验器（type/required/properties/items；`type` 段的
/// 非原始关键字串 = **命名类型引用**——对 plan.types 递归解析，深度帽 16
/// 防自引用环）。其余 schema 关键字忽略（如实口径，登记 RESULT 边界）。
struct SchemaValidator<'a> {
    types: &'a BTreeMap<String, Value>,
}

const PRIMITIVES: [&str; 6] = ["object", "array", "string", "boolean", "integer", "number"];

impl SchemaValidator<'_> {
    fn validate(&self, value: &Value, schema: &Value, path: &str, errors: &mut Vec<String>) {
        self.walk(value, schema, path, errors, 0);
    }

    fn walk(
        &self,
        value: &Value,
        schema: &Value,
        path: &str,
        errors: &mut Vec<String>,
        depth: usize,
    ) {
        if depth > 16 {
            errors.push(format!("{path}: 类型引用链超过 16 层（疑似自引用）"));
            return;
        }
        let Some(obj) = schema.as_object() else {
            errors.push(format!("{path}: schema 不是对象"));
            return;
        };
        // type 段硬门（对抗面 2026-10-25）：在场但非字符串（如 123）此前被
        // and_then(as_str) 静默跳过——现报错（EX4006 面）。
        match obj.get("type") {
            None => {}
            Some(Value::String(ty)) => {
                if PRIMITIVES.contains(&ty.as_str()) {
                    let ok = match ty.as_str() {
                        "object" => value.is_object(),
                        "array" => value.is_array(),
                        "string" => value.is_string(),
                        "boolean" => value.is_boolean(),
                        "integer" => value.is_i64() || value.is_u64(),
                        _ => value.is_number(),
                    };
                    if !ok {
                        errors.push(format!("{path}: 应为 {ty}，实为 {}", kind_of(value)));
                        return;
                    }
                } else if let Some(named) = self.types.get(ty.as_str()) {
                    // 命名类型引用：对正本 schema 递归（引用 schema 自身无其他
                    // 约束关键字时即纯转发；有则合并语义——type 段替换为正本，
                    // 其余关键字覆写）。
                    let merged = if obj.len() == 1 {
                        named.clone()
                    } else {
                        let mut m = obj.clone();
                        m.remove("type");
                        let mut base = named.as_object().cloned().unwrap_or_default();
                        for (k, v) in m {
                            base.insert(k, v);
                        }
                        Value::Object(base)
                    };
                    self.walk(value, &merged, path, errors, depth + 1);
                    return;
                } else {
                    errors.push(format!("{path}: 不支持的 schema type {ty}"));
                    return;
                }
            }
            Some(other) => {
                errors.push(format!("{path}: schema type 段非字符串：{other}"));
                return;
            }
        }
        if let Some(required) = obj.get("required").and_then(Value::as_array) {
            if let Some(map) = value.as_object() {
                for name in required.iter().filter_map(Value::as_str) {
                    if !map.contains_key(name) {
                        errors.push(format!("{path}: 缺 required 字段 {name}"));
                    }
                }
            }
        }
        if let (Some(props), Some(map)) = (
            obj.get("properties").and_then(Value::as_object),
            value.as_object(),
        ) {
            for (name, sub) in props {
                if let Some(v) = map.get(name) {
                    self.walk(v, sub, &format!("{path}.{name}"), errors, depth + 1);
                }
            }
        }
        if let (Some(items), Some(arr)) = (obj.get("items"), value.as_array()) {
            for (i, v) in arr.iter().enumerate() {
                self.walk(v, items, &format!("{path}[{i}]"), errors, depth + 1);
            }
        }
    }
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// 命名类型对 plan.types 解析后的整值校验（EX4006 点名节点/端口/类型）。
fn envelope_check(
    types: &BTreeMap<String, Value>,
    node: &str,
    port: &str,
    value: &Value,
    schema: &Value,
) -> Result<(), ExeclineError> {
    let mut errors = Vec::new();
    SchemaValidator { types }.validate(value, schema, "$", &mut errors);
    if errors.is_empty() {
        return Ok(());
    }
    let type_expr = types
        .iter()
        .find(|(_, s)| *s == schema)
        .map(|(name, _)| name.clone())
        .unwrap_or_default();
    Err(GraphExecuteUnit::envelope_err(format!(
        "节点 {node} 端口 {port} 不符类型 {type_expr}：{}",
        errors.join("；")
    )))
}

/// 类型表达式核对（envelope 正门）：list<T> 拆包逐项、命名类型对 plan.types
/// 解析；值不符 = EX4006 点名节点/端口/类型。types 词典单参（plan.body.types
/// 的投影）——校验器可独立单测。
fn check_typed(
    types: &BTreeMap<String, Value>,
    node_id: &str,
    port: &str,
    type_expr: &str,
    value: &Value,
) -> Result<(), ExeclineError> {
    let (inner, is_list) = match type_expr
        .strip_prefix("list<")
        .and_then(|t| t.strip_suffix('>'))
    {
        Some(inner) => (inner, true),
        None => (type_expr, false),
    };
    let Some(schema) = types.get(inner) else {
        return Err(GraphExecuteUnit::envelope_err(format!(
            "节点 {node_id} 端口 {port}：类型表达式 {type_expr} 的基名 {inner} 不在 plan.types"
        )));
    };
    if !is_list {
        return envelope_check(types, node_id, port, value, schema);
    }
    let Some(items) = value.as_array() else {
        return Err(GraphExecuteUnit::envelope_err(format!(
            "节点 {node_id} 端口 {port}：{type_expr} 应为 array，实为 {}",
            kind_of(value)
        )));
    };
    for (i, item) in items.iter().enumerate() {
        let mut errors = Vec::new();
        SchemaValidator { types }.validate(item, schema, &format!("$[{i}]"), &mut errors);
        if !errors.is_empty() {
            return Err(GraphExecuteUnit::envelope_err(format!(
                "节点 {node_id} 端口 {port} 不符类型 {type_expr}：{}",
                errors.join("；")
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// B 席嫁接（合流批）——SchemaValidator 命名类型递归：required/类型/
    /// 嵌套 properties/items/**命名类型引用**（task_plan.steps → plan_step）。
    #[test]
    fn schema_validator_resolves_named_types_recursively() {
        let types: BTreeMap<String, Value> = [
            (
                "plan_step".to_string(),
                serde_json::json!({"type": "object", "required": ["id"],
                       "properties": {"id": {"type": "string"}}}),
            ),
            (
                "task_plan".to_string(),
                serde_json::json!({"type": "object", "required": ["instance_id", "steps"],
                       "properties": {"instance_id": {"type": "string"},
                                      "steps": {"type": "array", "items": {"type": "plan_step"}}}}),
            ),
        ]
        .into_iter()
        .collect();
        let validator = SchemaValidator { types: &types };
        let schema = &types["task_plan"];
        let ok = serde_json::json!({"instance_id": "i", "steps": [{"id": "s1"}]});
        let mut errors = Vec::new();
        validator.validate(&ok, schema, "$", &mut errors);
        assert!(errors.is_empty(), "{errors:?}");
        // 缺 steps / 步进项缺 id（命名引用递归面）。
        let bad = serde_json::json!({"instance_id": "i"});
        let mut errors = Vec::new();
        validator.validate(&bad, schema, "$", &mut errors);
        assert!(errors.iter().any(|e| e.contains("steps")), "{errors:?}");
        let bad = serde_json::json!({"instance_id": "i", "steps": [{"action": "x"}]});
        let mut errors = Vec::new();
        validator.validate(&bad, schema, "$", &mut errors);
        assert!(errors.iter().any(|e| e.contains("id")), "{errors:?}");
    }

    /// B 席嫁接——深度帽 16：自引用命名类型（type 段指回自身）不无限递归，
    /// 以「超过 16 层」诊断拒收。
    #[test]
    fn schema_validator_depth_caps_named_reference_chains() {
        let types: BTreeMap<String, Value> = [(
            "recurse".to_string(),
            serde_json::json!({"type": "recurse"}),
        )]
        .into_iter()
        .collect();
        let validator = SchemaValidator { types: &types };
        let mut errors = Vec::new();
        validator.validate(
            &serde_json::json!({"anything": 1}),
            &serde_json::json!({"type": "recurse"}),
            "$",
            &mut errors,
        );
        assert!(
            errors.iter().any(|e| e.contains("16 层")),
            "深度帽诊断：{errors:?}"
        );
    }

    /// B 席嫁接——check_typed 的 list<T> 拆包：数组逐项按元素类型核；
    /// 非数组值与基名缺席各按口径拒。
    #[test]
    fn check_typed_unpacks_list_element_type() {
        let types: BTreeMap<String, Value> = [(
            "code_file".to_string(),
            serde_json::json!({"type": "object", "required": ["path", "content_ref"],
                   "properties": {"path": {"type": "string"},
                                  "content_ref": {"type": "string"}}}),
        )]
        .into_iter()
        .collect();
        let ok = serde_json::json!([{"path": "a.py", "content_ref": "ws#1"}]);
        assert!(check_typed(&types, "coder", "files", "list<code_file>", &ok).is_ok());
        // 元素缺 required 字段：EX4006 点名节点/端口/类型。
        let bad_item = serde_json::json!([{"path": "a.py"}]);
        let err = check_typed(&types, "coder", "files", "list<code_file>", &bad_item).unwrap_err();
        assert_eq!(
            err.code(),
            crate::error::ErrorCode::EnvelopeMismatch,
            "{err}"
        );
        for token in ["coder", "files", "list<code_file>", "content_ref"] {
            assert!(err.to_string().contains(token), "点名 {token}: {err}");
        }
        // 非数组值。
        let err = check_typed(
            &types,
            "coder",
            "files",
            "list<code_file>",
            &serde_json::json!({"path": "a.py"}),
        )
        .unwrap_err();
        assert!(err.to_string().contains("应为 array"), "{err}");
        // 基名不在词典。
        let err = check_typed(
            &types,
            "coder",
            "files",
            "list<ghost>",
            &serde_json::json!([]),
        )
        .unwrap_err();
        assert!(err.to_string().contains("ghost"), "{err}");
    }

    /// schema 子集校验：正例过 / 缺 required 键拒 / 类型不符拒 / items 递归拒。
    #[test]
    fn validate_schema_subset_semantics() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["pass", "failures"],
            "properties": {
                "pass": {"type": "boolean"},
                "failures": {"type": "array", "items": {"type": "string"}}
            }
        });
        assert!(validate_schema(
            &serde_json::json!({"pass": true, "failures": ["x"]}),
            &schema
        )
        .is_ok());
        assert!(validate_schema(&serde_json::json!({"pass": true}), &schema).is_err());
        assert!(
            validate_schema(&serde_json::json!({"pass": "yes", "failures": []}), &schema).is_err()
        );
        assert!(
            validate_schema(&serde_json::json!({"pass": true, "failures": [3]}), &schema).is_err(),
            "items 递归校验"
        );
    }

    /// quota 截断：超帽截断带尾注，未超原样。
    #[test]
    fn quota_truncation_marks_and_preserves() {
        assert_eq!(truncate_chars("abc", 5), "abc");
        let long = "x".repeat(100);
        let cut = truncate_chars(&long, 10);
        assert!(cut.starts_with("xxxxxxxxxx\n…[truncated at 10 chars]"));
        assert_eq!(
            cut.chars().count(),
            10 + 1 + "\n…[truncated at 10 chars]".chars().count() - 1
        );
    }
}
