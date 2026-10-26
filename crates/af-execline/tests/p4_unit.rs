//! P4 SwevExecuteUnit 验收测试（af-bench 吸收方案 §5-P4 草案 + §2.2/§2.3/§2.4）。
//!
//! 先红后绿（ADR-007）：本文件先于实现提交，桩 `unimplemented!()` 下必红。
//!
//! 注入手法（零模型、零凭证、零真容器、零网络——资源纪律；本包测试即
//! Layer 1 对拍的"金种子"，方案 §5-P4）：
//! - FakeRunner（AgentRunner 缝）：捕获 AgentLaunch 全形状（run_id/user_message/
//!   spec 采样覆写/policy/预算），返回固定 RunReport 或结构化失败；
//! - FakeBridge（HostBridge 缝，P2 手法 + 智能分派）：按命令形状回放（rm/run/
//!   capture_diff/docker exec），setup 失败臂可注入；
//! - 假 docker（PATH 首段注入，p3_verify 手法）：verify 的 eval 执行链真子进程；
//! - 假判分 venv（p3_judge 手法）：judge 子进程真拉起回 JSON。
//!
//! 行为对照锚 = af-bench main.rs:625-914（run_swev 单题执行体）逐段：
//! 三件套落盘 + 18 键 legacy 行 + v1 投影 / gate 分支（main.rs:806-813）/
//! run_failed 行 + cleanup（main.rs:818-845）/ setup_failed 行（main.rs:654-677）/
//! bash 护栏（main.rs:710-734）/ 装载期指纹门（§3.2-2 双重防线）。

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use af_execline::agent::{AgentLaunch, AgentRunFailure, AgentRunOutcome, AgentRunner};
use af_execline::error::ErrorCode;
use af_execline::exec::{ExecContext, ExecuteUnit};
use af_execline::hostbridge::HostBridge;
use af_execline::judge::JudgeLocator;
use af_execline::record::{
    swev_legacy_row, swev_run_failed_row, swev_setup_failed_row, swev_v1_row, ExecutionFacts,
    PendingRecordSink, RecordSink,
};
use af_execline::suite::next_attempt_index;
use af_execline::suite::swev::SWEV_INSTANCE_PROTOCOL;
use af_execline::suite::swev_unit::{SwevExecuteUnit, SwevUnitSpec};
use af_execline::{ExecOutput, ExeclineError};
use af_harness::{EndpointSpec, RunReport, RunStatus, TokenTotals};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const TASK: &str = "astropy__astropy-12907";
const PROFILE: &str = "glm-cp";
const CONTAINER: &str = "afbench-swev-astropy-astropy-12907-glm-cp";
const CAPTURE_CMD: &str = concat!(
    "docker exec afbench-swev-astropy-astropy-12907-glm-cp ",
    "bash -lc 'cd /testbed && git -c core.fileMode=false diff' "
);
const NORMAL_DIFF: &str = "diff --git a/astropy/sampler.py b/astropy/sampler.py\n--- a/astropy/sampler.py\n+++ b/astropy/sampler.py\n@@ -1 +1 @@\n-fix\n+fix2\n";
const TEST_DIFF: &str = "diff --git a/tests/test_sampler.py b/tests/test_sampler.py\n--- a/tests/test_sampler.py\n+++ b/tests/test_sampler.py\n@@ -1 +1 @@\n-x\n+y\n";

// ---- 公共助手 ----

static SPAWN_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

fn guard() -> &'static Mutex<()> {
    SPAWN_GUARD.get_or_init(|| Mutex::new(()))
}

