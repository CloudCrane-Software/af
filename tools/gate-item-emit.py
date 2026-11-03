#!/usr/bin/env python3
"""Normalize one legacy item-*.json into GateCheckResult JSON.

HANDOFF (main agent, serial merge): all-gates.sh is intentionally untouched while
J5 owns it. Wire each legacy item's existing --json-out product to this emitter,
then freeze the resulting check definitions/results through af-eval. Do not wire a
bare legacy exit code to verdict=pass.
"""

import argparse
import json
import os
import sys
from typing import Any

SKIP_REASONS = {"not_applicable", "environment_unavailable", "explicitly_excluded"}
STATUSES = {"pass", "fail", "skip"}


def skipped(check_id: str, required: bool, detail: str) -> dict[str, Any]:
    return {
        "check_id": check_id,
        "required": required,
        "status": "skip",
        "skip_reason": "not_applicable",
        "detail": detail,
    }


def normalize(item: Any, check_id: str, required: bool) -> dict[str, Any]:
    if not isinstance(item, dict):
        return skipped(check_id, required, "legacy item JSON must be an object")
    status = item.get("status")
    if isinstance(status, str):
        status = status.lower()
    if status not in STATUSES:
        return skipped(check_id, required, f"legacy item has no explicit PASS/FAIL/SKIP: {status!r}")
    if item.get("check_id", check_id) != check_id:
        return skipped(check_id, required, "legacy item check_id differs from frozen check_id")
    if bool(item.get("required", required)) != required:
        return skipped(check_id, required, "legacy item required differs from frozen definition")
    detail = str(item.get("detail", ""))
    exit_code = item.get("exit_code")
    # Exit status is only a conflict signal. It can never manufacture PASS.
    if status == "pass" and isinstance(exit_code, int) and exit_code != 0:
        return skipped(check_id, required, f"nonzero exit_code={exit_code} conflicts with PASS")
    result: dict[str, Any] = {
        "check_id": check_id,
        "required": required,
        "status": status,
        "detail": detail,
    }
    if status == "skip":
        reason = item.get("skip_reason")
        if reason not in SKIP_REASONS:
            return skipped(check_id, required, f"SKIP needs one of {sorted(SKIP_REASONS)}: {reason!r}")
        result["skip_reason"] = reason
    return result


def persist_json(path: str, payload: dict[str, Any]) -> None:
    data = json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    if os.path.exists(path):
        with open(path, "rb") as existing:
            if existing.read() == data:
                return
        raise RuntimeError(f"refusing to overwrite different JSON output: {path}")
    with open(path, "xb") as output:
        output.write(data)
        output.flush()
        os.fsync(output.fileno())


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, help="legacy item-*.json; never infer PASS from exit code")
    parser.add_argument("--json-out", required=True, help="new normalized GateCheckResult JSON (no overwrite)")
    parser.add_argument("--check-id", required=True)
    required = parser.add_mutually_exclusive_group()
    required.add_argument("--required", dest="required", action="store_true", default=True)
    required.add_argument("--optional", dest="required", action="store_false")
    args = parser.parse_args()
    try:
        with open(args.input, "r", encoding="utf-8") as source:
            item = json.load(source)
        result = normalize(item, args.check_id, args.required)
    except (OSError, ValueError) as error:
        result = skipped(args.check_id, args.required, f"legacy item JSON unreadable: {error}")
    persist_json(args.json_out, result)
    sys.stdout.write(json.dumps(result, ensure_ascii=False, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
