#!/usr/bin/env bash
# exp1-repeats-armed-run.sh — EXP-1 repeat 轮 72 单的**自动起批段**（anolis 常驻）。
#
# 为什么需要它：2026-11-06 00:52–01:01 实测，glm-cp 通道（zhipu coding plan）
# 已达每周/每月上限（provider 错误码 1310），限额 2026-11-06 14:16:36 重置；
# 72 单批自身墙钟 ≈2.2 h（并发 4）。⇒ 数据落盘必然晚于任何一次短会话，
#   故把「等窗口 → 起批 → 早停闸 → 落标记」做成常驻脚本，不依赖代理在线。
#
# 早停闸（不空烧 72 单）：起批后前 N 单若连续命中 429（无 report.json 且账本含
# rate_limit/1310），则杀批 → 等 30 min → 再试；最多 MAX_ATTEMPTS 轮；
# 仍不可用 ⇒ 写 GAP-MARKER（缺口登记兜底，不假报成功）。
#
# 幂等：同 order_id 重放不重复终态（af-order 既有语义）；状态根固定，重复执行安全。
#
# 用法（anolis 上，pan-ding 身份）：
#   nohup bash exp1-repeats-armed-run.sh > /data/bench/exp1-armed.log 2>&1 &
# 环境变量：
#   T0_EPOCH      配额窗重置时刻（epoch 秒；缺省 2026-11-06 14:16:40 +0800）
#   STATE_ROOT    正式批状态根（缺省 /data/bench/exp1-state-r2r；与 attempt1 证据根分离）
#   MAX_ATTEMPTS  最大尝试轮数（缺省 6）
#   RETRY_SLEEP   轮间等待秒（缺省 1800）
#   EARLY_STOP_N  早停判定窗口（前 N 单，缺省 8）
set -u

SELF_DIR="$(cd "$(dirname "$0")" && pwd)"
T0_EPOCH="${T0_EPOCH:-$(date -d '2026-11-06 14:16:40 +0800' +%s)}"
BASE_ROOT="${STATE_ROOT:-/data/bench/exp1-state-r2r}"
STATE_ROOT="$BASE_ROOT"
MAX_ATTEMPTS="${MAX_ATTEMPTS:-6}"
RETRY_SLEEP="${RETRY_SLEEP:-1800}"
EARLY_STOP_N="${EARLY_STOP_N:-8}"
SUBMIT_SH="${SUBMIT_SH:-/data/bench/exp1/exp1-repeats-submit.sh}"
LOG_DIR="${LOG_DIR:-/data/bench/exp1-armed}"
MARK_DONE="$LOG_DIR/DONE-MARKER"
MARK_GAP="$LOG_DIR/GAP-MARKER"
mkdir -p "$LOG_DIR"

log() { echo "[$(date '+%F %T')] $*"; }

log "armed-run 启动：T0=$(date -d "@$T0_EPOCH" '+%F %T') state_root=$STATE_ROOT attempts<=$MAX_ATTEMPTS"

# ---- 等窗口 ----
while [ "$(date +%s)" -lt "$T0_EPOCH" ]; do
  sleep 60
done
log "到达配额窗重置时刻，开始起批"

