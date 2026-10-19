#!/usr/bin/env bash
set -u
cd "$(dirname "$0")/.."
rm -rf cold/runs && mkdir -p cold/runs
for i in $(seq 1 10); do
  d=cold/runs/run-$i
  rm -rf "$d"
  S=$(date +%s%N)
  OUT=$(/usr/bin/time -f "RSS_KB=%M" ./target/debug/poc-kernelrun run "$d" 2>&1)
  E=$(date +%s%N)
  WALL_US=$(( (E - S) / 1000 ))
  COLD=$(echo "$OUT" | grep -o 'COLD_START_MS=[0-9.]*' | cut -d= -f2)
  RSS=$(echo "$OUT" | grep -o 'RSS_KB=[0-9]*' | cut -d= -f2)
  OK=$(echo "$OUT" | grep -c ' OK ')
  EXITLINE=$(echo "$OUT" | grep -o 'STEPS_OK=[0-9]*')
  echo "run=$i steps_ok=$OK $EXITLINE internal_ms=$COLD wall_us=$WALL_US rss_kb=$RSS"
done
