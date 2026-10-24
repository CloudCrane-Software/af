//! 宿主桥接抽象（af-bench 吸收方案 §2.3 两个可注入缝之一，P2 落位）。
//!
//! `HostBridge` 是"在哪个 docker 宿主上执行一条命令"的唯一缝：trait 面按方案
//! §2.3 钉死——`host`（`"local"` 本机直执行，其余为 ssh 宿主别名）/ `script` /
//! `timeout_sec` 三参，返回 `(status, stdout+stderr 合并文本)`。真实实现
//! [`SshBridge`] = af-bench `tb.rs:42-64`（`ssh`）与 `swev.rs:268-293`（`tb_ssh`）
//! 收敛为一：两函数语义同构（timeout 包裹 + local/ssh 分支 + stderr 合并），
//! 行为以 tb.rs:42-64 为准（方案 §1.1 唯一合并点；两处 format 模板产出
//! 逐字节相等已实测证实）。测试注入 FakeBridge 断言命令构造（tests/p2_bridge.rs），
//! 零容器、零 ssh、零网络。
//!
//! anolis 分支（真 `ssh -o BatchMode=yes`）不在单测覆盖——属 L2 对拍面（方案 §4）；
//! 其命令构造由源文本比对钉死（tests/p2_bridge.rs 的 cmp_p2_* 系列）。

/// 宿主命令执行缝（方案 §2.3 原签名）。
pub trait HostBridge {
    /// 执行一条宿主命令，返回 (status, stdout+stderr 合并文本)。
    /// `host == "local"` 时本机直接执行（本机 docker 直执行面，免 ssh 一跳），
    /// 其余经 `ssh <host>`（BatchMode）。两种路径都用 timeout 包裹，超时语义一致。
    fn run(&self, host: &str, script: &str, timeout_sec: u32) -> Result<(bool, String), String>;
}

/// 真实桥接实现（← af-bench tb.rs:42-64 原样：函数 `ssh` 并入 trait 更名 `run`，
/// 函数体逐语句一致，仅模块级 `shell_quote` 调用改走 [`crate::suite::tb`] 的
/// P1 已迁副本）。
#[derive(Debug, Clone, Copy, Default)]
pub struct SshBridge;

impl HostBridge for SshBridge {
    fn run(&self, host: &str, script: &str, timeout_sec: u32) -> Result<(bool, String), String> {
        let _ = (host, script, timeout_sec);
        unimplemented!("P2 GREEN 落位：tb.rs:42-64 真实现（RED 先行，tests/p2_bridge.rs）");
    }
}
