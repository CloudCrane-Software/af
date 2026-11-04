#!/usr/bin/env bash
# =============================================================================
# run_drill.sh — F2 执行面**端到端演练**（隔离目录；一次性；无常驻）
#
# 演练链路：派单(ingest) → 认领(claim+lease) → 跑门(桩门) → 落 SUMMARY
#           → 回报(result+reported.jsonl) → integrator 复核面读这份 SUMMARY
# 附负向臂：幂等重投 / 门红 / SUMMARY 含 SKIP / 宿主缺件 / 租约未过期 / 租约过期
#           / 门摘要被篡改（复核面回读）
#
# ★ 诚实边界：
#   · 门是**桩门**（drill/stub_gate.sh）——只验管道，不构成任何真实 F2 门结论；
#     真实 F2 全量门（ci/run.sh 十四步）在供给齐备的独立 runner 宿主上跑，
#     本机**未跑**（理由见 README §五）。
#   · 不碰在役收件箱/门户/观测面；夹具只落隔离目录。
#   · 候选树 = 共享树的 **sparse shared clone**（对象共享、不复制 1.5G、
#     不写 af 仓的 worktree 登记）；共享树本身只读。
# =============================================================================
set -u

DRILL_ROOT="${1:-/home/pan-ding/workspace/f2-drill-scratch/run}"
AF_WT="/home/pan-ding/workspace/af-wt-t7-1-f2"          # 本批工作树（门码面来源）
SHARED="/home/pan-ding/workspace/agent黑灯工厂/af"     # 共享树（只读：克隆源）
DRILL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RAW="$DRILL_DIR/../raw"
mkdir -p "$RAW" "$DRILL_ROOT"
LOG="$RAW/drill-e2e.txt"
: > "$LOG"

CAND="$DRILL_ROOT/candidate"
QUEUE="$DRILL_ROOT/queue"
TRIGGERS="$DRILL_ROOT/triggers"
OUT="$DRILL_ROOT/out"

say() { printf '%s\n' "$*" | tee -a "$LOG"; }
step() { # step <编号> <标题> ; 之后调用 run_cmd
  say ""
  say "===== [$1] $2 ====="
}
run_cmd() { # run_cmd <说明> <命令...>
  local title="$1"; shift
  say "--- $title"
  say "\$ $*"
  "$@" >> "$LOG" 2>&1
  local rc=$?
  say "--- exit=$rc"
  return $rc
}
jq_get() { python3 -c "import json,sys;d=json.load(open(sys.argv[1]));print(eval(sys.argv[2],{'d':d}))" "$1" "$2" 2>/dev/null; }

say "F2 执行面端到端演练（隔离目录）"
say "时间=$(date '+%F %T')  DRILL_ROOT=$DRILL_ROOT"
say "本机工作树=$AF_WT  共享树(只读)=$SHARED"
say "★ 门是桩门（stub_gate.sh）；真实 ci/run.sh 全量门本机未跑（README §五 已登记理由）"

# ── S01 候选树（sparse shared clone）──────────────────────────────────────────
step S01 "候选树：共享树的 sparse shared clone（对象共享，不写 af 仓 worktree 登记）"
rm -rf "$CAND" "$QUEUE" "$TRIGGERS" "$OUT"
mkdir -p "$OUT"
git clone --shared --no-checkout "$SHARED" "$CAND" >> "$LOG" 2>&1
git -C "$CAND" sparse-checkout init --cone >> "$LOG" 2>&1
git -C "$CAND" sparse-checkout set ci .gitea >> "$LOG" 2>&1
git -C "$CAND" checkout -q f489601 >> "$LOG" 2>&1
CAND_SHA="$(git -C "$CAND" rev-parse HEAD)"
say "候选树 HEAD=$CAND_SHA  干净性=$(git -C "$CAND" status --porcelain | wc -l) 行变更"
say "集成器复核器在位=$( [ -f "$CAND/ci/integrator_bind_check.py" ] && echo yes || echo no )"
SHA_F489="$(git -C "$CAND" rev-parse f489601)"
SHA_56FF="$(git -C "$CAND" rev-parse 56ff52a)"
SHA_04C7="$(git -C "$CAND" rev-parse 04c70fa)"
SHA_335B="$(git -C "$CAND" rev-parse 335b6d6)"
SHA_E448="$(git -C "$CAND" rev-parse e448453)"

# ── S02 夹具 ─────────────────────────────────────────────────────────────────
step S02 "造演练夹具（本地文件面；非合成 gitea 投递）"
run_cmd "make_fixture（正向臂：source=base=$CAND_SHA）" \
  python3 -I "$DRILL_DIR/make_fixture.py" --out "$DRILL_ROOT" \
  --source-sha "$CAND_SHA" --base-sha "$CAND_SHA" --delivery-id drill-f2-delivery-0001

