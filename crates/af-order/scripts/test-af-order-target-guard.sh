#!/usr/bin/env bash
# TDD 夹具测试：af-order-target-guard.sh（W3-C-25 共享 target 跨工作树假绿机制）。
#
# 夹具在 mktemp 目录里构造两棵假工作树 + 一个假 cargo target：
#   ① 同根 .d（env-dep CARGO_MANIFEST_DIR 指向当前树）——必须放行；
#   ② 外来 .d（指向另一棵树）——check 必须抓出、ensure 必须清理；
#   ③ registry .d（指向 CARGO_HOME）——豁免，不得冤枉；
#   ④ 无 env-dep 行的库 .d——靠守卫 provenance 章判异树；
#   ⑤ record JSON 必须含三要素：worktree_root/commit/manifest_dirs + 测试名单；
#   ⑥ 包装脚本 --dry-run 必须按工作树派生独立 CARGO_TARGET_DIR（隔离根因）。
# 全程不碰真实 ~/afbuild-* 目录。
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
GUARD="$HERE/af-order-target-guard.sh"
WRAPPER="$HERE/af-order-test.sh"

pass=0 fail=0
ok()  { pass=$((pass+1)); echo "PASS: $1"; }
bad() { fail=$((fail+1)); echo "FAIL: $1"; }

if [ ! -x "$GUARD" ]; then
  echo "FAIL: 守卫脚本不存在/不可执行: $GUARD（TDD 红——尚未实现）"
  exit 1
fi

T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT

ROOTA="$T/wt-a"; ROOTB="$T/wt-b-other-tree"
mkdir -p "$ROOTA/crates/af-order" "$ROOTB/crates/af-order"
git -c init.defaultBranch=main init -q "$ROOTA"
git -C "$ROOTA" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init

TARGET="$T/target"; DEPS="$TARGET/debug/deps"; FP="$TARGET/debug/.fingerprint"
mkdir -p "$DEPS" "$FP"
CARGO_HOME_DIR="${CARGO_HOME:-$HOME/.cargo}"

# ① 同根：env-dep 指向当前树 ROOTA（同根二进制 + fingerprint 邻件）
printf 'x: y\n\n# env-dep:CARGO_MANIFEST_DIR=%s/crates/af-order\n' "$ROOTA" \
  > "$DEPS/af_order-aaaa.d"
printf 'fake bin\n' > "$DEPS/af_order-aaaa"
# ② 外来：env-dep 指向另一工作树 ROOTB（假绿机制的夹具化复刻）
printf 'x: y\n\n# env-dep:CARGO_MANIFEST_DIR=%s/crates/af-order\n' "$ROOTB" \
  > "$DEPS/exec_contract-bbbb.d"
printf 'stale foreign bin\n' > "$DEPS/exec_contract-bbbb"
mkdir -p "$FP/exec-contract-bbbb" && printf '{}' > "$FP/exec-contract-bbbb/lib.json"
# ③ registry：指向 CARGO_HOME——豁免
printf 'x: y\n\n# env-dep:CARGO_MANIFEST_DIR=%s/registry/src/index.crates.io-x/serde-1.0.0\n' \
  "$CARGO_HOME_DIR" > "$DEPS/serde-cccc.d"

# ---- 1. check：抓外来、放行同根与 registry ----
set +e
"$GUARD" check --root "$ROOTA" --target "$TARGET" > "$T/check.out" 2>&1
rc=$?
set -e
[ "$rc" -eq 1 ] && ok "check 对含外来件的 target 报 exit 1（实得 $rc）" \
               || bad "check 期望 exit 1，实得 $rc；输出：$(cat "$T/check.out")"
grep -q 'exec_contract-bbbb\.d' "$T/check.out" \
  && ok "check 点名外来 .d 文件" || bad "check 未点名 exec_contract-bbbb.d"
grep -q "$ROOTB" "$T/check.out" \
  && ok "check 点名外来工作树根" || bad "check 未点名外来根 $ROOTB"
grep -q 'af_order-aaaa\.d' "$T/check.out" \
  && bad "check 冤枉同根件 af_order-aaaa.d" || ok "check 不冤枉同根件"
grep -q 'serde-cccc\.d' "$T/check.out" \
  && bad "check 冤枉 registry 件 serde-cccc.d" || ok "check 豁免 registry 件"

# ---- 2. check：空 target / 无 deps → exit 0 ----
mkdir -p "$T/clean-target/debug/deps"
set +e
"$GUARD" check --root "$ROOTA" --target "$T/clean-target" >/dev/null 2>&1
rc=$?
set -e
[ "$rc" -eq 0 ] && ok "check 对干净 target exit 0" || bad "干净 target check 期望 0 实得 $rc"

# ---- 3. ensure：危险 target 拒绝 ----
set +e
"$GUARD" ensure --root "$ROOTA" --target / >/dev/null 2>&1; rc1=$?
"$GUARD" ensure --root "$ROOTA" --target "$HOME" >/dev/null 2>&1; rc2=$?
set -e
[ "$rc1" -ne 0 ] && ok "ensure 拒绝 / 作 target（exit $rc1）" || bad "ensure 未拒绝 /"
[ "$rc2" -ne 0 ] && ok "ensure 拒绝 HOME 作 target（exit $rc2）" || bad "ensure 未拒绝 HOME"

