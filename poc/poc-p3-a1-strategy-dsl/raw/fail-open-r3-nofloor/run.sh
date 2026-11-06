#!/usr/bin/env bash
# 把夹具拷到仓外 /tmp，避开父 git 工作树让 R0 按「非 git 树」INFO 跳过，
# 只测 R1-R3。门脚本取自仓内正本（不另维护副本）。
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$HERE/../../../.." && pwd)"
DST=/tmp/p3-a1-r3-nofloor
rm -rf "$DST"
mkdir -p "$DST/ci" "$DST/strategies"
cp "$REPO_ROOT/ci/strategies_check.py" "$DST/ci/strategies_check.py"
cp "$HERE/strategies/st-b-C058.yaml" "$DST/strategies/"
cp "$HERE/strategies/index.yaml" "$DST/strategies/"
python3 -B "$DST/ci/strategies_check.py" --skip-compile
echo "fixture_exit=$?"
