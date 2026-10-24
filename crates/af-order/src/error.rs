//! af-order 错误。稳定码 `OR1xxx`（用法）/ `OR2xxx`（spec / 凭证 / 状态），
//! 数字只增不改；引擎与账本失败透传 af-execline 的 `EX1xxx`/`EX2xxx`/`EX3xxx`
//! 稳定码，不改写、不吞错（ADR-007 §3 错误码契约）。

/// RED 桩错误：实现未落地（最小可编译形态，转绿时替换为完整错误表）。
#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    #[error("OR0001: 未实现: {detail}")]
    Unimplemented { detail: String },
}
