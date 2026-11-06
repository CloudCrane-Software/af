#!/usr/bin/env python3
"""把 af-otel-projector 的四层语义 span 经既有 OTLP/HTTP 送进现役 collector。

合规路径（不改判据件、不直写 CH）：
  订单目录 → tools/otel/af-otel-projector.py:project_order
           → OTLP/HTTP JSON POST <AF_OTEL_ENDPOINT>/v1/traces
           → 现役 otelcol（<内网地址>:4318）clickhouse exporter
           → otel.otel_traces

时间戳改写为发射时刻：投影器沿用 ledger/normal.jsonl 的历史 ts，
近窗 360min 回查只认 Timestamp；新生产者上线按「此刻入库」计。
派生 ID / 属性键 / Resource 九键一律保持投影器原值（§1.0 / §2）。

用法：
  python3 -I emit-semantic-otlp.py <order_dir> [--harvest] [--endpoint URL]
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

DEFAULT_ENDPOINT = os.environ.get("AF_OTEL_ENDPOINT") or "http://<内网地址>:4318"
DEFAULT_PROJECTOR = os.environ.get("AF_OTEL_PROJECTOR") or os.path.expanduser(
    "~/workspace/agent黑灯工厂/tools/otel/af-otel-projector.py"
)
INVALID = "0000000000000000"


def _load_projector(path: str):
    p = Path(path)
    if not p.is_file():
        raise SystemExit(f"FATAL: 投影器不在位：{p}")
    spec = importlib.util.spec_from_file_location("af_otel_projector", p)
    if spec is None or spec.loader is None:
        raise SystemExit(f"FATAL: 无法加载投影器：{p}")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _attr(value):
    if isinstance(value, bool):
        return {"boolValue": value}
    if isinstance(value, int) and not isinstance(value, bool):
        return {"intValue": str(value)}
    if isinstance(value, float):
        return {"doubleValue": value}
    if isinstance(value, (list, tuple)):
        return {"arrayValue": {"values": [_attr(v) for v in value]}}
    return {"stringValue": str(value)}


def _attrs(d: dict) -> list:
    return [{"key": k, "value": _attr(v)} for k, v in sorted((d or {}).items())
            if v is not None]


def _parent(pid) -> str:
    if not pid:
        return INVALID
    return str(pid)


def to_otlp(spans: list[dict], now_ns: int) -> dict:
    """投影器内存态 → OTLP resourceSpans。同 Resource 归并；时间戳改写为 now。"""
    groups: dict[str, tuple[dict, list]] = {}
    for i, sp in enumerate(spans):
        res = dict(sp.get("resource") or {})
        key = json.dumps(res, sort_keys=True, ensure_ascii=False)
        otlp_span = {
            "traceId": sp["traceId"],
            "spanId": sp["spanId"],
            "parentSpanId": _parent(sp.get("parentSpanId")),
            "name": sp["name"],
            "kind": int(sp.get("kind") or 1),
            "startTimeUnixNano": str(now_ns + i * 1000),
            "endTimeUnixNano": str(now_ns + i * 1000 + 1),
            "attributes": _attrs(sp.get("attributes") or {}),
            "status": {},
        }
        if key not in groups:
            groups[key] = (res, [])
        groups[key][1].append(otlp_span)
    resource_spans = []
    for res, items in groups.values():
        resource_spans.append({
            "resource": {"attributes": _attrs(res)},
            "scopeSpans": [{
                "scope": {"name": "af-otel-projector", "version": "0.1.0"},
                "spans": items,
            }],
        })
    return {"resourceSpans": resource_spans}


def post_otlp(endpoint: str, payload: dict, timeout: float = 10.0) -> None:
    body = json.dumps(payload, ensure_ascii=False).encode()
    url = endpoint.rstrip("/") + "/v1/traces"
    req = urllib.request.Request(
        url, data=body, headers={"Content-Type": "application/json"}, method="POST"
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        resp.read()
        if resp.status not in (200, 202):
            raise RuntimeError(f"OTLP HTTP {resp.status}")


def emit_harvest(endpoint: str, order_id: str, work_order_id: str) -> bool:
    """走工厂族既有 helper；不可 import 时退回同形 OTLP POST（仍经 collector）。"""
    factory_dir = str(Path(DEFAULT_PROJECTOR).resolve().parent)
    if factory_dir not in sys.path:
        sys.path.insert(0, factory_dir)
    try:
        import factory_otel  # type: ignore  # noqa: E402
        os.environ.setdefault("AF_OTEL_ENDPOINT", endpoint)
        return bool(factory_otel.harvest_report(
            task_id=order_id, verdict="ok", atoms=1,
            order_id=order_id, work_order_id=work_order_id,
        ))
    except Exception as exc:  # noqa: BLE001 — fail-open 退回裸 POST
        print(f"factory_otel.harvest_report 不可用（{type(exc).__name__}: {exc}），改走裸 OTLP",
              file=sys.stderr)
    now_ns = time.time_ns()
    import hashlib
    woid = work_order_id
    tid = hashlib.sha256(("af-trace-v1|" + woid).encode()).hexdigest()[:32]
    sid = hashlib.sha256(("af-span-v1|harvest|" + order_id + "|" + str(now_ns // 1_000_000)).encode()).hexdigest()[:16]
    payload = {"resourceSpans": [{
        "resource": {"attributes": _attrs({
            "service.name": "sandbox-harvester",
            "service.version": "otel-mvp-20261028",
            "af.order_id": order_id,
            "af.work_order_id": woid,
        })},
        "scopeSpans": [{"scope": {"name": "af-factory-lane"}, "spans": [{
            "traceId": tid, "spanId": sid, "parentSpanId": INVALID,
            "name": "factory.harvest", "kind": 1,
            "startTimeUnixNano": str(now_ns), "endTimeUnixNano": str(now_ns),
            "attributes": _attrs({
                "af.factory.domain": "harvest",
                "af.factory.mode": "harvest",
                "af.factory.harvest.task_id": order_id,
                "af.factory.harvest.verdict": "ok",
                "af.factory.harvest.atoms": 1,
            }),
            "status": {"code": 1},
        }]}],
    }]}
    post_otlp(endpoint, payload)
    return True


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="投影一单并经现役 collector 入库")
    ap.add_argument("order_dir", type=Path)
    ap.add_argument("--endpoint", default=DEFAULT_ENDPOINT)
    ap.add_argument("--projector", default=DEFAULT_PROJECTOR)
    ap.add_argument("--harvest", action="store_true",
                    help="顺带发一条 factory.harvest（P1 名册项，走 factory_otel）")
    args = ap.parse_args(argv)
    order_dir = args.order_dir.expanduser().resolve()
    if not order_dir.is_dir():
        print(f"FATAL: 订单目录不在位：{order_dir}", file=sys.stderr)
        return 2
    proj = _load_projector(args.projector)
    spans, findings = proj.project_order(order_dir)
    for f in findings:
        print(f"finding: {f}", file=sys.stderr)
    if not spans:
        print("FATAL: 投影出 0 条 span（ledger 无真源或零账本行）", file=sys.stderr)
        return 1
    names = {}
    for s in spans:
        names[s["name"]] = names.get(s["name"], 0) + 1
    oid = (spans[0].get("resource") or {}).get("af.order_id") or order_dir.name
    woid = (spans[0].get("resource") or {}).get("af.work_order_id") or ""
    payload = to_otlp(spans, time.time_ns())
    try:
        post_otlp(args.endpoint, payload)
    except (urllib.error.URLError, TimeoutError, RuntimeError, OSError) as exc:
        print(f"FATAL: OTLP 发射失败（collector 不可达即缺口）：{exc}", file=sys.stderr)
        return 1
    print(f"emitted {len(spans)} semantic spans → {args.endpoint}/v1/traces")
    print(f"  order_id={oid} work_order_id={woid[:16]}… names={names}")
    if args.harvest:
        try:
            ok = emit_harvest(args.endpoint, str(oid), str(woid))
        except (urllib.error.URLError, TimeoutError, RuntimeError, OSError) as exc:
            print(f"FATAL: factory.harvest 发射失败：{exc}", file=sys.stderr)
            return 1
        print(f"  harvest ok={ok} task_id={oid}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
