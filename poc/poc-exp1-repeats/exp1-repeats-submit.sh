#!/usr/bin/env bash
# exp1-repeats-submit.sh — EXP-1 repeat 轮（A0/A1/A2 各 24 任务第二跑，72 单）提交脚本
#
# 预注册依据（只读正本，不改）：
#   计划/研究组/B线核心批-20261029/experiments-v1.json EXP-1
#     · randomization.repeats = "A0/A1/A2 三臂各 24 任务跑第 2 次（估 flakiness）；剂量臂不重复"
#     · sample_size.repeats_runs = 72（runs_total=216 = main 96 + dose 48 + repeat 72）
#   计划/研究组/B线核心批-20261029/exp1-allocation.json → runs[] 中 round=="repeat" 的 72 行
#     （task_id/arm/repeat=1/block_id/arm_position 全量预登记）
#
# 与主轮（exp1-submit.sh，144 单）的差异，逐条：
#   1) 臂集合 6 → 3（A0/A1/A2），剂量臂 A1d25/A1d50 不重复（预注册明文）；
#   2) 订单号前缀 E1- → E1R-（order_id=E1R-<arm>-<paper>）：
#      · af-order 同 order_id 幂等不重跑（主轮 E1-<arm>-<paper> 已终态，不可复用）；
#      · 选 E1R- 而非 E1-<arm>-r2-<paper>：主轮聚合脚本 tools/b-exp1-aggregate.py 的
#        订单 glob 是 "E1-*"，"E1-<arm>-r2-*" 会被它误吞并触发 fail-loud（arm=None 不在
#        闭集）；"E1R-*" 不匹配 "E1-*"，两轮镜像天然隔离，杜绝混镜像；
#   3) 状态根独立：/data/bench/exp1-state-r2（不与主轮 /data/bench/exp1-state 混用）；
#   4) line 绑定沿用主轮 bline-exp1/1（与 EXP-2 repeat 轮同款：同产线、新 order_id）；
#   5) 并发 4（波次制度 anolis_concurrency=4；主轮同档，实测墙钟口径可比）。
#
# 其余纪律（逐字沿用主轮脚本）：
#   · 预检 glm-cp profile 存在（af 仓 profile 定义 或 af-order 既有订单实例），
#     不存在则停手并打印升级指引——不得静默替换模型档；
#   · 密钥零明文：profile 的 key_cmd 由引擎内部执行，本脚本不接触任何密钥；
#     密钥通道预检只跑 key_cmd 并报长度（口径锚 profile.rs resolve_key，
#     trim 后 <16 字符即失败），绝不打印密钥内容；KEY_PREFLIGHT=0 可跳过。
#   · 运行身份：anolis 上以 pan-ding 运行（root 身份下多数档位 key_cmd 读不到
#     ~/.zcode provider 配置）。
#   · STOP 熔断：$STATE_ROOT/STOP 在位则本轮不提交（与 burn-runner 同款纪律）。
#   · 幂等：同 order_id 重放不重复终态（af-order 既有语义），可安全重跑本脚本。
#   · 无裸 set -e（波次制度 §六.2：预期失败清理命令会中止全链）→ 分段守卫。
#
# 用法（anolis 上）：
#   DRY_RUN=1 bash exp1-repeats-submit.sh       # 只跑预检+打印将执行的命令，不提交
#   bash exp1-repeats-submit.sh                 # 全量 72 包
#   ARMS="A0" EXPECT_PKGS=24 bash exp1-repeats-submit.sh   # 冒烟：单臂 24 包
set -u

