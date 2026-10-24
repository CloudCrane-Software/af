//! P3 判分单元验收测试（af-bench 吸收方案 §5-P3 草案 + §3.2 四约束）。
//!
//! 先红后绿（ADR-007）：本文件先于实现提交，judge 模块 `unimplemented!()` 桩下必红。
//!
//! 对照形态：判分调用在 af-bench 内联于 `swev::verify`（swev.rs:205-210），硬编码
//! 路径、私有、不可注入——无法像 P1 那样 `#[path]` 编译原文件对拍。对照采用三层
//! 钉死（P2 同款手法）：
//! 1. **假判分 venv 行为证据**：可执行假 python + 假判分脚本 + 匹配指纹锚，经真
//!    [`JudgeLocator`] 构造真子进程——argv/环境/父进程全部实测断言（§3.2-1
//!    契约的行为证据，非源码目测）；
//! 2. **源文本比对**（cmp_p3_*）：判分器定位/argv 形状/超时标记/note 拼装模板
//!    原文必须在 af-bench swev.rs 在册——实现改构造即与黄金断言失配，af-bench
//!    模板漂移即报警，双向钉死；
//! 3. **真 venv fixture**（`#[ignore]`）：crate 内三件套小样本真跑现产判分器
//!    断言 resolved（venv/脚本缺席时显式跳过，不假绿）。

use af_execline::error::ErrorCode;
use af_execline::judge::{invoke, JudgeLocator};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

// ---- 公共助手 ----

