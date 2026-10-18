#!/usr/bin/env bash
# Exp5 round 1 (BENIGN, REAL hot change): kill in-flight wf, then physically edit
# step_a's implementation in app.py (workflow source untouched), restart.
# Expect: app_version unchanged (only workflow sources hashed) -> recovered;
# executed steps skipped (RECORDED old outputs win); tail runs NEW impl.
set -u
cd "$(dirname "$0")"
PSQL="/usr/lib/postgresql/18/bin/psql -h /tmp -p 5544 -U dbos -d dbos"
PY=.venv/bin/python

pkill -9 -f "poc-dbos/.venv/bin/python app.py" 2>/dev/null && sleep 0.5
$PSQL -c "DROP SCHEMA dbos CASCADE; CREATE SCHEMA dbos;" >/dev/null
rm -f artifacts/*.log artifacts/gates/* artifacts/hotver

echo "== phase A: OLD step_a impl, park wf_hot:hotB1 at gate, kill -9 =="
$PY app.py serve wf_hot:hotB1 --wait 280 > logs/exp5b_p1.log 2>&1 &
P1=$!
for i in $(seq 1 200); do grep -q "GATE-ENTER hotB1.hot-gate" artifacts/gates.log 2>/dev/null && break; sleep 0.1; done
sleep 0.5; kill -9 $P1; sleep 1
H1=$($PSQL -tAc "select application_version from dbos.workflow_status where workflow_uuid='wf_hot-hotB1'")
echo "app_version H1=$H1"
echo "-- fx (recorded outputs are OLD impl):"; cat artifacts/fx.log
$PSQL -c "select function_id,left(output,40) as recorded_out from dbos.operation_outputs order by function_id"

echo; echo "== HOT EDIT: sed changes step_a impl in app.py (else \"\" -> else \"-IMPL2\") =="
grep -n 'return f"{label}-ok{suffix}"' app.py | head -1
sed -i 's|else ""$|else "-IMPL2"|' app.py
grep -n 'IMPL2' app.py | head -2
H2=$($PY -c "
import inspect, app
print(app.dbos._registry.compute_app_version('poc-dbos'))
" 2>/dev/null | tail -1)
echo "app_version after edit (workflow source only) H2=$H2  -> same as H1? $([ \"$H1\" = \"$H2\" ] && echo YES || echo NO)"

echo; echo "== phase B: restart with edited step impl =="
$PY -c "from common import gate_open; gate_open('hotB1.hot-gate')"
$PY app.py wait 90 > logs/exp5b_p2.log 2>&1
grep -E "Application version|Recovering|No workflows" logs/exp5b_p2.log
echo "-- fx after recovery (s1/s2 must NOT re-EXEC):"
cat artifacts/fx.log
$PSQL -c "select workflow_uuid,status,application_version from dbos.workflow_status"
timeout 20 $PY -c "
from dbos import DBOSClient
from common import SYSTEM_DB_URL
c = DBOSClient(system_database_url=SYSTEM_DB_URL, dbos_system_schema='dbos')
h = c.retrieve_workflow('wf_hot-hotB1')
print('status =', h.get_status().status)
print('result =', h.get_result())
"
# revert the sed edit so the file stays canonical
sed -i 's|else "-IMPL2"|else ""|' app.py
grep -c 'IMPL2' app.py || echo "app.py reverted"
