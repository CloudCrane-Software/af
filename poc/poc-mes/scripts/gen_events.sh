#!/usr/bin/env bash
# 生成模拟 MES 事件流：100 万行主数据 + 2 万行迟到回填数据（Parquet，确定性 rand64(seed)）
set -euo pipefail
CH=/home/pan-ding/ch-mes/bin/clickhouse
UF=/home/pan-ding/ch-mes/user_files
mkdir -p "$UF"

# 事件列模式（Parquet 往返声明的 schema，生成与读取必须同列序）
SCHEMA='seq UInt64, job_id UInt64, line LowCardinality(String), station LowCardinality(String), event_type LowCardinality(String), ts DateTime64(3), qty_good UInt32, qty_defect UInt32, defect_code LowCardinality(String), cycle_ms UInt32'

# ---------- 主数据 1,000,000 行 ----------
# 每 10 个事件一个工单：p=0..9 依次 ST01(start,complete,defect) ST02(...) ST03(...) ST04(...)
# 时间轴：base + n*1200ms + 抖动(<=250ms)；随机源一律 xxHash64(toString(n)+tag) 纯函数（rand64(seed) 非纯函数，仅首次播种） → 1M 行覆盖约 13.9 天，同工单事件时间连续且 start 先于 complete
GEN_MAIN="
SELECT
    number AS seq,
    intDiv(number, 10) AS job_id,
    concat('L', toString(1 + intDiv(number, 10) % 5)) AS line,
    multiIf(p <= 1, 'ST01', p <= 4, 'ST02', p <= 6, 'ST03', 'ST04') AS station,
    multiIf(p IN (0, 3, 5, 7), 'start', p IN (1, 4, 6, 8), 'complete', 'defect') AS event_type,
    toDateTime64('2026-09-07 06:00:00', 3, 'Asia/Shanghai') + toIntervalMillisecond(number * 1200 + xxHash64(concat(toString(number), 'j')) % 250) AS ts,
    if(event_type = 'complete', toUInt32(38 + xxHash64(concat(toString(number), 'a')) % 23), toUInt32(0)) AS qty_good,
    if(event_type = 'defect', toUInt32(1 + xxHash64(concat(toString(number), 'b')) % 4), toUInt32(0)) AS qty_defect,
    if(event_type = 'defect', concat('D', leftPad(toString(1 + xxHash64(concat(toString(number), 'c')) % 8), 2, '0')), '') AS defect_code,
    toUInt32(if(event_type = 'complete',
                multiIf(station = 'ST01', 28000, station = 'ST02', 35000, station = 'ST03', 40000, 30000)
                    + xxHash64(concat(toString(number), 'd')) % 9000, 0)) AS cycle_ms
FROM (SELECT number, number % 10 AS p FROM numbers(1000000))"

# ---------- 迟到回填 20,000 行 ----------
# 16,000 行全新事件（seq 2,000,000+，全新工单 200,000+，ts 落在头 3 天历史窗口内）
GEN_LATE_NEW="
SELECT
    number AS seq,
    intDiv(number, 10) AS job_id,
    concat('L', toString(1 + intDiv(number, 10) % 5)) AS line,
    multiIf(p <= 1, 'ST01', p <= 4, 'ST02', p <= 6, 'ST03', 'ST04') AS station,
    multiIf(p IN (0, 3, 5, 7), 'start', p IN (1, 4, 6, 8), 'complete', 'defect') AS event_type,
    toDateTime64('2026-09-07 06:00:00', 3, 'Asia/Shanghai') + toIntervalMillisecond(xxHash64(concat(toString(number), 't')) % 259200000) AS ts,
    if(event_type = 'complete', toUInt32(38 + xxHash64(concat(toString(number), 'a')) % 23), toUInt32(0)) AS qty_good,
    if(event_type = 'defect', toUInt32(1 + xxHash64(concat(toString(number), 'b')) % 4), toUInt32(0)) AS qty_defect,
    if(event_type = 'defect', concat('D', leftPad(toString(1 + xxHash64(concat(toString(number), 'c')) % 8), 2, '0')), '') AS defect_code,
    toUInt32(if(event_type = 'complete',
                multiIf(station = 'ST01', 28000, station = 'ST02', 35000, station = 'ST03', 40000, 30000)
                    + xxHash64(concat(toString(number), 'd')) % 9000, 0)) AS cycle_ms
