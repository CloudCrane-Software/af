#!/usr/bin/env bash
# 写入路径基准驱动：crate vs http × 批大小梯度 [1k, 1w, 5w, 20w, 1e6]，每档总行数 1,000,000
# 依赖: writer 二进制已 build --release；server 已起
set -euo pipefail
TASK=/home/pan-ding/workspace/临时工作区/temp2/af/poc/poc-mes
RES=$TASK/results
BIN="$TASK/writer/target/release/writer"
CLIENT="/home/pan-ding/ch-mes/bin/clickhouse client --host 127.0.0.1 --port 5222"
mkdir -p "$RES"

: > "$RES/writer_bench.tsv"
TOTAL=1000000
for BATCH in 1000 10000 50000 200000 1000000; do
  for MODE in crate http; do
    echo "--- mode=$MODE batch=$BATCH total=$TOTAL ---"
    OUT=$("$BIN" "$MODE" "$BATCH" "$TOTAL")
    echo "$OUT" | tee -a "$RES/writer_bench_raw.log" | tee -a "$RES/writer_bench.tsv"
    CNT=$($CLIENT --query "SELECT count() FROM mes.bench_events FORMAT TSV")
    echo "verify count=$CNT (expect $TOTAL)"
    [ "$CNT" = "$TOTAL" ] || { echo "COUNT MISMATCH mode=$MODE batch=$BATCH got=$CNT"; exit 1; }
  done
done
echo "== summary =="
column -t -s $'\t' "$RES/writer_bench.tsv"
