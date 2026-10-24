//! bench 套件共享层（af-bench 吸收 P1）。
//!
//! 本模块收 af-bench 中与套件类型无关的纯函数（吸收方案 §1.1 "suite 层"）：
//! - `next_attempt_index` ← af-bench main.rs:594-610（D-20 run 目录序号递增不覆写）；
//! - `status_label` ← af-bench main.rs:579-588（RunStatus 六态映射）。
//! 套件专属物在子模块 [`swev`] / [`tb`]。本包不碰引擎、容器、网络、模型。

pub mod swev;
pub mod tb;

use af_harness::RunStatus;
use std::path::Path;

pub fn status_label(_status: &RunStatus) -> String {
    unimplemented!("P1 RED 桩：实现随绿提交自 af-bench main.rs:579-588 原样搬入")
}

pub fn next_attempt_index(_runs_root: &Path, _task_id: &str, _profile: &str) -> u32 {
    unimplemented!("P1 RED 桩：实现随绿提交自 af-bench main.rs:594-610 原样搬入")
}
