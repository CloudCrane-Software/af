//! P1 纯函数层验收测试（af-bench 吸收方案 §5-P1 草案）+ 测试随迁。
//!
//! 先红后绿（ADR-007）：本文件先于实现提交，unimplemented!()/占位常量桩下必红。
//!
//! 随迁（af-bench 现有单测逐字随代码搬入，方案 §1.3"测试随迁"）：
//! - `next_attempt_index_counts_existing_dirs_only` ← af-bench main.rs:48-73；
//! - `container_guardrail_constants_match_calibration` ← af-bench tb.rs:326-341。
//!   注：方案记"3 个单测"，实测 af-bench 仅 2 个（`grep '#\[test\]'`：main.rs:49、
//!   tb.rs:334），如实登记于 RESULT-P1.md。
//!
//! 行为逐字节等价的**活源对拍**（#[path] 直接编译 af-bench 原文件，同输入同输出）
//! 在 tests/p1_comparison.rs；本文件锁定黄金值与边界行为。

use af_execline::profile::{profile, resolve_key};
use af_execline::suite::swev::{
    diff_touches_tests, load_swev_tasks, SwevTask, ARCHAEOLOGY_PATTERNS, SWEV_INSTANCE_PROTOCOL,
    SWEV_SYSTEM_PROMPT,
};
use af_execline::suite::tb::{
    bash_tool_decl, load_tb_tasks, shell_quote, split_rc, tail, SWEV_AGENT_CONTAINER_MEM,
    SWEV_AGENT_CONTAINER_PIDS, TB_CONTAINER_MEM, TB_CONTAINER_PIDS,
};
use af_execline::suite::{next_attempt_index, status_label};
use af_harness::{AuthzCode, BudgetFault, RunStatus};
use af_stop::Decision;
use std::path::PathBuf;

