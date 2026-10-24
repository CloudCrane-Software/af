//! Terminal-Bench 4.0 套件——纯函数层（af-bench 吸收 P1，原样搬入）。
//!
//! 来源与去向（吸收方案 §1.1，整文件搬入的 P1 子集）：
//! - `TbTask`/`load_tb_tasks` ← af-bench tb.rs:12-36；
//! - `shell_quote`/`split_rc`/`tail` ← af-bench tb.rs:66-92, 299-305——原为模块私有，
//!   可见性提升为 pub（P2 宿主桥接与 P5 TB 单元复用），函数体逐字节一致；
//! - 容器资源护栏常量 ← af-bench tb.rs:101-120（含 2026-10-21 anolis 事故校准注释）；
//! - `bash_tool_decl` ← af-bench tb.rs:307-324。
//!
//! `ssh` 桥接已随 P2 收敛为 [`crate::hostbridge::HostBridge`] 真实现（tb.rs:42-64
//! 与 swev.rs:268-293 合一，行为以 tb.rs:42-64 为准）；`TbEnv` 生命周期
//! setup/exec/cleanup 挂 bridge 随 P2 搬入；verify（tb.rs:226-292）留 P5 TB 单元。
//!
//! 行为逐字节等价：`load_tb_tasks`/`bash_tool_decl`/护栏常量由 tests/p1_comparison.rs
//! 活源对拍（#[path] 编译 af-bench tb.rs 原文件）；`shell_quote`/`split_rc`/`tail`
//! 在 af-bench 为私有、无法外部对拍（见 p1_comparison.rs 模块注），以 tests/p1_suite.rs
//! 黄金值断言固化。

use crate::hostbridge::HostBridge;
use serde_json::{json, Value};
use std::path::Path;

pub struct TbTask {
    pub task_id: String,
    pub prompt: String,
}

pub fn load_tb_tasks(root: &Path, names: &[String]) -> Result<Vec<TbTask>, String> {
    let mut out = Vec::new();
    for name in names {
        let dir = root.join(name);
        let prompt_path = dir.join("instruction.md");
        let prompt = std::fs::read_to_string(&prompt_path)
            .map_err(|err| format!("读 {} 失败: {err}", prompt_path.display()))?;
        if !dir.join("environment").join("Dockerfile").is_file() {
            return Err(format!("{name} 缺 environment/Dockerfile"));
        }
        if !dir.join("tests").join("test.sh").is_file() {
            return Err(format!("{name} 缺 tests/test.sh"));
        }
        out.push(TbTask {
            task_id: name.clone(),
            prompt,
        });
    }
    Ok(out)
}

/// ← af-bench tb.rs:66-68 原样（私有 → pub，体不动）。
pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// 从命令输出末尾剥离 `__AF_RC=<n>` 标记行，返回 (正文, 退出码)。
/// 标记行缺失 = 外层 timeout 杀掉了整个桥接（含 echo），调用方按超时处理。
/// 只删标记行本身，其后内容（stderr 合并流）原样保留。
/// ← af-bench tb.rs:70-92 原样（私有 → pub，体不动）。
pub fn split_rc(text: &str) -> (String, Option<i32>) {
    if let Some(pos) = text.rfind("__AF_RC=") {
        let line_start = text[..pos].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let line_end = text[pos..]
            .find('\n')
            .map(|p| pos + p)
            .unwrap_or(text.len());
        let line = text[line_start..line_end].trim();
        if let Some(rc) = line
            .strip_prefix("__AF_RC=")
            .and_then(|v| v.trim().parse::<i32>().ok())
        {
            let mut out = String::with_capacity(text.len());
            out.push_str(&text[..line_start]);
            out.push_str(&text[line_end..]);
            return (out, Some(rc));
        }
    }
    (text.to_string(), None)
}

// ---- 容器资源护栏（2026-10-21 运营者指令：既要吃满资源，又不能把机器跑死）----
// 事故背景：anolis（16C/125G）10-21 因"测评容器无内存上限 + 无 swap"耗尽假死，
// 运营者硬重启两次。此后 af-bench 在每个 docker 宿主创建的每个容器都带
// --memory/--pids-limit。口径：上限是天花板不是预留——实测单容器 agent 期
// 30-90MiB、判分期峰值 ~889MiB（本地臂 423 点采样），正常负载远低于此；
// 上限只为单容器失控（内存泄漏 / fork 炸弹）时不拖垮整机。
// 校准=按 anolis 125G×调度器并发度（factory-sched HOST_LIMITS anolis=14 臂齐发）：
//   agent 容器 6g×14=84G + 宿主系统与在役 ollama 30B（~18G）后仍留 ~19G 余量；
//   TB 任务/验证器容器 2g×14=28G（TB 为轻量挑战，实测峰值 <1G）。
// 本代际（fix2）判分在 agent 容器内原地执行（docker exec 喂官方 eval_script），
// 没有 fix3 的独立干净判分容器——判分负载随 agent 容器 6g/512 封顶，故不设
// 判分容器常量（4g/256 为 fix3 口径，见 main=f373113，不适用于本代际）。
// 常量数值与 main 护栏提交逐字一致；只往本代际已有的 docker run 创建点加标志，
// 不引入任何 fix3 行为。基建修复不改判分语义，结果代号仍 fix2。
// （← af-bench tb.rs:101-114 注释原样随迁。）
/// swev agent 容器（sweb.eval 镜像，agent 工作与原地判分都在这一个容器里）。
pub const SWEV_AGENT_CONTAINER_MEM: &str = "6g";
pub const SWEV_AGENT_CONTAINER_PIDS: &str = "512";
/// TB 路径全部容器统一口径（任务环境/清卷辅助/验证器容器，均为轻量负载）。
pub const TB_CONTAINER_MEM: &str = "2g";
pub const TB_CONTAINER_PIDS: &str = "256";

