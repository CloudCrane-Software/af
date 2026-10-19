//! 沙箱客户端错误。不可用、旧纪元、白名单拒绝和预算拒绝是不同的失败，不合并成一个字符串。

use af_credential::Denial;
use thiserror::Error;

use crate::budget::BudgetDecision;

pub type Result<T> = std::result::Result<T, SandboxError>;

#[derive(Debug, Error)]
pub enum SandboxError {
    /// 后端声明自己不可用，或者本次调用在到达执行前就发现后端不可用。
    #[error("沙箱不可用")]
    SandboxDown,

    /// 句柄上的纪元或 boot 与宿主当前值不一致。句柄不再代表活会话。
    #[error(
        "旧纪元会话被拒绝：request_epoch={request_epoch} current_epoch={current_epoch} request_boot={request_boot} current_boot={current_boot}"
    )]
    StaleEpoch {
        request_epoch: u64,
        current_epoch: u64,
        request_boot: u64,
        current_boot: u64,
    },

    /// boot 与纪元都能对上，但宿主没有这个会话。不向后端探活，也不补建。
    #[error("会话 {session_id} 不在本宿主的活表里")]
    SessionGone { session_id: u64 },

    /// 同一纪元里已经 close。不是旧纪元。
    #[error("会话 {session_id} 已关闭")]
    SessionClosed { session_id: u64 },

    /// 凭证没有放行 `sandbox:exec`。`denial` 是 af-credential 的原拒绝，不改写。
    ///
    /// `fact_present` 只说明事实层还有没有这个 tool。衰减后事实可以还在，
    /// 这个字段为真也不等于放行。
    #[error("工具白名单拒绝 {tool}（事实仍在={fact_present}）")]
    ToolDenied {
        tool: &'static str,
        fact_present: bool,
        denial: Denial,
    },

    /// `denial` / `inspect` 的真错误（执行限额、令牌损坏）。不是“拒绝”，也不放行。
    #[error("凭证判定失败（fail-closed）：{message}")]
    CredentialFault { message: String },

    /// 调用方交来的预算决策没有放行沙箱并发维。决策原样带回。
    #[error("沙箱并发预算未被调用方放行")]
    BudgetRejected { decision: BudgetDecision },

    /// 注入时钟已到会话上限，或后端报告的耗时到达上限。
    #[error("沙箱超时：timeout_ms={timeout_ms} elapsed_ms={elapsed_ms}")]
    TimedOut { timeout_ms: u64, elapsed_ms: u64 },

    #[error("镜像引用无效")]
    InvalidImage,

    #[error("资源限额无效：{field}")]
    InvalidResources { field: &'static str },

    #[error("命令无效")]
    InvalidCommand,

    #[error("产物路径无效：{path}")]
    InvalidArtifact { path: String },

    /// 请求的路径里至少有一个后端没有。不返回其余已读到的字节。
    #[error("产物不存在：{path}")]
    ArtifactMissing { path: String },

    #[error("沙箱后端拒绝：{detail}")]
    BackendRejected { detail: String },

    /// 纪元或会话号再加一会溢出。不再签发新句柄。
    #[error("纪元或会话计数已用尽")]
    CounterExhausted,

    #[error("沙箱宿主锁已损坏")]
    HostPoisoned,
}
