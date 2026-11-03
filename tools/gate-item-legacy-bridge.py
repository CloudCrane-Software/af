#!/usr/bin/env python3
"""Bridge one legacy all-gates item ledger line into gate-item-emit.py.

Legacy item products today are ``item-N.result`` pipe files written by
all-gates.sh ``run_item``/``record_item``::

    STATUS|ms|note        # STATUS ∈ PASS|FAIL|SKIP, note is free text

The T5 终裁⑤ plan (计划/产品化工作方案-v0.3-20261103.md) is for legacy items to
grow their own ``--json-out`` structured products (item-*.json); those go
straight to ``gate-item-emit.py`` and this bridge retires. Until then, this
bridge converts the recorded ledger line into the legacy item-*.json shape and
hands it to the emitter through its **existing CLI interface** — validation
(status enum, skip_reason enum, check_id match, required match, no-overwrite
persist) stays entirely inside gate-item-emit.py.

Honesty rules (mirroring the emitter's docstring):
  * status passes through from the recorded ledger line only. A bare legacy
    exit code never manufactures PASS — this bridge does not even read exit
    codes (the pipe line does not carry one; fabricating `exit_code: 0` would
    be a claim about a process that may not exist, e.g. record_item rows).
  * SKIP stays SKIP. skip_reason mapping is fail-loud: only two recorded
    vocabularies are recognised (anolis-only → environment_unavailable;
    全局时限/全局预算 → environment_unavailable); anything else refuses to
    guess and exits nonzero for human/J5 classification.
  * FAIL stays FAIL (timeout rc=124 included).

HANDOFF (main agent, serial merge): all-gates.sh, test-manifest.list,
test-ledger-migration-recon.py and numbers-sota-check.py are owned by the J5
repair batch (fix run in flight) and are intentionally untouched by this
bridge. Wiring the per-item invocation INTO all-gates.sh is that batch's call;
this file only makes the per-item conversion executable and verified.
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile

SKIP_ENVIRONMENT_MARKERS = ("anolis-only", "全局时限", "全局预算")


def fail(message: str) -> int:
    sys.stderr.write(f"gate-item-legacy-bridge: {message}\n")
    return 2


def parse_result_line(text: str) -> tuple[str, str]:
    """Split `STATUS|ms|note`; note may itself contain pipes."""
    parts = text.rstrip("\n").split("|", 2)
    if len(parts) != 3:
        raise ValueError(f"legacy item result line must be STATUS|ms|note, got: {text!r}")
    status, _milliseconds, note = parts
    return status, note


def skip_reason_for(note: str) -> str:
    """Map the two recorded skip vocabularies; refuse to guess otherwise."""
    for marker in SKIP_ENVIRONMENT_MARKERS:
        if marker in note:
            return "environment_unavailable"
    raise ValueError(
        "unrecognised legacy SKIP vocabulary (refusing to guess skip_reason): "
        f"{note!r} — classify as one of not_applicable/environment_unavailable/"
        "explicitly_excluded or extend SKIP_ENVIRONMENT_MARKERS with a recorded marker"
    )


def legacy_item_json(check_id: str, required: bool, status: str, note: str) -> dict:
    item: dict = {
        "check_id": check_id,
        "required": required,
        "status": status.lower(),
        "detail": note,
    }
    if status.upper() == "SKIP":
        item["skip_reason"] = skip_reason_for(note)
    return item


def main() -> int:
    here = os.path.dirname(os.path.abspath(__file__))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--result", required=True,
                        help="legacy item-N.result ledger line file (STATUS|ms|note)")
    parser.add_argument("--check-id", required=True)
    parser.add_argument("--json-out", required=True,
                        help="normalized GateCheckResult JSON (no overwrite)")
    required_group = parser.add_mutually_exclusive_group()
    required_group.add_argument("--required", dest="required", action="store_true", default=True)
    required_group.add_argument("--optional", dest="required", action="store_false")
    parser.add_argument("--emit", default=os.path.join(here, "gate-item-emit.py"),
                        help="path to gate-item-emit.py (default: sibling)")
    args = parser.parse_args()

    try:
        with open(args.result, "r", encoding="utf-8") as source:
            status, note = parse_result_line(source.read())
        item = legacy_item_json(args.check_id, args.required, status, note)
    except OSError as error:
        return fail(f"legacy item result unreadable: {error}")
    except ValueError as error:
        return fail(str(error))

    # Intermediate legacy item-*.json lives in a scratch file, never in any
    # owned tree; the emitter validates and persists the normalized output.
    with tempfile.TemporaryDirectory(prefix="gate-item-bridge-") as work:
        legacy_path = os.path.join(work, "legacy-item.json")
        with open(legacy_path, "w", encoding="utf-8") as target:
            json.dump(item, target, ensure_ascii=False)
        command = [
            sys.executable, args.emit,
            "--input", legacy_path,
            "--json-out", args.json_out,
            "--check-id", args.check_id,
            "--required" if args.required else "--optional",
        ]
        completed = subprocess.run(command, check=False)
        if completed.returncode != 0:
            return fail(f"gate-item-emit.py rc={completed.returncode}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
