#!/usr/bin/env bash
# af-order target 溯源守卫（红队 W3-C-25）。
#
# 缺陷回顾：共享 CARGO_TARGET_DIR 跨工作树复用时，cargo 对 workspace 成员的
# fingerprint local 表为空，freshness 退化为「输出 mtime vs 源 mtime」比较，
# 不校验内容与路径——从主线树跑 cargo test 会原样执行另一棵树构建的测试
# 二进制并报 exit 0（可复现的假绿：~/afbuild-phase2rt-af-order 曾存着修复
# 分支工作树的 exec_contract 测试二进制，其 deps/*.d 的
# CARGO_MANIFEST_DIR 指向 af-wt-rt-af-order）。
#
# 守卫契约（fixHint 落地）：
#   check   —— 只读盘点 target 内全部 deps/*.d 的 CARGO_MANIFEST_DIR env-dep，
#              非「当前工作树根 / CARGO_HOME(registry) / rustc sysroot」来源
#              即判外来；另查守卫章（record/ensure 写下的上次验证树）。发现
#              跨树污染 exit 1 并逐件点名。
#   ensure  —— check 判脏则清理重建：清空 target 构建产物（等价 cargo
#              clean，保留 cargo tmp），并写下当前树的守卫章——此后任何
#              其他树再来 check 立即红。
#   record  —— 产出验收记录 JSON（三要素：依赖文件中的 CARGO_MANIFEST_DIR
#              集合、源码提交 commit、测试名单），同时刷新守卫章，防止把
#              跨树二进制当成当前树证据。
#
# 用法:
#   af-order-target-guard.sh check  --root DIR --target DIR
#   af-order-target-guard.sh ensure --root DIR --target DIR
#   af-order-target-guard.sh record --root DIR --target DIR --out FILE \
#                                   [--test-list-file FILE]
# 退出码: 0 干净 / 1 发现跨树污染 / 2 用法或环境错误。
set -euo pipefail

PROG=af-order-target-guard
STAMP_NAME=af-order-guard-provenance.json

usage() {
  sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'
  exit 2
}

CMD=${1:-}
case "$CMD" in
  check|ensure|record) shift ;;
  *) usage ;;
esac

ROOT='' TARGET='' OUT='' TEST_LIST_FILE=''
while [ $# -gt 0 ]; do
  case "$1" in
    --root) ROOT=${2:?--root 需要值}; shift 2 ;;
    --target) TARGET=${2:?--target 需要值}; shift 2 ;;
    --out) OUT=${2:?--out 需要值}; shift 2 ;;
    --test-list-file) TEST_LIST_FILE=${2:?--test-list-file 需要值}; shift 2 ;;
    *) echo "$PROG: 未知参数: $1" >&2; usage ;;
  esac
done
[ -n "$TARGET" ] || usage
if [ -z "$ROOT" ]; then
  ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || {
    echo "$PROG: 不在 git 工作树内且未给 --root" >&2
    exit 2
  }
fi

# ---------- 工具 ----------

