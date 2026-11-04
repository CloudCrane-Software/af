#!/usr/bin/env bash
# =============================================================================
# T7② 门读数与零改动核 —— poc/poc-t7-2-integrator/gate_checks.sh
#
# 跑法（工作树根下）：bash poc/poc-t7-2-integrator/gate_checks.sh [af主仓路径]
#   · 门 1：cargo fmt --check --all（exit 0 要绿）
#   · 门 2：本批新增面自跑门（复核器自证：正常记录件绿 + 削弱变体逐个红）
#   · 门 3：af 主仓零改动核（status / HEAD / refs / PoC-E blob 逐字未动）
#   · 门 4：判据面零弱化（ci/ 除本批新增件外零 diff / clippy 基线 sha256 未变 /
#           crates/ 零新增 crate / ci/run.sh 未改＝未新增 step 编号）
#
# ★ cargo build/test/clippy **本批未跑**（禁改 crates/ ⇒ 产物由主线全量门复验批承担；
#   本机 14G 内存禁高并发编译）——如实登记，不写成跑过。
# =============================================================================
set -u -o pipefail

WT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
AF_MAIN="${1:-/home/pan-ding/workspace/agent黑灯工厂/af}"
cd "$WT" || exit 2
export PATH="$HOME/.cargo/bin:$PATH"
export LANG=C.UTF-8

echo "════ 门 1：cargo fmt --check --all ════"
echo "工作树：$WT"
cargo fmt --check --all
echo "exit=$?  （期望 0；本批不改 Rust ⇒ 这门必绿）"
echo

echo "════ 门 2：本批新增面自跑门（复核器自证）════"
# ★ 返修批：--tag=-r2 ⇒ 落盘 raw/selftest-readings-r2.json；
#   初版读数 raw/selftest-readings.json 与 raw/selftest-v1.py **原样保留、未改**。
python3 -I poc/poc-t7-2-integrator/selftest.py --out poc/poc-t7-2-integrator/raw --tag=-r2
echo "selftest exit=$?  （期望 0：正常记录件绿 + 削弱变体逐个红 + 真实词表 + 三方比对 + --source 绕口）"
echo
echo "-- 门 2a：复核员实测场景的独立复现（返修批新增）--"
python3 -I poc/poc-t7-2-integrator/r2_repro.py --out poc/poc-t7-2-integrator/raw
echo "r2_repro exit=$?  （期望 0：R2-REPRO-GREEN；failures=[]）"
echo
echo "-- 门 2b：复核器在真实 af 仓（真实远端 origin）上的读数 --"
echo "   [2b-1] actual：对真实远端分支 feature/poc-e-codeadapter 自算实测侧"
python3 -I ci/integrator_bind_check.py actual --base-ref origin/product/main --branch feature/poc-e-codeadapter
echo "   [2b-2] verify：真实仓上**无记录件**场景（历史合流无记录面 ⇒ 必须拒绝，不是跳过）"
python3 -I ci/integrator_bind_check.py verify --record /tmp/t7-2-no-such-record.json \
  --branch feature/poc-e-codeadapter --remote origin --base-ref origin/product/main --json
echo "   [2b-2] exit=$?  （期望非 0 / RECORD-ABSENT）"
echo