/// TB system 提示（← af-bench main.rs:975 原样，一个字节不动——测量语义载荷）。
/// fix2：完成协议随 harness 语义变更为"提交命令输出首行"——TB 提示同步；
/// 与 swe-v 不同，TB 无 instance 纪律模板（main.rs:1015 user_message 即裸 prompt）。
pub const TB_SYSTEM_PROMPT: &str = "You are an autonomous engineering agent working inside a Linux container. Complete the task using the bash tool. The working directory /app contains the task materials. When the task is fully done and verified, finish by running a bash command that prints TASK_COMPLETE as its FIRST output line (e.g. `echo TASK_COMPLETE`). Mentioning TASK_COMPLETE in plain text does not finish the run; after the token prints you cannot continue working.";

/// ← af-bench tb.rs:299-305 原样（私有 → pub，体不动）。
pub fn tail(text: &str, cap: usize) -> String {
    if text.len() <= cap {
        text.to_string()
    } else {
        text[text.len() - cap..].to_string()
    }
}

/// bash 工具声明（ToolDecl JSON）。`workdir` 写进描述，与 exec 的 cd 前缀一致，
/// 避免"工具描述说 /app、实际在 /testbed"的目录矛盾（S0 零补丁缺陷嫌疑之一）。
/// fix2：描述补防错条款（mcode 式"把失败模式写进描述"）——新 shell 语义、
/// 非交互要求、超时行为、exit 行在末尾（首行留给命令输出，配合提交式完成判定）。
/// ← af-bench tb.rs:307-324 原样。
pub fn bash_tool_decl(workdir: &str) -> Value {
    json!({
        "name": "bash",
        "description": format!("Execute a bash command in the task environment (a Linux container). Every command runs in a fresh non-interactive shell with the working directory set to {workdir}; shell state (cd, exported variables) does not persist between commands. Interactive commands (editors, prompts) and commands that never exit are killed by a timeout — use non-interactive flags. The tool returns the command's output followed by a final `exit=<code>` line. Use it to inspect, modify, build and test."),
        "parameters": {
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "The bash command to execute."}
            },
            "required": ["command"]
        },
        "effect": "external_once"
    })
}

// ---- TbEnv 生命周期（P2 搬入，挂 HostBridge；verify 留 P5 TB 单元）----

/// 任务容器句柄（← af-bench tb.rs:94-99 原样）。
/// 字段与可见性逐字保留（P5 TB verify 容器链消费 container/task/host）。
/// P4 增 `Clone`（派生，行为无变化）：swev 单元的 bash 工具闭包需拥有环境
/// 句柄（af-harness 工具函数体 'static，run.rs:53-55），Arc<TbEnv> 克隆进闭包。
#[derive(Clone)]
pub struct TbEnv {
    pub container: String,
    pub task: String,
    pub host: String,
    pub workdir: String,
}

impl TbEnv {
    /// 建环境与容器（← af-bench tb.rs:122-171 原样；模块级 `ssh()` 换 `bridge.run()`，
    /// 命令串与超时逐字节不变）。任务目录须已在 anolis:/data/bench/tb/<task>。
    pub fn setup(
        bridge: &dyn HostBridge,
        host: &str,
        task: &TbTask,
        profile: &str,
    ) -> Result<Self, String> {
        let task = &task.task_id;
        let image = format!("afbench-tbenv-{task}");
        let container = format!("afbench-{task}-{profile}");
        let remote_dir = format!("/data/bench/tb/{task}");
        let (ok, log) = bridge.run(
            host,
            &format!(
                "docker build -q --network host \
                 --build-arg HTTPS_PROXY=http://127.0.0.1:8123 \
                 --build-arg HTTP_PROXY=http://127.0.0.1:8123 \
                 --build-arg NO_PROXY=localhost,127.0.0.1,100.64.0.0/10 \
                  \
                  \
                 -t {image} {remote_dir}/environment"
            ),
            1800,
        )?;
        if !ok {
            return Err(format!("build {image} 失败: {}", tail(&log, 800)));
        }
        let _ = bridge.run(host, &format!("docker rm -f {container}"), 30);
        // fix2 环境卫生（mini-swe-agent env 块同源）：分页器挂死与进度条/着色噪音
        // 是 SWE 容器高频暗坑，容器级环境变量一次性根治。测量语义变更，计入 fix2。
        let (ok, log) = bridge.run(
            host,
            &format!(
                "docker run -d --name {container} \
                 --memory {TB_CONTAINER_MEM} --pids-limit {TB_CONTAINER_PIDS} \
                 -e HTTPS_PROXY=http://172.17.0.1:8123 \
                 -e HTTP_PROXY=http://172.17.0.1:8123 \
                 -e NO_PROXY=localhost,127.0.0.1,100.64.0.0/10 \
                 -e PAGER=cat -e MANPAGER=cat -e LESS=-R \
                 -e PIP_PROGRESS_BAR=off -e TQDM_DISABLE=1 -e NO_COLOR=1 \
                 {image} sleep infinity"
            ),
            60,
        )?;
        if !ok {
            return Err(format!("起容器 {container} 失败: {}", tail(&log, 400)));
        }
        Ok(Self {
            container,
            task: task.clone(),
            host: host.into(),
            workdir: "/app".into(),
        })
    }

