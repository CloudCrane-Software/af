#!/usr/bin/env bash
# 用法: bench_queries.sh <pre|post>
#   pre  : 数据=主 1M 行；三模型 3 查询 × 5 次计时；与 plain 真值 md5 比对
#   post : 数据=主 1M + 迟到 20k 行；A/C 对 plain 真值，B 对去重(同键取最大 seq)真值
# 输出: results/bench_<mode>.tsv  (mode,model,query,run,latency_ms)
#       results/check_<mode>.log  (md5 校验 PASS/FAIL)
set -euo pipefail
TASK=/home/pan-ding/workspace/临时工作区/temp2/af/poc/poc-mes
RES=$TASK/results
MODE=${1:?pre|post}
UF=/home/pan-ding/ch-mes/user_files
CLIENT="/home/pan-ding/ch-mes/bin/clickhouse client --host 127.0.0.1 --port 5222"
LOCAL="/home/pan-ding/ch-mes/bin/clickhouse local --config-file /home/pan-ding/ch-mes/local_config.xml"
SCHEMA='seq UInt64, job_id UInt64, line LowCardinality(String), station LowCardinality(String), event_type LowCardinality(String), ts DateTime64(3), qty_good UInt32, qty_defect UInt32, defect_code LowCardinality(String), cycle_ms UInt32'
N_RUNS=5
mkdir -p "$RES"

# 注意：clickhouse-local 的 file() 相对名按 CWD 解析（不读 user_files_path），本地真值查询必须用绝对路径
if [ "$MODE" = pre ]; then
  SRC_ALL="file('/home/pan-ding/ch-mes/user_files/mes_events.parquet', Parquet, '$SCHEMA')"
else
  SRC_ALL="(SELECT * FROM file('/home/pan-ding/ch-mes/user_files/mes_events.parquet', Parquet, '$SCHEMA') UNION ALL SELECT * FROM file('/home/pan-ding/ch-mes/user_files/mes_late.parquet', Parquet, '$SCHEMA'))"
fi
# ReplacingMergeTree FINAL 的真值语义：同业务键 (job_id,event_type,station) 取 seq 最大行
DEDUP="(SELECT * FROM $SRC_ALL ORDER BY job_id, event_type, station, seq DESC LIMIT 1 BY job_id, event_type, station)"

BASE_AGG=$(cat <<SQL
SELECT line, toStartOfHour(ts) AS hour,
    countIf(event_type = 'start')    AS starts,
    countIf(event_type = 'complete') AS completes,
    sum(qty_good)   AS good_qty,
    sum(qty_defect) AS defect_qty,
    countIf(event_type = 'complete') AS cycle_n,
    sumIf(cycle_ms, event_type = 'complete') AS cycle_sum
  FROM $SRC_ALL GROUP BY line, hour
SQL
)
D_BASE_AGG=$(cat <<SQL
SELECT line, toStartOfHour(ts) AS hour,
    countIf(event_type = 'start')    AS starts,
    countIf(event_type = 'complete') AS completes,
    sum(qty_good)   AS good_qty,
    sum(qty_defect) AS defect_qty,
    countIf(event_type = 'complete') AS cycle_n,
    sumIf(cycle_ms, event_type = 'complete') AS cycle_sum
  FROM $DEDUP GROUP BY line, hour
SQL
)

# ---------- 真值三查询（与各模型查询端同形，只换数据源）----------
T_Q1="SELECT line, hour,
    sum(starts) OVER (PARTITION BY line ORDER BY hour)
      - sum(completes) OVER (PARTITION BY line ORDER BY hour) AS wip
  FROM ($BASE_AGG) ORDER BY line, hour"
T_Q2="SELECT line, toStartOfInterval(hour, INTERVAL 8 HOUR) AS shift,
    sum(good_qty) AS g,
    round(sum(good_qty) / (sum(good_qty) + sum(defect_qty)), 4) AS yield
  FROM ($BASE_AGG) GROUP BY line, shift ORDER BY line, shift"
