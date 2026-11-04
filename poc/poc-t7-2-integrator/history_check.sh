#!/usr/bin/env bash
# =============================================================================
# T7② 历史合流核验读数（复核器自算）—— poc/poc-t7-2-integrator/history_check.sh
#
# 用途：取 af 仓近期真实合流，逐笔用**复核器自己**算实测侧的三项 git 可导键
# （base_sha / source_sha / tested_tree），并给出：
#   · 合流提交 / 两个父 / 合流时间 / 被合分支 / 该分支现在的远端头
#   · 复核器自算的实测侧读数（actual 子命令）
#   · 落地树（合并树）与合流提交实际树的关系（机算 ort 合并树 vs 真实合并提交树）
#   · suite_digest：★ 历史**无记录值**（全史无 integrator 记录件，见末节核证）；
#     这里给出的是「按本批定义在历史树上反算的重建值」——**不是**历史记录侧的值，
#     **不冒充**历史值。
#
# ★ 本脚本**不写任何记录件**（禁事后伪造记录件）：历史合流只能给读数。
#
# 用法：bash poc/poc-t7-2-integrator/history_check.sh [仓根]
# =============================================================================
set -u -o pipefail

REPO_ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
cd "$REPO_ROOT" || exit 2
CHECKER="python3 -I ci/integrator_bind_check.py"

echo "=== T7② 历史合流核验读数 ==="
echo "仓根    ：$REPO_ROOT"
echo "复核器  ：$CHECKER"
echo "git 版本：$(git --version)"
echo "产品主线：product/main = $(git rev-parse product/main)（远端 $(git ls-remote origin refs/heads/product/main | cut -f1)）"
echo

# 逐笔：<合流提交> <形态说明> <base_ref> <source_sha> <被合分支名>
#   base_ref / source 取「合流当时」的口径：
#     · 单支直并入 product/main（真合并提交）：base = 合并提交第一父（合流时主线头），
#       source = 第二父（被合分支头）
#     · 集成批快进：base = 快进起点（合流时 product/main 头），source = 集成分支头
ENTRIES=(
  "ae6869d|单支直并入 product/main（真合并提交）|P1|f14e79e|feature/poc-e-codeadapter"
  "df7c6e3|单支直并入 product/main（真合并提交）|P1|b99bbb9|feat/l3-l3-independent-port"
  "e448453|单支直并入 product/main（真合并提交）|P1|0e0e76a|fix/gate-inputs-repin"
  "83bb71c|集成批快进（product/main 由 0bb3b14 快进到集成分支头 83bb71c）|0bb3b14|83bb71c|feature/main-integration"
)

for entry in "${ENTRIES[@]}"; do
  IFS='|' read -r MERGE_SHA FORM BASE_SPEC SOURCE_SHA BRANCH <<< "$entry"
  echo "────────────── 合流 ${MERGE_SHA} ──────────────"
  git log -1 --format='合流提交 : %H%n合流时间 : %cI%n标题     : %s' "$MERGE_SHA" | head -3
  P1=$(git rev-parse "$MERGE_SHA^1")
  P2=$(git rev-parse "$MERGE_SHA^2")
  MERGE_TREE=$(git rev-parse "$MERGE_SHA^{tree}")
  if [ "$BASE_SPEC" = "P1" ]; then BASE_REF="$P1"; else BASE_REF="$BASE_SPEC"; fi
  echo "形态     : $FORM"
  echo "被合分支 : $BRANCH"
  echo "base_ref : $BASE_REF（合流时的目标分支头）"
  echo "source   : $SOURCE_SHA（合流时的被合分支头）"
  echo "合流提交树: $MERGE_TREE"
  echo "-- 该分支现在的远端头（ls-remote origin refs/heads/$BRANCH）--"
  git ls-remote origin "refs/heads/$BRANCH" | sed 's/^/    /' || true
  echo "-- 复核器自算实测侧（actual 子命令）--"
  $CHECKER actual --base-ref "$BASE_REF" --source "$SOURCE_SHA" || echo "(actual 失败 exit=$?)"
  echo "-- 落地树 vs 合流提交实际树 --"
  CALC_TREE=$($CHECKER actual --base-ref "$BASE_REF" --source "$SOURCE_SHA" 2>/dev/null \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["merge_tree"])')
  echo "机算 ort 合并树 : $CALC_TREE"
  if [ "$CALC_TREE" = "$MERGE_TREE" ]; then
    echo "比对     : ★ 逐位相同 ⇒ 该历史合流是干净的 ort 合并（无 evil merge 夹带）"
  else
    echo "比对     : 不同（机算 $CALC_TREE vs 实际 $MERGE_TREE）⇒ 该合流有冲突解法人工改动面"
  fi
  echo
done

echo "────────────── 历史记录面核证（记录件是否存在）──────────────"
echo "· 全史是否有过 integrator 相关文件（git log --all --name-only -- '*integrator*'）："
git log --all --oneline --name-only -- '*integrator*' | sed 's/^/    /' | head -10
echo "    ↑（空 = 全史无）"
echo "· 仓内现存的 gate_result / record 类跟踪件："
git ls-files | grep -iE "integrator|gate_result|gate-result" | sed 's/^/    /' | head -10
echo "    ↑（空 = 无）"
echo "· 结论：★ 历史合流**无等价记录面** —— 复核器对历史合流必然拒绝（RECORD-ABSENT）。"
echo "  本脚本据此只给真实读数，**不伪造记录件**。"