fn temp_root(tag: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("af-execline-p4unit-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn write_exec(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// 独立 sha256 oracle（不借用被测实现的 sha256_hex）。
fn sha256_of(path: &Path) -> String {
    let data = std::fs::read(path).unwrap();
    let digest = Sha256::digest(&data);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 题库根 + 任务包（main.rs:640 load_swev_tasks 读 <data_root>/<task>.json；
/// 该文件同时是判分 task_json 三件套之一，main.rs:850）。
fn data_root_with_task(root: &Path) -> PathBuf {
    let data_root = root.join("data");
    std::fs::create_dir_all(&data_root).unwrap();
    std::fs::write(
        data_root.join(format!("{TASK}.json")),
        json!({
            "instance_id": TASK,
            "repo": "astropy/astropy",
            "problem_statement": "Fix the sampler",
            "image": "sweb.eval.x86_64.astropy-12907",
            "eval_script": "#!/bin/bash\nset -uxo pipefail\nfixture eval script body\n",
        })
        .to_string(),
    )
    .unwrap();
    data_root
}

/// run 目录（main.rs:698 + 760-763：attempt = next_attempt_index 分配，
/// 目录名 swev-<task>-<profile>-a<N>，只新建不覆写）。
fn fresh_attempt_dir(root: &Path) -> PathBuf {
    let runs_root = root.join("runs");
    std::fs::create_dir_all(&runs_root).unwrap();
    let attempt = next_attempt_index(&runs_root, TASK, PROFILE);
    runs_root.join(format!("swev-{TASK}-{PROFILE}-a{attempt}"))
}

/// 假判分 venv（p3 手法）：假 python 回一行 JSON；脚本任意字节，锚 = 真实 sha256。
fn fake_judge(tag: &str, json_line: &str) -> JudgeLocator {
    let root = temp_root(tag);
    let python = root.join("fake-python");
    write_exec(&python, &format!("#!/bin/bash\necho '{json_line}'\n"));
    let script = root.join("fake-swev-judge.py");
    std::fs::write(&script, b"# fixture judge script bytes").unwrap();
    let script_sha256 = sha256_of(&script);
    JudgeLocator {
        python,
        script,
        script_sha256,
    }
}

/// 假 docker（PATH 首段注入，p3_verify 手法）：吃 stdin 落证、出一行日志、
/// 退出 0（超时臂 p3 已钉死，此处只走正常流）。
fn with_fake_docker<R>(f: impl FnOnce() -> R) -> R {
    let _lock = guard().lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("af-execline-p4-fakebin-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("docker");
    std::fs::write(
        &script,
        format!(
            "#!/bin/bash\n\
             /usr/bin/cat > \"{dir}/stdin-$3.txt\"\n\
             echo '+ fake eval output line'\n\
             exit 0\n",
            dir = dir.display(),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let orig = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{}", dir.display(), orig));
    let out = f();
    std::env::set_var("PATH", orig);
    out
}

// ---- FakeBridge（HostBridge 缝；Send+Sync——单元持 Arc<dyn HostBridge>）----

type BridgeResult = Result<(bool, String), String>;

/// 智能分派假桥：按命令形状回放，全调用留痕。
struct FakeBridge {
    calls: Mutex<Vec<(String, String, u32)>>,
    /// `docker run` 结果：Ok((false, log)) → setup 失败臂（main.rs:654）。
    run_result: Mutex<BridgeResult>,
    /// capture_diff 返回文本（gate 与 verify 两次同值）。
    diff_text: Mutex<String>,
}

impl FakeBridge {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            run_result: Mutex::new(Ok((true, String::new()))),
            diff_text: Mutex::new(NORMAL_DIFF.to_string()),
        }
    }

    fn with_run_failure(log: &str) -> Self {
        Self {
            run_result: Mutex::new(Ok((false, log.to_string()))),
            ..Self::new()
        }
    }

    fn with_diff(diff: &str) -> Self {
        Self {
            diff_text: Mutex::new(diff.to_string()),
            ..Self::new()
        }
    }

    fn calls(&self) -> Vec<(String, String, u32)> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl HostBridge for FakeBridge {
    fn run(&self, host: &str, script: &str, timeout_sec: u32) -> BridgeResult {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).push((
            host.to_string(),
            script.to_string(),
            timeout_sec,
        ));
        if script.starts_with("docker rm -f") {
            Ok((true, String::new()))
        } else if script.starts_with("docker run") {
            self.run_result
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        } else if script.contains("git -c core.fileMode=false diff") {
            let text = self
                .diff_text
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            Ok((true, text))
        } else if script.starts_with("docker exec") {
            // bash 工具体（TbEnv::exec 消费）：带 __AF_RC 标记行。
            Ok((true, "exec-output\n__AF_RC=0".to_string()))
        } else {
            panic!("FakeBridge 未预期的命令: {script}")
        }
    }
}

// ---- FakeRunner（AgentRunner 缝）----

/// FakeRunner 捕获的缝输入形状（断言 main.rs:736-798 的组装面）。
struct CapturedLaunch {
    attempt_dir: PathBuf,
    run_id: String,
    user_message: String,
    system: String,
    endpoint: EndpointSpec,
    max_turns: u32,
    completion_signals: Vec<String>,
    tool_free_turn_limit: u32,
    tool_names: Vec<String>,
    bash_workdir_in_description: String,
    spend_cap_cents: i64,
    tool_spend_cents: i64,
    budget_ledger_cents: i64,
    bash: Arc<dyn Fn(&str) -> String + Send + Sync>,
}

fn fixed_report() -> RunReport {
    RunReport {
        run_id: "fake-run".into(),
        status: RunStatus::Completed {
            signal: "TASK_COMPLETE".into(),
        },
        turns: 3,
        model_calls: 5,
        effect_replays: 0,
        effect_fresh: 2,
        transcript: Vec::new(),
        whitelist: vec!["bash".into()],
        tokens: TokenTotals {
            input: 100,
            output: 50,
            cache_read: 10,
        },
    }
}

struct FakeRunner {
    launches: Mutex<Vec<CapturedLaunch>>,
    /// Ok(报告) / Err(失败)；http_retries 固定 2（Ok）/ 失败自备。
    result: Result<RunReport, AgentRunFailure>,
}

impl FakeRunner {
    fn ok() -> Self {
        Self {
            launches: Mutex::new(Vec::new()),
            result: Ok(fixed_report()),
        }
    }

    fn failing(message: &str, http_retries: u64) -> Self {
        Self {
            launches: Mutex::new(Vec::new()),
            result: Err(AgentRunFailure {
                message: message.to_string(),
                http_retries,
            }),
        }
    }

    fn launches(&self) -> usize {
        self.launches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    fn launch(&self) -> CapturedLaunch {
        let mut launches = self.launches.lock().unwrap_or_else(|e| e.into_inner());
        if launches.len() != 1 {
            panic!("期望恰一次 launch 捕获，实得 {}", launches.len());
        }
        launches.remove(0)
    }
}

impl AgentRunner for FakeRunner {
    fn run(&self, launch: AgentLaunch<'_>) -> Result<AgentRunOutcome, AgentRunFailure> {
        self.launches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(CapturedLaunch {
                attempt_dir: launch.attempt_dir.to_path_buf(),
                run_id: launch.run_id.clone(),
                user_message: launch.user_message.clone(),
                system: launch.spec.context_template.system.clone(),
                endpoint: launch.spec.endpoint.clone(),
                max_turns: launch.spec.policy.max_turns,
                completion_signals: launch.spec.policy.stop.completion_signals.clone(),
                tool_free_turn_limit: launch.spec.policy.stop.tool_free_turn_limit,
                tool_names: launch
                    .spec
                    .toolset
                    .iter()
                    .map(|tool| tool.name.clone())
                    .collect(),
                bash_workdir_in_description: launch
                    .spec
                    .toolset
                    .first()
                    .map(|tool| tool.description.clone())
                    .unwrap_or_default(),
                spend_cap_cents: launch.spend_cap_cents,
                tool_spend_cents: launch.tool_spend_cents,
                budget_ledger_cents: launch.budget_ledger_cents,
                bash: launch.bash,
            });
        match &self.result {
            Ok(report) => Ok(AgentRunOutcome {
                report: report.clone(),
                http_retries: 2,
            }),
            Err(failure) => Err(AgentRunFailure {
                message: failure.message.clone(),
                http_retries: failure.http_retries,
            }),
        }
    }
}

// ---- FakeSink / FailingSink（RecordSink 缝）----

#[derive(Default)]
struct FakeSink {
    legacy: Mutex<Vec<String>>,
    v1: Mutex<Vec<String>>,
}

impl FakeSink {
    fn legacy_rows(&self) -> Vec<Value> {
        self.legacy
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|row| serde_json::from_str(row).unwrap())
            .collect()
    }

    fn v1_rows(&self) -> Vec<Value> {
        self.v1
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|row| serde_json::from_str(row).unwrap())
            .collect()
    }
}

impl RecordSink for FakeSink {
    fn append_legacy(&self, row: &str) -> Result<(), String> {
        self.legacy
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(row.to_string());
        Ok(())
    }

    fn append_v1(&self, row: &str) -> Result<(), String> {
        self.v1
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(row.to_string());
        Ok(())
    }
}

struct FailingSink;

impl RecordSink for FailingSink {
    fn append_legacy(&self, _row: &str) -> Result<(), String> {
        Err("写 results 失败: 设备满".into())
    }

    fn append_v1(&self, _row: &str) -> Result<(), String> {
        Err("写 records 失败: 设备满".into())
    }
}

// ---- 场景装配 ----

/// 一次单题执行的全部材料（路径/sink/两缝/单元）。
struct Parts {
    root: PathBuf,
    data_root: PathBuf,
    attempt_dir: PathBuf,
    sink: FakeSink,
    bridge: Arc<FakeBridge>,
    runner: Arc<FakeRunner>,
    unit: SwevExecuteUnit,
}

/// 装配并执行一次单题执行（FakeRunner/FakeBridge/假判分/记录 sink），
/// 把产出与结果交给检查闭包（ctx 借用本地件，闭包内完成全部断言）。
fn execute_scenario<R>(
    tag: &str,
    bridge: FakeBridge,
    runner: FakeRunner,
    judge_json: &str,
    check: impl FnOnce(&Parts, Result<ExecOutput, ExeclineError>) -> R,
) -> R {
    let root = temp_root(tag);
    let data_root = data_root_with_task(&root);
    let attempt_dir = fresh_attempt_dir(&root);
    let sink = FakeSink::default();
    let judge = fake_judge(&format!("{tag}-judge"), judge_json);
    let spec = SwevUnitSpec::fix2(PROFILE, judge);
    let bridge = Arc::new(bridge);
    let runner = Arc::new(runner);
    let unit = SwevExecuteUnit::new(spec, bridge.clone(), runner.clone())
        .expect("单元装载（指纹门 + profile 检查）");
    let parts = Parts {
        root,
        data_root,
        attempt_dir,
        sink,
        bridge,
        runner,
        unit,
    };
    let ctx = ExecContext {
        order_id: "wo-p4",
        stable_id: "sid-p4",
        line_id: "line-p4",
        out_dir: &parts.root,
        task_id: TASK,
        host_alias: "local",
        data_root: &parts.data_root,
        attempt_dir: &parts.attempt_dir,
        record_sink: &parts.sink,
        graph_version: None,
        task_sha256: None,
        trial: false,
    };
    let outcome = parts.unit.execute(&ctx);
    check(&parts, outcome)
}

/// 弹出行的 wall_ms 类非确定字段（agent_wall_ms/wall_ms），返回 (余下对象, 值)。
fn pop_wall(row: &mut Value, key: &str) -> u64 {
    let value = row
        .as_object_mut()
        .unwrap()
        .remove(key)
        .expect("行必须带 wall 字段");
    value.as_u64().expect("wall 字段是 u64 毫秒数")
}

// ---- 主链：FakeRunner+FakeBridge 单题全流程（方案 §5-P4 草案第一条）----

/// 三件套落盘断言（§1.1 不变量：路径与字节语义不动）；返回 eval.log 指纹。
fn assert_trio_on_disk(attempt_dir: &Path) -> String {
    assert_eq!(
        std::fs::read_to_string(attempt_dir.join("agent.diff")).unwrap(),
        NORMAL_DIFF,
        "agent.diff = capture_diff 文本"
    );
    assert_eq!(
        std::fs::read_to_string(attempt_dir.join("eval.sh")).unwrap(),
        "#!/bin/bash\nset -uxo pipefail\nfixture eval script body\n",
        "eval.sh = task.eval_script 逐字节"
    );
    assert_eq!(
        std::fs::read_to_string(attempt_dir.join("eval.log")).unwrap(),
        "+ fake eval output line\n",
        "eval.log = 容器输出"
    );
    sha256_of(&attempt_dir.join("eval.log"))
}

/// 18 键 legacy 行字段级断言（main.rs:861-890）；返回弹出的 agent_wall_ms。
fn assert_legacy_row(parts: &Parts, eval_sha: &str) -> (Value, u64) {
    let legacy_rows = parts.sink.legacy_rows();
    assert_eq!(legacy_rows.len(), 1, "恰一行 legacy");
    let mut legacy = legacy_rows.into_iter().next().unwrap();
    assert_eq!(legacy.as_object().unwrap().len(), 18, "18 键齐备");
    let wall = pop_wall(&mut legacy, "agent_wall_ms");
    assert_eq!(
        legacy,
        json!({
            "suite": "swe-v",
            "task_id": TASK,
            "profile": PROFILE,
            "trial": false,
            "reward": true,
            "status": "completed",
            "scaffold": "v3",
            "reasoning_tier": "off",
            "run_dir": parts.attempt_dir.display().to_string(),
            "eval_sha256": eval_sha,
            "gate": "ok",
            "attempts": 1,
            "http_retries": 2,
            "turns": 3,
            "model_calls": 5,
            "tokens": {"input": 100, "output": 50, "cache_read": 10},
            "verifier_note": "judge: {\"resolved\": true}",
        }),
        "18 键字段级对照（judge note 无超时/log_parsed 前缀）"
    );
    (legacy, wall)
}

/// v1 行映射断言（契约 §1.2 行形状 + 方案 §1.4 映射表）；返回弹出的 wall_ms。
fn assert_v1_row(parts: &Parts, eval_sha: &str, legacy_wall: u64) -> (Value, u64) {
    let v1_rows = parts.sink.v1_rows();
    assert_eq!(v1_rows.len(), 1, "恰一行 v1 投影");
    let mut v1 = v1_rows.into_iter().next().unwrap();
    assert_eq!(v1.as_object().unwrap().len(), 14, "v1 14 键齐备");
    let wall_v1 = pop_wall(&mut v1, "wall_ms");
    assert_eq!(wall_v1, legacy_wall, "两轨同源：agent_wall_ms == wall_ms");
    assert_eq!(
        v1,
        json!({
            "plan_id": "wo-p4",
            "node_id": "line-p4",
            "item_id": TASK,
            "attempt": 1,
            "host_alias": "local",
            "unit_id": "swev-unit",
            "unit_version": "0.1.0",
            "status": "completed",
            "verdict": true,
            "evidence": {
                "run_dir": parts.attempt_dir.display().to_string(),
                "evidence_digest": eval_sha,
            },
            "gate": "ok",
            "usage": {
                "turns": 3,
                "unit_calls": 5,
                "http_retries": 2,
                "quota": {"input": 100, "output": 50, "cache_read": 10},
            },
            "note": "judge: {\"resolved\": true}",
        }),
        "v1 映射：task_id→item_id / reward→verdict / gate→gate / \
         turns+model_calls+tokens+http_retries→usage / note"
    );
    (v1, wall_v1)
}

/// ExecOutput 扩展字段断言（§2.2：verdict/record/evidence/工件资格）。
fn assert_exec_output(output: &ExecOutput, parts: &Parts, v1: &Value) {
    assert_eq!(output.verdict, Some(true));
    let mut record = output.record.clone().expect("record 携带 v1 行");
    pop_wall(&mut record, "wall_ms");
    assert_eq!(record, *v1, "record = v1 行同源（双轨不各自装配）");
    let task_json = parts.data_root.join(format!("{TASK}.json"));
    assert_eq!(
        output.evidence,
        vec![
            task_json,
            parts.attempt_dir.join("eval.log"),
            parts.attempt_dir.join("agent.diff"),
        ],
        "evidence = 三件套实际路径"
    );
    for path in &output.evidence {
        assert!(path.is_file(), "证据在盘: {}", path.display());
    }
    assert_eq!(output.note, "judge: {\"resolved\": true}");
    assert_eq!(output.artifacts.len(), 1, "协议一次一个工件");
    assert_eq!(output.artifacts[0].artifact_id, format!("swev-{TASK}"));
    assert!(output.artifacts[0].is_final);
    assert!(
        output.artifacts[0].qualified,
        "resolved=true → 合格终态工件"
    );
}

/// AgentLaunch 形状断言（main.rs:736-798 组装面：提示词/采样覆写/policy/工具/预算）。
fn assert_launch_shape(launch: &CapturedLaunch, attempt_dir: &Path) {
    assert_eq!(launch.attempt_dir, *attempt_dir);
    assert_eq!(launch.run_id, format!("{TASK}-a1"), "main.rs:778 run_id");
    assert_eq!(
        launch.user_message,
        format!(
            "Repository: astropy/astropy\n\nIssue description:\nFix the sampler\n\n{SWEV_INSTANCE_PROTOCOL}"
        ),
        "main.rs:775 prompt + instance 模板"
    );
    assert_eq!(launch.system, af_execline::suite::swev::SWEV_SYSTEM_PROMPT);
    // 采样覆写（main.rs:634-636）+ glm-cp 档案基底（main.rs:59-74）。
    assert_eq!(
        launch.endpoint.base_url,
        "https://open.bigmodel.cn/api/anthropic"
    );
    assert_eq!(launch.endpoint.model, "GLM-5.3-Flash");
    assert_eq!(launch.endpoint.temperature_milli, Some(0));
    assert_eq!(launch.endpoint.top_p_milli, Some(1_000));
    assert_eq!(launch.endpoint.max_tokens, Some(32_000));
    // Policy（main.rs:746-758）。
    assert_eq!(launch.max_turns, 250);
    assert_eq!(launch.completion_signals, vec!["TASK_COMPLETE".to_string()]);
    assert_eq!(launch.tool_free_turn_limit, 3);
    // 工具集（main.rs:742-745：bash 声明，workdir /testbed 进描述）。
    assert_eq!(launch.tool_names, vec!["bash".to_string()]);
    assert!(launch.bash_workdir_in_description.contains("/testbed"));
    // 预算（main.rs:786-793：$3 上限、工具 1 分、账本 300）。
    assert_eq!(launch.spend_cap_cents, 300);
    assert_eq!(launch.tool_spend_cents, 1);
    assert_eq!(launch.budget_ledger_cents, 300);
}

/// 桥调用序列断言（rm 前置 → docker run → capture×2(gate/verify) → cleanup rm）。
fn assert_bridge_sequence(calls: &[(String, String, u32)]) {
    assert_eq!(calls.len(), 5, "恰五次桥调用: {calls:#?}");
    assert_eq!(
        calls[0],
        ("local".into(), format!("docker rm -f {CONTAINER}"), 30)
    );
    assert!(calls[1]
        .1
        .starts_with(&format!("docker run -d --name {CONTAINER}")));
    assert!(calls[1].1.contains("--memory 6g"));
    assert!(calls[1].1.contains("--pids-limit 512"));
    assert!(calls[1]
        .1
        .contains("sweb.eval.x86_64.astropy-12907 sleep infinity"));
    assert_eq!(calls[1].2, 120);
    assert_eq!(calls[2], ("local".into(), CAPTURE_CMD.to_string(), 60));
    assert_eq!(calls[3], ("local".into(), CAPTURE_CMD.to_string(), 60));
    assert_eq!(
        calls[4],
        ("local".into(), format!("docker rm -f {CONTAINER}"), 60)
    );
}

/// 三件套落盘 + 18 键 legacy 行字段级断言 + v1 行映射断言 + AgentLaunch
/// 形状（main.rs:625-914 逐段对照：setup→run→gate→verify→行落盘→cleanup）。
#[test]
fn full_flow_trio_rows_and_launch_shape() {
    with_fake_docker(|| {
        execute_scenario(
            "full",
            FakeBridge::new(),
            FakeRunner::ok(),
            r#"{"resolved": true}"#,
            |parts, outcome| {
                let output = outcome.expect("全流程应 Ok");

                // 三件套落盘 → 18 键 legacy 行 → v1 投影 → ExecOutput 扩展字段。
                let eval_sha = assert_trio_on_disk(&parts.attempt_dir);
                let (_legacy, legacy_wall) = assert_legacy_row(parts, &eval_sha);
                let (v1, _wall_v1) = assert_v1_row(parts, &eval_sha, legacy_wall);
                assert_exec_output(&output, parts, &v1);

                // AgentLaunch 形状 + 桥调用序列。
                let launch = parts.runner.launch();
                assert_launch_shape(&launch, &parts.attempt_dir);
                assert_bridge_sequence(parts.bridge.calls().as_slice());
            },
        )
    })
}

// ---- gate 分支（main.rs:806-813）----

/// 空 diff → no_patch（FakeRunner 回空补丁）。
#[test]
fn gate_no_patch_on_empty_diff() {
    with_fake_docker(|| {
        execute_scenario(
            "nopatch",
            FakeBridge::with_diff(""),
            FakeRunner::ok(),
            r#"{"resolved": true}"#,
            |parts, outcome| {
                outcome.expect("空 diff 仍是完整执行（judge 照跑）");
                let rows = parts.sink.legacy_rows();
                assert_eq!(rows[0]["gate"], json!("no_patch"), "空补丁 → no_patch");
                assert_eq!(rows[0]["reward"], json!(true));
            },
        )
    })
}

/// diff 碰测试文件 → test_edit（gate 记账不是门控：行照落、verdict 照判）。
#[test]
fn gate_test_edit_on_test_file_diff() {
    with_fake_docker(|| {
        execute_scenario(
            "testedit",
            FakeBridge::with_diff(TEST_DIFF),
            FakeRunner::ok(),
            r#"{"resolved": true}"#,
            |parts, outcome| {
                let output = outcome.expect("test_edit 不阻断执行");
                assert_eq!(
                    parts.sink.legacy_rows()[0]["gate"],
                    json!("test_edit"),
                    "diff_touches_tests 命中 → test_edit"
                );
                assert_eq!(
                    output.verdict,
                    Some(true),
                    "gate 只记账，verdict 仍出自 judge"
                );
            },
        )
    })
}

// ---- runner 失败 → run_failed 行 + 容器 cleanup（main.rs:818-845）----

#[test]
fn runner_failure_row_and_container_cleanup() {
    execute_scenario(
        "runfail",
        FakeBridge::new(),
        FakeRunner::failing("端点 401 重取耗尽", 5),
        r#"{"resolved": true}"#,
        |parts, outcome| {
            let err = outcome.expect_err("runner 失败 → Err（订单侧不停终态，EX3001 可重试）");
            assert_eq!(err.code(), ErrorCode::UnitFailed);
            assert_eq!(ErrorCode::UnitFailed.as_str(), "EX3001");

            // run_failed 行 11 键（main.rs:822-834：记实际目录 + http_retries + 失败文案）。
            let rows = parts.sink.legacy_rows();
            assert_eq!(rows.len(), 1);
            let row = &rows[0];
            assert_eq!(row.as_object().unwrap().len(), 11);
            assert_eq!(
                row,
                &json!({
                    "suite": "swe-v",
                    "task_id": TASK,
                    "profile": PROFILE,
                    "trial": false,
                    "reward": null,
                    "status": "run_failed",
                    "scaffold": "v3",
                    "run_dir": parts.attempt_dir.display().to_string(),
                    "eval_sha256": null,
                    "http_retries": 5,
                    "verifier_note": "端点 401 重取耗尽",
                }),
                "run_failed 行字段级对照"
            );
            assert!(parts.sink.v1_rows().is_empty(), "失败行无 v1 投影");
            assert_eq!(parts.runner.launches(), 1, "runner 恰被调用一次");

            // cleanup 必达（RAII guard Drop → docker rm -f）：rm(run 前) + rm(guard)。
            let calls = parts.bridge.calls();
            assert!(calls.len() >= 3, "rm/run/rm 至少三次: {calls:#?}");
            assert_eq!(
                calls.last().unwrap().1,
                format!("docker rm -f {CONTAINER}"),
                "容器清理已调（Drop 断言）"
            );
            // 判分链未启动：三件套零文件（run 失败短路在 verify 之前）。
            assert!(!parts.attempt_dir.join("eval.log").exists());
            assert!(!parts.attempt_dir.join("agent.diff").exists());
        },
    )
}

// ---- setup 失败 → setup_failed 行（main.rs:654-677）----

#[test]
fn setup_failure_row_with_nulls() {
    execute_scenario(
        "setupfail",
        FakeBridge::with_run_failure("docker daemon exploded"),
        FakeRunner::ok(),
        r#"{"resolved": true}"#,
        |parts, outcome| {
            let err = outcome.expect_err("setup 失败 → Err（EX3001 可重试）");
            assert_eq!(err.code(), ErrorCode::UnitFailed);

            // setup_failed 行 9 键：字段齐备含 null（D-20）。
            let rows = parts.sink.legacy_rows();
            assert_eq!(rows.len(), 1);
            let row = &rows[0];
            assert_eq!(row.as_object().unwrap().len(), 9);
            assert_eq!(
                row,
                &json!({
                    "suite": "swe-v",
                    "task_id": TASK,
                    "profile": PROFILE,
                    "trial": false,
                    "reward": null,
                    "status": "setup_failed",
                    "run_dir": null,
                    "eval_sha256": null,
                    "verifier_note": "起容器失败: docker daemon exploded",
                }),
                "setup_failed 行字段级对照（run_dir/eval_sha256 为 null）"
            );
            assert!(parts.sink.v1_rows().is_empty());
            // runner 未被调用、run 目录未建（setup 相在 run 相之前）。
            assert_eq!(parts.runner.launches(), 0);
            assert!(!parts.attempt_dir.exists());
        },
    )
}

// ---- bash 工具行为（main.rs:710-734：反考古护栏 + 重复命令 ≥3 拦截）----

/// FakeBridge 回放命令序列：经捕获的 bash 闭包断言护栏，并核对桥收到的
/// docker exec 命令构造（cd /testbed 前缀）。runner 失败臂驱动（闭包在
/// runner 调用前已组装，捕获不依赖执行成败）。
#[test]
fn bash_tool_guards_archaeology_and_repeats() {
    execute_scenario(
        "bashguard",
        FakeBridge::new(),
        FakeRunner::failing("unused", 0),
        r#"{"resolved": true}"#,
        |parts, _outcome| {
            let bash = parts.runner.launch().bash;

            // 普通命令：桥回 exec-output，输出带 exit 行、无护栏前缀。
            let normal = bash("echo once");
            assert!(normal.contains("exec-output"), "{normal}");
            assert!(normal.contains("exit=0"), "{normal}");
            assert!(!normal.contains("[harness]"), "{normal}");

            // 反考古护栏（main.rs:713-721）：命中 ARCHAEOLOGY_PATTERNS 即注入。
            let arch = bash("git fsck --lost-found");
            assert!(
                arch.starts_with(
                    "[harness] Searching for an upstream fix (git archaeology / full-disk search) \
                     is unavailable in this environment"
                ),
                "{arch}"
            );
            assert!(arch.contains("exec-output"), "原输出在护栏之后: {arch}");

            // 重复命令 ≥3 拦截（main.rs:722-733）：第 3 次同命令 → 教学包装。
            assert!(!bash("echo dup").contains("exact command"));
            assert!(!bash("echo dup").contains("exact command"));
            let third = bash("echo dup");
            assert!(
                third.starts_with("[harness] You have issued this exact command 3 times."),
                "{third}"
            );
            assert!(third.contains("exit=0"), "原输出保留: {third}");

            // 桥收到的 bash 命令构造（tb.rs:213-218：docker exec + cd 前缀 + __AF_RC 标记）。
            // 调用序列：rm(前置)/run/rm(guard Drop) 三次容器面 + 五次 bash
            // （echo once / git fsck / dup×3）。
            let calls = parts.bridge.calls();
            let exec_calls: Vec<&(String, String, u32)> = calls
                .iter()
                .filter(|(_, script, _)| script.starts_with("docker exec"))
                .collect();
            assert_eq!(exec_calls.len(), 5, "五次 bash 调用: {calls:#?}");
            assert_eq!(
                exec_calls[0].1,
                format!(
                    "docker exec {CONTAINER} bash -lc 'cd /testbed && echo once'; echo __AF_RC=$?"
                )
            );
            assert_eq!(exec_calls[0].2, 180, "bash 单命令 180s 超时");
            assert!(exec_calls[1]
                .1
                .contains("'cd /testbed && git fsck --lost-found'"));
        },
    )
}

// ---- 装载期指纹门（§3.2-2 双重防线：单元装载 + verify 入口）----

#[test]
fn unit_load_fingerprint_gate_fail_loud() {
    let root = temp_root("loadgate");
    let judge = fake_judge("loadgate-j", r#"{"resolved": true}"#);
    // 造改字节的脚本副本：锚仍声明原脚本 → 装载即拒（EX3002）。
    let tampered = root.join("judge-tampered.py");
    std::fs::write(&tampered, b"# fixture judge script bytes TAMPERED\n").unwrap();
    let bad_spec = SwevUnitSpec::fix2(
        PROFILE,
        JudgeLocator {
            script: tampered,
            ..judge.clone()
        },
    );

    let bridge = Arc::new(FakeBridge::new());
    let runner = Arc::new(FakeRunner::ok());
    let err = SwevExecuteUnit::new(bad_spec, bridge.clone(), runner.clone())
        .expect_err("装载期指纹门 fail-loud");
    assert_eq!(err.code(), ErrorCode::JudgeFingerprintMismatch);
    assert!(err.to_string().starts_with("EX3002:"), "{err}");
    assert!(bridge.calls().is_empty(), "拒跑：桥零调用");
    assert_eq!(runner.launches(), 0, "拒跑：runner 零调用");

    // 锚匹配 → 装载成功。
    let good_spec = SwevUnitSpec::fix2(PROFILE, judge);
    let unit = SwevExecuteUnit::new(good_spec, bridge, runner).expect("锚匹配装载成功");
    assert_eq!(unit.unit_id(), "swev-unit");
}

// ---- sink 写失败 → EX3001 fail-loud（不静默丢行）----

#[test]
fn sink_write_failure_fails_loud() {
    with_fake_docker(|| {
        let root = temp_root("sinkfail");
        let data_root = data_root_with_task(&root);
        let attempt_dir = fresh_attempt_dir(&root);
        let judge = fake_judge("sinkfail-j", r#"{"resolved": true}"#);
        let spec = SwevUnitSpec::fix2(PROFILE, judge);
        let bridge = Arc::new(FakeBridge::new());
        let runner = Arc::new(FakeRunner::ok());
        let unit = SwevExecuteUnit::new(spec, bridge, runner).expect("装载");
        let sink = FailingSink;
        let ctx = ExecContext {
            order_id: "wo-p4",
            stable_id: "sid-p4",
            line_id: "line-p4",
            out_dir: &root,
            task_id: TASK,
            host_alias: "local",
            data_root: &data_root,
            attempt_dir: &attempt_dir,
            record_sink: &sink,
            graph_version: None,
            task_sha256: None,
            trial: false,
        };
        let err = unit.execute(&ctx).expect_err("行写失败必须上抛");
        assert_eq!(err.code(), ErrorCode::UnitFailed);
        assert!(
            err.to_string().contains("设备满"),
            "sink 错误文案透传: {err}"
        );
        // 判分链已完成（先产证据后落行）：eval.log 在盘。
        assert!(attempt_dir.join("eval.log").is_file());
    })
}

// ---- record 纯函数：失败行键集 + note 截断（main.rs:656-667/822-834/889 口径）----

#[test]
fn failure_rows_key_sets_and_note_truncation() {
    let long_note = "x".repeat(400);

    let setup = swev_setup_failed_row(TASK, PROFILE, false, &long_note);
    assert_eq!(setup.as_object().unwrap().len(), 9);
    assert_eq!(
        setup["verifier_note"].as_str().unwrap().len(),
        300,
        "截断至 300 字符"
    );
    assert_eq!(setup["status"], json!("setup_failed"));
    assert_eq!(setup["run_dir"], json!(null));

    let run_dir = PathBuf::from("/tmp/runs/swev-x-a1");
    let failed = swev_run_failed_row(TASK, PROFILE, true, &run_dir, 7, &long_note);
    assert_eq!(failed.as_object().unwrap().len(), 11);
    assert_eq!(failed["verifier_note"].as_str().unwrap().len(), 300);
    assert_eq!(failed["run_dir"], json!(run_dir.display().to_string()));
    assert_eq!(failed["http_retries"], json!(7));

    // legacy 18 键行同源截断（main.rs:889）。
    let facts = ExecutionFacts {
        suite: "swe-v",
        task_id: TASK.to_string(),
        profile: PROFILE.to_string(),
        trial: false,
        reward: None,
        status: "run_failed".into(),
        scaffold: "v3",
        reasoning_tier: "off",
        run_dir: Some(run_dir.display().to_string()),
        eval_sha256: None,
        gate: "ok".into(),
        attempts: 1,
        http_retries: 0,
        turns: 0,
        model_calls: 0,
        tokens: TokenTotals::default(),
        agent_wall_ms: 0,
        verifier_note: long_note.clone(),
    };
    let legacy = swev_legacy_row(&facts);
    assert_eq!(legacy.as_object().unwrap().len(), 18);
    assert_eq!(legacy["verifier_note"].as_str().unwrap().len(), 300);
    assert_eq!(legacy["reward"], json!(null), "None 判定 → null");

    // v1 投影键集（契约 §1.2 行形状）。
    let v1 = swev_v1_row(&facts, "plan-1", "node-1", "anolis", "swev-unit", "0.1.0");
    assert_eq!(v1.as_object().unwrap().len(), 14);
    assert_eq!(v1["plan_id"], json!("plan-1"));
    assert_eq!(v1["item_id"], json!(TASK));
    assert_eq!(v1["verdict"], json!(null));
    assert_eq!(v1["host_alias"], json!("anolis"));
    assert_eq!(v1["note"].as_str().unwrap().len(), 300, "v1 note 同源截断");
}

// ---- 驱动层占位 sink：误用 fail-loud（P5 接线前的防线）----

#[test]
fn pending_driver_sink_fails_loud() {
    let sink = PendingRecordSink;
    let err = RecordSink::append_legacy(&sink, "{}").expect_err("占位 sink 不得静默丢行");
    assert!(err.contains("P5"), "文案指向接线任务包: {err}");
    let err = RecordSink::append_v1(&sink, "{}").expect_err("占位 sink 不得静默丢行");
    assert!(err.contains("P5"), "文案指向接线任务包: {err}");
}
