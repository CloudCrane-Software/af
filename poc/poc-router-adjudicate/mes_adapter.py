#!/usr/bin/env python3
"""mes_adapter — MES→Trigger 适配器（poc-router-adjudicate 消费腿上游接线）。

把 af-execline 驱动层落盘的 MES 正本事件（mes-events.jsonl，权威记录）中
的 complete 行包装成 af.trigger.v1 收件箱可收的 webhook body，HMAC-SHA256
签名后 POST 到 Trigger 收件箱（poc/poc-super-agent/tools-mirror/
trigger-inbox.py，契约 poc/poc-super-agent/contracts/af.trigger.v1.md）。

口径（与收件箱 envelope_for 兜底分支逐项对齐，非 git 事件）：
- event = "order_complete"（注册表 source 白名单）；
- repository.full_name = "<line_id>/<order_id>" —— thread_key/dedup_key 的
  业务对象（mes:<line>/<order>、order_complete:<line>/<order>，单粒度去重）；
- order_id / stable_id / mes_event（complete 行原样内嵌）——消费腿
  （af-order src/router.rs）从已验 body_b64 重解析这三项并与源单正本对账。

只做包装与投递，不碰 MES、不派单、不执行门套件（收件箱纪律）。
"""
import hashlib
import hmac
import http.client
import json

EVENT = "order_complete"


def load_mes_complete(mes_events_path):
    """读 MES 正本，取 complete 行（解析态原样返回；找不到=抛错，不猜）。"""
    with open(mes_events_path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            event = json.loads(line)
            if event.get("kind") == "complete":
                return event
    raise SystemExit(f"mes_adapter: MES 正本缺 complete 事件: {mes_events_path}")


def build_body(order_id, stable_id, mes_complete, line_id, delivery_id):
    """组 webhook body（消费腿的全部派单决策输入都在这里）。"""
    return {
        "event": EVENT,
        "delivery_id": delivery_id,
        "repository": {"full_name": f"{line_id}/{order_id}", "name": order_id},
        "order_id": order_id,
        "stable_id": stable_id,
        "line_id": line_id,
        "mes_event": mes_complete,
    }


def encode_body(body):
    """确定性序列化（紧凑、ensure_ascii=False）——payload_sha256 的字节面。"""
    return json.dumps(body, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def sign(secret, body_bytes):
    """HMAC-SHA256 对原始 body 字节（收件箱验签同款）。"""
    return "sha256=" + hmac.new(secret, body_bytes, hashlib.sha256).hexdigest()


def post(port, source, body_bytes, signature):
    """POST /inbox/<source>（127.0.0.1 收件箱；返回 (status, json)）。"""
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=10)
    try:
        conn.request(
            "POST", f"/inbox/{source}", body=body_bytes,
            headers={"Content-Type": "application/json",
                     "X-AF-Signature": signature})
        resp = conn.getresponse()
        return resp.status, json.loads(resp.read().decode("utf-8"))
    finally:
        conn.close()
