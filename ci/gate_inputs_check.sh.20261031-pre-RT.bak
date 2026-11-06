#!/usr/bin/env bash
# =============================================================================
# ci/gate_inputs_check.sh — 判据正本哈希钉对账（红队终局轮 RT-E6 修复，2026-11-01）
#
# 缺陷回顾（逃逸问题台账 RT-E6「判据正本钉扎不一致」）：
#   mutations-registry 有外部 sha256 fail-closed 钉（ci/baselines/mutations-registry.sha256，
#   run.sh step0 check-registry 每次对账、漂移即红），而门判据正本——clippy-baseline.txt/
#   arm-wiring.yaml/branches.yaml/criteria.yaml/fix-scan.yaml/budget-cast-whitelist.json
#   ——全部无钉：改判据正本即改门（clippy 基线可预注册未来签名洗白、arm-wiring 可删格、
#   cast 白名单可放行违规），门照绿。钉扎此前是逐件决定而非族纪律，无「门输入全集」对账。
#
# 本机制（复用 mutations-registry 同款模式，最小实现；不占新 step 编号——挂 run.sh
# step0 预检，承 check-registry 同槽先例）：
#   门输入全集清单硬编码于下方 GATE_INPUTS（清单增删=脚本 diff 本身，走同批评审——
#   清单即造册，钉文件不含清单外条目）；逐件 sha256 落
#   ci/baselines/gate-inputs.sha256（sha256sum 格式纯数据，update 机生成、禁手改）。
#   check 对账（漂移/缺件/摘钉/钉外多行/重复钉行一律红——fail-closed：防护被关掉
#   本身就是红，正本缺席同红）。有意变更判据正本（如 clippy_gate --update 收紧基线、
#   arm-wiring 摘格换债号、白名单放行调整）须同批
#   `bash ci/gate_inputs_check.sh update` 重钉，与判据改动同一提交过评审
#   （防悄悄改基线=逃逸，与 update-registry-baseline 同一纪律）。
#
# 契约：
#   gate_inputs_check.sh check  [--root DIR]   对账（0=一致；1=漂移或缺件；2=用法错）
#   gate_inputs_check.sh update [--root DIR]   机生成重钉（须与判据正本改动同批提交）
# =============================================================================
set -euo pipefail

PROG=gate_inputs_check
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PIN_REL="ci/baselines/gate-inputs.sha256"
# 门输入全集清单（判据正本，相对仓根；增删=改本清单，脚本 diff 即评审面）
GATE_INPUTS=(
  "ci/clippy-baseline.txt"
  "ci/arm-wiring.yaml"
  "ci/branches.yaml"
  "ci/criteria.yaml"
  "ci/fix-scan.yaml"
  "ci/budget-cast-whitelist.json"
)

usage() { sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }

CMD=${1:-}
case "$CMD" in
  check|update) shift ;;
  *) usage ;;
esac
ROOT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --root) ROOT=${2:?}; shift 2 ;;
    *) echo "$PROG: 未知参数: $1" >&2; usage ;;
  esac
done
[ -n "$ROOT" ] || ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PIN="$ROOT/$PIN_REL"

pin_lines() { grep -vE '^(#|[[:space:]]*$)' "$PIN" | tr -d '\r'; }

do_check() {
  if [ ! -f "$PIN" ]; then
    echo "$PROG: 判据正本钉缺席（$PIN_REL）——门输入未钉，fail-closed 红" >&2
    return 1
  fi
  local rc=0 f want_sha got_sha p in_list
  # ① 逐件：正本在场 + 钉行在场 + 哈希逐位一致
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    if [ ! -f "$ROOT/$f" ]; then
      echo "$PROG: 判据正本漂移：门输入缺席（$f）——判据正本被删即门失效，fail-closed 红" >&2
      rc=1
      continue
    fi
    # cd 后按清单相对路径取哈希——哈希值与路径拼写无关（绝对/./ 前缀同值）
    want_sha=$( (cd "$ROOT" && sha256sum "$f") | cut -d' ' -f1)
    got_sha=$(pin_lines | awk -v f="$f" '$2==f{print $1}' | head -1)
    if [ -z "$got_sha" ]; then
      echo "$PROG: 判据正本漂移：钉行缺失（$f 未钉——摘钉=逃逸通道，恢复钉行或同批评审改 GATE_INPUTS 清单）" >&2
      rc=1
    elif [ "$got_sha" != "$want_sha" ]; then
      echo "$PROG: 判据正本漂移：$f 期望 sha256=${want_sha:0:12}… 实得 ${got_sha:0:12}…——判据被改须走 update 同批评审重钉" >&2
      rc=1
    fi
  done < <(printf '%s\n' "${GATE_INPUTS[@]}")
  # ② 反向：钉文件不得含清单外条目（钉面外挂/收缩均须改清单同批评审）
  while IFS= read -r p; do
    [ -n "$p" ] || continue
    in_list=0
    for f in "${GATE_INPUTS[@]}"; do [ "$p" = "$f" ] && in_list=1; done
    if [ "$in_list" -eq 0 ]; then
      echo "$PROG: 判据正本漂移：钉含清单外条目（$p）——门输入全集清单外未造册，须改 GATE_INPUTS 同批评审" >&2
      rc=1
    fi
  done < <(pin_lines | awk 'NF>=2{print $2}')
  # ③ 重复钉行（同一正本两行钉=可挑行对账的形态病）
  while IFS= read -r p; do
    [ -n "$p" ] || continue
    echo "$PROG: 判据正本漂移：重复钉行（$p）——钉文件禁手改，须 update 机生成重钉" >&2
    rc=1
  done < <(pin_lines | awk 'NF>=2{c[$2]++; if(c[$2]>1) print $2}')
  if [ "$rc" -eq 0 ]; then
    echo "$PROG: 判据正本钉对账一致: ${#GATE_INPUTS[@]} 件（$PIN_REL）"
  fi
  return "$rc"
}

do_update() {
  mkdir -p "$(dirname "$PIN")"
  local f
  : > "$PIN"
  for f in "${GATE_INPUTS[@]}"; do
    if [ ! -f "$ROOT/$f" ]; then
      rm -f "$PIN"
      echo "$PROG: update 失败：门输入缺席（$ROOT/$f）——清单与盘面不一致，先对齐再重钉" >&2
      return 2
    fi
    # 子 shell cd：钉行路径恒为仓相对形态（ci/…，承 mutations-registry.sha256 先例；
    # 也使 `cd 仓根 && sha256sum -c ci/baselines/gate-inputs.sha256` 直接可用）
    ( cd "$ROOT" && sha256sum "$f" ) >> "$PIN"
  done
  echo "$PROG: 判据正本钉已重钉: $PIN_REL（${#GATE_INPUTS[@]} 件；须与判据正本改动同批提交）"
  cat "$PIN"
}

case "$CMD" in
  check)  do_check ;;
  update) do_update ;;
esac
