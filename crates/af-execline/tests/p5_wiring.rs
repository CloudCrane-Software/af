//! P5 订单生命周期接线验收测试（af-bench 吸收方案 §5-P5 草案）。
//!
//! 先红后绿（ADR-007）：本文件先于实现提交，桩 `unimplemented!()` 下必红。
//!
//! 覆盖（草案逐项）：
//! - **全生命周期**（真引擎子进程 + FakeRunner + FakeBridge）：8 op 入账序号
//!   连续、MES 四类各一、results 行与三件套落盘（对齐 lifecycle.rs:1-11 口径）；
//! - **同 order_id 二次 drive**：replayed=true、单元不重跑（runner 零新增调用）、
//!   close 成功记录唯一（driver.rs:207-215 幂等语义对真实单元成立）；
//! - **TB 目录覆写不对称回归注记**（main.rs:1000-1002 vs 594-610，R8 保留）；
//! - RecordSink 文件实现（append + 每行 flush；跨实例续写同文件）。
//!
//! 注入手法与 p4_unit 同源：FakeRunner（零模型/零凭证）+ FakeBridge（零容器/
//! 零 ssh）+ 假 docker（verify eval 执行链真子进程）+ 假判分 venv（judge 真拉起）。
//! 引擎不打桩（EngineClient::locate 定位/构建真 af-engine）。

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Arc, Mutex, OnceLock};

use af_execline::suite::swev_unit::{SwevExecuteUnit, SwevUnitSpec};
use af_execline::{
    judge::JudgeLocator, AttemptLayout, DriverConfig, EngineClient, FileRecordSink, FixedClock,
    MesFileWriter, OrderLedger, OrderLifecycleDriver, RecordSink,
};
use af_mes::MesEvent;
use af_workorder::{BudgetVoucher, LineBinding, Priority, Sla};

const TASK: &str = "astropy__astropy-12907";
const PROFILE: &str = "glm-cp";
const FIXED_MS: i64 = 1_767_225_600_000;

// ---- 公共助手（lifecycle.rs 引擎定位 + p4_unit 注入手法）----

static SPAWN_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

fn guard() -> &'static Mutex<()> {
    SPAWN_GUARD.get_or_init(|| Mutex::new(()))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace 根可解析")
}

fn target_dir() -> PathBuf {
    match std::env::var("CARGO_TARGET_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => workspace_root().join("target"),
    }
}

fn engine_client() -> EngineClient {
    EngineClient::locate(&workspace_root(), &target_dir())
        .expect("af-engine 二进制定位/构建失败：验收要求真引擎子进程，不打桩、不静默跳过")
}

