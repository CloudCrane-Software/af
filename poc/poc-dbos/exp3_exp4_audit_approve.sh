#!/usr/bin/env bash
# Exp3+4: audit surface (system tables) with SUCCESS/ERROR/PENDING workflows,
# then human approval via DBOSClient.send from a SEPARATE process.
set -u
cd "$(dirname "$0")"
PSQL="/usr/lib/postgresql/18/bin/psql -h /tmp -p 5544 -U dbos -d dbos"
PY=.venv/bin/python

echo "== reset =="
pkill -9 -f "poc-dbos/.venv/bin/python app.py" 2>/dev/null && sleep 0.5
pgrep -f "poc-dbos/.venv/bin/python app.py" >/dev/null && { echo "FATAL: leftover app processes"; exit 1; } || echo "no leftover app processes"
$PSQL -c "DROP SCHEMA dbos CASCADE; CREATE SCHEMA dbos;" >/dev/null
rm -f artifacts/*.log artifacts/gates/*

echo "== start wf_fail:boom1 (will ERROR) + wf_approve:order7 (parks on recv) =="
$PY app.py serve wf_fail:boom1 wf_approve:order7 --wait 280 > logs/exp3_serve.log 2>&1 &
SERVE=$!
for i in $(seq 1 200); do
  grep -q "APPROVAL-WAIT order7" artifacts/fx.log 2>/dev/null && break
  sleep 0.1
done
sleep 1
echo "-- serve process alive, workflows parked; statuses now:"
$PSQL -c "select workflow_uuid,status,name from dbos.workflow_status order by created_at"

echo; echo "== EXP3: system tables audit =="
echo "-- (a) tables in dbos schema:"
./audit.sh tables
echo "-- (b) workflow_status (all states):"
$PSQL -c "select workflow_uuid, status, name, authenticated_user, assumed_role, executor_id, recovery_attempts, application_version, inputs, to_timestamp(created_at/1000) as created, to_timestamp(updated_at/1000) as updated from dbos.workflow_status order by created_at"
echo "-- (c) operation_outputs (per-step records, timestamps):"
$PSQL -c "select workflow_uuid, function_id, function_name, to_timestamp(started_at_epoch_ms/1000) as started, to_timestamp(completed_at_epoch_ms/1000) as completed, left(coalesce(error,''),80) as error from dbos.operation_outputs order by workflow_uuid, function_id"
echo "-- (d) raw step output bytes (pickle):"
$PSQL -tAc "select output from dbos.operation_outputs where workflow_uuid='wf_approve-order7' and function_id=1"
echo "-- (e) workflow_input + workflow_output tables:"
$PSQL -c "select * from dbos.workflow_input" -c "select workflow_uuid, left(coalesce(output,'-'),40) as output, left(coalesce(error,'-'),80) as error from dbos.workflow_output"
echo "-- (f) notifications table BEFORE approval (should be empty):"
$PSQL -c "select count(*) from dbos.notifications"
echo "-- (g) decoded error of the ERROR workflow:"
$PY -c "
from dbos import DBOSClient
from common import SYSTEM_DB_URL
c = DBOSClient(system_database_url=SYSTEM_DB_URL, dbos_system_schema='dbos')
st = c.get_workflow_status('wf_fail-boom1')
print('status =', st.status)
print('error  =', (st.error or '')[:200])
"
echo "-- (h) dbos CLI admin surface:"
.venv/bin/dbos --help 2>/dev/null | sed -n '/Commands:/,$p' | head -15
.venv/bin/dbos workflow list 2>/dev/null | head -5 || .venv/bin/dbos list-workflows 2>/dev/null | head -5

echo; echo "== EXP4: external approval via DBOSClient.send (separate process) =="
$PY approve.py wf_approve-order7 "approved:bob" 2>&1 | grep -v "INFO"
sleep 4
echo "-- notifications AFTER approval (consumed flag):"
$PSQL -c "select destination_uuid, topic, message, consumed, to_timestamp(created_at_epoch_ms/1000) as created from dbos.notifications"
echo "-- fx.log approval-related lines:"
grep -E "order7" artifacts/fx.log
echo "-- final status:"
$PSQL -c "select workflow_uuid,status from dbos.workflow_status order by created_at"
kill -9 $SERVE 2>/dev/null
echo "serve process ($SERVE) killed; done"
