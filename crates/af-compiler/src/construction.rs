//! E1 缺口④：协议教学内置——编译器生成的任务书自动内嵌 declare/envelope
//! 协议教学段（锚 `af-execline/src/graph_unit.rs:24-32` 的 typed envelope
//! 交接语义 + declare 契约面）。
//!
//! 背景（E1-报告缺口④）：SWE 任务靠包内专用模板（swev_system/
//! swev_instance_protocol）教会 declare/TASK_COMPLETE 协议；通用建设任务
//! 没有专用模板——E2G5 实证 40 轮帽内 agent 未交付 envelope（EX4006 违例），
//! E2G6 到 42 轮才摸出 declare。本模块把协议教学内置进编译链：任务书
//! （task_book，喂给 build 节点的输入字段）由编译器生成时自动携带
//! [`PROTOCOL_TEACHING`] 教学段 + 本任务的确切输出契约（端口名 + 物化
//! schema 自编译产物 types 词典提取）——通用任务不靠专用模板也会 declare。
//!
//! 与运行时装配段的关系：图执行器 drive_task_node 仍在环组装面注入
//! [OUTPUT CONTRACT]/[artifact protocol]（端口键集 + declare 工具定义）——
//! 本教学段是编译期生成的任务书面材料，两段同源同契约（envelope 端口
//! 键控对象），不冲突、可叠加。

use serde_json::Value;
use std::path::Path;
use std::process::ExitCode;

use crate::components::ComponentRegistry;
use crate::package::{write_package_manifest, TaskPackageManifest};
use crate::{bind, generate_plan, lower, ExecutablePlan, PlanError};

/// declare/envelope 协议教学段（E1 缺口④；英文——与运行时契约段/包模板
/// 同语言面，模型协议信号在英文面最稳）。锚 graph_unit.rs:24-32 语义：
/// envelope 按端口名键控、逐端口过声明 schema、违例拒收给反馈。
pub const PROTOCOL_TEACHING: &str = "\
[ARTIFACT PROTOCOL — read this first; it is how the task is finished]
1. Your work is delivered by calling the `declare` tool — never by a prose \
summary. The `declare` argument is ONE JSON object keyed by artifact port name.
2. Calling `declare` ends the run: its tool output is the completion signal. \
After a successful declare you cannot continue or revise, so verify your work \
BEFORE declaring.
3. The declared payload is a typed envelope: each port value is checked \
against the declared schema for that port (see the output contract below). \
Malformed declarations are rejected with feedback — read the feedback, fix \
the payload, and call `declare` again. You have unlimited retries until the \
run ends by declare or by the turn budget.
4. Only what you declare counts as the deliverable. The envelope is handed to \
the acceptance checks exactly as typed: missing ports, extra ports, or \
wrong-typed values are all rejections, not warnings.
5. Do the actual work with bash tool calls in the workspace, then declare \
the final artifact.";

/// 建设任务通用图源（E1 缺口①锚点形的编译期内置版：build 单节点 +
/// deliverable 类型 + budgets/stops/failures——与
/// tests/fixtures/construction_build.yaml 同形，为编译链自足不读外部样例）。
pub const CONSTRUCTION_GRAPH_YAML: &str = r#"version: 1
meta:
  name: factory_construction_build
  graph_version: "1.0"
  entry: build
  input: construction_task_input
  output: { ref: deliverable, from: build }
  intent: "工厂建设任务通用形（E1 缺口①④编译期内置）：build 单节点直出 deliverable；图输入通用字段（task_id/任务书文本），协议教学内置于编译器生成的任务书"

types:
  - name: construction_task_input
    schema:
      type: object
      required: [task_id, task_book]
      properties:
        task_id: { type: string }
        task_book: { type: string }
        image: { type: string }
        test_command: { type: string }
  - name: deliverable
    schema:
      type: object
      required: [files, tests_passed]
      properties:
        files: { type: array, items: { type: string } }
        tests_passed: { type: boolean }

nodes:
  - id: build
    kind: task
    uses:
      model: route.main@1
      tools: [pkg.sweagent.env@1]
    emits: { deliverable: deliverable }
    context:
      sections:
        - { from: input, fields: [task_id, task_book], policy: project, quota_tokens: 2400 }

policies:
  budgets:
    - { id: cost_cap, dimension: cents, scope: plan, cap: 300 }
    - { id: turn_cap, dimension: turns, scope: node:build, cap: 60 }
  stop:
    - { when: "last_tool.first_line in ['TASK_COMPLETE']", outcome: completed }
    - { when: "steps.tool_free >= 3", outcome: failed }
  failures:
    - class: transport_transient
      retry: { max: 3, backoff_ms: 800 }
    - class: environment_failure
      terminal: true
      outcome: failed
    - class: budget_exhausted
      terminal: true
      outcome: failed
