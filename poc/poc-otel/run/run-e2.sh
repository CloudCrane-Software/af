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
LOG=$ROOT/run/e2.log
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


echo "==== E2 DONE $(date +%T) ====" >> "$LOG"
