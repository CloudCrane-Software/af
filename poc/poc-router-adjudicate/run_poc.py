#!/usr/bin/env python3
"""run_poc — poc-router-adjudicate 消费腿全链驱动（真跑一次 + 留证）。

链（事件驱动，源单不手植——消费腿输入=信封）：
  腿⓪上游  af-order CLI submit（真实 bin）：源任务单 create→…→close，
           MES 正本落 complete 事件（这一步是上游生产，不是消费腿的一部分）
  腿A收件  trigger-inbox.py 真跑（HTTP 127.0.0.1）：MES 适配器把 complete
           事件包装成 af.trigger.v1 webhook body → HMAC 签名 POST → 202，
           信封真由收件箱落盘 inbox/<source>/<delivery>.json
           （负例探针：同单重投 200 duplicate / 坏签名 401）
  腿B消费  af-order example router_consume_leg：信封验证（body_b64 自证+
           冗余字段核对+注册表重推导）→ 裁决目标从信封派生（与源单正本
           对账）→ 防重放游标 → 裁决单生成 → af-station 入队 → steward
           认领 → 真签 → admit → complete → consumed.jsonl 落账
  腿C重放  同信封再消费一次 → consumed 游标命中 → fail-closed 拒派 exit 1

证据全落本目录 run/（相对仓库根 poc/poc-router-adjudicate/run/）。
用法：python3 poc/poc-router-adjudicate/run_poc.py
（需 CARGO_TARGET_DIR 已指构建盘，如 /data/bench/af-target。）
"""
import hashlib
import json
import os
import shutil
import sqlite3
import subprocess
import sys
import time
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import mes_adapter  # noqa: E402  同目录消费腿上游适配器

POC = Path(__file__).resolve().parent
REPO = POC.parents[1]
RUN = POC / "run"
TOOLS_TRIGGER_INBOX = REPO / "poc" / "poc-super-agent" / "tools-mirror" / "trigger-inbox.py"

ORDER_ID = "wo-pocr-leg-1"
SOURCE = "mes-execline"
LINE_ID = "line-poc"
PORT = 18765
DELIVERY_ID = f"mes-{ORDER_ID}-complete-1"
# 沙箱专用固定密钥（127.0.0.1 本机收件箱 PoC 复现面；绝非生产秘密）。
SECRET = b"pocr-sandbox-secret-4f2a91c7"
TARGET_DIR = os.environ.get("CARGO_TARGET_DIR", str(REPO / "target"))


def log(stage, msg):
    print(f"[{datetime.now(timezone.utc).isoformat(timespec='seconds')}] [{stage}] {msg}",
          flush=True)