json_escape() {
  local s=$1
  s=${s//\\/\\\\}
  s=${s//\"/\\\"}
  s=${s//$'\t'/\\t}
  s=${s//$'\r'/\\r}
  s=${s//$'\n'/\\n}
  printf '%s' "$s"
}

# under PREFIX VALUE: VALUE 是否位于 PREFIX 之下（或相等）。
under() { [[ "$2" == "$1" || "$2" == "$1"/* ]]; }

cargo_home_dir() { printf '%s' "${CARGO_HOME:-$HOME/.cargo}"; }

sysroot_dir() { rustc --print sysroot 2>/dev/null || true; }

stamp_path() { printf '%s/%s' "$TARGET" "$STAMP_NAME"; }

# 收集 target 内全部 deps/*.d 记录的 CARGO_MANIFEST_DIR（原值，不去重）。
# 路径含空白属病态场景，本守卫按空白截断并如实记录该局限。
collect_manifest_dirs() {
  local f val
  while IFS= read -r -d '' f; do
    val=$(grep -h -o 'CARGO_MANIFEST_DIR=[^[:space:]]*' "$f" 2>/dev/null |
      cut -d= -f2- || true)
    [ -n "$val" ] && printf '%s\n' "$val"
  done < <(find "$TARGET" -type f -name '*.d' -path '*/deps/*' -print0 2>/dev/null)
  return 0
}

# 扫描外来产物；每条打一行诊断；FINDINGS 全局累加。$1=sysroot（可为空）。
scan_artifacts() {
  local sysroot=$1 f v val
  while IFS= read -r -d '' f; do
    val=$(grep -h -o 'CARGO_MANIFEST_DIR=[^[:space:]]*' "$f" 2>/dev/null |
      cut -d= -f2- || true)
    [ -n "$val" ] || continue
    while IFS= read -r v; do
      [ -z "$v" ] && continue
      under "$ROOT" "$v" && continue
      under "$(cargo_home_dir)" "$v" && continue
      if [ -n "$sysroot" ] && under "$sysroot" "$v"; then continue; fi
      echo "[$PROG] 外来产物: ${f#"$TARGET"/} → CARGO_MANIFEST_DIR=$v（非当前工作树/registry/sysroot）"
      FINDINGS=$((FINDINGS + 1))
    done <<< "$val"
  done < <(find "$TARGET" -type f -name '*.d' -path '*/deps/*' -print0 2>/dev/null)
}

# 守卫章：record/ensure 写下的「上次验证树」。异树即判污染（覆盖无 env-dep
# 行的库产物——它们自身不带 CARGO_MANIFEST_DIR）。
check_stamp() {
  local sp recorded
  sp=$(stamp_path)
  [ -f "$sp" ] || return 0
  recorded=$(grep -o '"worktree_root": *"[^"]*"' "$sp" 2>/dev/null |
    head -1 | sed 's/.*"worktree_root": *"//; s/"$//') || true
  if [ -n "$recorded" ] && [ "$recorded" != "$ROOT" ]; then
    echo "[$PROG] 外来章: target 上次由另一工作树验证/清理: $recorded"
    FINDINGS=$((FINDINGS + 1))
  fi
}

# 生成验收记录 JSON。$1=测试名单文件（可空）。
build_record_json() {
  local tlf=${1:-}
  local commit branch dirs
  commit=$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)
  branch=$(git -C "$ROOT" rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)
  dirs=$(collect_manifest_dirs | sort -u)
  local j first d line
  j="{
  \"generated_at\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",
  \"worktree_root\": \"$(json_escape "$ROOT")\",
  \"branch\": \"$(json_escape "$branch")\",
  \"commit\": \"$(json_escape "$commit")\",
  \"cargo_target_dir\": \"$(json_escape "$TARGET")\",
  \"manifest_dirs\": ["
  first=1
  while IFS= read -r d; do
    [ -z "$d" ] && continue
    if [ "$first" -eq 1 ]; then first=0; else j+=","; fi
    j+=$'\n   '"\"$(json_escape "$d")\""
  done <<< "$dirs"
  j+=$'\n  ],
  "test_list": ['
  first=1
  if [ -n "$tlf" ] && [ -f "$tlf" ]; then
    while IFS= read -r line || [ -n "$line" ]; do
      [ -z "$line" ] && continue
      if [ "$first" -eq 1 ]; then first=0; else j+=","; fi
      j+=$'\n   '"\"$(json_escape "$line")\""
    done < "$tlf"
  fi
  j+=$'\n  ]
}'
  printf '%s' "$j"
}

# ---------- 子命令 ----------

case "$CMD" in
  check)
    FINDINGS=0
    echo "[$PROG] check: root=$ROOT target=$TARGET"
    SR=$(sysroot_dir)
    check_stamp
    scan_artifacts "$SR"
    if [ "$FINDINGS" -eq 0 ]; then
      echo "[$PROG] PROVENANCE OK: 外来产物 0（可复用；验收仍须 record 留三要素）"
      exit 0
    fi
    echo "[$PROG] 发现 $FINDINGS 处跨树污染——共享 target 里的绿色可能不属于本树（W3-C-25）"
    echo "[$PROG] 处置: $0 ensure --root \"$ROOT\" --target \"$TARGET\"（清理重建），或改用本工作树专用 CARGO_TARGET_DIR"
    exit 1
    ;;

  ensure)
    # 危险 target 拒绝清理。
    if [ -z "$TARGET" ] || [ "$TARGET" = "/" ] || [ "$TARGET" = "$HOME" ] ||
      [ "$TARGET" = "$ROOT" ] || [ "$TARGET" = "$(cargo_home_dir)" ]; then
      echo "[$PROG] 拒绝清理危险/非法 target: $TARGET" >&2
      exit 2
    fi
    FINDINGS=0
    SR=$(sysroot_dir)
    check_stamp
    scan_artifacts "$SR"
    if [ "$FINDINGS" -eq 0 ]; then
      echo "[$PROG] ensure: 无外来产物，无需清理"
      exit 0
    fi
    echo "[$PROG] ensure: 发现 $FINDINGS 处跨树污染 → 清理重建（等价 cargo clean，保留 tmp）"
    if [ -d "$TARGET" ]; then
      find "$TARGET" -mindepth 1 -maxdepth 1 ! -name tmp -exec rm -rf -- {} +
    fi
    mkdir -p "$TARGET"
    build_record_json > "$(stamp_path)"
    echo "[$PROG] ensure: 已清理 $TARGET 并写下当前树守卫章；产物将自当前树重建"
    exit 0
    ;;

  record)
    [ -n "$OUT" ] || usage
    mkdir -p "$TARGET"
    mkdir -p "$(dirname "$OUT")"
    j=$(build_record_json "$TEST_LIST_FILE")
    printf '%s\n' "$j" > "$OUT"
    printf '%s\n' "$j" > "$(stamp_path)"
    echo "[$PROG] record: 验收记录 → $OUT（守卫章已刷新）"
    ;;
esac