"#;

/// 建设任务规格（调用方面：非 SWE 语义，零 SWE 字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstructionSpec {
    /// 任务号（容器命名回退同款语义）。
    pub task_id: String,
    /// 任务陈述（做什么）。
    pub statement: String,
    /// 验收判据（怎么算完成——进任务书的 acceptance 段）。
    pub acceptance: String,
    /// 镜像覆写（缺省 = 执行器补 DEFAULT_CONSTRUCTION_IMAGE）。
    pub image: Option<String>,
    /// check 节点判据命令（本通用形单 build 节点无 check 节点，此字段进
    /// task.json 供后续带 check 的形使用）。
    pub test_command: Option<String>,
    /// 组件注册表根（编译绑定面 + 任务包 registry_root 声明——D-48：缺省
    /// 常量是本机死路径，必须由调用方给出真包根）。
    pub registry_root: std::path::PathBuf,
}

/// 编译期内置建设图（四段全链：装载 → 校验 → lower → bind → plan）。
/// 产物 = 可提交 plan 形态（缺口①）。
pub fn compile_construction_plan(registry_root: &Path) -> Result<ExecutablePlan, PlanError> {
    let doc = af_dsl::load_str(CONSTRUCTION_GRAPH_YAML)
        .map_err(|e| PlanError::Lower(format!("建设图源装载失败：{e}")))?;
    let report = af_dsl::validate(&doc);
    if !report.is_ok() {
        return Err(PlanError::Lower(format!("建设图源校验未过：{report}")));
    }
    let ir = lower(&doc).map_err(|e| PlanError::Lower(e.to_string()))?;
    let registry = ComponentRegistry::load(registry_root)
        .map_err(|e| PlanError::Lower(format!("组件注册表装载失败：{e:#}")))?;
    let report = bind(&ir, &registry);
    generate_plan(&ir, &report)
}

