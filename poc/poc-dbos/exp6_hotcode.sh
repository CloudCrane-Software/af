#!/usr/bin/env bash
# Exp5: hot code change while a workflow is IN FLIGHT.
#   v1 recorded: id1=step_a(hot-s1), id2=step_a(hot-s2), parked before s3.
#   v2 code: inserts step_new_hot between s1 and s2 (shifts ids), changes s1's
#   return value (-v2 suffix) and s3's label.
# Phase A: kill -9 while parked (v1).
# Phase B: restart with v2 code, AUTO app version -> expect NO recovery (version pin).
# Phase C: restart with v2 code pinned to OLD app version -> recovery proceeds,
#          observe step-replay mismatch handling.
set -u
cd "$(dirname "$0")"
PSQL="/usr/lib/postgresql/18/bin/psql -h /tmp -p 5544 -U dbos -d dbos"
PY=.venv/bin/python

echo "== reset =="
pkill -9 -f "poc-dbos/.venv/bin/python app.py" 2>/dev/null && sleep 0.5
pgrep -f "poc-dbos/.venv/bin/python app.py" >/dev/null && { echo "FATAL: leftover app processes"; exit 1; } || echo "no leftover app processes"
$PSQL -c "DROP SCHEMA dbos CASCADE; CREATE SCHEMA dbos;" >/dev/null
rm -f artifacts/*.log artifacts/gates/* artifacts/hotver

echo "== phase A: v1 run, park at hot gate, kill -9 =="
$PY app.py serve wf_hot:hot1 --wait 280 > logs/exp5_p1.log 2>&1 &
P1=$!
for i in $(seq 1 200); do grep -q "GATE-ENTER hot1.hot-gate" artifacts/gates.log 2>/dev/null && break; sleep 0.1; done
sleep 0.5
kill -9 $P1; sleep 1
echo "-- fx at kill (v1, pid P1=$P1):"; cat artifacts/fx.log
OLD_VER=$($PSQL -tAc "select application_version from dbos.workflow_status where workflow_uuid='wf_hot-hot1'")
echo "-- recorded app_version (hash of v1 workflow sources): $OLD_VER"
$PSQL -c "select workflow_uuid,status,application_version from dbos.workflow_status" \
        -c "select function_id,function_name,left(output,30) as out from dbos.operation_outputs order by function_id"

echo; echo "== phase B: v2 code (hotver=2), restart with AUTO app version =="
echo 2 > artifacts/hotver
$PY app.py wait 90 > logs/exp5_p2.log 2>&1
grep -E "Application version|Recovering|No workflows to recover" logs/exp5_p2.log
echo "-- workflow status after phase B (expect still PENDING = stuck, NOT recovered):"
$PSQL -c "select workflow_uuid,status,recovery_attempts from dbos.workflow_status"
echo "-- fx.log (expect unchanged: no new EXEC lines):"
tail -3 artifacts/fx.log

echo; echo "== phase C: v2 code pinned to OLD app version -> forced recovery =="
POC_APP_VERSION="$OLD_VER" $PY app.py wait 90 > logs/exp5_p3.log 2>&1
grep -E "Application version|Recovering|No workflows" logs/exp5_p3.log
echo "-- fx.log final (v2 replay; which steps actually executed?):"
cat artifacts/fx.log
echo "-- DB final:"
$PSQL -c "select workflow_uuid,status,recovery_attempts,left(coalesce(error,'-'),200) as error from dbos.workflow_status" \
        -c "select function_id,function_name,left(coalesce(output,'-'),40) as out from dbos.operation_outputs order by function_id"
echo "-- CLI view of the workflow:"
.venv/bin/dbos workflow get wf_hot-hot1 -s "postgresql://dbos@/dbos?host=/tmp&port=5544" 2>&1 | $PY -c "import json,sys; d=json.load(sys.stdin); print('status:',d['status']); print('error:',d['error']); print('output:',d['output'])"
rm -f artifacts/hotver
