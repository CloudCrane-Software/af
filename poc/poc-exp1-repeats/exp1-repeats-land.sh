#!/usr/bin/env bash
# exp1-repeats-land.sh — EXP-1 repeat 轮 72 单的**自动落数段**（本机常驻）。
#
# 与起批段（anolis: exp1-repeats-armed-run.sh）配对：
#   ① 轮询 anolis 的 DONE-MARKER（最长 WAIT_MAX 秒）；
#   ② 只读回传 repeat 轮镜像（tar over ssh）→ ~/exp1-raw-r2/mirror/orders；
#   ③ 先跑**口径对拍**（--selfcheck-only，主轮已发布汇总 vs 本件同源实现）——
#      不全 PASS 则拒绝落数（口径不同源 ⇒ 数据不可信）；
#   ④ 跑聚合器，写 data/ 三件（per-run 明细 + 汇总 json + 落数日志）；
#   ⑤ git add + commit + push（显式 refspec，非 force）到 feature/exp1-repeats-backfill。
#
# GAP-MARKER 情形（配额窗始终不可用）：写 gap-登记-补跑窗口未开.md 兜底件并落盘，不假报成功。
# 全程幂等：已落数则跳过（看 data/exp1-repeats-summary.json 是否存在）。
set -u

SELF_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="${REPO_ROOT:-$(cd "$SELF_DIR/../.." && pwd)}"
BRANCH="${BRANCH:-feature/exp1-repeats-backfill}"
REMOTE_REFSPEC="${REMOTE_REFSPEC:-origin}"
ANOLIS="${ANOLIS:-anolis}"
STATE_ROOT_REMOTE="${STATE_ROOT_REMOTE:-/data/bench/exp1-state-r2r}"
MIRROR_ROOT="${MIRROR_ROOT:-$HOME/exp1-raw-r2/mirror}"
WAIT_MAX="${WAIT_MAX:-43200}"      # 12 h
POLL_SLEEP="${POLL_SLEEP:-120}"
EXP1_DIR="${EXP1_DIR:-/home/pan-ding/workspace/agent黑灯工厂/计划/研究组/B线核心批-20261029}"
MAIN_SUMMARY="${MAIN_SUMMARY:-$EXP1_DIR/exp1-aggregate-summary.json}"
MAIN_MIRROR="${MAIN_MIRROR:-$HOME/exp1-raw/mirror/orders}"
LOG="${LOG:-$SELF_DIR/data/land.log}"
DATA_DIR="${DATA_DIR:-$SELF_DIR/data}"
mkdir -p "$DATA_DIR"

log() { echo "[$(date '+%F %T')] $*" | tee -a "$LOG"; }

if [ -f "$DATA_DIR/exp1-repeats-summary.json" ]; then
  log "已落数（$DATA_DIR/exp1-repeats-summary.json 存在）→ 幂等退出"
  exit 0
fi

# ---- ① 等 DONE-MARKER ----
deadline=$(( $(date +%s) + WAIT_MAX ))
ARMED_DIR="${ARMED_DIR:-/data/bench/exp1-armed}"
while [ "$(date +%s)" -lt "$deadline" ]; do
  if ssh -o BatchMode=yes -o ConnectTimeout=15 "$ANOLIS" \
       "cat $ARMED_DIR/DONE-MARKER 2>/dev/null" | grep -q "finished_at="; then
    log "DONE-MARKER 出现，开始回传"
    break
  fi
  if ssh -o BatchMode=yes -o ConnectTimeout=15 "$ANOLIS" \
       "ls $ARMED_DIR/GAP-MARKER 2>/dev/null" | grep -q GAP-MARKER; then
    log "检测到 GAP-MARKER（配额窗始终不可用）→ 走缺口登记兜底"
    cat > "$SELF_DIR/gap-登记-补跑窗口未开.md" <<EOF
# 缺口登记：EXP-1 repeat 轮 72 单补跑窗口未开（P3-B1 残项）

