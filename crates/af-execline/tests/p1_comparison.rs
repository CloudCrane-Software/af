//! P1 对照测试：同输入同输出，证明搬运函数行为逐字节等价（吸收方案约束）。
//!
//! 对照路径一【活源对拍】：`#[path]` 把 af-bench 的 tb.rs / swev.rs **原文件**编译进
//! 本测试二进制（只读引用，不修改 af-bench 本体——薄壳化是 P6 的事），af-bench 侧
//! 即旧路行为基准，新旧实现同输入逐例比对。本工作树 af-bench 三文件与主仓
//! 逐字节一致（本任务 `diff` 实测；方案 §0 基线事实同）。
//! 注：af-bench 的 `shell_quote`/`split_rc`/`tail` 是模块**私有**函数，外部模块不可
//! 见（且 `include!` 无法吃下其 `//!` 文件头，E0753 实测），故这三项不做活源对拍，
//! 以 tests/p1_suite.rs 的黄金值断言固化（源码逐行推导）。
//!
//! 对照路径二【源文本比对】：af-bench main.rs 依赖面大（af-harness 全家桶 + CLI），
//! 无法整体编译进本 crate。其纯函数载荷（profile 表 key_cmd/端点字段、未知档案报错
//! 文案、两段提示词、反考古模式、status_label 六标签）改用字节级源文本比对：
//! 从 af-execline 侧常量/返回值**重建源码字面量**（`{:?}`/原样拼 `r#"…"#`），
//! 断言 main.rs 原文含该字面量——常量差一个字节即失配。
//!
//! `next_attempt_index`/`resolve_key` 无可比对物（私有于 main.rs）：
//! 行为对照 = 随迁单测（main.rs:48-73 原测试）+ 黄金值断言（p1_suite.rs）。

// ---- 活源基准：af-bench 原文件（模块名固定 `tb`——swev.rs 的 `use crate::tb;` 要求
// 本测试 crate 根上有 tb 模块）。dead_code 放行：原文件含 P2/P5 才搬的生命周期
// 函数（TbEnv/ssh/setup/verify），此处只作对照基准，不作删除。----

#[path = "../../af-bench/src/tb.rs"]
#[allow(dead_code)]
mod tb;

#[path = "../../af-bench/src/swev.rs"]
#[allow(dead_code)]
mod swev;

use af_execline::suite::swev as new_swev;
use af_execline::suite::tb as new_tb;
use af_execline::suite::tb::{
    SWEV_AGENT_CONTAINER_MEM, SWEV_AGENT_CONTAINER_PIDS, TB_CONTAINER_MEM, TB_CONTAINER_PIDS,
};
use std::path::PathBuf;

/// 读 af-bench main.rs 原文（源文本比对基准）。
fn bench_main_src() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../af-bench/src/main.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("读 {} 失败: {err}", path.display()))
}