FROM (SELECT number, number % 10 AS p FROM numbers(2000000, 16000))"

# 4,000 行更正事件：按 k=rand64(n+11)%1e6 复刻既有行的业务键 (job_id,event_type,station)，
# 内容为更正值（qty_good+5, cycle_ms+1000），seq=k+3e6（版本更高，ReplacingMergeTree 应以其为准）
GEN_LATE_FIX="
SELECT
    3000000 + k AS seq,
    intDiv(k, 10) AS job_id,
    concat('L', toString(1 + intDiv(k, 10) % 5)) AS line,
    multiIf(p <= 1, 'ST01', p <= 4, 'ST02', p <= 6, 'ST03', 'ST04') AS station,
    multiIf(p IN (0, 3, 5, 7), 'start', p IN (1, 4, 6, 8), 'complete', 'defect') AS event_type,
    toDateTime64('2026-09-07 06:00:00', 3, 'Asia/Shanghai') + toIntervalMillisecond(k * 1200 + xxHash64(concat(toString(k), 'j')) % 250 + 900000) AS ts,
    if(event_type = 'complete', toUInt32(38 + xxHash64(concat(toString(k), 'a')) % 23 + 5), toUInt32(0)) AS qty_good,
    if(event_type = 'defect', toUInt32(1 + xxHash64(concat(toString(k), 'b')) % 4), toUInt32(0)) AS qty_defect,
    if(event_type = 'defect', concat('D', leftPad(toString(1 + xxHash64(concat(toString(k), 'c')) % 8), 2, '0')), '') AS defect_code,
    toUInt32(if(event_type = 'complete',
                multiIf(station = 'ST01', 28000, station = 'ST02', 35000, station = 'ST03', 40000, 30000)
                    + xxHash64(concat(toString(k), 'd')) % 9000 + 1000, 0)) AS cycle_ms
FROM (SELECT number, xxHash64(concat(toString(number), 'k')) % 1000000 AS k, (xxHash64(concat(toString(number), 'k')) % 1000000) % 10 AS p FROM numbers(2100000, 4000))"

echo "== generating main 1M rows =="
$CH local --config-file /home/pan-ding/ch-mes/local_config.xml --query "$GEN_MAIN FORMAT Parquet" > "$UF/mes_events.parquet"
echo "== generating late-new 16k rows =="
$CH local --config-file /home/pan-ding/ch-mes/local_config.xml --query "$GEN_LATE_NEW FORMAT Parquet" > "$UF/mes_late_new.parquet"
echo "== generating late-fix 4k rows =="
$CH local --config-file /home/pan-ding/ch-mes/local_config.xml --query "$GEN_LATE_FIX FORMAT Parquet" > "$UF/mes_late_fix.parquet"
# 合并为单一迟到回填文件：单查询 UNION ALL 输出合法 Parquet（Parquet 不可文件级 cat 拼接，会损坏 footer）
$CH local --config-file /home/pan-ding/ch-mes/local_config.xml --query "$GEN_LATE_NEW UNION ALL $GEN_LATE_FIX FORMAT Parquet" > "$UF/mes_late.parquet"

echo "== verify =="
$CH local --config-file /home/pan-ding/ch-mes/local_config.xml --query "
SELECT 'main' src, count() c, uniqExact(seq) useq, min(ts) mn, max(ts) mx FROM file('mes_events.parquet', Parquet, '$SCHEMA')
UNION ALL SELECT 'late', count(), uniqExact(seq), min(ts), max(ts) FROM file('mes_late.parquet', Parquet, '$SCHEMA')
UNION ALL SELECT 'late-fix-keys', count(), uniqExact(job_id, event_type, station), min(ts), max(ts) FROM file('mes_late_fix.parquet', Parquet, '$SCHEMA')
ORDER BY 1 FORMAT PrettyCompactMonoBlock"
ls -la "$UF"/*.parquet