T_Q3="SELECT line, hour, round(cycle_sum / cycle_n) AS takt_ms
  FROM ($BASE_AGG) WHERE cycle_n > 0 ORDER BY line, hour"

D_Q1="SELECT line, hour,
    sum(starts) OVER (PARTITION BY line ORDER BY hour)
      - sum(completes) OVER (PARTITION BY line ORDER BY hour) AS wip
  FROM ($D_BASE_AGG) ORDER BY line, hour"
D_Q2="SELECT line, toStartOfInterval(hour, INTERVAL 8 HOUR) AS shift,
    sum(good_qty) AS g,
    round(sum(good_qty) / (sum(good_qty) + sum(defect_qty)), 4) AS yield
  FROM ($D_BASE_AGG) GROUP BY line, shift ORDER BY line, shift"
D_Q3="SELECT line, hour, round(cycle_sum / cycle_n) AS takt_ms
  FROM ($D_BASE_AGG) WHERE cycle_n > 0 ORDER BY line, hour"

# ---------- 三模型的查询 ----------
Q_A1="SELECT line, hour,
    sum(starts) OVER (PARTITION BY line ORDER BY hour)
      - sum(completes) OVER (PARTITION BY line ORDER BY hour) AS wip
  FROM (SELECT line, hour, sum(starts) AS starts, sum(completes) AS completes
        FROM mes.agg_hourly GROUP BY line, hour) ORDER BY line, hour"
Q_A2="SELECT line, toStartOfInterval(hour, INTERVAL 8 HOUR) AS shift,
    sum(good_qty) AS g,
    round(sum(good_qty) / (sum(good_qty) + sum(defect_qty)), 4) AS yield
  FROM mes.agg_hourly GROUP BY line, shift ORDER BY line, shift"
Q_A3="SELECT line, hour, round(sum(cycle_sum) / sum(cycle_n)) AS takt_ms
  FROM mes.agg_hourly GROUP BY line, hour HAVING sum(cycle_n) > 0 ORDER BY line, hour"

Q_B1="SELECT line, hour,
    sum(starts) OVER (PARTITION BY line ORDER BY hour)
      - sum(completes) OVER (PARTITION BY line ORDER BY hour) AS wip
  FROM (SELECT line, toStartOfHour(ts) AS hour,
          countIf(event_type = 'start') AS starts,
          countIf(event_type = 'complete') AS completes
        FROM mes.events_repl FINAL GROUP BY line, hour) ORDER BY line, hour"
Q_B2="SELECT line, toStartOfInterval(toStartOfHour(ts), INTERVAL 8 HOUR) AS shift,
    sum(qty_good) AS g,
    round(sum(qty_good) / (sum(qty_good) + sum(qty_defect)), 4) AS yield
  FROM mes.events_repl FINAL GROUP BY line, shift ORDER BY line, shift"
Q_B3="SELECT line, toStartOfHour(ts) AS hour,
    round(avgIf(cycle_ms, event_type = 'complete')) AS takt_ms
  FROM mes.events_repl FINAL GROUP BY line, hour
  HAVING countIf(event_type = 'complete') > 0 ORDER BY line, hour"

Q_C1="SELECT line, hour,
    sum(s) OVER (PARTITION BY line ORDER BY hour)
      - sum(c) OVER (PARTITION BY line ORDER BY hour) AS wip
  FROM (SELECT line, hour, sumMerge(starts) AS s, sumMerge(completes) AS c
        FROM mes.hourly_agg GROUP BY line, hour) ORDER BY line, hour"
Q_C2="SELECT line, toStartOfInterval(hour, INTERVAL 8 HOUR) AS shift,
    sumMerge(good_qty) AS g,
    round(sumMerge(good_qty) / (sumMerge(good_qty) + sumMerge(defect_qty)), 4) AS yield
  FROM mes.hourly_agg GROUP BY line, shift ORDER BY line, shift"
