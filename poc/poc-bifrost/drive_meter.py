#!/usr/bin/env python3
"""drive_meter.py — 模拟 af-meter Reading{available, recovers_at} 驱动 Bifrost 路由切换。

接线形态 A（配置热更）: meter 状态翻转时 PUT /api/providers/vllm/keys/{primary} enabled
接线形态 B（请求标记）: 每请求按 Reading 打 X-Meter-Route header，静态 CEL 规则定向 key

场景:
  1) 订阅可用打主 → 日窗耗尽切备 → recovers_at 到达切回（两种接线都跑，测切换生效时延）
  2) 在途请求: 主 delay=3s 的 3 个慢请求在途时切换，观察在途与新请求行为
  3) 故障传播: meter 认为主可用但 mock 返回 500/429/连接拒绝时的端到端行为
"""
import json
import threading
import time
import urllib.error
import urllib.request

B = "http://127.0.0.1:5160"
PRIMARY_KEY = "8555a558-9668-4cb8-94c7-09bad65fb172"   # vllm key -> :5161 subscription
SECONDARY_KEY = "ee0fea1f-c548-4169-a88d-2136879741dd"  # vllm key -> :5162 metered-api
PRIMARY_FULL = {"value": "sk-mock-sub", "name": "sub-primary",
                "vllm_key_config": {"url": "http://127.0.0.1:5161", "model_name": "mock-model"},
                "models": ["mock-model"], "weight": 1.0}
LOG = []


def log(msg):
    line = f"[{time.strftime('%H:%M:%S')}.{int(time.time()*1000)%1000:03d}] {msg}"
    LOG.append(line)
    print(line, flush=True)


def api(method, path, body=None, timeout=10):
    req = urllib.request.Request(B + path,
                                 data=json.dumps(body).encode() if body is not None else None,
                                 headers={"Content-Type": "application/json"}, method=method)
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.status, (json.loads(r.read()) if r.status != 200 else json.loads(r.read()))


