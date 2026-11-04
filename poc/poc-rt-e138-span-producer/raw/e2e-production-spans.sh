#!/usr/bin/env bash
# RT-E138 端到端：跑一次订单链路 → 状态面真 span（订单级键行 > 0）→ 断言器。
# 用法：bash poc/poc-rt-e138-span-producer/raw/e2e-production-spans.sh
# 每步打印命令、输出、rc。不手工造状态面文件。
set -u
cd "$(dirname "$0")/../../.."
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-8}"
export LANG=C.UTF-8
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/data/bench/targets/rt-e138-span-producer}"
STATE="$PWD/状态观测/otel-production.json"
export AF_OTEL_PRODUCTION_STATE="$STATE"

run() {
  echo
  echo "\$ $*"
  "$@"
  local rc=$?
  echo "rc=$rc"
  return 0
}

echo "== RT-E138 e2e $(date -Is) =="
echo "CARGO_TARGET_DIR=$CARGO_TARGET_DIR"
echo "AF_OTEL_PRODUCTION_STATE=$AF_OTEL_PRODUCTION_STATE"

echo
echo "== 0. 状态面起始（必须缺席或由生产者写入；此处只 mkdir 目录）=="
run mkdir -p "$(dirname "$STATE")"
if [ -f "$STATE" ]; then
  echo "NOTE: 状态面已存在，将由生产者覆盖本单行（不手工改内容）"
else
  echo "状态面文件缺席（预期：由生产者创建）"
fi

echo
echo "== 1. 订单链路：stub 全生命周期 drive + 四层树夹具投影 =="
run cargo test -p af-execline --test span_produce -- --nocapture --test-threads=1

echo
echo "== 2. 状态面内容与订单级键行计数 =="
echo "\$ python3 -I -c <count>"
python3 -I - <<'PY'
import json, os, sys
path = os.environ["AF_OTEL_PRODUCTION_STATE"]
print("path=", path)
print("exists=", os.path.isfile(path))
if not os.path.isfile(path):
    print("keyed_rows=0")
    sys.exit(1)
obj = json.load(open(path, encoding="utf-8"))
spans = obj.get("spans") or []
keyed = [s for s in spans if isinstance(s, dict) and str(s.get("order_id") or "").strip()]
print("schema=", obj.get("schema"))
print("producer=", obj.get("producer"))
print("updated_at_unix=", obj.get("updated_at_unix"))
print("span_rows=", len(spans))
print("keyed_rows=", len(keyed))
print("order_ids=", sorted({s["order_id"] for s in keyed}))
print("span_names=", sorted({s.get("span_name") for s in keyed}))
if not keyed:
    sys.exit(1)
PY
echo "rc=$?"

echo
echo "== 3. 断言器 --production-check（发布面；本机若缺件则如实记红）=="
run python3 -I tools/otel/otlp-assert.py --production-check
echo "== e2e 完毕 =="
