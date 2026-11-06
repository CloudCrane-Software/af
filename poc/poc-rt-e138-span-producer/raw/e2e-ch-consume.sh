#!/usr/bin/env bash
# 端到端：真订单 → 投影器 → 现役 OTLP collector → CH → --production-check
# 在 anolis 工作树执行；工厂真仓与 collector 在 pandev-comp（<内网地址>）。
# stdin heredoc 交给 ssh 转发到远端 python（避免把 heredoc 塞进 ssh 双引号）。
set -u
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
EMIT="$ROOT/poc/poc-rt-e138-span-producer/raw/emit-semantic-otlp.py"
HOST=<内网地址>
WS=/home/pan-ding/workspace/agent黑灯工厂
ORD=$WS/af-state/orders/F1-swev-django-11099-glm-cp
PROJ=$WS/tools/otel/af-otel-projector.py

run() {
  echo "\$ $*"
  "$@"
  echo "rc=$?"
}

echo "== 0. 现役 collector / 投影器在位 =="
run ssh -o BatchMode=yes pan-ding@$HOST "ss -lntp | grep 4318; test -f $PROJ && echo projector_ok; test -d $ORD && echo order_ok"

echo "== 1. 投影 F1 经 OTLP 送入 collector（含 factory.harvest）=="
scp -o BatchMode=yes "$EMIT" pan-ding@$HOST:/tmp/emit-semantic-otlp.py
run ssh -o BatchMode=yes pan-ding@$HOST \
  python3 -I /tmp/emit-semantic-otlp.py --projector "$PROJ" --endpoint http://<内网地址>:4318 --harvest "$ORD"

echo "== 2. 等 collector batch + CH async_insert（15s）=="
sleep 15

echo "== 3. 真库订单级键 / 语义层计数 =="
ssh -o BatchMode=yes pan-ding@$HOST "cd $WS && python3 -I -" <<'PY'
import sys
sys.path.insert(0, "tools/otel")
import ch_client as ch
q = """
SELECT count() AS rows_360m,
       countIf(SpanName IN ('invoke_workflow','invoke_agent','chat','execute_tool')) AS semantic_rows,
       countIf(ResourceAttributes['af.order_id'] != '') AS order_keyed,
       countIf(ResourceAttributes['af.work_order_id'] != '') AS woid_keyed
FROM otel.otel_traces
WHERE Timestamp > now() - INTERVAL 360 MINUTE
  AND Timestamp <= now() + INTERVAL 5 MINUTE
"""
print(ch.query(q)[0])
PY
echo "ch_query_rc=$?"

echo "== 4. 断言器 --production（写状态面）+ --production-check =="
run ssh -o BatchMode=yes pan-ding@$HOST "cd $WS && python3 -I tools/otel/otlp-assert.py --production"
run ssh -o BatchMode=yes pan-ding@$HOST "cd $WS && python3 -I tools/otel/otlp-assert.py --production-check"
