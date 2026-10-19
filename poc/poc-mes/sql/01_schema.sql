-- MES 三种建模 schema 草案（ClickHouse 26.7.34.3）
-- 公共事件列：seq 行号/版本, job_id 工单, line 产线, station 工位, event_type(start|complete|defect),
--            ts DateTime64(3), qty_good 合格数, qty_defect 缺陷数, defect_code 缺陷码, cycle_ms 周期时间(仅 complete)

DROP DATABASE IF EXISTS mes;
CREATE DATABASE mes;

-- ========== 模型 A：MergeTree 明细 + 物化视图增量聚合 ==========
CREATE TABLE mes.events_mt (
    seq         UInt64,
    job_id      UInt64,
    line        LowCardinality(String),
    station     LowCardinality(String),
    event_type  LowCardinality(String),
    ts          DateTime64(3),
    qty_good    UInt32,
    qty_defect  UInt32,
    defect_code LowCardinality(String),
    cycle_ms    UInt32
) ENGINE = MergeTree
PARTITION BY toYYYYMMDD(ts)
ORDER BY (line, station, ts, event_type, job_id);

-- 小时滚动聚合目标表（MV 增量写入，同键可能多行，查询端需再 GROUP BY）
CREATE TABLE mes.agg_hourly (
    line       LowCardinality(String),
    hour       DateTime,
    starts     UInt64,
    completes  UInt64,
    defects    UInt64,
    good_qty   UInt64,
    defect_qty UInt64,
    cycle_n    UInt64,
    cycle_sum  UInt64
) ENGINE = MergeTree
PARTITION BY toYYYYMM(hour)
ORDER BY (line, hour);

CREATE MATERIALIZED VIEW mes.mv_hourly TO mes.agg_hourly AS
SELECT
    line,
    toStartOfHour(ts) AS hour,
    countIf(event_type = 'start')    AS starts,
    countIf(event_type = 'complete') AS completes,
    countIf(event_type = 'defect')   AS defects,
    sum(qty_good)  AS good_qty,
    sum(qty_defect) AS defect_qty,
    countIf(event_type = 'complete') AS cycle_n,
    sumIf(cycle_ms, event_type = 'complete') AS cycle_sum
FROM mes.events_mt
GROUP BY line, hour;

-- ========== 模型 B：ReplacingMergeTree（键=业务去重键，版本=seq，查询端 FINAL）==========
CREATE TABLE mes.events_repl (
    seq         UInt64,
    job_id      UInt64,
    line        LowCardinality(String),
    station     LowCardinality(String),
    event_type  LowCardinality(String),
    ts          DateTime64(3),
    qty_good    UInt32,
    qty_defect  UInt32,
    defect_code LowCardinality(String),
    cycle_ms    UInt32
) ENGINE = ReplacingMergeTree(seq)
PARTITION BY toYYYYMMDD(ts)
ORDER BY (job_id, event_type, station);

-- ========== 模型 C：AggregatingMergeTree（写入端预聚合为 state，无明细）==========
CREATE TABLE mes.hourly_agg (
    line       LowCardinality(String),
    hour       DateTime,
    starts     AggregateFunction(sum, UInt64),
    completes  AggregateFunction(sum, UInt64),
    defects    AggregateFunction(sum, UInt64),
    good_qty   AggregateFunction(sum, UInt64),
    defect_qty AggregateFunction(sum, UInt64),
    cycle_n    AggregateFunction(sum, UInt64),
    cycle_sum  AggregateFunction(sum, UInt64),
    cycle_med  AggregateFunction(quantile, Nullable(UInt32))  -- 加分项：不可加的 p50 状态
) ENGINE = AggregatingMergeTree
PARTITION BY toYYYYMM(hour)
ORDER BY (line, hour);

-- ========== 写入基准表（Rust 双路径共用）==========
CREATE TABLE mes.bench_events (
    seq         UInt64,
    job_id      UInt64,
    line        LowCardinality(String),
    station     LowCardinality(String),
    event_type  LowCardinality(String),
    ts          DateTime,
    qty_good    UInt32,
    qty_defect  UInt32,
    defect_code LowCardinality(String),
    cycle_ms    UInt32
) ENGINE = MergeTree
PARTITION BY toYYYYMMDD(ts)
ORDER BY (seq);
