#!/usr/bin/env bash
# Exp1: kill -9 BETWEEN step2 and step3, restart, verify: s1/s2 not re-run, s3 completes.
set -u
cd "$(dirname "$0")"
PSQL="/usr/lib/postgresql/18/bin/psql -h /tmp -p 5544 -U dbos -d dbos"
PY=.venv/bin/python

echo "== reset =="
pkill -9 -f "poc-dbos/.venv/bin/python app.py" 2>/dev/null && sleep 0.5
pgrep -f "poc-dbos/.venv/bin/python app.py" >/dev/null && { echo "FATAL: leftover app processes"; exit 1; } || echo "no leftover app processes"
$PSQL -c "DROP SCHEMA dbos CASCADE; CREATE SCHEMA dbos;" >/dev/null
rm -f artifacts/*.log artifacts/gates/*

echo "== phase 1: start wf_3step:crash1 with gate CLOSED =="
$PY app.py serve wf_3step:crash1 --wait 300 > logs/exp1_p1.log 2>&1 &
P1=$!
for i in $(seq 1 100); do
  grep -q "GATE-ENTER crash1.after-step2" artifacts/gates.log 2>/dev/null && break
  sleep 0.3
done
sleep 0.5
echo "-- fx.log after park (pid P1=$P1):"; cat artifacts/fx.log

echo "== kill -9 $P1 =="
kill -9 $P1
sleep 1
if ps -p $P1 >/dev/null 2>&1; then echo "STILL ALIVE"; else echo "P1 gone (killed)"; fi
pgrep -f "app.py serve wf_3step:crash1" >/dev/null && echo "leftover serve processes!" || echo "no leftover serve processes"
echo "-- DB while dead:"
$PSQL -c "select workflow_uuid,status,recovery_attempts,executor_id from dbos.workflow_status" \
        -c "select function_id,function_name,error is not null as has_error from dbos.operation_outputs order by function_id"

echo "== phase 2: open gate + restart same app (recovery at launch) =="
$PY -c "from common import gate_open; gate_open('crash1.after-step2')"
$PY app.py wait 120 > logs/exp1_p2.log 2>&1
echo "-- exp1_p2.log:"; cat logs/exp1_p2.log
echo "-- fx.log after recovery (full):"; cat artifacts/fx.log
echo "-- gates.log:"; cat artifacts/gates.log
echo "-- DB final:"
$PSQL -c "select workflow_uuid,status,recovery_attempts from dbos.workflow_status" \
        -c "select function_id,function_name from dbos.operation_outputs order by function_id"
echo "-- counts (EXEC lines per step, must be 1 each):"
grep -c "EXEC crash1 step1" artifacts/fx.log; grep -c "EXEC crash1 step2" artifacts/fx.log; grep -c "EXEC crash1 step3" artifacts/fx.log
echo "-- decoded result:"
timeout 30 $PY -c "
from dbos import DBOSClient
from common import SYSTEM_DB_URL
c = DBOSClient(system_database_url=SYSTEM_DB_URL, dbos_system_schema='dbos')
h = c.retrieve_workflow('wf_3step-crash1')
print('status =', h.get_status().status)
print('result =', h.get_result())
"
