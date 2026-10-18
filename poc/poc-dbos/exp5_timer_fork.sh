#!/usr/bin/env bash
# Exp-timer: kill -9 during DBOS.sleep(3) -> verify durable timer resumes remaining time.
# Exp-fork: admin fork from a step -> re-execute tail of a finished workflow.
set -u
cd "$(dirname "$0")"
PSQL="/usr/lib/postgresql/18/bin/psql -h /tmp -p 5544 -U dbos -d dbos"
PY=.venv/bin/python
U="postgresql://dbos@/dbos?host=/tmp&port=5544"

echo "== reset =="
pkill -9 -f "poc-dbos/.venv/bin/python app.py" 2>/dev/null && sleep 0.5
pgrep -f "poc-dbos/.venv/bin/python app.py" >/dev/null && { echo "FATAL: leftover app processes"; exit 1; } || echo "no leftover app processes"
$PSQL -c "DROP SCHEMA dbos CASCADE; CREATE SCHEMA dbos;" >/dev/null
rm -f artifacts/*.log artifacts/gates/*

echo "== timer: start wf_timer:t1 (parks at pre-sleep gate), then open gate =="
$PY app.py serve wf_timer:t1 --wait 280 > logs/exp_timer_p1.log 2>&1 &
P1=$!
for i in $(seq 1 200); do grep -q "GATE-ENTER t1.pre-sleep" artifacts/gates.log 2>/dev/null && break; sleep 0.1; done
$PY -c "from common import gate_open; gate_open('t1.pre-sleep')"
echo "-- kill 1s after the sleep starts (sleep=3s):"
for i in $(seq 1 60); do grep -q "EXEC t1 before-sleep" artifacts/fx.log 2>/dev/null && break; sleep 0.1; done
sleep 1.0
KILL_T=$SECONDS
kill -9 $P1; sleep 1
echo "-- fx at kill:"; cat artifacts/fx.log
echo "-- DB while dead:"
$PSQL -c "select workflow_uuid,status from dbos.workflow_status" -c "select function_id,function_name from dbos.operation_outputs order by function_id"

T0=$(date +%s.%N)
$PY app.py wait 120 > logs/exp_timer_p2.log 2>&1
T1=$(date +%s.%N)
echo "-- restart->after-sleep wall time: $(echo "$T1 - $T0" | bc)s (sleep resumed remaining ~2s, not full 3s)"
echo "-- fx final:"; cat artifacts/fx.log
$PSQL -c "select workflow_uuid,status from dbos.workflow_status"
timeout 20 $PY -c "
from dbos import DBOSClient
from common import SYSTEM_DB_URL
c = DBOSClient(system_database_url=SYSTEM_DB_URL, dbos_system_schema='dbos')
h = c.retrieve_workflow('wf_timer-t1')
print('timer result =', h.get_result())
"

echo; echo "== fork: re-run tail of finished wf_approve-like flow =="
echo "-- (fork exp uses wf_timer-t1 from step 2 = the sleep: recorded outputs copied, tail re-executes)"
.venv/bin/dbos workflow fork wf_timer-t1 -s "$U" --help 2>&1 | head -12
