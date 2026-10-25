#!/usr/bin/env bash
# =============================================================================
# AF CI 骨架（ci-skeleton 定案管道）— AF/ci/run.sh
#
# 六步验收管道，全部真实执行、逐步计时、产物落 AF/ci/out/：
#   1 fmt     cargo fmt --check                          硬门：非零即红
#   2 clippy  cargo clippy + 基线门（ci/clippy_gate.py）   硬门：只拦新增警告
#   3 test    cargo test -j4 --workspace                 硬门：非零即红
#   4 cue     poc-cue recipe vet + export + 结构断言      硬门：正例过/反例拒/断言全中
#   5 scrub   ci/parse_scrub.py 对 7 份真实日志判定       硬门：期望判定逐一核对
#   6 cosign  断网 netns verify-blob --trusted-root    硬门：正例过/篡改例拒
#             （sudo unshare -n 优先；no_new_privs 沙箱回退 unshare -Urn，隔离语义等价）
#
# 外部依赖: cargo+rustup(rustfmt/clippy 组件)、python3、unshare（sudo 可选）、
#           AF/bin/cosign、AF/poc/poc-cue/bin/cue、AF/poc/poc-cas 真实测试资产。
# 可重复性: 全部路径相对本脚本所在仓库根；产物写 ci/out/（可整目录删除重跑）。
# 退出码: 0=全绿；1=有硬门失败（所有步骤仍会跑完，一次拿全量诊断）。
# =============================================================================
set -u -o pipefail

AF_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CI_DIR="$AF_ROOT/ci"
OUT="$CI_DIR/out"
mkdir -p "$OUT"
export AF_ROOT OUT   # 步骤函数会在子 bash 里执行，必须导出

# ---- 计时与登记 -------------------------------------------------------------
declare -a SUMMARY=()
OVERALL=0

now_ns() { date +%s%N; }

step() { # step <编号> <名称> -- <命令...>   执行、计时、记录
  local id="$1" name="$2"; shift 2; [ "${1:-}" = "--" ] && shift
  local t0=$(now_ns) rc
  "$@" > "$OUT/step${id}.log" 2>&1
  rc=$?
  local t1=$(now_ns)
  local ms=$(( (t1 - t0) / 1000000 ))
  if [ $rc -eq 0 ]; then
    SUMMARY+=("step$id  PASS  ${ms}ms  $name")
    printf '  [%s] PASS %6dms  %s\n' "$id" "$ms" "$name"
  else
    SUMMARY+=("step$id  FAIL  ${ms}ms  $name  (exit=$rc, 日志: ci/out/step${id}.log)")
    printf '  [%s] FAIL %6dms  %s  (exit=%s)\n' "$id" "$ms" "$name" "$rc"
    tail -15 "$OUT/step${id}.log" | sed 's/^/      | /'
    OVERALL=1
  fi
  return 0   # 步骤失败不中断管道，跑完全部步骤收集完整诊断
}

# ---- 0. 预检 ---------------------------------------------------------------
echo "== AF CI 管道开始 $(date '+%F %T') root=$AF_ROOT"
if [ -f "$HOME/.cargo/env" ]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi
rustup component add rustfmt clippy >>"$OUT/step0.log" 2>&1
PREFLIGHT=()
for tool in cargo python3 sudo unshare "$AF_ROOT/bin/cosign" "$AF_ROOT/poc/poc-cue/bin/cue"; do
  if ! command -v "$tool" >/dev/null 2>&1 && [ ! -x "$tool" ]; then
    PREFLIGHT+=("缺依赖: $tool")
  fi
done
# workspace manifest 完整性（共享仓库里并发改动可能弄坏 members）
if ! (cd "$AF_ROOT" && cargo metadata --no-deps --format-version 1 >/dev/null 2>>"$OUT/step0.log"); then
  PREFLIGHT+=("cargo metadata 失败：workspace manifest 已损坏（查看 ci/out/step0.log）")
