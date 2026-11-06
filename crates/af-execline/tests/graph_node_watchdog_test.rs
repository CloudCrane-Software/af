//! E1 缺口⑤节点监督两测（e1-报告.md E2G6：turn 42 declare 成功后运行时
//! 挂起 2h+、容器消失、进程僵死、零干预环境无看门狗；「artifacts 目录
//! declare 前应为空」实证）。
//!
//! ① wall-clock 帽杀：FakeRunner 起 `sleep 777777` 后永久挂起（伪挂起
//!   进程），spec 覆写帽 = 2s——断言 EX4009 判死、失败行落盘（run_failed
//!   的 timeout 分类）、sleep 进程链被 SIGKILL、驱动未真等 sleep 完。
//! ② 产物先落盘拒 declare：artifacts 目录置只读（空目录 + 写不进 = 产物
//!   无法先落盘），FakeRunner 真调 declare 工具面——断言拒收（fail-loud
//!   文案指路）、envelope 未落盘；恢复写权限后同链路重跑——断言 declare
//!   时点产物已在盘（envelope-v1 工件在场）。
//!
//! 注入手法与 graph_unit.rs 同源：FakeRunner + FakeBridge（零模型/零容器/
//! 零 ssh），计划经真编译器全链产出（digest/模板核对走真路径）。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use af_execline::exec::{ExecContext, ExecuteUnit};
use af_execline::graph_unit::{GraphExecuteUnit, GraphUnitSpec};
use af_execline::record::FileRecordSink;
use serde_json::{json, Value};

const TASK: &str = "django__django-11099";
const PROFILE: &str = "glm-cp";
/// 伪挂起进程的 sleep 秒值（唯一可辨，断言回收用）。
const HANG_SLEEP_SECS: &str = "777777";

fn repo_packages() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../af/packages")
}

fn temp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("af-graph-watchdog-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn sop_full_src() -> String {
    std::fs::read_to_string(
        repo_packages()
            .join("sweng")
            .join("graphs")
            .join("sop_full@1.1.afb"),
    )
    .expect("sop_full@1.1.afb 仓库正本应在盘")
}

fn compile_plan(graph_src: &str) -> String {
    let doc = af_dsl::load_str(graph_src).expect("sop_full 应装载");
    let verdict = af_dsl::validate(&doc);
    assert!(verdict.is_ok(), "sop_full 应零违例：{verdict}");
    let reg = af_compiler::ComponentRegistry::load(repo_packages()).expect("仓库注册表应装载");
    af_compiler::plan_json(&doc, &reg).expect("sop_full 计划应生成")
}

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

// ---- FakeBridge（graph_unit.rs 测试同款脚本化 docker 面）----

struct FakeBridge {
    calls: Mutex<Vec<String>>,
}

impl FakeBridge {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
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
            Ok((
                true,
                "diff --git a/astropy/sampler.py b/astropy/sampler.py\n--- a/a\n+++ b/b\n".into(),
            ))
        } else if script.contains("docker exec") {
            Ok((true, "Ran 12 tests in 3s\n\nOK\n__AF_RC=0".into()))
        } else {
            Ok((true, String::new()))
        }
    }
}

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
        order_id: "wo-graph-watchdog-1",
        stable_id: "sid-graph-watchdog-1",
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

fn graph_unit(
    spec: GraphUnitSpec,
    runner: Arc<dyn af_execline::AgentRunner>,
) -> (GraphExecuteUnit, Arc<FakeBridge>) {
    let bridge = Arc::new(FakeBridge::new());
    let unit = GraphExecuteUnit::new(spec, bridge.clone(), runner)
        .expect("图单元装载（profile/注册表可解析）");
    (unit, bridge)
}

fn spec_with_wall_clock(wall_clock: Duration) -> GraphUnitSpec {
    let mut spec = GraphUnitSpec::v1(PROFILE, repo_packages());
    spec.default_node_wall_clock = wall_clock;
    spec
}

/// /proc 进程态：缺席（None）或 stat 的 state 字母（'Z' = 僵尸已被杀）。
#[cfg(target_os = "linux")]
fn proc_state(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rparen = stat.rfind(')')?;
    let mut tokens = stat[rparen + 1..].split_whitespace();
    tokens.next().and_then(|state| state.chars().next())
}

// ============================================================
// ① 伪挂起进程：wall-clock 帽判死 + 进程链回收 + EX4009 终态行
// ============================================================

/// 挂起 Runner：planner 节点起 `sleep 777777` 后永久 park（E2G6 僵死复现），
/// 其余节点不应被驱动到。sleep 的 pid 记入共享面供回收断言。
struct HangRunner {
    children: Mutex<Vec<u32>>,
}