/// 每测试独立临时根（tag 唯一，先清后建），收尾不强求删除（temp_dir 下无碍）。
fn temp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("af-execline-p1-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// 取 Err 文案（af-bench `TbTask` 无 Debug derive，unwrap_err 不可用）。
fn as_err<T>(result: Result<T, String>) -> String {
    result.err().unwrap_or_else(|| panic!("期望 Err，实得 Ok"))
}

// ---- 随迁测试（af-bench 原单测，行为与断言逐字保留，仅临时目录名换前缀防撞） ----

/// D-20 项 1：run 目录序号按既有同名前缀目录递增，重跑不覆写。
/// ← af-bench main.rs:48-73 逐字随迁。
#[test]
fn next_attempt_index_counts_existing_dirs_only() {
    let root = std::env::temp_dir().join(format!("af-execline-d20-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let task = "astropy__astropy-12907";
    let profile = "glm-cp";
    // 空目录 → 第一跑 a1。
    assert_eq!(next_attempt_index(&root, task, profile), 1);
    // 既有 a1/a2 → 下一跑 a3；不匹配前缀的目录（他题/他档案/杂件）不计数。
    for name in [
        "swev-astropy__astropy-12907-glm-cp-a1",
        "swev-astropy__astropy-12907-glm-cp-a2",
        "swev-astropy__astropy-13033-glm-cp-a1",
        "swev-astropy__astropy-12907-kimi-k3-a7",
        "not-a-run-dir",
    ] {
        std::fs::create_dir_all(root.join(name)).unwrap();
    }
    assert_eq!(next_attempt_index(&root, task, profile), 3);
    // 垃圾后缀（非数字）必须被忽略，不能打断递增。
    std::fs::write(root.join("swev-astropy__astropy-12907-glm-cp-aX"), "").unwrap();
    assert_eq!(next_attempt_index(&root, task, profile), 3);
    let _ = std::fs::remove_dir_all(&root);
}

/// 资源护栏常量必须与运营者校准口径一致（agent 6g/512、TB 2g/256）。
/// ← af-bench tb.rs:326-341 逐字随迁（校准推导见 af-bench tb.rs:101-114 注释）。
#[test]
fn container_guardrail_constants_match_calibration() {
    assert_eq!(SWEV_AGENT_CONTAINER_MEM, "6g");
    assert_eq!(SWEV_AGENT_CONTAINER_PIDS, "512");
    assert_eq!(TB_CONTAINER_MEM, "2g");
    assert_eq!(TB_CONTAINER_PIDS, "256");
}

// ---- swev 纯函数 ----

/// `diff_touches_tests` 边界（af-bench swev.rs:121-139）：b/ 前缀剥离、
/// conftest.py/tox.ini/pytest.ini/setup.cfg 命中、大小写不敏感（比较侧 lowercase，
/// 命中串保留原大小写）、非测试文件干净。
#[test]
fn diff_touches_tests_flags_test_paths_and_strips_b_prefix() {
    let diff = concat!(
        "diff --git a/src/main.rs b/src/main.rs\n",
        "diff --git a/tests/test_x.py b/tests/test_x.py\n",
        "diff --git a/conftest.py b/conftest.py\n",
        "diff --git a/tox.ini b/tox.ini\n",
        "diff --git a/pytest.ini b/pytest.ini\n",
        "diff --git a/setup.cfg b/setup.cfg\n",
        "diff --git a/TESTS/Upper.PY b/TESTS/Upper.PY\n",
        "diff --git a/docs/changes.rst b/docs/changes.rst\n",
    );
    assert_eq!(
        diff_touches_tests(diff),
        vec![
            "tests/test_x.py",
            "conftest.py",
            "tox.ini",
            "pytest.ini",
            "setup.cfg",
            "TESTS/Upper.PY",
        ]
    );
    // 干净 diff → 空表。
    assert!(diff_touches_tests("diff --git a/src/lib.rs b/src/lib.rs\n").is_empty());
    assert!(diff_touches_tests("").is_empty());
}

/// SwevTask::prompt 格式（af-bench swev.rs:33-38）：两段式、原样嵌入换行。
#[test]
fn swev_task_prompt_format() {
    let task = SwevTask {
        instance_id: "astropy__astropy-12907".into(),
        repo: "astropy/astropy".into(),
        problem_statement: "Line1\nLine2".into(),
        image: String::new(),
        eval_script: String::new(),
        hints_text: String::new(),
    };
    assert_eq!(
        task.prompt(),
        "Repository: astropy/astropy\n\nIssue description:\nLine1\nLine2"
    );
}

/// load_swev_tasks（af-bench swev.rs:41-52）：正常读入（hints_text 缺省空）、
/// 缺文件与坏 JSON 的报错文案。
#[test]
fn load_swev_tasks_reads_and_reports_errors() {
    let root = temp_root("swev-load");
    std::fs::write(
        root.join("full.json"),
        r##"{"instance_id":"astropy__astropy-12907","repo":"astropy/astropy","problem_statement":"Fix the sampler","image":"sweb.eval.x86_64.astropy-12907","eval_script":"#!/bin/bash\nexit 0","hints_text":"some hint"}"##,
    )
    .unwrap();
    std::fs::write(
        root.join("nohints.json"),
        r#"{"instance_id":"django__django-1","repo":"django/django","problem_statement":"P","image":"img","eval_script":"x"}"#,
    )
    .unwrap();
    std::fs::write(root.join("bad.json"), "{").unwrap();

    let tasks = load_swev_tasks(&root, &["full".to_string(), "nohints".to_string()]).unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].instance_id, "astropy__astropy-12907");
    assert_eq!(tasks[0].repo, "astropy/astropy");
    assert_eq!(tasks[0].problem_statement, "Fix the sampler");
    assert_eq!(tasks[0].image, "sweb.eval.x86_64.astropy-12907");
    assert_eq!(tasks[0].eval_script, "#!/bin/bash\nexit 0");
    assert_eq!(tasks[0].hints_text, "some hint");
    // hints_text 是 #[serde(default)]：数据包缺字段 → 空串（判分器全字段来源不动）。
    assert_eq!(tasks[1].hints_text, "");

    let err = as_err(load_swev_tasks(&root, &["missing".to_string()]));
    assert!(err.starts_with("读 "), "{err}");
    assert!(err.contains("失败"), "{err}");
    let err = as_err(load_swev_tasks(&root, &["bad".to_string()]));
    assert!(err.contains("不是 JSON"), "{err}");
}

// ---- tb 纯函数 ----

/// split_rc（af-bench tb.rs:73-92）：标记行缺失 → (原文, None)；
/// 标记行在末尾/中部 → 剥离该行、rc 解析、其余原样保留。
#[test]
fn split_rc_marker_missing_and_present() {
    let (text, rc) = split_rc("plain output\nno marker");
    assert_eq!(text, "plain output\nno marker");
    assert_eq!(rc, None);

    let (text, rc) = split_rc("out\n__AF_RC=0");
    assert_eq!(text, "out\n");
    assert_eq!(rc, Some(0));

    let (text, rc) = split_rc("a\n__AF_RC=7\nstderr tail");
    assert_eq!(text, "a\nstderr tail");
    assert_eq!(rc, Some(7));
}

/// shell_quote（af-bench tb.rs:66-68）：整体单引号包裹，内部单引号转义。
#[test]
fn shell_quote_wraps_and_escapes_single_quotes() {
    assert_eq!(shell_quote("ls -la"), "'ls -la'");
    assert_eq!(shell_quote("it's"), "'it'\\''s'");
    assert_eq!(shell_quote(""), "''");
    assert_eq!(shell_quote("a\nb\tc"), "'a\nb\tc'");
}

/// tail（af-bench tb.rs:299-305）：短文原样，长文保末尾 cap 字节。
#[test]
fn tail_caps_to_last_bytes() {
    assert_eq!(tail("abc", 5), "abc");
    assert_eq!(tail("abcdefghij", 3), "hij");
    assert_eq!(tail("", 3), "");
}

/// bash_tool_decl（af-bench tb.rs:307-324）：ToolDecl JSON 形状与 workdir 注入。
#[test]
fn bash_tool_decl_shape_and_workdir_injection() {
    let decl = bash_tool_decl("/testbed");
    assert_eq!(decl["name"], "bash");
    assert_eq!(decl["parameters"]["type"], "object");
    assert_eq!(
        decl["parameters"]["properties"]["command"]["type"],
        "string"
    );
    assert_eq!(
        decl["parameters"]["required"],
        serde_json::json!(["command"])
    );
    assert_eq!(decl["effect"], "external_once");
    let desc = decl["description"].as_str().unwrap();
    assert!(desc.contains("/testbed"), "{desc}");
    assert!(desc.contains("exit=<code>"), "{desc}");

    let decl = bash_tool_decl("/app");
    let desc = decl["description"].as_str().unwrap();
    assert!(desc.contains("/app"), "{desc}");
    assert!(!desc.contains("/testbed"), "{desc}");
}

/// load_tb_tasks（af-bench tb.rs:17-36）：读 instruction.md、校验目录形状、
/// 缺件报错文案。
#[test]
fn load_tb_tasks_reads_prompt_and_checks_layout() {
    let root = temp_root("tb-load");
    for name in ["ok", "no_docker", "no_test"] {
        let dir = root.join(name);
        std::fs::create_dir_all(dir.join("environment")).unwrap();
        std::fs::create_dir_all(dir.join("tests")).unwrap();
        std::fs::write(dir.join("instruction.md"), "do the thing\n").unwrap();
    }
    std::fs::write(root.join("ok/environment/Dockerfile"), "FROM x\n").unwrap();
    std::fs::write(root.join("ok/tests/test.sh"), "set -e\n").unwrap();
    // no_docker：缺 environment/Dockerfile；no_test：缺 tests/test.sh。
    std::fs::write(root.join("no_test/environment/Dockerfile"), "FROM x\n").unwrap();

    let tasks = load_tb_tasks(&root, &["ok".to_string()]).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].task_id, "ok");
    assert_eq!(tasks[0].prompt, "do the thing\n");

    assert_eq!(
        as_err(load_tb_tasks(&root, &["no_docker".to_string()])),
        "no_docker 缺 environment/Dockerfile"
    );
    assert_eq!(
        as_err(load_tb_tasks(&root, &["no_test".to_string()])),
        "no_test 缺 tests/test.sh"
    );
}