Q_C3="SELECT line, hour,
    round(sumMerge(cycle_sum) / sumMerge(cycle_n)) AS takt_ms
  FROM mes.hourly_agg GROUP BY line, hour
  HAVING sumMerge(cycle_n) > 0 ORDER BY line, hour"

: > "$RES/bench_$MODE.tsv"
: > "$RES/check_$MODE.log"

run_model() { # $1=model $2=qid $3=sql $4=truth_sql
  local model=$1 qid=$2 sql=$3 truth_sql=$4
  local i out_first expected t0 t1 tval ms m1 m2 err
  for i in $(seq 1 $N_RUNS); do
    err=$(mktemp)
    t0=$EPOCHREALTIME
    if [ "$i" -eq 1 ]; then
      out_first=$($CLIENT --time --query "$sql" --format TSV 2>"$err")
    else
      $CLIENT --time --query "$sql" --format Null > /dev/null 2>"$err"
    fi
    t1=$EPOCHREALTIME
    tval=$(tr -d '[:space:]' < "$err"); rm -f "$err"
    # 优先取 client --time 的执行秒数，非数字则退回壁钟差
    ms=$(awk -v t="$tval" -v a="$t0" -v b="$t1" \
      'BEGIN{ if (t ~ /^[0-9]+(\.[0-9]+)?$/) printf "%.1f", t*1000; else printf "%.1f", (b-a)*1000 }')
    echo "$MODE,$model,$qid,$i,$ms" >> "$RES/bench_$MODE.tsv"
  done
  expected=$($LOCAL --query "$truth_sql" --format TSV)
  m1=$(printf '%s\n' "$out_first" | md5sum | awk '{print $1}')
  m2=$(printf '%s\n' "$expected" | md5sum | awk '{print $1}')
  if [ "$m1" = "$m2" ]; then
    echo "CHECK $MODE $model $qid PASS md5=$m1" >> "$RES/check_$MODE.log"
  else
    echo "CHECK $MODE $model $qid FAIL model_md5=$m1 truth_md5=$m2 rows_model=$(printf '%s\n' "$out_first" | wc -l) rows_truth=$(printf '%s\n' "$expected" | wc -l)" >> "$RES/check_$MODE.log"
    printf '%s\n' "$out_first" > "$RES/fail_${MODE}_${model}_${qid}_model.tsv"
    printf '%s\n' "$expected" > "$RES/fail_${MODE}_${model}_${qid}_truth.tsv"
  fi
}

run_model A Q1 "$Q_A1" "$T_Q1"
run_model A Q2 "$Q_A2" "$T_Q2"
run_model A Q3 "$Q_A3" "$T_Q3"
if [ "$MODE" = pre ]; then
  run_model B Q1 "$Q_B1" "$T_Q1"
  run_model B Q2 "$Q_B2" "$T_Q2"
  run_model B Q3 "$Q_B3" "$T_Q3"
else
  run_model B Q1 "$Q_B1" "$D_Q1"
  run_model B Q2 "$Q_B2" "$D_Q2"
  run_model B Q3 "$Q_B3" "$D_Q3"
fi
run_model C Q1 "$Q_C1" "$T_Q1"
run_model C Q2 "$Q_C2" "$T_Q2"
run_model C Q3 "$Q_C3" "$T_Q3"

echo "== latency (ms, $N_RUNS runs, client --time) =="
awk -F, '{k=$1"/"$2"/"$3; s[k]+=$5; c[k]++; if(!(k in mn)||$5<mn[k])mn[k]=$5}
  END{for(k in s) printf "%s min=%.1f avg=%.1f\n", k, mn[k], s[k]/c[k]}' "$RES/bench_$MODE.tsv" | sort
echo "== checks =="
cat "$RES/check_$MODE.log"