# ── S03 宿主供给核（对**真实** af 工作树 ⇒ 真实缺件面）───────────────────────
step S03 "负向臂 N-A：宿主供给核（对真实 af 工作树；cosign 不入库 ⇒ 真实缺件）"
run_cmd "f2_runner supply --repo $AF_WT" python3 -I "$AF_WT/ci/f2_runner.py" supply --repo "$AF_WT"

# ── S04 装演练用 cosign 替身（只让供给核过；不参与任何真实验签）─────────────
step S04 "正向臂准备：装**演练用** cosign 替身到候选树 bin/（bin/ 在 .gitignore 行 2 ⇒ 不入版本面）"
mkdir -p "$CAND/bin"
cat > "$CAND/bin/cosign" <<'EOS'
#!/usr/bin/env bash
# DRILL STUB cosign —— 演练替身；不做任何验签（真实 cosign 由宿主供给，见
# docs/CI-BOUNDARY.md §一）。
echo "cosign (DRILL STUB) 2.4.0-dry-run"
EOS
chmod +x "$CAND/bin/cosign"
run_cmd "f2_runner supply --repo $CAND（替身装好后）" python3 -I "$AF_WT/ci/f2_runner.py" supply --repo "$CAND"
say "候选树 git status（替身不应污染）：$(git -C "$CAND" status --porcelain | wc -l) 行"

# ── S05 ingest（建队列）─────────────────────────────────────────────────────
step S05 "派单侧：ingest（在役面只读 → runner 自有队列）"
run_cmd "f2_dispatch ingest" python3 -I "$AF_WT/ci/f2_dispatch.py" ingest \
  --triggers-home "$TRIGGERS" --queue "$QUEUE" --json
TASK_ID="$(python3 - "$QUEUE" <<'PY'
import json,sys,pathlib
tasks=sorted(pathlib.Path(sys.argv[1], "tasks").glob("*.json"))
print(json.load(open(tasks[0]))["task_id"] if tasks else "")
PY
)"
say "TASK_ID=$TASK_ID"

# ── S06 幂等重投 ────────────────────────────────────────────────────────────
step S06 "负向臂 N-B：同一投递再 ingest 一次 ⇒ 幂等（不得产生第二个任务）"
run_cmd "f2_dispatch ingest（第二次）" python3 -I "$AF_WT/ci/f2_dispatch.py" ingest \
  --triggers-home "$TRIGGERS" --queue "$QUEUE" --json
say "队列任务件数=$(ls "$QUEUE/tasks" | wc -l)（应为 1）"

# ── S07 领单 → 跑门 → SUMMARY → 回报 ───────────────────────────────────────
step S07 "runner：领单 → 跑门（桩门）→ 落 SUMMARY → 结论入账 → 回报"
AF_DRILL_MODE=green AF_DRILL_SUMMARY="$OUT/SUMMARY.txt" \
run_cmd "f2_runner run（正向臂）" python3 -I "$AF_WT/ci/f2_runner.py" run \
  --queue "$QUEUE" --task-id "$TASK_ID" --repo "$CAND" --worktree "$CAND" \
  --gate-command "bash $DRILL_DIR/stub_gate.sh" \
  --summary-path "$OUT/SUMMARY.txt" --record-dir "$OUT" --runner-id drill-runner-1
RESULT="$QUEUE/results/$TASK_ID.json"
say "结果件 verdict=$(jq_get "$RESULT" "d['verdict']")  reason=$(jq_get "$RESULT" "d['reason_codes']")"
say "四元组（自记录件读出，非手抄）=$(jq_get "$RESULT" "d['integrator'].get('four_tuple')")"
RECORD="$OUT/integrator-record-f2-$TASK_ID.json"
say "记录件=$RECORD  在位=$( [ -f "$RECORD" ] && echo yes || echo no )"

# ── S08 integrator 复核面读这份 SUMMARY ─────────────────────────────────────
step S08 "T7② integrator 复核面：verify（机器侧核对四项绑定 + 门摘要回读）"
run_cmd "integrator_bind_check verify" python3 -I "$CAND/ci/integrator_bind_check.py" verify \
  --record "$RECORD" --repo "$CAND" --branch feature/f2-drill --remote origin \
  --base-ref origin/product/main --source "$CAND_SHA" --allow-local-source