# ---- 4. ensure：清理重建——外来件清光，随后 check 过 ----
"$GUARD" ensure --root "$ROOTA" --target "$TARGET" > "$T/ensure.out" 2>&1
grep -q 'exec_contract-bbbb' "$DEPS/"*.d 2>/dev/null \
  && bad "ensure 后外来 .d 仍在" || ok "ensure 清掉外来 .d"
[ ! -e "$DEPS/exec_contract-bbbb" ] \
  && ok "ensure 清掉外来二进制" || bad "ensure 后外来二进制仍在"
[ ! -e "$FP/exec-contract-bbbb" ] \
  && ok "ensure 清掉外来 fingerprint" || bad "ensure 后外来 fingerprint 仍在"
set +e
"$GUARD" check --root "$ROOTA" --target "$TARGET" > "$T/check2.out" 2>&1
rc=$?
set -e
[ "$rc" -eq 0 ] && ok "ensure 后同树 check 过" || bad "ensure 后 check 期望 0 实得 $rc：$(cat "$T/check2.out")"

# ---- 5. 无 env-dep 的库 .d：靠 provenance 章判异树 ----
# ensure 全量清理后 deps/ 目录被一并移除（cargo 重建时会重新创建），先还原。
mkdir -p "$DEPS"
printf 'x: y\n' > "$DEPS/libaf_execline-dddd.d"   # 无 CARGO_MANIFEST_DIR 行
printf 'rlib\n' > "$DEPS/libaf_execline-dddd.rlib"
set +e
"$GUARD" check --root "$ROOTA" --target "$TARGET" >/dev/null 2>&1; rc_same=$?
"$GUARD" check --root "$ROOTB" --target "$TARGET" > "$T/check3.out" 2>&1; rc_foreign=$?
set -e
[ "$rc_same" -eq 0 ] && ok "有章且同树：无 env-dep 库件 check 过" || bad "同树 check 期望 0 实得 $rc_same"
[ "$rc_foreign" -eq 1 ] \
  && ok "换树后仅凭章即抓出（exit 1）" || bad "换树 check 期望 1 实得 $rc_foreign：$(cat "$T/check3.out")"

# ---- 6. record：JSON 三要素 + 测试名单 + 写章 ----
LIST="$T/list.txt"
printf 'af_order::tests::spec_parse: test\ncorrupted_voucher_reports_or2003_not_or2005: test\n' > "$LIST"
"$GUARD" record --root "$ROOTA" --target "$TARGET" --test-list-file "$LIST" \
  --out "$T/rec.json" > "$T/record.out" 2>&1
for pat in "\"worktree_root\": \"$ROOTA\"" "\"commit\": " "\"manifest_dirs\"" \
           "af_order::tests::spec_parse" "corrupted_voucher_reports_or2003_not_or2005" \
           "\"cargo_target_dir\": \"$TARGET\""; do
  grep -qF "$pat" "$T/rec.json" && ok "record 含 $pat" || bad "record 缺 $pat；JSON：$(cat "$T/rec.json")"
done
grep -q '"worktree_root"' "$TARGET/af-order-guard-provenance.json" \
  && ok "record 写下守卫章" || bad "record 未写章"
# record 换树后写章 → 原树 check 应抓出
"$GUARD" record --root "$ROOTB" --target "$TARGET" --out "$T/rec-b.json" >/dev/null 2>&1
set +e
"$GUARD" check --root "$ROOTA" --target "$TARGET" >/dev/null 2>&1; rc=$?
set -e
[ "$rc" -eq 1 ] && ok "record 异树章后，原树 check 抓出（exit 1）" || bad "原树 check 期望 1 实得 $rc"

# ---- 7. 包装脚本 --dry-run：按工作树派生独立 target，无副作用 ----
if [ ! -x "$WRAPPER" ]; then
  bad "包装脚本不存在/不可执行: $WRAPPER"
else
  out=$(cd "$ROOTA" && env -u CARGO_TARGET_DIR "$WRAPPER" --dry-run 2>&1) \
    && ok "wrapper dry-run exit 0" || bad "wrapper dry-run 失败：$out"
  grep -q "afbuild-wt-a-af-order" <<<"$out" \
    && ok "wrapper 缺省按工作树派生独立 target（afbuild-wt-a-af-order）" \
    || bad "wrapper 未派生按树 target：$out"
  out2=$(cd "$ROOTA" && CARGO_TARGET_DIR="$T/explicit-target" "$WRAPPER" --dry-run 2>&1) \
    && ok "wrapper 尊重显式 CARGO_TARGET_DIR" || bad "wrapper 显式 target 失败：$out2"
  grep -q "$T/explicit-target" <<<"$out2" \
    && ok "wrapper 显式 target 透传" || bad "wrapper 未透传显式 target：$out2"
fi

echo "----"
echo "pass=$pass fail=$fail"
[ "$fail" -eq 0 ]
