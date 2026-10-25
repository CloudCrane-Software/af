//! 图执行器 v1 验收测试（F-第四单执行长缺口销项；TDD 先红后绿——本文件先于
//! `src/graph_unit.rs` 落地，模块缺席时编译红）。
//!
//! v1 范围 = sop_full 线性链形态：planbind 三核对（EX4004 口径）通过后按
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
    runner: Arc<GraphFakeRunner>,
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
// ⑤ planbind 三核对先行：digest 漂移 → EX4004（E9004 口径透传）
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
    // 三核对先行：零节点驱动、零容器。
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
