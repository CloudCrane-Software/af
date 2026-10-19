//! af-mes — MES 事件进 ClickHouse（L2）。
//!
//! 裁决（poc-mes，不许推翻）：
//! - 事件表是 MergeTree，按月分区，排序键是工单 + 时刻；聚合走物化视图。
//! - 批写入双阈值：行数（默认 5 万）或 `max_delay`，先到先发。
//! - 官方 `clickhouse` 0.15.3：HTTP + RowBinary + lz4。校验模式会改走
//!   `RowBinaryWithNamesAndTypes`，本 crate 关掉校验，钉住 RowBinary。
//! - `OPTIMIZE TABLE … FINAL` 不跨分区合并。读 ReplacingMergeTree 的跨分区残留
//!   用 [`select_final`]，不在写路径发 OPTIMIZE。
//! - `rand64()` 不是纯函数。DDL、物化视图和读包装器都不生成它。
//!
//! 端点由调用方注入。时刻由 [`Clock`] 注入；延迟阈值在 [`MesWriter::push`] /
//! [`MesWriter::poll`] 上比较，crate 内不睡眠、不起定时线程。
//!
//! 写入失败的批落进 outbox 文件再重试（至少一次，不丢）。outbox 达到上限时
//! 返回 [`MesError::Backpressure`]，该行不接收。
//! 真实 ClickHouse 不进 `cargo test`；集成用本 crate 的 [`mock`] HTTP 服务。

mod ch;
mod clock;
mod error;
mod event;
mod final_query;
pub mod mock;
mod outbox;
mod reader;
mod rowbin;
mod schema;
mod writer;

pub use clock::{Clock, ManualClock, SystemClock};
pub use error::{Accept, MesError};
pub use event::MesEvent;
pub use final_query::{guard_read_sql, select_final};
pub use reader::{EventFilter, MesReader};
pub use schema::{
    DDL_MES_EVENT, DDL_MES_LINE_HOUR, DDL_MES_LINE_HOUR_MV, DEFAULT_DATABASE, DEFAULT_MAX_DELAY,
    DEFAULT_MAX_OUTBOX_BATCHES, DEFAULT_MAX_ROWS, SCHEMA_STATEMENTS, TABLE_MES_EVENT,
    TABLE_MES_LINE_HOUR, VIEW_MES_LINE_HOUR,
};
pub use writer::{MesWriter, WriterConfig};
