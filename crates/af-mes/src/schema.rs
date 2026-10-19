//! MES 建库 DDL。语句是常量，apply 时原样提交，不拼接随机函数。
//!
//! 事件表是 MergeTree（裁决：追加日志，不是 ReplacingMergeTree）。
//! 分区 `toYYYYMM(occurred_at)` 按月。排序键只有工单 + 时刻。
//! 产线 / 小时聚合是物化视图，目标表用 SummingMergeTree，这样同一键的
//! 计数在合并时相加。SummingMergeTree 仍是 MergeTree 家族，源表不是它。
//!
//! 这里没有 `OPTIMIZE TABLE … FINAL`。该语句不跨分区合并；跨分区去重
//! 在读路径的 `SELECT … FINAL`（见 [`crate::select_final`]）。

use std::time::Duration;

/// 默认库名。端点本身由调用方注入，库名也可在配置里换。
pub const DEFAULT_DATABASE: &str = "af_mes";

pub const TABLE_MES_EVENT: &str = "mes_event";
pub const TABLE_MES_LINE_HOUR: &str = "mes_line_hour";
pub const VIEW_MES_LINE_HOUR: &str = "mes_line_hour_mv";

/// 行数阈值。先到这个数就发，不必再等延迟。
pub const DEFAULT_MAX_ROWS: u64 = 50_000;

/// 延迟阈值。先到这个间隔就发，不必攒满行数。
/// 比较的是注入时钟的差值，不是睡眠。
pub const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(1);

/// outbox 里未确认批的上限。达到后 `push` 返回背压，已落盘的批不丢。
pub const DEFAULT_MAX_OUTBOX_BATCHES: usize = 8;

pub const DDL_MES_EVENT: &str = "\
CREATE TABLE IF NOT EXISTS mes_event
(
    work_order_id String,
    occurred_at DateTime64(3, 'UTC'),
    event_id String,
    line_id String,
    station_id String,
    kind String,
    quantity UInt64
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(occurred_at)
ORDER BY (work_order_id, occurred_at)
";

pub const DDL_MES_LINE_HOUR: &str = "\
CREATE TABLE IF NOT EXISTS mes_line_hour
(
    line_id String,
    hour DateTime('UTC'),
    event_count UInt64,
    quantity_sum UInt64
)
ENGINE = SummingMergeTree
PARTITION BY toYYYYMM(hour)
ORDER BY (line_id, hour)
";

/// 从 `mes_event` 的插入块按产线、小时聚合。视图读的是插入块，不是 `SELECT FINAL`。
pub const DDL_MES_LINE_HOUR_MV: &str = "\
CREATE MATERIALIZED VIEW IF NOT EXISTS mes_line_hour_mv
TO mes_line_hour
AS
SELECT
    line_id,
    toStartOfHour(occurred_at) AS hour,
    count() AS event_count,
    sum(quantity) AS quantity_sum
FROM mes_event
GROUP BY line_id, hour
";

/// 建表顺序：事件表、聚合目标、物化视图。视图依赖前两张表。
pub const SCHEMA_STATEMENTS: &[&str] = &[DDL_MES_EVENT, DDL_MES_LINE_HOUR, DDL_MES_LINE_HOUR_MV];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_table_is_merge_tree_partitioned_by_month() {
        assert!(DDL_MES_EVENT.contains("ENGINE = MergeTree\n"));
        assert!(!DDL_MES_EVENT.contains("ReplacingMergeTree"));
        assert!(DDL_MES_EVENT.contains("PARTITION BY toYYYYMM(occurred_at)"));
        assert!(DDL_MES_EVENT.contains("ORDER BY (work_order_id, occurred_at)"));
        assert!(DDL_MES_EVENT.contains("DateTime64(3, 'UTC')"));
        assert!(DDL_MES_EVENT.contains("work_order_id String"));
    }

    #[test]
    fn materialized_view_aggregates_by_line_and_hour() {
        assert!(DDL_MES_LINE_HOUR.contains("ENGINE = SummingMergeTree"));
        assert!(DDL_MES_LINE_HOUR.contains("ORDER BY (line_id, hour)"));
        assert!(DDL_MES_LINE_HOUR_MV.contains("CREATE MATERIALIZED VIEW"));
        assert!(DDL_MES_LINE_HOUR_MV.contains("TO mes_line_hour"));
        assert!(DDL_MES_LINE_HOUR_MV.contains("toStartOfHour(occurred_at) AS hour"));
        assert!(DDL_MES_LINE_HOUR_MV.contains("GROUP BY line_id, hour"));
        assert!(DDL_MES_LINE_HOUR_MV.contains("sum(quantity) AS quantity_sum"));
        assert_eq!(
            SCHEMA_STATEMENTS,
            &[DDL_MES_EVENT, DDL_MES_LINE_HOUR, DDL_MES_LINE_HOUR_MV]
        );
    }

    #[test]
    fn schema_does_not_use_rand64_or_optimize_final() {
        for sql in SCHEMA_STATEMENTS {
            let folded = sql.to_ascii_lowercase();
            assert!(
                !folded.contains("rand64"),
                "rand64 不是纯函数，不许进 DDL: {sql}"
            );
            assert!(
                !folded.contains("optimize"),
                "OPTIMIZE FINAL 不跨分区，不许进 DDL: {sql}"
            );
        }
    }

    #[test]
    fn default_batch_row_threshold_is_fifty_thousand() {
        assert_eq!(DEFAULT_MAX_ROWS, 50_000);
    }
}