# ── S09 摘要被篡改 ⇒ 复核面回读拒绝 ─────────────────────────────────────────
step S09 "负向臂 N-C：门摘要文件被追加一行 ⇒ 复核面三方比对（磁盘↔内嵌 sha↔内嵌原文）拒绝"
cp "$OUT/SUMMARY.txt" "$OUT/SUMMARY.txt.bak"
printf 'step99  PASS  1ms  注入行（篡改演练）\n' >> "$OUT/SUMMARY.txt"
run_cmd "integrator_bind_check verify（篡改后）" python3 -I "$CAND/ci/integrator_bind_check.py" verify \
  --record "$RECORD" --repo "$CAND" --branch feature/f2-drill --remote origin \
  --base-ref origin/product/main --source "$CAND_SHA" --allow-local-source
mv "$OUT/SUMMARY.txt.bak" "$OUT/SUMMARY.txt"

# ── S10 门红 ────────────────────────────────────────────────────────────────
step S10 "负向臂 N-D：门红（桩门 mode=red）⇒ 不写记录件"
git -C "$CAND" checkout -q "$SHA_56FF"
python3 -I "$DRILL_DIR/make_fixture.py" --out "$DRILL_ROOT" --source-sha "$SHA_56FF" \
  --base-sha "$SHA_56FF" --delivery-id drill-f2-delivery-0002 >> "$LOG" 2>&1
run_cmd "f2_dispatch ingest（红门臂）" python3 -I "$AF_WT/ci/f2_dispatch.py" ingest \
  --triggers-home "$TRIGGERS" --queue "$QUEUE" --json
RED_TASK="$(python3 - "$QUEUE" <<'PY'
import json,sys,pathlib
ids=[]
for p in sorted(pathlib.Path(sys.argv[1], "tasks").glob("*.json")):
    d=json.load(open(p))
    if d.get("delivery_id")=="drill-f2-delivery-0002": ids.append(d["task_id"])
print(ids[0] if ids else "")
PY
)"
AF_DRILL_MODE=red AF_DRILL_SUMMARY="$OUT/SUMMARY-red.txt" \
run_cmd "f2_runner run（红门臂）" python3 -I "$AF_WT/ci/f2_runner.py" run \
  --queue "$QUEUE" --task-id "$RED_TASK" --repo "$CAND" --worktree "$CAND" \
  --gate-command "bash $DRILL_DIR/stub_gate.sh" \
  --summary-path "$OUT/SUMMARY-red.txt" --record-dir "$OUT" --runner-id drill-runner-1
RED_RESULT="$QUEUE/results/$RED_TASK.json"
say "红门臂 verdict=$(jq_get "$RED_RESULT" "d['verdict']")  reason=$(jq_get "$RED_RESULT" "d['reason_codes']")"
say "红门臂记录件在位=$( [ -f "$OUT/integrator-record-f2-$RED_TASK.json" ] && echo yes || echo no )（应为 no）"

# ── S11 SUMMARY 含 SKIP ─────────────────────────────────────────────────────
step S11 "负向臂 N-E：SUMMARY 里出现 SKIP 状态（不在白名单）⇒ 不得计绿 ⇒ 不写记录件"
git -C "$CAND" checkout -q "$SHA_04C7"
python3 -I "$DRILL_DIR/make_fixture.py" --out "$DRILL_ROOT" --source-sha "$SHA_04C7" \
  --base-sha "$SHA_04C7" --delivery-id drill-f2-delivery-0003 >> "$LOG" 2>&1
run_cmd "f2_dispatch ingest（SKIP 臂）" python3 -I "$AF_WT/ci/f2_dispatch.py" ingest \
  --triggers-home "$TRIGGERS" --queue "$QUEUE" --json
SKIP_TASK="$(python3 - "$QUEUE" <<'PY'
import json,sys,pathlib
ids=[]
for p in sorted(pathlib.Path(sys.argv[1], "tasks").glob("*.json")):
    d=json.load(open(p))
    if d.get("delivery_id")=="drill-f2-delivery-0003": ids.append(d["task_id"])
print(ids[0] if ids else "")
PY
)"
AF_DRILL_MODE=skip AF_DRILL_SUMMARY="$OUT/SUMMARY-skip.txt" \
run_cmd "f2_runner run（SKIP 臂）" python3 -I "$AF_WT/ci/f2_runner.py" run \
  --queue "$QUEUE" --task-id "$SKIP_TASK" --repo "$CAND" --worktree "$CAND" \
  --gate-command "bash $DRILL_DIR/stub_gate.sh" \
  --summary-path "$OUT/SUMMARY-skip.txt" --record-dir "$OUT" --runner-id drill-runner-1