AF_ORDER="${AF_ORDER:-/data/af-factory/target-p3/debug/af-order}"
AF_WORKSPACE="${AF_WORKSPACE:-/data/af-factory/af-p3}"
AF_TARGET="${AF_TARGET:-/data/af-factory/target-p3}"
PROFILE_RS="${PROFILE_RS:-$AF_WORKSPACE/crates/af-execline/src/profile.rs}"
PKGS_ROOT="${PKGS_ROOT:-/data/bench/exp1-pkgs}"
STATE_ROOT="${STATE_ROOT:-/data/bench/exp1-state-r2}"
SPEC_DIR="${SPEC_DIR:-$STATE_ROOT/specs}"
LOG_DIR="${LOG_DIR:-$STATE_ROOT/logs}"
RESULT_DIR="${RESULT_DIR:-$STATE_ROOT/results}"
ORDER_PREFIX="${ORDER_PREFIX:-E1R-}"
PROFILE="${PROFILE:-glm-cp}"
UNIT="${UNIT:-report}"
HOST="${HOST:-local}"
MAX_TURNS="${MAX_TURNS:-40}"
SPEND_CAP_CENTS="${SPEND_CAP_CENTS:-300}"
MES_CH="${MES_CH:-}"   # 预注册 command_template 无 --mes-ch；默认不传
CONCURRENCY="${CONCURRENCY:-4}"
PER_PKG_TIMEOUT_SECS="${PER_PKG_TIMEOUT_SECS:-1800}"
LINE_ID="${LINE_ID:-bline-exp1}"
LINE_VERSION="${LINE_VERSION:-1}"
PLAN_SHA256="d841f1b6951f1ff37ddadec5e2437e5fb88fc75c8fd248cf762829e9b2e52448"
EXPECT_PKGS="${EXPECT_PKGS:-72}"   # repeat 轮真跑=72（A0/A1/A2 各 24）
ARMS="${ARMS:-A0 A1 A2}"
ORDER_STATE_ROOTS="${ORDER_STATE_ROOTS:-/data/bench/af-p3-state /data/bench/exp1-state}"
KEYCHECK="${KEYCHECK:-/data/bench/exp1/exp1-keycheck.py}"
MKSPEC="${MKSPEC:-/data/bench/exp1/exp1-repeats-mkspec.py}"
LIST_FILE="${LIST_FILE:-/data/bench/exp1/pkg-order-r2.txt}"
MIN_FREE_MB="${MIN_FREE_MB:-2000}"

die() { echo "FATAL: $*" >&2; exit 3; }

# ---------------------------------------------------------------- 预检 1：STOP 熔断
if [ -e "$STATE_ROOT/STOP" ]; then
  echo "STOP 熔断在位（$STATE_ROOT/STOP），本轮不提交" >&2
  exit 3
fi

# ---------------------------------------------------------------- 预检 2：glm-cp profile 存在
profile_src_ok=0
if [ -r "$PROFILE_RS" ]; then
  if grep -q "\"$PROFILE\" =>" "$PROFILE_RS" \
     && grep -A 14 'pub const PROFILE_NAMES' "$PROFILE_RS" | grep -q "\"$PROFILE\""; then
    profile_src_ok=1
    echo "预检：profile 定义命中（$PROFILE_RS：match 臂 + PROFILE_NAMES 闭集）"
  fi
fi
profile_order_ok=0
for root in $ORDER_STATE_ROOTS; do
  if [ -d "$root/orders" ] && grep -rqs -- "$PROFILE" "$root/orders" 2>/dev/null; then
    profile_order_ok=1
    echo "预检：既有订单实例命中（$root/orders 内含 $PROFILE）"
  fi
done
if [ "$profile_src_ok" -ne 1 ] && [ "$profile_order_ok" -ne 1 ]; then
  echo "FATAL: 预检未过——profile '$PROFILE' 既不在 af 仓 profile 定义，也没有既有订单实例" >&2
  exit 3
fi

# ---------------------------------------------------------------- 预检 2b：密钥通道可解析（零明文）
KEY_PREFLIGHT="${KEY_PREFLIGHT:-1}"
if [ "$(id -u)" = "0" ]; then
  echo "预检注意：当前为 root；anolis 上 key_cmd 通常要 pan-ding 身份（burn-runner 同款）" >&2
fi
if [ "$KEY_PREFLIGHT" = "1" ]; then
  key_line=$(python3 "$KEYCHECK" "$PROFILE_RS" "$PROFILE" 2>&1)
  key_rc=$?
  if [ "$key_rc" -ne 0 ]; then
    echo "FATAL: 预检未过——profile '$PROFILE' 密钥通道不可用：$key_line" >&2
    exit 3
  fi
  echo "预检：$PROFILE 密钥通道可解析（$key_line；只报长度，零明文）"
else
  echo "预检：密钥通道预检跳过（KEY_PREFLIGHT=0）——已知风险自负"
fi

# ---------------------------------------------------------------- 预检 3：二进制与引擎
[ -x "$AF_ORDER" ] || die "af-order 不可执行：$AF_ORDER"
[ -f "$AF_TARGET/debug/af-engine" ] || die "af-engine 缺席：$AF_TARGET/debug/af-engine"
command -v python3 >/dev/null 2>&1 || die "python3 缺席（spec 生成需要）"
[ -r "$MKSPEC" ] || die "spec 生成脚本不可读：$MKSPEC"