// ---- profile / resolve_key ----

/// profile 表（af-bench main.rs:163-327）：全档案可解析且 key_cmd 非空。
#[test]
fn profile_table_all_entries_parse_with_nonempty_key_cmd() {
    const ALL: [&str; 10] = [
        "mavis-m31f",
        "mavis-m3",
        "glm-cp",
        "glm-cp-max",
        "step-5",
        "kimi-k3",
        "qwen30b-local",
        "qwen3-4b-local",
        "qwen3-8b-local",
        "qwen3-8b-anolis",
    ];
    for name in ALL {
        let prof = profile(name).unwrap_or_else(|err| panic!("{name}: {err}"));
        assert!(!prof.key_cmd.is_empty(), "{name} key_cmd 为空");
        assert!(!prof.endpoint.base_url.is_empty(), "{name} base_url 为空");
        assert!(!prof.endpoint.model.is_empty(), "{name} model 为空");
        assert!(!prof.endpoint.key_ref.is_empty(), "{name} key_ref 为空");
    }
}

/// 未知档案报错含可选清单（af-bench main.rs:323-326 逐字节）。
#[test]
fn profile_unknown_name_error_lists_options() {
    assert_eq!(
        as_err(profile("no-such")),
        "未知档案 no-such（可选 mavis-m31f / mavis-m3 / glm-cp / glm-cp-max / step-5 / \
         kimi-k3 / qwen30b-local / qwen3-4b-local / qwen3-8b-local）"
    );
}