def chat(header_route=None, timeout=30):
    """经 Bifrost 发一次补全, 返回 (latency_ms, route_key, http_code)"""
    t0 = time.perf_counter()
    headers = {"Content-Type": "application/json"}
    if header_route:
        headers["X-Meter-Route"] = header_route
    req = urllib.request.Request(B + "/v1/chat/completions",
        data=json.dumps({"model": "vllm/mock-model",
                         "messages": [{"role": "user", "content": "hi"}]}).encode(),
        headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            d = json.loads(r.read())
            return (time.perf_counter() - t0) * 1000, \
                d.get("extra_fields", {}).get("routing_info", {}).get("key"), r.status
    except urllib.error.HTTPError as e:
        try:
            d = json.loads(e.read())
            return (time.perf_counter() - t0) * 1000, \
                d.get("extra_fields", {}).get("routing_info", {}).get("key"), e.code
        except Exception:
            return (time.perf_counter() - t0) * 1000, None, e.code


def set_primary_enabled(enabled):
    t0 = time.perf_counter()
    api("PUT", f"/api/providers/vllm/keys/{PRIMARY_KEY}", dict(PRIMARY_FULL, enabled=enabled))
    return (time.perf_counter() - t0) * 1000


def first_request_hitting(target, limit=60, header_route=None):
    """连续发请求直到 routing_info.key == target, 返回 (第几次, 时延ms)"""
    t0 = time.perf_counter()
    for i in range(limit):
        ms, key, _ = chat(header_route=header_route)
        if key == target:
            return i + 1, (time.perf_counter() - t0) * 1000, ms
    return None, (time.perf_counter() - t0) * 1000, ms


def stats(port):
    with urllib.request.urlopen(f"http://127.0.0.1:{port}/stats", timeout=5) as r:
        return json.loads(r.read())


class MiniMeter:
    """af-meter 单日窗简化版: Window{capacity=3}; given(now)->Reading{available,recovers_at}"""
    def __init__(self, capacity=3, reset_seconds=6.0):
        self.capacity = capacity
        self.remaining = capacity
        self.reset_seconds = reset_seconds
        self.exhausted_at = None

    def given(self, now=None):
        now = now if now is not None else time.monotonic()
        if self.remaining <= 0 and self.exhausted_at is not None \
                and now - self.exhausted_at >= self.reset_seconds:
            self.remaining = self.capacity      # refresh(now): 窗口重置
            self.exhausted_at = None
        recovers_at = None
        if self.remaining <= 0 and self.exhausted_at is not None:
            recovers_at = self.exhausted_at + self.reset_seconds
        return {"available": self.remaining > 0, "remaining": self.remaining,
                "recovers_at": recovers_at}

    def try_consume(self, need=1):
        r = self.given()
        if not r["available"] or r["remaining"] < need:
            return False, r
        self.remaining -= need
        if self.remaining <= 0:
            self.exhausted_at = time.monotonic()
        return True, r


def scenario1_form_a():
    log("== 场景1A 配置热更接线: enabled 翻转 ==")
    meter = MiniMeter(capacity=3, reset_seconds=6.0)
    set_primary_enabled(True)
    log(f"初始路由分布(8发): {[chat()[1] for _ in range(8)]}")

    n = 0
    switch_events = []
    while n < 8:
        n += 1
        ok, reading = meter.try_consume()
        if ok:
            ms, key, _ = chat()
            log(f"req{n:2d} meter=SUB avail={reading['remaining']} -> gw route={key} {ms:.0f}ms")
        else:
            log(f"req{n:2d} meter=EXHAUSTED recovers_at=+{(reading['recovers_at']-time.monotonic()):.1f}s -> flip primary off")
            dt = set_primary_enabled(False)
            log(f"    PUT enabled=false took {dt:.0f}ms")
            i, total, ms = first_request_hitting("api-secondary")
            switch_events.append(("to-secondary", i, total, ms))
            log(f"    first req on secondary: attempt#{i} after {total:.0f}ms (req {ms:.0f}ms)")
            # 等到 recovers_at
            wait = reading["recovers_at"] - time.monotonic()
            if wait > 0:
                time.sleep(wait + 0.05)
            ok2, reading2 = meter.given(), None
            log(f"    meter refresh -> {meter.given()}")
            dt = set_primary_enabled(True)
            log(f"    PUT enabled=true took {dt:.0f}ms")
            i, total, ms = first_request_hitting("sub-primary")
            switch_events.append(("back-to-primary", i, total, ms))
            log(f"    first req back on primary: attempt#{i} after {total:.0f}ms (req {ms:.0f}ms)")
            break
    return switch_events


def scenario1_form_b():
    log("== 场景1B 请求标记接线: X-Meter-Route header ==")
    meter = MiniMeter(capacity=3, reset_seconds=6.0)
    for n in range(1, 9):
        reading = meter.given()
        if reading["available"]:
            ok, reading = meter.try_consume()
            route = "primary"
        else:
            route = "secondary"
        ms, key, _ = chat(header_route=route)
        expect = {"primary": "sub-primary", "secondary": "api-secondary"}[route]
        verdict = "OK" if key == expect else "MISMATCH"
        rec = f"recovers_at=+{(reading['recovers_at']-time.monotonic()):.1f}s" if reading.get("recovers_at") else ""
        log(f"req{n:2d} meter={'SUB' if route=='primary' else 'EXHAUSTED'} header={route} -> gw={key} {ms:.0f}ms {verdict} {rec}")
        if not reading.get("available") and reading.get("recovers_at"):
            wait = reading["recovers_at"] - time.monotonic()
            if 0 < wait < 10:
                time.sleep(wait + 0.05)
                log(f"    meter refresh -> {meter.given()} (切回)")


def scenario2_inflight():
    log("== 场景2 在途请求: 主 delay=3s, 3 并发在途时切备 ==")
    s_before = stats(5161)
    urllib.request.urlopen(urllib.request.Request("http://127.0.0.1:5161/control",
        data=json.dumps({"mode": "ok", "delay": 3.0}).encode(),
        headers={"Content-Type": "application/json"}), timeout=5).read()
    results = []

    def slow_req(idx):
        t0 = time.perf_counter()
        headers = {"Content-Type": "application/json"}
        if idx == 0:  # 只给第一个在途请求打 primary header（接线B视角）; 其余不带(网关自身选key)
            headers["X-Meter-Route"] = "primary"
        req = urllib.request.Request(B + "/v1/chat/completions",
            data=json.dumps({"model": "vllm/mock-model",
                             "messages": [{"role": "user", "content": "slow"}]}).encode(),
            headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                d = json.loads(r.read())
                results.append((idx, (time.perf_counter()-t0)*1000,
                                d["choices"][0]["message"]["content"],
                                d.get("extra_fields", {}).get("routing_info", {}).get("key"), r.status))
        except urllib.error.HTTPError as e:
            results.append((idx, (time.perf_counter()-t0)*1000, "ERR", None, e.code))

    threads = [threading.Thread(target=slow_req, args=(i,)) for i in range(3)]
    for t in threads: t.start()
    time.sleep(0.6)  # 让 3 个请求都到达主
    inflight = stats(5161)["inflight"]
    log(f"在途检测: primary inflight={inflight} (期望3)")
    dt = set_primary_enabled(False)
    log(f"此刻切换: PUT primary enabled=false took {dt:.0f}ms")
    i, total, ms = first_request_hitting("api-secondary", limit=30)
    log(f"切换后新请求立即在备: attempt#{i} after {total:.0f}ms (req {ms:.0f}ms)")
    for t in threads: t.join()
    s_after = stats(5161)
    log(f"在途请求结局: {results}")
    log(f"primary stats: before={s_before} after={s_after} (在途3个是否都完成在主上: "
        f"completed增量={s_after['completed']-s_before['completed']}, failed增量={s_after['failed']-s_before['failed']})")
    urllib.request.urlopen(urllib.request.Request("http://127.0.0.1:5161/control",
        data=json.dumps({"mode": "ok", "delay": 0.0}).encode(),
        headers={"Content-Type": "application/json"}), timeout=5).read()
    set_primary_enabled(True)


def scenario3_failures():
    log("== 场景3 故障传播: meter 认为主可用, 主异常 ==")
    set_primary_enabled(True)
    # 3a: 500 + max_retries=2 (当前 provider 配置)
    urllib.request.urlopen(urllib.request.Request("http://127.0.0.1:5161/control",
        data=json.dumps({"mode": "fail500"}).encode(),
        headers={"Content-Type": "application/json"}), timeout=5).read()
    s1 = stats(5161)
    ms, key, code = chat(header_route="primary")
    s2 = stats(5161)
    log(f"3a 主500(max_retries=2): {code} in {ms:.0f}ms, 主收到 {s2['received']-s1['received']} 次(同key重试), 备收到0 -> {'透传' if code==500 else 'fallback?'}")
    # 3b: 429
    urllib.request.urlopen(urllib.request.Request("http://127.0.0.1:5161/control",
        data=json.dumps({"mode": "fail429"}).encode(),
        headers={"Content-Type": "application/json"}), timeout=5).read()
    s1 = stats(5161)
    ms, key, code = chat(header_route="primary")
    s2 = stats(5161)
    log(f"3b 主429(max_retries=2): {code} in {ms:.0f}ms, 主收到 {s2['received']-s1['received']} 次")
    # 3c: 连接拒绝(主进程直接断开)
    urllib.request.urlopen(urllib.request.Request("http://127.0.0.1:5161/control",
        data=json.dumps({"mode": "drop"}).encode(),
        headers={"Content-Type": "application/json"}), timeout=5).read()
    ms, key, code = chat(header_route="primary", timeout=60)
    log(f"3c 主drop连接(max_retries=2): {code} in {ms:.0f}ms")
    # 恢复
    urllib.request.urlopen(urllib.request.Request("http://127.0.0.1:5161/control",
        data=json.dumps({"mode": "ok", "delay": 0.0}).encode(),
        headers={"Content-Type": "application/json"}), timeout=5).read()
    time.sleep(0.3)
    ms, key, code = chat(header_route="primary")
    log(f"3d 主恢复后: {code} route={key} {ms:.0f}ms")


if __name__ == "__main__":
    scenario1_form_a()
    scenario1_form_b()
    scenario2_inflight()
    scenario3_failures()
    with open("/home/pan-ding/workspace/临时工作区/temp2/af/poc/poc-bifrost/run/drive_log.txt", "w") as f:
        f.write("\n".join(LOG))
    print("LOG saved.")
