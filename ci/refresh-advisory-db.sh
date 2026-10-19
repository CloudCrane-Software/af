#!/bin/bash
# advisory-db 本地化刷新（T9b/F18 修复版）
# 1) GitHub 经 hk 代理更新 ci/advisory-db（重试 3 次）
# 2) 同步到 cargo-deny 默认库位（~/.cargo/advisory-dbs/advisory-db-<url哈希>），
#    该哈希目录由 cargo-deny 按默认 URL 派生——动态发现，发现不了就报错
set -e
cd "$(dirname "$0")/.."
export HTTPS_PROXY=${HTTPS_PROXY:-socks5h://100.64.0.3:1080}

ok=""
for i in 1 2 3; do
  if [ -d ci/advisory-db/.git ]; then
    git -C ci/advisory-db pull --ff-only -q && { ok=1; break; }
  else
    git clone --depth=1 -q https://github.com/rustsec/advisory-db ci/advisory-db && { ok=1; break; }
  fi
  sleep 5
done
if [ -z "$ok" ]; then
  echo "WARN: advisory-db 刷新失败，沿用旧库（过期风险自担）" >&2
fi

DST_PARENT="$HOME/.cargo/advisory-dbs"
mkdir -p "$DST_PARENT"
synced=0
for d in "$DST_PARENT"/advisory-db-*; do
  [ -d "$d/.git" ] || continue
  git -C "$d" fetch --depth=1 -q origin 2>/dev/null && git -C "$d" reset -q --hard origin/HEAD \
    || rsync -a --delete --exclude .git ci/advisory-db/ "$d"/
  synced=1
  echo "已同步 cargo-deny 库位：$d"
done
if [ "$synced" = 0 ]; then
  echo "ERROR: 未发现 cargo-deny 哈希库位（$DST_PARENT/advisory-db-*）；先跑一次 cargo deny check 让它自建，再重跑本脚本" >&2
  exit 1
fi
