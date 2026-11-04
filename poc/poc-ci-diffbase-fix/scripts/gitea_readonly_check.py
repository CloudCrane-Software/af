#!/usr/bin/env python3
"""gitea 只读核验（外派任务书 §二③）：default_branch / branch_protections。

只 GET，不写。凭据从 ~/.git-credentials 用 urllib.parse 解析（主机名 netloc 段
unquote 后比对 100.64.0.6:3300）；凭据只以 sha256 指纹前 12 位出现在输出里，
明文零落盘、零打印。用法：python3 -I gitea_readonly_check.py [输出.json]
"""
import hashlib
import json
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

CRED_FILE = Path.home() / ".git-credentials"
WANT_HOST, WANT_PORT = "100.64.0.6", 3300
BASE = "http://100.64.0.6:3300/api/v1"
ENDPOINTS = [
    ("repo", "/repos/opchadmin/af"),
    ("branch_protections", "/repos/opchadmin/af/branch_protections"),
    ("branches_main", "/repos/opchadmin/af/branches/main"),
    ("branches_product_main", "/repos/opchadmin/af/branches/product%2Fmain"),
]


def find_cred():
    """按任务书口径：逐行 urllib.parse，netloc 主机段 unquote 后比对。"""
    for line in CRED_FILE.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        u = urllib.parse.urlsplit(line)
        # 凭据文件把端口 URL 编码进主机段（100.64.0.6%3a3300）——unquote 后再拆 host:port
        hostport = urllib.parse.unquote(u.hostname or "")
        host, _, port_s = hostport.rpartition(":")
        try:
            port = int(port_s) if port_s else None
        except ValueError:
            port = None
        if host == WANT_HOST and port == WANT_PORT and u.username and u.password:
            return f"{u.username}:{u.password}"
    return None


def main() -> int:
    cred = find_cred()
    if not cred:
        print("[gitea-readonly] FATAL: ~/.git-credentials 未找到 100.64.0.6:3300 条目")
        return 2
    # 凭据指纹：仅 sha256 前 12 位（证据可对拍、明文不出现在任何输出/落盘）
    fp = hashlib.sha256(cred.encode()).hexdigest()[:12]
    print(f"[gitea-readonly] 凭据指纹 sha256(cred)[:12]={fp}（明文不落盘）")
    out = {"cred_fingerprint": fp, "method": "GET（只读，零写操作）", "results": {}}
    import base64
    hdr = {"Authorization": "Basic " + base64.b64encode(cred.encode()).decode()}
    for name, path in ENDPOINTS:
        url = BASE + path
        req = urllib.request.Request(url, headers=hdr, method="GET")
        try:
            with urllib.request.urlopen(req, timeout=20) as r:
                body = r.read().decode("utf-8")
                rc = r.status
        except urllib.error.HTTPError as e:
            body, rc = e.read().decode("utf-8", "replace"), e.code
        except Exception as e:  # 网络层失败也要如实入账
            out["results"][name] = {"http_status": None, "error": repr(e)}
            print(f"[gitea-readonly] {name}: 请求失败 {e!r}")
            continue
        try:
            data = json.loads(body)
        except json.JSONDecodeError:
            data = {"_raw": body[:300]}
        rec = {"http_status": rc}
        if name == "repo":
            rec["default_branch"] = data.get("default_branch")
            rec["full_name"] = data.get("full_name")
            rec["mirror"] = data.get("mirror")
        elif name == "branch_protections":
            # gitea API：无保护规则时可能 404 或空数组——两种都如实记
            rec["count"] = len(data) if isinstance(data, list) else None
            rec["body_shape"] = type(data).__name__
            if isinstance(data, dict):
                rec["body_keys"] = sorted(data.keys())[:10]
        else:
            rec["name"] = data.get("name") if isinstance(data, dict) else None
            rec["protected"] = (data.get("protected") if isinstance(data, dict)
                                else ("404" if rc == 404 else None))
            rec["commit_sha"] = ((data.get("commit") or {}).get("id")
                                 if isinstance(data, dict) else None)
        out["results"][name] = rec
        print(f"[gitea-readonly] {name}: {json.dumps(rec, ensure_ascii=False)}")
    dest = sys.argv[1] if len(sys.argv) > 1 else "gitea-readonly.json"
    Path(dest).write_text(json.dumps(out, ensure_ascii=False, indent=2) + "\n",
                          encoding="utf-8")
    print(f"[gitea-readonly] 读数落 {dest}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
