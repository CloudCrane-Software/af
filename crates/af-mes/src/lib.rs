//! af-mes — MES 事件进 ClickHouse（L2）。
//!
//! 裁决（poc-mes，不许推翻）：
//! - 事件表是 MergeTree，按月分区，排序键是工单 + 时刻；聚合走物化视图。
//! - 批写入双阈值：行数（默认 5 万）或 `max_delay`，先到先发。
//! - 官方 `clickhouse` 0.15.3：HTTP + RowBinary + lz4。
//! - `OPTIMIZE TABLE … FINAL` 不跨分区合并。读 ReplacingMergeTree 的跨分区残留
//!   用 [`select_final`]，不在写路径发 OPTIMIZE。
//! - `rand64()` 不是纯函数。DDL、物化视图和读包装器都不生成它。
//!
//! 时刻由 [`Clock`] 注入。真实 ClickHouse 不进 `cargo test`。

mod clock;
mod error;
mod event;
mod final_query;
mod schema;

pub use clock::{Clock, ManualClock, SystemClock};
pub use error::MesError;
pub use event::MesEvent;
pub use final_query::{guard_read_sql, select_final};
pub use schema::{
    DDL_MES_EVENT, DDL_MES_LINE_HOUR, DDL_MES_LINE_HOUR_MV, DEFAULT_DATABASE, DEFAULT_MAX_DELAY,
    DEFAULT_MAX_OUTBOX_BATCHES, DEFAULT_MAX_ROWS, SCHEMA_STATEMENTS, TABLE_MES_EVENT,
    TABLE_MES_LINE_HOUR, VIEW_MES_LINE_HOUR,
};
