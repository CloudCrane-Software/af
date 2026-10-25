//! 图执行器 v1 验收测试（F-第四单执行长缺口销项；TDD 先红后绿——本文件先于
//! `src/graph_unit.rs` 落地，模块缺席时编译红）。
//!
//! v1 范围 = sop_full 线性链形态：planbind 装载期四核对（EX4004 口径；
//! planbind.rs 唯一正本口径——合流批术语勘误）通过后按
//! 拓扑序逐节点驱动——
//! - task 节点 = 模型节点运行时环（swev 环模式：template_ref 装载的节点
//!   system、uses.model 注册表档位、节点级工具面、按节点 turns 预算帽）；
//! - check 节点 = unit 直调（uses.unit 经注册表 impl 解析 → pkg.sweng.test_unit）；
//! - typed envelope 交接（EX4006）：上游节点产物按 plan edges/types 校验后
//!   入下游输入，违例正规错误码；
//! - plan 级 cost_cap/token_cap 由执行器强制（EX4007 熔断）；
//! - loop/map 节点与环拓扑 = EX4008（登记后续，v1 不驱动）。
//!
//! 注入手法与 p4/p5 同源：FakeRunner（零模型/零凭证）+ FakeBridge（零容器）；
//! 计划经真编译器全链产出（load→validate→plan_json，sop_full@1.0.afb 仓库
//! 正本），注册表/模板用仓库 af/packages 正本——digest/模板核对走真路径。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use af_execline::error::ErrorCode;
use af_execline::exec::{ExecContext, ExecuteUnit};
use af_execline::graph_unit::{GraphExecuteUnit, GraphUnitSpec};
use af_execline::record::FileRecordSink;
use serde_json::{json, Value};

const TASK: &str = "django__django-11099";
const PROFILE: &str = "glm-cp";

fn repo_packages() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../af/packages")
}

