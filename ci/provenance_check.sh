#!/usr/bin/env bash
# =============================================================================
# ci/provenance_check.sh — step3 pre-step 溯源门（G0-CI机制批 子代理B 草案）
#
# 缺陷回顾（红队 W3-C-25；复盘 §三 C8 / §五 5.1）：
#   共享 CARGO_TARGET_DIR 跨工作树时，cargo 对 workspace 成员的 fingerprint
#   local 表为空，freshness 退化为「输出 mtime vs 源 mtime」——从本树跑
#   cargo test 会原样执行另一棵工作树构建的测试二进制并报 exit 0（假绿）。
#   W3-C-25 实证：共享 target 内 exec_contract-*.d 的 CARGO_MANIFEST_DIR
#   env-dep 全部指向 af-wt-rt-af-order；2026-10-27 晚 SPEC 亲测
#   ~/afbuild-phase2i 66 处 env-dep 其中 64 处指向外来树 af-wt-f6v，
#   2026-10-28 本批施工员以本树 root 只读复跑复核：raw 67 处中 64 处指向
#   af-wt-f6v（去重后 9 类外来 manifest 目录，含 af-wt-phase2-execline 的
#   af-compiler——对本树同样属外来）——「外来产物比其源树活得久」在盘实证。
#
# 本门 = crates/af-order/scripts/af-order-target-guard.sh（W3-C-25 已落域级
# 守卫，check/ensure/record 三段语义）的 CI 九门层泛化：域守卫护单包验证
# 路径，本门护 ci/run.sh step3 全 workspace 面（复盘 C8-3①「共享 target 先
# 一次性 clean，CI step3 前加 pre-step」）。
#
# 契约：
#   provenance_check.sh check  --root DIR [--target DIR]
#       只读扫描 target 内 deps/*.d 的 CARGO_MANIFEST_DIR env-dep 与守卫章；
#       来源非「本工作树 / CARGO_HOME(registry) / rustc sysroot」即外来，
#       逐件点名 exit 1。
#   provenance_check.sh ensure --root DIR [--target DIR]
#       check 判脏 → 清空 target 构建产物（等价 cargo clean）→ 写守卫章 →
#       复检。拒清危险路径（/、$HOME、--root 本身、CARGO_HOME、sysroot）exit 2。
#   provenance_check.sh record --root DIR [--target DIR] --out FILE \
#                   [--test-log FILE]
#       产出 provenance 三要素 JSON（worktree_root / 源码 commit /
#       manifest_dirs 集合 + 测试汇总），并刷新守卫章——绿记录无三要素
#       不得作为结案证据（复盘 C8-1）。
# 退出码: 0 干净/成功；1 发现外来污染（check）或清后复检仍脏（ensure）；
#         2 用法/环境错误。
# =============================================================================
set -euo pipefail

PROG=provenance_check
STAMP_NAME=ci-step3-provenance-stamp.json

usage() { sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }

CMD=${1:-}
case "$CMD" in
  check|ensure|record) shift ;;
  *) usage ;;
esac

ROOT='' TARGET='' OUT='' TESTLOG=''
while [ $# -gt 0 ]; do
  case "$1" in
    --root) ROOT=${2:?--root 需要值}; shift 2 ;;
    --target) TARGET=${2:?--target 需要值}; shift 2 ;;
    --out) OUT=${2:?--out 需要值}; shift 2 ;;
    --test-log) TESTLOG=${2:?--test-log 需要值}; shift 2 ;;
    *) echo "$PROG: 未知参数: $1" >&2; usage ;;
  esac
done
[ -n "$ROOT" ] || { echo "$PROG: 缺 --root" >&2; exit 2; }
ROOT="$(cd "$ROOT" && pwd -P)" || exit 2
[ -n "$TARGET" ] || TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"

# ---------- 工具（语义与 af-order-target-guard.sh 对齐） ----------

