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
  **secret 未配置 → 该 source 一律 401（fail-closed，2026-11-03 修复批）**：
  绝不用空密钥降级验签（`or b""` 前科=空密钥 HMAC 可伪造通过，负例实证 202→已修死）；
  /healthz 对 secret 未配置的 source 报 503 not_ready（不假绿）。
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

────────── 生产副本（v0.3-T5②/T6③ 常驻批 2026-11-04）──────────
源件：poc/poc-super-agent/tools-mirror/trigger-inbox.py（PoC-D 原件，**禁改**，
sha256 25f0d00e392a0e3fc4d0456f7d87b014f0560585d51d476a806c055130da0bdf）。
本件=PoC 原件 + 以下**收窄/加固**，判定顺序与五态语义逐字未改：
 ① 绑定面收窄闸：`AF_TRIGGER_BIND` 缺省 127.0.0.1；**通配（0.0.0.0/::/空）一律
    拒起（exit 3，无豁免）**；非回环绑定须同时给 `AF_TRIGGER_BIND_ACK=<同一绑定
    地址>` 显式确认，否则拒起（fail-loud，不静默起非回环监听）。
 ② 幂等竞态锁：⑤重复判定 + ⑥落盘 同一把进程内锁（ThreadingHTTPServer 并发下
    「查存在→写」原为 TOCTOU：并发同 delivery 会双双落盘、后写覆盖前写＝违反
    「不重写」；加锁后同 delivery 并发只落一次、其余 200 duplicate）。
 ③ 原子落盘补目录 fsync（tmp→fsync→os.replace→目录 fsync）。
 ④ /healthz 增报 bind/port（只增字段；PoC 自检断言的 secrets_missing 形状不变）。
