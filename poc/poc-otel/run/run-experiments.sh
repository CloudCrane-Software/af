#!/usr/bin/env bash
# poc-otel 实验驱动：E1 协议对比 / E2 批量参数 / E3 collector 故障
set -u
ROOT=/home/pan-ding/workspace/临时工作区/temp2/af/poc/poc-otel
PROBE=$ROOT/otel-probe/target/release/otel-probe
OTELCOL=$ROOT/collector/build/otelcol
RUN=$ROOT/run
OUT=$ROOT/out
CFG=$ROOT/collector/otelcol-config.yaml
LOG=$ROOT/run/experiments.log

COLPID=""
start_collector() { # $1 run-name
  sed "s|__OUT__|$OUT/$1.json|" "$CFG" > "$RUN/$1.yaml"
  "$OTELCOL" --config "$RUN/$1.yaml" > "$RUN/$1.collector.log" 2>&1 &
  COLPID=$!
  sleep 2
}
stop_collector() {
  [ -n "$COLPID" ] && kill "$COLPID" 2>/dev/null && wait "$COLPID" 2>/dev/null
  COLPID=""
}
count_file() { # $1 file — 统计 OTLP JSON 行里的 span 数
  python3 - "$1" <<'PYEOF'
import json,sys
try:
    n=0
    for line in open(sys.argv[1],'rb').read().split(b'\n'):
        line=line.strip()
        if not line: continue
        try: d=json.loads(line)
        except Exception: continue
        for rs in d.get('resourceSpans',[]):
            for ss in rs.get('scopeSpans',[]):
                n+=len(ss.get('spans',[]))
    print(n)
except Exception as e:
    print("ERR",e)
PYEOF
}

echo "==== E1 raw: exporter 往返时延 (payload=1024B) ====" >> "$LOG"
for transport in grpc http; do
  if [ "$transport" = grpc ]; then EP=http://127.0.0.1:4317; else EP=http://127.0.0.1:4318/v1/traces; fi
  start_collector "e1raw-$transport"
  for batch in 1 64 512 2048; do
    total=40000; if [ "$batch" = 1 ]; then total=2000; fi
    echo "--- E1 raw $transport batch=$batch total=$total $(date +%T) ---" >> "$LOG"
    "$PROBE" raw --transport "$transport" --endpoint "$EP" --batch "$batch" --total "$total" \
      --payload-bytes 1024 --out "$OUT/e1-raw-$transport-b$batch.json" >> "$LOG" 2>&1
  done
  stop_collector
done

echo "==== E1 pipe: 全链路吞吐 gRPC vs HTTP (20000 spans, b512/d200ms/q65536) ====" >> "$LOG"
for transport in grpc http; do
  if [ "$transport" = grpc ]; then EP=http://127.0.0.1:4317; else EP=http://127.0.0.1:4318/v1/traces; fi
  start_collector "e1pipe-$transport"
  echo "--- E1 pipe $transport $(date +%T) ---" >> "$LOG"
  "$PROBE" pipe --transport "$transport" --endpoint "$EP" --total 20000 --rate 0 \
    --queue 65536 --export-batch 512 --delay-ms 200 --payload-bytes 1024 \
    --tail-file "$OUT/e1pipe-$transport.json" --out "$OUT/e1-pipe-$transport.json" >> "$LOG" 2>&1
  echo "collector_file_spans: $(count_file "$OUT/e1pipe-$transport.json")" >> "$LOG"
  stop_collector
done

echo "==== E2a: 批量参数-吞吐 (grpc, 20000 spans, 最快速率; 每 run 独立 collector) ====" >> "$LOG"
for combo in "100 512" "500 512" "1000 512" "5000 512" "500 64" "500 2048"; do
  set -- $combo; d=$1; b=$2
  start_collector "e2a-d$d-b$b"
  echo "--- E2a delay=${d}ms batch=$b $(date +%T) ---" >> "$LOG"
  "$PROBE" pipe --transport grpc --endpoint http://127.0.0.1:4317 --total 20000 --rate 0 \
    --queue 65536 --export-batch "$b" --delay-ms "$d" --payload-bytes 1024 \
    --tail-file "$OUT/e2a-d$d-b$b.json" --out "$OUT/e2a-d$d-b$b.json" >> "$LOG" 2>&1
  echo "collector_file_spans: $(count_file "$OUT/e2a-d$d-b$b.json")" >> "$LOG"
  stop_collector
done

echo "==== E2b: 批量参数-时延 (1000 spans @50/s, tail 时延; 每 run 独立 collector) ====" >> "$LOG"
for combo in "grpc 100 512" "grpc 500 512" "grpc 1000 512" "grpc 5000 512" "http 500 512"; do
  set -- $combo; tr=$1; d=$2; b=$3
  if [ "$tr" = grpc ]; then EP=http://127.0.0.1:4317; else EP=http://127.0.0.1:4318/v1/traces; fi
  start_collector "e2b-$tr-d$d-b$b"
  echo "--- E2b $tr delay=${d}ms batch=$b rate=50 $(date +%T) ---" >> "$LOG"
  "$PROBE" pipe --transport "$tr" --endpoint "$EP" --total 1000 --rate 50 \
    --queue 65536 --export-batch "$b" --delay-ms "$d" --payload-bytes 1024 \
    --tail-file "$OUT/e2b-$tr-d$d-b$b.json" --out "$OUT/e2b-$tr-d$d-b$b.json" >> "$LOG" 2>&1
  echo "collector_file_spans: $(count_file "$OUT/e2b-$tr-d$d-b$b.json")" >> "$LOG"
  stop_collector
done

echo "==== E3: collector 挂掉行为 ====" >> "$LOG"
for transport in grpc http; do
  for mode in recover shutdown_down; do
    echo "--- E3 $transport $mode $(date +%T) ---" >> "$LOG"
    "$PROBE" down --mode "$mode" --transport "$transport" \
      --endpoint "$( [ "$transport" = grpc ] && echo http://127.0.0.1:4317 || echo http://127.0.0.1:4318/v1/traces )" \
      --baseline 500 --burst 4000 --queue 8192 --export-batch 512 --delay-ms 200 \
      --outage-secs 10 --payload-bytes 1024 \
      --collector-bin "$OTELCOL" \
      --collector-config "$CFG" \
      --tail-file "$OUT/e3-$mode-$transport.json" \
      --out "$OUT/e3-$mode-$transport.json" >> "$LOG" 2>&1
    echo "collector_file_spans: $(count_file "$OUT/e3-$mode-$transport.json")" >> "$LOG"
  done
done

echo "==== ALL DONE $(date +%T) ====" >> "$LOG"
