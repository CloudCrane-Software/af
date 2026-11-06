#!/usr/bin/env python3
"""Collect one dynamic resource observation without modifying the static recipe."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

# python -I removes the script directory; add only this PoC's own module directory.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from validate_machines import load_registry  # noqa: E402

MOUNT_TARGETS = {"root": "/", "data": "/data", "tmp": "/tmp"}
MEMORY_KEYS = {
    "MemTotal": "total_bytes",
    "MemAvailable": "available_bytes",
    "SwapTotal": "swap_total_bytes",
    "SwapFree": "swap_free_bytes",
    "Shmem": "shmem_bytes",
}


class ObservationError(RuntimeError):
    pass


def _parse_meminfo(text: str) -> dict[str, int | None]:
    values = {key: None for key in MEMORY_KEYS.values()}
    for line in text.splitlines():
        name, separator, rest = line.partition(":")
        if not separator or name not in MEMORY_KEYS:
            continue
        fields = rest.split()
        if len(fields) != 2 or fields[1] != "kB":
            raise ObservationError(f"unexpected meminfo line: {line}")
        values[MEMORY_KEYS[name]] = int(fields[0]) * 1024
    return values


def _parse_df(text: str) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for line in text.splitlines()[1:]:
        if not line.strip():
            continue
        fields = line.split()
        if len(fields) < 6:
            raise ObservationError(f"unexpected df row: {line}")
        try:
            rows.append(
                {
                    "source": fields[0],
                    "total_bytes": int(fields[1]),
                    "used_bytes": int(fields[2]),
                    "available_bytes": int(fields[3]),
                    "used_pct": int(fields[4].rstrip("%")),
                    "target": fields[5],
                }
            )
        except ValueError as exc:
            raise ObservationError(f"non-numeric df row: {line}") from exc
    return rows


def _local_df(target: str | None = None, tmpfs: bool = False) -> list[dict[str, Any]]:
    command = ["df", "-B1", "-P"]
    if tmpfs:
        command.extend(["-t", "tmpfs"])
    if target is not None:
        command.extend(["--", target])
    result = subprocess.run(command, text=True, capture_output=True, timeout=10, check=False)
    if result.returncode != 0:
        raise ObservationError(result.stderr.strip() or f"df failed rc={result.returncode}")
    return _parse_df(result.stdout)


def _local_sample(mounts: list[str]) -> tuple[dict[str, Any], dict[str, Any], list[dict[str, Any]], list[str]]:
    reasons: list[str] = []
    memory = _parse_meminfo(Path("/proc/meminfo").read_text(encoding="utf-8"))
    load = Path("/proc/loadavg").read_text(encoding="utf-8").split()
    cpu = {
        "logical": os.cpu_count(),
        "load_1m": float(load[0]),
        "load_5m": float(load[1]),
        "load_15m": float(load[2]),
    }
    try:
        memory["tmpfs_used_bytes"] = sum(row["used_bytes"] for row in _local_df(tmpfs=True))
    except (OSError, ObservationError, subprocess.SubprocessError):
        memory["tmpfs_used_bytes"] = None
        reasons.append("TMPFS_READ_FAILED")
    disks: list[dict[str, Any]] = []
    for mount in mounts:
        try:
            rows = _local_df(target=MOUNT_TARGETS[mount])
            if len(rows) != 1:
                raise ObservationError(f"expected one df row for {mount}")
            row = rows[0]
            row.pop("target")
            disks.append({"mount": mount, **row, "read_error": None})
        except (OSError, ObservationError, subprocess.SubprocessError) as exc:
            reasons.append(f"DISK_{mount.upper()}_READ_FAILED")
            disks.append({"mount": mount, "read_error": str(exc)})
    return memory, cpu, disks, reasons


def _remote_sample(alias: str, mounts: list[str]) -> tuple[dict[str, Any], dict[str, Any], list[dict[str, Any]], list[str]]:
    sections = ["MEMINFO", "LOADAVG", "CPU", "TMPFS"] + [f"DISK_{mount.upper()}" for mount in mounts]
    commands = []
    for section in sections:
        commands.append(f"printf '%s\\n' __POCG_{section}__")
        if section == "MEMINFO":
            commands.append("cat /proc/meminfo")
        elif section == "LOADAVG":
            commands.append("cat /proc/loadavg")
        elif section == "CPU":
            commands.append("nproc")
        elif section == "TMPFS":
            commands.append("df -B1 -P -t tmpfs")
        else:
            mount = section[len("DISK_") :].lower()
            commands.append(f"df -B1 -P -- {MOUNT_TARGETS[mount]}")
    remote_command = "; ".join(commands)
    result = subprocess.run(
        [
            "ssh",
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=8",
            "-o",
            "ConnectionAttempts=1",
            alias,
            remote_command,
        ],
        text=True,
        capture_output=True,
        timeout=25,
        check=False,
    )
    if result.returncode != 0:
        raise ObservationError(result.stderr.strip() or f"ssh failed rc={result.returncode}")
    parsed: dict[str, list[str]] = {}
    current: str | None = None
    for line in result.stdout.splitlines():
        if line.startswith("__POCG_") and line.endswith("__"):
            current = line[len("__POCG_") : -2]
            parsed[current] = []
        elif current is not None:
            parsed[current].append(line)
    if set(parsed) != set(sections):
        raise ObservationError("remote sample is missing one or more sections")

    memory = _parse_meminfo("\n".join(parsed["MEMINFO"]))
    load = parsed["LOADAVG"][0].split()
    cpu = {
        "logical": int(parsed["CPU"][0].strip()),
        "load_1m": float(load[0]),
        "load_5m": float(load[1]),
        "load_15m": float(load[2]),
    }
    reasons: list[str] = []
    try:
        memory["tmpfs_used_bytes"] = sum(row["used_bytes"] for row in _parse_df("\n".join(parsed["TMPFS"])))
    except ObservationError:
        memory["tmpfs_used_bytes"] = None
        reasons.append("TMPFS_READ_FAILED")
    disks: list[dict[str, Any]] = []
    for mount in mounts:
        section = f"DISK_{mount.upper()}"
        try:
            rows = _parse_df("\n".join(parsed[section]))
            if len(rows) != 1:
                raise ObservationError(f"expected one df row for {mount}")
            row = rows[0]
            row.pop("target")
            disks.append({"mount": mount, **row, "read_error": None})
        except ObservationError as exc:
            reasons.append(f"DISK_{mount.upper()}_READ_FAILED")
            disks.append({"mount": mount, "read_error": str(exc)})
    return memory, cpu, disks, reasons


def collect(node: dict[str, Any]) -> dict[str, Any]:
    observed_at = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    local = node["endpoint"]["kind"] == "local"
    mounts = [disk["mount"] for disk in node["disks"]]
    reasons: list[str] = []
    memory: dict[str, Any] = {key: None for key in MEMORY_KEYS.values()}
    memory["tmpfs_used_bytes"] = None
    cpu: dict[str, Any] = {"logical": None, "load_1m": None, "load_5m": None, "load_15m": None}
    disks: list[dict[str, Any]] = []
    ssh_ok: bool | None = None if local else False
    try:
        if local:
            memory, cpu, disks, sample_reasons = _local_sample(mounts)
        else:
            memory, cpu, disks, sample_reasons = _remote_sample(node["network"]["ssh_alias"], mounts)
            ssh_ok = True
        reasons.extend(sample_reasons)
    except (OSError, ObservationError, subprocess.SubprocessError, ValueError, IndexError) as exc:
        reasons.append("LOCAL_READ_FAILED" if local else "SSH_UNREACHABLE")
        collection_error = str(exc)
    else:
        collection_error = None

    if node["admin_state"] != "active":
        health_state = "quarantined"
        reasons.append("ADMIN_NOT_ACTIVE")
    elif not ssh_ok and not local:
        health_state = "unknown"
    elif collection_error is not None:
        health_state = "unknown"
    else:
        health_state = "degraded" if reasons else "ok"
    if any(memory.get(key) is None for key in MEMORY_KEYS.values()) and health_state == "ok":
        health_state = "degraded"
        reasons.append("MEMORY_READ_PARTIAL")
    return {
        "schema_version": "af.resources.observation/v1",
        "node_id": node["node_id"],
        "observed_at": observed_at,
        "source": {"transport": "local" if local else "ssh", "read_only": True, "sudo": False},
        "memory": memory,
        "cpu": cpu,
        "disks": disks,
        "network": {"ssh_ok": ssh_ok},
        "health": {"state": health_state, "reason_codes": sorted(set(reasons))},
        "collection_error": collection_error,
    }


def write_observation(observation: dict[str, Any], state_root: Path) -> Path:
    directory = state_root / observation["node_id"]
    directory.mkdir(parents=True, exist_ok=True)
    filename = observation["observed_at"] + ".json"
    destination = directory / filename
    temporary = directory / f".{filename}.{os.getpid()}.tmp"
    temporary.write_text(json.dumps(observation, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    os.replace(temporary, destination)
    latest = directory / "latest.json"
    latest_tmp = directory / f".latest.json.{os.getpid()}.tmp"
    try:
        latest_tmp.unlink()
    except FileNotFoundError:
        pass
    latest_tmp.symlink_to(filename)
    os.replace(latest_tmp, latest)
    return destination


def main(argv: list[str] | None = None) -> int:
    repo_root = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registry", required=True)
    parser.add_argument("--node-id", required=True)
    parser.add_argument(
        "--state-root",
        type=Path,
        default=repo_root / "af-state/resources/observations",
        help="observation root (default: af-state/resources/observations)",
    )
    args = parser.parse_args(argv)
    try:
        registry = load_registry(args.registry)
        node = next(item for item in registry["nodes"] if item["node_id"] == args.node_id)
        observation = collect(node)
        destination = write_observation(observation, args.state_root)
    except (OSError, StopIteration) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    print(
        f"OK {destination} health={observation['health']['state']} "
        f"reason_codes={','.join(observation['health']['reason_codes']) or 'none'}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
