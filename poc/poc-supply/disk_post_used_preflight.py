#!/usr/bin/env python3
"""Fail-closed post-admission disk watermark check for one node and one mount."""

from __future__ import annotations

import argparse
import json
import sqlite3
import sys
import time
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from validate_machines import load_registry  # noqa: E402


class PreflightError(ValueError):
    pass


def _nonnegative(value: Any, name: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise PreflightError(f"{name} must be a non-negative integer byte count")
    return value


def evaluate_post_watermark(
    policy: dict[str, Any],
    reading: dict[str, Any],
    lease_reserved_bytes: int,
    request_bytes: int,
) -> dict[str, Any]:
    """Compute post_free=avail-sum(active lease reservations)-new request exactly."""
    total = _nonnegative(reading.get("total_bytes"), "total_bytes")
    available = _nonnegative(reading.get("available_bytes"), "available_bytes")
    reserved = _nonnegative(lease_reserved_bytes, "lease_reserved_bytes")
    request = _nonnegative(request_bytes, "request_bytes")
    minimum_free = _nonnegative(policy.get("min_post_free_bytes"), "min_post_free_bytes")
    maximum_pct = _nonnegative(policy.get("max_post_used_pct"), "max_post_used_pct")
    if total <= 0:
        raise PreflightError("total_bytes must be positive")
    if maximum_pct > 100:
        raise PreflightError("max_post_used_pct cannot exceed 100")
    if available > total:
        raise PreflightError("available_bytes cannot exceed total_bytes")

    post_free = available - reserved - request
    # df total may include root-reserved blocks, so total-post_free is deliberately
    # conservative relative to used+reserved+request.
    post_used = total - post_free
    reasons: list[dict[str, Any]] = []
    if post_free < minimum_free:
        reasons.append(
            {
                "code": "MIN_POST_FREE_BYTES",
                "required": minimum_free,
                "actual": post_free,
            }
        )
    if 100 * post_used > maximum_pct * total:
        reasons.append(
            {
                "code": "MAX_POST_USED_PCT",
                "required": maximum_pct,
                "actual": 100.0 * post_used / total,
            }
        )
    return {
        "admit": not reasons,
        "post_free_bytes": post_free,
        "post_used_bytes": post_used,
        "post_used_pct": 100.0 * post_used / total,
        "available_bytes": available,
        "lease_reserved_bytes": reserved,
        "request_bytes": request,
        "min_post_free_bytes": minimum_free,
        "max_post_used_pct": maximum_pct,
        "reasons": reasons,
    }


def active_reserved_bytes(db_path: str | Path, resource: str, now_ms: int) -> int:
    """Sum only unexpired active reservations for this exact node/mount resource."""
    try:
        with sqlite3.connect(str(db_path)) as connection:
            row = connection.execute(
                "SELECT COALESCE(SUM(reserved_bytes), 0) "
                "FROM resource_lease "
                "WHERE resource = ?1 AND state = 'active' AND lease_until_ms > ?2",
                (resource, now_ms),
            ).fetchone()
    except sqlite3.Error as exc:
        raise PreflightError(f"cannot read lease reservations from {db_path}: {exc}") from exc
    if row is None:
        raise PreflightError("lease reservation query returned no row")
    return int(row[0])


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registry", required=True)
    parser.add_argument("--observation", required=True)
    parser.add_argument("--leases-db", required=True)
    parser.add_argument("--node-id", required=True)
    parser.add_argument("--mount", required=True)
    parser.add_argument("--request-bytes", type=int, required=True)
    parser.add_argument("--now-ms", type=int, default=0)
    args = parser.parse_args(argv)
    now_ms = args.now_ms if args.now_ms > 0 else time.time_ns() // 1_000_000
    try:
        request = _nonnegative(args.request_bytes, "request_bytes")
        registry = load_registry(args.registry)
        node = next(item for item in registry["nodes"] if item["node_id"] == args.node_id)
        policy = next(disk for disk in node["disks"] if disk["mount"] == args.mount)
        observation = json.loads(Path(args.observation).read_text(encoding="utf-8"))
        if observation.get("schema_version") != "af.resources.observation/v1":
            raise PreflightError("unexpected observation schema")
        if observation.get("node_id") != args.node_id:
            raise PreflightError("observation node does not match requested node")
        if observation.get("health", {}).get("state") not in {"ok", "degraded"}:
            raise PreflightError("observation health is not usable for admission")
        reading = next(disk for disk in observation["disks"] if disk.get("mount") == args.mount)
        if reading.get("read_error") is not None:
            raise PreflightError(f"disk observation failed: {reading['read_error']}")
        resource = f"{args.node_id}/{args.mount}"
        reserved = active_reserved_bytes(args.leases_db, resource, now_ms)
        verdict = evaluate_post_watermark(policy, reading, reserved, request)
        verdict.update({"node_id": args.node_id, "mount": args.mount, "resource": resource, "now_ms": now_ms})
    except (OSError, ValueError, StopIteration, KeyError, json.JSONDecodeError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(verdict, ensure_ascii=False, sort_keys=True))
    return 0 if verdict["admit"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
