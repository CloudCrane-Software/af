#!/usr/bin/env python3
"""trigger-inbox — af.trigger.v1 Trigger 收件箱（PoC-D，v0.3 §二 阶段1 / T5.2）

Fast Path 收件端：HTTP 接收 webhook → 验签/白名单/幂等 → 原子落 inbox。
**只读注册表 + 写 inbox 目录**，别无写面——不派单、不执行门套件、不碰 MES
（派单=event-router 消费腿职责；门套件=F 门单职责；webhook 只进收件箱）。
不复用 portal（portal 0.0.0.0 三公网入口前科，T5.2 最小暴露面）：默认只听 127.0.0.1。

判定顺序（验收口径，五态全覆盖）：
  体积>1MiB → 413；未知 source → 422；坏签名/缺头 → 401；
  坏消息（非 JSON/缺字段/事件类型不在白名单/delivery_id 非法）→ 422；
  重复 delivery → 200 duplicate（不重写不重派）；合法 → 202 accepted。
幂等主键=<source>/<delivery_id>（文件存在即重复；网络重试语义）。
业务去重（dedup_key）是消费腿的第二道，本工具只生成不判定。

落盘：af-state/triggers/inbox/<source>/<delivery-id>.json，tmp+os.replace 原子写
（与 event-router save_state 同款）。信封=af.trigger.v1 七字段
（event_id/thread_key/payload_ref/payload_sha256/dedup_key/source/received_at）
+ 外层 delivery_id/event_type/body（body_b64 为字节精确原始 payload）。
秘密不落注册表：source→密钥走环境变量 AF_TRIGGER_SECRET_<SOURCE>（注册表只登记
key_id，T1 Ref 化纪律）。注册表运行位=<home>/registry.json（正本在
poc/poc-super-agent/registry/trigger-registry.v1.json，安装脚本安放）。

回滚：停进程即回滚；inbox 文件留证不清理（只增不删，审计面）。
"""
import base64
import hashlib
import hmac
import json
import os
import re
import sys
import time
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

WS = "/home/pan-ding/workspace/agent黑灯工厂"
HOME = os.environ.get("AF_TRIGGER_HOME", WS + "/af-state/triggers")  # 沙箱自检整体重定向
REGISTRY_F = HOME + "/registry.json"
PORT = int(os.environ.get("AF_TRIGGER_INBOX_PORT", "8765"))
MAX_BODY = 1 * 1024 * 1024          # 1 MiB 硬顶（超限 413）
DELIVERY_RE = re.compile(r"^[A-Za-z0-9._-]{1,128}$")  # 防路径穿越/超长 id
CONTRACT = "af.trigger.v1"

_REG = None            # 注册表（只读缓存；--reload 语义不需要，进程重启即新表）
_REG_LOADED_AT = 0.0


def log(msg):
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


def load_registry(force=False):
    """只读注册表。读不到=fail-loud 退出（收件端宁可拒收不可裸奔）。"""
    global _REG, _REG_LOADED_AT
    if _REG is not None and not force:
        return _REG
    try:
        reg = json.load(open(REGISTRY_F, encoding="utf-8"))
    except FileNotFoundError:
        log(f"注册表不在位（{REGISTRY_F}）——收件箱拒绝起服（fail-loud，不猜）")
        sys.exit(2)
    except Exception as e:
        log(f"注册表损坏（{REGISTRY_F}: {type(e).__name__}: {e}）——拒绝起服")
        sys.exit(2)
    if reg.get("schema") != "af.trigger.registry.v1":
        log(f"注册表 schema 不识别: {reg.get('schema')!r}——拒绝起服")
        sys.exit(2)
    _REG, _REG_LOADED_AT = reg, time.time()
    return reg


def source_def(reg, source):
    for s in reg.get("sources", []):
        if s.get("name") == source:
            return s
    return None


