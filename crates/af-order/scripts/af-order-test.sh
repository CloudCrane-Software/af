#!/usr/bin/env bash
# af-order 包级测试包装（红队 W3-C-25）——一命令拿到「属于本工作树」的绿色。
#
#   usage: af-order-test.sh [--dry-run] [cargo test 透传参数...]
#
# 职责：
#   1. target 隔离根因治本：未显式指定 CARGO_TARGET_DIR 时按工作树派生独立
#      目录 ~/afbuild-<工作树名>-af-order——不同工作树不再共用同一 target；
#   2. 复用防线下沉到运行前：先跑 af-order-target-guard.sh ensure，发现跨树
#      污染即清理重建（切换工作树时禁止复用共享测试产物）；
#   3. 验收记录三要素：跑完即 record——依赖文件中的 CARGO_MANIFEST_DIR 集合、
#      源码提交 commit、测试名单，落 <target>/af-order-evidence/ 下留档，
#      防止把跨树二进制当成当前树证据。
# 退出码：透传 cargo test 的退出码；环境错误为 2。
set -euo pipefail

DRY=0
ARGS=()
for a in "$@"; do
  case "$a" in
    --dry-run) DRY=1 ;;
    *) ARGS+=("$a") ;;
  esac
done

ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || {
  echo "af-order-test: 当前目录不在 git 工作树内" >&2
  exit 2
}
EXPLICIT=${CARGO_TARGET_DIR:-}
TARGET=${CARGO_TARGET_DIR:-$HOME/afbuild-$(basename "$ROOT")-af-order}
export CARGO_TARGET_DIR=$TARGET

if [ -n "$EXPLICIT" ]; then
  echo "[af-order-test] CARGO_TARGET_DIR: $TARGET（调用方显式指定；ensure 仍查跨树污染）"
else
  echo "[af-order-test] CARGO_TARGET_DIR: $TARGET（按工作树隔离派生，W3-C-25）"
fi
echo "[af-order-test] 工作树根: $ROOT"

GUARD="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/af-order-target-guard.sh"

if [ "$DRY" -eq 1 ]; then
  # 只读展示：target 判定结果（可能 exit 1，仅提示不处置）。
  "$GUARD" check --root "$ROOT" --target "$TARGET" || true
  echo "[af-order-test] dry-run：未执行 ensure/cargo/record"
  exit 0
fi

# 运行前清线：外来产物零容忍（清理重建）。
"$GUARD" ensure --root "$ROOT" --target "$TARGET"

rc=0
cargo test -p af-order ${ARGS[@]+"${ARGS[@]}"} || rc=$?

# 验收记录（测试名单取 cargo test -- --list，失败也尽力留档）。
LIST=$(mktemp)
trap 'rm -f "$LIST"' EXIT
cargo test -p af-order -- --list > "$LIST" 2>/dev/null || true

EVID=${AF_ORDER_EVIDENCE_DIR:-$TARGET/af-order-evidence}
mkdir -p "$EVID"
OUT="$EVID/af-order-provenance-$(date -u +%Y%m%dT%H%M%SZ)-$$.json"
"$GUARD" record --root "$ROOT" --target "$TARGET" --test-list-file "$LIST" --out "$OUT" \
  || echo "[af-order-test] record 失败（不影响测试结论，但证据链缺失）" >&2

echo "[af-order-test] 测试退出码: $rc"
echo "[af-order-test] 验收记录（三要素）: $OUT"
exit "$rc"