under() { [[ "$2" == "$1" || "$2" == "$1"/* ]]; }   # VALUE 是否位于 PREFIX 之下

cargo_home_dir() { printf '%s' "${CARGO_HOME:-$HOME/.cargo}"; }
sysroot_dir() { rustc --print sysroot 2>/dev/null || true; }

json_escape() {
  local s=$1
  s=${s//\\/\\\\}; s=${s//\"/\\\"}; s=${s//$'\t'/\\t}; s=${s//$'\r'/\\r}; s=${s//$'\n'/\\n}
  printf '%s' "$s"
}

# 收集 target 内全部 deps/*.d 记录的 CARGO_MANIFEST_DIR（原值不去重）。
# 路径含空白属病态场景，按空白截断并如实记录该局限（承域守卫同款声明）。
collect_manifest_dirs() {
  local f val
  while IFS= read -r -d '' f; do
    val=$(grep -h -o 'CARGO_MANIFEST_DIR=[^[:space:]]*' "$f" 2>/dev/null |
      cut -d= -f2- || true)
    [ -n "$val" ] && printf '%s\n' "$val"
  done < <(find "$TARGET" -type f -name '*.d' -path '*/deps/*' -print0 2>/dev/null)
  return 0
}

# 扫描外来来源；每条一行诊断到 stderr；输出（stdout）= 外来 env-dep 去重清单。
# $1=sysroot（可为空）。
scan_foreign() {
  local sysroot=$1 val ch; ch="$(cargo_home_dir)"
  local -a foreign=()
  while IFS= read -r val; do
    if under "$ROOT" "$val"; then continue; fi
    if under "$ch" "$val"; then continue; fi
    if [ -n "$sysroot" ] && under "$sysroot" "$val"; then continue; fi
    foreign+=("$val")
    echo "$PROG: 外来产物来源: CARGO_MANIFEST_DIR=$val（非本树 $ROOT / CARGO_HOME $ch / sysroot）" >&2
  done < <(collect_manifest_dirs | LC_ALL=C sort -u)
  if [ ${#foreign[@]} -gt 0 ]; then printf '%s\n' "${foreign[@]}"; fi
  return 0
}

# 守卫章：上次通过本门的树根。他树盖章即红（覆盖无 env-dep 行的库产物）。
stamp_path() { printf '%s/%s' "$TARGET" "$STAMP_NAME"; }

stamp_root() {
  local sp; sp="$(stamp_path)"
  [ -f "$sp" ] || return 0
  grep -o '"worktree_root"[[:space:]]*:[[:space:]]*"[^"]*"' "$sp" 2>/dev/null |
    sed 's/.*"\([^"]*\)"$/\1/'
}

write_stamp() {
  # 注：`$(git ... || printf unknown)` 形态在无提交仓库会串成 "HEAD\nunknown"
  # （git 失败仍先吐 stdout）——显式接力，失败即整体置 unknown。
  local commit
  commit=$(git -C "$ROOT" rev-parse HEAD 2>/dev/null) || commit=unknown
  [ -n "$commit" ] || commit=unknown
  printf '{"tool":"ci/provenance_check.sh","worktree_root":"%s","git_commit":"%s","generated_at":"%s"}\n' \
    "$(json_escape "$(printf '%s' "$ROOT")")" "$(json_escape "$commit")" \
    "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" > "$(stamp_path)" 2>/dev/null || true
}

# ---------- check ----------

do_check() { # 退出码 0/1
  local sysroot; sysroot="$(sysroot_dir)"
  local findings=0 stamp
  if [ -d "$TARGET" ]; then
    while IFS= read -r _; do findings=$((findings+1)); done < <(scan_foreign "$sysroot")
  fi
  stamp="$(stamp_root)"
  if [ -n "$stamp" ] && [ "$stamp" != "$ROOT" ]; then
    echo "$PROG: 守卫章异树——target 上次验证树根为 $stamp，当前树 $ROOT（跨树复用铁证）" >&2
    findings=$((findings+1))
  fi
  if [ "$findings" -gt 0 ]; then
    echo "$PROG: check FAIL: 共享 target 存在外来产物（$findings 类）——先 clean 再跑，否则绿属于别的分支（W3-C-25）" >&2
    return 1
  fi
  echo "$PROG: check OK: target=$TARGET 无外来来源（本树=$ROOT）"
}

# ---------- ensure ----------

refuse_dangerous_clean() { # $1=TARGET 实路径
  local t; t="$(cd "$1" 2>/dev/null && pwd -P)" || { echo "$PROG: target 不存在，无需清理" >&2; return 9; }
  local ch; ch="$(cargo_home_dir)"; local sr; sr="$(sysroot_dir)"
  if [ "$t" = "/" ] || [ "$t" = "$HOME" ] || [ "$t" = "$ROOT" ] || \
     under "$ch" "$t" || { [ -n "$sr" ] && under "$sr" "$t"; }; then
    echo "$PROG: 拒清危险路径: $t（/、\$HOME、工作树根、CARGO_HOME、sysroot 一律不清）" >&2
    return 2
  fi
  return 0
}

do_ensure() {
  # 危险路径**先判**（fail-closed）：SPEC-B1 ⑤.4 要求 --target 解析为 /、$HOME、
  # 工作树根、CARGO_HOME、sysroot 之一时一律 exit 2 且 target 未被改动——与
  # check 判脏与否无关（原实现把拒清放在 check 判脏之后，干净 target 上的危险
  # 路径会直接放行盖章，实测复现：--target $HOME 在无外来件时 exit 0）。
  # `|| rrc=$?` 而非裸调用：set -e 下非零返回会直接掀翻脚本（实测 exit=9）。
  local rrc=0
  refuse_dangerous_clean "$TARGET" || rrc=$?
  if [ "$rrc" -eq 9 ]; then          # target 不存在=干净
    mkdir -p "$TARGET" 2>/dev/null || true
    write_stamp; return 0
  fi
  if [ "$rrc" -ne 0 ]; then exit 2; fi
  if do_check; then write_stamp; return 0; fi
  echo "$PROG: ensure: 判脏 → 清理 target 构建产物（等价 cargo clean）……" >&2
  rm -rf "$TARGET"/debug "$TARGET"/release \
         "$TARGET"/.rustc_info.json "$TARGET"/.cargo-lock 2>/dev/null || true
  if ! do_check; then
    echo "$PROG: ensure FAIL: 清理后复检仍脏（文件系统异常？）" >&2
    return 1
  fi
  write_stamp
  echo "$PROG: ensure OK: target 已清并盖章（本树=$ROOT）——此后他树复用即红"
}

# ---------- record ----------

do_record() {
  [ -n "$OUT" ] || { echo "$PROG: record 需要 --out" >&2; exit 2; }
  local commit
  commit=$(git -C "$ROOT" rev-parse HEAD 2>/dev/null) || commit=unknown
  [ -n "$commit" ] || commit=unknown
  local dirs_json='' n=0 d
  while IFS= read -r d; do
    [ -n "$d" ] || continue
    [ $n -gt 0 ] && dirs_json="$dirs_json, "
    dirs_json="$dirs_json\"$(json_escape "$d")\""
    n=$((n+1))
  done < <(collect_manifest_dirs | LC_ALL=C sort -u)
  local tests_json='"not_provided"'
  if [ -n "$TESTLOG" ] && [ -f "$TESTLOG" ]; then
    tests_json=$(python3 - "$TESTLOG" <<'PYEOF'
import re, sys
passed = failed = 0
for line in open(sys.argv[1], errors='replace'):
    m = re.search(r'test result: .*?(\d+) passed', line)
    if m: passed += int(m.group(1))
    m = re.search(r'test result: .*?(\d+) failed', line)
    if m: failed += int(m.group(1))
print('{"passed":%d,"failed":%d}' % (passed, failed))
PYEOF
    ) || tests_json='"parse_error"'
  fi
  mkdir -p "$(dirname "$OUT")"
  printf '{"tool":"ci/provenance_check.sh","worktree_root":"%s","git_commit":"%s","cargo_target_dir":"%s","manifest_dirs_count":%d,"manifest_dirs":[%s],"foreign_env_deps":%s,"tests":%s,"generated_at":"%s"}\n' \
    "$(json_escape "$ROOT")" "$(json_escape "$commit")" "$(json_escape "$TARGET")" \
    "$n" "$dirs_json" '"see_check_log"' "$tests_json" \
    "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" > "$OUT"
  write_stamp
  echo "$PROG: record OK: $OUT（manifest_dirs=$n；三要素=worktree_root/commit/manifest_dirs）"
}

case "$CMD" in
  check)  do_check ;;
  ensure) do_ensure ;;
  record) do_record ;;
esac
