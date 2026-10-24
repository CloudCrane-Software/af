//! P3 证据链验收测试（af-bench 吸收方案 §5-P3：capture_diff / eval.sh 写入 /
//! eval.log 产出 / `EVAL_TIMEOUT_KILLED` 标记 / sha256_hex 搬入并接 bridge）。
//!
//! 先红后绿（ADR-007）：本文件先于实现提交，`unimplemented!()` 桩下必红。
//!
//! 注入手法（零 docker 守护进程、零 ssh、零网络、零模型——资源纪律）：
//! - FakeBridge（P2 手法照用）：capture_diff 走 `&dyn HostBridge`，取证命令串、
//!   超时、错误文案逐字节断言；
//! - 假 docker（PATH 首段注入）：eval 执行链是真子进程 `timeout … docker exec
//!   -i <容器> bash -s 2>&1`，假 docker 按容器名分派退出码（afexl-p3-timeout →
//!   124 模拟外层 timeout 击杀，其余 → 0），吃到的 stdin 落证——eval.sh→stdin、
//!   stdout→eval.log 的文件管道行为面全部实测；
//! - 假判分 venv（p3_judge.rs 同款）：judge 子进程真拉起，argv/note 拼装实测。
//!
//! PATH 注入只 prepend 同一假 bin 目录（值恒定，set_var 竞态无害），且所有涉
//! 子进程的测试体经全局 Mutex 串行——env 改写的并发窗口归零。

use af_execline::hostbridge::HostBridge;
use af_execline::judge::JudgeLocator;
use af_execline::suite::swev::{self, SwevTask};
use af_execline::suite::tb::TbEnv;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

type BridgeResult = Result<(bool, String), String>;

// ---- 助手 ----

static SPAWN_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

fn guard() -> &'static Mutex<()> {
    SPAWN_GUARD.get_or_init(|| Mutex::new(()))
}

fn temp_root(tag: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("af-execline-p3verify-{tag}-{}", std::process::id()));
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

fn task() -> SwevTask {
    SwevTask {
        instance_id: "astropy__astropy-12907".into(),
        repo: "astropy/astropy".into(),
        problem_statement: "Fix the sampler".into(),
        image: "sweb.eval.x86_64.astropy-12907".into(),
        eval_script: "#!/bin/bash\nset -uxo pipefail\nfixture eval script body\n".into(),
        hints_text: String::new(),
    }
}

fn env(container: &str) -> TbEnv {
    TbEnv {
        container: container.into(),
        task: "astropy__astropy-12907".into(),
        host: "local".into(),
        workdir: "/testbed".into(),
    }
}

/// 取 Err 文案。
fn as_err<T>(result: Result<T, String>) -> String {
    result.err().unwrap_or_else(|| panic!("期望 Err，实得 Ok"))
}

// ---- FakeBridge（P2 手法照用）----

#[derive(Default)]
struct FakeBridge {
    calls: RefCell<Vec<(String, String, u32)>>,
    scripted: RefCell<VecDeque<BridgeResult>>,
}

impl FakeBridge {
    fn new(scripted: Vec<BridgeResult>) -> Self {
        Self {
            calls: RefCell::default(),
            scripted: RefCell::new(scripted.into()),
        }
    }

    fn calls(&self) -> Vec<(String, String, u32)> {
        self.calls.borrow().clone()
    }
}

impl HostBridge for FakeBridge {
    fn run(&self, host: &str, script: &str, timeout_sec: u32) -> BridgeResult {
        self.calls
            .borrow_mut()
            .push((host.to_string(), script.to_string(), timeout_sec));
        self.scripted
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| Ok((true, String::new())))
    }
}

// ---- 假 docker（PATH 首段注入，值恒定）----

