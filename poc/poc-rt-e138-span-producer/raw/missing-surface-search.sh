#!/usr/bin/env bash
# RT-E138 阻断核证：任务书所述「现役面」存在性全机搜索（可重跑）。
# 用法：bash poc/poc-rt-e138-span-producer/raw/missing-surface-search.sh > <输出文件> 2>&1
# 每项给：命令、命中清单（空=不存在）、退出码。
set -u
WT="$(cd "$(dirname "$0")/../../.." && pwd)"   # 本工作树根

run() {
  echo "\$ $*"
  "$@"
  echo "rc=$?"
  echo "---"
}

echo "== RT-E138 现役面存在性搜索 $(date -Is) =="
echo "== 全盘（排除 /proc /sys）文件/目录名搜索 =="
for name in otlp-assert.py all-gates.sh gate-pins.list otel-production.json factory-keepalive.py factory-watch.py deploy-consistency-probe.sh 技术债登记.md 剩余欠账总表.md; do
  echo "\$ find / -name '$name' -not -path '/proc/*' -not -path '/sys/*'"
  find / -name "$name" -not -path "/proc/*" -not -path "/sys/*" 2>/dev/null
  echo "rc=$? (空输出=零命中)"
  echo "---"
done
echo "\$ find / -type d -name '状态观测' -not -path '/proc/*' -not -path '/sys/*'"
find / -type d -name "状态观测" -not -path "/proc/*" -not -path "/sys/*" 2>/dev/null
echo "rc=$? (空输出=零命中)"
echo "---"

echo "== 本仓（worktree HEAD）与全部本地/远端分支历史搜索 =="
cd "$WT" || exit 1
run git log --all --oneline --diff-filter=A -- '*otlp-assert*'
run git log --all --oneline --diff-filter=A -- '*all-gates*'
run git log --all --oneline --diff-filter=A -- '*gate-pins*'
run git ls-tree -r origin/product/main --name-only | grep -E '^(tools|状态观测)/'
run git status --short
run git rev-parse HEAD

echo "== gitea 服务端仓清单（凭据不落输出；API 树搜索）=="
run python3 - <<'PYEOF'
import urllib.parse, subprocess, json
creds = {}
for line in open('/home/pan-ding/.git-credentials'):
    line = line.strip()
    if not line:
        continue
    p = urllib.parse.urlparse(line)
    host = urllib.parse.unquote(p.hostname) if p.hostname else ''
    key = f"{host}:{p.port}" if p.port else host
    creds[key] = (urllib.parse.unquote(p.username), urllib.parse.unquote(p.password or ''))
u, p = [v for k, v in creds.items() if '100.64.0.6' in k][0]
base = 'http://100.64.0.6:3300'
out = subprocess.run(['curl', '-s', '-u', f'{u}:{p}', f'{base}/api/v1/repos/search?limit=50'],
                     capture_output=True, text=True, timeout=30)
repos = [r['full_name'] for r in json.loads(out.stdout).get('data', [])]
print('repos:', repos)
for repo in repos:
    t = subprocess.run(['curl', '-s', '-u', f'{u}:{p}',
                        f'{base}/api/v1/repos/{repo}/git/trees/HEAD?recursive=true'],
                       capture_output=True, text=True, timeout=30)
    try:
        names = [e['path'] for e in json.loads(t.stdout).get('tree', [])]
    except Exception:
        names = []
    hits = [n for n in names if any(k in n for k in ('otlp', 'all-gates', 'gate-pins', '状态观测', '欠账'))]
    print(f'{repo}: files={len(names)} otel/gate-hits={hits}')
PYEOF

echo "== 任务书引用的欠账总表唯一同名目录现状 =="
run ls -la /home/pan-ding/workspace/agent黑灯工厂/30-工件/
run ls -la /home/pan-ding/workspace/agent黑灯工厂/
echo "== 搜索完毕 =="
