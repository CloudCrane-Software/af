//! 判分器定位与调用——判分子进程的唯一调用点（af-bench 吸收方案 §3）。
//!
//! 独立性边界 = 子进程边界（§3.2-1）：af-execline 永不进程内链接/复刻判分逻辑，
//! 判分的一切调用收敛到本模块。argv 契约钉死——判分脚本恰好收到三个位置参数
//! `[task_json, eval.log, agent.diff]`、无额外 env 注入（子进程原样继承）、
//! 不经 wrapper shell（`Command` 直指解释器）——实现面（提示词/循环/工具）怎么
//! 改都碰不到判分。契约由 tests/p3_judge.rs 假判分 venv 行为断言 + 源文本比对
//! 双向钉死。
//!
//! 指纹门 fail-loud（§3.2-2）：`sha256(判分脚本)` 必须等于声明锚（默认 = 本机
//! 现产值 `a292547f…`，§3.1 实测；历史回放对账构造 [`JudgeLocator`] 指向
//! rsi-drill 冻结件 + `0338566c…` 锚即可，方案双锚口径）。不匹配 = EX3002 拒跑，
//! 不降级、不跳判、不静默换脚本。
//!
//! 判定值只从 judge stdout JSON 取（§3.2-3）：本模块只产出原始子进程结果
//! （`Output`），JSON 解读与 note 拼装原样留在 `suite::swev::verify`
//! （af-bench swev.rs:212-257 随迁），Rust 侧不做任何再解释。
//!
//! 判分脚本与 venv 绝对路径绑本机（方案 R2）：判分固定本机执行（容器在
//! anolis、judge 在本机的现行拓扑保持），不在 anolis 跑 judge。

use std::path::PathBuf;
use std::process::Command;

use crate::error::ExeclineError;

/// 判分器定位（方案 §3.2-2 / R2：路径参数化，默认 = 本机现产值；
/// P4 起进 SwevUnitSpec 声明，随 unit_version 版本化）。
#[derive(Debug, Clone)]
pub struct JudgeLocator {
    /// 判分 venv 的 python 解释器（swebench 4.1 装于其中，swev.rs:205）。
    pub python: PathBuf,
    /// 判分脚本路径（swev.rs:206；字节冻结由指纹门守）。
    pub script: PathBuf,
    /// 判分脚本 sha256 锚（64 位十六进制小写）。
    pub script_sha256: String,
}

impl JudgeLocator {
    /// 本机现产值（← af-bench swev.rs:205-209 硬编码逐字迁移）+ 现产指纹锚
    /// `a292547f…`（方案 §3.1 本机 sha256sum 实测，2026-10-24 复测一致）。
    pub fn production() -> Self {
        let _ = ();
        unimplemented!("P3 GREEN 落位：swev.rs:205-209 硬编码迁移（RED 先行，tests/p3_judge.rs）");
    }

    /// 指纹门（方案 §3.2-2 fail-loud）：`sha256(script)` 必须等于声明锚，否则
    /// EX3002 拒跑；脚本读不到 = 无法验真，同样拒跑。不降级、不跳判、不静默
    /// 换脚本。
    pub fn check_fingerprint(&self) -> Result<(), ExeclineError> {
        let _ = self;
        unimplemented!("P3 GREEN 落位：指纹门（RED 先行，tests/p3_judge.rs）");
    }
}

/// 判分子进程调用（方案 §3.2-1 唯一调用点；← af-bench swev.rs:205-210 原样，
/// 硬编码路径换 locator 字段）。argv = `[python, script, task_json, log, diff]`
/// ——判分脚本恰收到三个位置参数；不注入 env、不经 wrapper shell。
/// stdout/stderr/退出码原样交调用方解读（`suite::swev::verify`）。
pub fn invoke(
    locator: &JudgeLocator,
    task_json: &std::path::Path,
    log: &std::path::Path,
    diff: &std::path::Path,
) -> std::io::Result<std::process::Output> {
    let _ = Command::new(&locator.python);
    let _ = (task_json, log, diff);
    unimplemented!("P3 GREEN 落位：swev.rs:205-210 原样迁移（RED 先行，tests/p3_judge.rs）");
}