本副本与在役运行位（工作区 tools/trigger-inbox.py）**必须逐字同源**
（sha256 对账见证据）；判定行/启动/调度逻辑与 PoC 原件一致。
"""
import base64
import hashlib
import hmac
import ipaddress
import json
import os
import re
import sys
import threading
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

# ---------- 绑定面收窄闸（生产副本新增①；见文件头） ----------
BIND = os.environ.get("AF_TRIGGER_BIND", "127.0.0.1")
BIND_ACK_ENV = "AF_TRIGGER_BIND_ACK"      # 非回环绑定的显式确认闸（值=绑定地址本身）
_WILDCARD = {"", "*", "0", "0.0.0.0", "::", "[::]", "0:0:0:0:0:0:0:0"}


def _is_loopback(host):
    """回环判定：127.0.0.0/8、::1、localhost 算回环；其余（含私网 IP）不算。"""
    h = (host or "").strip()
    if h.lower() == "localhost":
        return True
    try:
        return ipaddress.ip_address(h).is_loopback
    except ValueError:
        return False


def resolve_bind():
    """绑定地址决议（fail-loud）：通配一律拒起；非回环须显式确认。返回绑定地址。

    纪律来源：T5.2 最小暴露面（portal 0.0.0.0 三公网入口前科）+ 本批
    「只许绑 127.0.0.1 / 禁通配」硬约束。非回环不是不可能（现役 Fast Path
    gov 门户→本机 8765 走 tailnet 私网 IP 且 ACL 收窄到 gov→pandev:8765 单条），
    但必须是**显式、可审计**的动作：确认值写进在役进程 environ（keepalive 可读）。
    """
    h = (BIND or "").strip()
    if h.lower() in _WILDCARD:
        log(f"绑定地址被拒（{h!r}）——通配/空绑定一律禁起（最小暴露面纪律，"
            f"无豁免通道）；请给回环地址或经裁决的非回环地址")
        sys.exit(3)
    if _is_loopback(h):
        return h
    ack = (os.environ.get(BIND_ACK_ENV) or "").strip()
    if ack != h:
        log(f"绑定地址被拒（{h}）——非回环绑定须显式确认：{BIND_ACK_ENV}={h}"
            f"（现值 {ack!r}）；确认前拒起（fail-loud，不静默起非回环监听）")
        sys.exit(3)
    log(f"绑定面确认：非回环 {h}（{BIND_ACK_ENV} 已显式确认；"
        f"暴露面收窄责任落在 ACL/上游，不由本件承担）")
    return h


_DUP_LOCK = threading.Lock()   # 幂等判定+落盘的进程内互斥（生产副本新增②）

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


def secret_env_name(source):
    """source → 环境变量名（'- '/'.'→'_'，大写）。"""
    return "AF_TRIGGER_SECRET_" + re.sub(r"[^A-Za-z0-9]", "_", source).upper()


def secret_for(source):
    """密钥解析：环境变量 AF_TRIGGER_SECRET_<SOURCE>。
    注册表只登记 key_id（秘密不落表，T1）。取不到=None——**fail-closed 真值**：
    调用方必须把 None 当拒绝（401），严禁 `or b""` 空密钥降级（2026-11-03
    修复批修死：空密钥 HMAC 可伪造通过曾实证 202 accepted + 落盘）。"""
    v = os.environ.get(secret_env_name(source))
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
    """tmp+os.replace 原子写（tmp 同目录保证原子性；event-router save_state 同款）。

    生产副本新增③：os.replace 之后补一次**目录 fsync**——rename 的持久性需要父
    目录 fsync 才落盘（掉电后目录项可能丢）；只增 fsync，不改写入语义。
    """
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(obj, f, ensure_ascii=False, indent=1)
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)
    try:
        dfd = os.open(os.path.dirname(path), os.O_RDONLY)
        try:
            os.fsync(dfd)
        finally:
            os.close(dfd)
    except OSError:
        pass


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
            # secret 未配置的 source 一律点名（fail-closed 的健康面：不假绿）
            missing = [s["name"] for s in reg.get("sources", [])
                       if secret_for(s["name"]) is None]
            ready = not missing
            self._reply(200 if ready else 503,
                        {"status": "ok" if ready else "not_ready",
                         "contract": CONTRACT,
                         "sources": [s["name"] for s in reg.get("sources", [])],
                         "secrets_missing": missing, "home": HOME,
                         # 生产副本新增④：只增字段（PoC 自检断言的形状不变）——
                         # 暴露面自证用：跑者能自报实际绑定面与端口。
                         "bind": BIND, "port": PORT})
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
        # ③ HMAC 验签（对原始 body 字节；常时比较；缺头=401 同坏签名）。
        # secret 未配置 → 该 source 一律 401（fail-closed；修复批修死 `or b""`
        # 空密钥降级——攻击者用空密钥 HMAC 伪造签名曾可 202 落盘，负例实证）。
        sec = secret_for(source)
        if sec is None:
            self._reply(401, {"error": "secret_not_configured",
                              "key_id": sdef.get("key_id"),
                              "detail": (f"源 {source} 的 secret 未配置"
                                         f"（{secret_env_name(source)}）——fail-closed 拒绝验签，"
                                         f"不降级空密钥")})
            self.close_connection = True
            return
        sig_hdr = self.headers.get("X-AF-Signature") or ""
        mac = hmac.new(sec, body_raw, hashlib.sha256).hexdigest()
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
        #    生产副本新增②：⑤⑥ 同锁——ThreadingHTTPServer 并发下「查存在→写」
        #    原为 TOCTOU（并发同 delivery 双双落盘、后写覆盖前写＝违反不重写）。
        inbox_dir = os.path.join(HOME, "inbox", source)
        path = os.path.join(inbox_dir, delivery_id + ".json")
        with _DUP_LOCK:
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
    bind = resolve_bind()          # 生产副本新增①：绑定面决议（fail-loud，通配拒起）
    srv = ThreadingHTTPServer((bind, PORT), Handler)
    no_sec = [s["name"] for s in reg.get("sources", []) if secret_for(s["name"]) is None]
    log(f"trigger-inbox 起（{bind}:{PORT}，home={HOME}，"
        f"sources={[s['name'] for s in reg.get('sources', [])]}，"
        f"body 上限 {MAX_BODY // 1024}KiB，注册表只读；"
        f"secret 未配置（该 source 一律 401，fail-closed）：{no_sec or '无'}）")
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        log("停（KeyboardInterrupt）")


if __name__ == "__main__":
    main()
