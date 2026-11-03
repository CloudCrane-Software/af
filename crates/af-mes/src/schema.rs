//! MES 建库 DDL。语句是常量，apply 时原样提交，不拼接随机函数。
//!
//! 事件表是 MergeTree（裁决：追加日志，不是 ReplacingMergeTree）。
//! 分区 `toYYYYMM(occurred_at)` 按月。排序键只有工单 + 时刻。
//! 产线 / 小时聚合是物化视图，目标表用 SummingMergeTree，这样同一键的
//! 计数在合并时相加。SummingMergeTree 仍是 MergeTree 家族，源表不是它。
//!
//! 这里没有 `OPTIMIZE TABLE … FINAL`。该语句不跨分区合并；跨分区去重
//! 在读路径的 `SELECT … FINAL`（见 [`crate::select_final`]）。
//!
//! 事件表末列 `ingested_at` 是入库时刻锚（2026-11-03 修）：`MATERIALIZED now64(3)`
//! 由服务端在写入时计算，插入方不可指定（列清单带上它即被 CH 拒绝）——此前全表
//! 只有写手自报的 `occurred_at`（DEFAULT now64 仅缺省兜底），冒烟写手曾把 7 行
//! 2027 未来戳直打生产库（202701 分区），与真实事件结构性不可分。对旧表由
//! [`DDL_MES_EVENT_INGEST_ANCHOR`]（幂等 ADD COLUMN IF NOT EXISTS）补列。

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
    quantity UInt64,
    ingested_at DateTime64(3, 'UTC') MATERIALIZED now64(3)
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

/// 已存在的旧 `mes_event`（无锚列）补上入库时刻锚。幂等（ADD COLUMN IF NOT
/// EXISTS），随 [`SCHEMA_STATEMENTS`] 走 apply_schema——旧表每次 apply 即被补列。
/// 只加列不改数据：存量行的值回填是一次性运维动作（`ALTER … MATERIALIZE COLUMN`，
/// 重写数据部），不进本清单、不随 apply 反复跑。
pub const DDL_MES_EVENT_INGEST_ANCHOR: &str = "\
ALTER TABLE mes_event
ADD COLUMN IF NOT EXISTS ingested_at DateTime64(3, 'UTC') MATERIALIZED now64(3)
";

/// 建表顺序：事件表、聚合目标、物化视图、旧表补锚列。视图依赖前两张表。
pub const SCHEMA_STATEMENTS: &[&str] = &[
    DDL_MES_EVENT,
    DDL_MES_LINE_HOUR,
    DDL_MES_LINE_HOUR_MV,
    DDL_MES_EVENT_INGEST_ANCHOR,
];

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
    fn event_table_has_server_side_ingest_anchor() {
        // 入库时刻锚（2026-11-03 修）：MATERIALIZED now64(3) 由服务端写入时计算，
        // 插入方不可指定——此前全表唯一时间轴是写手自报 occurred_at，冒烟写手
        // 曾把 2027 未来戳直打生产库且与真实事件结构性不可分。
        let anchor = "ingested_at DateTime64(3, 'UTC') MATERIALIZED now64(3)";
        assert!(
            DDL_MES_EVENT.contains(anchor),
            "建表 DDL 必须带入库时刻锚: {DDL_MES_EVENT}"
        );
        // 锚必须是末列：RowBinary/列清单插入按位置映射普通列，末列追加不动既有列位。
        assert!(
            DDL_MES_EVENT
                .contains("quantity UInt64,\n    ingested_at DateTime64(3, 'UTC') MATERIALIZED now64(3)\n)"),
            "锚列必须排在 quantity 之后（列位兼容防回归）: {DDL_MES_EVENT}"
        );
        assert!(
            DDL_MES_EVENT_INGEST_ANCHOR.contains("ALTER TABLE mes_event")
                && DDL_MES_EVENT_INGEST_ANCHOR.contains(anchor)
                && DDL_MES_EVENT_INGEST_ANCHOR.contains("ADD COLUMN IF NOT EXISTS"),
            "旧表补锚列语句必须幂等且带同一定义的锚: {DDL_MES_EVENT_INGEST_ANCHOR}"
        );
        assert_eq!(
            SCHEMA_STATEMENTS.last(),
            Some(&DDL_MES_EVENT_INGEST_ANCHOR),
            "补锚列语句必须在 apply 序列内，否则旧表永远缺锚"
        );
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
            &[
                DDL_MES_EVENT,
                DDL_MES_LINE_HOUR,
                DDL_MES_LINE_HOUR_MV,
                DDL_MES_EVENT_INGEST_ANCHOR
            ]
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