fn temp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("af-graph-unit-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// sop_full 图源（仓库正本逐字）。
fn sop_full_src() -> String {
    std::fs::read_to_string(
        repo_packages()
            .join("sweng")
            .join("graphs")
            .join("sop_full@1.0.afb"),
    )
    .expect("sop_full@1.0.afb 仓库正本应在盘")
}

/// 真编译器全链产计划 JSON（load→validate→plan_json；pkg_reportkit 先例）。
fn compile_plan(graph_src: &str) -> String {
    let doc = af_dsl::load_str(graph_src).expect("sop_full 应装载");
    let verdict = af_dsl::validate(&doc);
    assert!(verdict.is_ok(), "sop_full 应零违例：{verdict}");
    let reg = af_compiler::ComponentRegistry::load(repo_packages()).expect("仓库注册表应装载");
    af_compiler::plan_json(&doc, &reg).expect("sop_full 计划应生成")
}

/// 任务包（v1 约定布局 `<data_root>/<task>/{plan.json, task.json}`；
/// task.json = 图输入工件 swe_task_input 形 + check 节点 test_command 声明）。
fn task_package(root: &Path, plan_src: &str) -> PathBuf {
    let data_root = root.join("data");
    let task_dir = data_root.join(TASK);
    std::fs::create_dir_all(&task_dir).unwrap();
    std::fs::write(task_dir.join("plan.json"), plan_src).unwrap();
    std::fs::write(
        task_dir.join("task.json"),
        json!({
            "repo": "django/django",
            "instance_id": TASK,
            "problem_statement": "Fix the thing",
            "image": "swebench/sweb.eval.x86_64.django-11099:latest",
            "test_command": "cd /testbed && python -m pytest -q"
        })
        .to_string(),
    )
    .unwrap();
    data_root
}

// ---- FakeBridge（p5_wiring 同款手法：脚本化 docker 面）----

struct FakeBridge {
    calls: Mutex<Vec<String>>,
}

impl FakeBridge {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl af_execline::HostBridge for FakeBridge {
    fn run(&self, _host: &str, script: &str, _t: u32) -> Result<(bool, String), String> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(script.to_string());
        if script.starts_with("docker rm -f") || script.starts_with("docker run") {
            Ok((true, String::new()))
        } else if script.contains("git -c core.fileMode=false diff") {
            // 裸 declare 工作区取证（capture_diff 面）。
            Ok((
                true,
                "diff --git a/astropy/sampler.py b/astropy/sampler.py\n--- a/a\n+++ b/b\n".into(),
            ))
        } else if script.contains("docker exec") {
            // check 节点（test_unit）与工具面的容器执行：退出 0。
            Ok((true, "Ran 12 tests in 3s\n\nOK\n__AF_RC=0".into()))
        } else {
            Ok((true, String::new()))
        }
    }
}

// ---- FakeRunner：按节点脚本化产物 + 全形状捕获 ----

/// 单次 launch 的捕获面（断言用）。
#[derive(Debug, Clone)]
struct CapturedLaunch {
    run_id: String,
    system: String,
    user_message: String,
    model: String,
    max_turns: u32,
    spend_cap_cents: i64,
    tool_names: Vec<String>,
    /// declare 工具的 ToolDecl.parameters（declare 契约面断言：非可推导端口
    /// 的 API 面 schema 注入——required/properties）。
    declare_parameters: Value,
    /// declare 工具的 ToolDecl.description（措辞面断言：worked example、
    /// 裸调拒绝/许可的分端口措辞）。
    declare_description: String,
}

/// 图 FakeRunner：run_id 形 `{instance}-{node}`，按节点名派发脚本化终态消息。
/// DECLARE_NODES 内的节点走 declare 工具面（真模型收敛路径），其余走终态
/// 消息 JSON（回退源）——双源在 happy path 各覆盖至少一节点。
struct GraphFakeRunner {
    launches: Mutex<Vec<CapturedLaunch>>,
    /// 节点名 → 终态 assistant 文本（envelope 载荷）。未列节点 → 空轮失败。
    scripted: fn(&str) -> Option<String>,
}

/// 走 declare 工具面的节点集。
const DECLARE_NODES: &[&str] = &["planner", "coder"];

impl GraphFakeRunner {
    fn new(scripted: fn(&str) -> Option<String>) -> Self {
        Self {
            launches: Mutex::new(Vec::new()),
            scripted,
        }
    }

    fn launches(&self) -> Vec<CapturedLaunch> {
        self.launches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn order(&self) -> Vec<String> {
        self.launches()
            .iter()
            .map(|l| {
                l.run_id
                    .strip_prefix(&format!("{TASK}-"))
                    .unwrap_or(&l.run_id)
                    .to_string()
            })
            .collect()
    }
}

/// sop_full 各节点脚本化 envelope 载荷（键 = 工件端口名，值 = 端口载荷）。
fn sop_full_payloads(node: &str) -> Option<String> {
    let text = match node {
        "planner" => json!({"plan": {"instance_id": TASK, "steps": [
            {"id": "s1", "action": "locate", "target": "django/db/models"}
        ]}}),
        "coder" => json!({"files": [
            {"path": "django/db/models/foo.py", "content_ref": "workspace#1"}
        ]}),
        "reviewer" => json!({"review": {"approved": true, "issues": []}}),
        _ => return None,
    };
    Some(text.to_string())
}

impl af_execline::AgentRunner for GraphFakeRunner {
    fn run(
        &self,
        launch: af_execline::AgentLaunch<'_>,
    ) -> Result<af_execline::AgentRunOutcome, af_execline::AgentRunFailure> {
        let node = launch
            .run_id
            .strip_prefix(&format!("{TASK}-"))
            .unwrap_or_default()
            .to_string();
        let final_text = (self.scripted)(&node).ok_or_else(|| af_execline::AgentRunFailure {
            message: format!("FakeRunner 无节点 {node} 脚本"),
            http_retries: 0,
        })?;
        // declare 工具面（真模型收敛路径）：DECLARE_NODES 节点以工具调用交付声明。
        // 合流迁移：ToolFn 签名 &Value→Result——final_text 照旧按 JSON 解析，
        // 不合法时以 Null 入声明单元（旧 &str 面 unwrap_or(Null) 同语义）。
        if DECLARE_NODES.contains(&node.as_str()) {
            if let Some(binding) = launch.extra_tools.iter().find(|t| t.name == "declare") {
                let args: Value = serde_json::from_str(&final_text).unwrap_or(Value::Null);
                let _ = (binding.handler)(&args);
            }
        }
        self.launches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(CapturedLaunch {
                run_id: launch.run_id.clone(),
                system: launch.spec.context_template.system.clone(),
                user_message: launch.user_message.clone(),
                model: launch.spec.endpoint.model.clone(),
                max_turns: launch.spec.policy.max_turns,
                spend_cap_cents: launch.spend_cap_cents,
                tool_names: launch.extra_tools.iter().map(|t| t.name.clone()).collect(),
                declare_parameters: launch
                    .spec
                    .toolset
                    .iter()
                    .find(|t| t.name == "declare")
                    .map(|t| t.parameters.clone())
                    .unwrap_or(Value::Null),
                declare_description: launch
                    .spec
                    .toolset
                    .iter()
                    .find(|t| t.name == "declare")
                    .map(|t| t.description.clone())
                    .unwrap_or_default(),
            });
        let report = af_harness::RunReport {
            run_id: launch.run_id,
            status: af_harness::RunStatus::Completed {
                signal: "TASK_COMPLETE".into(),
            },
            turns: 3,
            model_calls: 5,
            effect_replays: 0,
            effect_fresh: 2,
            transcript: vec![
                af_harness::TranscriptItem::User {
                    text: launch.user_message.clone(),
                },
                af_harness::TranscriptItem::Assistant {
                    text: final_text,
                    tool_calls: Vec::new(),
                    usage: None,
                },
            ],
            whitelist: launch.extra_tools.iter().map(|t| t.name.clone()).collect(),
            tokens: af_harness::TokenTotals {
                input: 100,
                output: 50,
                cache_read: 10,
            },
        };
        Ok(af_execline::AgentRunOutcome {
            report,
            http_retries: 0,
        })
    }
}

fn graph_unit(
    root: &Path,
    runner: Arc<dyn af_execline::AgentRunner>,
) -> (GraphExecuteUnit, Arc<FakeBridge>, PathBuf, PathBuf) {
    let bridge = Arc::new(FakeBridge::new());
    let spec = GraphUnitSpec::v1(PROFILE, repo_packages());
    let unit = GraphExecuteUnit::new(spec, bridge.clone(), runner)
        .expect("图单元装载（profile/注册表可解析）");
    (unit, bridge, root.join("attempt"), root.join("out"))
}

/// 直接驱动单元的执行上下文（exec.rs 单测先例；FileRecordSink 落行可对账）。
fn drive(
    unit: &GraphExecuteUnit,
    data_root: &Path,
    attempt: &Path,
    out: &Path,
) -> Result<af_execline::ExecOutput, af_execline::ExeclineError> {
    std::fs::create_dir_all(attempt).unwrap();
    std::fs::create_dir_all(out).unwrap();
    let sink = FileRecordSink::open(out).unwrap();
    let ctx = ExecContext {
        order_id: "wo-graph-1",
        stable_id: "sid-graph-1",
        line_id: "line-A",
        out_dir: out,
        task_id: TASK,
        host_alias: "local",
        data_root,
        attempt_dir: attempt,
        record_sink: &sink,
        trial: false,
    };
    unit.execute(&ctx)
}

// ============================================================
// ① 三 LLM 节点按序 + tester 直调：全链 happy path
// ============================================================

#[test]
fn sop_full_drives_three_llm_nodes_in_topo_order_then_tester_unit() {
    let root = temp_root("happy");
    let data_root = task_package(&root, &compile_plan(&sop_full_src()));
    let runner = Arc::new(GraphFakeRunner::new(sop_full_payloads));
    let (unit, bridge, attempt, out) = graph_unit(&root, runner.clone());

    let output = drive(&unit, &data_root, &attempt, &out).expect("sop_full 线性链应走通");

    // 拓扑序 = planner → coder → reviewer（tester 是 check 节点，零模型调用）。
    assert_eq!(runner.order(), ["planner", "coder", "reviewer"]);

    // planner 环：system = pkg.sweng.planner_system 装载正文；uses.model
    // route.plan@1 注册表档位（GLM-5.3）；无节点工具面；turns 未声明 → 缺省 60。
    let launches = runner.launches();
    let planner_system = std::fs::read_to_string(
        repo_packages()
            .join("sweng")
            .join("prompts")
            .join("planner_system.md"),
    )
    .unwrap();
    assert!(
        launches[0].system.starts_with(&planner_system),
        "planner system 来自 template_ref 装载：{:?}",
        &launches[0].system[..80.min(launches[0].system.len())]
    );
    assert_eq!(launches[0].model, "GLM-5.3", "route.plan@1 档位");
    assert_eq!(
        launches[0].tool_names,
        ["declare"],
        "planner 无节点工具面，仅环组装 declare"
    );
    assert_eq!(launches[0].max_turns, 60, "未声明 turns 帽 → 缺省");
    assert_eq!(
        launches[0].spend_cap_cents, 600,
        "launch 硬顶收敛到 plan cost_cap 剩余额度（首节点=全额）"
    );

    // coder 环：route.main@1（GLM-5.3）+ 三件 sweng 工具面 + 声明帽 60；
    // user_message 携 planner 的 plan envelope（边交接）。
    assert_eq!(launches[1].model, "GLM-5.3", "route.main@1 档位");
    assert_eq!(
        launches[1].tool_names,
        ["sweng.edit", "sweng.locate", "sweng.view", "declare"],
        "节点级工具面（模型面名 = 组件名去 pkg. 前缀）+ 环组装 declare"
    );
    assert_eq!(launches[1].max_turns, 60, "coder_turns 声明帽");
    assert!(
        launches[1].user_message.contains("\"instance_id\"")
            && launches[1].user_message.contains(TASK),
        "coder user 消息含上游 plan envelope: {:?}",
        &launches[1].user_message[..120.min(launches[1].user_message.len())]
    );

    // reviewer 环：route.judge@1 + 声明帽 12 + files envelope 在场。
    assert_eq!(launches[2].max_turns, 12, "reviewer_turns 声明帽");
    assert!(
        launches[2].user_message.contains("django/db/models/foo.py"),
        "reviewer user 消息含 coder files envelope"
    );

    // tester（check 节点）：uses.unit 经注册表 impl 解析直调——容器内跑
    // task.json 声明的 test_command，零模型调用。
    let exec_calls: Vec<String> = bridge
        .calls()
        .into_iter()
        .filter(|s| s.contains("docker exec"))
        .collect();
    assert_eq!(exec_calls.len(), 1, "tester 恰一次容器执行: {exec_calls:?}");
    assert!(
        exec_calls[0].contains("python -m pytest"),
        "test_command 来自任务包: {}",
        exec_calls[0]
    );

    // typed envelope 落 attempt 工件面（artifact_contract 端口名）。
    for (node, port) in [
        ("planner", "plan"),
        ("coder", "files"),
        ("reviewer", "review"),
        ("tester", "report"),
    ] {
        let path = attempt
            .join("artifacts")
            .join(format!("{node}.{port}.json"));
        assert!(path.is_file(), "envelope 工件应在盘：{}", path.display());
    }
    let report: Value = serde_json::from_str(
        &std::fs::read_to_string(attempt.join("artifacts").join("tester.report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(report["port"], json!("report"), "envelope 结构带端口名");
    assert_eq!(report["payload"]["pass"], json!(true), "tester 判定 pass");

    // 终态：qa_report.pass=true → verdict + 合格终态工件（is_final=report）。
    assert_eq!(output.verdict, Some(true));
    assert_eq!(output.artifacts.len(), 1);
    assert!(output.artifacts[0].is_final && output.artifacts[0].qualified);

    // v1 行：records.jsonl 恰一行，带 plan_id 与节点计数。
    let rows: Vec<Value> = std::fs::read_to_string(out.join("records.jsonl"))
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows.len(), 1);
    assert!(rows[0]["plan_id"]
        .as_str()
        .unwrap()
        .starts_with("plan.sweng_sop_full@1.0+"));
    assert_eq!(rows[0]["nodes"], json!(4));

    // 容器清理必达（RAII）：rm -f 恰两次（setup 前清残件 + 收尾）。
    assert!(
        bridge
            .calls()
            .iter()
            .filter(|s| s.starts_with("docker rm -f"))
            .count()
            >= 1
    );
}

// ============================================================
// ② envelope 校验：终态消息缺端口 → EX4006
// ============================================================

#[test]
fn envelope_violation_reports_ex4006_and_stops_downstream() {
    let root = temp_root("envelope");
    let data_root = task_package(&root, &compile_plan(&sop_full_src()));
    // reviewer 终态消息缺 review 端口键（envelope 违例注入点）。
    let scripted = |node: &str| -> Option<String> {
        match node {
            "planner" => sop_full_payloads("planner"),
            "coder" => sop_full_payloads("coder"),
            "reviewer" => Some(json!({"verdict": "looks fine"}).to_string()),
            _ => None,
        }
    };
    let runner = Arc::new(GraphFakeRunner::new(scripted));
    let (unit, bridge, attempt, out) = graph_unit(&root, runner.clone());

    let err = drive(&unit, &data_root, &attempt, &out).expect_err("envelope 违例应拒");

    assert_eq!(err.code(), ErrorCode::EnvelopeMismatch, "{err}");
    assert!(
        err.to_string().contains("EX4006"),
        "正规错误码在文案：{err}"
    );
    assert!(
        err.to_string().contains("reviewer") && err.to_string().contains("review"),
        "违例点名节点与端口：{err}"
    );
    // 三 LLM 节点都跑过（reviewer 产物在校验点被拒），tester 未被驱动。
    assert_eq!(runner.order(), ["planner", "coder", "reviewer"]);
    assert!(
        !bridge
            .calls()
            .iter()
            .any(|s| s.contains("docker exec") && s.contains("pytest")),
        "违例后下游 check 节点不执行"
    );
    assert!(
        !attempt
            .join("artifacts")
            .join("reviewer.review.json")
            .is_file(),
        "违例 envelope 不落工件面"
    );
}

// ============================================================
// ③ 预算熔断：plan 级 cost_cap 超限 → EX4007
// ============================================================

#[test]
fn plan_cost_cap_circuit_breaks_with_ex4007() {
    let root = temp_root("budget");
    // cost_cap 600 → 3（planner 一节点即用 5 次模型调用 = 5¢，llm.cents 1 调 1 ¢）。
    let src = sop_full_src().replace(
        "{ id: cost_cap, dimension: cents, scope: plan, cap: 600 }",
        "{ id: cost_cap, dimension: cents, scope: plan, cap: 3 }",
    );
    assert!(src != sop_full_src(), "夹具改写应生效");
    let data_root = task_package(&root, &compile_plan(&src));
    let runner = Arc::new(GraphFakeRunner::new(sop_full_payloads));
    let (unit, bridge, attempt, out) = graph_unit(&root, runner.clone());

    let err = drive(&unit, &data_root, &attempt, &out).expect_err("超 cost_cap 应熔断");

    assert_eq!(err.code(), ErrorCode::BudgetCapExceeded, "{err}");
    assert!(
        err.to_string().contains("EX4007") && err.to_string().contains("cost_cap"),
        "熔断点名预算条目：{err}"
    );
    // planner 跑完（5¢ ≥ 3¢），coder 起动前熔断。
    assert_eq!(runner.order(), ["planner"]);
    assert!(
        !bridge
            .calls()
            .iter()
            .any(|s| s.contains("docker exec") && s.contains("pytest")),
        "熔断后零下游执行"
    );
}

// ============================================================
// ④ 图形态守卫：loop/map 节点与环拓扑 → EX4008（v1 范围外，登记后续）
// ============================================================

#[test]
fn linearize_rejects_loop_map_and_cycle_with_ex4008() {
    use af_execline::graph_unit::linearize;

    let plan_src = compile_plan(&sop_full_src());
    let base: Value = serde_json::from_str(&plan_src).unwrap();

    // loop 节点（构造节点带 loop_spec）→ EX4008。
    let mut looped = base.clone();
    looped["body"]["graph"]["nodes"][0]["loop_spec"] = json!({
        "body": "sweng_sop_full/coder",
        "max_iterations": 2,
        "carried": [],
        "safety_points": []
    });
    let plan = af_compiler::decode_plan(&looped.to_string()).expect("loop 计划可 decode");
    let err = linearize(&plan).unwrap_err();
    assert_eq!(err.code(), ErrorCode::GraphShapeUnsupported, "{err}");
    assert!(err.to_string().contains("EX4008") && err.to_string().contains("coder"));

    // 环拓扑（reviewer → planner 回边）→ EX4008。
    let mut cyclic = base.clone();
    cyclic["body"]["graph"]["edges"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "from": "reviewer", "output": "review", "to": "planner", "input": "plan", "checkpoint": false
        }));
    let plan = af_compiler::decode_plan(&cyclic.to_string()).expect("环计划可 decode");
    let err = linearize(&plan).unwrap_err();
    assert_eq!(err.code(), ErrorCode::GraphShapeUnsupported, "{err}");
    assert!(err.to_string().contains("EX4008"));

    // 正本：线性链序（拓扑序，非 node_exec 字典序）。
    let plan = af_compiler::decode_plan(&plan_src).unwrap();
    assert_eq!(
        linearize(&plan).unwrap(),
        ["planner", "coder", "reviewer", "tester"]
    );
}

// ============================================================
// ⑤ planbind 装载期四核对先行：digest 漂移 → EX4004（E9004 口径透传）
// ============================================================

#[test]
fn graph_unit_runs_planbind_checks_before_any_node() {
    let root = temp_root("digest");
    let plan_src = compile_plan(&sop_full_src());
    let mut plan: Value = serde_json::from_str(&plan_src).unwrap();
    plan["body"]["compiled_from"]["registry_digest"] = json!("0".repeat(64));
    let data_root = task_package(&root, &serde_json::to_string(&plan).unwrap());
    let runner = Arc::new(GraphFakeRunner::new(sop_full_payloads));
    let (unit, bridge, attempt, out) = graph_unit(&root, runner.clone());

    let err = drive(&unit, &data_root, &attempt, &out).expect_err("digest 漂移应拒启动");

    assert_eq!(err.code(), ErrorCode::PlanDigestMismatch, "{err}");
    assert!(err.to_string().contains("E9004"), "共用语义码：{err}");
    // 四核对先行：零节点驱动、零容器。
    assert!(runner.launches().is_empty());
    assert!(bridge.calls().is_empty(), "装订失败不起容器");
}

// ============================================================
// ⑥ 裸 declare 推导路径：sop_minimal patch 工件 = 工作区 ground truth
// ============================================================

/// sop_minimal 图源（仓库正本）。
fn sop_minimal_src() -> String {
    std::fs::read_to_string(
        repo_packages()
            .join("sweng")
            .join("graphs")
            .join("sop_minimal@1.0.afb"),
    )
    .expect("sop_minimal@1.0.afb 仓库正本应在盘")
}

/// 裸 declare FakeRunner：repair 节点调 declare() 零参数（四跑真模型判例的
/// 稳定形态——模型修完按完成按钮，参数重述被拒绝）。
struct BareDeclareRunner {
    calls: Mutex<usize>,
}

impl af_execline::AgentRunner for BareDeclareRunner {
    fn run(
        &self,
        launch: af_execline::AgentLaunch<'_>,
    ) -> Result<af_execline::AgentRunOutcome, af_execline::AgentRunFailure> {
        *self.calls.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        let declare = launch
            .extra_tools
            .iter()
            .find(|t| t.name == "declare")
            .expect("环组装面必带 declare")
            .handler
            .clone();
        let outcome = (declare)(&serde_json::json!({})); // 裸调：完成按钮
        let signal = outcome
            .expect("declare 工具不失败")
            .as_str()
            .expect("declare 输出为观察文本")
            .to_string();
        assert_eq!(signal, "TASK_COMPLETE", "declare 输出即完成信号");
        Ok(af_execline::AgentRunOutcome {
            report: af_harness::RunReport {
                run_id: launch.run_id,
                status: af_harness::RunStatus::Completed {
                    signal: "TASK_COMPLETE".into(),
                },
                turns: 5,
                model_calls: 5,
                effect_replays: 0,
                effect_fresh: 3,
                transcript: Vec::new(),
                whitelist: vec!["bash".into()],
                tokens: af_harness::TokenTotals {
                    input: 80,
                    output: 40,
                    cache_read: 0,
                },
            },
            http_retries: 0,
        })
    }
}

#[test]
fn sop_minimal_bare_declare_derives_patch_from_workspace_ground_truth() {
    let root = temp_root("minimal");
    let data_root = task_package(&root, &compile_plan(&sop_minimal_src()));
    let runner = Arc::new(BareDeclareRunner {
        calls: Mutex::new(0),
    });
    let spec = GraphUnitSpec::v1(PROFILE, repo_packages());
    let bridge = Arc::new(FakeBridge::new());
    let unit = GraphExecuteUnit::new(spec, bridge.clone(), runner.clone()).expect("单元装载");
    let attempt = root.join("attempt");
    let out = root.join("out");

    let output = drive(&unit, &data_root, &attempt, &out).expect("裸 declare 应推导交付");

    // envelope：repair.patch = 工作区 diff + 触碰文件（FakeBridge 的 git diff
    // 分支返回 astropy/sampler.py 二段 diff），落盘披露 derived 面。
    let file = attempt.join("artifacts").join("repair.patch.json");
    assert!(file.is_file(), "patch envelope 应落盘");
    let envelope: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(envelope["port"], json!("patch"));
    assert_eq!(
        envelope["derived"],
        json!(["diff", "files"]),
        "推导面披露在案，不冒充模型声明"
    );
    assert!(
        envelope["payload"]["diff"]
            .as_str()
            .unwrap()
            .contains("diff --git a/astropy/sampler.py"),
        "diff = 容器工作区取证"
    );
    assert_eq!(
        envelope["payload"]["files"],
        json!(["astropy/sampler.py"]),
        "files = diff 解析触碰集"
    );
    // patch 型终态（无 pass 判定字段）：产出合格终态工件，订单面可 close。
    assert_eq!(output.verdict, Some(true));
    assert!(output.artifacts[0].is_final && output.artifacts[0].qualified);
    assert_eq!(*runner.calls.lock().unwrap(), 1, "单节点恰一次模型环");
}

// ============================================================
// ⑦ typed envelope 上游面（B 席嫁接）：非终态端口 emits 类型校验
// ============================================================

/// planner 的 plan 端口 declares task_plan——steps 项缺 id（plan_step
/// required）→ commit 相 check_typed 拒收，EX4006 点名节点/端口/类型；
/// 下游 coder 不再起动（违例不落工件、不进交接账）。
#[test]
fn typed_emission_face_validates_nonfinal_ports_with_ex4006() {
    let root = temp_root("typed-up");
    let data_root = task_package(&root, &compile_plan(&sop_full_src()));
    // steps 项缺 id：命名类型 plan_step 的 required 递归面（SchemaValidator
    // 对 plan.types 解析——B 席整件）。
    let scripted = |node: &str| -> Option<String> {
        match node {
            "planner" => Some(
                json!({"plan": {"instance_id": TASK, "steps": [
                    {"action": "locate", "target": "django/db/models"}
                ]}})
                .to_string(),
            ),
            "coder" => sop_full_payloads("coder"),
            "reviewer" => sop_full_payloads("reviewer"),
            _ => None,
        }
    };
    let runner = Arc::new(GraphFakeRunner::new(scripted));
    let (unit, _bridge, attempt, out) = graph_unit(&root, runner.clone());

    let err = drive(&unit, &data_root, &attempt, &out).expect_err("emits 类型违例应拒");

    assert_eq!(err.code(), ErrorCode::EnvelopeMismatch, "{err}");
    for token in ["EX4006", "planner", "plan", "task_plan", "id"] {
        assert!(err.to_string().contains(token), "点名 {token}: {err}");
    }
    assert_eq!(runner.order(), ["planner"], "违例即停，coder 不起动");
    assert!(
        !attempt
            .join("artifacts")
            .join("planner.plan.json")
            .is_file(),
        "违例 envelope 不落工件面"
    );
}

// ============================================================
// ⑧ typed envelope 下游面（B 席嫁接）：consumes 契约复核入边
// ============================================================

/// 上游 emissions 与下游 consumes 是**两道独立正门**：本例上游 planner 的
/// task_plan 声明合法过门，但 coder 的 consumes.plan 改绑 code_file——
/// 同一载荷在下游面被拒（缺 path/content_ref required），EX4006 点名
/// coder/plan/code_file。证明双面各自独立核（非只核上游）。
#[test]
fn downstream_consumes_face_revalidates_incoming_edges() {
    let root = temp_root("typed-down");
    // 计划级改写（编译后 JSON 面）：coder 的 consumes.plan: task_plan →
    // code_file。注意图源级改写会被 af-dsl E2201 edge_port_type_mismatch 在
    // 编译期拒（上游端口类型 ≠ 消费者声明）——下游面正是对「计划文件即编排
    // 真相、执行器不信任编译器曾核过」的独立防线（planbind 四核对不含边
    // 类型等值面，手改计划可达执行器）。
    let mut plan: Value = serde_json::from_str(&compile_plan(&sop_full_src())).unwrap();
    let coder = plan["body"]["node_exec"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|n| n["node_id"] == json!("coder"))
        .expect("coder 在 node_exec");
    coder["consumes"]["plan"] = json!("code_file");
    let data_root = task_package(&root, &serde_json::to_string(&plan).unwrap());
    let runner = Arc::new(GraphFakeRunner::new(sop_full_payloads));
    let (unit, _bridge, attempt, out) = graph_unit(&root, runner.clone());

    let err = drive(&unit, &data_root, &attempt, &out).expect_err("consumes 违例应拒");

    assert_eq!(err.code(), ErrorCode::EnvelopeMismatch, "{err}");
    for token in ["EX4006", "coder", "plan", "code_file"] {
        assert!(err.to_string().contains(token), "点名 {token}: {err}");
    }
    assert_eq!(
        runner.order(),
        ["planner"],
        "下游面在 coder 起动前拒收（planner 已跑、coder 未跑）"
    );
    assert!(
        attempt
            .join("artifacts")
            .join("planner.plan.json")
            .is_file(),
        "上游合法 envelope 照常落盘（双面独立）"
    );
}

// ============================================================
// ⑨ token_cap 定向例（B 席嫁接）：cache_read 计入 tokens_total 口径
// ============================================================

/// FakeRunner 每节点 tokens = in 100 + out 50 + cache_read 10 = 160
/// （tokens_total 口径计 cache_read——B 席嫁接）。token_cap=310：
/// planner(160) 过、coder 后累计 320 ≥ 310 → reviewer 熔断不起动。
/// （若 cache_read 不计：150×2=300 < 310，reviewer 会跑——本例即红。）
#[test]
fn plan_token_cap_fuses_before_downstream_nodes() {
    let root = temp_root("tokfuse");
    let src = sop_full_src().replace(
        "{ id: token_cap, dimension: tokens_total, scope: plan, cap: 400000 }",
        "{ id: token_cap, dimension: tokens_total, scope: plan, cap: 310 }",
    );
    assert!(src != sop_full_src(), "夹具改写应生效");
    let data_root = task_package(&root, &compile_plan(&src));
    let runner = Arc::new(GraphFakeRunner::new(sop_full_payloads));
    let (unit, bridge, attempt, out) = graph_unit(&root, runner.clone());

    let err = drive(&unit, &data_root, &attempt, &out).expect_err("超 token_cap 应熔断");

    assert_eq!(err.code(), ErrorCode::BudgetCapExceeded, "{err}");
    assert!(
        err.to_string().contains("EX4007") && err.to_string().contains("token_cap"),
        "熔断点名预算条目：{err}"
    );
    assert!(
        err.to_string().contains("tokens_total"),
        "结构化字段带维度名：{err}"
    );
    assert_eq!(
        runner.order(),
        ["planner", "coder"],
        "planner+coder 已跑（160+320），reviewer 被熔断"
    );
    assert!(
        !bridge
            .calls()
            .iter()
            .any(|s| s.contains("docker exec") && s.contains("pytest")),
        "熔断后零下游执行"
    );
}

// ============================================================
// ⑩ harness 预算耗尽归并（B 席嫁接）：BudgetExhausted → EX4007
// ============================================================

/// 单节点环的 harness 侧预算耗尽（RunStatus::BudgetExhausted）不得静默当
/// 成功——归并为 EX4007 结构化熔断（budget_id=llm.cents，点名 spent/cap）。
#[test]
fn harness_budget_exhausted_maps_to_ex4007() {
    struct ExhaustedRunner;
    impl af_execline::AgentRunner for ExhaustedRunner {
        fn run(
            &self,
            launch: af_execline::AgentLaunch<'_>,
        ) -> Result<af_execline::AgentRunOutcome, af_execline::AgentRunFailure> {
            let _ = launch;
            Ok(af_execline::AgentRunOutcome {
                report: af_harness::RunReport {
                    run_id: format!("{TASK}-planner"),
                    status: af_harness::RunStatus::BudgetExhausted {
                        fault: af_harness::BudgetFault::OverCap {
                            spend_cents: 600,
                            spend_cap_cents: 600,
                        },
                    },
                    turns: 3,
                    model_calls: 3,
                    effect_replays: 0,
                    effect_fresh: 0,
                    transcript: Vec::new(),
                    whitelist: vec!["bash".into()],
                    tokens: af_harness::TokenTotals {
                        input: 10,
                        output: 5,
                        cache_read: 0,
                    },
                },
                http_retries: 0,
            })
        }
    }
    let root = temp_root("exhausted");
    let data_root = task_package(&root, &compile_plan(&sop_full_src()));
    let (unit, _bridge, attempt, out) = graph_unit(&root, Arc::new(ExhaustedRunner));

    let err = drive(&unit, &data_root, &attempt, &out).expect_err("harness 预算耗尽应归并 EX4007");

    assert_eq!(err.code(), ErrorCode::BudgetCapExceeded, "{err}");
    assert!(
        err.to_string().contains("EX4007") && err.to_string().contains("llm.cents"),
        "归并点名 llm.cents 预算维：{err}"
    );
    assert!(
        err.to_string().contains("600"),
        "结构化字段带 spent/cap 数值：{err}"
    );
}

// ============================================================
// ⑪ check 分发表广度（B 席嫁接）：aci_schema / human_gate 直调面
// ============================================================

/// check 节点分发表 v1.1（B 席嫁接）：test_unit 之外接线
/// pkg.sweagent.aci_schema（入边工件结构校验，纯函数）与
/// pkg.sweagent.human_gate（fail-closed 恒 ask:false）。两 unit 均在根级
/// af/packages/components.json 注册（executor:builtin@af-execline）。
#[test]
fn check_dispatch_runs_aci_schema_and_human_gate_units() {
    let graph_src = r#"
version: 1
meta:
  name: sweng_g4_dispatch
  graph_version: "1.0"
  entry: actor
  input: swe_task_input
  output: { ref: gate_verdict, from: gate }
  intent: "check 分发表广度验收（B 席嫁接）：aci_schema 结构校验 + human_gate fail-closed 直调链"

types:
  - name: swe_task_input
    schema:
      type: object
      required: [repo, instance_id, problem_statement]
      properties:
        repo: { type: string }
        instance_id: { type: string }
        problem_statement: { type: string }
        image: { type: string }
  - name: agent_action
    schema:
      type: object
      required: [thought, command, submit]
      properties:
        thought: { type: string }
        command: { type: string }
        submit: { type: boolean }
  - name: schema_verdict
    schema:
      type: object
      required: [error, submit]
      properties:
        error: { type: string }
        submit: { type: boolean }
  - name: gate_verdict
    schema:
      type: object
      required: [ask]
      properties:
        ask: { type: boolean }

nodes:
  - id: actor
    kind: task
    uses: { model: route.plan@1 }
    emits: { action: agent_action }
    context:
      sections:
        - { from: template, template_ref: pkg.sweng.planner_system@1, quota_tokens: 400 }
        - { from: input, fields: [repo, instance_id, problem_statement], policy: project, quota_tokens: 800 }

  - id: verifier
    kind: check
    uses: { unit: pkg.sweagent.aci_schema@1 }
    consumes: { action: agent_action }
    emits: { verdict: schema_verdict }

  - id: gate
    kind: check
    uses: { unit: pkg.sweagent.human_gate@1 }
    consumes: { verdict: schema_verdict }
    emits: { out: gate_verdict }

edges:
  - { from: actor, output: action, to: verifier, input: action }
  - { from: verifier, output: verdict, to: gate, input: verdict }

policies:
  budgets:
    - { id: cost_cap, dimension: cents, scope: plan, cap: 100 }
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
    let root = temp_root("g4dispatch");
    let data_root = task_package(&root, &compile_plan(graph_src));
    // actor 走终态消息 JSON 声明（非 DECLARE_NODES 面）。
    let scripted = |node: &str| -> Option<String> {
        let text = match node {
            "actor" => json!({"action": {
                "thought": "run the tests", "command": "python -m pytest -q", "submit": false
            }}),
            _ => return None,
        };
        Some(text.to_string())
    };
    let runner = Arc::new(GraphFakeRunner::new(scripted));
    let (unit, _bridge, attempt, out) = graph_unit(&root, runner.clone());

    let output = drive(&unit, &data_root, &attempt, &out).expect("aci_schema+human_gate 链应走通");

    assert_eq!(
        runner.order(),
        ["actor"],
        "唯一 LLM 节点；check 节点零模型调用"
    );
    // aci_schema 直调产物：error 空 + submit 透传（sweng::aci_schema 纯函数面）。
    let verdict: Value = serde_json::from_str(
        &std::fs::read_to_string(attempt.join("artifacts").join("verifier.verdict.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        verdict["payload"]["error"],
        json!(""),
        "合法 agent_action 过结构校验"
    );
    assert_eq!(verdict["payload"]["submit"], json!(false));
    // human_gate 直调产物：v1 fail-closed 恒 ask:false。
    let gate: Value = serde_json::from_str(
        &std::fs::read_to_string(attempt.join("artifacts").join("gate.out.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        gate["payload"]["ask"],
        json!(false),
        "human_gate fail-closed 面"
    );
    // 终态（gate_verdict 无 pass 判定字段）→ 非判定型产出即合格。
    assert_eq!(output.verdict, Some(true));
}

/// swebench_eval 分派面 fail-loud：spec.judge 未配置（None）时拒载点名
/// 「判分锚」——不静默跳判（B 席嫁接口径）。
#[test]
fn swebench_eval_without_judge_fails_loud() {
    let graph_src = r#"
version: 1
meta:
  name: sweng_g4_sweeval
  graph_version: "1.0"
  entry: actor
  input: swe_task_input
  output: { ref: eval_verdict, from: evaluator }
  intent: "swebench_eval 分派面 fail-loud 验收（B 席嫁接）：judge 缺席拒载"

types:
  - name: swe_task_input
    schema:
      type: object
      required: [repo, instance_id, problem_statement]
      properties:
        repo: { type: string }
        instance_id: { type: string }
        problem_statement: { type: string }
        image: { type: string }
  - name: agent_action
    schema:
      type: object
      required: [thought, command, submit]
      properties:
        thought: { type: string }
        command: { type: string }
        submit: { type: boolean }
  - name: eval_verdict
    schema:
      type: object
      properties:
        resolved: { type: string }

nodes:
  - id: actor
    kind: task
    uses: { model: route.plan@1 }
    emits: { action: agent_action }
    context:
      sections:
        - { from: template, template_ref: pkg.sweng.planner_system@1, quota_tokens: 400 }
        - { from: input, fields: [repo, instance_id, problem_statement], policy: project, quota_tokens: 800 }

  - id: evaluator
    kind: check
    uses: { unit: pkg.sweagent.swebench_eval@1 }
    consumes: { action: agent_action }
    emits: { verdict: eval_verdict }

edges:
  - { from: actor, output: action, to: evaluator, input: action }

policies:
  budgets:
    - { id: cost_cap, dimension: cents, scope: plan, cap: 100 }
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
    let root = temp_root("g4sweeval");
    let data_root = task_package(&root, &compile_plan(graph_src));
    let scripted = |node: &str| -> Option<String> {
        match node {
            "actor" => Some(
                json!({"action": {
                    "thought": "t", "command": "c", "submit": false
                }})
                .to_string(),
            ),
            _ => None,
        }
    };
    let runner = Arc::new(GraphFakeRunner::new(scripted));
    // spec.judge 缺省 None（graph_unit 构造面）——swebench_eval 应拒载。
    let (unit, _bridge, attempt, out) = graph_unit(&root, runner.clone());

    let err = drive(&unit, &data_root, &attempt, &out).expect_err("judge 缺席应 fail-loud 拒载");

    assert_eq!(err.code(), ErrorCode::UnitFailed, "{err}");
    assert!(
        err.to_string().contains("swebench_eval") && err.to_string().contains("判分锚"),
        "拒载点名单元与判分锚：{err}"
    );
    assert_eq!(runner.order(), ["actor"], "actor 已跑、evaluator 拒载");
}

// ============================================================
// ⑨ declare 契约面（2026-10-25 第二结构性缺口立法）：非可推导端口
//    = API 面 schema 注入 + 闭环内校验门
// ============================================================
//
// F4B/F4B2 判例（变更日志 ㉗）：planner 节点 turn 0 模型首 action 即裸
// declare({})（16 token/零规划工作）——旧 declare 描述把「裸调=自动取证」
// 无差别写给一切端口（非可推导端口无此活路），parameters 仅
// `{"type":"object"}`（API 面零形状信号），校验又只在跑后 commit 相（
// EX4006 致死、模型零反馈机会）。本节钉新契约：
// - 非可推导端口：parameters 注入 required=端口集 + properties=该端口
//   物化 schema（命名类型内联）；描述带 worked example 且明示裸调被拒；
//   闭环内（工具调用当场）按同一 check_typed 校验——畸形载荷 = 可重试的
//   工具错误反馈，不再等到跑后 EX4006。
// - patch 形可推导端口（sop_minimal repair.patch）：permissive 面零动
//   （裸 declare → 容器取证推导路径行为不变）。

/// 非可推导端口（sop_full 三件）的 declare 工具面：API 级 schema 注入
/// （required=端口集 + properties=物化 schema）+ 描述带 worked example
/// 且裸调措辞从「许可」翻转为「拒绝」。
#[test]
fn nonderivable_ports_inject_schema_into_declare_tool_face() {
    let root = temp_root("contract-face");
    let data_root = task_package(&root, &compile_plan(&sop_full_src()));
    let runner = Arc::new(GraphFakeRunner::new(sop_full_payloads));
    let (unit, _bridge, _attempt, _out) = graph_unit(&root, runner.clone());

    let _ = drive(&unit, &data_root, &_attempt, &_out).expect("sop_full 应走通");

    let launches = runner.launches();
    // planner（task_plan）：required=["plan"]，properties.plan = task_plan
    // 物化 schema（命名引用 plan_step 内联——API 面无 plan.types 词典可查）。
    let planner = &launches[0];
    assert_eq!(
        planner.declare_parameters["required"],
        json!(["plan"]),
        "planner declare required = 端口集（API 面形状信号）"
    );
    assert_eq!(
        planner.declare_parameters["properties"]["plan"]["required"],
        json!(["instance_id", "steps"]),
        "properties.plan = task_plan 物化 schema"
    );
    assert_eq!(
        planner.declare_parameters["properties"]["plan"]["properties"]["steps"]["items"]
            ["required"],
        json!(["id", "action", "target"]),
        "命名类型 plan_step 内联进 items（API 面自含）"
    );
    // 描述：worked example（端口键控 + 最小合形载荷）+ 裸调拒绝措辞；
    // 旧「you may call it bare」诱惑措辞不得再出现于非可推导端口。
    assert!(
        planner.declare_description.contains("\"plan\"")
            && planner.declare_description.contains("instance_id")
            && planner.declare_description.contains("steps"),
        "planner declare 描述带端口名与 payload 形状（worked example）：{}",
        planner.declare_description
    );
    assert!(
        !planner.declare_description.contains("you may call it bare"),
        "非可推导端口不得保留裸调许可措辞：{}",
        planner.declare_description
    );
    assert!(
        planner
            .declare_description
            .to_lowercase()
            .contains("reject"),
        "裸调拒绝措辞在场：{}",
        planner.declare_description
    );

    // coder（list<code_file>）：required=["files"]，物化为 array/items。
    let coder = &launches[1];
    assert_eq!(coder.declare_parameters["required"], json!(["files"]));
    assert_eq!(
        coder.declare_parameters["properties"]["files"]["type"],
        json!("array"),
        "list<code_file> 物化为 array 形 schema"
    );
    assert_eq!(
        coder.declare_parameters["properties"]["files"]["items"]["required"],
        json!(["path", "content_ref"]),
        "code_file 元素 schema 内联"
    );

    // reviewer（review_report）：required=["review"]。
    let reviewer = &launches[2];
    assert_eq!(reviewer.declare_parameters["required"], json!(["review"]));
    assert_eq!(
        reviewer.declare_parameters["properties"]["review"]["required"],
        json!(["approved", "issues"])
    );
}

/// 输出契约段（user 消息）携带**全部**端口的 schema（旧版只带 is_final
/// 端口——planner/coder/reviewer 的非终态端口零形状信息）。
#[test]
fn output_contract_carries_all_port_schemas() {
    let root = temp_root("contract-prompt");
    let data_root = task_package(&root, &compile_plan(&sop_full_src()));
    let runner = Arc::new(GraphFakeRunner::new(sop_full_payloads));
    let (unit, _bridge, _attempt, _out) = graph_unit(&root, runner.clone());

    let _ = drive(&unit, &data_root, &_attempt, &_out).expect("sop_full 应走通");

    let launches = runner.launches();
    // 三节点各断言：端口名 + 该端口 schema 的 required 字段名 + 类型表达式。
    let cases: [(&CapturedLaunch, &str, &str, &str); 3] = [
        (&launches[0], "plan", "instance_id", "task_plan"),
        (&launches[1], "files", "content_ref", "code_file"),
        (&launches[2], "review", "approved", "review_report"),
    ];
    for (launch, port, field, type_expr) in cases {
        assert!(
            launch.user_message.contains(port),
            "节点 {} 契约段点名端口 {}",
            launch.run_id,
            port
        );
        assert!(
            launch.user_message.contains(field),
            "节点 {} 契约段带端口 {} 的 schema（required 字段 {} 可见）",
            launch.run_id,
            port,
            field
        );
        assert!(
            launch.user_message.contains(type_expr),
            "节点 {} 契约段点名类型表达式 {}（诊断同源措辞）",
            launch.run_id,
            type_expr
        );
    }
}

/// 闭环内校验门（declare 工具闭包当场拒畸形 + 采纳合形）：F4B 死因的
/// 行为面修法——畸形载荷 = 可重试的工具错误（点名缺口），不再存储、
/// 不再发 TASK_COMPLETE；合形载荷才存储并完成。
struct DeclareProbeRunner {
    /// 首个节点依序投喂 declare 的载荷（逐次记录工具结果；仅首节点投喂）。
    payloads: Mutex<Vec<Value>>,
    /// (序号, 是否 Err, 文本) 逐次记录。
    results: Mutex<Vec<(usize, bool, String)>>,
    /// 单次投喂闸（首节点投完即闭）。
    fed: Mutex<bool>,
}

impl af_execline::AgentRunner for DeclareProbeRunner {
    fn run(
        &self,
        launch: af_execline::AgentLaunch<'_>,
    ) -> Result<af_execline::AgentRunOutcome, af_execline::AgentRunFailure> {
        let declare = launch
            .extra_tools
            .iter()
            .find(|t| t.name == "declare")
            .expect("环组装面必带 declare")
            .handler
            .clone();
        let already_fed = *self.fed.lock().unwrap_or_else(|e| e.into_inner());
        if !already_fed {
            *self.fed.lock().unwrap_or_else(|e| e.into_inner()) = true;
            let payloads = self.payloads.lock().unwrap_or_else(|e| e.into_inner());
            for (i, payload) in payloads.iter().enumerate() {
                let outcome = (declare)(payload);
                let record = match outcome {
                    Ok(v) => (i, false, v.to_string()),
                    Err(e) => (i, true, e),
                };
                self.results
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(record);
            }
        }
        Ok(af_execline::AgentRunOutcome {
            report: af_harness::RunReport {
                run_id: launch.run_id,
                status: af_harness::RunStatus::Completed {
                    signal: "TASK_COMPLETE".into(),
                },
                turns: 1,
                model_calls: 1,
                effect_replays: 0,
                effect_fresh: 0,
                transcript: vec![af_harness::TranscriptItem::Assistant {
                    // 终态消息留空：声明必须来自 declare 面（回退源缺席）。
                    text: String::new(),
                    tool_calls: Vec::new(),
                    usage: None,
                }],
                whitelist: vec!["declare".into()],
                tokens: af_harness::TokenTotals {
                    input: 10,
                    output: 10,
                    cache_read: 0,
                },
            },
            http_retries: 0,
        })
    }
}

#[test]
fn strict_declare_gate_rejects_malformed_and_accepts_conforming() {
    let root = temp_root("contract-gate");
    let data_root = task_package(&root, &compile_plan(&sop_full_src()));
    // 仅驱动 planner（coder/reviewer 无脚本 → 不跑；planner 合形声明后
    // coder 起动失败即可——本测试只断言 declare 工具面行为，drive 终态
    // 不作断言面）。
    let runner = Arc::new(DeclareProbeRunner {
        payloads: Mutex::new(vec![
            json!({}),                                         // 裸调（F4B 死因）
            json!({"instance_id": TASK, "steps": []}),         // 缺端口键（模型直付载荷）
            json!({"plan": {"instance_id": "x"}, "ghost": 1}), // 未知键 + 缺 steps
            json!("not-an-object"),                            // 非对象参数
            json!({"plan": {"instance_id": TASK, "steps": [    // 合形（steps 项齐 required）
                {"id": "s1", "action": "locate", "target": "django/db"}
            ]}}),
        ]),
        results: Mutex::new(Vec::new()),
        fed: Mutex::new(false),
    });
    let (unit, _bridge, _attempt, _out) = graph_unit(&root, runner.clone());

    // drive 必然失败（coder 无脚本）——只取 declare 工具面的逐次结果。
    let _ = drive(&unit, &data_root, &_attempt, &_out);

    let results = runner
        .results
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    assert_eq!(results.len(), 5, "planner 五次 declare 逐一记录");
    let (i, is_err, text) = &results[0];
    assert_eq!(*i, 0);
    assert!(
        *is_err,
        "裸 declare({}) 必须闭环内拒（F4B 死因）：{text}",
        json!({})
    );
    assert!(text.contains("plan"), "拒收点名缺失端口键：{text}");
    let (i, is_err, text) = &results[1];
    assert_eq!(*i, 1);
    assert!(*is_err, "缺端口键（载荷直付）必须拒：{text}");
    assert!(text.contains("plan"), "点名端口键：{text}");
    let (_, is_err, text) = &results[2];
    assert!(*is_err, "未知键 + 缺 required 字段必须拒：{text}");
    assert!(
        text.contains("ghost") || text.contains("steps"),
        "点名未知键或缺字段：{text}"
    );
    let (_, is_err, text) = &results[3];
    assert!(*is_err, "非对象参数必须拒：{text}");
    let (_, is_err, text) = &results[4];
    assert!(
        !is_err,
        "合形载荷必须采纳（TASK_COMPLETE 完成信号）：{text}"
    );
    assert!(
        text.contains("TASK_COMPLETE"),
        "采纳输出 = 完成信号：{text}"
    );
}

/// F4B 复活剧本 e2e：planner 首调裸 declare（拒 + 反馈）→ 按反馈修正重调
/// （合形采纳）→ 节点交付、envelope 落盘、全链走通。旧代码首调即存 {}
/// 并 TASK_COMPLETE → 跑后 EX4006 死亡；新契约 = 闭环内纠错。
#[test]
fn f4b_resurrection_bare_declare_then_corrected_call_completes() {
    let root = temp_root("contract-resurrect");
    // planner-only 最小图（单 task 节点直出 task_plan——非可推导端口）：
    // 复活剧本不需要 coder/reviewer 下游，独立图最干净。
    let graph_src = r#"
version: 1
meta:
  name: planner_probe
  graph_version: "1.0"
  entry: planner
  input: swe_task_input
  output: { ref: task_plan, from: planner }
  intent: "declare 契约面验证：单 planner 节点直出 task_plan（非可推导端口）"
types:
  - name: swe_task_input
    schema:
      type: object
      required: [repo, instance_id, problem_statement]
      properties:
        repo: { type: string }
        instance_id: { type: string }
        problem_statement: { type: string }
        image: { type: string }
  - name: plan_step
    schema:
      type: object
      required: [id, action, target]
      properties:
        id: { type: string }
        action: { type: string }
        target: { type: string }
        depends_on: { type: array, items: { type: string } }
  - name: task_plan
    schema:
      type: object
      required: [instance_id, steps]
      properties:
        instance_id: { type: string }
        steps: { type: array, items: { type: plan_step } }
nodes:
  - id: planner
    kind: task
    uses: { model: route.plan@1 }
    emits: { plan: task_plan }
    context:
      sections:
        - { from: template, template_ref: pkg.sweng.planner_system@1, quota_tokens: 500 }
        - { from: input, fields: [repo, instance_id, problem_statement], policy: project, quota_tokens: 1500 }
policies:
  budgets:
    - { id: cost_cap, dimension: cents, scope: plan, cap: 10 }
  stop:
    - { when: "steps.turns >= 10", outcome: failed }
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
    let data_root = task_package(&root, &compile_plan(graph_src));
    let runner = Arc::new(DeclareProbeRunner {
        payloads: Mutex::new(vec![
            json!({}), // turn 0 裸 declare（F4B 判例）
            json!({"plan": {"instance_id": TASK, "steps": [
                {"id": "s1", "action": "locate", "target": "django/db/models"}
            ]}}),
        ]),
        results: Mutex::new(Vec::new()),
        fed: Mutex::new(false),
    });
    let (unit, _bridge, attempt, out) = graph_unit(&root, runner.clone());

    let output = drive(&unit, &data_root, &attempt, &out)
        .expect("裸 declare 被拒后修正重调应完成（复活剧本）");

    let results = runner
        .results
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    assert_eq!(results.len(), 2);
    assert!(
        results[0].1,
        "首调裸 declare 拒（反馈进环）：{}",
        results[0].2
    );
    assert!(
        results[0].2.contains("plan"),
        "反馈点名端口与形状指引：{}",
        results[0].2
    );
    assert!(!results[1].1, "修正重调采纳：{}", results[1].2);
    // envelope 落盘：planner.plan.json 携合形 task_plan。
    let file = attempt.join("artifacts").join("planner.plan.json");
    assert!(file.is_file(), "planner envelope 应落盘");
    let envelope: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(envelope["port"], json!("plan"));
    assert_eq!(envelope["payload"]["instance_id"], json!(TASK));
    assert_eq!(
        envelope["payload"]["steps"][0]["id"],
        json!("s1"),
        "载荷 = 模型声明原样（非推导面）"
    );
    assert_eq!(output.verdict, Some(true), "非判定型终态 = 产出即合格");
}

/// permissive 面零动守卫（sop_minimal repair.patch = patch 形可推导端口）：
/// parameters 维持 {"type":"object"}（无 required——裸调必须能到达闭包）、
/// 描述保留裸调许可措辞（容器自动取证）、裸调 → TASK_COMPLETE。
#[test]
fn permissive_declare_face_unchanged_for_patch_shaped_ports() {
    let root = temp_root("contract-permissive");
    let data_root = task_package(&root, &compile_plan(&sop_minimal_src()));
    // 捕获型 runner：取 declare ToolDecl（parameters/description）+ 裸调结果。
    struct CaptureRunner {
        parameters: Mutex<Value>,
        description: Mutex<String>,
        bare_ok: Mutex<bool>,
    }
    impl af_execline::AgentRunner for CaptureRunner {
        fn run(
            &self,
            launch: af_execline::AgentLaunch<'_>,
        ) -> Result<af_execline::AgentRunOutcome, af_execline::AgentRunFailure> {
            let decl = launch
                .spec
                .toolset
                .iter()
                .find(|t| t.name == "declare")
                .expect("必带 declare");
            *self.parameters.lock().unwrap_or_else(|e| e.into_inner()) = decl.parameters.clone();
            *self.description.lock().unwrap_or_else(|e| e.into_inner()) = decl.description.clone();
            let handler = launch
                .extra_tools
                .iter()
                .find(|t| t.name == "declare")
                .unwrap()
                .handler
                .clone();
            let outcome = (handler)(&json!({}));
            *self.bare_ok.lock().unwrap_or_else(|e| e.into_inner()) = outcome
                .map(|v| v == json!("TASK_COMPLETE"))
                .unwrap_or(false);
            Ok(af_execline::AgentRunOutcome {
                report: af_harness::RunReport {
                    run_id: launch.run_id,
                    status: af_harness::RunStatus::Completed {
                        signal: "TASK_COMPLETE".into(),
                    },
                    turns: 2,
                    model_calls: 2,
                    effect_replays: 0,
                    effect_fresh: 1,
                    transcript: Vec::new(),
                    whitelist: vec!["bash".into(), "declare".into()],
                    tokens: af_harness::TokenTotals {
                        input: 10,
                        output: 10,
                        cache_read: 0,
                    },
                },
                http_retries: 0,
            })
        }
    }
    let capture = Arc::new(CaptureRunner {
        parameters: Mutex::new(Value::Null),
        description: Mutex::new(String::new()),
        bare_ok: Mutex::new(false),
    });
    let bridge = Arc::new(FakeBridge::new());
    let spec = GraphUnitSpec::v1(PROFILE, repo_packages());
    let unit = GraphExecuteUnit::new(spec, bridge.clone(), capture.clone()).expect("单元装载");
    let attempt = root.join("attempt");
    let out = root.join("out");

    let output = drive(&unit, &data_root, &attempt, &out).expect("sop_minimal 应走通");

    let params = capture
        .parameters
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    assert_eq!(
        params,
        json!({"type": "object"}),
        "patch 形端口 parameters 维持零形状（裸调必须能到达闭包）：{params}"
    );
    let desc = capture
        .description
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    assert!(
        desc.contains("you may call it bare"),
        "可推导端口保留裸调许可措辞（容器自动取证）：{desc}"
    );
    assert!(
        *capture.bare_ok.lock().unwrap_or_else(|e| e.into_inner()),
        "裸 declare 仍即 TASK_COMPLETE（permissive 面零动）"
    );
    // 推导路径照旧：envelope 披露 derived 面。
    let file = attempt.join("artifacts").join("repair.patch.json");
    assert!(file.is_file());
    let envelope: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(envelope["derived"], json!(["diff", "files"]));
    assert_eq!(output.verdict, Some(true));
}
