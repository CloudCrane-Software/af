//! af-trace —— 分级背压追踪通道（PoC）
//!
//! 双通道语义：
//!
//! - **critical（同步、fail-closed）**：[`Trace::write_critical`] 把事件放进有界 critical
//!   缓冲，由后台线程落盘（PoC 里是内存 JSON-line sink）。缓冲满时**立即返回
//!   `Err(TraceError::CriticalFull)`**，事件原样退还调用方，绝不静默丢弃；调用方必须处理
//!   （重试或中止）。返回 `Ok(seq)` 表示该事件**保证**出现在落盘结果里。
//! - **normal（异步、有损）**：[`Trace::write_normal`] 把事件放进有界 normal 队列，由后台
//!   线程消费到内存 sink。队列满时**立即丢弃并 `gap_counter += 1`**，不阻塞生产者。
//! - **缺口可验证**：每次 normal 丢弃都尝试发出一条 `kind="gap"` 的 critical span（含丢弃
//!   总数与丢弃 seq 区间）；若此刻 critical 缓冲也满，则在内部合并（coalesce）成一条更大
//!   的 gap span，[`Trace::shutdown`] 时强制补发。不变量：
//!   **`Σ(gap span.count) == gap_counter == 实际丢弃数`**。
//!
//! 契约：[`Trace::shutdown`] 必须在所有生产者线程结束之后调用。

mod error;
mod event;
mod trace;

pub use error::TraceError;
pub use event::{GapInfo, Level, TraceEvent};
pub use trace::{ConsumerHold, ShutdownReport, Trace, TraceConfig};