/// 假 docker bin 目录：吃 stdin 落证（stdin-<容器>.txt = eval.sh 字节）、出一行
/// 日志、按容器名分派退出码——`afexl-p3-timeout` → 124（模拟外层 timeout 击杀），
/// 其余 → 0。目录按测试进程命名，同二进制内所有测试共享同一份内容。
fn fake_docker_bin() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("af-execline-p3-fakebin-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("docker");
    std::fs::write(
        &script,
        format!(
            "#!/bin/bash\n\
             /usr/bin/cat > '{dir}/stdin-$3.txt'\n\
             echo '+ fake eval output line'\n\
             if [ \"$3\" = \"afexl-p3-timeout\" ]; then\n\
             \texit 124\n\
             fi\n\
             exit 0\n",
            dir = dir.display(),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

/// 串行 + PATH 首段注入执行 `f`（所有真子进程测试体都走这里）。
fn with_fake_docker<R>(f: impl FnOnce(&Path) -> R) -> R {
    let _lock = guard().lock().unwrap_or_else(|e| e.into_inner());
    let bin = fake_docker_bin();
    let orig = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{}", bin.display(), orig));
    let out = f(&bin);
    std::env::set_var("PATH", orig);
    out
}

/// 假判分 venv：假 python 记 argv（argv.txt）后回一行 JSON；脚本任意字节，
/// 锚 = 真实 sha256（过 verify 前置指纹门）。
fn fake_judge(tag: &str, json_line: &str) -> (JudgeLocator, PathBuf) {
    let root = temp_root(tag);
    let argv_dump = root.join("argv.txt");
    let python = root.join("fake-python");
    write_exec(
        &python,
        &format!(
            "#!/bin/bash\n\
             printf '%s\\n' \"$0\" \"$@\" > '{argv}'\n\
             echo '{json_line}'\n",
            argv = argv_dump.display(),
        ),
    );
    let script = root.join("fake-swev-judge.py");
    std::fs::write(&script, b"# fixture judge script bytes").unwrap();
    (
        JudgeLocator {
            script_sha256: sha256_of(&script),
            python,
            script,
        },
        root,
    )
}

/// 跑一遍完整 verify（local 宿主）：FakeBridge 回固定 diff 文本。
fn run_verify(
    judge: &JudgeLocator,
    container: &str,
    tag: &str,
) -> (Option<bool>, String, Option<String>, PathBuf) {
    let root = temp_root(tag);
    let bridge = FakeBridge::new(vec![Ok((true, "diff --git a/x b/x\n".into()))]);
    let task_json = root.join("task.json");
    std::fs::write(&task_json, b"{}").unwrap();
    let (resolved, note, sha) = swev::verify(
        &bridge,
        judge,
        "local",
        &env(container),
        &task(),
        &root,
        &task_json,
    );
    (resolved, note, sha, root)
}

/// 读假判分 venv 记下的 argv（[python, script, task_json, log, diff]）。
fn judge_argv(jroot: &Path) -> Vec<String> {
    std::fs::read_to_string(jroot.join("argv.txt"))
        .unwrap()
        .lines()
        .map(String::from)
        .collect()
}

// ---- capture_diff（方案 §5-P3：搬入并接 bridge；swev.rs:102-117）----

#[test]
fn capture_diff_command_artifact_and_error_contract() {
    let bridge = FakeBridge::new(vec![Ok((true, "diff --git a/x b/x\n".into()))]);
    let root = temp_root("capdiff");
    let e = env("afexl-p3-capdiff");
    let diff = swev::capture_diff(&bridge, "local", &e, &root).unwrap();
    assert_eq!(diff, "diff --git a/x b/x\n");
    // 命令串与超时逐字节（swev.rs:103-107——注意 `diff' ` 后的原样尾随空格）。
    assert_eq!(
        bridge.calls(),
        vec![(
            "local".to_string(),
            "docker exec afexl-p3-capdiff bash -lc 'cd /testbed && git -c core.fileMode=false diff' "
                .to_string(),
            60
        )]
    );
    // agent.diff 落盘 = 返回文本同一份字节（判分三件套之一，路径与字节语义不动）。
    assert_eq!(
        std::fs::read_to_string(root.join("agent.diff")).unwrap(),
        "diff --git a/x b/x\n"
    );

    // 桥接失败 → "取 diff 失败: <err>"（swev.rs:108-110）。
    let bridge = FakeBridge::new(vec![Err("命令桥接启动失败: ssh dead".into())]);
    assert_eq!(
        as_err(swev::capture_diff(&bridge, "local", &e, &root)),
        "取 diff 失败: 命令桥接启动失败: ssh dead"
    );

    // 写 agent.diff 失败（同名目录占位）→ "写 agent.diff 失败"（swev.rs:111-115）。
    let root2 = temp_root("capdiff2");
    std::fs::create_dir_all(root2.join("agent.diff")).unwrap();
    let bridge = FakeBridge::new(vec![Ok((true, "x".into()))]);
    assert_eq!(
        as_err(swev::capture_diff(&bridge, "local", &e, &root2)),
        "写 agent.diff 失败"
    );
}

// ---- verify 主链：超时标记 → 哈希 → 判分（swev.rs:145-258）----

/// 顺序断言（方案 §3.2-4）：eval 执行退出 124 → `EVAL_TIMEOUT_KILLED 1800` 标记
/// 落日志末尾 → **之后**才计算 eval_sha256（哈希的是判分器实际读到的字节）。
/// 返回指纹与对最终日志字节的独立重算全等——哈希先于标记写入即失配。
/// 同时钉死：eval.sh 逐字节、eval.sh 经 stdin 喂容器（假 docker 吃到的 stdin
/// 落证）、note 前缀栈（超时击杀 → log_parsed=false → judge 原文）、
/// verify→judge 的 argv 接线、capture_diff 恰一次桥调用。
#[test]
fn verify_timeout_marker_before_hash_and_note_stack() {
    with_fake_docker(|bin| {
        let (judge, jroot) = fake_judge("vtimeout", r#"{"resolved": false, "log_parsed": false}"#);
        let (resolved, note, sha, root) = run_verify(&judge, "afexl-p3-timeout", "vtimeout");

        assert_eq!(resolved, Some(false));
        // eval.sh = task.eval_script 逐字节（swev.rs:157-160）。
        let t = task();
        assert_eq!(
            std::fs::read_to_string(root.join("eval.sh")).unwrap(),
            t.eval_script
        );
        // eval.log = 容器输出 + 超时标记行（swev.rs:191-200，D-20 项 4）。
        assert_eq!(
            std::fs::read_to_string(root.join("eval.log")).unwrap(),
            "+ fake eval output line\n\nEVAL_TIMEOUT_KILLED 1800\n"
        );
        // 顺序断言主体：返回 sha256 == 对最终日志（含标记）独立重算。
        let expected = sha256_of(&root.join("eval.log"));
        assert_eq!(sha.as_deref(), Some(expected.as_str()));
        // note 前缀栈（swev.rs:219-231，D-20 项 3 消费端）。
        assert_eq!(
            note,
            "eval 超时击杀（已落标记 1800s）；[log_parsed=false] judge: \
             {\"resolved\": false, \"log_parsed\": false}"
        );
        // verify→judge argv 接线：[python, script, task_json, eval.log, agent.diff]
        // ——判分三件套的路径拼装（swev.rs:207-209 消费 work_dir.join）。
        assert_eq!(
            judge_argv(&jroot),
            vec![
                judge.python.display().to_string(),
                judge.script.display().to_string(),
                root.join("task.json").display().to_string(),
                root.join("eval.log").display().to_string(),
                root.join("agent.diff").display().to_string(),
            ]
        );
        // eval 脚本经 stdin 喂容器 bash（swev.rs:161-163, 179-181）——假 docker
        // 吃到的 stdin 必须就是 eval.sh 字节。
        assert_eq!(
            std::fs::read_to_string(bin.join("stdin-afexl-p3-timeout.txt")).unwrap(),
            t.eval_script
        );
        // capture_diff 恰一次桥调用（命令/超时由上一测试逐字节钉死）。
        // （FakeBridge 在 run_verify 内部，这里只断言桥不参与 eval/judge 段：
        // eval 与 judge 都是真子进程，不经 bridge.run——swev.rs:169-186 原样。）
    })
}

/// 正常流（退出非 124）：无标记行、无超时前缀；log_parsed 前缀矩阵——
/// judge JSON 带 `log_parsed: false` → `[log_parsed=false] ` 前缀透传
/// （swev.rs:219-222）；`true` 与缺席 → 无前缀。
#[test]
fn verify_normal_flow_and_log_parsed_prefix_matrix() {
    with_fake_docker(|_bin| {
        // 变体 A：log_parsed=false → 前缀透传。
        let (judge, _ja) = fake_judge("vnorm-a", r#"{"resolved": false, "log_parsed": false}"#);
        let (resolved, note, sha, root) = run_verify(&judge, "afexl-p3-ok", "vnorm-a");
        assert_eq!(resolved, Some(false));
        assert_eq!(
            note,
            "[log_parsed=false] judge: {\"resolved\": false, \"log_parsed\": false}"
        );
        // 无超时 → 日志无标记行；sha = 对无标记日志的独立重算。
        assert_eq!(
            std::fs::read_to_string(root.join("eval.log")).unwrap(),
            "+ fake eval output line\n"
        );
        assert_eq!(
            sha.as_deref(),
            Some(sha256_of(&root.join("eval.log")).as_str())
        );

        // 变体 B：log_parsed=true → 无前缀。
        let (judge, _jb) = fake_judge("vnorm-b", r#"{"resolved": true, "log_parsed": true}"#);
        let (resolved, note, _sha, _root) = run_verify(&judge, "afexl-p3-ok", "vnorm-b");
        assert_eq!(resolved, Some(true));
        assert_eq!(note, "judge: {\"resolved\": true, \"log_parsed\": true}");

        // 变体 C：log_parsed 缺席 → 无前缀。
        let (judge, _jc) = fake_judge("vnorm-c", r#"{"resolved": true}"#);
        let (resolved, note, _sha, _root) = run_verify(&judge, "afexl-p3-ok", "vnorm-c");
        assert_eq!(resolved, Some(true));
        assert_eq!(note, "judge: {\"resolved\": true}");
    })
}

/// judge 失败三臂（swev.rs:244-256 原样文案）：退出非零 / stdout 非 JSON /
/// 拉起失败。三臂都保留 eval_sha256（判分已跑完、日志字节可对账）。
#[test]
fn verify_judge_failure_paths_match_bench_messages() {
    with_fake_docker(|_bin| {
        // ① judge 退出非零 → "judge 退出 <status>: <stderr 前 300>"。
        let root = temp_root("jerr");
        let python = root.join("fake-python");
        write_exec(&python, "#!/bin/bash\necho 'boom detail' >&2\nexit 3\n");
        let script = root.join("fake-swev-judge.py");
        std::fs::write(&script, b"# fixture").unwrap();
        let judge = JudgeLocator {
            script_sha256: sha256_of(&script),
            python,
            script,
        };
        let (resolved, note, sha, vroot) = run_verify(&judge, "afexl-p3-ok", "jerr-run");
        assert_eq!(resolved, None);
        assert!(
            note.starts_with("judge 退出 exit status: 3: boom detail"),
            "{note}"
        );
        assert_eq!(
            sha.as_deref(),
            Some(sha256_of(&vroot.join("eval.log")).as_str())
        );

        // ② judge stdout 非 JSON → "judge 输出不是 JSON: <serde 错>: <前 200 字符>"。
        let root = temp_root("jnotjson");
        let python = root.join("fake-python");
        write_exec(&python, "#!/bin/bash\necho 'not json at all'\n");
        let script = root.join("fake-swev-judge.py");
        std::fs::write(&script, b"# fixture").unwrap();
        let judge = JudgeLocator {
            script_sha256: sha256_of(&script),
            python,
            script,
        };
        let (resolved, note, sha, vroot) = run_verify(&judge, "afexl-p3-ok", "jnotjson-run");
        assert_eq!(resolved, None);
        assert!(note.starts_with("judge 输出不是 JSON: "), "{note}");
        assert!(note.contains("not json at all"), "{note}");
        assert_eq!(
            sha.as_deref(),
            Some(sha256_of(&vroot.join("eval.log")).as_str())
        );

        // ③ judge 拉起失败（python 缺席，脚本与锚完好——指纹门只验脚本）→
        //    "judge 启动失败: <io 错>"。
        let root = temp_root("jspawn");
        let script = root.join("fake-swev-judge.py");
        std::fs::write(&script, b"# fixture").unwrap();
        let judge = JudgeLocator {
            script_sha256: sha256_of(&script),
            python: root.join("no-such-python"),
            script,
        };
        let (resolved, note, sha, vroot) = run_verify(&judge, "afexl-p3-ok", "jspawn-run");
        assert_eq!(resolved, None);
        assert!(note.starts_with("judge 启动失败: "), "{note}");
        assert!(note.contains("No such file"), "{note}");
        assert_eq!(
            sha.as_deref(),
            Some(sha256_of(&vroot.join("eval.log")).as_str())
        );
    })
}

/// eval.sh 写失败短路（swev.rs:158-159）：目录占位 → (None, "写 eval.sh 失败",
/// None)——eval 执行与判分都不跑。
#[test]
fn verify_eval_sh_write_failure_short_circuits() {
    with_fake_docker(|_bin| {
        let root = temp_root("vshfail");
        std::fs::create_dir_all(root.join("eval.sh")).unwrap(); // 目录占位 → fs::write 失败
        let bridge = FakeBridge::new(vec![Ok((true, "diff --git a/x b/x\n".into()))]);
        let (judge, _) = fake_judge("vshfail", r#"{"resolved": true}"#);
        let task_json = root.join("task.json");
        std::fs::write(&task_json, b"{}").unwrap();
        let out = swev::verify(
            &bridge,
            &judge,
            "local",
            &env("afexl-p3-ok"),
            &task(),
            &root,
            &task_json,
        );
        assert_eq!(out, (None, "写 eval.sh 失败".to_string(), None));
    })
}

/// capture_diff 失败短路（swev.rs:152-155）：桥接失败文案透传，eval_sha256 为
/// None（桥接早期失败，D-20 项 2 语义）。
#[test]
fn verify_capture_diff_failure_short_circuits() {
    with_fake_docker(|_bin| {
        let root = temp_root("vdiffail");
        let bridge = FakeBridge::new(vec![Err("命令桥接启动失败: ssh dead".into())]);
        let (judge, _) = fake_judge("vdiffail", r#"{"resolved": true}"#);
        let task_json = root.join("task.json");
        std::fs::write(&task_json, b"{}").unwrap();
        let out = swev::verify(
            &bridge,
            &judge,
            "local",
            &env("afexl-p3-ok"),
            &task(),
            &root,
            &task_json,
        );
        assert_eq!(
            out,
            (
                None,
                "取 diff 失败: 命令桥接启动失败: ssh dead".to_string(),
                None
            )
        );
    })
}

// ---- 指纹门拒跑（方案 §3.2-2：不匹配 = EX3xxx 拒跑，不降级不跳判）----

/// 指纹不匹配 → verify 入口即拒（EX3002 fail-loud）：桥零调用、run 目录未建、
/// eval/判分全不启动——"拒跑"的行为证据，不只是错误码存在。
#[test]
fn verify_fingerprint_gate_blocks_before_any_work() {
    // 无需假 docker：门在最前，什么都不该跑。
    let bridge = FakeBridge::default();
    let root = temp_root("vgate");
    let (mut judge, _) = fake_judge("vgate", r#"{"resolved": true}"#);
    // 造改字节的脚本副本：锚仍声明原脚本 → 失配。
    let tampered = root.join("judge-tampered.py");
    std::fs::write(&tampered, b"# fixture judge script bytes TAMPERED\n").unwrap();
    judge.script = tampered;

    let task_json = root.join("task.json");
    std::fs::write(&task_json, b"{}").unwrap();
    let (resolved, note, sha) = swev::verify(
        &bridge,
        &judge,
        "local",
        &env("afexl-p3-ok"),
        &task(),
        &root,
        &task_json,
    );

    assert_eq!(resolved, None);
    assert!(note.starts_with("EX3002:"), "{note}");
    assert!(note.contains("判分脚本指纹不匹配"), "{note}");
    assert_eq!(sha, None);
    // 拒跑实证：桥零调用（capture_diff 未发生 → work_dir 也未建）。
    assert!(bridge.calls().is_empty());
    assert!(!root.join("agent.diff").exists());
    assert!(!root.join("eval.sh").exists());
    assert!(!root.join("eval.log").exists());
}
