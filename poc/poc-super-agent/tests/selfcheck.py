#!/usr/bin/env python3
"""selfcheck.py — PoC-D 收卷自检（正例链路 + 负例必红）

验收口径（任务书）：
  正例：投一条测试 webhook → 收件箱落盘 → router 检出 → F1 派单记录产生；
        再加 pull_request opened→F2（映射表第二行）；
  负例（必红）：坏签名 401 / 超限 413 / 重复 delivery 200 且不重复派单；
  附带负例：坏消息 422、非白名单事件 422、pull_request closed 收下不派单、
        业务重放（同内容换 delivery）不重复派单、篡改信封拒读、
        无订单 chat 必拒、Reviser 签 Approve 内核必拒（四层第③层）、
        桥面裸 release 必拒（release 唯一写者）；
  安灯：活性判据文件落盘（green）；阈值=0 配方下转 amber + alarms.log 摘要行；
  Slow Path：F1 派单记录 → 六步 → DecisionArtifact + Released（ReleaseGate::admit）。

隔离：全程 AF_TRIGGER_HOME=<sandbox>（收件箱/注册表/派单/游标/安灯/慢路留档全在
沙箱）；event-router 模块的 CHANGELOG 被 monkeypatch 到沙箱——**不写生产变更日志**
（首跑教训：changelog() 直写 WS/计划/变更日志.md）。STOP 熔断控制为静态验证
（生产 .router-STOP 文件属在役面，测试不得触碰）。
"""
import base64
import glob
import hashlib
import hmac
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
POC = os.path.dirname(HERE)
WS = "/home/pan-ding/workspace/agent黑灯工厂"
AF = "/home/pan-ding/workspace/af-wt-pocd"
SANDBOX = os.environ.get("POCD_SANDBOX", "/tmp/pocd-selfcheck")
HOME = os.path.join(SANDBOX, "triggers")
OBS = os.path.join(SANDBOX, "obs")
INBOX_TOOL = os.path.join(WS, "tools", "trigger-inbox.py")
ROUTER_PY = os.path.join(WS, "tools", "event-router.py")
BRIDGE = os.path.join(POC, "bridge", "target", "debug", "af-super-agent-bridge")
SECRET = "pocd-selfcheck-secret-1"
PORT = int(os.environ.get("POCD_PORT", "18899"))
BASE = f"http://127.0.0.1:{PORT}"

RESULTS = []


def check(name, ok, detail=""):
    RESULTS.append({"name": name, "ok": bool(ok), "detail": str(detail)[:300]})
    print(f"{'PASS' if ok else 'FAIL'}  {name}  {str(detail)[:160]}")
    return bool(ok)


def post(body_bytes, sig=None, path="/inbox/gitea-webhook", hdr_delivery=None):
    req = urllib.request.Request(BASE + path, data=body_bytes, method="POST")
    req.add_header("Content-Type", "application/json")
    if sig is not None:
        req.add_header("X-AF-Signature", "sha256=" + sig)
    if hdr_delivery:
        req.add_header("X-AF-Delivery", hdr_delivery)
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            return r.status, json.loads(r.read().decode())
    except urllib.error.HTTPError as e:
        try:
            return e.code, json.loads(e.read().decode())
        except Exception:
            return e.code, {}


def sign(body_bytes, secret=SECRET):
    return hmac.new(secret.encode(), body_bytes, hashlib.sha256).hexdigest()