fn temp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("af-execline-p1cmp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// 取 Err 文案（af-bench `TbTask` 无 Debug derive，unwrap_err 不可用）。
fn as_err<T>(result: Result<T, String>) -> String {
    result.err().unwrap_or_else(|| panic!("期望 Err，实得 Ok"))
}

// ---- 活源对拍：swev 纯函数 ----

/// diff_touches_tests 电池：b/ 前缀、四类测试配置命中、大小写、`test` 子串边界
/// （latest/testing 同样命中——原实现语义）、多命中保序、单 token 行跳过、
/// 连续 b/ 前缀剥离、非 diff 行忽略。
#[test]
fn cmp_diff_touches_tests_battery() {
    let cases: Vec<&str> = vec![
        "",
        "diff --git a/src/main.rs b/src/main.rs\n+patch\n",
        "diff --git a/tests/test_x.py b/tests/test_x.py\n",
        "diff --git a/conftest.py b/conftest.py\n",
        "diff --git a/tox.ini b/tox.ini\n",
        "diff --git a/pytest.ini b/pytest.ini\n",
        "diff --git a/setup.cfg b/setup.cfg\n",
        "diff --git a/TESTS/Upper.PY b/TESTS/Upper.PY\n",
        "diff --git a/src/testing.py b/src/testing.py\n",
        "diff --git a/latest/news.rst b/latest/news.rst\n",
        "diff --git a/docs/changes.rst b/docs/changes.rst\n",
        "diff --git only-one-token\n",
        "diff --git a/b/b/x b/b/b/x\n",
        "not a diff line at all\n__AF_RC=0\n",
        "diff --git a/tests/a.py b/tests/a.py\ndiff --git a/src/b.rs b/src/b.rs\ndiff --git a/tests/c.py b/tests/c.py\n",
    ];
    for (i, case) in cases.iter().enumerate() {
        assert_eq!(
            new_swev::diff_touches_tests(case),
            swev::diff_touches_tests(case),
            "case {i}: {case:?}"
        );
    }
    // 电池非空转：合并命中必须非空（防两边同返空的假绿），关键样例点名命中。
    let all_hits: Vec<String> = cases
        .iter()
        .flat_map(|case| new_swev::diff_touches_tests(case))
        .collect();
    assert!(!all_hits.is_empty());
    assert_eq!(
        new_swev::diff_touches_tests("diff --git a/tox.ini b/tox.ini\n"),
        vec!["tox.ini"]
    );
}

/// load_swev_tasks + SwevTask::prompt：同 fixture 目录双读逐字段比，错误路径
/// 文案逐字节比；prompt 对特殊字符（换行/引号/非 ASCII）同构。
#[test]
fn cmp_swev_task_load_and_prompt() {
    let root = temp_root("swev");
    std::fs::write(
        root.join("a.json"),
        r##"{"instance_id":"astropy__astropy-12907","repo":"astropy/astropy","problem_statement":"Fix\nthe \"sampler\" — unicode λ","image":"sweb.eval.x86_64.astropy-12907","eval_script":"#!/bin/bash\nexit 0","hints_text":"h"}"##,
    )
    .unwrap();
    std::fs::write(
        root.join("b.json"),
        r#"{"instance_id":"django__django-1","repo":"django/django","problem_statement":"P","image":"img","eval_script":"x"}"#,
    )
    .unwrap();
    std::fs::write(root.join("bad.json"), "{").unwrap();

    let names = vec!["a".to_string(), "b".to_string()];
    let old = swev::load_swev_tasks(&root, &names).unwrap();
    let new = new_swev::load_swev_tasks(&root, &names).unwrap();
    assert_eq!(old.len(), new.len());
    for (o, n) in old.iter().zip(new.iter()) {
        assert_eq!(o.instance_id, n.instance_id);
        assert_eq!(o.repo, n.repo);
        assert_eq!(o.problem_statement, n.problem_statement);
        assert_eq!(o.image, n.image);
        assert_eq!(o.eval_script, n.eval_script);
        assert_eq!(o.hints_text, n.hints_text);
        assert_eq!(o.prompt(), n.prompt());
    }

    // 错误路径：缺文件 / 坏 JSON 的报错文案逐字节一致。
    for bad in ["missing", "bad"] {
        assert_eq!(
            as_err(new_swev::load_swev_tasks(&root, &[bad.to_string()])),
            as_err(swev::load_swev_tasks(&root, &[bad.to_string()])),
            "case {bad}"
        );
    }
}

// ---- 活源对拍：tb 纯函数 ----

/// load_tb_tasks：同 fixture 双读比（task_id/prompt 逐字段），缺件报错文案逐字节比。
#[test]
fn cmp_tb_load_tasks() {
    let root = temp_root("tb");
    for name in ["ok", "no_docker", "no_test"] {
        let dir = root.join(name);
        std::fs::create_dir_all(dir.join("environment")).unwrap();
        std::fs::create_dir_all(dir.join("tests")).unwrap();
        std::fs::write(dir.join("instruction.md"), "do the thing\n多字节内容\n").unwrap();
    }
    std::fs::write(root.join("ok/environment/Dockerfile"), "FROM x\n").unwrap();
    std::fs::write(root.join("ok/tests/test.sh"), "set -e\n").unwrap();
    std::fs::write(root.join("no_test/environment/Dockerfile"), "FROM x\n").unwrap();

    let old = tb::load_tb_tasks(&root, &["ok".to_string()]).unwrap();
    let new = new_tb::load_tb_tasks(&root, &["ok".to_string()]).unwrap();
    assert_eq!(old.len(), new.len());
    assert_eq!(
        (new[0].task_id.as_str(), new[0].prompt.as_str()),
        (old[0].task_id.as_str(), old[0].prompt.as_str())
    );

    for bad in ["no_docker", "no_test"] {
        assert_eq!(
            as_err(new_tb::load_tb_tasks(&root, &[bad.to_string()])),
            as_err(tb::load_tb_tasks(&root, &[bad.to_string()])),
            "case {bad}"
        );
    }
}

/// bash_tool_decl：不同 workdir 下新旧 JSON 值全等。
#[test]
fn cmp_bash_tool_decl() {
    for workdir in ["/testbed", "/app", ""] {
        assert_eq!(
            new_tb::bash_tool_decl(workdir),
            tb::bash_tool_decl(workdir),
            "workdir {workdir:?}"
        );
    }
}

/// 容器资源护栏常量：新旧全等。
#[test]
fn cmp_guardrail_constants() {
    assert_eq!(SWEV_AGENT_CONTAINER_MEM, tb::SWEV_AGENT_CONTAINER_MEM);
    assert_eq!(SWEV_AGENT_CONTAINER_PIDS, tb::SWEV_AGENT_CONTAINER_PIDS);
    assert_eq!(TB_CONTAINER_MEM, tb::TB_CONTAINER_MEM);
    assert_eq!(TB_CONTAINER_PIDS, tb::TB_CONTAINER_PIDS);
}

// ---- 源文本比对：main.rs 载荷 ----

/// profile 表：每个档案的 key_cmd / base_url / model / key_ref 字符串以 `{:?}`
/// 重建字面量后必须原样出现在 af-bench main.rs 中（字节级锁定，含转义形）。
#[test]
fn cmp_profile_table_against_bench_source() {
    let src = bench_main_src();
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
        let prof = af_execline::profile::profile(name).unwrap();
        assert!(
            src.contains(&format!("{:?}", prof.key_cmd)),
            "{name} key_cmd 字面量不在 af-bench main.rs 中"
        );
        assert!(
            src.contains(&format!("{:?}", prof.endpoint.base_url)),
            "{name} base_url 字面量不在 af-bench main.rs 中"
        );
        assert!(
            src.contains(&format!("{:?}", prof.endpoint.model)),
            "{name} model 字面量不在 af-bench main.rs 中"
        );
        assert!(
            src.contains(&format!("{:?}", prof.endpoint.key_ref)),
            "{name} key_ref 字面量不在 af-bench main.rs 中"
        );
    }
    // 未知档案报错的格式模板（{other} 占位）原文在册——与 p1_suite 的实例化
    // 断言合并即锁死文案字节级一致。
    assert!(src.contains(
        "未知档案 {other}（可选 mavis-m31f / mavis-m3 / glm-cp / glm-cp-max / step-5 / \
         kimi-k3 / qwen30b-local / qwen3-4b-local / qwen3-8b-local）"
    ));
}