# ---------------------------------------------------------------- 预检 3b：磁盘余量
free_mb=$(df -Pm "$(dirname "$STATE_ROOT")" 2>/dev/null | awk 'NR==2{print $4}')
if [ -n "$free_mb" ] && [ "$free_mb" -lt "$MIN_FREE_MB" ]; then
  die "磁盘余量不足：${free_mb}MB < ${MIN_FREE_MB}MB（state 根所在盘）"
fi
[ -n "$free_mb" ] && echo "预检：磁盘余量 ${free_mb}MB（≥${MIN_FREE_MB}MB 门）"

# ---------------------------------------------------------------- 预检 4：任务包在场且 plan.json 冻结
[ -d "$PKGS_ROOT" ] || die "任务包根缺席：$PKGS_ROOT"
plan_bad=0
for arm in $ARMS; do
  [ -d "$PKGS_ROOT/$arm" ] || { echo "FATAL: 臂目录缺席：$PKGS_ROOT/$arm" >&2; exit 3; }
  for d in "$PKGS_ROOT/$arm"/*/; do
    [ -f "$d/task.json" ] || { echo "FATAL: 缺 task.json：$d" >&2; exit 3; }
    [ -f "$d/plan.json" ] || { echo "FATAL: 缺 plan.json：$d" >&2; exit 3; }
  done
done
while IFS= read -r line; do
  sha="${line%% *}"
  if [ "$sha" != "$PLAN_SHA256" ]; then
    echo "FATAL: plan.json 非冻结版：$line（锚 $PLAN_SHA256）" >&2
    plan_bad=1
  fi
done < <(find "$PKGS_ROOT" -name plan.json -print0 | xargs -0 sha256sum)
[ "$plan_bad" -eq 0 ] || exit 3
plan_n=$(find "$PKGS_ROOT" -name plan.json | wc -l)
echo "预检：$plan_n 个 plan.json 全部命中冻结版 sha256 $PLAN_SHA256（全集 144；本轮取臂 $ARMS）"

mkdir -p "$SPEC_DIR" "$LOG_DIR" "$RESULT_DIR"

# ---------------------------------------------------------------- 包清单（预注册分配顺序，臂过滤）
build_default_list() {
  for arm in $ARMS; do
    for d in "$PKGS_ROOT/$arm"/*/; do
      basename "$d" | sed "s|^|$arm/|"
    done
  done
}
if [ -r "$LIST_FILE" ]; then
  tmp_sorted=$(mktemp)
  build_default_list | sort > "$tmp_sorted"
  if ! diff -q <(sort "$LIST_FILE") "$tmp_sorted" >/dev/null; then
    echo "FATAL: $LIST_FILE（按臂 $ARMS 过滤）与包集合不一一对应" >&2
    diff <(sort "$LIST_FILE") "$tmp_sorted" | head -20 >&2
    rm -f "$tmp_sorted"
    exit 3
  fi
  rm -f "$tmp_sorted"
  list_n=$(wc -l < "$LIST_FILE")
  [ "$list_n" -eq "$EXPECT_PKGS" ] || die "包清单项数 $list_n ≠ EXPECT_PKGS $EXPECT_PKGS"
  echo "包清单：$LIST_FILE（分配表顺序，臂 $ARMS 共 $list_n 项校验通过）"
  cp "$LIST_FILE" "$STATE_ROOT/pkg-list.txt"
else
  echo "包清单：$LIST_FILE 缺席，回落臂序×论文序（可复算默认序）"
  build_default_list > "$STATE_ROOT/pkg-list.txt"
fi