    /// 任务工作目录（TB=/app，SWE-V=/testbed）——bash 工具每条命令都先 cd 到这里。
    /// ← af-bench tb.rs:173-176 原样。
    pub fn workdir(&self) -> &str {
        &self.workdir
    }

    /// bash 工具体：容器内执行，fix2 带内恢复语义（不改判分链）：
    ///   · exit=N 从输出首行移到**末行**——首行留给命令自身输出，完成判定
    ///     「工具输出首行=信号词」才有因果可讲；分隔改真实换行（原字面 `\n` 疵点一并修）；
    ///   · 超时（外层 timeout 杀桥接，无 __AF_RC 标记）→ mini-swe-agent timeout_template
    ///     同源教学反馈，运行继续而非死文案；
    ///   · 桥接失败 → 声明"基建瞬断、可重试"，同样不死；
    ///   · 普通非零退出不教学（失败是常态信号，逐条指点只会加噪）；
    ///   · 截断附「换窄命令」指引（mini observation_template 同源）。
    /// （← af-bench tb.rs:178-222 原样，`ssh()` 换 `bridge.run()`。）
    pub fn exec(&self, bridge: &dyn HostBridge, command: &str) -> String {
        let script = format!(
            "docker exec {} bash -lc {}; echo __AF_RC=$?",
            self.container,
            shell_quote(&format!("cd {} && {}", self.workdir, command))
        );
        let (mut text, rc) = match bridge.run(&self.host, &script, 180) {
            Ok((_, raw)) => split_rc(&raw),
            Err(err) => {
                return format!(
                    "[harness] The execution bridge failed before the command could produce output (transient infrastructure error: {}). This is not a task failure — retry the command.\nexit=127",
                    err
                );
            }
        };
        if rc.is_none() {
            text.push_str(
                "\n[harness] The command was killed after exceeding the 180s timeout (no exit code was reported). Partial output (if any) is above. Re-run with a different approach: pipe input or use non-interactive flags (-y / -n / DEBIAN_FRONTEND=noninteractive), wrap long work in an explicit `timeout <secs> ...`, and avoid commands that never exit on their own (servers, watch loops, pagers).",
            );
        }
        const CAP: usize = 12_000;
        if text.len() > CAP {
            let keep = CAP / 2;
            text = format!(
                "{}\n...[output truncated: {} bytes elided. Re-run with a narrower command: pipe through head/tail/sed -n, grep for specific patterns, or redirect output to a file and read it in parts.]\n{}",
                &text[..keep],
                text.len() - keep * 2,
                &text[text.len() - keep..]
            );
        }
        text.push_str(&format!(
            "\nexit={}",
            rc.map(|n| n.to_string())
                .unwrap_or_else(|| "124".to_string())
        ));
        text
    }

    /// 判分：/app 提取 → 验证器镜像 → 挂载跑 test.sh → reward.txt。
    /// 返回 (reward, verifier_log_tail)。reward: Some(true)=pass Some(false)=fail
    /// None=infra（无 reward.txt / 提取失败 / build 失败 / 读失败）。
    /// （← af-bench tb.rs:224-292 原样搬入，P5：`ssh()` 换 `bridge.run()`，
    /// 命令串与超时逐字节不变——TB 判分是容器链不是 swev-judge 子进程，
    /// 判分三件套契约不适用本套件。）
    pub fn verify(&self, _bridge: &dyn HostBridge) -> (Option<bool>, String) {
        unimplemented!("P5 GREEN 落位：TbEnv::verify")
    }

    /// 清容器（← af-bench tb.rs:294-296 原样，`ssh()` 换 `bridge.run()`）。
    pub fn cleanup(&self, bridge: &dyn HostBridge) {
        let _ = bridge.run(&self.host, &format!("docker rm -f {}", self.container), 60);
    }
}
