//! 读包装器与（随后的）写入器共用的类型化错误。

use std::path::PathBuf;

/// MES 读路径与写入器的错误。写入侧变体在批写入落地时使用。
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