fn temp_root(tag: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("af-execline-p3judge-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// 写可执行脚本（0o755）。
fn write_exec(path: &Path, body: &str) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// 独立 sha256 oracle（测试侧直接用 sha2，不借用被测实现的 sha256_hex）。
fn sha256_of(path: &Path) -> String {
    let data = std::fs::read(path).unwrap();
    let digest = Sha256::digest(&data);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 读 af-bench 源原文（源文本比对基准；本工作树与主仓逐字节一致，开工 diff 实测）。
fn bench_swev_src() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../af-bench/src/swev.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("读 {} 失败: {err}", path.display()))
}

// ---- 方案 §3.2-1：argv 契约（恰好三个位置参数、无 env 注入、不经 wrapper shell）----

/// argv 契约行为证据：对抗路径（空格/引号/$/;/反斜杠）逐字节穿透、恰五元素
/// [python, script, task_json, log, diff]（判分脚本恰收到三个位置参数）、
/// 子进程原始环境与父进程全等（无注入/清洗，python3 假解释器不改动继承
/// environ）、父进程不是 bash/ssh 包装层。若实现改成 bash -c 字符串拼接或
/// 加了 env 注入，四断言至少一个失配。
#[test]
fn judge_invoke_argv_contract_no_env_injection_no_wrapper_shell() {
    let root = temp_root("argv");
    // 对抗路径只作用在被传参数上；dump 文件留在干净根目录（脚本内单引号安全）。
    let task_json = root.join("task json's; $HOME .json");
    let log = root.join("ev al.log");
    let diff = root.join("agent dif f\\patch");
    std::fs::write(&task_json, b"{}").unwrap();
    std::fs::write(&log, b"log").unwrap();
    std::fs::write(&diff, b"diff").unwrap();

    let argv_dump = root.join("argv.txt");
    let env_dump = root.join("env.txt");
    let parent_dump = root.join("parent.txt");
    let python = root.join("fake-python");
    // 假 python 用 python3 而非 bash：python 不改动继承 environ（bash 启动即
    // 自维护 SHLVL/_/PWD/OLDPWD），dump 出的才是 execve 传入的原始环境——
    // ②的全等断言才能零豁免。
    write_exec(
        &python,
        &format!(
            "#!/usr/bin/python3\n\
             import os\n\
             import sys\n\
             with open('{argv}', 'w', encoding='utf-8') as f:\n\
             \tfor arg in sys.argv:\n\
             \t\tf.write(arg + '\\n')\n\
             with open('{env_}', 'w', encoding='utf-8') as f:\n\
             \tfor key, value in os.environ.items():\n\
             \t\tf.write(key + '=' + value + '\\n')\n\
             try:\n\
             \twith open('/proc/' + str(os.getppid()) + '/cmdline', 'rb') as p:\n\
             \t\twith open('{parent}', 'wb') as f:\n\
             \t\t\tf.write(p.read().replace(b'\\0', b' '))\n\
             except OSError:\n\
             \tpass\n\
             print('{{\"resolved\": true}}')\n",
            argv = argv_dump.display(),
            env_ = env_dump.display(),
            parent = parent_dump.display(),
        ),
    );

    let script = root.join("fake-swev-judge.py");
    std::fs::write(&script, b"# fixture judge script").unwrap();
    let locator = JudgeLocator {
        script_sha256: sha256_of(&script),
        python: python.clone(),
        script: script.clone(),
    };

    let output = invoke(&locator, &task_json, &log, &diff).unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        r#"{"resolved": true}"#
    );

    // ① 恰五元素、对抗路径逐字节穿透（wrapper shell 的字符串拼接必失真）。
    let argv: Vec<String> = std::fs::read_to_string(&argv_dump)
        .unwrap()
        .lines()
        .map(String::from)
        .collect();
    assert_eq!(
        argv,
        vec![
            python.display().to_string(),
            script.display().to_string(),
            task_json.display().to_string(),
            log.display().to_string(),
            diff.display().to_string(),
        ],
        "argv 契约失守：恰 [python, script, task_json, log, diff] 五元素，一个不多"
    );

    // ② 无 env 注入：子进程环境与测试进程环境全等（无增无删无改）。
    let dumped: HashMap<String, String> = std::fs::read_to_string(&env_dump)
        .unwrap()
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let mine: HashMap<String, String> = std::env::vars().collect();
    assert_eq!(dumped, mine, "子进程环境 ≠ 父进程环境 = 有 env 注入或清洗");

    // ③ 不经 wrapper shell：假 python 的父进程不得是 bash/ssh/timeout 等包装层
    //    （`bash -c`/`ssh host` 包一层，ppid 就换成它们）。argv 逐字节断言
    //    （①）已同时排除有损 shell 拼接。
    let parent = std::fs::read_to_string(&parent_dump).unwrap_or_default();
    if parent.trim().is_empty() {
        eprintln!("（/proc 父进程命令行不可读，跳过父进程断言——①已独立钉死契约）");
    } else {
        let first_token = parent.split_whitespace().next().unwrap_or_default();
        let basename = first_token.rsplit('/').next().unwrap_or(first_token);
        for wrapper in ["bash", "sh", "ssh", "timeout", "dash", "zsh"] {
            assert_ne!(
                basename, wrapper,
                "假 python 的父进程是包装层（判分不经 wrapper shell 契约失守）: {parent}"
            );
        }
    }
}

// ---- 方案 §3.2-2：指纹门 fail-loud（EX3xxx）----