attempt=1
while [ "$attempt" -le "$MAX_ATTEMPTS" ]; do
  attempt_log="$LOG_DIR/attempt-$attempt.log"
  rm -f "$STATE_ROOT/STOP" 2>/dev/null || true
  log "第 $attempt 轮起批 → $attempt_log"
  (
    cd /data/bench/exp1 && \
    env STATE_ROOT="$STATE_ROOT" \
        RESULT_DIR="$STATE_ROOT/results" \
        LOG_DIR="$STATE_ROOT/logs" \
        SPEC_DIR="$STATE_ROOT/specs" \
        LIST_FILE=/data/bench/exp1/pkg-order-r2.txt \
        ARMS="A0 A1 A2" EXPECT_PKGS=72 \
        bash "$SUBMIT_SH" > "$attempt_log" 2>&1
    echo "SUBMIT_EXIT=$?" >> "$attempt_log"
  ) &
  submit_pid=$!

  # ---- 早停闸：前 EARLY_STOP_N 单若连续 429 失败 ⇒ 杀批等下一轮 ----
  aborted=0
  waited=0
  while kill -0 "$submit_pid" 2>/dev/null; do
    sleep 60
    waited=$((waited + 60))
    n_res=$(ls "$STATE_ROOT"/results/*.result 2>/dev/null | wc -l)
    [ "$n_res" -ge "$EARLY_STOP_N" ] && break
  done
  if kill -0 "$submit_pid" 2>/dev/null; then
    n_res=$(ls "$STATE_ROOT"/results/*.result 2>/dev/null | wc -l)
    if [ "$n_res" -ge "$EARLY_STOP_N" ]; then
      # 前 N 单里没有一份带 report.json ⇒ 判定为配额窗仍不可用
      with_report=0
      for d in "$STATE_ROOT"/orders/*/runs/*/*/; do
        [ -f "$d/report.json" ] && with_report=$((with_report + 1))
      done
      if [ "$with_report" -eq 0 ]; then
        log "早停闸命中：前 $n_res 单零 report.json（配额窗仍不可用）→ 杀批，$RETRY_SLEEP s 后重试"
        pkill -f "exp1-state-r2r/specs" 2>/dev/null || true
        pkill -f "pkg-list.tx[t]" 2>/dev/null || true
        sleep 5
        kill -9 "$submit_pid" 2>/dev/null || true
        aborted=1
      fi
    fi
  fi

  if [ "$aborted" -eq 1 ]; then
    # 本轮已落的失败工件留证（不作数据），下一轮用新状态根避免污染
    mv "$STATE_ROOT" "${BASE_ROOT}-a${attempt}-quota429" 2>/dev/null || true
    STATE_ROOT="${BASE_ROOT}-a${attempt}"
    attempt=$((attempt + 1))
    log "等待 $RETRY_SLEEP s 后再试（下一轮状态根 $STATE_ROOT）"
    sleep "$RETRY_SLEEP"
    continue
  fi

  wait "$submit_pid" 2>/dev/null || true
  total=$(ls "$STATE_ROOT"/results/*.result 2>/dev/null | wc -l)
  closed=0
  for f in "$STATE_ROOT"/results/*.result; do
    [ -f "$f" ] || continue
    grep -q "order_state=Closed" "$f" && closed=$((closed + 1))
  done
  log "第 $attempt 轮结束：结果单 $total / Closed $closed"
  if [ "$total" -eq 72 ]; then
    {
      echo "finished_at=$(date '+%F %T')"
      echo "attempt=$attempt"
      echo "state_root=$STATE_ROOT"
      echo "results=$total"
      echo "closed=$closed"
    } > "$MARK_DONE"
    log "写 DONE-MARKER：$MARK_DONE"
    exit 0
  fi
  log "第 $attempt 轮未达 72 单（$total），$RETRY_SLEEP s 后再试"
  mv "$STATE_ROOT" "${BASE_ROOT}-a${attempt}-partial" 2>/dev/null || true
  STATE_ROOT="${BASE_ROOT}-a${attempt}"
  attempt=$((attempt + 1))
  sleep "$RETRY_SLEEP"
done

{
  echo "gap_at=$(date '+%F %T')"
  echo "attempts_used=$MAX_ATTEMPTS"
  echo "reason=配额窗在 $MAX_ATTEMPTS 轮内始终不可用（glm-cp provider code 1310 每周/每月上限）"
  echo "action=按口径登记分支收口：缺口如实登记（口径-EXP1-repeats-v1.md §七 偏差①/②）"
} > "$MARK_GAP"
log "写 GAP-MARKER：$MARK_GAP"
exit 1