# ---------------------------------------------------------------- 单包提交
submit_one() {
  local pkg="$1"
  local arm="${pkg%%/*}"
  local paper="${pkg#*/}"
  local order_id="${ORDER_PREFIX}${arm}-${paper}"
  local spec="$SPEC_DIR/${order_id}.json"
  local log="$LOG_DIR/${order_id}.log"
  local res="$RESULT_DIR/${order_id}.result"
  local pkg_dir="$PKGS_ROOT/$pkg"

  if ! python3 "$MKSPEC" "$spec" "$order_id" "$pkg" "$LINE_ID" "$LINE_VERSION" "$SPEND_CAP_CENTS"; then
    echo "order_id=$order_id pkg=$pkg exit=3 status=spec_gen_failed" > "$res"
    return 0
  fi

  local t0 t1 rc
  t0=$(date +%s)
  timeout "$PER_PKG_TIMEOUT_SECS" "$AF_ORDER" submit \
    --spec "$spec" \
    --state "$STATE_ROOT" \
    --workspace "$AF_WORKSPACE" \
    --target-dir "$AF_TARGET" \
    --unit "$UNIT" \
    --task "$pkg" \
    --profile "$PROFILE" \
    --data-root "$PKGS_ROOT" \
    --host "$HOST" \
    --max-turns "$MAX_TURNS" \
    --spend-cap-cents "$SPEND_CAP_CENTS" \
    ${MES_CH:+--mes-ch "$MES_CH"} \
    > "$log" 2>&1
  rc=$?
  t1=$(date +%s)
  local state="ok"
  [ "$rc" -eq 0 ] || state="submit_failed"
  local order_state="unknown"
  if [ -f "$STATE_ROOT/orders/$order_id/ledger.jsonl" ]; then
    order_state=$(tail -1 "$STATE_ROOT/orders/$order_id/ledger.jsonl" \
      | python3 -c 'import json,sys;
try:
    print(json.loads(sys.stdin.read()).get("state_after","?"))
except Exception:
    print("?")' 2>/dev/null)
  fi
  echo "order_id=$order_id pkg=$pkg exit=$rc status=$state order_state=$order_state wall_s=$((t1-t0))" > "$res"
  return 0
}
export -f submit_one
export AF_ORDER AF_WORKSPACE AF_TARGET PKGS_ROOT STATE_ROOT SPEC_DIR LOG_DIR RESULT_DIR MKSPEC
export ORDER_PREFIX PROFILE UNIT HOST MAX_TURNS SPEND_CAP_CENTS MES_CH PER_PKG_TIMEOUT_SECS LINE_ID LINE_VERSION

if [ "${DRY_RUN:-0}" = "1" ]; then
  first=$(head -1 "$STATE_ROOT/pkg-list.txt")
  echo "=== DRY_RUN：预检全部通过；将执行的命令形如（首包 $first）："
  echo "timeout $PER_PKG_TIMEOUT_SECS $AF_ORDER submit \\"
  echo "  --spec $SPEC_DIR/${ORDER_PREFIX}${first%%/*}-${first#*/}.json \\"
  echo "  --state $STATE_ROOT --workspace $AF_WORKSPACE --target-dir $AF_TARGET \\"
  echo "  --unit $UNIT --task $first --profile $PROFILE --data-root $PKGS_ROOT \\"
  echo "  --host $HOST --max-turns $MAX_TURNS --spend-cap-cents $SPEND_CAP_CENTS"
  echo "=== DRY_RUN 退出（未提交任何订单）"
  exit 0
fi

echo "=== 开始提交：并发 $CONCURRENCY，包数 $(wc -l < "$STATE_ROOT/pkg-list.txt")，order_id 前缀 $ORDER_PREFIX，臂 $ARMS"
START_TS=$(date +%s)
xargs -a "$STATE_ROOT/pkg-list.txt" -P "$CONCURRENCY" -I{} bash -c 'submit_one "$@"' _ {}
END_TS=$(date +%s)

# ---------------------------------------------------------------- 末尾汇总
echo "=== 提交完毕，墙钟 $((END_TS-START_TS))s；汇总（$RESULT_DIR）："
total=0; ok=0; failed=0; closed=0
: > "$STATE_ROOT/failed-pkgs.txt"
while IFS= read -r res; do
  total=$((total+1))
  status=$(sed -n 's/.*status=\([a-z_]*\).*/\1/p' "$res")
  ostate=$(sed -n 's/.*order_state=\([a-z_A-Z?]*\).*/\1/p' "$res")
  if [ "$status" = "ok" ] && [ "$ostate" = "Closed" ]; then
    ok=$((ok+1)); closed=$((closed+1))
  else
    failed=$((failed+1))
    cat "$res" >> "$STATE_ROOT/failed-pkgs.txt"
  fi
done < <(ls "$RESULT_DIR"/*.result 2>/dev/null)

echo "--- 总计 $total 包：Closed $closed / ok $ok / 非终态 $failed"
if [ "$failed" -gt 0 ]; then
  echo "--- 非 Closed 明细（$STATE_ROOT/failed-pkgs.txt）："
  cat "$STATE_ROOT/failed-pkgs.txt"
fi
for arm in $ARMS; do
  a_total=$(grep -c "pkg=$arm/" "$STATE_ROOT"/failed-pkgs.txt 2>/dev/null || true)
  a_all=$(grep -c "^$arm/" "$STATE_ROOT/pkg-list.txt" || true)
  a_ok=$((a_all - a_total))
  echo "    臂 $arm：Closed $a_ok/$a_all"
done
echo "--- 订单状态根：$STATE_ROOT/orders（ledger/records/runs 全套）；日志：$LOG_DIR"
[ "$failed" -eq 0 ] && [ "$total" -eq "$EXPECT_PKGS" ] && exit 0
exit 1