def secret_for(source):
    """密钥解析：环境变量 AF_TRIGGER_SECRET_<SOURCE>（'- '→'_'，大写）。
    注册表只登记 key_id（秘密不落表，T1）。取不到=None → 该 source 一律 401。"""
    env = "AF_TRIGGER_SECRET_" + re.sub(r"[^A-Za-z0-9]", "_", source).upper()
    v = os.environ.get(env)
    return v.encode() if v else None


def _short(sha):
    return (sha or "")[:12]


def envelope_for(source_defn, source, body_raw, parsed, delivery_id, inbox_rel):
    """按 af.trigger.v1 生成信封（thread_key/dedup_key 按 §2/§3 从 payload 抽取）。"""
    payload_sha256 = hashlib.sha256(body_raw).hexdigest()
    domain = source_defn.get("domain", "unknown")
    repo_obj = parsed.get("repository") or {}
    repo = str(repo_obj.get("full_name") or repo_obj.get("name") or "unknown-repo")
    etype = parsed.get("event")
    after = str(parsed.get("after") or parsed.get("sha") or "")
    pr = parsed.get("pull_request") or {}
    if etype == "push":
        thread_key = f"{domain}:{repo}@{_short(after)}"
        dedup_key = f"push:{repo}:{after}"
        biz_obj = _short(after)
    elif etype == "pull_request":
        num = pr.get("number") or parsed.get("number") or "?"
        head = str((pr.get("head") or {}).get("sha") or parsed.get("head_sha") or "")
        thread_key = f"{domain}:{repo}#{num}"
        dedup_key = f"pull_request:{repo}:{num}:{head}"
        biz_obj = f"#{num}"
    else:  # 白名单外到不了这里（422 已挡）；防御性兜底
        thread_key = f"{domain}:{repo}"
        dedup_key = f"{etype}:{repo}"
        biz_obj = ""
    return {
        "contract": CONTRACT,
        "event_id": f"{source}-{delivery_id}",
        "thread_key": thread_key,
        "payload_ref": inbox_rel,
        "payload_sha256": payload_sha256,
        "dedup_key": dedup_key,
        "source": source,
        "received_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        # ---- 外层实现字段（非七字段契约面）----
        "delivery_id": delivery_id,
        "event_type": etype,
        "key_id": source_defn.get("key_id"),
        "hmac": source_defn.get("hmac", "sha256"),
        "biz_object": biz_obj,
        "body": parsed,                       # 解析态（消费腿便利）
        "body_b64": base64.b64encode(body_raw).decode(),  # 字节精确原始 payload
    }


def atomic_write_json(path, obj):
    """tmp+os.replace 原子写（tmp 同目录保证原子性；event-router save_state 同款）。"""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(obj, f, ensure_ascii=False, indent=1)
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)


