#!/usr/bin/env bash
# 主驱动：建三模型表 → 加载 1M 行(计时) → 存储快照 → pre 基准 → 迟到回填 20k(计时)
#        → post 基准(正确性+时延) → OPTIMIZE events_repl FINAL → 存储与 B 查询复测
set -euo pipefail
TASK=/home/pan-ding/workspace/临时工作区/temp2/af/poc/poc-mes
RES=$TASK/results
UF=/home/pan-ding/ch-mes/user_files
CLIENT="/home/pan-ding/ch-mes/bin/clickhouse client --host 127.0.0.1 --port 5222"
LOCAL="/home/pan-ding/ch-mes/bin/clickhouse local --user_files_path $UF --timezone Asia/Shanghai"
SCHEMA='seq UInt64, job_id UInt64, line LowCardinality(String), station LowCardinality(String), event_type LowCardinality(String), ts DateTime64(3), qty_good UInt32, qty_defect UInt32, defect_code LowCardinality(String), cycle_ms UInt32'
mkdir -p "$RES"

timed() { # $1=标签 $2=SQL ；打印 "label<TAB>秒"
  local label=$1 sql=$2 t0 t1
  t0=$EPOCHREALTIME
  $CLIENT --query "$sql"
  t1=$EPOCHREALTIME
  awk -v l="$label" -v a="$t0" -v b="$t1" 'BEGIN{printf "LOAD\t%s\t%.3f\n", l, b-a}'
}

echo "===== phase1: schema ====="
$CLIENT --multiquery < "$TASK/sql/01_schema.sql"
echo OK

echo "===== phase2: load 1M rows into 3 models (timed) ====="
COLS='seq, job_id, line, station, event_type, ts, qty_good, qty_defect, defect_code, cycle_ms'
timed A_raw_insert "INSERT INTO mes.events_mt ($COLS) SELECT $COLS FROM file('mes_events.parquet', Parquet, '$SCHEMA')"
timed B_repl_insert "INSERT INTO mes.events_repl ($COLS) SELECT $COLS FROM file('mes_events.parquet', Parquet, '$SCHEMA')"
timed C_agg_states  "INSERT INTO mes.hourly_agg
  SELECT line, toStartOfHour(ts) AS hour,
    sumState(toUInt64(event_type = 'start'))    AS starts,
    sumState(toUInt64(event_type = 'complete')) AS completes,
    sumState(toUInt64(event_type = 'defect'))   AS defects,
    sumState(toUInt64(qty_good))   AS good_qty,
    sumState(toUInt64(qty_defect)) AS defect_qty,
    sumState(toUInt64(event_type = 'complete')) AS cycle_n,
    sumState(toUInt64(cycle_ms))   AS cycle_sum,
    quantileState(if(event_type = 'complete', toUInt32(cycle_ms), CAST(NULL, 'Nullable(UInt32)'))) AS cycle_med
  FROM file('mes_events.parquet', Parquet, '$SCHEMA') GROUP BY line, hour"

echo "----- load sanity -----"
$CLIENT --query "
SELECT 'A_events_mt'    t, count() c, countIf(event_type='start') starts, countIf(event_type='complete') completes FROM mes.events_mt
UNION ALL SELECT 'B_events_repl', count(), countIf(event_type='start'), countIf(event_type='complete') FROM mes.events_repl
UNION ALL SELECT 'C_states_rows', count(), toUInt64(0), toUInt64(0) FROM mes.hourly_agg
UNION ALL SELECT 'C_sumMerge_starts', sumMerge(starts), toUInt64(0), toUInt64(0) FROM mes.hourly_agg
UNION ALL SELECT 'A_mv_hourly_rows', count(), toUInt64(0), toUInt64(0) FROM mes.agg_hourly
FORMAT PrettyCompactMonoBlock"

echo "===== phase3: storage snapshot (1M loaded) ====="
$CLIENT --query "SELECT table, sum(rows) AS rows, count() AS parts, formatReadableSize(sum(bytes_on_disk)) AS disk
  FROM system.parts WHERE database='mes' AND active GROUP BY table ORDER BY table FORMAT PrettyCompactMonoBlock"
du -sh /home/pan-ding/ch-mes/data

echo "===== phase4: pre-backfill bench (correctness vs plain truth + latency) ====="
bash "$TASK/scripts/bench_queries.sh" pre

echo "===== phase5: late backfill 20k rows (timed) ====="
timed A_raw_insert "INSERT INTO mes.events_mt ($COLS) SELECT $COLS FROM file('mes_late.parquet', Parquet, '$SCHEMA')"
timed B_repl_insert "INSERT INTO mes.events_repl ($COLS) SELECT $COLS FROM file('mes_late.parquet', Parquet, '$SCHEMA')"
timed C_agg_states  "INSERT INTO mes.hourly_agg
  SELECT line, toStartOfHour(ts) AS hour,
    sumState(toUInt64(event_type = 'start'))    AS starts,
    sumState(toUInt64(event_type = 'complete')) AS completes,
    sumState(toUInt64(event_type = 'defect'))   AS defects,
    sumState(toUInt64(qty_good))   AS good_qty,
    sumState(toUInt64(qty_defect)) AS defect_qty,
    sumState(toUInt64(event_type = 'complete')) AS cycle_n,
    sumState(toUInt64(cycle_ms))   AS cycle_sum,
    quantileState(if(event_type = 'complete', toUInt32(cycle_ms), CAST(NULL, 'Nullable(UInt32)'))) AS cycle_med
  FROM file('mes_late.parquet', Parquet, '$SCHEMA') GROUP BY line, hour"

echo "===== phase6: post-backfill bench ====="
bash "$TASK/scripts/bench_queries.sh" post

echo "===== phase7: OPTIMIZE B FINAL (maintenance path) ====="
$CLIENT --query "OPTIMIZE TABLE mes.events_repl FINAL"
$CLIENT --query "SELECT 'after_optimize' phase, count() rows_total FROM mes.events_repl"
$CLIENT --query "SELECT table, sum(rows) AS rows, count() AS parts, formatReadableSize(sum(bytes_on_disk)) AS disk
  FROM system.parts WHERE database='mes' AND active GROUP BY table ORDER BY table FORMAT PrettyCompactMonoBlock"
du -sh /home/pan-ding/ch-mes/data
echo "--- B Q1 latency after OPTIMIZE (3 runs) ---"
for i in 1 2 3; do $CLIENT --time --query "
SELECT line, hour,
    sum(starts) OVER (PARTITION BY line ORDER BY hour)
      - sum(completes) OVER (PARTITION BY line ORDER BY hour) AS wip
  FROM (SELECT line, toStartOfHour(ts) AS hour,
          countIf(event_type = 'start') AS starts,
          countIf(event_type = 'complete') AS completes
        FROM mes.events_repl FINAL GROUP BY line, hour) ORDER BY line, hour" --format Null 2>&1 | tail -1; done

echo "===== storage summary (bytes_on_disk exact) ====="
$CLIENT --query "SELECT table, sum(rows) AS rows, sum(bytes_on_disk) AS bytes
  FROM system.parts WHERE database='mes' AND active GROUP BY table ORDER BY table FORMAT TSV"

echo "ALL_DONE"
