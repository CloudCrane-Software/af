#!/usr/bin/env bash
# =============================================================================
# ci/baseline_calc.sh — step9 9a/9b 基线机算器（G0-CI机制批 子代理B 草案）
#
# 缺陷回顾（复盘 §三 C3-③，附-A2 亲测）：
#   - 9b 基线手写常数 32（ci/run.sh:338 `if [ "${verified_k:-0}" -ne 32 ]`）——
#     人手改基线 = 判据基准可改（C6 同族病）；基线必须从 proofs 源机算。
#   - 9a 定理清单正本（proofs/rocq-规范库-v0/theorems-manifest.txt，W3-C-29 已落）
#     尚无独立于 run.sh 内联逻辑的再生成/校验工具——基线漂移防护需要
#     「正本可确定性再生成」这一自洽性面。
#
# 契约（三态退出码，全显式判退出码，禁裸管道消费退出码）：
#   gen-kani    [--src DIR] [-o FILE]  从 Kani INV 镜像 harness 源机算清单
#                                      （尊重 lib.rs 的 #[cfg(feature=..)] 门控：
#                                       门控模块不计——conflict_demo 2 条默认不编入）
#   verify-kani [-m FILE] [--src DIR]  再生成 vs 正本对账（名字集合+count+sha256）
#   gen-rocq    [--src DIR] [-o FILE]  再生成 theorems-manifest 数据段（与 9a-2
#                                      同一提取口径：^Theorem 名 + ^Theorem..^Proof.
#                                      陈述块按文件序拼接 SHA-256）
#   verify-rocq [-m FILE] [--src DIR]  再生成数据段 vs 正本数据行对账
#
# 正本路径缺省（相对 --root，缺省 root=脚本所在仓根）：
#   9b 正本 = ci/baselines/kani-inv-harness.manifest
#   9a 正本 = proofs/rocq-规范库-v0/theorems-manifest.txt
# 退出码: 0 一致/生成成功；1 漂移或缺件；2 用法/环境错误。
#
# 已知限制（详见 SPEC-B2 ⑧）：kani::proof 提取按行状态机，假定属性与 fn 声明
#   分行且属性以行首 `#[kani::proof` 起；单行多 harness、宏生成 harness 不识别。
# =============================================================================
set -euo pipefail

PROG=baseline_calc

usage() { sed -n '2,32p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }

CMD=${1:-}
case "$CMD" in
  gen-kani|verify-kani|gen-rocq|verify-rocq) shift ;;
  *) usage ;;
esac

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="" SRC_KANI="" SRC_ROCQ="" MAN_KANI="" MAN_ROCQ="" OUT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --root) ROOT=${2:?}; shift 2 ;;
    --src)  case "$CMD" in *-kani) SRC_KANI=${2:?};; *-rocq) SRC_ROCQ=${2:?};; esac; shift 2 ;;
    -o|--out) OUT=${2:?}; shift 2 ;;
    -m|--manifest) case "$CMD" in *-kani) MAN_KANI=${2:?};; *-rocq) MAN_ROCQ=${2:?};; esac; shift 2 ;;
    *) echo "$PROG: 未知参数: $1" >&2; usage ;;
  esac
done
[ -n "$ROOT" ] || ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
[ -n "$SRC_KANI" ] || SRC_KANI="$ROOT/proofs/Kani镜像员-INV组/harness/src"
[ -n "$SRC_ROCQ" ] || SRC_ROCQ="$ROOT/proofs/rocq-规范库-v0"
[ -n "$MAN_KANI" ] || MAN_KANI="$ROOT/ci/baselines/kani-inv-harness.manifest"
[ -n "$MAN_ROCQ" ] || MAN_ROCQ="$SRC_ROCQ/theorems-manifest.txt"

# ---------- 9b：Kani INV 镜像 harness 清单机算 ----------

# 提取 lib.rs 未被 #[cfg(...)] 门控的模块名（门控模块不计入默认 harness 面）。
kani_active_modules() { # $1 = lib.rs
  awk '
    /^#[[:space:]]*\[/ {
      if ($0 ~ /#\[cfg\(/) gated = 1
      next
    }
    /^[[:space:]]*(pub[[:space:]]+)?mod[[:space:]]+[A-Za-z0-9_]+[[:space:]]*;/ {
      if (!gated) {
        line = $0
        sub(/^[[:space:]]*(pub[[:space:]]+)?mod[[:space:]]+/, "", line)
        sub(/;.*$/, "", line)
        print line
      }
      gated = 0
      next
    }
    { gated = 0 }
  ' "$1"
}

