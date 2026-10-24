//! P6 影子对拍验收测试（af-bench 吸收方案 §4 三层 / §5-P6 草案）。
//!
//! 先红后绿（ADR-007）：本文件先于实现提交，`src/shadow.rs` 桩 `unimplemented!()`
//! 下非 ignore 用例必红。
//!
//! **旧路活源基准** = `tests/fixtures/p6-frozen-bench/`（af-bench P5 完成点
//! 1830371 三文件的字节级冻结件——P6 薄壳化后 af-bench 本体不再承载这些载荷，
//! 新旧对拍的"旧"以此为准；冻结件与活源的等价由 provenance 测试钉 sha256）。
//!
//! 覆盖（编排者裁定的 P6 范围）：
//! - **L0 判分回放**（`#[ignore]` 真跑，零容器零模型）：≥4 份既有证据在新旧
//!   两路判分调用点复判，resolved/log_parsed/stdout 逐位一致（§4.1-L0 判据）；
//! - **L1 FakeRunner 取证-判分链**（everywhere 可跑 = 假判分；`#[ignore]` 真判分
//!   变体 = 真脚本+假容器）：新旧两路取证/判分/结果行 ≥6 维字段比对；
//! - **L2 钩子**：入口/配置面（L2Canary）+ `#[ignore]` 端到端用例在册不执行；
//! - **shadow-diff 三维度**：行字段（确定性/计量/键集差）、判分一致性、
//!   MES 映射表（§4.2）。
//!
//! 注入手法（零真容器、零模型、零网络）：假 docker（PATH 首段注入，按命令形状
//! 回放 fixture 证据 + argv 留痕）+ 假判分 venv（p3_judge 手法）+ FakeRunner
//! （p4 手法）+ SshBridge 真桥（host=local → 与旧路 tb_ssh 同一 bash -c 管道，
//! 新旧两路见同一假 docker）。引擎不打桩（MES 维度真 af-engine 子进程）。

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Arc, Mutex, OnceLock};

use af_execline::exec::ExecContext;
use af_execline::hostbridge::SshBridge;
use af_execline::judge::JudgeLocator;
use af_execline::record::FileRecordSink;
use af_execline::shadow::{
    self, check_mes_against_row, diff_legacy_rows, diff_results_files, legacy_family_from_row,
    mes_mapping_table, L0Evidence, L2Canary, OLD_BIN_SHA256_PREFIX,
};
use af_execline::suite::next_attempt_index;
use af_execline::suite::swev_unit::{SwevExecuteUnit, SwevUnitSpec};
use af_execline::ExecuteUnit;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

// ---- 旧路活源：冻结 af-bench 原文件（模块名 `tb`/`swev`——冻结 swev.rs 的
// `use crate::tb;` 要求本测试 crate 根上有 tb 模块；p1_comparison 同款）----

#[path = "fixtures/p6-frozen-bench/tb.rs"]
#[allow(dead_code)]
mod tb;

#[path = "fixtures/p6-frozen-bench/swev.rs"]
#[allow(dead_code)]
mod swev;

const TASK: &str = "astropy__astropy-12907";
const PROFILE: &str = "glm-cp";
const CONTAINER: &str = "afbench-swev-astropy-astropy-12907-glm-cp";
const NORMAL_DIFF: &str = "diff --git a/astropy/sampler.py b/astropy/sampler.py\n--- a/astropy/sampler.py\n+++ b/astropy/sampler.py\n@@ -1 +1 @@\n-fix\n+fix2\n";

// ---- 公共助手 ----

static SPAWN_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

fn guard() -> &'static Mutex<()> {
    SPAWN_GUARD.get_or_init(|| Mutex::new(()))
}

