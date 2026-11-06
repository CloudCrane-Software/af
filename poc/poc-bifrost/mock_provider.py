#!/usr/bin/env python3
"""mock_provider.py — OpenAI /v1/chat/completions 兼容 mock provider，带控制端点。

用法: python3 mock_provider.py --port 5161 --name subscription --price-per-call 0

HTTP 接口:
  POST /v1/chat/completions   OpenAI 格式补全; 响应 header X-Mock-Provider=<name>
  POST /control               {"mode":"ok|fail500|fail429|drop","delay":秒,"fail_after":N}
  GET  /control               当前模式
  GET  /stats                 {received, inflight, completed, failed}
"""
import argparse
import json
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LOCK = threading.Lock()
STATE = {"mode": "ok", "delay": 0.0, "fail_after": 0}
STATS = {"received": 0, "inflight": 0, "completed": 0, "failed": 0}
EVENTS = []  # (t, name, event)


def log_event(kind):
    with LOCK:
        EVENTS.append((round(time.time(), 3), NAME, kind))
        if len(EVENTS) > 20000:
            del EVENTS[:10000]


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):  # 静默默认访问日志
        pass

    def _send_json(self, code, obj, extra_headers=None):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        for k, v in (extra_headers or {}).items():
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/control":
            with LOCK:
                self._send_json(200, {"state": STATE, "stats": STATS})
        elif self.path == "/stats":
            with LOCK:
                self._send_json(200, STATS)
        elif self.path == "/v1/models":
            self._send_json(200, {"object": "list", "data": [
                {"id": MODEL, "object": "model", "owned_by": NAME}]})
        else:
            self._send_json(404, {"error": "not found"})

    def do_POST(self):
        if self.path == "/control":
            n = int(self.headers.get("Content-Length", 0))
            req = json.loads(self.rfile.read(n) or b"{}")
            with LOCK:
                STATE.update({k: v for k, v in req.items()
                              if k in ("mode", "delay", "fail_after")})
                cur = dict(STATE)
            log_event("control:" + json.dumps(req))
            self._send_json(200, {"ok": True, "state": cur})
            return
        if not self.path.startswith("/v1/chat/completions"):
            self._send_json(404, {"error": "not found"})
            return

        n = int(self.headers.get("Content-Length", 0))
        req = json.loads(self.rfile.read(n) or b"{}")
        with LOCK:
            STATS["received"] += 1
            STATS["inflight"] += 1
        log_event("req_in")
        try:
            with LOCK:
                mode, delay = STATE["mode"], STATE["delay"]
                fail_after = STATE["fail_after"]
            if fail_after > 0:  # 成功 fail_after 个请求后永久切 fail500（模拟窗口耗尽）
                with LOCK:
                    done = STATS["completed"] + STATS["failed"]
                if done >= fail_after:
                    mode = "fail500"
            if delay:
                time.sleep(delay)
            if mode in ("fail500", "fail429"):
                code = 500 if mode == "fail500" else 429
                with LOCK:
                    STATS["inflight"] -= 1
                    STATS["failed"] += 1
                log_event("req_fail:%d" % code)
                self._send_json(code, {"error": {
                    "message": f"mock {mode} from {NAME}",
                    "type": "server_error" if code == 500 else "rate_limit_error",
                    "code": mode}}, {"X-Mock-Provider": NAME})
                return
            if mode == "drop":
                with LOCK:
                    STATS["inflight"] -= 1
                    STATS["failed"] += 1
                log_event("req_drop")
                self.close_connection = True
                self.wfile.close()
                return
            with LOCK:
                STATS["inflight"] -= 1
                STATS["completed"] += 1
            log_event("req_ok")
            self._send_json(200, {
                "id": "chatcmpl-mock-%d" % (time.time_ns() % 10**9),
                "object": "chat.completion",
                "created": int(time.time()),
                "model": req.get("model", MODEL),
                "choices": [{"index": 0, "finish_reason": "stop", "message": {
                    "role": "assistant",
                    "content": f"hello from {NAME}"}}],
                "usage": {"prompt_tokens": 10, "completion_tokens": 5,
                          "total_tokens": 15},
            }, {"X-Mock-Provider": NAME})
        except (BrokenPipeError, ConnectionResetError):
            with LOCK:
                STATS["inflight"] -= 1
            log_event("client_gone")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, required=True)
    ap.add_argument("--name", required=True)
    ap.add_argument("--model", default="mock-model")
    args = ap.parse_args()
    NAME, MODEL = args.name, args.model
    srv = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    srv.daemon_threads = True
    print(f"mock provider {NAME} listening on {args.port}", flush=True)
    srv.serve_forever()
