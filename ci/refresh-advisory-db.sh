#!/bin/bash
# T9b: advisory-db 本地化刷新（GitHub 经 hk 代理，重试 3 次；失败保留旧库并报警）
set -e
cd "$(dirname "$0")"
export HTTPS_PROXY=${HTTPS_PROXY:-socks5h://100.64.0.3:1080}
for i in 1 2 3; do
  if [ -d advisory-db/.git ]; then
    git -C advisory-db pull --ff-only && exit 0
  else
    git clone --depth=1 https://github.com/rustsec/advisory-db advisory-db && exit 0
  fi
  sleep 5
done
echo "WARN: advisory-db 刷新失败，沿用旧库（过期风险自担）" >&2; exit 1
# 同步到 cargo-deny 默认库位（db-path 语义=克隆父目录，直读需放默认路径）
DST=~/.cargo/advisory-dbs/advisory-db
mkdir -p ~/.cargo/advisory-dbs
if [ -d "$DST" ]; then (cd "$DST" && git fetch --depth=1 origin && git reset -q --hard origin/HEAD) || rsync -a --delete ci/advisory-db/ "$DST"/; else cp -r ci/advisory-db "$DST"; fi
echo "advisory-db 已同步到 $DST"
