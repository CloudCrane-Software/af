#!/usr/bin/env bash
# =============================================================================
# PoC-E F1 快速门 — poc/poc-e-codeadapter/gate_f1.sh
#
# af 仓 ci/run.sh 的**可跑子集**（对样例仓跑）：取其 step1 fmt / step2 clippy /
# step3 test 三步，同款 step 执行-计时-SUMMARY 语义（step 失败不中断、跑完收全
# 量诊断、退出码=有硬门失败）。af ci/run.sh 其余步（cue/scrub/cosign/deny/
# neutrality/fv/对账三件套/横切）绑定 af 仓资产与判据正本，在样例仓上不可跑，
# 故不伪造这些步——门=可跑子集，如实登记（PoC-E 判据：门绿才合入）。
#
# 用法: gate_f1.sh <worktree> <out-dir>
#   step1  cargo fmt --check --all            硬门：非零即红（同 ci/run.sh step1，
#                                             K5 判分链路径 lint 属 af 仓件，不在子集）
#   step2  cargo clippy --workspace --all-targets -- -D warnings
#           硬门：样例仓无基线文件，取 fail-closed 语义（全警告即红）；
#           af 仓原步为基线门（只拦新增警告 ci/clippy_gate.py），语义差异如实登记
#   step3  cargo test -j4 --workspace         硬门：非零即红（同 ci/run.sh step3；
#           provenance 门属共享 target 防护，样例仓自带独立 target，不在子集）
#
# 独立 target：CARGO_TARGET_DIR=<out-dir>/target，不污染样例仓工作树，
# 可整目录删除复跑。退出码：0=门绿（SUMMARY 写 PIPELINE-GREEN）；1=有红。
# =============================================================================
set -u -o pipefail

WT="$(cd "$1" && pwd)"
OUT="$2"
mkdir -p "$OUT"
export CARGO_TARGET_DIR="$OUT/target"

declare -a SUMMARY=()
OVERALL=0
now_ns() { date +%s%N; }

step() {
  local id="$1" name="$2"; shift 2; [ "${1:-}" = "--" ] && shift
  local t0 t1 ms rc
  t0=$(now_ns)
  "$@" > "$OUT/step${id}.log" 2>&1
  rc=$?
  t1=$(now_ns)
  ms=$(( (t1 - t0) / 1000000 ))
  if [ $rc -eq 0 ]; then
    SUMMARY+=("step$id  PASS  ${ms}ms  $name")
    printf '  [%s] PASS %6dms  %s\n' "$id" "$ms" "$name"
  else
    SUMMARY+=("step$id  FAIL  ${ms}ms  $name  (exit=$rc, 日志: step${id}.log)")
    printf '  [%s] FAIL %6dms  %s  (exit=%s)\n' "$id" "$ms" "$name" "$rc"
    tail -15 "$OUT/step${id}.log" | sed 's/^/      | /'
    OVERALL=1
  fi
  return 0
}

if [ -f "$HOME/.cargo/env" ]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

echo "== PoC-E F1 快速门（af ci/run.sh 可跑子集 fmt/clippy/test） $(date '+%F %T') wt=$WT"

step 1 "cargo fmt --check --all" -- \
  bash -c "cd '$WT' && cargo fmt --check --all"

step 2 "cargo clippy --workspace --all-targets -- -D warnings" -- \
  bash -c "cd '$WT' && cargo clippy --workspace --all-targets -- -D warnings"

step 3 "cargo test -j4 --workspace" -- \
  bash -c "cd '$WT' && cargo test -j4 --workspace"

printf '%s\n' "${SUMMARY[@]}" | tee "$OUT/SUMMARY.txt"
if [ $OVERALL -eq 0 ]; then
  echo "PIPELINE-GREEN" | tee -a "$OUT/SUMMARY.txt"
else
  echo "PIPELINE-RED (exit 1)" | tee -a "$OUT/SUMMARY.txt"
fi
exit $OVERALL
