//! bench 套件共享层（af-bench 吸收 P1）。
//!
//! 本模块收 af-bench 中与套件类型无关的纯函数（吸收方案 §1.1 "suite 层"）：
//! - `status_label` ← af-bench main.rs:579-588（RunStatus 六态映射），原样搬入；
//! - `next_attempt_index` ← af-bench main.rs:594-610（D-20 run 目录序号递增不覆写），
//!   原样搬入（其单测 main.rs:48-73 随迁至 tests/p1_suite.rs）。
//!
//! 套件专属物在子模块 [`swev`] / [`tb`]；执行单元层 [`swev_unit`]（P4）与
//! [`tb_unit`]（P5）。本包不碰引擎、容器、网络、模型
//! （`resolve_key` 的 bash 子进程仅密钥解析器调用，测试用 printf 假命令，零真实凭证）。

pub mod report_unit;
pub mod swev;
pub mod swev_unit;
pub mod tb;
pub mod tb_unit;

use af_harness::RunStatus;
use std::path::Path;

/// ← af-bench main.rs:579-588 原样。
pub fn status_label(status: &RunStatus) -> String {
    match status {
        RunStatus::Completed { .. } => "completed".into(),
        RunStatus::MaxTurns => "max_turns".into(),
        RunStatus::ToolFreeLimit => "tool_free_limit".into(),
        RunStatus::BudgetExhausted { .. } => "budget_exhausted".into(),
        RunStatus::Stopped { .. } => "stopped".into(),
        RunStatus::Unauthorized { .. } => "unauthorized".into(),
    }
}

/// D-20 项 1（判分复核实证的假阴性根因修复）：swev run 目录唯一化。
/// 旧实现固定 `swev-<task>-<profile>-a1` 且先 `remove_dir_all`——同一 --out 重跑同题时
/// 第二跑静默覆写第一跑的 eval.log/agent.diff（step5-12907 假阴性即此机理）。
/// 新实现：后缀序号按既有同名前缀目录的最大序号 +1 递增（a1/a2/…），绝不删除旧目录。
/// ← af-bench main.rs:590-610 原样。
pub fn next_attempt_index(runs_root: &Path, task_id: &str, profile: &str) -> u32 {
    next_attempt_index_with_prefix(runs_root, &format!("swev-{task_id}-{profile}-a"))
}

/// 前缀参数化形态（图执行器 `graph-<task>-<profile>-a<N>` 同 D-20 递增语义；
/// swev 先例的泛化，行为对既有 swev 前缀逐字节不变）。
pub fn next_attempt_index_with_prefix(runs_root: &Path, prefix: &str) -> u32 {
    let mut max = 0u32;
    if let Ok(entries) = std::fs::read_dir(runs_root) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let lossy = name.to_string_lossy();
            let Some(rest) = lossy.strip_prefix(prefix) else {
                continue;
            };
            if let Ok(n) = rest.parse::<u32>() {
                max = max.max(n);
            }
        }
    }
    max + 1
}
