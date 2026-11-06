#!/usr/bin/env bash
# Exp5 round 2 (WORKFLOW SOURCE hot change): kill in-flight wf, then edit wf_hot's
# SOURCE (insert step_new_hot call unconditionally), restart.
#   B: auto app version -> hash changes -> expect NO recovery (run stays PENDING).
#   C: pin POC_APP_VERSION=<old hash> -> forced recovery with new source ->
#      expect explicit step-replay mismatch error (DBOSUnexpectedStepError).
set -u
cd "$(dirname "$0")"
PSQL="/usr/lib/postgresql/18/bin/psql -h /tmp -p 5544 -U dbos -d dbos"
PY=.venv/bin/python
U="postgresql://dbos@/dbos?host=/tmp&port=5544"

pkill -9 -f "poc-dbos/.venv/bin/python app.py" 2>/dev/null && sleep 0.5
$PSQL -c "DROP SCHEMA dbos CASCADE; CREATE SCHEMA dbos;" >/dev/null
rm -f artifacts/*.log artifacts/gates/* artifacts/hotver

echo "== phase A: v1 workflow source, park wf_hot:hotB2, kill -9 =="
$PY app.py serve wf_hot:hotB2 --wait 280 > logs/exp5c_p1.log 2>&1 &
P1=$!
for i in $(seq 1 200); do grep -q "GATE-ENTER hotB2.hot-gate" artifacts/gates.log 2>/dev/null && break; sleep 0.1; done
sleep 0.5; kill -9 $P1; sleep 1
H1=$($PSQL -tAc "select application_version from dbos.workflow_status where workflow_uuid='wf_hot-hotB2'")
echo "app_version H1=$H1"
echo "-- fx:"; cat artifacts/fx.log

echo; echo "== HOT EDIT: wf_hot source inserts step_new_hot unconditionally (if v == \"2\" -> if v != \"2\") =="
sed -i 's|^    if v == "2":$|    if v != "2":|' app.py
grep -n 'if v != "2":' app.py | head -1
H2=$($PY -c "
import app
print(app.dbos._registry.compute_app_version('poc-dbos'))
" 2>/dev/null | tail -1)
echo "H2 (new source)=$H2  -> changed? $([ \"$H1\" != \"$H2\" ] && echo YES || echo NO)"

echo; echo "== phase B: restart with NEW source, AUTO version =="
$PY app.py wait 60 > logs/exp5c_p2.log 2>&1
grep -E "Application version|Recovering|No workflows" logs/exp5c_p2.log
echo "-- status after phase B (expect PENDING = stuck, not poisoned, not recovered):"
$PSQL -c "select workflow_uuid,status,recovery_attempts,application_version from dbos.workflow_status"
echo "-- fx (expect NO new EXEC):"; tail -3 artifacts/fx.log

echo; echo "== phase C: restart with NEW source pinned to OLD version H1 =="
POC_APP_VERSION="$H1" $PY app.py wait 60 > logs/exp5c_p3.log 2>&1
grep -E "Application version|Recovering|No workflows" logs/exp5c_p3.log
echo "-- status after phase C:"
$PSQL -c "select workflow_uuid,status,recovery_attempts from dbos.workflow_status"
echo "-- fx final:"; cat artifacts/fx.log
echo "-- CLI view:"
.venv/bin/dbos workflow get wf_hot-hotB2 -s "$U" 2>&1 | $PY -c "import json,sys; d=json.load(sys.stdin); print('status:',d['status']); print('error:',d['error']); print('output:',d['output'])"
echo "-- steps recorded:"
.venv/bin/dbos workflow steps wf_hot-hotB2 -s "$U" 2>&1 | $PY -c "import json,sys; [print(s['function_id'], s['function_name'], repr(s['output'])[:40], (s['error'] or '')[:120]) for s in json.load(sys.stdin)]"
# revert hot edit
sed -i 's|^    if v != "2":$|    if v == "2":|' app.py
grep -n 'if v == "2":' app.py | head -1