/// 指纹门：锚匹配放行；改字节副本（任务书草案原话）在原锚下必拒，EX3002、
/// 双指纹（实测/声明）进文案；脚本缺席 = 无法验真，同样拒跑。
#[test]
fn judge_fingerprint_gate_fail_loud_ex3002() {
    let root = temp_root("fp");
    let original = root.join("judge.py");
    std::fs::write(&original, b"print('original bytes')\n").unwrap();

    // 锚匹配 → 放行。
    let good = JudgeLocator {
        python: root.join("no-python-needed"),
        script: original.clone(),
        script_sha256: sha256_of(&original),
    };
    good.check_fingerprint().unwrap();

    // 造一个改字节的脚本副本，锚仍声明原脚本 → fail-loud。
    let tampered = root.join("judge-tampered.py");
    std::fs::write(&tampered, b"print('original bytes')\n# one more byte\n").unwrap();
    let bad = JudgeLocator {
        script: tampered.clone(),
        script_sha256: sha256_of(&original),
        ..good.clone()
    };
    let err = bad.check_fingerprint().unwrap_err();
    assert_eq!(err.code(), ErrorCode::JudgeFingerprintMismatch);
    assert_eq!(ErrorCode::JudgeFingerprintMismatch.as_str(), "EX3002");
    assert_eq!(
        ErrorCode::JudgeFingerprintMismatch.name(),
        "JUDGE_FINGERPRINT_MISMATCH"
    );
    let msg = err.to_string();
    assert!(msg.starts_with("EX3002:"), "{msg}");
    assert!(msg.contains("不降级"), "fail-loud 文案: {msg}");
    assert!(
        msg.contains(&sha256_of(&tampered)),
        "实测指纹必须进文案: {msg}"
    );
    assert!(
        msg.contains(&sha256_of(&original)),
        "声明锚必须进文案: {msg}"
    );

    // 脚本缺席 = 无法验真 → 同样拒跑（不静默放行）。
    let missing = JudgeLocator {
        script: root.join("no-such-judge.py"),
        script_sha256: sha256_of(&original),
        ..good
    };
    let err = missing.check_fingerprint().unwrap_err();
    assert_eq!(err.code(), ErrorCode::JudgeFingerprintMismatch);
    assert!(err.to_string().starts_with("EX3002:"), "{}", err);
}

// ---- 默认定位 = 本机现产值（af-bench swev.rs:205-209 硬编码的迁移）----

#[test]
fn judge_production_locator_is_current_machine_values() {
    let loc = JudgeLocator::production();
    assert_eq!(
        loc.python,
        PathBuf::from("/home/pan-ding/swev-venv/bin/python")
    );
    assert_eq!(
        loc.script,
        PathBuf::from("/home/pan-ding/workspace/agent黑灯工厂/tools/swev-judge.py")
    );
    // 现产锚（方案 §3.1 sha256sum 实测；本任务 2026-10-24 复测一致）。
    assert_eq!(
        loc.script_sha256,
        "a292547f3bfce825b0b44997fd5cc145a5aa7ee89d8533729716c0275950617c"
    );
    // 历史锚口径（0338566c…，rsi-drill 冻结件）随方案 §3.1 双锚并记：回放对账
    // 构造 Locator 指向冻结件 + 该锚即可，无内置构造器（P6 对拍如需再加）。
}

// ---- 源文本比对：判分链模板原文必须在 af-bench swev.rs 在册 ----