- 登记时刻：$(date '+%F %T')
- 触发：起批段在 6 轮尝试内始终命中 glm-cp 通道配额上限（provider code 1310
  「已达到每周/每月使用上限」），72 单未取得任何带 report.json 的工件。
- 口径依据：口径-EXP1-repeats-v1.md §七 偏差①/②（实测证据见 evidence/attempt1-quota429/）。
- harness 状态：在位（提交脚本 7 项预检全过；聚合器口径对拍全项 PASS）。
- 成本口径：72 单并发 4 ≈ 2.2 h；≈\$2.16（3¢/run 保守估）；cap \$30。
- 后续：配额窗开启后重跑 \`exp1-repeats-armed-run.sh\` + \`exp1-repeats-land.sh\` 即可自动补齐，
  本卡按「缺口如实登记」收口，不作任何数据或判定声明。
EOF
    log "已写缺口登记件：$SELF_DIR/gap-登记-补跑窗口未开.md"
    exit 1
  fi
  sleep "$POLL_SLEEP"
done

if [ "$(date +%s)" -ge "$deadline" ]; then
  log "等待超时（${WAIT_MAX}s）→ 未落数，退出（数据仍在 anolis，可人工重跑本脚本）"
  exit 1
fi

# ---- ② 回传镜像 ----
REMOTE_ROOT=$(ssh -o BatchMode=yes "$ANOLIS" "grep -E '^state_root=' $ARMED_DIR/DONE-MARKER 2>/dev/null | cut -d= -f2" | tail -1)
[ -n "$REMOTE_ROOT" ] || REMOTE_ROOT="$STATE_ROOT_REMOTE"
log "回传源：$REMOTE_ROOT/orders"
rm -rf "$MIRROR_ROOT"
mkdir -p "$MIRROR_ROOT"
ssh -o BatchMode=yes "$ANOLIS" "tar czf - -C '$REMOTE_ROOT' orders" | tar xzf - -C "$MIRROR_ROOT"
n=$(ls "$MIRROR_ROOT"/orders 2>/dev/null | wc -l)
log "回传完成：$n 个订单目录"

# ---- ③ 口径对拍（先决门）----
log "口径对拍（主轮已发布汇总 vs 本件同源实现）…"
if ! python3 -I "$SELF_DIR/exp1-repeats-aggregate.py" --selfcheck-only \
      --regress-main "$MAIN_SUMMARY" --main-mirror "$MAIN_MIRROR" >> "$LOG" 2>&1; then
  log "对拍未 PASS → 拒绝落数（口径不同源 ⇒ 数据不可信）"
  exit 2
fi
log "对拍 PASS"

# ---- ④ 聚合 ----
log "聚合 repeat 轮读数…"
python3 -I "$SELF_DIR/exp1-repeats-aggregate.py" \
  --mirror "$MIRROR_ROOT/orders" --main-mirror "$MAIN_MIRROR" \
  --exp1-dir "$EXP1_DIR" --out-dir "$DATA_DIR" >> "$LOG" 2>&1
rc=$?
log "聚合 rc=$rc"
[ "$rc" -eq 0 ] || exit "$rc"

# ---- ⑤ 落 commit + push ----
cd "$REPO_ROOT" || exit 3
git checkout "$BRANCH" >> "$LOG" 2>&1 || exit 3
git add -A "$SELF_DIR" >> "$LOG" 2>&1
if git diff --cached --quiet; then
  log "无待提交变更（幂等）"
else
  git commit -m "feat(poc): EXP-1 repeat 轮 72 单补跑读数落盘（flakiness + 不收敛任务名单；口径同源对拍 PASS）" >> "$LOG" 2>&1
  git push "$REMOTE_REFSPEC" "$BRANCH:$BRANCH" >> "$LOG" 2>&1
  log "push rc=$? （显式 refspec，非 force）"
fi
log "落数完成"