def run_router_leg(state=None, want_amber_test=False):
    """以子进程跑 poll_triggers（env 重定向到沙箱；CHANGELOG monkeypatch 到沙箱）。
    返回 (state, stdout)。"""
    code = f"""
import importlib.util, json, sys
spec = importlib.util.spec_from_file_location("event_router", {ROUTER_PY!r})
er = importlib.util.module_from_spec(spec); spec.loader.exec_module(er)
er.CHANGELOG = {os.path.join(SANDBOX, 'changelog.md')!r}   # 不写生产变更日志
st = json.loads({json.dumps(state or {})!r})
st.setdefault("results", {{}}); st.setdefault("trigger_consumed", {{}})
st.setdefault("trigger_dedup", {{}}); st.setdefault("trigger_leg", {{}})
er.poll_triggers(st)
print("STATE_JSON_BEGIN")
print(json.dumps({{"trigger_consumed": st.get("trigger_consumed"),
                   "trigger_dedup": st.get("trigger_dedup"),
                   "trigger_leg": st.get("trigger_leg")}}, ensure_ascii=False))
"""
    env = dict(os.environ)
    env.update({"AF_TRIGGER_HOME": HOME, "AF_TRIGGER_ANDON_DIR": OBS,
                "AF_TRIGGER_ALARMS_F": os.path.join(OBS, "alarms.log"),
                "ROUTER_DRY": "0"})
    r = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True,
                       env=env, timeout=120)
    return r


def router_state(stdout):
    m = re.search(r"STATE_JSON_BEGIN\n(.*)", stdout, re.S)
    return json.loads(m.group(1)) if m else {}


def slowpath_cli(dispatch_file, *extra):
    env = dict(os.environ)
    env["AF_TRIGGER_HOME"] = HOME
    r = subprocess.run([sys.executable, os.path.join(POC, "cli", "cli.py"),
                        "decide", "--dispatch", dispatch_file,
                        "--out-root", os.path.join(HOME, "slowpath"), *extra],
                       capture_output=True, text=True, env=env, timeout=120)
    try:
        trail = json.loads(r.stdout)
    except Exception:
        trail = {}
    return r.returncode, trail


