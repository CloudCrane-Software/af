//! Terminal-Bench 4.0 套件——纯函数层（af-bench 吸收 P1，RED 骨架）。
//!
//! 目标形态（绿提交落位，全部原样搬入）：
//! - `TbTask`/`load_tb_tasks` ← af-bench tb.rs:12-36；
//! - `shell_quote`/`split_rc`/`tail` ← af-bench tb.rs:66-92, 299-305（可见性提升为 pub，
//!   函数体逐字节不动——P2 宿主桥接与 P5 TB 单元复用）；
//! - 容器资源护栏常量 ← af-bench tb.rs:101-120（含校准注释与常量守卫测试）；
//! - `bash_tool_decl` ← af-bench tb.rs:307-324。
//! `ssh` 桥接与 `TbEnv` 生命周期不属 P1：HostBridge 抽象在 P2、TB 单元在 P5。

use serde_json::Value;
use std::path::Path;

pub struct TbTask {
    pub task_id: String,
    pub prompt: String,
}

pub fn load_tb_tasks(_root: &Path, _names: &[String]) -> Result<Vec<TbTask>, String> {
    unimplemented!("P1 RED 桩")
}

pub fn shell_quote(_text: &str) -> String {
    unimplemented!("P1 RED 桩")
}

pub fn split_rc(_text: &str) -> (String, Option<i32>) {
    unimplemented!("P1 RED 桩")
}

pub fn tail(_text: &str, _cap: usize) -> String {
    unimplemented!("P1 RED 桩")
}

pub fn bash_tool_decl(_workdir: &str) -> Value {
    unimplemented!("P1 RED 桩")
}

/// RED 占位：绿提交替换为 af-bench tb.rs 校准口径（6g/512）。
pub const SWEV_AGENT_CONTAINER_MEM: &str = "";
/// RED 占位：同上。
pub const SWEV_AGENT_CONTAINER_PIDS: &str = "";
/// RED 占位：绿提交替换为 af-bench tb.rs 校准口径（2g/256）。
pub const TB_CONTAINER_MEM: &str = "";
/// RED 占位：同上。
pub const TB_CONTAINER_PIDS: &str = "";