/// 两段提示词与反考古模式：从 af-execline 常量重建源码声明文本，断言 main.rs
/// 原文逐字节含它（raw 字符串 r#"…"# 与数组字面量均按源码排版重建）。
#[test]
fn cmp_prompts_and_patterns_against_bench_source() {
    use af_execline::suite::swev::{
        ARCHAEOLOGY_PATTERNS, SWEV_INSTANCE_PROTOCOL, SWEV_SYSTEM_PROMPT,
    };
    let src = bench_main_src();
    assert!(src.contains(&format!(
        "const SWEV_SYSTEM_PROMPT: &str = {SWEV_SYSTEM_PROMPT:?};"
    )));
    assert!(src.contains(&format!(
        "const SWEV_INSTANCE_PROTOCOL: &str = r#\"{SWEV_INSTANCE_PROTOCOL}\"#;"
    )));
    let patterns_decl = format!(
        "const ARCHAEOLOGY_PATTERNS: &[&str] = &[\n{}\n];",
        ARCHAEOLOGY_PATTERNS
            .iter()
            .map(|pattern| format!("    {pattern:?},"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(src.contains(&patterns_decl));
}

/// status_label 六标签：每个标签的 match 臂产出（`"<label>".into()`）原文在册；
/// 六态→标签的映射本身由 p1_suite.rs 黄金值断言锁定。
#[test]
fn cmp_status_labels_against_bench_source() {
    let src = bench_main_src();
    for label in [
        "completed",
        "max_turns",
        "tool_free_limit",
        "budget_exhausted",
        "stopped",
        "unauthorized",
    ] {
        assert!(
            src.contains(&format!("\"{label}\".into()")),
            "标签 {label} 不在 af-bench main.rs status_label 映射中"
        );
    }
}