fn temp_root(tag: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("af-execline-p5wiring-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn write_exec(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// 独立 sha256 oracle（不借用被测实现的哈希）。
fn sha256_of(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let data = std::fs::read(path).unwrap();
    let digest = Sha256::digest(&data);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 题库根 + 任务包（p4_unit 同款）。
fn data_root_with_task(root: &Path) -> PathBuf {
    let data_root = root.join("data");
    std::fs::create_dir_all(&data_root).unwrap();
    std::fs::write(
        data_root.join(format!("{TASK}.json")),
        serde_json::json!({
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

/// 假判分 venv（p3/p4 手法）：假 python 回一行 JSON；锚 = 脚本真实 sha256。
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

/// 假 docker（PATH 首段注入，p4 手法）：吃 stdin 落证、出一行日志、退出 0。
fn with_fake_docker<R>(f: impl FnOnce() -> R) -> R {
    let _lock = guard().lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("af-execline-p5-fakebin-{}", process::id()));
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

// ---- FakeBridge / FakeRunner（p4_unit 同款，收敛到本包用面）----

struct FakeBridge {
    calls: Mutex<Vec<(String, String, u32)>>,
}

impl FakeBridge {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<(String, String, u32)> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl af_execline::HostBridge for FakeBridge {
    fn run(&self, host: &str, script: &str, timeout_sec: u32) -> Result<(bool, String), String> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).push((
            host.to_string(),
            script.to_string(),
            timeout_sec,
        ));
        if script.starts_with("docker rm -f") || script.starts_with("docker run") {
            Ok((true, String::new()))
        } else if script.contains("git -c core.fileMode=false diff") {
            Ok((
                true,
                "diff --git a/astropy/sampler.py b/astropy/sampler.py\n--- a/a\n+++ b/b\n".into(),
            ))
        } else if script.starts_with("docker exec") {
            Ok((true, "exec-output\n__AF_RC=0".to_string()))
        } else {
            panic!("FakeBridge 未预期的命令: {script}")
        }
    }
}

/// FakeRunner：只计数 + 固定报告（launch 全形状断言已在 p4_unit 钉死）。
struct FakeRunner {
    launches: Mutex<usize>,
}

impl FakeRunner {
    fn new() -> Self {
        Self {
            launches: Mutex::new(0),
        }
    }

    fn launches(&self) -> usize {
        *self.launches.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl af_execline::AgentRunner for FakeRunner {
    fn run(
        &self,
        _launch: af_execline::AgentLaunch<'_>,
    ) -> Result<af_execline::AgentRunOutcome, af_execline::AgentRunFailure> {
        let mut count = self.launches.lock().unwrap_or_else(|e| e.into_inner());
        *count += 1;
        Ok(af_execline::AgentRunOutcome {
            report: af_harness::RunReport {
                run_id: "fake-run".into(),
                status: af_harness::RunStatus::Completed {
                    signal: "TASK_COMPLETE".into(),
                },
                turns: 3,
                model_calls: 5,
                effect_replays: 0,
                effect_fresh: 2,
                transcript: Vec::new(),
                whitelist: vec!["bash".into()],
                tokens: af_harness::TokenTotals {
                    input: 100,
                    output: 50,
                    cache_read: 10,
                },
            },
            http_retries: 2,
        })
    }
}

fn swev_unit(tag: &str, bridge: Arc<FakeBridge>, runner: Arc<FakeRunner>) -> SwevExecuteUnit {
    let judge = fake_judge(&format!("p5wiring-{tag}-judge"), r#"{"resolved": true}"#);
    let spec = SwevUnitSpec::fix2(PROFILE, judge);
    SwevExecuteUnit::new(spec, bridge, runner).expect("单元装载（指纹门 + profile）")
}

fn swev_config(order_id: &str, out_dir: PathBuf, data_root: PathBuf) -> DriverConfig {
    DriverConfig {
        task_sha256: None,
        order_id: order_id.to_string(),
        graph_version: "g-p5-1".to_string(),
        sla: Sla::new(None, Priority::Normal),
        line: LineBinding {
            line_id: "line-p5".into(),
            line_version: "lv-1".into(),
        },
        voucher: BudgetVoucher {
            voucher_id: format!("bv-{order_id}"),
            amount: 500,
            signed: true,
        },
        out_dir,
        station_id: "st-p5".to_string(),
        host_alias: "local".into(),
        data_root,
        task_id: TASK.to_string(),
        trial: false,
        profile: PROFILE.to_string(),
        attempt_layout: AttemptLayout::SwevIncrement,
    }
}

fn open_driver(order_id: &str, out_dir: PathBuf, data_root: PathBuf) -> OrderLifecycleDriver {
    OrderLifecycleDriver::open(
        swev_config(order_id, out_dir, data_root),
        engine_client(),
        Box::new(FixedClock::new(FIXED_MS)),
    )
    .expect("打开驱动")
}

fn ledger_records(out: &Path) -> Vec<af_execline::LedgerRecord> {
    OrderLedger::load(&out.join("ledger.jsonl")).expect("ledger 可重载")
}

fn mes_events(out: &Path) -> Vec<MesEvent> {
    MesFileWriter::load(&out.join("mes-events.jsonl")).expect("MES 事件可重载")
}

fn jsonl_lines(path: &Path) -> Vec<serde_json::Value> {
    let text =
        fs::read_to_string(path).unwrap_or_else(|err| panic!("读 {}: {err}", path.display()));
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

// ---------- ① 全生命周期：8 op 连续 + MES 四类各一 + 行与三件套落盘 ----------

/// 草案第一条（对齐 lifecycle.rs 既有断言风格）：真引擎子进程全程门控，
/// SwevExecuteUnit（FakeRunner/FakeBridge/假判分/假 docker）经驱动执行——
/// attempt 目录由驱动层 next_attempt_index 分配（`runs/swev-<task>-<profile>-a1`），
/// 结果行经驱动层 RecordSink 文件实现落 out 目录。
#[test]
fn full_lifecycle_with_swev_unit_ledger_mes_rows_and_trio() {
    with_fake_docker(|| {
        let root = temp_root("p5-full");
        let data_root = data_root_with_task(&root);
        let out = root.join("order-out");
        let mut driver = open_driver("wo-p5-full", out.clone(), data_root.clone());
        let bridge = Arc::new(FakeBridge::new());
        let runner = Arc::new(FakeRunner::new());
        let unit = swev_unit("full", bridge.clone(), runner.clone());

        let report = driver.drive(&unit).expect("真实单元全生命周期应走通");

        // 终态与工件（resolved=true → 合格终态工件 → close 放行）。
        assert_eq!(report.order.state(), af_workorder::OrderState::Closed);
        assert!(!report.replayed, "首驱不是重放");
        assert_eq!(report.order.artifacts().len(), 1);
        assert!(report.order.artifacts()[0].qualified);
        let exec = report.exec_output.expect("驱动报告带执行产出");
        assert_eq!(exec.verdict, Some(true));
        // 判定走真引擎子进程。
        assert!(driver.engine_bin().ends_with("af-engine"));

        // ledger：8 op 全在账且成功，序号从 1 连续。
        let records = ledger_records(&out);
        let seqs: Vec<u64> = records.iter().map(|r| r.seq).collect();
        let expected: Vec<u64> = (1..=records.len() as u64).collect();
        assert_eq!(seqs, expected, "入账序号必须连续");
        for op in [
            "create_order",
            "bind_line",
            "attach_voucher",
            "release",
            "start",
            "attach_artifact",
            "complete",
            "close",
        ] {
            assert!(
                records.iter().any(|r| r.op == op && r.ok),
                "ledger 缺少成功记录: {op}"
            );
        }
        assert_eq!(
            records.iter().filter(|r| r.op == "close" && r.ok).count(),
            1,
            "close 成功记录唯一"
        );

        // MES：四类跃迁恰各一条，work_order_id 收敛 stable_id。
        let events = mes_events(&out);
        let mut kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
        kinds.sort_unstable();
        assert_eq!(kinds, vec!["close", "complete", "release", "start"]);
        assert!(events
            .iter()
            .all(|e| e.work_order_id == report.order.stable_id().as_str()));

        // attempt 目录：驱动层分配（D-20 命名），三件套落盘其中。
        let attempt_dir = out.join("runs").join(format!("swev-{TASK}-{PROFILE}-a1"));
        assert!(
            attempt_dir.is_dir(),
            "驱动层分配 attempt 目录: {}",
            attempt_dir.display()
        );
        assert_eq!(
            std::fs::read_to_string(attempt_dir.join("eval.log")).unwrap(),
            "+ fake eval output line\n",
            "eval.log = 容器输出"
        );
        assert!(attempt_dir.join("eval.sh").is_file(), "eval.sh 在盘");
        assert!(attempt_dir.join("agent.diff").is_file(), "agent.diff 在盘");

        // legacy 行：经驱动 RecordSink 落 out/results.jsonl，恰一行 18 键，
        // run_dir = 驱动层分配的 attempt 目录。
        let legacy = jsonl_lines(&out.join("results.jsonl"));
        assert_eq!(legacy.len(), 1, "恰一行 legacy（驱动层文件 sink）");
        let mut legacy = legacy.into_iter().next().unwrap();
        assert_eq!(legacy.as_object().unwrap().len(), 18, "18 键齐备");
        let wall = legacy
            .as_object_mut()
            .unwrap()
            .remove("agent_wall_ms")
            .expect("wall 字段在");
        assert!(wall.as_u64().unwrap() < 60_000, "墙钟毫秒数合理: {wall}");
        assert_eq!(legacy["suite"], serde_json::json!("swe-v"));
        assert_eq!(legacy["task_id"], serde_json::json!(TASK));
        assert_eq!(legacy["profile"], serde_json::json!(PROFILE));
        assert_eq!(legacy["reward"], serde_json::json!(true));
        assert_eq!(legacy["status"], serde_json::json!("completed"));
        assert_eq!(
            legacy["run_dir"],
            serde_json::json!(attempt_dir.display().to_string()),
            "run_dir = 驱动层分配的 D-20 目录"
        );
        assert_eq!(
            legacy["eval_sha256"],
            serde_json::json!(sha256_of(&attempt_dir.join("eval.log"))),
            "eval_sha256 独立重算对账"
        );

        // v1 行：out/records.jsonl，plan_id/node_id 来自订单上下文。
        let v1 = jsonl_lines(&out.join("records.jsonl"));
        assert_eq!(v1.len(), 1, "恰一行 v1 投影");
        assert_eq!(v1[0]["plan_id"], serde_json::json!("wo-p5-full"));
        assert_eq!(v1[0]["node_id"], serde_json::json!("line-p5"));
        assert_eq!(v1[0]["item_id"], serde_json::json!(TASK));
        assert_eq!(v1[0]["verdict"], serde_json::json!(true));

        // 判分链走的是注入缝：runner 恰一次、零模型零凭证。
        assert_eq!(runner.launches(), 1);
    })
}

// ---------- ② 同 order_id 二次 drive：replay 语义对真实单元成立 ----------

#[test]
fn second_drive_replays_real_unit_without_rerun_or_duplicate_rows() {
    with_fake_docker(|| {
        let root = temp_root("p5-replay");
        let data_root = data_root_with_task(&root);
        let out = root.join("order-out");
        let mut driver = open_driver("wo-p5-replay", out.clone(), data_root.clone());
        let bridge = Arc::new(FakeBridge::new());
        let runner = Arc::new(FakeRunner::new());
        let unit = swev_unit("replay", bridge.clone(), runner.clone());

        let first = driver.drive(&unit).expect("首驱走通");
        assert!(!first.replayed);
        assert_eq!(runner.launches(), 1);
        drop(driver);

        // 同 order_id、同 out_dir 二次驱动：同一单元实例（观察 runner 计数）。
        let mut driver2 = open_driver("wo-p5-replay", out.clone(), data_root);
        let second = driver2.drive(&unit).expect("二次驱动应幂等放行");

        assert!(second.replayed, "二次驱动必须标记为重放");
        assert_eq!(
            second.order.state(),
            af_workorder::OrderState::Closed,
            "终态不变"
        );
        assert_eq!(
            runner.launches(),
            1,
            "单元不重跑：runner 调用数仍为首驱的 1"
        );
        assert_eq!(
            bridge
                .calls()
                .iter()
                .filter(|(_, s, _)| s.starts_with("docker run"))
                .count(),
            1,
            "容器不重起：docker run 仍恰一次"
        );
        assert!(second.mes_events.is_empty(), "重放不得重复发 MES 事件");

        // close 成功记录唯一；results/records 行不重落（仍各一行）。
        let records = ledger_records(&out);
        assert_eq!(
            records.iter().filter(|r| r.op == "close" && r.ok).count(),
            1,
            "close 成功记录唯一"
        );
        assert!(
            records.iter().any(|r| r.op == "replay_skip"),
            "重放审计行在账"
        );
        let seqs: Vec<u64> = records.iter().map(|r| r.seq).collect();
        let expected: Vec<u64> = (1..=records.len() as u64).collect();
        assert_eq!(seqs, expected, "重放后序号仍连续");
        assert_eq!(
            jsonl_lines(&out.join("results.jsonl")).len(),
            1,
            "legacy 行不重落"
        );
        assert_eq!(
            jsonl_lines(&out.join("records.jsonl")).len(),
            1,
            "v1 行不重落"
        );
        assert_eq!(mes_events(&out).len(), 4, "MES 仍四类各一");
    })
}

// ---------- ③ TB 目录覆写不对称回归注记（R8，main.rs:1000-1002 vs 594-610）----------

/// swev 已 D-20 递增不覆写、TB 仍 remove_dir_all 固定名覆写——迁移期不顺手修
/// （方案 §1.3/§6-R8：修了就不是迁移），本测试把不对称钉死为回归注记：
/// 任何"顺手统一"的改动都会在此红灯。
#[test]
fn attempt_layout_asymmetry_regression_r8() {
    use af_execline::driver::allocate_attempt_dir;

    // swev 臂：既有 a1 → 分配 a2，a1 哨兵原样（绝不覆写旧目录）。
    let root = temp_root("p5-layout-swev");
    let runs = root.join("runs");
    let a1 = runs.join("swev-T-P-a1");
    fs::create_dir_all(&a1).unwrap();
    fs::write(a1.join("sentinel"), "旧目录是判分复核证据").unwrap();
    let next = allocate_attempt_dir(&runs, "T", "P", &AttemptLayout::SwevIncrement);
    assert_eq!(next, runs.join("swev-T-P-a2"), "D-20：最大序号 +1 递增");
    assert!(
        a1.join("sentinel").is_file(),
        "swev 旧目录绝不删除/覆写（D-20 项 1）"
    );
    // 空根 → a1。
    let fresh = allocate_attempt_dir(&runs, "X", "P", &AttemptLayout::SwevIncrement);
    assert_eq!(fresh, runs.join("swev-X-P-a1"));

    // TB 臂：固定名 tb-<task>-<profile>，分配即 remove_dir_all 覆写（R8 保留）。
    let root2 = temp_root("p5-layout-tb");
    let runs2 = root2.join("runs");
    let tb_dir = runs2.join("tb-T-P");
    fs::create_dir_all(&tb_dir).unwrap();
    fs::write(tb_dir.join("sentinel"), "旧 run").unwrap();
    let again = allocate_attempt_dir(&runs2, "T", "P", &AttemptLayout::TbOverwrite);
    assert_eq!(again, tb_dir, "TB 固定名（不递增）");
    assert!(
        !tb_dir.join("sentinel").exists(),
        "TB 覆写不对称如实保留：remove_dir_all 已清旧目录（main.rs:1000-1002）"
    );
    assert!(!tb_dir.exists(), "目录本体亦移除（建目录是单元的事）");
}

// ---------- ④ RecordSink 文件实现（append + 每行 flush）----------

#[test]
fn file_record_sink_append_flush_and_reopen_append() {
    let dir = temp_root("p5-sink");
    {
        let sink = FileRecordSink::open(&dir).expect("打开双文件 sink");
        sink.append_legacy(r#"{"a":1}"#).expect("legacy 落行");
        sink.append_v1(r#"{"b":2}"#).expect("v1 落行");
    }
    assert_eq!(
        fs::read_to_string(dir.join("results.jsonl")).unwrap(),
        "{\"a\":1}\n",
        "legacy 轨 = {{output_dir}}/results.jsonl，逐行完整（writeln+flush）"
    );
    assert_eq!(
        fs::read_to_string(dir.join("records.jsonl")).unwrap(),
        "{\"b\":2}\n",
        "v1 轨 = {{output_dir}}/records.jsonl"
    );

    // 跨实例续写：重开同目录 → append 语义（不截断历史行，main.rs:644-648）。
    let sink = FileRecordSink::open(&dir).expect("重开 sink");
    sink.append_legacy(r#"{"a":3}"#).unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("results.jsonl")).unwrap(),
        "{\"a\":1}\n{\"a\":3}\n",
        "append 语义：跨单续写同文件"
    );
}
