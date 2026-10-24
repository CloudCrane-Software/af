//! SWE-bench Verified 套件——纯函数层（af-bench 吸收 P1，RED 骨架）。
//!
//! 目标形态（绿提交落位，全部原样搬入）：
//! - `SwevTask`/`load_swev_tasks`/`SwevTask::prompt`/`diff_touches_tests`
//!   ← af-bench swev.rs:16-139；
//! - 两段提示词 `SWEV_SYSTEM_PROMPT`/`SWEV_INSTANCE_PROTOCOL` 与反考古模式
//!   `ARCHAEOLOGY_PATTERNS` ← af-bench main.rs:543, 548-577（测量语义载荷逐字节不动）。
//! 容器 setup / capture_diff / verify / sha256_hex 不属 P1：宿主桥接在 P2、
//! 判分单元与证据链在 P3（吸收方案 §5）。

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct SwevTask {
    pub instance_id: String,
    pub repo: String,
    pub problem_statement: String,
    pub image: String,
    pub eval_script: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub hints_text: String,
}

impl SwevTask {
    pub fn prompt(&self) -> String {
        unimplemented!("P1 RED 桩")
    }
}

pub fn load_swev_tasks(_root: &Path, _names: &[String]) -> Result<Vec<SwevTask>, String> {
    unimplemented!("P1 RED 桩")
}

pub fn diff_touches_tests(_diff: &str) -> Vec<String> {
    unimplemented!("P1 RED 桩")
}

/// RED 占位：绿提交替换为 af-bench main.rs:543 原文。
pub const SWEV_SYSTEM_PROMPT: &str = "";

/// RED 占位：绿提交替换为 af-bench main.rs:548-566 原文。
pub const SWEV_INSTANCE_PROTOCOL: &str = "";

/// RED 占位：绿提交替换为 af-bench main.rs:570-577 原文。
pub const ARCHAEOLOGY_PATTERNS: &[&str] = &[];
