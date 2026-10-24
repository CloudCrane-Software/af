//! Terminal-Bench 4.0 套件——纯函数层（af-bench 吸收 P1，原样搬入）。
//!
//! 来源与去向（吸收方案 §1.1，整文件搬入的 P1 子集）：
//! - `TbTask`/`load_tb_tasks` ← af-bench tb.rs:12-36；
//! - `shell_quote`/`split_rc`/`tail` ← af-bench tb.rs:66-92, 299-305——原为模块私有，
//!   可见性提升为 pub（P2 宿主桥接与 P5 TB 单元复用），函数体逐字节一致；
//! - 容器资源护栏常量 ← af-bench tb.rs:101-120（含 2026-10-21 anolis 事故校准注释）；
//! - `bash_tool_decl` ← af-bench tb.rs:307-324。
//!
//! `ssh` 桥接与 `TbEnv` 生命周期（setup/exec/verify/cleanup）不属 P1：
//! HostBridge 抽象在 P2、TB 单元在 P5（方案 §5）。
//!
//! 行为逐字节等价：`load_tb_tasks`/`bash_tool_decl`/护栏常量由 tests/p1_comparison.rs
//! 活源对拍（#[path] 编译 af-bench tb.rs 原文件）；`shell_quote`/`split_rc`/`tail`
//! 在 af-bench 为私有、无法外部对拍（见 p1_comparison.rs 模块注），以 tests/p1_suite.rs
//! 黄金值断言固化。

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