fi
if [ ${#PREFLIGHT[@]} -gt 0 ]; then
  printf '预检失败:\n' ; printf '  %s\n' "${PREFLIGHT[@]}"
  exit 2
fi
echo "  [0] 预检 OK（工具/组件/workspace manifest）"

# ---- 1. cargo fmt --check ---------------------------------------------------
echo "== step1 cargo fmt --check"
step 1 "cargo fmt --check" -- \
  bash -c "cd '$AF_ROOT' && cargo fmt --check --all"

# ---- 2. clippy + 警告基线门 --------------------------------------------------
echo "== step2 clippy（基线门：新增警告才 fail）"
step 2 "cargo clippy + clippy_gate" -- \
  bash -c "cd '$AF_ROOT' && cargo clippy --workspace --all-targets -j4 --message-format=json > ci/out/step2-clippy.jsonl 2>/dev/null; rc=\$?; [ \$rc -ne 0 ] && exit \$rc; python3 ci/clippy_gate.py ci/out/step2-clippy.jsonl"

# ---- 3. cargo test ----------------------------------------------------------
echo "== step3 cargo test"
step 3 "cargo test -j4 --workspace" -- \
  bash -c "cd '$AF_ROOT' && cargo test -j4 --workspace"

# ---- 4. CUE 配方预编译 --------------------------------------------------------
echo "== step4 cue vet/export + 结构断言"
cue_step() {
  local CUE="$AF_ROOT/poc/poc-cue/bin/cue"
  local R="$AF_ROOT/poc/poc-cue/recipe"
  # 4a. L3+L4 合法合并：vet 必须过
  "$CUE" vet "$R/recipe-default.cue" "$R/operator-override.cue" || return 1
  # 4b. export 落 JSON（部署期预编译产物）
  "$CUE" export "$R/recipe-default.cue" "$R/operator-override.cue" \
    > "$OUT/step4-recipe-merged.json" || return 1
  # 4c. 结构断言：L4 覆盖生效 + L3 默认保留 + 键集闭合
  python3 - "$OUT/step4-recipe-merged.json" <<'PYEOF' || return 1
import json, sys
d = json.load(open(sys.argv[1]))
expected = {
    "scheduling": {"batchIntervalSec": 15, "maxConcurrentJobs": 4,
                   "retry": {"maxAttempts": 3, "backoffSec": 5}},
    "thresholds": {"cycleTimeWarnSec": 120, "defectRatioWarn": 0.05,
                   "temperatureCritC": 85},
}
assert set(d) == {"scheduling", "thresholds"}, f"顶层键集漂移: {sorted(d)}"
assert d == expected, f"合并结果漂移:\n got={d}\n want={expected}"
print("recipe-merged.json 结构断言: 9 值全中，键集闭合")
PYEOF
  # 4d. 反例：L4 越界覆盖必须被 vet 拒绝（非零退出才算过）
  if "$CUE" vet "$R/recipe-default.cue" "$R/operator-override-bad.cue" \
      > "$OUT/step4-bad-override.log" 2>&1; then
    echo "反例 operator-override-bad.cue 未被拒绝" >&2; return 1
  fi
}
step 4 "cue recipe vet+export+断言" -- bash -c "$(declare -f cue_step); cue_step"

# ---- 5. zot scrub 输出解析门（真实日志测试集） ---------------------------------
echo "== step5 parse_scrub 真实日志测试集"
scrub_step() {
  local P="$AF_ROOT/ci/parse_scrub.py"
  local L="$AF_ROOT/poc/poc-cas/verify-logs"
  local verdict_file="$OUT/step5-scrub-verdicts.txt"; : > "$verdict_file"
  local rc_all=0
  # 应通过（STATUS 全 ok）
  for f in scrub-baseline.log scrub-after-gc.log; do
    python3 "$P" "$L/$f" >/dev/null 2>&1; rc=$?
    echo "PASS-EXPECTED  exit=$rc  $f" >> "$verdict_file"
    [ $rc -ne 0 ] && rc_all=1
  done
  # 应失败（STATUS=affected）
  for f in "$L"/scrub-corrupt1.log "$L"/scrub-corrupt2.log "$L"/scrub-corrupt3a.log \
           "$L"/scrub-corrupt3b-*.log; do
    python3 "$P" "$f" >/dev/null 2>&1; rc=$?
    echo "FAIL-EXPECTED  exit=$rc  $(basename "$f")" >> "$verdict_file"
    [ $rc -ne 1 ] && rc_all=1
  done
  cat "$verdict_file"
  return $rc_all
}
step 5 "parse_scrub × 7 真实日志" -- bash -c "$(declare -f scrub_step); scrub_step"

# ---- 6. cosign 离线验签（断网命名空间） ---------------------------------------
echo "== step6 cosign --trusted-root 断网验签"
cosign_step() {
  local COSIGN="$AF_ROOT/bin/cosign"
  local T="$AF_ROOT/poc/poc-cas/verify-tr"
  cd "$T" || return 2
  # 断网手段择一即可：sudo unshare -n（真 root netns）优先；sudo 不可用（如
  # no_new_privs 沙箱）时回退 unshare -Urn——非特权 user+netns，loopback down、
  # 无外联，断网隔离语义等价，门禁强度不变（6b 篡改例仍必须被拒）。
  local NETNS=()
  if command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
    NETNS=(sudo unshare -n)
  elif unshare -Urn true 2>/dev/null; then
    NETNS=(unshare -Urn)
  else
    echo "无可用断网手段：sudo(unshare -n) 与 unshare -Urn 均失败" >&2
    return 2
  fi
  # 6a. 正例：无网络命名空间内，bundle+trusted-root 离线验签必须过
  "${NETNS[@]}" "$COSIGN" verify-blob \
    --key tr-key.pub --bundle tr-bundle.json --trusted-root trusted-root.json \
    artifact-tr.txt > "$OUT/step6-verify.log" 2>&1 || { return 1; }
  # 6b. 负例：篡改 1 字节的工件必须被拒（证明门真的在验）
  if "${NETNS[@]}" "$COSIGN" verify-blob \
      --key tr-key.pub --bundle tr-bundle.json --trusted-root trusted-root.json \
      artifact-tampered.txt >> "$OUT/step6-verify.log" 2>&1; then
    echo "篡改工件未被拒绝" >&2; return 1
  fi
  echo "正例: $(head -1 "$OUT/step6-verify.log")；篡改例: 已拒绝（exit 1）；netns: ${NETNS[*]}"
}
step 6 "cosign 断网 verify-blob" -- bash -c "$(declare -f cosign_step); cosign_step"

# ---- 7. cargo deny 供应链门（licenses/bans/sources/advisories，离线本地 db） ----
echo "== step7 cargo deny（四类检查，advisory-db 本地化离线）"
step 7 "cargo deny --offline check" -- bash -c "cd '$AF_ROOT' && cargo deny --offline check"

# ---- 8. 中立性门（N1 删包自洽 + N2 领域词扫描 + N3 golden 登记；G4 施工批
#         Phase 3 挂入，裁决15/16/19）------------------------------------------------
# 挂门前置断言（裁决15）：挂门时对照臂（沙箱 cp 集含 rust-toolchain.toml——
# 裁决19）cargo test -p af-dsl -p af-compiler 实测 exit=0。2026-10-25 挂门时点
# 实测 CONTROL_EXIT=0（全绿）；此后该断言由门内 C1 判据常态化复测。
# 环境开关：AF_NEUT_SANDBOX_DIR / AF_NEUT_TARGET_DIR（复核修正 2026-10-25：
# 缺省=家目录外置 $HOME/afb-neut-sbx + $HOME/afbuild-neutrality——/tmp 每用户
# 配额会使内建 target 冷建撞 os error 122 连锁判据红；门内另有 128MB 配额
# 探针 fail-fast）。详见 ci/neutrality_gate.sh 头注。
echo "== step8 neutrality 删包自洽+领域词扫描+golden 登记"
step 8 "neutrality 删包自洽+领域词扫描+golden 登记" -- \
  bash "$CI_DIR/neutrality_gate.sh"

# ---- 汇总 -------------------------------------------------------------------
echo "== 汇总"
printf '%s\n' "${SUMMARY[@]}" | tee "$OUT/SUMMARY.txt"
if [ $OVERALL -eq 0 ]; then
  echo "== 管道全绿"
else
  echo "== 管道存在红项（见上），exit 1"
fi
exit $OVERALL
