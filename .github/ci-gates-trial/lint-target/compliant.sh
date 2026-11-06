#!/usr/bin/env bash
# A-17 四规则合规样板（真绿面）：
set -eu
umask 077
BASE="$(pwd)/stage"
DEST="$BASE/in"
mkdir -p "$DEST"
REAL="$(realpath -m "$DEST")"
case "$REAL" in "$BASE"/*) ;; *) echo "FAIL prefix" >&2; exit 2 ;; esac
STATE="$BASE/state"
mkdir -p "$STATE"
TOKEN_FILE="$STATE/agent.token"
printf 'demo-material\n' > "$TOKEN_FILE"
chmod 600 "$TOKEN_FILE"
unshare -n bash -c 'ip link show lo'
echo "ok"