/// 从编译产物 types 词典提取输出契约的物化 schema（教学段的「确切端口
/// + 形状」面）。
///
/// 终态端口 = interface.output_from 节点的 is_final 端口；本通用形恒为
/// deliverable 单端口——提取保持对图的忠实（不硬编码端口名），提取失败
/// 即 fail-loud。
pub fn output_contract_block(plan: &ExecutablePlan) -> Result<String, PlanError> {
    let body = &plan.body;
    let node = body
        .node_exec
        .iter()
        .find(|n| n.node_id == body.interface.output_from)
        .ok_or_else(|| {
            PlanError::Serialize(format!(
                "输出节点 {} 无 node_exec 投影（内部不一致）",
                body.interface.output_from
            ))
        })?;
    let final_ports: Vec<&str> = node
        .artifact_contract
        .iter()
        .filter(|a| a.is_final)
        .map(|a| a.name.as_str())
        .collect();
    if final_ports.is_empty() {
        return Err(PlanError::Serialize(
            "输出节点无 is_final 端口（无交付面）".into(),
        ));
    }
    let mut lines = vec!["[OUTPUT CONTRACT — the exact payload to declare]".to_string()];
    for port in final_ports {
        let expr = node
            .artifact_contract
            .iter()
            .find(|a| a.name == port)
            .and_then(|a| a.r#type.clone())
            .unwrap_or_else(|| body.interface.output_type.clone());
        let schema = body
            .types
            .get(&expr)
            .ok_or_else(|| PlanError::Serialize(format!("类型 {expr} 不在 types 词典")))?;
        lines.push(format!(
            "declare with key {port:?} (type {expr}), value schema:"
        ));
        lines.push(
            serde_json::to_string_pretty(schema)
                .map_err(|e| PlanError::Serialize(format!("schema 序列化失败：{e}")))?,
        );
    }
    Ok(lines.join("\n"))
}

/// 编译器生成的任务书（E1 缺口④落点）：协议教学段 + 本任务的输出契约 +
/// 任务陈述 + 验收判据——通用任务不靠专用模板也会 declare。
pub fn task_book(spec: &ConstructionSpec, plan: &ExecutablePlan) -> Result<String, PlanError> {
    let contract = output_contract_block(plan)?;
    Ok(format!(
        "{PROTOCOL_TEACHING}\n\n{contract}\n\n[task]\n{}\n\n[acceptance]\n{}",
        spec.statement, spec.acceptance
    ))
}

/// 任务包 task.json（通用字段面——缺口③编译面：零 SWE 字段；registry_root
/// 声明随包，D-48 不靠缺省死路径）。
pub fn task_json(spec: &ConstructionSpec, plan: &ExecutablePlan) -> Result<Value, PlanError> {
    let mut obj = serde_json::json!({
        "task_id": spec.task_id,
        "task_book": task_book(spec, plan)?,
        "registry_root": spec.registry_root,
    });
    if let Some(image) = &spec.image {
        obj["image"] = Value::String(image.clone());
    }
    if let Some(cmd) = &spec.test_command {
        obj["test_command"] = Value::String(cmd.clone());
    }
    Ok(obj)
}

/// 组包一体入口（编译→提交一跳的编译侧半链）：四段全链编译 → plan.json
/// 落盘（提交形态）→ task.json 落盘（任务书内嵌协议教学）→
/// package_manifest.json 写内容 hash 留档（缺口②，改动须清态换号重提）。
/// 返回 manifest 留证。
pub fn assemble_construction_package(
    task_dir: &Path,
    spec: &ConstructionSpec,
) -> Result<TaskPackageManifest, PlanError> {
    std::fs::create_dir_all(task_dir)
        .map_err(|e| PlanError::Serialize(format!("建任务包目录失败：{e}")))?;
    let plan = compile_construction_plan(&spec.registry_root)?;
    let plan_json =
        serde_json::to_string_pretty(&plan).map_err(|e| PlanError::Serialize(e.to_string()))?;
    std::fs::write(task_dir.join("plan.json"), format!("{plan_json}\n"))
        .map_err(|e| PlanError::Serialize(format!("写 plan.json 失败：{e}")))?;
    let task = task_json(spec, &plan)?;
    std::fs::write(
        task_dir.join("task.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&task).map_err(|e| PlanError::Serialize(e.to_string()))?
        ),
    )
    .map_err(|e| PlanError::Serialize(format!("写 task.json 失败：{e}")))?;
    write_package_manifest(task_dir, plan.body.plan_id.clone())
        .map_err(|e| PlanError::Serialize(format!("写 package_manifest 失败：{e}")))
}

/// CLI 入口 `af-compiler-construct --registry <包根> --task-id <号> \
/// --statement <陈述> --acceptance <判据> [--image <镜像>] [--test-command <命令>] <out_dir>`
/// ——组建设任务包（plan.json + task.json + package_manifest.json）到
/// <out_dir>，stdout 留证 JSON。失败全部 stderr 点名 + 非零退出码。
pub fn main(args: &[String]) -> ExitCode {
    let mut registry: Option<String> = None;
    let mut task_id: Option<String> = None;
    let mut statement: Option<String> = None;
    let mut acceptance: Option<String> = None;
    let mut image: Option<String> = None;
    let mut test_command: Option<String> = None;
    let mut out_dir: Option<String> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut take = |name: &str| -> Option<String> {
            if arg.as_str() == name {
                it.next().cloned()
            } else {
                None
            }
        };
        if let Some(v) = take("--registry") {
            registry = Some(v);
        } else if let Some(v) = take("--task-id") {
            task_id = Some(v);
        } else if let Some(v) = take("--statement") {
            statement = Some(v);
        } else if let Some(v) = take("--acceptance") {
            acceptance = Some(v);
        } else if let Some(v) = take("--image") {
            image = Some(v);
        } else if let Some(v) = take("--test-command") {
            test_command = Some(v);
        } else if out_dir.is_none() && !arg.starts_with("--") {
            out_dir = Some(arg.clone());
        } else {
            eprintln!("用法：af-compiler-construct --registry <包根> --task-id <号> --statement <陈述> --acceptance <判据> [--image <镜像>] [--test-command <命令>] <out_dir>");
            return ExitCode::from(2);
        }
    }
    let (Some(registry), Some(task_id), Some(statement), Some(acceptance), Some(out_dir)) =
        (registry, task_id, statement, acceptance, out_dir)
    else {
        eprintln!("--registry/--task-id/--statement/--acceptance/<out_dir> 五件必填");
        return ExitCode::from(2);
    };
    let spec = ConstructionSpec {
        task_id,
        statement,
        acceptance,
        image,
        test_command,
        registry_root: std::path::PathBuf::from(registry),
    };
    match assemble_construction_package(std::path::Path::new(&out_dir), &spec) {
        Ok(manifest) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "assembled": true,
                    "out_dir": out_dir,
                    "plan_id": manifest.plan_id,
                    "manifest": manifest,
                }))
                .unwrap_or_default()
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("建设任务包组装失败：{err}");
            ExitCode::FAILURE
        }
    }
}