SKIP_RESULT="$QUEUE/results/$SKIP_TASK.json"
say "SKIP 臂 verdict=$(jq_get "$SKIP_RESULT" "d['verdict']")  reason=$(jq_get "$SKIP_RESULT" "d['reason_codes']")"
say "SKIP 臂记录件在位=$( [ -f "$OUT/integrator-record-f2-$SKIP_TASK.json" ] && echo yes || echo no )（应为 no）"

# ── S12 宿主缺件 ⇒ 不跑门 ───────────────────────────────────────────────────
step S12 "负向臂 N-F：宿主供给缺件（撤走 cosign 替身）⇒ 不跑门、SKIP 不得计绿、不写记录件"
rm -f "$CAND/bin/cosign"
git -C "$CAND" checkout -q "$SHA_335B"
python3 -I "$DRILL_DIR/make_fixture.py" --out "$DRILL_ROOT" --source-sha "$SHA_335B" \
  --base-sha "$SHA_335B" --delivery-id drill-f2-delivery-0004 >> "$LOG" 2>&1
run_cmd "f2_dispatch ingest（缺件臂）" python3 -I "$AF_WT/ci/f2_dispatch.py" ingest \
  --triggers-home "$TRIGGERS" --queue "$QUEUE" --json
SUP_TASK="$(python3 - "$QUEUE" <<'PY'
import json,sys,pathlib
ids=[]
for p in sorted(pathlib.Path(sys.argv[1], "tasks").glob("*.json")):
    d=json.load(open(p))
    if d.get("delivery_id")=="drill-f2-delivery-0004": ids.append(d["task_id"])
print(ids[0] if ids else "")
PY
)"
AF_DRILL_MODE=green AF_DRILL_SUMMARY="$OUT/SUMMARY-nosupply.txt" \
run_cmd "f2_runner run（缺件臂）" python3 -I "$AF_WT/ci/f2_runner.py" run \
  --queue "$QUEUE" --task-id "$SUP_TASK" --repo "$CAND" --worktree "$CAND" \
  --gate-command "bash $DRILL_DIR/stub_gate.sh" \
  --summary-path "$OUT/SUMMARY-nosupply.txt" --record-dir "$OUT" --runner-id drill-runner-1
SUP_RESULT="$QUEUE/results/$SUP_TASK.json"
say "缺件臂 verdict=$(jq_get "$SUP_RESULT" "d['verdict']")  reason=$(jq_get "$SUP_RESULT" "d['reason_codes']")"
say "缺件臂门日志在位=$(ls "$QUEUE/logs" 2>/dev/null | grep -c "$SUP_TASK" || true)（应为 0：门未执行）"
say "缺件臂记录件在位=$( [ -f "$OUT/integrator-record-f2-$SUP_TASK.json" ] && echo yes || echo no )（应为 no）"

# ── S13 租约未过期 ⇒ 拒绝重复领单 ───────────────────────────────────────────
step S13 "负向臂 N-G：任务上存在**未过期**租约 ⇒ 第二个 runner 领不走（认领唯一写者）"
git -C "$CAND" checkout -q "$CAND_SHA"
python3 -I "$DRILL_DIR/make_fixture.py" --out "$DRILL_ROOT/lease-arm" --source-sha "$CAND_SHA" \
  --base-sha "$CAND_SHA" --delivery-id drill-f2-delivery-0005 --salt lease-arm >> "$LOG" 2>&1
run_cmd "f2_dispatch ingest（租约臂；独立夹具目录 ⇒ 独立 order_id）" python3 -I "$AF_WT/ci/f2_dispatch.py" ingest \
  --triggers-home "$DRILL_ROOT/lease-arm/triggers" --queue "$QUEUE" --json
LEASE_TASK="$(python3 - "$QUEUE" <<'PY'
import json,sys,pathlib
ids=[]
for p in sorted(pathlib.Path(sys.argv[1], "tasks").glob("*.json")):
    d=json.load(open(p))
    if d.get("delivery_id")=="drill-f2-delivery-0005": ids.append(d["task_id"])
print(ids[0] if ids else "")
PY
)"
python3 - "$QUEUE" "$LEASE_TASK" <<'PY'
import json,os,sys,pathlib
from datetime import datetime, timedelta, timezone
q=pathlib.Path(sys.argv[1]); tid=sys.argv[2]
p=q/"tasks"/f"{tid}.json"
d=json.load(open(p))
d["lease"]={"runner_id":"other-runner","claimed_at":datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds"),
            "expires_at":(datetime.now(timezone.utc)+timedelta(seconds=600)).astimezone().isoformat(timespec="seconds"),
            "ttl_sec":600}
