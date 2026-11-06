#!/usr/bin/env bash
# =============================================================================
# ci/gate_inputs_check.sh — 门输入全集 sha256 钉对账（红队终局轮 RT-E6/E51 修复）
#
# 缺陷回顾（逃逸问题台账 RT-E6/E51「判据正本钉扎不一致 / 门输入全集只钉 6 件」）：
#   钉扎此前是逐件决定而非族纪律——无「门输入全集清单」对账。RT-E6 首版修复只钉了
#   6 件（clippy-baseline/arm-wiring/branches/criteria/fix-scan/budget-cast-whitelist），
#   而被门实际消费的判据正本共 11 件——另 5 件（中立性三件 + 判分链登记表 + 债务登
#   记册镜像）无钉：改判据正本即改门。
#   其中 ci/scorechain-manifest.txt（判分链逐件 sha256 登记表）无钉 ⇒ 攻击者删掉表内
#   judge.rs 登记行（登记表 4 件→3 件；k5 lint 只校验「在册行逐位对锚」、不校验下界）
#   → 判分链门本体 crates/af-execline/src/judge.rs 脱离哈希锚保护面、任意改写而
#   step1 硬门照绿（2026-11-02 红队 RT-E6/E51 复现：k5 lint PASS「3 件」、
#   gate_inputs_check PASS「6 件」、mutation_runner PASS「6 探针」、xcheck 硬项
#   6/6 全绿——四门齐绿）。
#
# 本机制（复用 mutations-registry 同款模式，不占新 step 编号——挂 run.sh step0
# 预检，承 check-registry 同槽先例）：
#   ① 门输入全集清单硬编码于下方 GATE_INPUTS（11 件全集；清单增删=脚本 diff 本身，
#      走同批评审——清单即造册，钉文件不含清单外条目）；逐件 sha256 落
#      ci/baselines/gate-inputs.sha256（sha256sum 格式纯数据，update 机生成、禁手改）。
#   ② 全集自动守卫：ci/ 一级目录下所有判据类正本（.yaml/.yml/.json/.tsv/.txt/.md）
#      必须在 GATE_INPUTS 在册——确非门输入者须逐件显式登记 GATE_INPUT_EXCLUSIONS
#      （脚本 diff 即评审面，不得静默漏钉）。新增判据正本漏钉当场红（防「全集」
#      退化为陈年快照——RT-E6/E51 同型复发的根面）。
#   ③ check 对账（漂移/缺件/摘钉/钉外多行/重复钉行/漏造册一律红——fail-closed：
#      防护被关掉本身就是红，正本缺席同红）。有意变更判据正本（如 clippy_gate
#      --update 收紧基线、arm-wiring 摘格换债号、白名单放行调整）须同批
#      `bash ci/gate_inputs_check.sh update` 重钉，与判据改动同一提交过评审
#      （防悄悄改基线=逃逸，与 update-registry-baseline 同一纪律）。
#
# 契约：
#   gate_inputs_check.sh check  [--root DIR]   对账（0=一致；1=漂移/缺件/漏造册；2=用法错）
#   gate_inputs_check.sh update [--root DIR]   机生成重钉（须与判据正本改动同批提交）
# =============================================================================
set -euo pipefail

