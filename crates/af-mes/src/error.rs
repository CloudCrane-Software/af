//! 写入、outbox 与读包装器的类型化错误。
//!
//! [`MesError::Backpressure`]：这一行没有被接收，调用方保留它。
//! [`MesError::OutboxPersist`]：这一行已经在内存批里。调用方不得把同一行再 `push`；
//! 修掉 outbox 目录后调用 [`crate::MesWriter::flush`]。

use std::path::PathBuf;

/// `push` / `flush` / `poll` 在数据没有丢时的结果。
///
/// `Err` 才表示这一行仍归调用方（背压），或批留在内存里出不去（outbox 落盘失败）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Accept {
    /// 还在内存批里。行数和延迟都没到。
    Buffered { pending: usize },
    /// ClickHouse 已接受这批。`rows == 0` 表示没有待发的批。
    Sent { rows: usize },
    /// ClickHouse 没接住。整批已写入 outbox，之后由 retry 重放。
    /// 调用方不得把同一行再 `push` 一次。
    Parked { rows: usize },
}

/// MES 写入器与读路径的错误。
#[derive(Debug, thiserror::Error)]
pub enum MesError {
    #[error("配置无效: {0}")]
    Config(String),

    /// 不能再收新行。事件不在写入器里。
    #[error(
        "背压（{reason}）：内存 {pending_rows} 行，outbox {outbox_batches}/{outbox_limit} 批，事件未接收"
    )]
    Backpressure {
        pending_rows: usize,
        outbox_batches: usize,
        outbox_limit: usize,
        reason: &'static str,
    },

    /// ClickHouse 拒绝且 outbox 文件也没写成。批仍在内存。
    #[error("写入失败且 outbox 未能落盘，批仍留在内存（{rows} 行）: {detail}")]
    OutboxPersist { rows: usize, detail: String },

    /// 重试时 ClickHouse 仍失败。该文件保留，前面已成功的文件已删除。
    #[error("outbox 重试仍失败，文件保留 {path}: {detail}")]
    RetryFailed { path: PathBuf, detail: String },

    #[error("ClickHouse: {0}")]
    ClickHouse(String),

    #[error("非法读查询: {0}")]
    BadQuery(String),

    #[error("outbox 文件损坏 {path}: {detail}")]
    CorruptOutbox { path: PathBuf, detail: String },

    #[error("io: {0}")]
    Io(String),
}