/// status_label 六态映射（af-bench main.rs:579-588）。
#[test]
fn status_label_six_state_mapping() {
    assert_eq!(
        status_label(&RunStatus::Completed {
            signal: "TASK_COMPLETE".into()
        }),
        "completed"
    );
    assert_eq!(status_label(&RunStatus::MaxTurns), "max_turns");
    assert_eq!(status_label(&RunStatus::ToolFreeLimit), "tool_free_limit");
    assert_eq!(
        status_label(&RunStatus::BudgetExhausted {
            fault: BudgetFault::NegativeSpend { spend_cents: -1 }
        }),
        "budget_exhausted"
    );
    assert_eq!(
        status_label(&RunStatus::Stopped {
            decision: Decision::Continue
        }),
        "stopped"
    );
    assert_eq!(
        status_label(&RunStatus::Unauthorized {
            tool: "bash".into(),
            fact_present: false,
            code: AuthzCode::CredentialDenied,
            detail: "d".into()
        }),
        "unauthorized"
    );
}

/// resolve_key（af-bench main.rs:329-350，用 printf 假命令，不碰真解析器）：
/// 输出过短拒绝、16 字符放行、非零退出报错。
#[test]
fn resolve_key_rejects_short_output() {
    assert_eq!(
        resolve_key("printf 12345").unwrap_err(),
        "密钥解析器输出过短，疑似失败"
    );
    assert_eq!(
        resolve_key("printf %s 0123456789abcdef").unwrap(),
        "0123456789abcdef"
    );
    let err = resolve_key("echo boom >&2; exit 3").unwrap_err();
    assert!(err.starts_with("密钥解析器退出 "), "{err}");
    assert!(err.contains("boom"), "{err}");
}

// ---- 常量载荷（黄金值；字节级由 p1_comparison.rs 对 main.rs 源文本钉死） ----

/// 两段提示词与反考古模式（af-bench main.rs:543, 548-577）。
#[test]
fn prompts_and_archaeology_constants() {
    assert_eq!(
        SWEV_SYSTEM_PROMPT,
        "You are an autonomous software engineer that fixes issues in a real repository \
         by issuing bash commands inside a Linux container. The repository checkout is at \
         /testbed."
    );
    assert!(SWEV_INSTANCE_PROTOCOL.starts_with("# How to work\n"));
    assert!(SWEV_INSTANCE_PROTOCOL.contains("# Hard boundaries"));
    assert!(SWEV_INSTANCE_PROTOCOL.contains("# How to finish (the ONLY way)"));
    assert!(SWEV_INSTANCE_PROTOCOL.contains("TASK_COMPLETE"));
    assert!(SWEV_INSTANCE_PROTOCOL.ends_with(
        "After the token prints, the run ends immediately and you cannot continue working."
    ));
    assert_eq!(
        ARCHAEOLOGY_PATTERNS,
        &[
            "cat-file --batch-all-objects",
            "git fsck",
            "--lost-found",
            "git log --all --grep",
            "find / -name",
            "find / -type",
        ]
    );
}
