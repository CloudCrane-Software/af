#!/usr/bin/env bash
# xcheck 反例探针（G0-CI 机制批 · 子代理 D，SPEC-2 ⑤.4「随批带探针」PP1）
# 四个 mutation 全部在**输出目录副本**上执行（正本零改动，SPEC ⑤.5），各跑必红：
#   ① gates：AGENT.md 副本门数 九门→十门（复盘 C2 六门/九门史复发形态）
#   ② packages：README 副本件数 37→35（复盘 W1-C65 原案复发）
#   ③ redlist：expected-fail 副本删一条非注释行（红名单三代漂移：只改一侧）
#   ④ obs-dates：组件建设状态副本改任一观察期满日期（复盘 C7/W2-C-17 四文书三个日期）
# 对照臂：正本实跑不得红（exit 0 或 2——advisory 不误红）。
# 自检性：sed 未命中（正本措辞漂移）⇒ 副本与正本无异 ⇒ 探针不红 ⇒ 本脚本报 FAIL，
#   不会静默放过（探针自身失效也必须红）。
# 用法：bash ci/xcheck_probe.sh（工作目录任意；内部 cd 到本脚本所在仓库根）
# 退出码：0=四探针全红且对照臂不红；1=任一探针未红或对照臂红（探针失效）。
# 挂载定位：standalone 工具——不并入 ci/run.sh step14 常跑面；由 mutations 元探针
#   框架（B3，diff 触及 ci/ 时全量跑）或人工复检调用。
set -u
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO" || exit 1
# 副本来源=与 xcheck.py 默认值一致的工作区/仓内正本（可环境变量覆写）
AGENT_MD="${AGENT_MD:-/home/pan-ding/workspace/agent黑灯工厂/AGENT.md}"
PKG_README="${PKG_README:-$REPO/af/packages/README.md}"
EXPECTED_FAIL="${EXPECTED_FAIL:-$REPO/ci/neutrality-expected-fail.txt}"
DOC_STATUS="${DOC_STATUS:-/home/pan-ding/workspace/agent黑灯工厂/状态/组件建设状态.md}"
TMP="$(mktemp -d)" ; trap 'rm -rf "$TMP"' EXIT
fails=0

run_probe() { # run_probe <名> <期望exit=1> <期望FAIL组名> -- <xcheck 参数...>
  local name="$1" want_grp="$3"; shift 3; [ "${1:-}" = "--" ] && shift
  local out="$TMP/$name.out" rc
  python3 ci/xcheck.py "$@" >"$out" 2>&1; rc=$?
  if [ "$rc" -eq 1 ] && grep -q "^\[FAIL\] $want_grp" "$out"; then
    echo "PASS 探针 $name：exit=1，$want_grp 红（$(grep -m1 "^\[FAIL\] $want_grp" -A1 "$out" | tail -1 | sed 's/^ *//')）"
  else
    echo "FAIL 探针 $name：期望 exit=1 且 $want_grp 红，实得 exit=$rc"; sed 's/^/      | /' "$out"; fails=$((fails+1))
  fi
}

# ---- 对照臂：正本不得红 ----
python3 ci/xcheck.py >"$TMP/control.out" 2>&1; crc=$?
if [ "$crc" -eq 1 ]; then
  echo "FAIL 对照臂：正本 xcheck 红（exit=1）——先修正本再谈探针"; sed 's/^/      | /' "$TMP/control.out"; fails=$((fails+1))
else
  echo "PASS 对照臂：正本 xcheck exit=$crc（0=全过 / 2=仅 advisory 红，均不误红）"
fi

# ---- ① gates：九门→十门 ----
sed '0,/\*\*九门\*\*（step1-8 硬门/s//**十门**（step1-8 硬门/' "$AGENT_MD" >"$TMP/agent-ten.txt"
if cmp -s "$AGENT_MD" "$TMP/agent-ten.txt"; then
  echo "FAIL 探针 gates：sed 未命中『**九门**（step1-8 硬门』——AGENT.md 措辞漂移，探针源失效"; fails=$((fails+1))
else
  run_probe gates 1 gates -- --agent-md "$TMP/agent-ten.txt"
fi

# ---- ② packages：37→35 ----
sed 's/十包实为 \*\*37 件\*\*/十包实为 **35 件**/' "$PKG_README" >"$TMP/readme-35.md"
if cmp -s "$PKG_README" "$TMP/readme-35.md"; then
  echo "FAIL 探针 packages：sed 未命中『十包实为 **37 件**』——README 措辞漂移，探针源失效"; fails=$((fails+1))
else
  run_probe packages 1 packages -- --readme "$TMP/readme-35.md"
fi

# ---- ③ redlist：删 expected-fail 副本第一条非注释行 ----
first_ln="$(grep -n -m1 -v '^[[:space:]]*\(#\|$\)' "$EXPECTED_FAIL" | cut -d: -f1)"
if [ -z "$first_ln" ]; then
  echo "FAIL 探针 redlist：expected-fail 无非注释行可删"; fails=$((fails+1))
else
  sed "${first_ln}d" "$EXPECTED_FAIL" >"$TMP/ef-minus1.txt"
  run_probe redlist 1 redlist -- --expected-fail "$TMP/ef-minus1.txt"
fi

# ---- ④ obs-dates：组件建设状态副本改观察期满日期 ----
sed 's/2026-10-29 01:05/2026-10-30 01:05/g' "$DOC_STATUS" >"$TMP/status-shift.md"
if cmp -s "$DOC_STATUS" "$TMP/status-shift.md"; then
  echo "FAIL 探针 obs-dates：sed 未命中『2026-10-29 01:05』——文书措辞漂移，探针源失效"; fails=$((fails+1))
else
  run_probe obs-dates 1 obs-dates -- --doc-status "$TMP/status-shift.md"
fi

echo "== xcheck_probe: 探针红项 $((4 - fails))/4（含对照臂共 $((5 - fails))/5 项合预期）"
[ "$fails" -eq 0 ] || exit 1
exit 0
