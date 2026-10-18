#!/usr/bin/env bash
# Exp2: kill -9 INSIDE step_charge (after its 1st side-effect append).
# Verify: step re-executes on recovery -> side-effect appends counted (at-least-once).
set -u
cd "$(dirname "$0")"
PSQL="/usr/lib/postgresql/18/bin/psql -h /tmp -p 5544 -U dbos -d dbos"
PY=.venv/bin/python

echo "== reset =="
pkill -9 -f "poc-dbos/.venv/bin/python app.py" 2>/dev/null && sleep 0.5
pgrep -f "poc-dbos/.venv/bin/python app.py" >/dev/null && { echo "FATAL: leftover app processes"; exit 1; } || echo "no leftover app processes"
$PSQL -c "DROP SCHEMA dbos CASCADE; CREATE SCHEMA dbos;" >/dev/null
rm -f artifacts/*.log artifacts/gates/*

echo "== phase 1: start wf_charge:midkill, kill after 1st CHARGE append =="
$PY app.py serve wf_charge:midkill --wait 300 > logs/exp2_p1.log 2>&1 &
P1=$!
for i in $(seq 1 200); do
  [ "$(grep -c 'CHARGE midkill' artifacts/fx.log 2>/dev/null)" = "1" ] && break
  sleep 0.1
done
echo "-- fx.log at kill moment:"; cat artifacts/fx.log
kill -9 $P1
sleep 1
ps -p $P1 >/dev/null 2>&1 && echo "STILL ALIVE" || echo "P1 gone (killed mid-step_charge)"
echo "-- DB while dead:"
$PSQL -c "select workflow_uuid,status,recovery_attempts from dbos.workflow_status" \
        -c "select function_id,function_name from dbos.operation_outputs order by function_id"

echo "== phase 2: restart same app (no gate involved) =="
$PY app.py wait 120 > logs/exp2_p2.log 2>&1
grep -E "Recovering|wait done" logs/exp2_p2.log
echo "-- fx.log final (full):"; cat artifacts/fx.log
echo "-- CHARGE append total count (at-least-once evidence):"
grep -c "CHARGE midkill" artifacts/fx.log
echo "-- step-level EXEC counts:"
echo "step1: $(grep -c 'EXEC midkill step1' artifacts/fx.log)  step3: $(grep -c 'EXEC midkill step3' artifacts/fx.log)"
echo "-- DB final:"
$PSQL -c "select workflow_uuid,status,recovery_attempts from dbos.workflow_status" \
        -c "select function_id,function_name from dbos.operation_outputs order by function_id"
echo "-- decoded result:"
timeout 30 $PY -c "
from dbos import DBOSClient
from common import SYSTEM_DB_URL
c = DBOSClient(system_database_url=SYSTEM_DB_URL, dbos_system_schema='dbos')
h = c.retrieve_workflow('wf_charge-midkill')
print('status =', h.get_status().status)
print('result =', h.get_result())
"
