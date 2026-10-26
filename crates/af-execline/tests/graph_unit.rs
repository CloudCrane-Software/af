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
//! 计划经真编译器全链产出（load→validate→plan_json，sop_full@1.1.afb 仓库
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
            .join("sop_full@1.1.afb"),
    )
    .expect("sop_full@1.1.afb 仓库正本应在盘")
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
    /// HTTP 层退避重试回数（红队 W2-C33：供给面重试审计面）。
    http_retries: u64,
}

/// 走 declare 工具面的节点集。
const DECLARE_NODES: &[&str] = &["planner", "coder"];

impl GraphFakeRunner {
    fn new(scripted: fn(&str) -> Option<String>) -> Self {
        Self {
            launches: Mutex::new(Vec::new()),
            scripted,
            http_retries: 0,
        }
    }

    /// 非零 http_retries（W2-C33 测试缝）。
    fn with_http_retries(scripted: fn(&str) -> Option<String>, retries: u64) -> Self {
        Self {
            launches: Mutex::new(Vec::new()),
            scripted,
            http_retries: retries,
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
            http_retries: self.http_retries,
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
        graph_version: None,
        task_sha256: None,
        plan_sha256: None,
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
    // task.json 声明的 test_command，零模型调用；红队 W2-C48 起判据命令须
    // 随任务包冻结（缺省 pytest 面废除），W2-C49 起判据留档在 tester 臂
    // 另取一次工作区 diff（与公共段「3 task 节点 + 1 判据面」四次取证同源
    // 计数）。
    let exec_calls: Vec<String> = bridge
        .calls()
        .into_iter()
        .filter(|s| s.contains("docker exec") && s.contains("pytest"))
        .collect();
    assert_eq!(exec_calls.len(), 1, "tester 恰一次判据执行: {exec_calls:?}");
    assert!(
        exec_calls[0].contains("python -m pytest"),
        "test_command 来自任务包: {}",
        exec_calls[0]
    );
    // 红队 W2-C48：三 task 节点收尾取证 + tester 判据面取证，各恰一次
    // `git diff` 过桥（keyed-declare/终态消息两路同样取证）。
    let diff_calls = bridge
        .calls()
        .iter()
        .filter(|s| s.contains("git -c core.fileMode=false diff"))
        .count();
    assert_eq!(diff_calls, 4, "取证面：3 task 节点 + 1 判据面: {}", bridge.calls().len());

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

    // 红队 W2-C49：test_unit 判据留档——pass=true 也不许丢观察：tester 节点
    // 目录必须齐 test.log（完整观察文本）+ test-evidence.json（命令/退出码/
    // 判定/体量口径）+ agent.diff（工作区取证），envelope 带 evidence 指针，
    // 订单证据树自此可复核 pass 与外部评审结论并存的依据面。
    let tester_dir = attempt.join("nodes").join("tester");
    let test_log = std::fs::read_to_string(tester_dir.join("test.log"))
        .expect("W2-C49：pass=true 也须留 test.log");
    assert!(
        test_log.contains("Ran 12 tests") && test_log.contains("exit=0"),
        "test.log 须为完整观察文本（含 exit= 尾行）: {test_log:?}"
    );
    let evidence_json: Value = serde_json::from_str(
        &std::fs::read_to_string(tester_dir.join("test-evidence.json"))
            .expect("W2-C49：pass=true 也须留 test-evidence.json"),
    )
    .unwrap();
    assert_eq!(evidence_json["pass"], json!(true));
    assert_eq!(evidence_json["exit_code"], json!(0));
    assert!(
        evidence_json["command"].as_str().unwrap_or("").contains("python -m pytest"),
        "evidence 记命令: {evidence_json}"
    );
    let diff_text = std::fs::read_to_string(tester_dir.join("agent.diff"))
        .expect("W2-C49：tester 节点目录须留 agent.diff");
    assert!(diff_text.contains("diff --git"), "agent.diff 内容: {diff_text:?}");
    assert_eq!(
        report["payload"]["evidence"]["log"], json!("test.log"),
        "envelope 带 evidence 指针"
    );
    assert!(
        report["payload"]["evidence"]["diff"]["sha256"].is_string(),
        "envelope evidence 带 diff 指纹: {}",
        report["payload"]["evidence"]
    );

    // 终态：qa_report.pass=true → verdict + 合格终态工件（is_final=report）。
    assert_eq!(output.verdict, Some(true));
    assert_eq!(output.artifacts.len(), 1);
    assert!(output.artifacts[0].is_final && output.artifacts[0].qualified);

    // v1 行：records.jsonl 恰一行，按 af.plan.record.v1 契约键投影（红队
    // W1-C36：图行与 swev/report 同一契约面——node_id/item_id/attempt/
    // host_alias/status/evidence.run_dir/gate/usage.quota/wall_ms/note 齐，
    // 无 suite/order_id/nodes/cents_used 契约外键）。
    let rows: Vec<Value> = std::fs::read_to_string(out.join("records.jsonl"))
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert!(row["plan_id"]
        .as_str()
        .unwrap()
        .starts_with("plan.sweng_sop_full@1.1+"));
    for key in [
        "plan_id",
        "node_id",
        "item_id",
        "attempt",
        "host_alias",
        "unit_id",
        "unit_version",
        "status",
        "verdict",
        "gate",
        "wall_ms",
        "note",
    ] {
        assert!(row.get(key).is_some(), "v1 契约键 {key} 缺席：{row}");
    }
    assert!(!row.is_null());
    // 契约外键不得混入。
    for key in ["suite", "order_id", "nodes", "cents_used", "tokens_used"] {
        assert!(row.get(key).is_none(), "契约外键 {key} 不得混入：{row}");
    }
    // evidence.run_dir 指向实际 run 目录（trace 工具 L1→L2 父子边）。
    let run_dir = row["evidence"]["run_dir"].as_str().unwrap();
    assert!(
        Path::new(run_dir).is_dir(),
        "evidence.run_dir 须指向实际 run 目录：{run_dir}"
    );
    // usage.quota 三分账在案（FakeRunner 每节点 in=100/out=50/cache=10 ×3）。
    assert_eq!(row["usage"]["quota"]["input"], json!(300));
    assert_eq!(row["usage"]["quota"]["output"], json!(150));
    assert_eq!(row["usage"]["quota"]["cache_read"], json!(30));
    assert_eq!(row["item_id"], json!(TASK));

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
        "{ id: token_cap, dimension: tokens_total, scope: plan, cap: 1000000 }",
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
    let _ = _bridge;

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

/// 红队 W2-C29：aci_schema 载荷必须按 consumes 契约取 agent_action 端口，
/// 禁 `inputs.values().next()`——inputs 是 BTreeMap，values().next()=端口名
/// 字典序最小者。本图给 verifier 两条入边：agent_action 端口声明在前、
/// 字典序在后（target > probe），诱饵工件恰含 thought/command/submit 三字段
/// （结构上过得了 aci_schema 校验）——旧码会把结构校验跑在字典序最小的诱饵
/// 上并输出其 submit，真 action 漏校。
#[test]
fn aci_schema_picks_contract_port_not_lexicographic_first() {
    let graph_src = r#"
version: 1
meta:
  name: sweng_aci_port_contract
  graph_version: "1.0"
  entry: actor
  input: swe_task_input
  output: { ref: schema_verdict, from: verifier }
  intent: "W2-C29：aci_schema 多入边按 consumes 契约取 agent_action，不取字典序首"

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
  - name: decoy_payload
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

nodes:
  - id: actor
    kind: task
    uses: { model: route.plan@1 }
    emits: { target: agent_action }
    context:
      sections:
        - { from: template, template_ref: pkg.sweng.planner_system@1, quota_tokens: 400 }

  - id: observer
    kind: task
    uses: { model: route.plan@1 }
    emits: { probe: decoy_payload }
    context:
      sections:
        - { from: template, template_ref: pkg.sweng.planner_system@1, quota_tokens: 400 }

  - id: verifier
    kind: check
    uses: { unit: pkg.sweagent.aci_schema@1 }
    consumes: { target: agent_action, probe: decoy_payload }
    emits: { verdict: schema_verdict }

edges:
  - { from: actor, output: target, to: verifier, input: target }
  - { from: observer, output: probe, to: verifier, input: probe }

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
    let root = temp_root("aci_port_contract");
    let data_root = task_package(&root, &compile_plan(graph_src));
    let scripted = |node: &str| -> Option<String> {
        let text = match node {
            // 真 action（submit=true）：合同端口 target 的载荷。
            "actor" => json!({"target": {
                "thought": "run the tests", "command": "python -m pytest -q", "submit": true
            }}),
            // 诱饵（submit=false，thought 打标）：恰含三字段、结构过得 aci_schema。
            "observer" => json!({"probe": {
                "thought": "DECOY not an action", "command": "echo decoy", "submit": false
            }}),
            _ => return None,
        };
        Some(text.to_string())
    };
    let runner = Arc::new(GraphFakeRunner::new(scripted));
    let (unit, _bridge, attempt, out) = graph_unit(&root, runner.clone());
    let _ = _bridge;

    let _ = drive(&unit, &data_root, &attempt, &out).expect("双入边 aci_schema 链应走通");

    let verdict: Value = serde_json::from_str(
        &std::fs::read_to_string(attempt.join("artifacts").join("verifier.verdict.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        verdict["payload"]["error"],
        json!(""),
        "真 agent_action 过结构校验"
    );
    assert_eq!(
        verdict["payload"]["submit"],
        json!(true),
        "aci_schema 必须校验 consumes 契约声明的 agent_action 端口（target），\
         不是字典序最小的诱饵端口（probe——旧 values().next() 会输出其 submit=false）"
    );
}

/// sweng_g4_sweeval 图源（actor(task) → evaluator(check: swebench_eval)）：
/// fail-loud 用例与 W2-C51 判分锚在场路由用例共用。
fn sweeval_graph_src() -> String {
    r#"
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
"#
    .to_string()
}

/// swebench_eval 分派面 fail-loud：spec.judge 未配置（None）时拒载点名
/// 「判分锚」——不静默跳判（B 席嫁接口径）。
#[test]
fn swebench_eval_without_judge_fails_loud() {
    let graph_src = sweeval_graph_src();
    let root = temp_root("g4sweeval");
    let data_root = task_package(&root, &compile_plan(&graph_src));
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
    let _ = _bridge;

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

// ============================================================
// 红队修复批（W1）：C-31/C-32/C-34/C-37/C-38
// ============================================================

/// 带 graph_version 期待的驱动（红队 W1-C32 测试缝）。
fn drive_with_gv(
    unit: &GraphExecuteUnit,
    data_root: &Path,
    attempt: &Path,
    out: &Path,
    graph_version: Option<&str>,
) -> Result<af_execline::ExecOutput, af_execline::ExeclineError> {
    std::fs::create_dir_all(attempt).unwrap();
    std::fs::create_dir_all(out).unwrap();
    let sink = FileRecordSink::open(out).unwrap();
    let ctx = ExecContext {
        order_id: "wo-graph-w1",
        stable_id: "sid-graph-w1",
        line_id: "line-A",
        out_dir: out,
        task_id: TASK,
        task_sha256: None,
        plan_sha256: None,
        host_alias: "local",
        data_root,
        attempt_dir: attempt,
        record_sink: &sink,
        trial: false,
        graph_version,
    };
    unit.execute(&ctx)
}

/// C-31 测试 Runner：planner/coder 走 declare 面（声明 = 端口键控全键，
/// 含无类型 shadow 端口）；reviewer 走终态消息 JSON（回退源双源覆盖）。
struct UntypedPortRunner {
    planner_declare_params: Mutex<Value>,
}

impl af_execline::AgentRunner for UntypedPortRunner {
    fn run(
        &self,
        launch: af_execline::AgentLaunch<'_>,
    ) -> Result<af_execline::AgentRunOutcome, af_execline::AgentRunFailure> {
        let node = launch
            .run_id
            .strip_prefix(&format!("{TASK}-"))
            .unwrap_or_default()
            .to_string();
        let (final_text, via_declare) = match node.as_str() {
            "planner" => (
                json!({
                    "plan": {"instance_id": TASK, "steps": [
                        {"id": "s1", "action": "locate", "target": "django/db/models"}
                    ]},
                    "shadow": {"note": "untyped port payload"}
                })
                .to_string(),
                true,
            ),
            "coder" => (
                json!({"files": [
                    {"path": "django/db/models/foo.py", "content_ref": "workspace#1"}
                ]})
                .to_string(),
                true,
            ),
            "reviewer" => (
                json!({"review": {"approved": true, "issues": []}}).to_string(),
                false,
            ),
            other => {
                return Err(af_execline::AgentRunFailure {
                    message: format!("UntypedPortRunner 无节点 {other} 脚本"),
                    http_retries: 0,
                })
            }
        };
        if via_declare {
            if node == "planner" {
                *self
                    .planner_declare_params
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = launch
                    .spec
                    .toolset
                    .iter()
                    .find(|t| t.name == "declare")
                    .map(|t| t.parameters.clone())
                    .unwrap_or(Value::Null);
            }
            let binding = launch
                .extra_tools
                .iter()
                .find(|t| t.name == "declare")
                .expect("declare 面必在");
            let args: Value = serde_json::from_str(&final_text).unwrap();
            (binding.handler)(&args).expect("全键声明应被 strict 闭包采纳");
        }
        Ok(af_execline::AgentRunOutcome {
            report: af_harness::RunReport {
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
                        text: final_text.clone(),
                        tool_calls: Vec::new(),
                        usage: None,
                    },
                ],
                whitelist: Vec::new(),
                tokens: af_harness::TokenTotals {
                    input: 100,
                    output: 50,
                    cache_read: 10,
                },
            },
            http_retries: 0,
        })
    }
}

/// C-31：多端口 task 节点带「无 r#type 且非 is_final」端口——strict declare
/// 的 required/accepted 键集 = 全部 artifact_contract 端口（无类型端口仅进
/// required 不给 schema）；declare 交全键 → commit 相 EX4006 不再致死。
/// 此前两相键集互斥：只交类型端口 → commit EX4006 全灭；补交无类型端口 →
/// 闭包「未知键」拒收——重试环无出口（系统性死锁，红队 W1-C31）。
#[test]
fn strict_declare_covers_untyped_nonfinal_ports_end_to_end() {
    let root = temp_root("w1-c31");
    let mut plan: Value = serde_json::from_str(&compile_plan(&sop_full_src())).unwrap();
    let planner = plan["body"]["node_exec"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|n| n["node_id"] == json!("planner"))
        .unwrap();
    planner["artifact_contract"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "shadow", "is_final": false}));
    let data_root = task_package(&root, &serde_json::to_string(&plan).unwrap());
    let runner = Arc::new(UntypedPortRunner {
        planner_declare_params: Mutex::new(Value::Null),
    });
    let (unit, _bridge, attempt, out) = graph_unit(&root, runner.clone());
    let _ = _bridge;

    let output = drive(&unit, &data_root, &attempt, &out)
        .expect("无类型非终态端口不得致死（required=全部端口 + 无类型不核形）");
    assert_eq!(output.verdict, Some(true), "终态 qa_report.pass=true");

    // API 面：required 含全键；properties 只含有类型端口（无类型端口不给
    // schema——形状留白，键到位即可）。
    let params = runner
        .planner_declare_params
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let required = params["required"].as_array().unwrap().clone();
    assert!(
        required.contains(&json!("plan")),
        "required 应含类型端口：{params}"
    );
    assert!(
        required.contains(&json!("shadow")),
        "required 应含无类型端口（W1-C31）：{params}"
    );
    assert!(
        params["properties"].get("plan").is_some(),
        "有类型端口应给 schema：{params}"
    );
    assert!(
        params["properties"].get("shadow").is_none(),
        "无类型端口不给 schema：{params}"
    );
}

/// C-32：WorkOrder 一次性绑定的 graph_version 作装订外部期待——与计划
/// compiled_from.graph_version 不符即拒（此前期待分支恒空 = 死代码，换
/// graph_version 漂移的计划照样装订执行）。
#[test]
fn graph_version_expectation_mismatch_rejects_binding() {
    let root = temp_root("w1-c32");
    let data_root = task_package(&root, &compile_plan(&sop_full_src()));
    let runner = Arc::new(GraphFakeRunner::new(sop_full_payloads));
    let (unit, bridge, attempt, out) = graph_unit(&root, runner.clone());

    // 期待漂移 → EX4004 口径装订拒绝，零节点驱动、零容器。
    let err = drive_with_gv(&unit, &data_root, &attempt, &out, Some("9.9"))
        .expect_err("graph_version 漂移应拒装订");
    assert_eq!(err.code(), ErrorCode::PlanBindMismatch, "{err}");
    assert!(err.to_string().contains("graph_version"), "{err}");
    assert!(runner.launches().is_empty());
    assert!(bridge.calls().is_empty(), "装订失败不起容器");

    // 期待一致（sop_full 图版本 1.1）→ 正常走通。
    drive_with_gv(&unit, &data_root, &attempt, &out, Some("1.1")).expect("期待一致应走通");
}

/// C-34：预算面收口——强制面外的 dimension/scope 组合 fail-loud 拒收
/// （此前 plan 级五维与节点级非 turns 条目被静默忽略 = 声明帽得到无预算
/// 运行，与本函数正典 id 门的 fail-loud 纪律自相矛盾）。
#[test]
fn unsupported_budget_dimension_scope_combos_rejected() {
    for (tag, entry) in [
        (
            "plan-clock",
            json!({"id": "wall", "dimension": "clock_ms", "scope": "plan", "cap": 1000}),
        ),
        (
            "plan-modelcalls",
            json!({"id": "calls", "dimension": "model_calls", "scope": "plan", "cap": 10}),
        ),
        (
            "node-cents",
            json!({"id": "nc", "dimension": "cents", "scope": "node:coder", "cap": 100}),
        ),
        (
            "loop-turns",
            json!({"id": "lt", "dimension": "turns", "scope": "loop:study", "cap": 10}),
        ),
        (
            "dangling-node",
            json!({"id": "dt", "dimension": "turns", "scope": "node:ghost", "cap": 10}),
        ),
    ] {
        let root = temp_root(&format!("w1-c34-{tag}"));
        let mut plan: Value = serde_json::from_str(&compile_plan(&sop_full_src())).unwrap();
        plan["body"]["budget"]["budgets"]
            .as_array_mut()
            .unwrap()
            .push(entry.clone());
        let data_root = task_package(&root, &serde_json::to_string(&plan).unwrap());
        let runner = Arc::new(GraphFakeRunner::new(sop_full_payloads));
        let (unit, _bridge, attempt, out) = graph_unit(&root, runner.clone());
        let err = drive(&unit, &data_root, &attempt, &out)
            .expect_err(&format!("{tag} 应 fail-loud 拒收"));
        assert_eq!(err.code(), ErrorCode::PlanBindMismatch, "{tag}: {err}");
        assert!(
            err.to_string().contains("强制集") || err.to_string().contains("强制面"),
            "{tag}: {err}"
        );
    }
}

/// C-37：同一 (to, input) 端口被多条边命中 = 编排真相二义——入边装配取
/// 后边、提示词装配取前边，载荷静默分叉；装订器先拒。**顺序对抗**：重影边
/// 排在原边之后（后边胜面）与之前（前边胜面）都必须拒——拒绝与边序无关，
/// 不给任何一序留「恰好装配到真边」的侥幸面。
#[test]
fn duplicate_to_input_edges_rejected_at_bind() {
    for (tag, dup_first) in [("dup-after", false), ("dup-before", true)] {
        let root = temp_root(&format!("w1-c37-{tag}"));
        let mut plan: Value = serde_json::from_str(&compile_plan(&sop_full_src())).unwrap();
        let edges = plan["body"]["graph"]["edges"].as_array().unwrap().clone();
        let first = edges
            .iter()
            .find(|e| e["to"] == json!("coder"))
            .expect("coder 入边在案")
            .clone();
        // 重影边来自不同上游（reviewer.review ≠ 原边载荷）——不同载荷 + 两种
        // 边序都只是排列差异，装订器一律先拒。
        let dup = json!({
            "from": "reviewer", "output": "review",
            "to": first["to"], "input": first["input"], "checkpoint": false
        });
        {
            let arr = plan["body"]["graph"]["edges"].as_array_mut().unwrap();
            if dup_first {
                arr.insert(0, dup);
            } else {
                arr.push(dup);
            }
        }
        let data_root = task_package(&root, &serde_json::to_string(&plan).unwrap());
        let runner = Arc::new(GraphFakeRunner::new(sop_full_payloads));
        let (unit, _bridge, attempt, out) = graph_unit(&root, runner.clone());
        let err = drive(&unit, &data_root, &attempt, &out)
            .expect_err(&format!("{tag}: 重复 (to,input) 边应拒装订"));
        assert_eq!(err.code(), ErrorCode::PlanBindMismatch, "{tag}: {err}");
        assert!(err.to_string().contains("边级重影"), "{tag}: {err}");
    }
}

/// C-38：节点 id / 工件端口名过路径安全核——含分隔符或纯点号段即拒
/// （手写 plan.json 不经 af-dsl 词法集，id 直接拼进节点目录与
/// {node_id}.{port}.json 工件名，可写出 attempt_dir 之外）。
#[test]
fn path_unsafe_node_ids_and_ports_rejected_at_bind() {
    for (tag, mutate) in [
        (
            "slash-id",
            Box::new(|plan: &mut Value| {
                for collection in ["nodes"] {
                    for n in plan["body"]["graph"][collection].as_array_mut().unwrap() {
                        if n["id"] == json!("planner") {
                            n["id"] = json!("pl/../../escape");
                        }
                    }
                }
                for n in plan["body"]["node_exec"].as_array_mut().unwrap() {
                    if n["node_id"] == json!("planner") {
                        n["node_id"] = json!("pl/../../escape");
                    }
                }
            }) as Box<dyn Fn(&mut Value)>,
        ),
        (
            "dotdot-id",
            Box::new(|plan: &mut Value| {
                for n in plan["body"]["graph"]["nodes"].as_array_mut().unwrap() {
                    if n["id"] == json!("planner") {
                        n["id"] = json!("..");
                    }
                }
                for n in plan["body"]["node_exec"].as_array_mut().unwrap() {
                    if n["node_id"] == json!("planner") {
                        n["node_id"] = json!("..");
                    }
                }
            }) as Box<dyn Fn(&mut Value)>,
        ),
        (
            "slash-port",
            Box::new(|plan: &mut Value| {
                for n in plan["body"]["node_exec"].as_array_mut().unwrap() {
                    if n["node_id"] == json!("planner") {
                        n["artifact_contract"][0]["name"] = json!("pl/an");
                    }
                }
            }) as Box<dyn Fn(&mut Value)>,
        ),
    ] {
        let root = temp_root(&format!("w1-c38-{tag}"));
        let mut plan: Value = serde_json::from_str(&compile_plan(&sop_full_src())).unwrap();
        mutate(&mut plan);
        let data_root = task_package(&root, &serde_json::to_string(&plan).unwrap());
        let runner = Arc::new(GraphFakeRunner::new(sop_full_payloads));
        let (unit, _bridge, attempt, out) = graph_unit(&root, runner.clone());
        let err = drive(&unit, &data_root, &attempt, &out).expect_err(&format!("{tag} 应拒"));
        assert_eq!(err.code(), ErrorCode::PlanBindMismatch, "{tag}: {err}");
        assert!(
            err.to_string().contains("路径分隔符") || err.to_string().contains("纯点号段"),
            "{tag}: {err}"
        );
    }
}

/// C-59①：任务包判据面摘要钉死——门面 submit 期待的 sha256 与盘上
/// task.json 不符即拒（判分真值源被换，清空判据即可绕判分的面收口）。
#[test]
fn task_digest_mismatch_rejects_execution() {
    let root = temp_root("w1-c59");
    let data_root = task_package(&root, &compile_plan(&sop_full_src()));
    let runner = Arc::new(GraphFakeRunner::new(sop_full_payloads));
    let (unit, bridge, attempt, out) = graph_unit(&root, runner.clone());
    std::fs::create_dir_all(&attempt).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    let bad = "0".repeat(64);
    let err = {
        let sink_b = FileRecordSink::open(&out).unwrap();
        let ctx_b = ExecContext {
            order_id: "wo-graph-w1c59",
            stable_id: "sid-graph-w1c59",
            line_id: "line-A",
            out_dir: &out,
            task_id: TASK,
            host_alias: "local",
            data_root: &data_root,
            attempt_dir: &attempt,
            record_sink: &sink_b,
            trial: false,
            graph_version: None,
            task_sha256: Some(bad.as_str()),
            plan_sha256: None,
        };
        unit.execute(&ctx_b).expect_err("判据面摘要漂移应拒")
    };
    assert!(err.to_string().contains("判据面摘要不符"), "{err}");
    assert!(runner.launches().is_empty(), "拒于执行前");
    assert!(bridge.calls().is_empty(), "拒于起容器");

    // 匹配摘要（盘上 task.json 实算）→ 正常走通。
    let real = af_execline::file_sha256_hex(&data_root.join(TASK).join("task.json")).unwrap();
    let output = {
        std::fs::remove_dir_all(&out).ok();
        std::fs::create_dir_all(&out).unwrap();
        let sink2 = FileRecordSink::open(&out).unwrap();
        let ctx2 = ExecContext {
            order_id: "wo-graph-w1c59",
            stable_id: "sid-graph-w1c59",
            line_id: "line-A",
            out_dir: &out,
            task_id: TASK,
            host_alias: "local",
            data_root: &data_root,
            attempt_dir: &attempt,
            record_sink: &sink2,
            trial: false,
            graph_version: None,
            task_sha256: Some(real.as_str()),
            plan_sha256: None,
        };
        unit.execute(&ctx2)
    };
    output.expect("摘要匹配应走通");
}

/// 探针任务包（W2：单节点图 input=in_t，required q——task.json 须满足）。
fn probe_task_package(root: &Path, plan_src: &str) -> PathBuf {
    let data_root = root.join("data");
    let task_dir = data_root.join(TASK);
    std::fs::create_dir_all(&task_dir).unwrap();
    std::fs::write(task_dir.join("plan.json"), plan_src).unwrap();
    // in_t schema 要 q；执行器容器 setup 另要 instance_id/image（W2-C39 归属
    // 文案：非 schema 必填项）——两者齐备才能过图输入双门。
    std::fs::write(
        task_dir.join("task.json"),
        json!({"q": "x", "repo": "django/django", "instance_id": TASK,
               "problem_statement": "probe", "image": "swebench/sweb.eval.x86_64.django-11099:latest"})
            .to_string(),
    )
    .unwrap();
    data_root
}

// ============================================================
// 红队修复批（W2）：终态判定字段 / 双相熔断 / 供给面重试记账
// ============================================================

/// 单 task 节点终态图源（W2-C29/C-31 探针）：worker 发射 result: verdict_t
/// （verdict_t 声明布尔 `resolved`——swebench 判分口径，非 pass）。
fn single_node_graph_src(budgets: &str) -> String {
    format!(
        r#"version: 1
meta:
  name: w2probe
  graph_version: "1.0"
  entry: worker
  input: in_t
  output: {{ ref: verdict_t, from: worker }}
types:
  - {{ name: in_t, schema: {{ type: object, required: [q], properties: {{ q: {{ type: string }} }} }} }}
  - {{ name: verdict_t, schema: {{ type: object, required: [resolved], properties: {{ resolved: {{ type: boolean }} }} }} }}
nodes:
  - id: worker
    kind: task
    uses: {{ model: route.main@1 }}
    emits: {{ result: verdict_t }}
edges: []
policies:
{budgets}"#
    )
}

/// 终态消息按端口名包裹（envelope 契约 = 端口键）：{"result": {"resolved": …}}。
fn payload_resolved_false(node: &str) -> Option<String> {
    match node {
        "worker" => Some(json!({"result": {"resolved": false}}).to_string()),
        _ => None,
    }
}

fn payload_resolved_true(node: &str) -> Option<String> {
    match node {
        "worker" => Some(json!({"result": {"resolved": true}}).to_string()),
        _ => None,
    }
}

#[test]
fn terminal_verdict_reads_declared_resolved_field_not_pass_default() {
    // 红队 W2-C29：终态类型声明 `resolved`（swebench 判分口径）时判据取
    // resolved——此前硬编码 get("pass").or(Some(true))，无 pass 的失败判分
    // （resolved=false）被缺字段推定成功、合格工件 close。
    for (payload, expect) in [
        (
            payload_resolved_false as fn(&str) -> Option<String>,
            Some(false),
        ),
        (
            payload_resolved_true as fn(&str) -> Option<String>,
            Some(true),
        ),
    ] {
        let root = temp_root("w2-verdict");
        let src = single_node_graph_src("  budgets: []\n  stop: []\n  failures: []\n");
        let data_root = probe_task_package(&root, &compile_plan(&src));
        let runner = Arc::new(GraphFakeRunner::new(payload));
        let (unit, _bridge, attempt, out) = graph_unit(&root, runner.clone());
        let output = drive(&unit, &data_root, &attempt, &out).expect("单节点链应走通");
        assert_eq!(
            output.verdict, expect,
            "终态判据须取声明的 resolved 字段（非 pass 缺省推定）"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[test]
fn image_missing_error_attributes_executor_setup_not_schema_gate() {
    // 红队 W1-C39 归属钉：schema 门与解析门各归其位——input schema 不要求
    // image（in_t required=[q]，validate_schema 放行），执行器容器 setup 仍
    // 硬性必填。缺 image 的拒绝必须如实归属执行器自加要求（「执行器容器
    // setup 硬性必填」「非 swe_task_input schema 必填项」），不得再说成
    // schema 面要求（W1-C39 前的旧文案「（swe_task_input 面）」）；解析门
    // 前零 launch、零容器调用。
    let root = temp_root("w1-c39");
    let src = single_node_graph_src("  budgets: []\n  stop: []\n  failures: []\n");
    let data_root = probe_task_package(&root, &compile_plan(&src));
    // 抹去 image：schema 门照放行，解析门按容器 setup 要求拒。
    let task_path = data_root.join(TASK).join("task.json");
    let mut task: Value =
        serde_json::from_str(&std::fs::read_to_string(&task_path).unwrap()).unwrap();
    task.as_object_mut().unwrap().remove("image");
    std::fs::write(&task_path, task.to_string()).unwrap();

    let runner = Arc::new(GraphFakeRunner::new(payload_resolved_true));
    let (unit, bridge, attempt, out) = graph_unit(&root, runner.clone());
    let err = drive(&unit, &data_root, &attempt, &out)
        .expect_err("schema 门放行的缺 image 输入应由解析门如实拒收");
    assert_eq!(err.code(), ErrorCode::UnitFailed, "{err}");
    assert!(err.to_string().starts_with("EX3001:"), "{err}");
    let msg = err.to_string();
    assert!(
        msg.contains("执行器容器 setup 硬性必填"),
        "归属须为执行器容器 setup：{msg}"
    );
    assert!(
        msg.contains("非 swe_task_input schema 必填项"),
        "须声明 schema 非穷尽边界（image 非 schema 必填项）：{msg}"
    );
    assert!(
        !msg.contains("（swe_task_input 面）"),
        "不得把缺 image 归为 schema 面要求（W1-C39 前的旧文案）：{msg}"
    );
    assert!(runner.launches().is_empty(), "解析门前零模型 launch");
    assert!(bridge.calls().is_empty(), "解析门前零容器调用");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn terminal_token_overshoot_circuit_breaks_before_completion() {
    // 红队 W2-C31：双相熔断第二相——终态节点跑完后对账，超 tokens_total 帽
    // 不得完工（此前只在起动前 gate，终态超帽照常 check_typed/判定/落行）。
    // FakeRunner 每节点 tokens=160（100+50+10）；帽 100 → 起动前 0<100 过、
    // 完成后 160≥100 熔断。
    let root = temp_root("w2-tokgate");
    let src = single_node_graph_src(
        "  budgets:\n    - { id: token_cap, dimension: tokens_total, scope: plan, cap: 100 }\n  stop: []\n  failures: []\n",
    );
    let data_root = probe_task_package(&root, &compile_plan(&src));
    let runner = Arc::new(GraphFakeRunner::new(payload_resolved_true));
    let (unit, _bridge, attempt, out) = graph_unit(&root, runner.clone());
    let err = drive(&unit, &data_root, &attempt, &out)
        .expect_err("终态超 token 帽应在完工前熔断（双相第二相）");
    match err {
        af_execline::ExeclineError::BudgetCapExceeded {
            dimension,
            spent,
            cap,
            ..
        } => {
            assert_eq!(dimension, "tokens_total", "熔断维度 = tokens_total");
            assert!(spent >= cap, "累计 {spent} ≥ 帽 {cap}");
        }
        other => panic!("应 EX4007 预算熔断，实得 {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn http_retries_are_recorded_in_graph_v1_row() {
    // 红队 W2-C33：graph v1 行须记 http_retries（此前 Ok 路径随 `..` 丢弃、
    // facts 硬编码 0——401 重取等供给面重试不可审计；swev/report 环都记）。
    let root = temp_root("w2-retries");
    let data_root = task_package(&root, &compile_plan(&sop_full_src()));
    let runner = Arc::new(GraphFakeRunner::with_http_retries(sop_full_payloads, 7));
    let (unit, _bridge, attempt, out) = graph_unit(&root, runner.clone());
    let output = drive(&unit, &data_root, &attempt, &out).expect("sop_full 应走通");
    assert_eq!(output.verdict, Some(true));
    let row: Value = serde_json::from_str(
        std::fs::read_to_string(out.join("records.jsonl"))
            .unwrap()
            .lines()
            .last()
            .unwrap(),
    )
    .expect("v1 行应合法 JSON");
    assert_eq!(
        row["usage"]["http_retries"], 21,
        "供给面重试须入 v1 行 usage.http_retries（3 个模型节点 × 7，stage 累账口径）"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn plan_digest_mismatch_rejects_before_binding() {
    // 红队 W2-C55：plan.json 摘要漂移即拒（先于装订）——graph 臂全部预算帽
    // （cost_cap/token_cap/节点 turns）都写在 plan.json，判据面（task.json）
    // 钉死不覆盖它；同 graph_version/registry_digest 换一份 plan.json 即可
    // 放宽预算且装订四核对全放行——故计划面须独立钉死。
    let root = temp_root("w2-planhash");
    let data_root = probe_task_package(
        &root,
        &compile_plan(&single_node_graph_src(
            "  budgets: []\n  stop: []\n  failures: []\n",
        )),
    );
    let runner = Arc::new(GraphFakeRunner::new(payload_resolved_true));
    let (unit, _bridge, attempt, out) = graph_unit(&root, runner.clone());
    std::fs::create_dir_all(&attempt).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    let sink = FileRecordSink::open(&out).unwrap();
    let bad_digest = "0".repeat(64);
    let ctx = ExecContext {
        order_id: "wo-graph-1",
        stable_id: "sid-graph-1",
        line_id: "line-A",
        out_dir: &out,
        task_id: TASK,
        host_alias: "local",
        data_root: &data_root,
        attempt_dir: &attempt,
        record_sink: &sink,
        graph_version: None,
        task_sha256: None,
        plan_sha256: Some(bad_digest.as_str()),
        trial: false,
    };
    let err = unit.execute(&ctx).expect_err("计划面摘要不符应拒");
    assert!(
        err.to_string().contains("计划面摘要不符"),
        "拒因须点名计划面摘要：{err}"
    );
    assert!(runner.launches().is_empty(), "拒在执行前，零 launch");
    let _ = std::fs::remove_dir_all(&root);
}

// ============================================================
// ⑩ 红队 W2-C51：判分锚在场 → swebench_eval 越过拒载门进入 verify 链
// ============================================================

/// diff 取证必败桥：`git diff` 取证脚本返回 Err——verify 链在取证步
/// 短路返回未决 verdict（不触 docker/judge 真子进程），但足以证明
/// 分派面已越过「spec.judge 未配置」拒载门、冻结判分链真实启动。
struct FailDiffBridge {
    calls: Mutex<Vec<String>>,
}

impl FailDiffBridge {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl af_execline::HostBridge for FailDiffBridge {
    fn run(&self, _host: &str, script: &str, _t: u32) -> Result<(bool, String), String> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(script.to_string());
        if script.contains("git -c core.fileMode=false diff") {
            Err("注入失败：diff 取证不可达（W2-C51 用例）".into())
        } else {
            Ok((true, String::new()))
        }
    }
}

/// W2-C51：spec.judge 在场（订单层 graph_unit_for 同款注入；此处指向
/// 测试自写假脚本 + 实算 sha256——指纹门可过、又不触真判分子进程）时，
/// swebench_eval 检查节点不再 fail-loud 拒载，而是真实进入 swev::verify
/// 链（指纹门 → capture_diff → …）。至此图臂冻结判分链从 spec 装配到
/// 分派执行全链可达。
#[test]
fn swebench_eval_with_judge_passes_gate_into_verify_chain() {
    let root = temp_root("g4sweeval-c51");
    let graph_src = sweeval_graph_src();
    let data_root = task_package(&root, &compile_plan(&graph_src));
    let scripted = |node: &str| -> Option<String> {
        match node {
            "actor" => Some(
                json!({"action": {"thought": "t", "command": "c", "submit": false}}).to_string(),
            ),
            _ => None,
        }
    };
    let runner = Arc::new(GraphFakeRunner::new(scripted));
    let bridge = Arc::new(FailDiffBridge::new());
    let mut spec = GraphUnitSpec::v1(PROFILE, repo_packages());
    let fake_script = root.join("fake-judge.py");
    std::fs::write(&fake_script, "print('fake judge')\n").unwrap();
    spec.judge = Some(af_execline::JudgeLocator {
        python: fake_script.clone(),
        script: fake_script.clone(),
        script_sha256: af_execline::file_sha256_hex(&fake_script).expect("假判分脚本应可读"),
    });
    let unit =
        GraphExecuteUnit::new(spec, bridge.clone(), runner.clone()).expect("图单元装载应通过");
    let attempt = root.join("attempt");
    let out = root.join("out");

    let err = drive(&unit, &data_root, &attempt, &out)
        .expect_err("取证必败应使 evaluator 失败（非拒载）");
    let text = err.to_string();
    assert!(
        !(text.contains("spec.judge") && text.contains("判分锚")),
        "judge 在场不得再走拒载门：{err}"
    );
    assert_eq!(
        runner.order(),
        ["actor"],
        "actor 已跑、evaluator 进 verify 链后失败"
    );
    assert!(
        bridge
            .calls()
            .iter()
            .any(|c| c.contains("git -c core.fileMode=false diff")),
        "verify 链真实启动过（diff 取证确经桥）：{:?}",
        bridge.calls()
    );
}

// ============================================================
// ⑫ 红队 W2-C34：失败路径落 legacy 失败行——失败单与「从未开跑」可区分
// ============================================================

/// 读回输出目录的 legacy 轨（results.jsonl）行集。
fn read_legacy_rows(out: &Path) -> Vec<Value> {
    let text = std::fs::read_to_string(out.join("results.jsonl")).unwrap_or_default();
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("results.jsonl 行应为合法 JSON"))
        .collect()
}

/// EX3001（check 节点 swebench_eval 拒载——驱动相失败）→ run_failed 行：
/// suite=graph、error_code=EX3001、verdict(reward)=null、run_dir 在场；
/// v1 轨零失败投影（RESULT-P4 §4）。此前失败路径双轨零行，失败单与
/// 「从未开跑」不可区分。
#[test]
fn unit_failure_emits_run_failed_row_ex3001() {
    let root = temp_root("w2c34-ex3001");
    let graph_src = sweeval_graph_src();
    let data_root = task_package(&root, &compile_plan(&graph_src));
    let scripted = |node: &str| -> Option<String> {
        match node {
            "actor" => Some(
                json!({"action": {"thought": "t", "command": "c", "submit": false}}).to_string(),
            ),
            _ => None,
        }
    };
    let runner = Arc::new(GraphFakeRunner::new(scripted));
    let (unit, _bridge, attempt, out) = graph_unit(&root, runner.clone());

    let err = drive(&unit, &data_root, &attempt, &out).expect_err("拒载应失败");
    assert_eq!(err.code(), ErrorCode::UnitFailed, "{err}");

    let rows = read_legacy_rows(&out);
    assert_eq!(rows.len(), 1, "失败路径恰落一行 legacy 失败行");
    assert_eq!(rows[0]["status"], "run_failed");
    assert_eq!(rows[0]["suite"], "graph");
    assert_eq!(rows[0]["error_code"], "EX3001");
    assert_eq!(rows[0]["task_id"], TASK);
    assert!(rows[0]["reward"].is_null(), "失败行 verdict=null");
    assert!(
        rows[0]["run_dir"].as_str().is_some(),
        "run_failed 行记实际目录: {}",
        rows[0]
    );
    // v1 轨无失败投影（文件在（sink 双开）但零行）。
    let v1 = std::fs::read_to_string(out.join("records.jsonl")).unwrap_or_default();
    assert!(v1.trim().is_empty(), "失败行不落 v1 轨: {v1}");
}

/// EX4006（envelope 违例——驱动相，stage.commit 拒收上游产物）→ run_failed 行。
#[test]
fn unit_failure_emits_run_failed_row_ex4006() {
    let root = temp_root("w2c34-ex4006");
    let data_root = task_package(&root, &compile_plan(&sop_full_src()));
    let scripted = |node: &str| -> Option<String> {
        match node {
            "planner" => sop_full_payloads("planner"),
            "coder" => sop_full_payloads("coder"),
            "reviewer" => Some(json!({"verdict": "looks fine"}).to_string()),
            _ => None,
        }
    };
    let runner = Arc::new(GraphFakeRunner::new(scripted));
    let (unit, _bridge, attempt, out) = graph_unit(&root, runner.clone());

    let err = drive(&unit, &data_root, &attempt, &out).expect_err("envelope 违例应拒");
    assert_eq!(err.code(), ErrorCode::EnvelopeMismatch, "{err}");

    let rows = read_legacy_rows(&out);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["status"], "run_failed");
    assert_eq!(rows[0]["error_code"], "EX4006");
    assert!(rows[0]["reward"].is_null());
}

/// EX4008（loop 节点——linearize 在 setup 相拒绝）→ setup_failed 行：
/// run_dir=null（执行未启动、run 目录未建——D-20 字段齐备含 null）。
#[test]
fn unit_failure_emits_setup_failed_row_ex4008() {
    let root = temp_root("w2c34-ex4008");
    let plan_src = compile_plan(&sop_full_src());
    let mut looped: Value = serde_json::from_str(&plan_src).unwrap();
    looped["body"]["graph"]["nodes"][0]["loop_spec"] = json!({
        "body": "sweng_sop_full/coder",
        "max_iterations": 2,
        "carried": [],
        "safety_points": []
    });
    let data_root = task_package(&root, &serde_json::to_string(&looped).unwrap());
    let runner = Arc::new(GraphFakeRunner::new(sop_full_payloads));
    let (unit, bridge, attempt, out) = graph_unit(&root, runner.clone());

    let err = drive(&unit, &data_root, &attempt, &out).expect_err("loop 节点应拒");
    assert_eq!(err.code(), ErrorCode::GraphShapeUnsupported, "{err}");
    assert_eq!(runner.launches().len(), 0, "setup 相拒绝零节点驱动");
    assert!(bridge.calls().is_empty(), "setup 相拒绝零容器调用");

    let rows = read_legacy_rows(&out);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["status"], "setup_failed");
    assert_eq!(rows[0]["error_code"], "EX4008");
    assert!(rows[0]["reward"].is_null());
    assert!(
        rows[0]["run_dir"].is_null(),
        "setup_failed 行 run 目录未建 → null: {}",
        rows[0]
    );
}

// ============================================================
// ⑬ 红队 W2-C48：判据命令冻结契约 + test_edit 判据退化门
// ============================================================

/// 测试面被改的取证桥：git diff 返回碰测试文件的补丁（S0 D2 越权形态）。
struct TestTamperBridge {
    calls: Mutex<Vec<String>>,
}

impl TestTamperBridge {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl af_execline::HostBridge for TestTamperBridge {
    fn run(&self, _host: &str, script: &str, _t: u32) -> Result<(bool, String), String> {
        self.calls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(script.to_string());
        if script.starts_with("docker rm -f") || script.starts_with("docker run") {
            Ok((true, String::new()))
        } else if script.contains("git -c core.fileMode=false diff") {
            Ok((
                true,
                "diff --git a/tests/test_foo.py b/tests/test_foo.py\n--- a/tests/test_foo.py\n+++ \
                 b/tests/test_foo.py\n"
                    .into(),
            ))
        } else if script.contains("docker exec") {
            Ok((true, "Ran 12 tests in 3s\n\nOK\n__AF_RC=0".into()))
        } else {
            Ok((true, String::new()))
        }
    }
}

/// ① 判据命令必须随任务包冻结声明：task.json 缺非空 test_command →
/// test_unit 检查节点 fail-loud 拒载，缺省全仓 `python -m pytest -q`
/// 判据面废除（先红：缺声明静默落缺省命令、对模型全权可控的 /testbed
/// 跑全仓测试）。
#[test]
fn test_unit_without_declared_command_fails_loud() {
    let root = temp_root("w2c48-nocmd");
    // 任务包不带 test_command 字段（task_package 除外手写）。
    let data_root = root.join("data");
    let task_dir = data_root.join(TASK);
    std::fs::create_dir_all(&task_dir).unwrap();
    std::fs::write(
        task_dir.join("plan.json"),
        compile_plan(&sop_full_src()),
    )
    .unwrap();
    std::fs::write(
        task_dir.join("task.json"),
        json!({
            "repo": "django/django",
            "instance_id": TASK,
            "problem_statement": "Fix the thing",
            "image": "swebench/sweb.eval.x86_64.django-11099:latest"
        })
        .to_string(),
    )
    .unwrap();
    let runner = Arc::new(GraphFakeRunner::new(sop_full_payloads));
    let (unit, bridge, attempt, out) = graph_unit(&root, runner.clone());

    let err = drive(&unit, &data_root, &attempt, &out).expect_err("缺判据声明应拒载");
    assert_eq!(err.code(), ErrorCode::UnitFailed, "{err}");
    assert!(
        err.to_string().contains("判据命令未声明") && err.to_string().contains("test_command"),
        "拒载点名判据命令缺声明：{err}"
    );
    assert!(
        !bridge
            .calls()
            .iter()
            .any(|s| s.contains("docker exec") && s.contains("pytest")),
        "缺声明零判据执行（缺省 pytest 面不再兜底）：{:?}",
        bridge.calls()
    );
}

/// ② test_edit 判据退化门：模型碰测试面（git diff 含 tests/）→ 退出码 0
/// 的 pass 亦作废（failures 点名 test_edit），合格终态不授予（qualified=false、
/// 行 gate=test_edit、verdict 如实）——判据不再仅凭任意 shell 退出码。
#[test]
fn test_unit_pass_revoked_when_model_touched_tests() {
    let root = temp_root("w2c48-tamper");
    let data_root = task_package(&root, &compile_plan(&sop_full_src()));
    let runner = Arc::new(GraphFakeRunner::new(sop_full_payloads));
    let bridge = Arc::new(TestTamperBridge::new());
    let spec = GraphUnitSpec::v1(PROFILE, repo_packages());
    let unit = GraphExecuteUnit::new(spec, bridge.clone(), runner)
        .expect("图单元装载（profile/注册表可解析）");
    let attempt = root.join("attempt");
    let out = root.join("out");

    let output = drive(&unit, &data_root, &attempt, &out).expect("判据退化非执行错误（照走完）");
    assert!(
        !output.artifacts[0].qualified,
        "碰测试面的通过不得授予合格终态"
    );
    // tester verdict envelope：pass 翻假 + failures 点名 test_edit。
    let verdict: Value = serde_json::from_str(
        &std::fs::read_to_string(attempt.join("artifacts").join("tester.report.json")).unwrap(),
    )
    .unwrap();
    let payload = &verdict["payload"];
    assert_eq!(payload["pass"], json!(false), "退出码 0 亦作废: {verdict}");
    assert!(
        payload["failures"]
            .as_array()
            .is_some_and(|f| f.iter().any(|x| x.as_str().unwrap_or("").contains("test_edit"))),
        "退化门点名 test_edit: {verdict}"
    );
    // 行面：gate=test_edit、verdict 如实 false（非合格）。
    let row: Value = serde_json::from_str(
        std::fs::read_to_string(out.join("records.jsonl"))
            .unwrap()
            .lines()
            .last()
            .expect("v1 行应在盘"),
    )
    .unwrap();
    assert_eq!(row["gate"], json!("test_edit"), "gate 记账: {row}");
}

/// ③ 判据退化非门控面也记账：碰测试面且判据本就失败（rc!=0）时，verdict
/// 上仍附 test_edit 清单（对账可查，不静默）。
#[test]
fn test_unit_failure_row_carries_test_edit_marker() {
    let root = temp_root("w2c48-tamper-fail");
    let data_root = task_package(&root, &compile_plan(&sop_full_src()));
    let runner = Arc::new(GraphFakeRunner::new(sop_full_payloads));
    // 桥：diff 碰测试面 + 判据命令失败（rc=1）。
    struct TamperFailBridge;
    impl af_execline::HostBridge for TamperFailBridge {
        fn run(&self, _host: &str, script: &str, _t: u32) -> Result<(bool, String), String> {
            if script.starts_with("docker rm -f") || script.starts_with("docker run") {
                return Ok((true, String::new()));
            }
            if script.contains("git -c core.fileMode=false diff") {
                return Ok((
                    true,
                    "diff --git a/conftest.py b/conftest.py\n--- a/conftest.py\n+++ b/conftest.py\n"
                        .into(),
                ));
            }
            Ok((true, "FAILED tests/test_x.py\n__AF_RC=1".into()))
        }
    }
    let bridge = Arc::new(TamperFailBridge);
    let spec = GraphUnitSpec::v1(PROFILE, repo_packages());
    let unit = GraphExecuteUnit::new(spec, bridge, runner).expect("图单元装载应通过");
    let attempt = root.join("attempt");
    let out = root.join("out");

    let output = drive(&unit, &data_root, &attempt, &out).expect("判据失败照走完（不 Err）");
    assert!(!output.artifacts[0].qualified);
    let verdict: Value = serde_json::from_str(
        &std::fs::read_to_string(attempt.join("artifacts").join("tester.report.json")).unwrap(),
    )
    .unwrap();
    let payload = &verdict["payload"];
    assert_eq!(payload["pass"], json!(false));
    assert_eq!(
        payload["test_edit"],
        json!(["conftest.py"]),
        "失败判定也附 test_edit 清单: {verdict}"
    );
}
