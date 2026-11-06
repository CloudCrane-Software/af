#!/usr/bin/env bash
# s17-trace-attribution-batch（D-55）证据复跑脚本——确定性、零网络、只读轨迹集合。
# 产出 readings/（acceptance ① 真实跑批读数 + ② 对齐率）+ raw/（selftest + 负例）。
# 纪律：零改判据件/门；只读 poc-p3-a1 既有读数与自建 corpus。
# 全程 cd 到 af 仓根、用**相对路径**——保证落盘读数跨机可复现（无绝对路径残留）。
# 用法：在 af 仓根 `bash poc/poc-s17-trace-attribution/run-readings.sh`。
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
cd "$ROOT"   # 归一到仓根，后续全用相对路径
BATCH="tools/trace_attribution_batch.py"
PY="python3 -B"
P3A1="poc/poc-p3-a1-strategy-dsl/raw"
CORPUS="poc/poc-s17-trace-attribution/corpus"
ANN="poc/poc-s17-trace-attribution/annotations/alignment-fixture.jsonl"
READ="poc/poc-s17-trace-attribution/readings"
RAW="poc/poc-s17-trace-attribution/raw"
mkdir -p "$READ" "$RAW"

echo "### acceptance ①：批脚本对既有跑批读数（poc-p3-a1 records+results）实跑落分布"
$PY "$BATCH" --report \
  --sources "$P3A1/records.jsonl" \
  --sources "$P3A1/results.jsonl" \
  --json-out "$READ/report-real-p3a1.json" | tee "$RAW/report-real-p3a1.txt"
echo "report-real rc=${PIPESTATUS[0]}"
echo ""

echo "### acceptance ②：对齐率标注夹具消费（corpus + alignment-fixture）"
$PY "$BATCH" --align \
  --sources "$CORPUS/results.jsonl" \
  --sources "$CORPUS/records.jsonl" \
  --annotations "$ANN" \
  --json-out "$READ/align-fixture.json" | tee "$RAW/align-fixture.txt"
echo "align rc=${PIPESTATUS[0]}"
echo ""

echo "### 附：corpus 全量分布（含 resolved 人口）"
$PY "$BATCH" --report \
  --sources "$CORPUS/results.jsonl" \
  --sources "$CORPUS/records.jsonl" \
  --json-out "$READ/report-corpus.json" | tee "$RAW/report-corpus.txt"
echo "report-corpus rc=${PIPESTATUS[0]}"
echo ""

echo "### selftest（判别力自证）"
$PY "$BATCH" --selftest | tee "$RAW/selftest.txt"
echo "selftest rc=${PIPESTATUS[0]}"
echo ""

echo "### 负例 ①：零轨迹（空文件在场）⇒ fail-closed 红 rc=1"
: > "$RAW/_zero.jsonl"
$PY "$BATCH" --report --sources "$RAW/_zero.jsonl" 2>&1 | tee "$RAW/negative-zero-trace.txt"
echo "zero-trace rc=${PIPESTATUS[0]}（期望 1）"
rm -f "$RAW/_zero.jsonl"
echo ""

echo "### 负例 ②：无标注（夹具缺件）⇒ fail-closed 红 rc=1"
$PY "$BATCH" --align --sources "$CORPUS/results.jsonl" --annotations "$RAW/_absent.jsonl" 2>&1 | tee "$RAW/negative-no-annotation.txt"
echo "no-annotation rc=${PIPESTATUS[0]}（期望 1）"
rm -f "$RAW/_absent.jsonl"
echo ""

echo "### 负例 ③：源缺件 ⇒ 不可证 rc=2（区别于零轨迹红）"
$PY "$BATCH" --report --sources "$RAW/_nope.jsonl" 2>&1 | tee "$RAW/negative-source-missing.txt"
echo "source-missing rc=${PIPESTATUS[0]}（期望 2）"
rm -f "$RAW/_nope.jsonl"
