#!/usr/bin/env python3
"""gitea API 助手（F1 端到端验证用）。

凭据纪律：运行时用 urllib.parse 解析 ~/.git-credentials 取 basic auth，
凭据本身绝不写入任何文件/日志/提交（本文件不含任何秘密材料）。
注意：~/.git-credentials 里主机名是 URL 编码的（<内网地址>%3a3300），
按字面正则匹配会取空——必须走 urllib.parse + unquote。

用法：
  gitea_api.py GET  /api/v1/repos/opchadmin/af/actions/runs
  gitea_api.py POST /api/v1/repos/opchadmin/af/pulls '{"base":...,...}'
"""
import base64
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request

GITEA_BASE = "http://<内网地址>:3300"
EXPECT_HOST = "<内网地址>"
EXPECT_PORT = 3300


def load_basic_auth():
    """从 ~/.git-credentials 解析出 (username, password)。只匹配 host/port，不打印。"""
    path = os.path.expanduser("~/.git-credentials")
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            u = urllib.parse.urlsplit(line)
            host_raw = u.netloc.rsplit("@", 1)[-1]  # 去掉 userinfo
            host_unq = urllib.parse.unquote(host_raw)  # <内网地址>%3a3300 -> <内网地址>:3300
            h = urllib.parse.urlsplit(f"{u.scheme}://{host_unq}")
            if h.hostname == EXPECT_HOST and h.port == EXPECT_PORT:
                userinfo = u.netloc.rsplit("@", 1)[0]
                user, _, pw = userinfo.partition(":")
                return urllib.parse.unquote(user), urllib.parse.unquote(pw)
    raise SystemExit(f"~/.git-credentials 里没找到 {EXPECT_HOST}:{EXPECT_PORT} 的凭据条目")


def api(method, path, body=None):
    url = path if path.startswith("http") else GITEA_BASE + path
    user, pw = load_basic_auth()
    token = base64.b64encode(f"{user}:{pw}".encode()).decode()
    req = urllib.request.Request(url, method=method.upper())
    req.add_header("Authorization", f"Basic {token}")
    data = None
    if body is not None:
        data = json.dumps(body, ensure_ascii=False).encode()
        req.add_header("Content-Type", "application/json")
    req.add_header("Accept", "application/json")
    try:
        with urllib.request.urlopen(req, data=data, timeout=30) as r:
            raw = r.read().decode()
            return r.status, (json.loads(raw) if raw.strip() else {})
    except urllib.error.HTTPError as e:
        raw = e.read().decode()
        try:
            payload = json.loads(raw) if raw.strip() else {}
        except json.JSONDecodeError:
            payload = {"raw": raw[:2000]}
        return e.code, payload


def main():
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    method, path = sys.argv[1], sys.argv[2]
    body = json.loads(sys.argv[3]) if len(sys.argv) > 3 else None
    status, payload = api(method, path, body)
    print(f"HTTP {status}")
    print(json.dumps(payload, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