impl af_execline::AgentRunner for HangRunner {
    // zombie_processes 允许：本测试的目的就是留下一个无人 wait 的伪挂起
    // 进程——看门狗必须能从进程链外回收它（测试断言其 Z/缺席），若在此
    // wait 则挂起测试自己先被拖死。
    #[allow(clippy::zombie_processes)]
    fn run(
        &self,
        launch: af_execline::AgentLaunch<'_>,
    ) -> Result<af_execline::AgentRunOutcome, af_execline::AgentRunFailure> {
        let node = launch
            .run_id
            .strip_prefix(&format!("{TASK}-"))
            .unwrap_or_default()
            .to_string();
        if node == "planner" {
            let child = std::process::Command::new("sleep")
                .arg(HANG_SLEEP_SECS)
                .spawn()
                .expect("伪挂起进程 sleep 应可拉起");
            self.children
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(child.id());
            // 模型环永久挂起：主线程 recv_timeout 封顶的唯一等待源。
            std::thread::park();
        }
        Err(af_execline::AgentRunFailure {
            message: format!("挂起测试不应驱动到节点 {node}"),
            http_retries: 0,
        })
    }
}

#[test]
#[cfg(target_os = "linux")]
fn hung_node_run_is_killed_by_wall_clock_cap_and_failed_row_marks_timeout() {
    let root = temp_root("hang");
    let data_root = task_package(&root, &compile_plan(&sop_full_src()));
    let runner = Arc::new(HangRunner {
        children: Mutex::new(Vec::new()),
    });
    let (unit, _bridge) = graph_unit(spec_with_wall_clock(Duration::from_secs(2)), runner.clone());
    let attempt = root.join("attempt");
    let out = root.join("out");

    let started = Instant::now();
    let err = drive(&unit, &data_root, &attempt, &out)
        .expect_err("挂起节点超 wall-clock 帽必须判死（EX4009）");
    let elapsed = started.elapsed();

    // 判死形态：EX4009 + timeout 分类文案 + 进程链回收计数。
    assert_eq!(
        err.code().as_str(),
        "EX4009",
        "超帽必须是节点运行时超时错误码: {err}"
    );
    let text = err.to_string();
    assert!(
        text.contains("wall-clock") && text.contains("SIGKILL"),
        "判死文案须点名 wall-clock 帽与进程链回收: {text}"
    );
    assert!(text.contains("planner"), "判死文案须点名挂起节点: {text}");
    // 帽杀必须真发生：不是等 sleep 自然完（777777s）。
    assert!(
        elapsed < Duration::from_secs(60),
        "看门狗必须在帽附近判死，而非等挂起进程自然结束: {elapsed:?}"
    );

    // 进程链回收：sleep 被 SIGKILL（僵尸待收或已消失，绝不存活）。
    // kill(1) 送达即退——目标进程的死是异步调度的；轮询等待其落地（有界），
    // 不接受「刚发信号就断言」的调度竞态。
    let children = runner
        .children
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    assert_eq!(children.len(), 1, "伪挂起进程恰一个");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut state = proc_state(children[0]);
    while state.is_some() && state != Some('Z') && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        state = proc_state(children[0]);
    }
    assert!(
        state.is_none() || state == Some('Z'),
        "sleep 必须已被回收（缺席或僵尸），实测 state={state:?}"
    );

    // 终态落盘：results.jsonl 有 run_failed 行，error_code=EX4009。
    let results = std::fs::read_to_string(out.join("results.jsonl")).unwrap();
    let line = results
        .lines()
        .find(|line| line.contains("\"status\":\"run_failed\""))
        .unwrap_or_else(|| panic!("失败行必须在案: {results}"));
    assert!(
        line.contains("\"error_code\":\"EX4009\""),
        "失败行必须 EX4009 timeout 分类: {line}"
    );
    assert!(
        line.contains("wall-clock"),
        "失败行 note 须点名 wall-clock: {line}"
    );
}

// ============================================================
// ② 产物先落盘：工件面写不进 → declare 拒收（fail-loud 指路）
// ============================================================

/// 声明 Runner：三节点全部走 declare 工具面（真模型收敛路径）。记录
/// declare 的拒收文案全形与「declare 后产物是否已在盘」的观测。
struct DeclareRunner {
    declare_err: Mutex<Option<String>>,
    landed_at_declare: Mutex<Option<bool>>,
}

fn declare_payload(node: &str) -> String {
    match node {
        "planner" => json!({"plan": {"instance_id": TASK, "steps": [
            {"id": "s1", "action": "locate", "target": "django/db/models"}
        ]}})
        .to_string(),
        "coder" => json!({"files": [
            {"path": "django/db/models/foo.py", "content_ref": "workspace#1"}
        ]})
        .to_string(),
        "reviewer" => json!({"review": {"approved": true, "issues": []}}).to_string(),
        other => panic!("declare 测试无节点 {other} 脚本"),
    }
}

