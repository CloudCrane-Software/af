#!/usr/bin/env python3
"""Minimal (resource, interval, profile, TTL) lease ledger with return and reap."""

from __future__ import annotations

import argparse
import json
import re
import sqlite3
import sys
import time
from pathlib import Path
from typing import Any

RESOURCE = re.compile(r"^(pandev|gov|anolis|mingda)/(root|data|tmp)$")
SAFE_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.-]*$")
SCHEMA = """
CREATE TABLE IF NOT EXISTS resource_lease (
    lease_id        TEXT PRIMARY KEY,
    resource        TEXT NOT NULL,
    work_interval   TEXT NOT NULL,
    profile_json    TEXT NOT NULL,
    holder          TEXT NOT NULL,
    reserved_bytes  INTEGER NOT NULL CHECK (reserved_bytes >= 0),
    ttl_ms          INTEGER NOT NULL CHECK (ttl_ms > 0),
    leased_at_ms    INTEGER NOT NULL,
    lease_until_ms  INTEGER NOT NULL,
    state           TEXT NOT NULL CHECK (state IN ('active', 'returned', 'expired')),
    returned_at_ms  INTEGER,
    expired_at_ms   INTEGER
);
CREATE UNIQUE INDEX IF NOT EXISTS resource_lease_one_active_interval
ON resource_lease(resource, work_interval)
WHERE state = 'active';
"""


class LeaseError(RuntimeError):
    pass


def _now_ms(value: int = 0) -> int:
    return value if value > 0 else time.time_ns() // 1_000_000


def _row(row: sqlite3.Row | None) -> dict[str, Any] | None:
    if row is None:
        return None
    result = dict(row)
    result["profile"] = json.loads(result.pop("profile_json"))
    return result