PROG=gate_inputs_check
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PIN_REL="ci/baselines/gate-inputs.sha256"
# 门输入全集清单（判据正本，相对仓根；增删=改本清单，脚本 diff 即评审面）
GATE_INPUTS=(
  # —— 中立性门判据正本（ci/neutrality_gate.sh / ci/neutrality_scan.py 消费）——
  "ci/neutrality-expected-fail.txt"    # 隔离臂失败集闭集名单（neutrality_gate.sh:42）
  "ci/neutrality-wordlist.txt"         # 词表黑名单+白名单合册（neutrality_gate.sh:43）
  "ci/neutrality-golden-registry.tsv"  # golden IR 互为闭集登记（neutrality_gate.sh:44）
  # —— 判分链登记正本（ci/k5-path-lint.py 消费）——
  "ci/scorechain-manifest.txt"         # 判分链逐件 sha256 登记表（k5-path-lint.py:17）
  # —— clippy 基线正本（ci/clippy_gate.py 消费）——
  "ci/clippy-baseline.txt"
  # —— 判定面分支/夹具/判据 schema/修复扫描/cast 白名单（各 checker 消费）——
  "ci/arm-wiring.yaml"                 # arm_wiring_check.py:41
  "ci/branches.yaml"                   # branches_check.py
  "ci/criteria.yaml"                   # criteria_check.py
  "ci/fix-scan.yaml"                   # fix_scan.py
  "ci/budget-cast-whitelist.json"      # budget_cast_lint.py:196
  # —— 债务登记册镜像（ci/arm_wiring_check.py:42 / ci/budget_cast_lint.py 消费）——
  "ci/debt-register.md"
  # —— step14 子门单点名册（红队终局轮 RT-E81 修复批，2026-11-02）——
  "ci/subgates.list"                    # step14 子门全集正本（step12 名册绑定 + xcheck 子门级断言消费）
  # —— harness 版本级回归门判据正本（D-54 s17-harness-regression-gate 批，2026-11-06）——
  "ci/baselines/harness-regression-set.json"    # 回归集定义件（模型/seed/任务集钉/双阈值/校准锚；harness_regression_gate.py 定义面消费）
  "ci/baselines/harness-regression-tasks.json"  # 120 题回归任务集（digest 另钉于定义件 task_set.sha256——双钉）
  #     注：读数台账 ci/baselines/harness-readings.jsonl 是 append-only 可变账
  #     （发布管线逐 release 追加），不入本钉面；其口径由定义件钉、逐条由门核对。
  # —— step9 基线清单正本（RT-E6 清偿批 2026-11-04 补钉：ci/baseline_calc.sh 对称
  #    管理的 9a/9b 两件此前无族钉——kani 件只有 numbers.json 值锚
  #    fv.kani_inv_harness.manifest_sha256（「摘基线+同批改账」可吸收）；rocq 件的
  #    陈述哈希行在本正本文件内=自指无外部钉。改基线清单即改 9a/9b 判据即改门）——
  "ci/baselines/kani-inv-harness.manifest"         # 9b Kani INV harness 清单正本（baseline_calc.sh verify-kani + run.sh 9b count 行）
  "proofs/rocq-规范库-v0/theorems-manifest.txt"    # 9a 定理清单正本（run.sh 9a-2 名集+陈述哈希对账 + baseline_calc.sh verify-rocq）
)
# 全集自动守卫的显式例外（逐件登记，脚本 diff 即评审面；不得静默漏钉）
GATE_INPUT_EXCLUSIONS=(
  "ci/REPORT.md"   # CI 骨架实施批报告文书——非门输入正本（无门消费）
)
# 判据类正本扩展名（全集自动守卫的判定口径）
CRITERIA_EXTS=(yaml yml json tsv txt md)

usage() { sed -n '2,/^# =\{20,\}$/p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }

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

# 全集自动守卫（②）：ci/ 一级目录下的判据类正本必须在 GATE_INPUTS 在册——
# 新增判据正本漏钉当场红（漏钉＝改判据即改门的逃逸通道；「全集」不得退化为
# 陈年快照）。确非门输入者须逐件登记 GATE_INPUT_EXCLUSIONS（脚本 diff 即评审面）。
check_registry_complete() {
  local rc=0 rel base ext p in_list excluded e is_crit
  while IFS= read -r rel; do
    [ -n "$rel" ] || continue
    base="${rel##*/}"
    ext="${base##*.}"
    [ "$ext" != "$base" ] || continue        # 无扩展名 → 非判据类正本
    is_crit=0
    for e in "${CRITERIA_EXTS[@]}"; do
      if [ "$ext" = "$e" ]; then is_crit=1; fi
    done
    if [ "$is_crit" -ne 1 ]; then continue; fi
    p="ci/$base"
    in_list=0
    for e in "${GATE_INPUTS[@]}"; do
      if [ "$p" = "$e" ]; then in_list=1; fi
    done
    if [ "$in_list" -eq 1 ]; then continue; fi
    excluded=0
    for e in "${GATE_INPUT_EXCLUSIONS[@]}"; do
      if [ "$p" = "$e" ]; then excluded=1; fi
    done
    if [ "$excluded" -eq 1 ]; then continue; fi
    echo "$PROG: 门输入全集缺口：判据正本未造册（$p）——新增门输入须同批加入 GATE_INPUTS 并 update 重钉（漏钉=改判据即改门的逃逸通道）；确非门输入者须逐件登记 GATE_INPUT_EXCLUSIONS" >&2
    rc=1
  done < <(cd "$ROOT" && find ci -maxdepth 1 -type f 2>/dev/null | LC_ALL=C sort)
  return "$rc"
}

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
  # ④ 全集自动守卫：判据类正本未造册即红（新增判据正本漏钉的逃逸通道）
  if ! check_registry_complete; then rc=1; fi
  if [ "$rc" -eq 0 ]; then
    echo "$PROG: 判据正本钉对账一致: ${#GATE_INPUTS[@]} 件（$PIN_REL）"
  fi
  return "$rc"
}

do_update() {
  # 先过全集守卫：漏造册的判据正本不钉——否则 update 会把「全集不全」写成
  # 「对账一致」（fail-closed：造册不全不得产生钉面）
  if ! check_registry_complete; then
    echo "$PROG: update 拒绝：门输入全集有缺口（见上）——先补 GATE_INPUTS 造册再重钉" >&2
    return 2
  fi
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