echo "════ 门 3：af 主仓零改动核 ════"
echo "-- git status --porcelain（主仓）--"
git -C "$AF_MAIN" status --porcelain
echo "   ↑ 期望：只有开工前既存的 untracked（redteam/scans/2026-11-04-e4484537/）"
echo "-- HEAD --"
echo "   主仓 HEAD = $(git -C "$AF_MAIN" rev-parse HEAD)  （开工时读数 56ff52a57c022983d74d994fd4e1c63e98f45235）"
echo "-- refs --"
BRANCH_TOTAL=$(git -C "$AF_MAIN" for-each-ref --format='%(refname)' refs/heads | wc -l)
echo "   主仓本地分支数 = $BRANCH_TOTAL"
echo "   ★ 返修批改写（证据可复核性）：初版写「开工时 93 ⇒ 只多本批这一条 = 94」但"
echo "     「开工前 93」**无可独立复核的快照** ⇒ 现改为「当前 $BRANCH_TOTAL，本批仅新增 1 条」，"
echo "     不再引用无来源的「开工前 93」。"
echo "   本批新建分支   = refs/heads/feat/t7-2-integrator-bind-check → $(git -C "$AF_MAIN" rev-parse refs/heads/feat/t7-2-integrator-bind-check)（= origin/product/main 56ff52a，未 commit）"
echo "   product/main    = $(git -C "$AF_MAIN" rev-parse refs/heads/product/main)  （禁改：与开工时同值）"
echo "   main（对照线）  = $(git -C "$AF_MAIN" rev-parse refs/heads/main)  （只读归档线，未动）"
echo "   远端 product/main = $(git -C "$AF_MAIN" ls-remote origin refs/heads/product/main | cut -f1)（禁 push：与本地同值 = 未推任何东西）"
echo "-- PoC-E 参考件 blob 逐字未动 --"
POCE_BLOB_IN_ORIGIN=$(git -C "$AF_MAIN" rev-parse origin/product/main:poc/poc-e-codeadapter/codeadapter.py)
POCE_BLOB_IN_WORKTREE=$(git -C "$AF_MAIN" hash-object poc/poc-e-codeadapter/codeadapter.py)
echo "   origin/product/main 的 blob = $POCE_BLOB_IN_ORIGIN"
echo "   主仓工作树的 blob           = $POCE_BLOB_IN_WORKTREE"
[ "$POCE_BLOB_IN_ORIGIN" = "$POCE_BLOB_IN_WORKTREE" ] && echo "   ⇒ 逐字未动 ✓" || echo "   ⇒ ★ 变了（红）"
echo

echo "════ 门 4：判据面零弱化 ════"
echo "-- ci/ 下相对 origin/product/main 的 diff（只应多出本批新增件）--"
git diff --stat origin/product/main -- ci/
echo "-- ci/run.sh 是否被改（改了就可能是新增 step 编号）--"
git diff --stat origin/product/main -- ci/run.sh
echo "   ↑ 空 = 未改（未新增 step 编号）"
echo "-- ci/clippy-baseline.txt sha256 --"
echo "   origin/product/main 版 = $(git show origin/product/main:ci/clippy-baseline.txt | sha256sum | cut -d' ' -f1)"
echo "   本工作树版             = $(sha256sum ci/clippy-baseline.txt | cut -d' ' -f1)"
[ "$(git show origin/product/main:ci/clippy-baseline.txt | sha256sum | cut -d' ' -f1)" = "$(sha256sum ci/clippy-baseline.txt | cut -d' ' -f1)" ] \
  && echo "   ⇒ 未变 ✓" || echo "   ⇒ ★ 变了（红）"
echo "-- crates/ 相对 origin/product/main 的 diff --"
git diff --stat origin/product/main -- crates/
echo "   ↑ 空 = crates/ 零改动"
echo "   crate 数：origin = $(git ls-tree --name-only origin/product/main crates/ | wc -l) / 本工作树 = $(git ls-tree --name-only HEAD crates/ | wc -l)（同一提交 ⇒ 同一数）"
echo "   新增 crate 检查：$(git diff --name-status origin/product/main -- crates/ | wc -l) 条差异"
echo "-- 本工作树新增的未跟踪文件（本批新增面）--"
git status --porcelain
echo
echo "-- 门 4b：SOP §一.3 词表 vs 代码 STEP_STATUS_ALLOWED **逐字一致**（返修批新增）--"
python3 - <<'PY'
import re, pathlib
code = pathlib.Path("ci/integrator_bind_check.py").read_text(encoding="utf-8")
m = re.search(r"STEP_STATUS_ALLOWED = frozenset\(\{(.*?)\}\)", code, re.S)
code_words = set(re.findall(r'"([^"]+)"', m.group(1)))
sop = pathlib.Path("docs/INTEGRATOR-SOP.md").read_text(encoding="utf-8")
missing = [w for w in sorted(code_words) if f"`{w}`" not in sop]
print("   代码 STEP_STATUS_ALLOWED =", sorted(code_words))
print("   SOP §一.3 未逐字列出的词 =", missing or "（无）")
print("   ⇒ ", "逐字一致 ✓" if not missing else "★ 不一致（红）")
PY
echo
echo "════ 门 5（未跑项，如实登记）════"
echo "· cargo build / cargo test / cargo clippy：**未跑**。"
echo "  理由：本批硬约束禁改 crates/ ⇒ 这三项的产物由主线全量门复验批承担（anolis 在跑，"
echo "  tip 56ff52a）；且本机 14G 内存禁高并发编译，不在本地跑重编译。"
echo "  ⇒ 不写成「已跑」；本批新增面（Python）由门 2 自跑门覆盖。"
