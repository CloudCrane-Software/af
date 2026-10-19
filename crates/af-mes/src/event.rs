//! 一行 MES 事件。字段顺序就是 RowBinary 列顺序，也是 DDL 列顺序。
//!
//! `occurred_at` 是 Unix 毫秒，对应 `DateTime64(3, 'UTC')` 的线格式（Int64）。
//! 不在这里调用 `rand64()`：它不是纯函数，重放和物化视图都会漂。

use serde::{Deserialize, Serialize};

/// 追加进 `mes_event` 的一行。MergeTree 不去重；`event_id` 供调用方自己认行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, clickhouse::Row)]
pub struct MesEvent {
    pub work_order_id: String,
    pub occurred_at: i64,
    pub event_id: String,
    pub line_id: String,
    pub station_id: String,
    pub kind: String,
    pub quantity: u64,
}

impl MesEvent {
    pub fn new(
        work_order_id: impl Into<String>,
        occurred_at_ms: i64,
        event_id: impl Into<String>,
        line_id: impl Into<String>,
        station_id: impl Into<String>,
        kind: impl Into<String>,
        quantity: u64,
    ) -> Self {
        Self {
            work_order_id: work_order_id.into(),
            occurred_at: occurred_at_ms,
            event_id: event_id.into(),
            line_id: line_id.into(),
            station_id: station_id.into(),
            kind: kind.into(),
            quantity,
        }
    }
}
