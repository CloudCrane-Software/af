//! P5 TbExecuteUnit 验收测试（af-bench 吸收方案 §5-P5 草案 TB 部分）。
//!
//! 先红后绿（ADR-007）：本文件先于实现提交，桩 `unimplemented!()` 下必红。
//!
//! 行为对照锚 = af-bench run_tb（main.rs:916-1109）逐段 + TB verify 容器链
//! （tb.rs:224-292 原样搬入）：TB 与 swe-v 的**原样差异**逐条钉死——
//! 无采样覆写（main.rs:628-631"TB 不动"）/ 无 bash 护栏（main.rs:963-970）/
//! user_message 裸 prompt（main.rs:1015）/ run_id 无 -a 后缀（main.rs:1013）/
//! 40 步 $1（main.rs:981/1021-1027）/ 12 键行（main.rs:1068-1085）/
//! verify 无 reward.txt → infra 行（tb.rs:284-288）。
//!
//! 注入手法与 p4_unit 同源：FakeRunner（零模型/零凭证）+ FakeBridge（TB 命令
//! 形状智能分派，verify 容器链全回放，零真容器/零 ssh/零网络）。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use af_execline::agent::{AgentLaunch, AgentRunFailure, AgentRunOutcome, AgentRunner};
use af_execline::error::ErrorCode;
use af_execline::exec::{ExecContext, ExecuteUnit};
use af_execline::hostbridge::HostBridge;
use af_execline::record::{RecordSink, TbExecutionFacts};
use af_execline::suite::tb::TB_SYSTEM_PROMPT;
use af_execline::suite::tb_unit::{TbExecuteUnit, TbUnitSpec};
use af_execline::ExecOutput;
use af_harness::{EndpointSpec, RunReport, RunStatus, TokenTotals};
use serde_json::{json, Value};

const TB_TASK: &str = "hello-world";
const TB_PROFILE: &str = "glm-cp";
const TB_CONTAINER: &str = "afbench-hello-world-glm-cp";
const TB_VOL: &str = "/data/bench/vol/hello-world-afbench-hello-world-glm-cp";
const INSTRUCTION: &str = "Fix the greeting bug in /app.\n";
const VERIFY_LOG: &str = "verifier stdout line";

// ---- 公共助手 ----

fn temp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("af-execline-p5tb-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// TB 题库根：<data_root>/<task>/{instruction.md, environment/Dockerfile, tests/test.sh}
/// （tb.rs:12-36 load_tb_tasks 的装载面）。
fn tb_data_root(root: &Path) -> PathBuf {
    let data = root.join("tbdata");
    let dir = data.join(TB_TASK);
    std::fs::create_dir_all(dir.join("environment")).unwrap();
    std::fs::create_dir_all(dir.join("tests")).unwrap();
    std::fs::write(dir.join("instruction.md"), INSTRUCTION).unwrap();
    std::fs::write(
        dir.join("environment").join("Dockerfile"),
        "FROM python:3.12-slim\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("tests").join("test.sh"),
        "#!/bin/bash\necho verify\n",
    )
    .unwrap();
    data
}

// ---- FakeBridge（TB 命令形状分派）----

type BridgeResult = Result<(bool, String), String>;

/// TB 假桥：setup 建容器 / bash 工具 / verify 容器链全回放，全调用留痕。
/// `run_result` = `docker run -d`（setup 起容器）结果，Ok((false, log)) → setup 失败臂；
/// `reward` = reward.txt 内容（"1"/"0"/"missing"）。
struct FakeBridge {
    calls: Mutex<Vec<(String, String, u32)>>,
    run_result: BridgeResult,
    reward: String,
}

impl FakeBridge {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            run_result: Ok((true, String::new())),
            reward: "1".into(),
        }
    }

    fn with_run_failure(log: &str) -> Self {
        Self {
            run_result: Ok((false, log.to_string())),
            ..Self::new()
        }
    }

    fn with_reward(reward: &str) -> Self {
        Self {
            reward: reward.to_string(),
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
        } else if script.contains("bash /tests/test.sh") {
            // verify：验证器容器执行（docker run --rm ... bash /tests/test.sh）。
            Ok((true, VERIFY_LOG.into()))
        } else if script.contains("--entrypoint sh python:3.12-slim") {
            // verify：旧卷清理（docker run --rm ... python:3.12-slim）。
            Ok((true, "cleaned".into()))
        } else if script.starts_with("mkdir -p") {
            // verify：提 /app（mkdir -p ... && docker cp ...）。
            Ok((true, "extracted".into()))
        } else if script.starts_with("cat /data/bench/vol/") {
            // verify：读 reward.txt。
            Ok((true, self.reward.clone()))
        } else if script.contains("docker build") {
            // setup 建镜像 + verify 建验证器镜像。
            Ok((true, "sha256:fake".into()))
        } else if script.starts_with("docker run -d") {
            // setup 起任务容器。
            self.run_result.clone()
        } else if script.starts_with("docker exec") {
            // bash 工具体（TbEnv::exec 消费）：带 __AF_RC 标记行。
            Ok((true, "exec-output\n__AF_RC=0".to_string()))
        } else {
            panic!("FakeBridge 未预期的命令: {script}")
        }
    }
}