class LeaseStore:
    def __init__(self, path: str | Path):
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.connection = sqlite3.connect(self.path, timeout=5, isolation_level=None)
        self.connection.row_factory = sqlite3.Row
        self.connection.execute("PRAGMA foreign_keys = ON")
        self.connection.execute("PRAGMA busy_timeout = 5000")
        self.connection.executescript(SCHEMA)

    def close(self) -> None:
        self.connection.close()

    def __enter__(self) -> "LeaseStore":
        return self

    def __exit__(self, *_: object) -> None:
        self.close()

    @staticmethod
    def _reap(connection: sqlite3.Connection, now_ms: int) -> list[str]:
        ids = [
            row[0]
            for row in connection.execute(
                "SELECT lease_id FROM resource_lease "
                "WHERE state = 'active' AND lease_until_ms < ?1 ORDER BY lease_id",
                (now_ms,),
            ).fetchall()
        ]
        connection.execute(
            "UPDATE resource_lease SET state = 'expired', expired_at_ms = ?1 "
            "WHERE state = 'active' AND lease_until_ms < ?1",
            (now_ms,),
        )
        return ids

    def acquire(
        self,
        lease_id: str,
        resource: str,
        work_interval: str,
        profile: dict[str, Any],
        holder: str,
        reserved_bytes: int,
        ttl_ms: int,
        now_ms: int = 0,
    ) -> dict[str, Any]:
        now = _now_ms(now_ms)
        if not SAFE_ID.fullmatch(lease_id) or not SAFE_ID.fullmatch(holder):
            raise LeaseError("lease_id and holder must be safe logical identifiers")
        if not RESOURCE.fullmatch(resource):
            raise LeaseError("resource must be <registered-node>/<logical-mount>")
        if not isinstance(work_interval, str) or not work_interval.strip():
            raise LeaseError("work_interval must be non-empty")
        if isinstance(reserved_bytes, bool) or not isinstance(reserved_bytes, int) or reserved_bytes < 0:
            raise LeaseError("reserved_bytes must be a non-negative integer")
        if isinstance(ttl_ms, bool) or not isinstance(ttl_ms, int) or ttl_ms <= 0:
            raise LeaseError("ttl_ms must be a positive integer")
        if not isinstance(profile, dict) or profile.get("schema_version") != "af.resources/v1":
            raise LeaseError("profile must be an af.resources/v1 JSON object")
        if profile.get("node_id") != resource.split("/", 1)[0]:
            raise LeaseError("profile node_id must match resource")
        profile_json = json.dumps(profile, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        try:
            self.connection.execute("BEGIN IMMEDIATE")
            self._reap(self.connection, now)
            self.connection.execute(
                "INSERT INTO resource_lease "
                "(lease_id, resource, work_interval, profile_json, holder, reserved_bytes, "
                " ttl_ms, leased_at_ms, lease_until_ms, state, returned_at_ms, expired_at_ms) "
                "VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'active', NULL, NULL)",
                (
                    lease_id,
                    resource,
                    work_interval,
                    profile_json,
                    holder,
                    reserved_bytes,
                    ttl_ms,
                    now,
                    now + ttl_ms,
                ),
            )
            self.connection.execute("COMMIT")
        except sqlite3.IntegrityError as exc:
            self.connection.execute("ROLLBACK")
            raise LeaseError(f"lease id or active resource interval already exists: {exc}") from exc
        except Exception:
            self.connection.execute("ROLLBACK")
            raise
        result = self.get(lease_id)
        if result is None:
            raise LeaseError("committed lease disappeared")
        return result

    def return_lease(self, lease_id: str, holder: str, now_ms: int = 0) -> bool:
        """Only the current holder can return; like af-station, expiry alone does not block return."""
        cursor = self.connection.execute(
            "UPDATE resource_lease SET state = 'returned', returned_at_ms = ?1 "
            "WHERE lease_id = ?2 AND holder = ?3 AND state = 'active'",
            (_now_ms(now_ms), lease_id, holder),
        )
        return cursor.rowcount == 1

    def heartbeat(self, lease_id: str, holder: str, extend_ms: int, now_ms: int = 0) -> bool:
        """Reset to now+TTL only for the current holder with an unexpired active lease."""
        now = _now_ms(now_ms)
        if isinstance(extend_ms, bool) or not isinstance(extend_ms, int) or extend_ms <= 0:
            raise LeaseError("extend_ms must be a positive integer")
        cursor = self.connection.execute(
            "UPDATE resource_lease SET ttl_ms = ?1, lease_until_ms = ?2 "
            "WHERE lease_id = ?3 AND holder = ?4 AND state = 'active' AND lease_until_ms > ?5",
            (extend_ms, now + extend_ms, lease_id, holder, now),
        )
        return cursor.rowcount == 1

    def reap(self, now_ms: int = 0) -> list[str]:
        now = _now_ms(now_ms)
        try:
            self.connection.execute("BEGIN IMMEDIATE")
            ids = self._reap(self.connection, now)
            self.connection.execute("COMMIT")
            return ids
        except Exception:
            self.connection.execute("ROLLBACK")
            raise

    def get(self, lease_id: str) -> dict[str, Any] | None:
        row = self.connection.execute("SELECT * FROM resource_lease WHERE lease_id = ?1", (lease_id,)).fetchone()
        return _row(row)


def _profile(text: str) -> dict[str, Any]:
    try:
        value = json.loads(text)
    except json.JSONDecodeError as exc:
        raise LeaseError(f"profile is not valid JSON: {exc}") from exc
    if not isinstance(value, dict):
        raise LeaseError("profile must be a JSON object")
    return value


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db", required=True)
    commands = parser.add_subparsers(dest="command", required=True)

    acquire = commands.add_parser("acquire")
    acquire.add_argument("--lease-id", required=True)
    acquire.add_argument("--resource", required=True)
    acquire.add_argument("--interval", required=True)
    acquire.add_argument("--profile", required=True, help="af.resources/v1 JSON object")
    acquire.add_argument("--holder", required=True)
    acquire.add_argument("--reserved-bytes", type=int, required=True)
    acquire.add_argument("--ttl-ms", type=int, required=True)
    acquire.add_argument("--now-ms", type=int, default=0)

    return_command = commands.add_parser("return")
    return_command.add_argument("--lease-id", required=True)
    return_command.add_argument("--holder", required=True)
    return_command.add_argument("--now-ms", type=int, default=0)

    heartbeat = commands.add_parser("heartbeat")
    heartbeat.add_argument("--lease-id", required=True)
    heartbeat.add_argument("--holder", required=True)
    heartbeat.add_argument("--extend-ms", type=int, required=True)
    heartbeat.add_argument("--now-ms", type=int, default=0)

    reap = commands.add_parser("reap")
    reap.add_argument("--now-ms", type=int, default=0)

    show = commands.add_parser("show")
    show.add_argument("--lease-id", required=True)

    args = parser.parse_args(argv)
    try:
        with LeaseStore(args.db) as store:
            if args.command == "acquire":
                result: Any = store.acquire(
                    args.lease_id,
                    args.resource,
                    args.interval,
                    _profile(args.profile),
                    args.holder,
                    args.reserved_bytes,
                    args.ttl_ms,
                    args.now_ms,
                )
            elif args.command == "return":
                if not store.return_lease(args.lease_id, args.holder, args.now_ms):
                    raise LeaseError("return rejected: lease is not active for this holder")
                result = store.get(args.lease_id)
            elif args.command == "heartbeat":
                if not store.heartbeat(args.lease_id, args.holder, args.extend_ms, args.now_ms):
                    raise LeaseError("heartbeat rejected: lease expired or holder does not match")
                result = store.get(args.lease_id)
            elif args.command == "reap":
                ids = store.reap(args.now_ms)
                result = {"reaped": len(ids), "lease_ids": ids}
            else:
                result = store.get(args.lease_id)
                if result is None:
                    raise LeaseError("lease not found")
    except (OSError, sqlite3.Error, LeaseError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