impl af_execline::AgentRunner for DeclareRunner {
    fn run(
        &self,
        launch: af_execline::AgentLaunch<'_>,
    ) -> Result<af_execline::AgentRunOutcome, af_execline::AgentRunFailure> {
        let node = launch
            .run_id
            .strip_prefix(&format!("{TASK}-"))
            .unwrap_or_default()
            .to_string();
        let binding = launch
            .extra_tools
            .iter()
            .find(|t| t.name == "declare")
            .expect("环组装面必须注册 declare");
        let args: Value = serde_json::from_str(&declare_payload(&node)).unwrap();
        match (binding.handler)(&args) {
            Ok(_) => {
                // declare 时点观测：工件面（attempt_dir = <attempt>/nodes/<node>，
                // 工件面在其上两级）是否已落盘——先于 commit 相的独立证据。
                let artifacts = launch
                    .attempt_dir
                    .parent()
                    .and_then(Path::parent)
                    .map(|p| p.join("artifacts"))
                    .expect("attempt_dir 形 <attempt>/nodes/<node>");
                let landed = artifacts
                    .join(format!("{node}.{}.json", port_of(&node)))
                    .exists();
                *self
                    .landed_at_declare
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = Some(landed);
            }
            Err(message) => {
                *self.declare_err.lock().unwrap_or_else(|e| e.into_inner()) = Some(message.clone());
                return Err(af_execline::AgentRunFailure {
                    message: format!("declare 调用被拒: {message}"),
                    http_retries: 0,
                });
            }
        }
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
                    text: declare_payload(&node),
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

fn port_of(node: &str) -> &'static str {
    match node {
        "planner" => "plan",
        "coder" => "files",
        "reviewer" => "review",
        other => panic!("未知节点 {other}"),
    }
}

#[test]
fn declare_rejected_when_artifacts_cannot_land_on_disk() {
    let root = temp_root("declare-disk");
    let data_root = task_package(&root, &compile_plan(&sop_full_src()));

    // 相位 A：artifacts 目录置只读（空目录且写不进 = 产物无法先落盘）。
    let attempt = root.join("attempt-ro");
    let out = root.join("out-ro");
    let artifacts = attempt.join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let original_perms = std::fs::metadata(&artifacts).unwrap().permissions();
    let mut perms = original_perms.clone();
    perms.set_readonly(true);
    std::fs::set_permissions(&artifacts, perms).unwrap();

    let runner = Arc::new(DeclareRunner {
        declare_err: Mutex::new(None),
        landed_at_declare: Mutex::new(None),
    });
    let (unit, _bridge) = graph_unit(
        spec_with_wall_clock(Duration::from_secs(30)),
        runner.clone(),
    );
    let err = drive(&unit, &data_root, &attempt, &out)
        .expect_err("产物无法落盘时 declare 必须拒收（运行失败）");
    assert_eq!(err.code().as_str(), "EX3001", "运行失败面归 EX3001: {err}");

    // 拒收触发：fail-loud 文案点名产物先落盘 + 工件目录指路。
    let declare_err = runner
        .declare_err
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .expect("declare 拒收文案必须被捕获");
    assert!(
        declare_err.contains("declare 拒收"),
        "拒收文案首 token: {declare_err}"
    );
    assert!(
        declare_err.contains("产物先落盘"),
        "拒收文案须点名产物先落盘（E1 缺口⑤）: {declare_err}"
    );
    assert!(
        declare_err.contains("artifacts"),
        "拒收文案须指路工件目录: {declare_err}"
    );
    assert!(
        declare_err.contains("planner.plan.json"),
        "拒收文案须点名未落盘工件文件: {declare_err}"
    );
    // 产物不在盘：拒收后工件面保持空（envelope 未写入）。
    assert_eq!(
        std::fs::read_dir(&artifacts).unwrap().count(),
        0,
        "拒收后 artifacts 目录必须仍为空"
    );
    assert!(
        runner
            .landed_at_declare
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_none(),
        "拒收相位无「declare 后在盘」观测"
    );

    // 恢复写权限（清理面 + 后续相位前提）：还原 chmod 前的原始权限，
    // 不用 set_readonly(false)（Unix 面会落成 world-writable）。
    std::fs::set_permissions(&artifacts, original_perms).unwrap();

    // 终态行：相位 A 失败行在案（run_failed）。
    let results = std::fs::read_to_string(out.join("results.jsonl")).unwrap();
    assert!(
        results.contains("\"status\":\"run_failed\""),
        "拒收相位失败行必须落盘: {results}"
    );

    // 相位 B：同链路恢复写权限重跑——declare 时点产物必须在盘（正控）。
    let attempt2 = root.join("attempt-rw");
    let out2 = root.join("out-rw");
    let runner2 = Arc::new(DeclareRunner {
        declare_err: Mutex::new(None),
        landed_at_declare: Mutex::new(None),
    });
    let (unit2, _bridge2) = graph_unit(
        spec_with_wall_clock(Duration::from_secs(30)),
        runner2.clone(),
    );
    drive(&unit2, &data_root, &attempt2, &out2).expect("写权限恢复后同链路应走通");
    assert_eq!(
        runner2
            .declare_err
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone(),
        None,
        "正控相位无拒收"
    );
    assert_eq!(
        runner2
            .landed_at_declare
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone(),
        Some(true),
        "declare 时点（先于 commit 相）产物必须已落盘"
    );
    let envelope = std::fs::read_to_string(attempt2.join("artifacts").join("planner.plan.json"))
        .expect("正控相位 envelope 工件必须在盘");
    assert!(
        envelope.contains("\"envelope\":\"v1\"") && envelope.contains("\"port\":\"plan\""),
        "envelope-v1 工件形: {envelope}"
    );
}