// ---- FakeRunner（AgentRunner 缝，捕获 TB 组装面）----

struct CapturedLaunch {
    run_id: String,
    user_message: String,
    system: String,
    endpoint: EndpointSpec,
    max_turns: u32,
    tool_free_turn_limit: u32,
    tool_workdir_in_description: String,
    spend_cap_cents: i64,
    tool_spend_cents: i64,
    budget_ledger_cents: i64,
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
                run_id: launch.run_id.clone(),
                user_message: launch.user_message.clone(),
                system: launch.spec.context_template.system.clone(),
                endpoint: launch.spec.endpoint.clone(),
                max_turns: launch.spec.policy.max_turns,
                tool_free_turn_limit: launch.spec.policy.stop.tool_free_turn_limit,
                tool_workdir_in_description: launch
                    .spec
                    .toolset
                    .first()
                    .map(|tool| tool.description.clone())
                    .unwrap_or_default(),
                spend_cap_cents: launch.spend_cap_cents,
                tool_spend_cents: launch.tool_spend_cents,
                budget_ledger_cents: launch.budget_ledger_cents,
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

// ---- FakeSink（RecordSink 缝）----

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

// ---- 场景装配 ----

/// 一次 TB 单题执行的全部材料；`check` 在 ctx 借用本地件内完成全部断言。
fn tb_scenario<R>(
    tag: &str,
    bridge: FakeBridge,
    runner: FakeRunner,
    check: impl FnOnce(
        &Path,
        &Path,
        &FakeSink,
        &FakeBridge,
        &FakeRunner,
        Result<ExecOutput, af_execline::ExeclineError>,
    ) -> R,
) -> R {
    let root = temp_root(tag);
    let data_root = tb_data_root(&root);
    let attempt_dir = root.join("runs").join(format!("tb-{TB_TASK}-{TB_PROFILE}"));
    let sink = FakeSink::default();
    let spec = TbUnitSpec::fix2(TB_PROFILE);
    let bridge = Arc::new(bridge);
    let runner = Arc::new(runner);
    let unit =
        TbExecuteUnit::new(spec, bridge.clone(), runner.clone()).expect("单元装载（profile 检查）");
    let ctx = ExecContext {
        order_id: "wo-p5-tb",
        stable_id: "sid-p5-tb",
        line_id: "line-p5",
        out_dir: &root,
        task_id: TB_TASK,
        host_alias: "local",
        data_root: &data_root,
        attempt_dir: &attempt_dir,
        record_sink: &sink,
        trial: false,
    };
    let outcome = unit.execute(&ctx);
    check(&data_root, &attempt_dir, &sink, &bridge, &runner, outcome)
}

/// 弹出行的 wall 字段（agent_wall_ms），返回 (余下对象, 值)。
fn pop_wall(row: &mut Value) -> u64 {
    let value = row
        .as_object_mut()
        .unwrap()
        .remove("agent_wall_ms")
        .expect("行必须带 agent_wall_ms");
    value.as_u64().expect("wall 字段是 u64 毫秒数")
}

// ---------- 主链：TB 单题全流程（12 键行 + launch 形状 + verify 容器链）----------

#[test]
fn tb_full_flow_row_launch_shape_and_verify_chain() {
    tb_scenario(
        "full",
        FakeBridge::new(),
        FakeRunner::ok(),
        |data_root, attempt_dir, sink, bridge, runner, outcome| {
            let output = outcome.expect("全流程应 Ok");

            // 12 键 legacy 行字段级对照（main.rs:1068-1085）。
            let rows = sink.legacy_rows();
            assert_eq!(rows.len(), 1, "恰一行 legacy");
            let mut row = rows.into_iter().next().unwrap();
            assert_eq!(row.as_object().unwrap().len(), 12, "12 键齐备");
            let wall = pop_wall(&mut row);
            assert!(wall < 60_000, "墙钟毫秒数合理: {wall}");
            assert_eq!(
                row,
                json!({
                    "suite": "terminal-bench",
                    "task_id": TB_TASK,
                    "profile": TB_PROFILE,
                    "trial": false,
                    "reward": true,
                    "status": "completed",
                    "http_retries": 2,
                    "turns": 3,
                    "model_calls": 5,
                    "tokens": {"input": 100, "output": 50, "cache_read": 10},
                    "verifier_note": VERIFY_LOG,
                }),
                "TB 12 键行字段级对照（verifier_note = verify 容器链 run_log 尾部）"
            );
            // TB 无 v1 投影（方案 §1.4 映射表面向 swe-v 18 键行；登记 RESULT-P5）。
            assert!(sink.v1_rows().is_empty(), "TB 无 v1 投影");

            // ExecOutput：verdict 只出自 verify 容器链；reward=1 → 合格终态工件；
            // 无本地判分三件套（TB 判分是容器链，evidence 空——不虚列）。
            assert_eq!(output.verdict, Some(true));
            assert_eq!(output.artifacts.len(), 1);
            assert_eq!(output.artifacts[0].artifact_id, format!("tb-{TB_TASK}"));
            assert!(output.artifacts[0].is_final);
            assert!(output.artifacts[0].qualified, "reward=1 → 合格终态工件");
            assert_eq!(output.evidence, Vec::<PathBuf>::new(), "TB 无本地三件套");
            assert_eq!(output.record, None, "无 v1 行随行");

            // AgentLaunch 形状（main.rs:971-992/1013-1015 TB 原样差异）。
            let launch = runner.launch();
            assert_eq!(launch.run_id, TB_TASK, "main.rs:1013 run_id 无 -a 后缀");
            assert_eq!(
                launch.user_message, INSTRUCTION,
                "main.rs:1015 user_message = 裸 prompt（无 instance 模板拼接）"
            );
            assert_eq!(launch.system, TB_SYSTEM_PROMPT);
            // 无采样覆写（main.rs:628-631"只作用于 swe-v 线；TB 不动"）：
            // glm-cp 档案基底三者皆 None（profile.rs:59-74）。
            assert_eq!(
                launch.endpoint.base_url,
                "https://open.bigmodel.cn/api/anthropic"
            );
            assert_eq!(launch.endpoint.model, "GLM-5.3-Flash");
            assert_eq!(launch.endpoint.temperature_milli, None, "TB 不覆写温度");
            assert_eq!(launch.endpoint.top_p_milli, None, "TB 不覆写 top_p");
            assert_eq!(launch.endpoint.max_tokens, None, "TB 不覆写 max_tokens");
            assert_eq!(launch.max_turns, 40, "main.rs:981 TB 40 步");
            assert_eq!(launch.tool_free_turn_limit, 3);
            assert!(
                launch.tool_workdir_in_description.contains("/app"),
                "workdir /app 进描述"
            );
            // 预算（main.rs:1021-1027：$1 上限、工具 1 分、账本 100）。
            assert_eq!(launch.spend_cap_cents, 100);
            assert_eq!(launch.tool_spend_cents, 1);
            assert_eq!(launch.budget_ledger_cents, 100);

            // bash 工具无护栏（main.rs:963-970：TB 注册处无反考古/重复包装）；
            // 题库任务包仍在盘（instruction.md 装载源）。
            assert!(data_root.join(TB_TASK).join("instruction.md").is_file());

            // verify 容器链调用序列（tb.rs:224-292 原样：清卷→提取→建验证器→
            // 跑 test.sh→读 reward）+ setup（build→rm→run -d）+ cleanup（guard rm）。
            let calls = bridge.calls();
            assert_eq!(calls.len(), 9, "恰九次桥调用: {calls:#?}");
            let hosts: Vec<&str> = calls.iter().map(|(h, _, _)| h.as_str()).collect();
            let scripts: Vec<&str> = calls.iter().map(|(_, s, _)| s.as_str()).collect();
            let timeouts: Vec<u32> = calls.iter().map(|(_, _, t)| *t).collect();
            assert!(hosts.iter().all(|h| *h == "local"));
            // 0: setup 建任务镜像（tb.rs:146-169）。
            assert!(scripts[0].contains("docker build -q --network host"));
            assert!(scripts[0].contains(&format!("-t afbench-tbenv-{TB_TASK}")));
            assert!(scripts[0].ends_with(&format!("/data/bench/tb/{TB_TASK}/environment")));
            assert_eq!(timeouts[0], 1800);
            // 1: setup 前置 rm；2: setup 起容器（2g/256 护栏，tb.rs:174-189）。
            assert_eq!(scripts[1], format!("docker rm -f {TB_CONTAINER}"));
            assert_eq!(timeouts[1], 30);
            assert!(scripts[2].starts_with(&format!("docker run -d --name {TB_CONTAINER}")));
            assert!(scripts[2].contains("--memory 2g"));
            assert!(scripts[2].contains("--pids-limit 256"));
            assert!(scripts[2].contains("sleep infinity"));
            assert_eq!(timeouts[2], 60);
            // 3: verify 清旧卷（tb.rs:227-235：vol = {task}-{container}，容器内 root 清）。
            assert!(scripts[3].contains(&format!("-v {TB_VOL}:/vol")));
            assert!(scripts[3].contains("--entrypoint sh python:3.12-slim"));
            assert!(scripts[3].contains("rm -rf /vol/* /vol/.[!.]*; chmod 777 /vol"));
            assert_eq!(timeouts[3], 120);
            // 4: verify 提 /app（tb.rs:236-247）。
            assert_eq!(
                scripts[4],
                format!("mkdir -p {TB_VOL}/app {TB_VOL}/logs && docker cp {TB_CONTAINER}:/app/. {TB_VOL}/app/")
            );
            assert_eq!(timeouts[4], 300);
            // 5: verify 建验证器镜像（tb.rs:248-266：afbench-tbver-<task>，tests 目录）。
            assert!(scripts[5].contains("docker build -q --network host"));
            assert!(scripts[5].contains(&format!("-t afbench-tbver-{TB_TASK}")));
            assert!(scripts[5].ends_with(&format!("/data/bench/tb/{TB_TASK}/tests")));
            assert_eq!(timeouts[5], 1800);
            // 6: verify 跑 test.sh（tb.rs:267-276：挂 app/logs，2g/256）。
            assert_eq!(
                scripts[6],
                format!(
                    "docker run --rm --memory 2g --pids-limit 256 -v {TB_VOL}/app:/app -v {TB_VOL}/logs:/logs afbench-tbver-{TB_TASK} bash /tests/test.sh"
                )
            );
            assert_eq!(timeouts[6], 1200);
            // 7: verify 读 reward.txt（tb.rs:277-292）。
            assert_eq!(
                scripts[7],
                format!("cat {TB_VOL}/logs/verifier/reward.txt 2>/dev/null || echo missing")
            );
            assert_eq!(timeouts[7], 30);
            // 8: cleanup（RAII guard Drop → docker rm -f）。
            assert_eq!(scripts[8], format!("docker rm -f {TB_CONTAINER}"));
            assert_eq!(timeouts[8], 60);

            // run 目录已建（main.rs:1002 create_dir_all；覆写在驱动层布局，R8）。
            assert!(attempt_dir.is_dir());
        },
    )
}

// ---------- TB verify 无 reward.txt → infra 行（tb.rs:284-288）----------

#[test]
fn tb_verify_missing_reward_is_infra_row() {
    tb_scenario(
        "infra",
        FakeBridge::with_reward("missing"),
        FakeRunner::ok(),
        |_data_root, _attempt_dir, sink, _bridge, _runner, outcome| {
            let output =
                outcome.expect("infra 不是错误：行照落、执行 Ok（订单侧自然进不了 close 门）");
            assert_eq!(output.verdict, None, "reward.txt 缺 → 无判定");
            assert!(!output.artifacts[0].qualified, "infra → 非合格终态工件");

            let rows = sink.legacy_rows();
            assert_eq!(rows.len(), 1);
            let row = &rows[0];
            assert_eq!(
                row["reward"],
                json!(null),
                "infra 行 reward=null（不占分母）"
            );
            assert_eq!(row["status"], json!("completed"), "status 仍是 runner 终态");
            let note = row["verifier_note"].as_str().unwrap();
            assert!(
                note.starts_with("无 reward.txt；验证器输出:"),
                "tb.rs:284-288 文案: {note}"
            );
            assert!(
                note.contains("verifier stdout line"),
                "验证器输出随行: {note}"
            );
        },
    )
}

/// reward=0 → Some(false)：真失败（占分母），非 infra。
#[test]
fn tb_verify_reward_zero_is_failure_not_infra() {
    tb_scenario(
        "fail",
        FakeBridge::with_reward("0"),
        FakeRunner::ok(),
        |_data_root, _attempt_dir, sink, _bridge, _runner, outcome| {
            let output = outcome.expect("reward=0 是完整执行");
            assert_eq!(output.verdict, Some(false));
            assert!(!output.artifacts[0].qualified);
            assert_eq!(sink.legacy_rows()[0]["reward"], json!(false));
            assert_eq!(sink.legacy_rows()[0]["verifier_note"], json!(VERIFY_LOG));
        },
    )
}

// ---------- setup 失败 → 7 键 setup_failed 行（main.rs:935-957）----------

#[test]
fn tb_setup_failure_row() {
    tb_scenario(
        "setupfail",
        FakeBridge::with_run_failure("docker daemon exploded"),
        FakeRunner::ok(),
        |_data_root, attempt_dir, sink, _bridge, runner, outcome| {
            let err = outcome.expect_err("setup 失败 → EX3001（订单侧不停终态，可重试）");
            assert_eq!(err.code(), ErrorCode::UnitFailed);
            assert_eq!(ErrorCode::UnitFailed.as_str(), "EX3001");

            let rows = sink.legacy_rows();
            assert_eq!(rows.len(), 1);
            let row = &rows[0];
            assert_eq!(row.as_object().unwrap().len(), 7, "TB setup_failed 行 7 键");
            assert_eq!(
                row,
                &json!({
                    "suite": "terminal-bench",
                    "task_id": TB_TASK,
                    "profile": TB_PROFILE,
                    "trial": false,
                    "reward": null,
                    "status": "setup_failed",
                    "verifier_note": format!("起容器 {TB_CONTAINER} 失败: docker daemon exploded"),
                }),
                "main.rs:938-946 字段级对照（TbEnv::setup 的失败文案进 note）"
            );
            assert!(sink.v1_rows().is_empty());
            assert_eq!(runner.launches(), 0, "runner 未被调用");
            assert!(
                !attempt_dir.exists(),
                "run 目录未建（setup 相在 run 相之前）"
            );
        },
    )
}

/// 任务装载失败（题库缺 instruction.md）同样归 setup_failed 行
/// （af-bench main.rs:921 原为批级 ? 中止无行；单元化口径同 P4 RESULT §3-5）。
#[test]
fn tb_task_load_failure_is_setup_failed_row() {
    let root = temp_root("loadfail");
    let empty_data = root.join("no-such-data-root"); // 整个题库目录缺席
    let attempt_dir = root.join("runs").join("tb");
    let sink = FakeSink::default();
    let unit = TbExecuteUnit::new(
        TbUnitSpec::fix2(TB_PROFILE),
        Arc::new(FakeBridge::new()),
        Arc::new(FakeRunner::ok()),
    )
    .expect("装载");
    let ctx = ExecContext {
        order_id: "wo-p5-tb",
        stable_id: "sid-p5-tb",
        line_id: "line-p5",
        out_dir: &root,
        task_id: TB_TASK,
        host_alias: "local",
        data_root: &empty_data,
        attempt_dir: &attempt_dir,
        record_sink: &sink,
        trial: false,
    };
    let err = unit.execute(&ctx).expect_err("装载失败 → EX3001");
    assert_eq!(err.code(), ErrorCode::UnitFailed);
    let row = &sink.legacy_rows()[0];
    assert_eq!(row["status"], json!("setup_failed"));
    assert!(
        row["verifier_note"]
            .as_str()
            .unwrap()
            .contains("instruction.md"),
        "装载失败文案透传"
    );
}

// ---------- runner 失败 → 8 键 run_failed 行 + cleanup（main.rs:1034-1057）----------

#[test]
fn tb_run_failure_row_and_container_cleanup() {
    tb_scenario(
        "runfail",
        FakeBridge::new(),
        FakeRunner::failing("供给 5xx 重试耗尽", 4),
        |_data_root, _attempt_dir, sink, bridge, runner, outcome| {
            let err = outcome.expect_err("runner 失败 → EX3001（EX3001 可重试）");
            assert_eq!(err.code(), ErrorCode::UnitFailed);

            // run_failed 行 8 键（main.rs:1037-1046）。
            let rows = sink.legacy_rows();
            assert_eq!(rows.len(), 1);
            let row = &rows[0];
            assert_eq!(row.as_object().unwrap().len(), 8);
            assert_eq!(
                row,
                &json!({
                    "suite": "terminal-bench",
                    "task_id": TB_TASK,
                    "profile": TB_PROFILE,
                    "trial": false,
                    "reward": null,
                    "status": "run_failed",
                    "http_retries": 4,
                    "verifier_note": "供给 5xx 重试耗尽",
                }),
                "run_failed 行字段级对照"
            );
            assert!(sink.v1_rows().is_empty());
            assert_eq!(runner.launches(), 1, "runner 恰被调用一次");

            // cleanup 必达（RAII guard Drop → docker rm -f）；verify 链未启动。
            let calls = bridge.calls();
            assert_eq!(
                calls.last().unwrap().1,
                format!("docker rm -f {TB_CONTAINER}"),
                "容器清理已调（Drop 断言）"
            );
            assert!(
                !calls
                    .iter()
                    .any(|(_, s, _)| s.contains("bash /tests/test.sh")),
                "判分链未启动（run 失败短路在 verify 之前）"
            );
        },
    )
}

// ---- record 纯函数：TB 行键集与 note 截断（main.rs:938-946/1037-1046/1068-1085）----

#[test]
fn tb_row_key_sets_and_note_truncation() {
    let long_note = "x".repeat(400);

    let setup = af_execline::record::tb_setup_failed_row(TB_TASK, TB_PROFILE, false, &long_note);
    assert_eq!(setup.as_object().unwrap().len(), 7);
    assert_eq!(
        setup["verifier_note"].as_str().unwrap().len(),
        300,
        "截断至 300"
    );
    assert_eq!(setup["reward"], json!(null));

    let failed = af_execline::record::tb_run_failed_row(TB_TASK, TB_PROFILE, true, 7, &long_note);
    assert_eq!(failed.as_object().unwrap().len(), 8);
    assert_eq!(failed["verifier_note"].as_str().unwrap().len(), 300);
    assert_eq!(failed["http_retries"], json!(7));
    assert_eq!(failed["trial"], json!(true));

    let facts = TbExecutionFacts {
        task_id: TB_TASK.to_string(),
        profile: TB_PROFILE.to_string(),
        trial: false,
        reward: None,
        status: "run_failed".into(),
        http_retries: 0,
        turns: 0,
        model_calls: 0,
        tokens: TokenTotals::default(),
        agent_wall_ms: 0,
        verifier_note: long_note.clone(),
    };
    let row = af_execline::record::tb_legacy_row(&facts);
    assert_eq!(row.as_object().unwrap().len(), 12);
    assert_eq!(row["verifier_note"].as_str().unwrap().len(), 300);
    assert_eq!(row["reward"], json!(null), "None 判定 → null");
    assert_eq!(row["suite"], json!("terminal-bench"));
}