fn temp_root(tag: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("af-execline-p6shadow-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn write_exec(path: &Path, body: &str) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// 独立 sha256 oracle。
fn sha256_of(path: &Path) -> String {
    let data = std::fs::read(path).unwrap();
    let digest = Sha256::digest(&data);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn frozen_bench() -> PathBuf {
    fixtures().join("p6-frozen-bench")
}

/// 题库根 + 任务包（一 json 两用：SwevTask 装载面 + 判分 task_json——
/// af-bench main.rs:640/850 同源口径）。
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

/// 假判分 venv（p3 手法）。
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

/// 假 docker（PATH 首段注入 + env 传 fixture 路径）：argv 全留痕；按命令形状
/// 回放 fixture——`git diff` 取证 → cat DIFF_FIXTURE；`bash -s`（eval 执行链）
/// → cat EVAL_FIXTURE；其余（rm/run/exec）→ 成功空输出。新旧两路经同一 shim。
fn with_fake_docker<R>(
    tag: &str,
    diff_fixture: &Path,
    eval_fixture: &Path,
    f: impl FnOnce(&Path) -> R,
) -> R {
    let _lock = guard().lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("af-execline-p6-fakebin-{tag}-{0}", process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let argv_log = dir.join("docker-argv.log");
    let script = dir.join("docker");
    write_exec(
        &script,
        &format!(
            "#!/bin/bash\n\
             printf '%s\\n' \"$*\" >> {log}\n\
             if printf '%s' \"$*\" | /usr/bin/grep -q 'git -c core.fileMode=false diff'; then\n\
             \t/usr/bin/cat {diff}; exit 0; fi\n\
             if printf '%s' \"$*\" | /usr/bin/grep -q 'bash -s'; then\n\
             \t/usr/bin/cat {eval}; exit 0; fi\n\
             exit 0\n",
            log = argv_log.display(),
            diff = diff_fixture.display(),
            eval = eval_fixture.display(),
        ),
    );
    std::env::set_var(
        "PATH",
        format!(
            "{}:{}",
            dir.display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );
    let out = f(&argv_log);
    // PATH 复位在断言后由调用方负责整体持有锁；此处恢复原值不可行（并发），
    // 采用 p4/p5 同款约定：所有 PATH 敏感测试共用 SPAWN_GUARD 串行。
    out
}

/// FakeRunner（p4/p5 手法）：固定 RunReport + http_retries=2，捕获 run_id。
struct FakeRunner {
    run_id: Mutex<Option<String>>,
}

impl FakeRunner {
    fn new() -> Self {
        Self {
            run_id: Mutex::new(None),
        }
    }

    fn captured_run_id(&self) -> String {
        self.run_id
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .expect("FakeRunner 恰被调用一次")
    }
}

impl af_execline::AgentRunner for FakeRunner {
    fn run(
        &self,
        launch: af_execline::AgentLaunch<'_>,
    ) -> Result<af_execline::AgentRunOutcome, af_execline::AgentRunFailure> {
        *self.run_id.lock().unwrap_or_else(|e| e.into_inner()) = Some(launch.run_id.clone());
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

/// 新路单题执行（SshBridge 真桥 + FakeRunner + 注入判分 locator + FileRecordSink）。
/// 返回 (attempt_dir, 新路 results.jsonl 行, FakeRunner 捕获的 run_id)。
fn new_path_run(
    data_root: &Path,
    out_dir: &Path,
    locator: JudgeLocator,
) -> (PathBuf, Value, String) {
    let runs_root = out_dir.join("runs");
    std::fs::create_dir_all(&runs_root).unwrap();
    let attempt = next_attempt_index(&runs_root, TASK, PROFILE);
    let attempt_dir = runs_root.join(format!("swev-{TASK}-{PROFILE}-a{attempt}"));
    let sink = FileRecordSink::open(out_dir).expect("打开双文件 sink");
    let spec = SwevUnitSpec::fix2(PROFILE, locator);
    let runner = Arc::new(FakeRunner::new());
    let unit = SwevExecuteUnit::new(spec, Arc::new(SshBridge), runner.clone()).expect("单元装载");
    let order_id = format!("{TASK}-a{attempt}");
    let stable = af_workorder::StableId::generate(&order_id);
    let ctx = ExecContext {
        order_id: &order_id,
        stable_id: stable.as_str(),
        line_id: "bench-line",
        out_dir,
        task_id: TASK,
        host_alias: "local",
        data_root,
        attempt_dir: &attempt_dir,
        record_sink: &sink,
        trial: false,
    };
    let output = unit.execute(&ctx).expect("新路全流程应 Ok");
    let rows: Vec<Value> = std::fs::read_to_string(out_dir.join("results.jsonl"))
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 1, "新路恰一行 legacy");
    let row = rows.into_iter().next().unwrap();
    assert_eq!(
        row["reward"],
        json!(output.verdict),
        "行 reward 与单元 verdict 同源（双轨不漂移）"
    );
    let run_id = runner.captured_run_id();
    (attempt_dir, row, run_id)
}

/// 冻结 af-bench main.rs:861-890 的 18 键行装配模板（旧路行 oracle——语句
/// 原样重建；模板在册由 cmp_p6_row_template_in_frozen_source 钉死）。
#[allow(clippy::too_many_arguments)]
fn old_template_row(
    task_id: &str,
    run_dir: &Path,
    resolved: Option<bool>,
    eval_sha256: Option<&str>,
    gate: &str,
    http_retries: u64,
    agent_wall_ms: u64,
    note: &str,
) -> Value {
    json!({
        "suite": "swe-v",
        "task_id": task_id,
        "profile": PROFILE,
        "trial": false,
        "reward": resolved,
        "status": "completed",
        "scaffold": "v3",
        "reasoning_tier": "off",
        "run_dir": run_dir.display().to_string(),
        "eval_sha256": eval_sha256,
        "gate": gate,
        "attempts": 1,
        "http_retries": http_retries,
        "turns": 3,
        "model_calls": 5,
        "tokens": {
            "input": 100,
            "output": 50,
            "cache_read": 10,
        },
        "agent_wall_ms": agent_wall_ms,
        "verifier_note": note.chars().take(300).collect::<String>(),
    })
}

fn jsonl_lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("读 {} 失败: {err}", path.display()))
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

// ---------- 冻结件 provenance + 旧路判分调用源文本钉死 ----------

/// 冻结件 = af-bench P5 完成点 1830371 三文件的字节级副本（sha256 钉死：
/// 误改一字节即红）。P1-P5 的活源对拍基准自 P6 起定居于此（明示等价迁移）。
#[test]
fn cmp_p6_frozen_fixture_provenance() {
    let expect = [
        (
            "main.rs",
            "b857383abdc4e3dfc0262c90ebb9c124ea3a44cb2b069f9175472f2f245981ad",
        ),
        (
            "swev.rs",
            "f013dd4fee4db48f8410e5f3185baf00a6df62a6f7fe76ddf0963a201754295d",
        ),
        (
            "tb.rs",
            "58664cc914e5cd542df43f7f7c3f5bf00b67d527b28e0124e4c9cb0da5962828",
        ),
    ];
    for (name, sha) in expect {
        assert_eq!(
            sha256_of(&frozen_bench().join(name)),
            sha,
            "冻结件 {name} 与 1830371 逐字节一致（provenance）"
        );
    }
}

/// 旧路判分调用（shadow::old_judge_call）= 冻结 swev.rs:205-210 语句逐字复制：
/// shadow.rs 源文本与冻结件双双含硬编码解释器/脚本路径与 .arg 链。任何一侧
/// 漂移即红（双向钉死，p3_judge cmp 手法）。
#[test]
fn cmp_p6_old_judge_call_matches_frozen_bench_source() {
    let shadow_src = include_str!("../src/shadow.rs");
    let frozen_swev = std::fs::read_to_string(frozen_bench().join("swev.rs")).unwrap();
    for needle in [
        r#"Command::new("/home/pan-ding/swev-venv/bin/python")"#,
        r#".arg("/home/pan-ding/workspace/agent黑灯工厂/tools/swev-judge.py")"#,
        ".arg(task_json_path)",
        ".arg(&log_path)",
        ".arg(&diff_path)",
        ".output()",
    ] {
        assert!(
            frozen_swev.contains(needle),
            "冻结 swev.rs 缺旧路判分调用片段: {needle}"
        );
        assert!(
            shadow_src.contains(needle),
            "shadow::old_judge_call 源文本偏离冻结件: {needle}"
        );
    }
}

/// 旧路 18 键行装配模板在冻结 main.rs 在册（L1 行 oracle 的依据；片段级）。
#[test]
fn cmp_p6_row_template_in_frozen_source() {
    let src = std::fs::read_to_string(frozen_bench().join("main.rs")).unwrap();
    for needle in [
        "\"suite\": \"swe-v\",",
        "\"reasoning_tier\": \"off\",",
        "\"eval_sha256\": eval_sha256,",
        "\"gate\": gate,",
        "\"attempts\": 1,",
        "\"http_retries\": http_retries,",
        "\"agent_wall_ms\": agent_wall_ms,",
        "note.chars().take(300).collect::<String>(),",
        // D-20 项 1 注释（run_dir 字段来源）
        "\"run_dir\": run_dir.display().to_string(),",
    ] {
        assert!(src.contains(needle), "冻结 main.rs 行模板缺片段: {needle}");
    }
}

// ---------- L1（everywhere 可跑）：FakeRunner + 假判分，≥6 维字段比对 ----------

/// L1 主对拍（零真容器/零模型/零网络）：新旧两路同题同证据（假 docker 回放
/// 同一 fixture diff/log），维度：
/// - D1 agent.diff 字节（旧 capture_diff vs 新单元落盘）；
/// - D2 docker 可见命令族（argv 留痕：旧路命令 ⊆ 新路，容器名/护栏参数一致）；
/// - D3 eval.sh 字节 = task.eval_script（冻结 write 语义）；
/// - D4 eval.log 字节 = fixture log（shim 透传；旧路同链等价由真判分变体证）；
/// - D5 判定 resolved（fake judge stdout 的新路消费）；
/// - D6 verifier_note 精确拼装（"judge: {stdout}"，冻结模板）；
/// - D7 legacy 行：新路 results.jsonl 行 vs 旧路冻结模板 oracle——Value 全等
///      + 序列化字节全等（serde_json 同序）+ 18 键；
/// - D8 run_id 公式（`{task}-a{attempt}`，main.rs:778）与 run_dir 字段；
/// - D9 eval_sha256 独立重算对账；
/// - D10 gate（旧路 diff_touches_tests 在同一 diff 上的记账）。
#[test]
fn l1_fake_runner_chain_vs_frozen_bench_six_dimensions() {
    let root = temp_root("l1-fake");
    let diff_fixture = root.join("fixture.diff");
    let eval_fixture = root.join("fixture-eval.log");
    std::fs::write(&diff_fixture, NORMAL_DIFF).unwrap();
    std::fs::write(&eval_fixture, "fixture eval log line\n").unwrap();
    let data_root = data_root_with_task(&root);
    let task = swev::load_swev_tasks(&data_root, &[TASK.to_string()])
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    with_fake_docker("l1", &diff_fixture, &eval_fixture, |argv_log| {
        // ---- 旧路（冻结 swev 模块）：setup → capture_diff ----
        let env_old = swev::setup("local", &task, PROFILE).expect("旧路 setup（假 docker）");
        assert_eq!(env_old.container, CONTAINER);
        let work_old = root.join("old-run");
        let diff_old = swev::capture_diff("local", &env_old, &work_old).expect("旧路取证");
        // 旧路 gate 记账（main.rs:806-813 语义 = diff_touches_tests，冻结函数本体）。
        let gate_old = if diff_old.trim().is_empty() {
            "no_patch"
        } else if !swev::diff_touches_tests(&diff_old).is_empty() {
            "test_edit"
        } else {
            "ok"
        };

        // ---- 新路（SwevExecuteUnit + SshBridge + FakeRunner + 假判分）----
        let judge = fake_judge("l1-judge", r#"{"resolved": true, "log_parsed": true}"#);
        let out_new = root.join("new-out");
        let (attempt_dir, mut row_new, run_id_new) = new_path_run(&data_root, &out_new, judge);
        let argv_new = std::fs::read_to_string(argv_log).unwrap();

        // D1 agent.diff 字节：新旧两路同一 shim 同一 fixture。
        assert_eq!(diff_old, NORMAL_DIFF, "旧路 capture 输出 = fixture");
        assert_eq!(
            std::fs::read_to_string(attempt_dir.join("agent.diff")).unwrap(),
            diff_old,
            "D1: 新路 agent.diff 与旧路取证逐字节一致"
        );

        // D2 docker 可见命令族：旧路出现过的每条 docker 命令，新路原样再现
        //（rm 前置 / run 护栏 / capture），容器名一致。
        let assert_in_new = |needle: &str| {
            assert!(
                argv_new.contains(needle),
                "D2: 新路 docker 命令族缺旧路命令: {needle}\n新路口志:\n{argv_new}"
            );
        };
        assert_in_new(&format!("rm -f {CONTAINER}"));
        assert_in_new(&format!(
            "run -d --name {CONTAINER} --memory 6g --pids-limit 512"
        ));
        assert_in_new(&format!(
            "exec {CONTAINER} bash -lc cd /testbed && git -c core.fileMode=false diff"
        ));
        // 旧路口志段（先于新路执行落盘）也含同样命令——同 shim 同日志验证。
        assert!(
            argv_new
                .lines()
                .filter(|l| l.contains("git -c core.fileMode=false diff"))
                .count()
                >= 3,
            "旧 1 次 + 新 2 次（gate/verify）capture"
        );

        // D3 eval.sh = task.eval_script（冻结 swev.rs:158 write 语义）。
        assert_eq!(
            std::fs::read_to_string(attempt_dir.join("eval.sh")).unwrap(),
            task.eval_script,
            "D3: eval.sh 逐字节 = 任务包 eval_script"
        );

        // D4 eval.log = fixture 透传。
        let eval_log_bytes = std::fs::read(attempt_dir.join("eval.log")).unwrap();
        assert_eq!(
            eval_log_bytes,
            std::fs::read(&eval_fixture).unwrap(),
            "D4: eval.log = 假容器回放的判分日志字节"
        );

        // D5/D6/D9 判定与 note 拼装 + 指纹独立重算。
        let judge_json = r#"{"resolved": true, "log_parsed": true}"#;
        let expected_note = format!("judge: {judge_json}");
        assert_eq!(row_new["reward"], json!(true), "D5: resolved 消费");
        assert_eq!(
            row_new["verifier_note"],
            json!(expected_note),
            "D6: note 精确拼装（无超时/log_parsed 前缀）"
        );
        let eval_sha_expected = {
            let digest = Sha256::digest(&eval_log_bytes);
            digest
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        assert_eq!(
            row_new["eval_sha256"],
            json!(eval_sha_expected),
            "D9: 指纹对账"
        );

        // D10 gate：旧路冻结函数在同一 diff 上记账 = 新路行内 gate。
        assert_eq!(gate_old, "ok");
        assert_eq!(row_new["gate"], json!(gate_old), "D10: gate 记账一致");

        // D7 行对拍：新旧 18 键行 Value 全等（wall 弹出）+ 序列化字节全等。
        assert_eq!(row_new.as_object().unwrap().len(), 18, "18 键齐备");
        let agent_wall = row_new
            .as_object_mut()
            .unwrap()
            .remove("agent_wall_ms")
            .and_then(|v| v.as_u64())
            .expect("wall 字段");
        let mut oracle = old_template_row(
            TASK,
            &attempt_dir,
            Some(true),
            Some(&eval_sha_expected),
            gate_old,
            2, // FakeRunner http_retries
            agent_wall,
            &expected_note,
        );
        // oracle 的 wall 与行同值（喂入的就是行的值）——弹出后比其余 17 键，
        // 序列化字节断言亦在两弹出对象间进行（wall 属计时豁免面）。
        assert_eq!(
            oracle
                .as_object_mut()
                .unwrap()
                .remove("agent_wall_ms")
                .and_then(|v| v.as_u64()),
            Some(agent_wall)
        );
        assert_eq!(
            row_new, oracle,
            "D7: 新路行 = 旧路冻结模板 oracle（Value 全等）"
        );
        assert_eq!(
            serde_json::to_string(&row_new).unwrap(),
            serde_json::to_string(&oracle).unwrap(),
            "D7: 序列化字节全等（results 行字节契约）"
        );

        // D8 run_id 公式与 run_dir 字段。
        assert_eq!(
            run_id_new,
            format!("{TASK}-a1"),
            "D8: run_id = {{task}}-a{{attempt}}（main.rs:778）"
        );
        assert_eq!(row_new["run_dir"], json!(attempt_dir.display().to_string()));
        assert!(
            row_new["run_dir"]
                .as_str()
                .unwrap()
                .ends_with(&format!("swev-{TASK}-{PROFILE}-a1")),
            "D8: run_dir 命名公式"
        );
    });
}

// ---------- L1（#[ignore] 真判分变体）：旧 verify 全链 vs 新单元全链 ----------

/// 真判分链对拍（判分核 = 现产 swev-judge.py 真 venv；容器 = 假 docker 回放
/// 历史证据字节）：旧路 `swev::verify`（冻结件）与新路单元在同题同证据上
/// 各自走"取证 → eval 执行 → judge"，≥6 维：resolved / note / eval_sha256 /
/// agent.diff 字节 / eval.log 字节 / 行内 reward 与指纹一致。判定随日志翻转
/// （eval.log → false，eval-ok.log → true）证明确实出自 judge 而非旁路。
/// venv/脚本缺席时打印说明后跳过（p3 手法，不假绿不误红）。
/// 运行：`cargo test -p af-execline --test p6_shadow -- --ignored`。
#[test]
#[ignore = "真判分 L1：需本机 swev-venv 与现产 swev-judge.py（-- --ignored 运行）"]
fn l1_real_judge_old_vs_new_full_verify_chain() {
    let loc = JudgeLocator::production();
    if !loc.python.is_file() || !loc.script.is_file() {
        eprintln!(
            "跳过：判分 venv/脚本缺席（{} / {}）",
            loc.python.display(),
            loc.script.display()
        );
        return;
    }
    loc.check_fingerprint()
        .expect("现产判分脚本指纹与默认锚不符（脚本漂移？§3.1 双锚口径）");
    let fx = fixtures().join("p6-shadow");
    let diff_fixture = fx.join("agent.diff");

    for (log_name, expected_resolved) in [("eval.log", false), ("eval-ok.log", true)] {
        let root = temp_root(&format!("l1-real-{log_name}"));
        let data_root = root.join("data");
        std::fs::create_dir_all(&data_root).unwrap();
        std::fs::copy(fx.join("task.json"), data_root.join(format!("{TASK}.json"))).unwrap();
        let task = swev::load_swev_tasks(&data_root, &[TASK.to_string()])
            .unwrap()
            .into_iter()
            .next()
            .unwrap();

        with_fake_docker(
            &format!("real-{log_name}"),
            &diff_fixture,
            &fx.join(log_name),
            |_log| {
                // 旧路全链：setup → capture → verify（真 judge）。
                let env_old = swev::setup("local", &task, PROFILE).expect("旧路 setup");
                let work_old = root.join("old-run");
                let diff_old = swev::capture_diff("local", &env_old, &work_old).unwrap();
                let task_json = data_root.join(format!("{TASK}.json"));
                let (r_old, note_old, sha_old) =
                    swev::verify("local", &env_old, &task, &work_old, &task_json);
                assert_eq!(r_old, Some(expected_resolved), "旧路判定（{log_name}）");

                // 新路全链（真 locator 指纹门 + SshBridge + FakeRunner）。
                let out_new = root.join("new-out");
                let (attempt_dir, row, _run_id) = new_path_run(&data_root, &out_new, loc.clone());
                let r_new = row["reward"].as_bool();
                assert_eq!(r_new, Some(expected_resolved), "新路判定（{log_name}）");

                // 六维对拍。
                assert_eq!(r_new, r_old, "维 1 resolved：新旧逐位一致");
                assert_eq!(
                    row["verifier_note"].as_str().unwrap(),
                    note_old.chars().take(300).collect::<String>().as_str(),
                    "维 2 verifier_note 拼装一致"
                );
                assert_eq!(
                    row["eval_sha256"].as_str(),
                    sha_old.as_deref(),
                    "维 3 eval_sha256 一致（同日志字节）"
                );
                assert_eq!(
                    std::fs::read_to_string(attempt_dir.join("agent.diff")).unwrap(),
                    diff_old,
                    "维 4 agent.diff 字节一致"
                );
                assert_eq!(
                    std::fs::read(attempt_dir.join("eval.log")).unwrap(),
                    std::fs::read(work_old.join("eval.log")).unwrap(),
                    "维 5 eval.log 字节一致"
                );
                assert_eq!(
                    std::fs::read_to_string(attempt_dir.join("eval.sh")).unwrap(),
                    task.eval_script,
                    "维 6 eval.sh = 任务包脚本"
                );
                eprintln!(
                    "[L1-真判分] {log_name}: old=({r_old:?}, {sha_old:?}) new 一致，note 前 60 字: {}",
                    note_old.chars().take(60).collect::<String>()
                );
            },
        );
    }
}

// ---------- L0（#[ignore] 真跑）：≥4 份既有证据新旧两路复判逐位一致 ----------

/// 默认证据集：bench-obs4-out 战役 astropy__astropy-12907 a1–a4（4 份独立
/// run 证据：各异 eval.log/agent.diff，task_json 由 /tmp/swev-s1 补齐）。
const L0_DATA_ROOT: &str = "/tmp/swev-s1";
const L0_RUNS_ROOT: &str =
    "/home/pan-ding/workspace/agent黑灯工厂/工具状态备份/observability/bench-obs4-out/runs";

fn l0_default_evidences() -> Vec<L0Evidence> {
    let data_root = PathBuf::from(L0_DATA_ROOT);
    let runs = PathBuf::from(L0_RUNS_ROOT);
    let mut out = Vec::new();
    for attempt in 1..=4 {
        let run_dir = runs.join(format!("swev-{TASK}-mavis-m31f-a{attempt}"));
        if run_dir.join("eval.log").is_file()
            && run_dir.join("agent.diff").is_file()
            && data_root.join(format!("{TASK}.json")).is_file()
        {
            out.push(L0Evidence::from_run_dir(TASK, &data_root, &run_dir));
        }
    }
    out
}

/// L0 判分回放（零容器零模型）：≥4 份既有证据在新旧两路判分调用点各自复判，
/// resolved/log_parsed/stdout 三项逐位一致 100%（§4.1-L0 判据）。缺席时跳过。
/// 历史行 reward 交叉信息只报告不判死（历史行可能出自他代判分脚本——L0 判据
/// 是新旧两路互检，§4.1）。
#[test]
#[ignore = "L0 真跑：需本机 swev-venv、现产 swev-judge.py 与历史证据目录（-- --ignored 运行）"]
fn l0_replay_four_historical_evidences() {
    let loc = JudgeLocator::production();
    if !loc.python.is_file() || !loc.script.is_file() {
        eprintln!("跳过：判分 venv/脚本缺席");
        return;
    }
    loc.check_fingerprint()
        .expect("现产判分脚本指纹与默认锚不符");
    let evidences = l0_default_evidences();
    if evidences.len() < 4 {
        eprintln!(
            "跳过：历史证据不足 4 份（在位 {} 份：{}）",
            evidences.len(),
            L0_RUNS_ROOT
        );
        return;
    }
    let pairs = shadow::l0_replay(&evidences, &loc);
    assert_eq!(pairs.len(), evidences.len(), "逐证据出对拍结果");
    for pair in &pairs {
        assert!(
            pair.old.exit_ok,
            "{:?} 旧路 judge 退出失败: {}",
            pair.task_id, pair.old.note
        );
        assert!(
            pair.new.exit_ok,
            "{:?} 新路 judge 退出失败: {}",
            pair.task_id, pair.new.note
        );
        assert!(
            pair.resolved_agree(),
            "{:?} resolved 不一致: old={:?} new={:?}",
            pair.task_id,
            pair.old.resolved,
            pair.new.resolved
        );
        assert!(
            pair.log_parsed_agree(),
            "{:?} log_parsed 不一致: old={:?} new={:?}",
            pair.task_id,
            pair.old.log_parsed,
            pair.new.log_parsed
        );
        assert!(
            pair.stdout_agree(),
            "{:?} judge stdout 字节不一致",
            pair.task_id
        );
        assert!(pair.agree(), "L0 判据：三项全齐");
        eprintln!(
            "[L0] {} → old(resolved={:?}, log_parsed={:?}) == new；stdout {} 字节逐位一致",
            pair.task_id,
            pair.old.resolved,
            pair.old.log_parsed,
            pair.old.stdout.len()
        );
    }
    // 历史行 reward 交叉报告（bench-mes 消费口径的行；不判死）。
    let results = PathBuf::from(L0_RUNS_ROOT).join("..").join("results.jsonl");
    if let Ok(text) = std::fs::read_to_string(results.canonicalize().unwrap_or(results.clone())) {
        for line in text.lines() {
            if let Ok(row) = serde_json::from_str::<Value>(line) {
                eprintln!(
                    "[L0-历史行] {} reward={} eval_sha256={:?}（仅报告）",
                    row["task_id"].as_str().unwrap_or("?"),
                    row["reward"],
                    row["eval_sha256"]
                        .as_str()
                        .unwrap_or("?")
                        .chars()
                        .take(12)
                        .collect::<String>()
                );
            }
        }
    }
}

// ---------- shadow-diff 维度一/三：行字段 + MES 映射（everywhere 可跑）----------

/// 行字段三分类：确定性失配判死 / 计量差异记录 / 键集差单独列；行全等判定。
#[test]
fn shadow_diff_row_dimensions() {
    let base = json!({
        "suite": "swe-v", "task_id": "T", "profile": "P", "trial": false,
        "reward": true, "status": "completed", "gate": "ok", "scaffold": "v3",
        "reasoning_tier": "off", "attempts": 1, "run_dir": "/r", "eval_sha256": "ab",
        "http_retries": 1, "turns": 3, "model_calls": 5,
        "tokens": {"input": 1, "output": 2, "cache_read": 3},
        "agent_wall_ms": 99, "verifier_note": "n",
    });
    // 全等行：确定性零失配，passes。
    let same = diff_legacy_rows(&base, &base.clone());
    assert!(same.rows_equal && same.passes(), "同行必过");

    // 计量差异（turns）：记录不判死。
    let mut metering = base.clone();
    metering["turns"] = json!(4);
    let d = diff_legacy_rows(&base, &metering);
    assert!(!d.rows_equal, "行不全等");
    assert!(d.passes(), "计量差异不判死（供给面抖动豁免）");
    assert_eq!(
        d.metering_diffs.len(),
        1,
        "计量差异如实记录: {:?}",
        d.metering_diffs
    );
    assert!(d.deterministic_diffs.is_empty());

    // 确定性差异（status/reward）：判死。
    let mut det = base.clone();
    det["status"] = json!("run_failed");
    det["reward"] = json!(null);
    let d = diff_legacy_rows(&base, &det);
    assert!(!d.passes(), "确定性字段失配必不过");
    assert_eq!(
        d.deterministic_diffs.len(),
        2,
        "{:?}",
        d.deterministic_diffs
    );

    // 键集差：new 多 order 字段单独列、缺键判死。
    let mut extra = base.clone();
    extra["order_id"] = json!("wo-1");
    let d = diff_legacy_rows(&base, &extra);
    assert!(d.passes(), "新行多订单字段不判死（差集单独核对）");
    assert_eq!(d.extra_keys_in_new, vec!["order_id".to_string()]);
    let mut missing = base.clone();
    missing.as_object_mut().unwrap().remove("eval_sha256");
    let d = diff_legacy_rows(&base, &missing);
    assert!(!d.passes(), "缺键必不过");
    assert_eq!(d.missing_keys_in_new, vec!["eval_sha256".to_string()]);

    // 文件级：逐行配对 + 行数不等报缺失。
    let root = temp_root("rowdiff");
    let old = root.join("old.jsonl");
    let new = root.join("new.jsonl");
    std::fs::write(&old, format!("{base}\n")).unwrap();
    std::fs::write(&new, format!("{base}\n{base}\n")).unwrap();
    let diffs = diff_results_files(&old, &new).expect("文件比对");
    assert_eq!(diffs.len(), 2);
    assert!(!diffs[1].passes(), "行数不配对即报");
}

/// MES 映射表维度（§4.2-3）：映射表口径齐备；driver 事件（真引擎生命周期）
/// 对 legacy 行的映射检查（start/complete 各恰 1、work_order_id 收敛、行内
/// reward/计量字段可映射）；旧路事件族 = legacy 行字段的投影（bench-mes.py 视角）。
#[test]
fn l1_mes_mapping_dimension_via_driver() {
    use af_execline::{
        AttemptLayout, DriverConfig, EngineClient, FixedClock, OrderLifecycleDriver,
    };

    // 映射表口径（§4.2-3 列举的对应关系逐条在册）。
    let table = mes_mapping_table();
    let pairs = table
        .iter()
        .map(|row| (row.legacy_event, row.new_counterpart))
        .collect::<Vec<_>>();
    for expect in [
        ("started", "start"),
        ("done", "complete"),
        ("reward", "verdict"),
        ("tokens_input", "usage.quota.input"),
        ("tokens_output", "usage.quota.output"),
        ("cache_read", "usage.quota.cache_read"),
        ("turns", "usage.turns"),
        ("wall_ms", "wall_ms"),
    ] {
        assert!(pairs.contains(&expect), "映射表缺 {expect:?}: {pairs:?}");
    }

    // 真引擎生命周期（p5 手法）→ driver MES 事件 + results 行。
    let root = temp_root("l1-mes");
    let data_root = data_root_with_task(&root);
    let diff_fixture = root.join("d.diff");
    let eval_fixture = root.join("e.log");
    std::fs::write(&diff_fixture, NORMAL_DIFF).unwrap();
    std::fs::write(&eval_fixture, "mes eval log\n").unwrap();
    with_fake_docker("mes", &diff_fixture, &eval_fixture, |_log| {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap();
        let target_dir = std::env::var("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| workspace_root.join("target"));
        let engine =
            EngineClient::locate(&workspace_root, &target_dir).expect("真 af-engine 子进程定位");
        let out = root.join("order-out");
        let cfg = DriverConfig {
            order_id: "wo-p6-mes".into(),
            graph_version: "g-p6-1".into(),
            sla: af_workorder::Sla::new(None, af_workorder::Priority::Normal),
            line: af_workorder::LineBinding {
                line_id: "line-p6".into(),
                line_version: "lv-1".into(),
            },
            voucher: af_workorder::BudgetVoucher {
                voucher_id: "bv-p6".into(),
                amount: 500,
                signed: true,
            },
            out_dir: out.clone(),
            station_id: "st-p6".into(),
            host_alias: "local".into(),
            data_root: data_root.clone(),
            task_id: TASK.into(),
            trial: false,
            profile: PROFILE.into(),
            attempt_layout: AttemptLayout::SwevIncrement,
        };
        let mut driver =
            OrderLifecycleDriver::open(cfg, engine, Box::new(FixedClock::new(1_767_225_600_000)))
                .expect("打开驱动");
        let judge = fake_judge("mes-judge", r#"{"resolved": true}"#);
        let unit = SwevExecuteUnit::new(
            SwevUnitSpec::fix2(PROFILE, judge),
            Arc::new(af_execline::hostbridge::SshBridge),
            Arc::new(FakeRunner::new()),
        )
        .expect("单元装载");
        let report = driver.drive(&unit).expect("全生命周期");
        let events = report.mes_events.clone();
        assert!(!events.is_empty(), "driver 事件在手");

        // 行（驱动层 sink 落盘的 legacy 行）。
        let rows = jsonl_lines(&out.join("results.jsonl"));
        assert_eq!(rows.len(), 1);
        let row = rows.into_iter().next().unwrap();

        // 新路事件对行的映射检查（空清单 = 通过）。
        let problems = check_mes_against_row(&events, &row);
        assert!(problems.is_empty(), "MES 映射检查问题: {problems:?}");

        // 旧路事件族 = 行字段投影（bench-mes.py 视角）。
        let family = legacy_family_from_row(&row);
        assert!(family.started && family.done, "行在即 started/done");
        assert_eq!(family.reward, Some(true));
        assert_eq!(family.tokens_input, Some(100));
        assert_eq!(family.tokens_output, Some(50));
        assert_eq!(family.cache_read, Some(10));
        assert_eq!(family.turns, Some(3));
        assert!(family.wall_ms.is_some());
    });
}

// ---------- L2 钩子：配置面 + #[ignore] 端到端在册 ----------

/// L2 金丝雀配置面（不执行真容器/真模型）：默认值 = 本机现产口径；两路命令行
/// CLI 参数面逐字一致（薄壳化契约）；前置清单覆盖 §4.3 要项。
#[test]
fn l2_canary_hooks_in_place() {
    let canary = L2Canary::from_env();
    assert_eq!(
        canary.old_bin,
        PathBuf::from("/tmp/threetest-bin/af-bench-g3-frozen"),
        "旧路默认 = 冻结二进制（盘点 §②）"
    );
    assert!(
        canary.new_bin.ends_with("af-bench"),
        "新路默认 = 薄壳 af-bench"
    );
    assert_eq!(canary.data_root, PathBuf::from("/tmp/swev-s1"));
    assert!(
        !canary.tasks.is_empty(),
        "题集非空（§4.1-L2 建议确认集快标 3 + N 档 7）"
    );
    assert_eq!(canary.profile, "glm-cp");
    assert_eq!(canary.host, "local", "R3：新路分宿主（旧路 anolis 时）");

    let old_line = canary.old_command_line();
    let new_line = canary.new_command_line();
    for line in [&old_line, &new_line] {
        assert!(line.contains("--suite swe-v"), "{line}");
        assert!(
            line.contains(&format!("--data-root {}", canary.data_root.display())),
            "{line}"
        );
        assert!(
            line.contains(&format!("--tasks {}", canary.tasks.join(","))),
            "{line}"
        );
        assert!(
            line.contains(&format!("--profile {}", canary.profile)),
            "{line}"
        );
        assert!(line.contains(&format!("--host {}", canary.host)), "{line}");
    }
    // 旧路命令行不含新路二进制路径，反之亦然（两路不串线）。
    assert!(old_line.contains(canary.old_bin.to_str().unwrap()));
    assert!(new_line.contains(canary.new_bin.to_str().unwrap()));

    let checklist = canary.preflight_checklist();
    let joined = checklist.join("\n");
    for item in ["双锚", "错峰", "分宿主", ":17", "预算"] {
        assert!(joined.contains(item), "前置清单缺要项 {item}: {joined}");
    }
    assert!(
        OLD_BIN_SHA256_PREFIX.eq("7a87876c0a5541c0"),
        "冻结旧路指纹前 16 位在册"
    );
}

/// L2 端到端金丝雀钩子（#[ignore] 在册不执行——真容器+真模型，属后续 F-首单
/// 任务包，需运营者批预算/窗口）。两路二进制与题库在位时执行：新旧各跑全程
/// → 硬门：新路三件套用旧 judge 复判 = 新路自判 100%；软门：results 行
/// 确定性字段 diff_results_files 全过 + resolved 一致率与差异归因记录。
/// 运行：`AF_SHADOW_NEW_BIN=<薄壳二进制> cargo test -p af-execline --test
/// p6_shadow l2_canary_end_to_end_golden_path -- --ignored`。
#[test]
#[ignore = "L2 金丝雀：真容器+真模型，需运营者批预算/窗口（F-首单任务包执行；本用例为入口在册）"]
fn l2_canary_end_to_end_golden_path() {
    let canary = L2Canary::from_env();
    if !canary.old_bin.is_file() || !canary.new_bin.is_file() || !canary.data_root.is_dir() {
        eprintln!(
            "跳过：L2 两路二进制/题库不在位（old={} new={} data={}）",
            canary.old_bin.display(),
            canary.new_bin.display(),
            canary.data_root.display()
        );
        return;
    }
    // §4.3 前置清单人工核销提示。
    for item in canary.preflight_checklist() {
        eprintln!("[L2-preflight] {item}");
    }
    for (name, line) in [
        ("旧路", canary.old_command_line()),
        ("新路", canary.new_command_line()),
    ] {
        eprintln!("[L2] {name} 命令行：{line}");
    }
    // 真执行（真容器+真模型）+ 硬门复判 + 软门行 diff。
    for (tag, bin, out) in [
        ("old", &canary.old_bin, &canary.out_old),
        ("new", &canary.new_bin, &canary.out_new),
    ] {
        let status = std::process::Command::new(bin)
            .args([
                "--suite",
                "swe-v",
                "--data-root",
                canary.data_root.to_str().unwrap(),
                "--tasks",
                &canary.tasks.join(","),
                "--profile",
                &canary.profile,
                "--host",
                &canary.host,
                "--out",
                out.to_str().unwrap(),
            ])
            .status()
            .unwrap_or_else(|err| panic!("{tag} 路拉起失败: {err}"));
        assert!(status.success(), "{tag} 路非零退出: {status}");
    }
    // 软门：结果行确定性字段。
    let diffs = diff_results_files(
        &canary.out_old.join("results.jsonl"),
        &canary.out_new.join("results.jsonl"),
    )
    .expect("行 diff");
    for (i, diff) in diffs.iter().enumerate() {
        assert!(
            diff.passes(),
            "L2 软门：第 {i} 行确定性字段失配: {:?}",
            diff.deterministic_diffs
        );
    }
    // 硬门：新路三件套用旧判分调用复判 = 新路行内 reward。
    let loc = JudgeLocator::production();
    let rows = jsonl_lines(&canary.out_new.join("results.jsonl"));
    for row in &rows {
        let Some(run_dir) = row["run_dir"].as_str() else {
            continue; // setup_failed/run_failed 行无三件套
        };
        let task_id = row["task_id"].as_str().unwrap_or_default().to_string();
        let evidence = L0Evidence::from_run_dir(&task_id, &canary.data_root, Path::new(run_dir));
        let pair = &shadow::l0_replay(&[evidence], &loc)
            .into_iter()
            .next()
            .expect("复判出结果");
        assert!(pair.agree(), "L2 硬门：新路证据旧 judge 复判一致");
        let row_reward = match &row["reward"] {
            Value::Bool(true) => "true".to_string(),
            Value::Bool(false) => "false".to_string(),
            Value::String(text) => text.clone(),
            _ => String::new(),
        };
        assert_eq!(
            pair.old.resolved.map(|r| if r { "true" } else { "false" }),
            Some(row_reward.as_str()),
            "L2 硬门：复判 = 新路自判"
        );
    }
}
