//! SWE-bench Verified 套件——纯函数层（af-bench 吸收 P1，原样搬入）。
//!
//! 来源与去向（吸收方案 §1.1）：
//! - `SwevTask`/`load_swev_tasks`/`SwevTask::prompt`/`diff_touches_tests`
//!   ← af-bench swev.rs:16-139，函数体逐字节一致；
//! - 两段提示词 `SWEV_SYSTEM_PROMPT`/`SWEV_INSTANCE_PROTOCOL` 与反考古模式
//!   `ARCHAEOLOGY_PATTERNS` ← af-bench main.rs:543, 548-577，测量语义载荷
//!   一个字节不动（方案 §1.1 行 4）。
//!
//! 容器 setup 已随 P2 搬入（挂 HostBridge，代理注入与资源护栏进命令构造）；
//! capture_diff / verify / sha256_hex 属 P3（判分单元与证据链，方案 §5）。
//! 判分三件套（task_json/eval.log/agent.diff）语义由 P3 以原样模块产出，
//! 本包不动其接口。
//!
//! 行为逐字节等价由 tests/p1_comparison.rs 对照测试钉死（#[path] 直接编译
//! af-bench 原文件，同输入同输出），黄金值见 tests/p1_suite.rs；P2 生命周期
//! 命令构造由 tests/p2_bridge.rs FakeBridge 断言钉死。

use crate::hostbridge::HostBridge;
use crate::judge::JudgeLocator;
use crate::suite::tb::{self, TbEnv};
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

/// 起容器（← af-bench swev.rs:54-99 原样；模块级 `tb_ssh()` 换 `bridge.run()`，
/// 命令串与超时逐字节不变）。sweb.eval 镜像自带 /testbed 与全部测试资产，无需 build。
pub fn setup(
    bridge: &dyn HostBridge,
    host: &str,
    task: &SwevTask,
    profile: &str,
) -> Result<TbEnv, String> {
    let container = format!(
        "afbench-swev-{}-{profile}",
        task.instance_id.replace("__", "-")
    );
    let _ = bridge.run(host, &format!("docker rm -f {container}"), 30);
    // 出网代理按宿主区分：anolis 容器经宿主 squid（172.17.0.1:8123）；
    // 本机（local）容器走 docker NAT 直连（本机无 8123 监听），语义同为"容器可出网"。
    let proxy_env = if host == "local" {
        ""
    } else {
        "-e HTTPS_PROXY=http://172.17.0.1:8123 \
         -e HTTP_PROXY=http://172.17.0.1:8123 \
         -e NO_PROXY=localhost,127.0.0.1,100.64.0.0/10"
    };
    // fix2 环境卫生（mini-swe-agent env 块同源）：分页器挂死与进度条/着色噪音
    // 是 SWE 容器高频暗坑，容器级环境变量一次性根治。测量语义变更，计入 fix2。
    // 资源护栏（2026-10-21，常量与校准注释见 tb 模块）：agent 容器 6g/512。
    // 基建修复不改判分语义：上限远高于实测峰值（agent 期 30-90MiB）；
    // 本代际判分在该容器内原地执行，判分负载同受此封顶。
    let mem = tb::SWEV_AGENT_CONTAINER_MEM;
    let pids = tb::SWEV_AGENT_CONTAINER_PIDS;
    let script = format!(
        "docker run -d --name {container} \
         --memory {mem} --pids-limit {pids} \
         {proxy_env} \
         -e PAGER=cat -e MANPAGER=cat -e LESS=-R \
         -e PIP_PROGRESS_BAR=off -e TQDM_DISABLE=1 -e NO_COLOR=1 \
         {} sleep infinity",
        task.image
    );
    match bridge.run(host, &script, 120) {
        Ok((true, _)) => Ok(tb::TbEnv {
            container,
            task: task.instance_id.clone(),
            host: host.into(),
            workdir: "/testbed".into(),
        }),
        Ok((false, log)) => Err(format!(
            "起容器失败: {}",
            log.chars().take(400).collect::<String>()
        )),
        Err(err) => Err(err),
    }
}

/// 取证 agent diff（完成度门禁与判分共用）：写 work_dir/agent.diff 并返回文本。
/// （P3 目标 ← af-bench swev.rs:102-117 原样搬入，`tb_ssh()` 换 `bridge.run()`；
/// RED 桩，tests/p3_verify.rs。）
pub fn capture_diff(
    bridge: &dyn HostBridge,
    host: &str,
    env: &TbEnv,
    work_dir: &Path,
) -> Result<String, String> {
    let _ = (bridge, host, env, work_dir);
    unimplemented!(
        "P3 GREEN 落位：swev.rs:102-117 原样搬入接 bridge（RED 先行，tests/p3_verify.rs）"
    );
}

/// 判分：指纹门 → 取证 diff → 写入 eval 脚本 → 容器内执行 → 本地官方判分。
/// `task_json_path` 是 swev-prep 产出的原始任务包（判分器全字段来源）。
/// 返回 (resolved, note, eval_sha256)——eval_sha256 是判分日志的字节指纹
/// （D-20 项 2：判分可追溯；桥接/判分早期失败时为 None）。
/// （P3 目标 ← af-bench swev.rs:145-258 原样搬入：`tb_ssh()` 换 `bridge.run()`、
/// judge 直造 Command 换 [`crate::judge::invoke`]（argv 契约收敛点，路径来自
/// locator）、新增指纹门前置（方案 §3.2-2）；RED 桩，tests/p3_verify.rs。）
pub fn verify(
    bridge: &dyn HostBridge,
    judge: &JudgeLocator,
    host: &str,
    env: &TbEnv,
    task: &SwevTask,
    work_dir: &Path,
    task_json_path: &Path,
) -> (Option<bool>, String, Option<String>) {
    let _ = (bridge, judge, host, env, task, work_dir, task_json_path);
    unimplemented!("P3 GREEN 落位：swev.rs:145-258 原样搬入（RED 先行，tests/p3_verify.rs）");
}