/// 判分器定位/argv 形状/eval 桥接/超时标记/判定透传与 note 拼装/取证与哈希——
/// P3 迁移面的全部载荷模板在册断言。实现改构造即与行为断言失配，af-bench
/// 模板漂移即与本断言失配，双向钉死。
#[test]
fn cmp_p3_judge_templates_against_bench_source() {
    let swev = bench_swev_src();

    // 判分器定位硬编码（swev.rs:205-206）——production() 默认值的迁移源。
    assert!(swev.contains(r#""/home/pan-ding/swev-venv/bin/python""#));
    assert!(swev.contains(r#""/home/pan-ding/workspace/agent黑灯工厂/tools/swev-judge.py""#));

    // argv 形状：python → script → task_json → log → diff 的 .arg 链（swev.rs:207-209）。
    assert!(swev.contains(".arg(task_json_path)"));
    assert!(swev.contains(".arg(&log_path)"));
    assert!(swev.contains(".arg(&diff_path)"));

    // eval 执行桥接：容器内 bash -s、stderr 并入、外层 timeout、超时标记行
    // （swev.rs:165-168, 191-200）。
    assert!(swev.contains(r#""docker exec -i {} bash -s 2>&1""#));
    assert!(swev.contains("const EVAL_TIMEOUT_SEC: u32 = 1800;"));
    assert!(swev.contains(r#""\nEVAL_TIMEOUT_KILLED {EVAL_TIMEOUT_SEC}\n""#));
    assert!(swev.contains(r#"format!("timeout {EVAL_TIMEOUT_SEC} {run_cmd}")"#));

    // 判定透传与 note 拼装（§3.2-3，swev.rs:216-231, 237, 247, 256）。
    assert!(swev.contains(r#""[log_parsed=false] ""#));
    assert!(swev.contains("judge: {}"));
    assert!(swev.contains("eval 超时击杀（已落标记 {EVAL_TIMEOUT_SEC}s）；"));
    assert!(swev.contains("judge 退出"));
    assert!(swev.contains("judge 启动失败"));
    assert!(swev.contains("judge 输出不是 JSON"));

    // 取证命令、判分输入三件套路径拼装、sha256_hex 实现（swev.rs:103-116, 261-266）。
    assert!(swev.contains("git -c core.fileMode=false diff"));
    assert!(swev.contains(r#"work_dir.join("agent.diff")"#));
    assert!(swev.contains(r#"work_dir.join("eval.sh")"#));
    assert!(swev.contains(r#"work_dir.join("eval.log")"#));
    assert!(swev.contains("Sha256::digest"));
    assert!(swev.contains("取 diff 失败"));
    assert!(swev.contains("写 agent.diff 失败"));
    assert!(swev.contains("写 eval.sh 失败"));
}

// ---- 真 venv fixture：三件套小样本真跑现产判分器 ----

/// 样例三件套 fixture 真跑 venv judge 断言 resolved（方案 §5-P3 草案）。
/// fixture = astropy__astropy-12907 裁剪件：真实 repo/version/base_commit/
/// test_patch（swebench 4.1 规格表内 → make_test_spec 零出网，本任务以假代理
/// 阻断网络实测），fail_to_pass/pass_to_pass 各一题；两份日志只差 f2p 的
/// PASSED/FAILED 一词 → resolved 随日志翻转。
/// venv/脚本缺席时打印说明后跳过（不假绿不误红，P2 真 docker 冒烟同款）。
/// 运行：`cargo test -p af-execline --test p3_judge -- --ignored`。
#[test]
#[ignore = "真判分 fixture：需本机 swev-venv 与现产 swev-judge.py（-- --ignored 运行）"]
fn real_judge_fixture_trio_resolved() {
    let loc = JudgeLocator::production();
    if !loc.python.is_file() || !loc.script.is_file() {
        eprintln!(
            "跳过：判分 venv/脚本缺席（{} / {}）",
            loc.python.display(),
            loc.script.display()
        );
        return;
    }
    // 指纹门：现产脚本必须与默认锚 a292547f… 严丝合缝（漂移 = fail-loud，§3.1）。
    loc.check_fingerprint()
        .expect("现产判分脚本指纹与默认锚不符（脚本漂移？核对方案 §3.1 双锚口径）");

    let fx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/p3-judge");
    let task_json = fx.join("task.json");
    let diff = fx.join("agent.diff");
    assert!(task_json.is_file() && diff.is_file(), "fixture 三件套缺席");

    let run = |log: &str| {
        let out = invoke(&loc, &task_json, &fx.join(log), &diff).expect("judge 子进程拉起失败");
        assert!(
            out.status.success(),
            "judge 退出 {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        let row: serde_json::Value =
            serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim())
                .expect("judge stdout 不是 JSON");
        (row["resolved"].clone(), row["log_parsed"].clone())
    };

    // f2p 失败形态：resolved=false、log_parsed=true（Start/End 标记齐备）。
    let (resolved, parsed) = run("eval.log");
    assert_eq!(resolved, serde_json::json!(false));
    assert_eq!(parsed, serde_json::json!(true));

    // f2p 通过形态：同任务同 diff、只换日志 → resolved=true。判定随日志翻转，
    // 证明判定值确实出自 judge stdout（方案 §3.2-3），不是旁路常量。
    let (resolved, parsed) = run("eval-ok.log");
    assert_eq!(resolved, serde_json::json!(true));
    assert_eq!(parsed, serde_json::json!(true));
}