def write_json(path, obj):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(obj, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")


def sha256_of(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def fresh_run_dir():
    for sub in ("state", "trigger-home", "logs"):
        shutil.rmtree(RUN / sub, ignore_errors=True)
    for stale in ("station.db", "evidence-sha256.txt", "run-meta.json", "station-tasks.json"):
        (RUN / stale).unlink(missing_ok=True)
    RUN.mkdir(parents=True, exist_ok=True)
    (RUN / "logs").mkdir(parents=True, exist_ok=True)


def phase0_build():
    log("build", "cargo build -p af-order（bin + example）")
    subprocess.run(
        ["cargo", "build", "-p", "af-order"],
        cwd=REPO, check=True,
        env={**os.environ, "CARGO_TARGET_DIR": TARGET_DIR},
    ).check_returncode()


def phaseA_submit():
    """腿⓪上游：真实 af-order CLI 落一张闭环源单（消费腿不造单）。"""
    log("A", f"submit 上游源单 {ORDER_ID}（--unit stub，真引擎门控）")
    spec = RUN / f"{ORDER_ID}-spec.json"
    spec.write_text(json.dumps({
        "order_id": ORDER_ID,
        "line": {"line_id": LINE_ID, "line_version": "lv-1"},
        "budget": 100,
        "note": "poc-router-adjudicate 消费腿上游单",
    }, ensure_ascii=False), encoding="utf-8")
    out = subprocess.run(
        [str(Path(TARGET_DIR) / "debug" / "af-order"), "submit",
         "--spec", str(spec), "--state", str(RUN / "state"),
         "--unit", "stub", "--workspace", str(REPO), "--target-dir", TARGET_DIR],
        cwd=REPO, check=True, capture_output=True, text=True,
    ).stdout
    summary = json.loads(out)
    if summary.get("state") != "Closed":
        raise SystemExit(f"phaseA: 上游单未闭环: {summary.get('state')}")
    write_json(RUN / "logs" / "phaseA-submit.json", summary)
    mes_path = RUN / "state" / "orders" / ORDER_ID / "mes-events.jsonl"
    complete = mes_adapter.load_mes_complete(mes_path)
    log("A", f"上游单 Closed，MES complete 事件 event_id={complete['event_id']}")
    return summary


def phaseB_inbox():
    """腿A收件：trigger-inbox 真跑 + 适配器投递 + 两枚负例/幂等探针。"""
    home = RUN / "trigger-home"
    (home / "inbox").mkdir(parents=True, exist_ok=True)
    write_json(home / "registry.json", {
        "$comment": "poc-router-adjudicate 运行位注册表（af.trigger.v1 契约 §6）："
                    "只登记 MES 消费腿源；git 域源属 PoC-D event-router，与本腿无关不复制。",
        "schema": "af.trigger.registry.v1",
        "sources": [{
            "name": SOURCE, "domain": "mes",
            "key_id": "pocr-mes-key-1", "hmac": "sha256",
            "events": [mes_adapter.EVENT],
        }],
    })
    env = {**os.environ,
           "AF_TRIGGER_HOME": str(home),
           "AF_TRIGGER_INBOX_PORT": str(PORT),
           "AF_TRIGGER_SECRET_MES_EXECLINE": SECRET.decode()}
    inbox_log = (RUN / "logs" / "trigger-inbox.log").open("w", encoding="utf-8")
    log("B", f"trigger-inbox.py 起服（127.0.0.1:{PORT}，home={home}）")
    inbox = subprocess.Popen([sys.executable, str(TOOLS_TRIGGER_INBOX)],
                             stdout=inbox_log, stderr=subprocess.STDOUT, env=env)
    try:
        # 健康面：secret 已配置 → 200 ok（fail-closed 的反面证据）。
        deadline = time.time() + 15
        health = None
        while time.time() < deadline:
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{PORT}/healthz", timeout=2) as r:
                    health = (r.status, json.loads(r.read().decode()))
                break
            except Exception:
                if inbox.poll() is not None:
                    raise SystemExit("phaseB: 收件箱进程早退（看 logs/trigger-inbox.log）")
                time.sleep(0.3)
        if not health or health[0] != 200 or health[1].get("status") != "ok":
            raise SystemExit(f"phaseB: healthz 未就绪: {health}")
        write_json(RUN / "logs" / "phaseB-healthz.json", health[1])

        # 投递：MES complete 事件 → webhook body → HMAC → POST → 202。
        mes_path = RUN / "state" / "orders" / ORDER_ID / "mes-events.jsonl"
        summary = json.loads((RUN / "logs" / "phaseA-submit.json").read_text(encoding="utf-8"))
        body = mes_adapter.build_body(ORDER_ID, summary["stable_id"],
                                      mes_adapter.load_mes_complete(mes_path),
                                      LINE_ID, DELIVERY_ID)
        body_bytes = mes_adapter.encode_body(body)
        status, resp = mes_adapter.post(PORT, SOURCE, body_bytes,
                                        mes_adapter.sign(SECRET, body_bytes))
        write_json(RUN / "logs" / "phaseB-adapter-202.json",
                   {"status": status, "response": resp,
                    "payload_sha256": hashlib.sha256(body_bytes).hexdigest(),
                    "body_bytes_len": len(body_bytes)})
        if status != 202 or resp.get("status") != "accepted":
            raise SystemExit(f"phaseB: 投递未 202: {status} {resp}")

        # 探针①（delivery 幂等）：同单重投 → 200 duplicate，不落第二份。
        status_dup, resp_dup = mes_adapter.post(PORT, SOURCE, body_bytes,
                                                mes_adapter.sign(SECRET, body_bytes))
        write_json(RUN / "logs" / "phaseB-probe-duplicate-200.json",
                   {"status": status_dup, "response": resp_dup})
        # 探针②（验签 fail-closed）：坏签名 → 401，不落盘。
        status_bad, resp_bad = mes_adapter.post(
            PORT, SOURCE, body_bytes,
            mes_adapter.sign(b"wrong-secret", body_bytes))
        write_json(RUN / "logs" / "phaseB-probe-badsig-401.json",
                   {"status": status_bad, "response": resp_bad})
        envelope = home / "inbox" / SOURCE / f"{DELIVERY_ID}.json"
        inbox_files = sorted(p.name for p in (home / "inbox" / SOURCE).glob("*.json"))
        if not envelope.is_file():
            raise SystemExit(f"phaseB: 信封未落盘: {envelope}")
        if inbox_files != [f"{DELIVERY_ID}.json"]:
            raise SystemExit(f"phaseB: 收件箱文件面异常: {inbox_files}")
        env_obj = json.loads(envelope.read_text(encoding="utf-8"))
        for probe_name, probe in [("duplicate", (status_dup, resp_dup, 200, "duplicate")),
                                  ("badsig", (status_bad, resp_bad, 401, None))]:
            status, resp, want_status, want_state = probe
            if status != want_status or (want_state and resp.get("status") != want_state):
                raise SystemExit(f"phaseB: 探针 {probe_name} 未达预期 {want_status}: {status} {resp}")
        log("B", f"信封真由收件箱落盘: {envelope.relative_to(RUN)}（{len(inbox_files)} 文件）")
        return envelope, env_obj
    finally:
        inbox.terminate()
        inbox.wait(timeout=10)
        inbox_log.close()


def phaseC_consume(envelope):
    """腿B消费：example router_consume_leg 全链（stdout 腿证据逐行 JSON）。"""
    args = [str(Path(TARGET_DIR) / "debug" / "examples" / "router_consume_leg"),
            "--envelope", str(envelope),
            "--state", str(RUN / "state"),
            "--station-db", str(RUN / "station.db"),
            "--triggers-home", str(RUN / "trigger-home"),
            "--workspace", str(REPO),
            "--target-dir", TARGET_DIR]
    result = subprocess.run(args, cwd=REPO, capture_output=True, text=True)
    (RUN / "logs" / "phaseC-consume-legs.jsonl").write_text(result.stdout, encoding="utf-8")
    (RUN / "logs" / "phaseC-consume-stderr.txt").write_text(result.stderr, encoding="utf-8")
    legs = [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
    if result.returncode != 0 or not any(l.get("leg") == "loop_complete" for l in legs):
        raise SystemExit(f"phaseC: 消费腿未闭环 rc={result.returncode}: {result.stderr[:500]}")
    log("C", f"消费腿闭环（{len(legs)} 腿证据）→ " +
        "/".join(l["leg"] for l in legs))
    return legs


def phaseC2_replay(envelope):
    """腿C重放：同信封再消费 → 游标命中拒派（负例证据）。"""
    args = [str(Path(TARGET_DIR) / "debug" / "examples" / "router_consume_leg"),
            "--envelope", str(envelope),
            "--state", str(RUN / "state"),
            "--station-db", str(RUN / "station.db"),
            "--triggers-home", str(RUN / "trigger-home"),
            "--workspace", str(REPO),
            "--target-dir", TARGET_DIR]
    result = subprocess.run(args, cwd=REPO, capture_output=True, text=True)
    combined = {"returncode": result.returncode,
                "stdout_lines": [l for l in result.stdout.splitlines() if l.strip()],
                "stderr": result.stderr.strip()}
    write_json(RUN / "logs" / "phaseC2-replay-reject.json", combined)
    if result.returncode == 0 or '"leg":"dedup_cursor"' not in result.stderr:
        raise SystemExit(f"phaseC2: 重放未拒派 rc={result.returncode}: {result.stderr[:300]}")
    log("C2", f"重放拒派 rc={result.returncode}（leg=dedup_cursor fail-closed）")


def collect_evidence():
    """证据收口：station 队列行转储 + sha256 清单 + 运行元数据。"""
    station = RUN / "station.db"
    if station.is_file():
        conn = sqlite3.connect(f"file:{station}?mode=ro", uri=True)
        try:
            tables = [r[0] for r in conn.execute(
                "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")]
            dump = {"tables": tables}
            for table in tables:
                cols = [d[1] for d in conn.execute(f"PRAGMA table_info({table})")]
                dump[table] = [dict(zip(cols, row))
                               for row in conn.execute(f"SELECT * FROM {table}")]
            write_json(RUN / "station-tasks.json", dump)
        finally:
            conn.close()
    lines = ["# poc-router-adjudicate 证据 sha256（相对 run/）"]
    paths = sorted(p for p in RUN.rglob("*")
                   if p.is_file() and "voucher-key.seed" not in p.name)
    for path in paths:
        rel = path.relative_to(RUN)
        lines.append(f"{sha256_of(path)}  {rel}")
    (RUN / "evidence-sha256.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=REPO,
                          capture_output=True, text=True).stdout.strip()
    write_json(RUN / "run-meta.json", {
        "ran_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "git_head": head,
        "order_id": ORDER_ID,
        "delivery_id": DELIVERY_ID,
        "source": SOURCE,
        "trigger_inbox_tool": str(TOOLS_TRIGGER_INBOX.relative_to(REPO)),
        "note": "凭证种子 voucher-key.seed（0600 秘密面）不入证据清单；"
                "复算以信封 payload_sha256 / 裁决单 doc_sha256 / 账本行为准。",
    })
    log("EV", f"证据收口：{len(paths)} 文件入 sha256 清单")


def main():
    fresh_run_dir()
    phase0_build()
    phaseA_submit()
    envelope, _ = phaseB_inbox()
    legs = phaseC_consume(envelope)
    phaseC2_replay(envelope)
    collect_evidence()
    doc_leg = next(l for l in legs if l["leg"] == "adjudication_doc")["detail"]
    receipt_leg = next(l for l in legs if l["leg"] == "receipt_and_completion")["detail"]
    print(json.dumps({
        "ok": True,
        "order_id": ORDER_ID,
        "delivery_id": DELIVERY_ID,
        "adjudication_doc": doc_leg["path"],
        "doc_sha256": doc_leg["doc_sha256"],
        "task_state": receipt_leg["task_state"],
        "receipt_decision": receipt_leg["receipt"]["decision"],
        "evidence_root": str(RUN),
    }, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
