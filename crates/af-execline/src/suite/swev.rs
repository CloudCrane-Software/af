//! SWE-bench Verified 套件——纯函数层（af-bench 吸收 P1，原样搬入）。
//!
//! 来源与去向（吸收方案 §1.1）：
//! - `SwevTask`/`load_swev_tasks`/`SwevTask::prompt`/`diff_touches_tests`
//!   ← af-bench swev.rs:16-139，函数体逐字节一致；
//! - 两段提示词 `SWEV_SYSTEM_PROMPT`/`SWEV_INSTANCE_PROTOCOL` 与反考古模式
//!   `ARCHAEOLOGY_PATTERNS` ← af-bench main.rs:543, 548-577，测量语义载荷
//!   一个字节不动（方案 §1.1 行 4）。
//!
//! 容器 setup / capture_diff / verify / sha256_hex 不属 P1：宿主桥接抽象在 P2、
//! 判分单元与证据链在 P3（方案 §5）。判分三件套（task_json/eval.log/agent.diff）
//! 语义由后续 P3 以原样模块产出，本包不动其接口。
//!
//! 行为逐字节等价由 tests/p1_comparison.rs 对照测试钉死（#[path] 直接编译
//! af-bench 原文件，同输入同输出），黄金值见 tests/p1_suite.rs。

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct SwevTask {
    pub instance_id: String,
    pub repo: String,
    pub problem_statement: String,
    pub image: String,
    pub eval_script: String,
    /// fix2 对齐后不再注入提示（审计 D4），字段保留只为数据包反序列化完整。
    #[serde(default)]
    #[allow(dead_code)]
    pub hints_text: String,
}

impl SwevTask {
    /// fix2 对齐（设计/harness-对齐审计.md D4/§6.A.5）：不再注入 hints_text——
    /// 三个标准 harness 都只给 problem_statement，官方 hints 是面向解题者的补丁提示，
    /// 注入属"反向虚高"（与榜单不可比）。字段保留（判分器全字段来源不动）。
    pub fn prompt(&self) -> String {
        format!(
            "Repository: {}\n\nIssue description:\n{}",
            self.repo, self.problem_statement
        )
    }
}

pub fn load_swev_tasks(root: &Path, names: &[String]) -> Result<Vec<SwevTask>, String> {
    let mut out = Vec::new();
    for name in names {
        let path = root.join(format!("{name}.json"));
        let text = std::fs::read_to_string(&path)
            .map_err(|err| format!("读 {} 失败: {err}", path.display()))?;
        let task: SwevTask = serde_json::from_str(&text)
            .map_err(|err| format!("{} 不是 JSON: {err}", path.display()))?;
        out.push(task);
    }
    Ok(out)
}

/// diff 是否碰了测试文件（评分 harness 拥有测试，模型改测试=越权，S0 实证缺陷）。
/// 返回被碰的文件列表（空=干净）。
pub fn diff_touches_tests(diff: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some(path) = rest.split_whitespace().nth(1) {
                let p = path.trim_start_matches("b/").to_lowercase();
                let is_test = p.contains("test")
                    || p.ends_with("conftest.py")
                    || p.ends_with("tox.ini")
                    || p.ends_with("pytest.ini")
                    || p.ends_with("setup.cfg");
                if is_test {
                    hits.push(path.trim_start_matches("b/").to_string());
                }
            }
        }
    }
    hits
}

/// system 只留一行角色（mini-swe-agent 实证一行 system 足以拿 74%+，字数不是变量，
/// 纪律随任务重申才是）；全部纪律/工作流/提交协议放 instance 模板随每题重发——
/// 一次性 system 在长对话里抗遗忘差，是 S0 D2（越权改测试）/D6（交付物缺失）的直接机制。
/// ← af-bench main.rs:543 原样。
pub const SWEV_SYSTEM_PROMPT: &str = "You are an autonomous software engineer that fixes issues in a real repository by issuing bash commands inside a Linux container. The repository checkout is at /testbed.";

/// fix2 instance 模板：随任务 user 消息每题重发。来源：mini-swe-agent instance_template
/// （工作流四步 + 提交三步 + 边界）+ mcode Core Judgment 两句（不悄悄扩缩任务/歧义先做
/// 不依赖答案的部分）。测试先行引导对应 S0 D3（浅尝辄止）。
/// ← af-bench main.rs:548-566 原样。
pub const SWEV_INSTANCE_PROTOCOL: &str = r#"# How to work
1. Locate the relevant code first (grep/read files) and understand the issue before changing anything.
2. Reproduce before fixing when feasible: write or run a small reproduction and confirm it fails for the stated reason.
3. Implement the minimal fix in repository source files only.
4. Verify before submitting: run the repository's own tests for the affected area, or your reproduction; iterate until they pass.
5. Do exactly what the task asks — do not silently expand, shrink or reshape it. If information is ambiguous, first complete everything that does not depend on the answer.

# Hard boundaries
- NEVER modify test files or test configuration (any path containing "test", plus conftest.py, tox.ini, pytest.ini, setup.cfg). The grading harness owns the tests; a patch touching them is rejected.
- Do NOT edit changelogs or release notes (CHANGES.rst, docs/changes*, docs/releases*) — grading ignores documentation; editing it only burns your turn budget.
- The container has NO usable internet access: pip install/download, curl, wget and git remotes fail. Never retry a failed download — implement the fix with what is already in the container.
- A run that ends with no code changes is an automatic failure.
- Every bash command runs in a fresh non-interactive shell starting in /testbed; shell state (cd, exported variables) does NOT persist between commands. Never open editors or interactive prompts; use non-interactive flags (e.g. -y, DEBIAN_FRONTEND=noninteractive). Commands may be killed by a timeout.

# How to finish (the ONLY way)
When your fix is verified:
1. Self-check: run `git diff --stat` and inspect `git diff` restricted to the files you changed; confirm only non-test source files changed and the change is minimal and focused on the issue.
2. Submit: run a bash command that PRINTS the token TASK_COMPLETE as its FIRST output line, e.g. `echo TASK_COMPLETE`.
Mentioning TASK_COMPLETE in plain text does NOT finish the run — only the printed output of an executed command does. After the token prints, the run ends immediately and you cannot continue working."#;

/// fix2 反考古护栏的触发模式（失败轨迹分析 §1.3 D-考古：51/145 run、单例烧 10-20 轮）。
/// 取证据里最明确的全库考古/全盘搜索形态，避免误伤正常源码搜索。
/// ← af-bench main.rs:570-577 原样。
pub const ARCHAEOLOGY_PATTERNS: &[&str] = &[
    "cat-file --batch-all-objects",
    "git fsck",
    "--lost-found",
    "git log --all --grep",
    "find / -name",
    "find / -type",
];