p.write_text(json.dumps(d,ensure_ascii=False,indent=1,sort_keys=True)+"\n")
print("已写入他人租约 expires_at=",d["lease"]["expires_at"])
PY
run_cmd "f2_runner run（他人租约未过期 ⇒ 应领单失败）" python3 -I "$AF_WT/ci/f2_runner.py" run \
  --queue "$QUEUE" --task-id "$LEASE_TASK" --repo "$CAND" --worktree "$CAND" \
  --gate-command "bash $DRILL_DIR/stub_gate.sh" \
  --summary-path "$OUT/SUMMARY-lease.txt" --record-dir "$OUT" --runner-id drill-runner-2

# ── S14 租约过期 ⇒ 可重领 ───────────────────────────────────────────────────
step S14 "负向臂 N-H：租约**已过期** ⇒ 允许重领（租约超时语义）"
python3 - "$QUEUE" "$LEASE_TASK" <<'PY'
import json,sys,pathlib
from datetime import datetime, timedelta, timezone
q=pathlib.Path(sys.argv[1]); tid=sys.argv[2]
p=q/"tasks"/f"{tid}.json"
d=json.load(open(p))
d["lease"]["expires_at"]=(datetime.now(timezone.utc)-timedelta(seconds=60)).astimezone().isoformat(timespec="seconds")
p.write_text(json.dumps(d,ensure_ascii=False,indent=1,sort_keys=True)+"\n")
print("已把租约置为过期 expires_at=",d["lease"]["expires_at"])
PY
mkdir -p "$CAND/bin"
cat > "$CAND/bin/cosign" <<'EOS'
#!/usr/bin/env bash
echo "cosign (DRILL STUB) 2.4.0-dry-run"
EOS
chmod +x "$CAND/bin/cosign"
AF_DRILL_MODE=green AF_DRILL_SUMMARY="$OUT/SUMMARY-lease.txt" \
run_cmd "f2_runner run（过期租约 ⇒ 应领单成功并跑门）" python3 -I "$AF_WT/ci/f2_runner.py" run \
  --queue "$QUEUE" --task-id "$LEASE_TASK" --repo "$CAND" --worktree "$CAND" \
  --gate-command "bash $DRILL_DIR/stub_gate.sh" \
  --summary-path "$OUT/SUMMARY-lease.txt" --record-dir "$OUT" --runner-id drill-runner-2

# ── S15 handover（结论入账交接：实测侧复算 + 动作清单）───────────────────────
step S15 "结论入账交接：handover（只读实测侧复算 + rebase 漂移动作清单）"
run_cmd "f2_runner handover" python3 -I "$AF_WT/ci/f2_runner.py" handover \
  --record "$RECORD" --repo "$CAND" --base-ref origin/product/main --branch feature/f2-drill

# ── S15b 交接漂移臂①：base 前移但**内容等价** ⇒ 门结论仍描述这棵树（SOP §5.2 步 6）
step S15b "交接漂移臂①：handover（base 取 c962cec：非门跑时基点，但落地树仍==门跑过的树）"
SHA_DRIFT_EQUIV="$(git -C "$CAND" rev-parse c962cec)"
run_cmd "f2_runner handover（漂移臂①）" python3 -I "$AF_WT/ci/f2_runner.py" handover \
  --record "$RECORD" --repo "$CAND" --base-ref "$SHA_DRIFT_EQUIV" --branch feature/f2-drill

# ── S15c 交接漂移臂②：落地树≠门跑过的树 ⇒ 出 rebase 动作清单 ──────────────────
step S15c "交接漂移臂②：handover（base 取 c1f45f9：落地树≠门跑过的树 ⇒ 出 rebase/重跑门 动作清单）"
SHA_DRIFT_DIFF="$(git -C "$CAND" rev-parse c1f45f9)"
run_cmd "f2_runner handover（漂移臂②）" python3 -I "$AF_WT/ci/f2_runner.py" handover \
  --record "$RECORD" --repo "$CAND" --base-ref "$SHA_DRIFT_DIFF" --branch feature/f2-drill

# ── S16 收尾 ────────────────────────────────────────────────────────────────
step S16 "收尾读数"
say "队列任务件数=$(ls "$QUEUE/tasks" | wc -l)"
say "结果件数=$(ls "$QUEUE/results" | wc -l)"
say "回报行数=$(wc -l < "$QUEUE/reported.jsonl")"
say "记录件数（正向臂 + 租约臂；红/SKIP/缺件臂应无记录）=$(ls "$OUT"/integrator-record-*.json 2>/dev/null | wc -l)"
say "演练结束=$(date '+%F %T')"