# 提取一个 .rs 文件内 #[kani::proof] 属性后首个 fn 名（行状态机）。
kani_proof_fns() { # $1 = .rs 文件
  awk '
    /^#\[kani::proof/ { inflag = 1; next }
    inflag {
      if (match($0, /(pub[[:space:]]+)?fn[[:space:]]+[A-Za-z0-9_]+/)) {
        s = substr($0, RSTART, RLENGTH)
        sub(/^(pub[[:space:]]+)?fn[[:space:]]+/, "", s)
        print s; inflag = 0
      } else if ($0 ~ /^[[:space:]]*(#\[|\/\/)/) { next }
      else { inflag = 0 }
    }
  ' "$1"
}

# 产出数据行：<模块文件>\t<模块>::<harness>（按文件名、名字稳定排序）。
kani_data_lines() {
  local lib="$SRC_KANI/lib.rs" mod f
  [ -f "$lib" ] || { echo "$PROG: 缺 $lib" >&2; return 2; }
  local -a mods=()
  while IFS= read -r mod; do
    f="$SRC_KANI/$mod.rs"
    [ -f "$f" ] || { echo "$PROG: lib.rs 声明模块无文件: $f" >&2; return 2; }
    mods+=("$mod")
  done < <(kani_active_modules "$lib")
  for mod in "${mods[@]}"; do
    f="$SRC_KANI/$mod.rs"
    while IFS= read -r fn; do
      [ -n "$fn" ] && printf '%s.rs\t%s::%s\n' "$mod" "$mod" "$fn"
    done < <(kani_proof_fns "$f")
  done | LC_ALL=C sort -t$'\t' -k1,1 -k2,2
}

kani_names_sha256() { # 数据行名列 → 排序去重 → sha256
  kani_data_lines | cut -f2 | LC_ALL=C sort -u | sha256sum | cut -d' ' -f1
}

gen_kani() {
  local out=${OUT:-/dev/stdout}
  local count; count=$(kani_data_lines | cut -f2 | LC_ALL=C sort -u | wc -l | tr -d ' ')
  local sha; sha=$(kani_names_sha256)
  {
    printf '# ci/baselines/kani-inv-harness.manifest — Kani INV 镜像 harness 清单正本\n'
    printf '# 生成: bash ci/baseline_calc.sh gen-kani -o ci/baselines/kani-inv-harness.manifest\n'
    printf '# 源:   proofs/Kani镜像员-INV组/harness/src（#[cfg(feature=..)] 门控模块不计）\n'
    printf '# 纪律: 有意增删 harness 须同批再生成本正本并同步 numbers.json\n'
    printf '#   （fv.kani_inv_harness.count / .manifest_sha256）——防悄悄改基线=逃逸。\n'
    printf '# 数据行: <模块文件>\\t<模块>::<harness>；count=去重 harness 总数；\n'
    printf '# names-sha256=名字清单（sort -u）SHA-256——run.sh 9b 以此替手写常数 32。\n'
    kani_data_lines
    printf 'count\t%s\n' "$count"
    printf 'names-sha256\t%s\n' "$sha"
  } > "$out"
  echo "$PROG: gen-kani → ${out}（count=$count, names-sha256=${sha:0:12}…）" >&2
}

verify_kani() {
  [ -f "$MAN_KANI" ] || { echo "$PROG: 9b FAIL: 基线正本缺席（$MAN_KANI）" >&2; return 1; }
  local man_data now_data man_count now_count man_sha now_sha
  man_data=$(grep -v '^#' "$MAN_KANI" | awk -F'\t' 'NF==2 && $1!="count" && $1!="names-sha256"' | LC_ALL=C sort)
  now_data=$(kani_data_lines | LC_ALL=C sort)
  man_count=$(awk -F'\t' '$1=="count"{print $2}' "$MAN_KANI")
  man_sha=$(awk -F'\t' '$1=="names-sha256"{print $2}' "$MAN_KANI")
  now_count=$(printf '%s\n' "$now_data" | cut -f2 | LC_ALL=C sort -u | wc -l | tr -d ' ')
  now_sha=$(printf '%s\n' "$now_data" | cut -f2 | LC_ALL=C sort -u | sha256sum | cut -d' ' -f1)
  if [ -z "$man_count" ] || [ -z "$man_sha" ]; then
    echo "$PROG: 9b FAIL: 基线正本缺 count/names-sha256 行（$MAN_KANI）" >&2; return 1
  fi
  if [ "$man_count" != "$now_count" ] || [ "$man_sha" != "$now_sha" ]; then
    echo "$PROG: 9b FAIL: 基线漂移——期望 count=$man_count/sha=${man_sha:0:12}…，源机算 count=$now_count/sha=${now_sha:0:12}…；有意变更须同批再生成正本（gen-kani）并登记变更依据" >&2
    return 1
  fi
  if [ "$man_data" != "$now_data" ]; then
    echo "$PROG: 9b FAIL: 基线数据行漂移（count/sha 相同但逐行不一致——查重复名）：" >&2
    diff <(printf '%s\n' "$man_data") <(printf '%s\n' "$now_data") | sed 's/^/      /' >&2 || true
    return 1
  fi
  echo "$PROG: 9b 基线对账一致: count=$now_count, names-sha256=${now_sha:0:12}…"
}

# ---------- 9a：Rocq 定理清单数据段再生成（口径与 run.sh 9a-2 逐字一致） ----------

rocq_data_lines() { # <文件>\t<定理名>，按文件名序
  local f
  (cd "$SRC_ROCQ" && for f in T*.v; do
    grep '^Theorem ' "$f" | sed "s/^Theorem \\([^ :]*\\).*/$f\\t\\1/"
  done) | LC_ALL=C sort -t$'\t' -k1,1 -k2,2
}

rocq_statements_sha256() { # 与 run.sh 9a-2 同口径：陈述块按 T*.v 文件序拼接
  (cd "$SRC_ROCQ" && for f in T*.v; do
    sed -n '/^Theorem /,/^Proof\./p' "$f" | sed '/^Proof\./d'
  done) | sha256sum | cut -d' ' -f1
}

gen_rocq() {
  local out=${OUT:-/dev/stdout}
  local sha; sha=$(rocq_statements_sha256)
  {
    printf '# theorems-manifest 数据段再生成（baseline_calc gen-rocq；正本=proofs/rocq-规范库-v0/theorems-manifest.txt）\n'
    rocq_data_lines
    printf 'statements-sha256\t%s\n' "$sha"
  } > "$out"
  echo "$PROG: gen-rocq → ${out}（statements-sha256=${sha:0:12}…）" >&2
}

verify_rocq() {
  [ -f "$MAN_ROCQ" ] || { echo "$PROG: 9a FAIL: 定理清单正本缺席（$MAN_ROCQ）" >&2; return 1; }
  local man_data now_data man_sha now_sha
  man_data=$(grep -v '^#' "$MAN_ROCQ" | awk -F'\t' 'NF>=2 && $1!="statements-sha256"' | LC_ALL=C sort)
  now_data=$(rocq_data_lines | LC_ALL=C sort)
  man_sha=$(grep '^statements-sha256' "$MAN_ROCQ" | awk '{print $2}')
  now_sha=$(rocq_statements_sha256)
  local rc=0
  if [ "$man_data" != "$now_data" ]; then
    echo "$PROG: 9a FAIL: 定理名集合漂移（正本 ↔ 源机算）：" >&2
    diff <(printf '%s\n' "$man_data") <(printf '%s\n' "$now_data") | sed 's/^/      /' >&2 || true
    rc=1
  fi
  if [ "$man_sha" != "$now_sha" ]; then
    echo "$PROG: 9a FAIL: 陈述块哈希漂移（期望 ${man_sha:0:12}… ↔ 实得 ${now_sha:0:12}…）" >&2
    rc=1
  fi
  [ $rc -eq 0 ] && echo "$PROG: 9a 清单对账一致: $(printf '%s\n' "$now_data" | wc -l | tr -d ' ') 条定理名 + statements-sha256=${now_sha:0:12}…"
  return $rc
}

case "$CMD" in
  gen-kani)    gen_kani ;;
  verify-kani) verify_kani ;;
  gen-rocq)    gen_rocq ;;
  verify-rocq) verify_rocq ;;
esac