def main():
    # ---- 0) 环境 ----
    shutil.rmtree(SANDBOX, ignore_errors=True)
    os.makedirs(HOME, exist_ok=True)
    os.makedirs(OBS, exist_ok=True)
    check("S0 桥已构建", os.access(BRIDGE, os.X_OK), BRIDGE)
    src = open(ROUTER_PY, encoding="utf-8").read()
    main_i = src.index("def main():")
    stop_i = src.index("if os.path.exists(STOP):", main_i)
    while_i = src.index("while True:", main_i)
    call_i = src.index("poll_triggers(st)", while_i)      # 主循环内的调用（非 def 行）
    save_i = src.index("save_state(st)", call_i)
    check("S0 STOP 熔断控制（静态：poll_triggers 在主循环 try 内、STOP 检查之后）",
          while_i < stop_i < call_i < save_i,
          "main() while 循环：STOP 在位即 continue（跳过全部 poll 腿）→poll_triggers→save_state")

    # 注册表安装（正本→沙箱运行位）
    r = subprocess.run([sys.executable, os.path.join(HERE, "install_registry.py"),
                        os.path.join(POC, "registry", "trigger-registry.v1.json"),
                        os.path.join(HOME, "registry.json")],
                       capture_output=True, text=True, timeout=30)
    inst = json.loads(r.stdout)
    check("S1 注册表安装+recipe_digest", len(inst.get("recipe_digest", "")) == 64,
          inst.get("recipe_digest", "")[:16])

    # ---- 2) 收件箱起服 ----
    env = dict(os.environ)
    env.update({"AF_TRIGGER_HOME": HOME, "AF_TRIGGER_INBOX_PORT": str(PORT),
                "AF_TRIGGER_SECRET_GITEA_WEBHOOK": SECRET})
    srv = subprocess.Popen([sys.executable, INBOX_TOOL], env=env,
                           stdout=open(os.path.join(SANDBOX, "inbox.log"), "w"),
                           stderr=subprocess.STDOUT)
    time.sleep(1.2)
    try:
        with urllib.request.urlopen(BASE + "/healthz", timeout=5) as hr:
            check("S2 收件箱起服（127.0.0.1 最小暴露面）", hr.status == 200, hr.read().decode()[:120])

        # ---- 3) 正例：push → 202 → 落盘 ----
        push = json.dumps({"event": "push", "delivery_id": "pocd-pos-push-0001",
                           "after": "1111111111111111111111111111111111111111",
                           "repository": {"full_name": "acme/widget"}}).encode()
        code, resp = post(push, sign(push))
        check("P1 合法 push 投递 202", code == 202 and resp.get("status") == "accepted", resp)
        env_f = os.path.join(HOME, "inbox", "gitea-webhook", "pocd-pos-push-0001.json")
        envj = json.load(open(env_f)) if os.path.exists(env_f) else {}
        check("P2 收件箱原子落盘+af.trigger.v1 七字段",
              all(k in envj for k in ["event_id", "thread_key", "payload_ref",
                                      "payload_sha256", "dedup_key", "source", "received_at"]),
              f"{envj.get('thread_key')} / {envj.get('event_id')}")
        check("P2b payload_sha256 对原始字节复算",
              envj.get("payload_sha256") == hashlib.sha256(push).hexdigest())

        # pull_request opened → 202
        pr = json.dumps({"event": "pull_request", "delivery_id": "pocd-pos-pr-0002",
                         "action": "opened", "number": 7,
                         "pull_request": {"number": 7, "head": {"sha": "2222222222222222222222222222222222222222"}},
                         "repository": {"full_name": "acme/widget"}}).encode()
        code, resp = post(pr, sign(pr))
        check("P3 合法 pull_request/opened 投递 202", code == 202, resp)

        # ---- 4) 负例三条（必红） ----
        code, resp = post(push, sig="deadbeef" * 8)
        check("N1 坏签名 401（必红）", code == 401, resp)
        big = b'{"event":"push","delivery_id":"pocd-neg-big","pad":"' + b"x" * (1024 * 1024 + 64) + b'"}'
        code, resp = post(big, sign(big))
        check("N2 超限 413（必红，>1MiB）", code == 413, resp)
        mt_before = os.path.getmtime(env_f)
        code, resp = post(push, sign(push))
        mt_after = os.path.getmtime(env_f)
        check("N3 重复 delivery 200 且不重写", code == 200 and resp.get("status") == "duplicate"
              and mt_before == mt_after, f"code={code} mtime不变={mt_before == mt_after}")
        badmsg = json.dumps({"event": "cron", "delivery_id": "pocd-neg-bad-0004"}).encode()
        code, resp = post(badmsg, sign(badmsg))
        check("N4 非白名单事件 422", code == 422, resp)
        badmsg2 = b'{"event":"push","delivery_id":"***illegal***"}'
        code, resp = post(badmsg2, sign(badmsg2))
        check("N5 非法 delivery_id 422", code == 422, resp)

        # ---- 5) 消费腿：F1/F2 派单 ----
        rr = run_router_leg()
        st = router_state(rr.stdout)
        dispatch_files = sorted(glob.glob(os.path.join(HOME, "dispatch", "*.json")))
        f1 = [p for p in dispatch_files if json.load(open(p))["suite"] == "F1"]
        f2 = [p for p in dispatch_files if json.load(open(p))["suite"] == "F2"]
        check("P4 router 检出→F1 派单记录产生（push→F1）", len(f1) == 1,
              os.path.basename(f1[0]) if f1 else "无")
        check("P5 pull_request opened→F2 派单记录", len(f2) == 1,
              os.path.basename(f2[0]) if f2 else "无")
        dj = json.load(open(f1[0]))
        rd = json.load(open(os.path.join(HOME, "registry.json")))["recipe_digest"]
        expect_oid = "F1-acme-widget-" + hashlib.sha256("|".join([
            "acme/widget", "F1", "1111111111111111111111111111111111111111", rd]).encode()).hexdigest()[:16]
        check("P6 order_id 确定性=hash(repo|suite|subject|recipe_digest)",
              dj["order_id"] == expect_oid, f"{dj['order_id']} == {expect_oid}")
        consumed = [json.loads(l) for l in open(os.path.join(HOME, "consumed.jsonl"))]
        check("P7 读侧游标 consumed.jsonl 一行一事件", len(consumed) == 2
              and all(c.get("order_id") for c in consumed), f"{len(consumed)} 行")
        andon = json.load(open(os.path.join(OBS, "trigger-inbox-liveness.json")))
        check("A1 活性安灯判据文件落盘（green）", andon.get("status") == "green",
              f"{andon.get('alarm_code')} age={andon.get('newest_delivery_age_s')}s")
        # 消费幂等：再跑一轮不重复派单
        rr2 = run_router_leg(state=st)
        check("P8 消费腿防重放（第二轮零新增）",
              len(glob.glob(os.path.join(HOME, "dispatch", "*.json"))) == 2, rr2.stdout[-120:])

        # 业务重放：同 push 内容换 delivery_id → 收下但不派单
        push2 = json.dumps({"event": "push", "delivery_id": "pocd-pos-push-0001-replay",
                            "after": "1111111111111111111111111111111111111111",
                            "repository": {"full_name": "acme/widget"}}).encode()
        code, resp = post(push2, sign(push2))
        rr3 = run_router_leg(state=router_state(rr2.stdout))
        check("N6 业务重放（同内容换 delivery）不重复派单",
              code == 202 and len(glob.glob(os.path.join(HOME, "dispatch", "*.json"))) == 2,
              f"code={code}")
        # pull_request closed：收下不派单（actions 映射）
        prc = json.dumps({"event": "pull_request", "delivery_id": "pocd-neg-prc-0007",
                          "action": "closed", "number": 7,
                          "pull_request": {"number": 7, "head": {"sha": "2222222222222222222222222222222222222222"}},
                          "repository": {"full_name": "acme/widget"}}).encode()
        code, resp = post(prc, sign(prc))
        rr4 = run_router_leg(state=router_state(rr3.stdout))
        check("N7 pull_request closed 收下（202）但不派单",
              code == 202 and len(glob.glob(os.path.join(HOME, "dispatch", "*.json"))) == 2,
              f"code={code}")
        # 篡改信封：router 只读已验消息
        tamper = os.path.join(HOME, "inbox", "gitea-webhook", "pocd-evil-tamper-0008.json")
        json.dump({"contract": "af.trigger.v1", "event_type": "push",
                   "payload_sha256": "0" * 64, "body_b64": base64.b64encode(push).decode(),
                   "body": {"event": "push", "delivery_id": "pocd-evil-tamper-0008",
                            "after": "3333", "repository": {"full_name": "acme/widget"}}},
                  open(tamper, "w"))
        rr5 = run_router_leg(state=router_state(rr4.stdout))
        st5 = router_state(rr5.stdout)
        check("N8 篡改信封（sha 不符）拒读不派单",
              len(glob.glob(os.path.join(HOME, "dispatch", "*.json"))) == 2
              and any("pocd-evil-tamper" not in k for k in st5["trigger_consumed"]),
              "拒读日志见 stdout")

        # ---- 6) Slow Path 六步（F1 派单记录→DecisionArtifact→admit） ----
        rc, trail = slowpath_cli(f1[0])
        check("S1 Slow Path 六步：admitted（Released 经 ReleaseGate）",
              rc == 0 and trail.get("result") == "admitted"
              and trail["steps"]["5-admit"]["detail"]["state_after"] == "Released",
              f"rc={rc} result={trail.get('result')} "
              f"steps={list(trail.get('steps', {}))}")
        art = json.load(open(os.path.join(HOME, "slowpath", dj["order_id"],
                                          "decision-artifact.json")))["artifact"]
        check("S2 DecisionArtifact 留档（Reviser/Revise/占位签名如实）",
              art["role"] == "Reviser" and art["decision"] == "Revise"
              and art["signature"]["algorithm"] == "ed25519-placeholder",
              f"{art['role']}/{art['decision']}/{art['signature']['algorithm']}")
        check("S3 六步全用现成 API（payload 格式=内核长度前缀域）",
              trail["steps"]["2-decision-payload"]["payload"].startswith(
                  "af-workorder/decision-payload/v1|"),
              trail["steps"]["2-decision-payload"]["payload"][:60])
        # 负例：Reviser 签 Approve → 内核四层第③层必拒
        env2 = dict(os.environ); env2["AF_TRIGGER_HOME"] = HOME
        pr = subprocess.run([sys.executable, os.path.join(POC, "cli", "cli.py"), "probe",
                             "--dispatch", f1[0], "--role", "Reviser", "--decision", "Approve"],
                            capture_output=True, text=True, env=env2, timeout=120)
        ptrail = json.loads(pr.stdout) if pr.stdout.strip() else {}
        check("N9 Reviser 签 Approve 内核必拒（四层第③层，Router 永远 Reviser 的机械证据）",
              pr.returncode == 0 and ptrail.get("result") == "rejected-at-layer4"
              and "PermissionDenied" in json.dumps(
                  ptrail.get("steps", {}).get("4-validate-four-layer", {})),
              ptrail.get("result"))
        # 负例：无订单纯对话必拒
        cr = subprocess.run([sys.executable, os.path.join(POC, "cli", "cli.py"),
                             "chat", "帮我总结一下今天的工作"],
                            capture_output=True, text=True, env=env2, timeout=60)
        check("N10 无订单纯对话必拒（T6.1，rc=3）", cr.returncode == 3
              and json.loads(cr.stdout)["refused"] is True, cr.stdout[:100])
        # 负例：桥面裸 release 必拒（release 唯一写者）
        br = subprocess.run([BRIDGE, "release"], capture_output=True, text=True, timeout=30)
        check("N11 桥面裸 release 必拒（Created→Released 唯一写者=ReleaseGate::admit）",
              br.returncode == 2 and "唯一写者" in br.stderr, br.stderr.strip()[:80])

        # ---- 7) 安灯转 amber（配方阈值=0） ----
        reg = json.load(open(os.path.join(HOME, "registry.json")))
        reg["liveness"]["inbox_silent_after_min"] = 0
        json.dump(reg, open(os.path.join(HOME, "registry.json"), "w"), ensure_ascii=False)
        time.sleep(1.1)  # 让 mtime 龄超过 0min 阈
        rr6 = run_router_leg(state=st5)
        andon2 = json.load(open(os.path.join(OBS, "trigger-inbox-liveness.json")))
        alarms = [json.loads(l) for l in open(os.path.join(OBS, "alarms.log"))] \
            if os.path.exists(os.path.join(OBS, "alarms.log")) else []
        check("A2 配方阈值驱动安灯 amber+摘要行（治『建成没事件可路由』）",
              andon2.get("status") == "amber" and any(a.get("code") == "trigger-inbox-silent"
                                                      for a in alarms),
              f"{andon2.get('status')} alarms={len(alarms)}")
        # registry digest 变了（阈值改动）→ order_id 应保持不变（阈值不入 order_id 指纹？
        # 否：recipe_digest 变了→order_id 变——这是配方变更的诚实后果，非缺陷；见报告）
    finally:
        srv.send_signal(signal.SIGTERM)
        srv.wait(timeout=10)

    n_ok = sum(1 for r in RESULTS if r["ok"])
    out = {"ts": time.strftime("%Y-%m-%d %H:%M:%S"), "total": len(RESULTS),
           "passed": n_ok, "failed": len(RESULTS) - n_ok, "results": RESULTS}
    json.dump(out, open(os.path.join(SANDBOX, "selfcheck-result.json"), "w"),
              ensure_ascii=False, indent=1)
    print(f"\n==== 自检汇总: {n_ok}/{len(RESULTS)} 通过 ====")
    return 0 if n_ok == len(RESULTS) else 1


if __name__ == "__main__":
    sys.exit(main())