class Handler(BaseHTTPRequestHandler):
    server_version = "af-trigger-inbox/0.1"
    protocol_version = "HTTP/1.1"

    def _reply(self, code, payload):
        body = json.dumps(payload, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, fmt, *args):  # 默认噪声改走自家 log（stderr→stdout 统一）
        log(f"http {self.address_string()} {fmt % args}")

    def do_GET(self):
        if self.path == "/healthz":
            reg = load_registry()
            self._reply(200, {"status": "ok", "contract": CONTRACT,
                              "sources": [s["name"] for s in reg.get("sources", [])],
                              "home": HOME})
        else:
            self._reply(404, {"error": "not_found"})

    def do_POST(self):
        if not self.path.startswith("/inbox/"):
            self._reply(404, {"error": "not_found"})
            self.close_connection = True
            return
        source = self.path[len("/inbox/"):].strip("/")
        if not re.fullmatch(r"[A-Za-z0-9._-]{1,64}", source):
            self._reply(422, {"error": "bad_source", "detail": "source 名非法"})
            self.close_connection = True
            return
        # ① 体积（Content-Length 声明即拒；未声明按 chunked 流式计，超限即断）
        try:
            n = int(self.headers.get("Content-Length") or "0")
        except ValueError:
            n = 0
        if n > MAX_BODY:
            self._reply(413, {"error": "payload_too_large", "limit_bytes": MAX_BODY})
            self.close_connection = True
            return
        body_raw = self._read_body_limited()
        if body_raw is None:
            self._reply(413, {"error": "payload_too_large", "limit_bytes": MAX_BODY})
            self.close_connection = True
            return
        reg = load_registry()
        sdef = source_def(reg, source)
        # ② source 白名单（注册表只读面）
        if sdef is None:
            self._reply(422, {"error": "unknown_source", "detail": f"source 不在注册表: {source}"})
            self.close_connection = True
            return
        # ③ HMAC 验签（对原始 body 字节；常时比较；缺头=401 同坏签名）
        sig_hdr = self.headers.get("X-AF-Signature") or ""
        mac = hmac.new(secret_for(source) or b"", body_raw, hashlib.sha256).hexdigest()
        want = sig_hdr[len("sha256="):] if sig_hdr.startswith("sha256=") else sig_hdr
        if not want or not hmac.compare_digest(mac.encode("ascii"), want.encode("ascii")):
            self._reply(401, {"error": "bad_signature", "key_id": sdef.get("key_id")})
            self.close_connection = True
            return
        # ④ 坏消息（非 JSON/缺件/白名单外事件类型/delivery_id 非法）
        try:
            parsed = json.loads(body_raw.decode("utf-8"))
        except Exception:
            self._reply(422, {"error": "bad_message", "detail": "body 不是合法 UTF-8 JSON"})
            self.close_connection = True
            return
        if not isinstance(parsed, dict):
            self._reply(422, {"error": "bad_message", "detail": "body 顶层必须是 JSON 对象"})
            self.close_connection = True
            return
        etype = parsed.get("event")
        if not isinstance(etype, str) or etype not in (sdef.get("events") or []):
            self._reply(422, {"error": "bad_message",
                              "detail": f"event_type 不在白名单: {etype!r}",
                              "allowed": sdef.get("events") or []})
            self.close_connection = True
            return
        delivery_id = parsed.get("delivery_id") or self.headers.get("X-AF-Delivery") or ""
        if not isinstance(delivery_id, str) or not DELIVERY_RE.fullmatch(delivery_id):
            self._reply(422, {"error": "bad_message",
                              "detail": "delivery_id 缺失或非法（^[A-Za-z0-9._-]{1,128}$）"})
            self.close_connection = True
            return
        # ⑤ 幂等：同 source/delivery 已收 → 200 duplicate（不重写不重派）
        inbox_dir = os.path.join(HOME, "inbox", source)
        path = os.path.join(inbox_dir, delivery_id + ".json")
        if os.path.exists(path):
            self._reply(200, {"status": "duplicate", "delivery_id": delivery_id,
                              "path": os.path.relpath(path, HOME)})
            return
        # ⑥ 落盘（原子）→ 202
        env = envelope_for(sdef, source, body_raw, parsed, delivery_id,
                           os.path.relpath(path, HOME))
        atomic_write_json(path, env)
        log(f"收件: {source}/{delivery_id} event={etype} thread_key={env['thread_key']}"
            f" sha={env['payload_sha256'][:12]}…")
        self._reply(202, {"status": "accepted", "delivery_id": delivery_id,
                          "path": env["payload_ref"], "thread_key": env["thread_key"],
                          "dedup_key": env["dedup_key"]})

    def _read_body_limited(self):
        """按 Content-Length 精确读；无长度头（chunked 等）拒收（413 同途）。"""
        try:
            n = int(self.headers.get("Content-Length") or "0")
        except ValueError:
            return None
        if n <= 0 or n > MAX_BODY:
            return None
        return self.rfile.read(n)


def main():
    reg = load_registry()
    os.makedirs(HOME + "/inbox", exist_ok=True)
    srv = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    log(f"trigger-inbox 起（127.0.0.1:{PORT}，home={HOME}，"
        f"sources={[s['name'] for s in reg.get('sources', [])]}，"
        f"body 上限 {MAX_BODY // 1024}KiB，注册表只读）")
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        log("停（KeyboardInterrupt）")


if __name__ == "__main__":
    main()
