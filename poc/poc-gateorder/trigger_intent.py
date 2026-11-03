#!/usr/bin/env python3
"""Map one webhook envelope to an af.gate.dispatch_intent.v1 declaration.

This program has no listener, inbox write, subprocess execution, or order API.
HANDOFF (PoC-D/main agent): trigger-inbox.py owns receipt/dedup/persistence. After
that inbox accepts an event, the router may consume this stdout declaration and
create the existing WorkOrder whose Task Graph contains the declared check node.
"""

import argparse
import hashlib
import json
import re
import sys
from typing import Any

SHA256 = re.compile(r"^[0-9a-f]{64}$")


def load_recipe(path: str) -> tuple[dict[str, Any], str]:
    with open(path, "rb") as source:
        raw = source.read()
    recipe = json.loads(raw)
    if recipe.get("recipe_kind") != "af.gate.trigger_recipe.v1":
        raise ValueError("unsupported trigger recipe")
    if not isinstance(recipe.get("registry"), dict) or not isinstance(recipe.get("rules"), list):
        raise ValueError("trigger recipe needs registry and rules")
    return recipe, hashlib.sha256(raw).hexdigest()


def intent(recipe: dict[str, Any], recipe_digest: str, args: argparse.Namespace) -> dict[str, Any]:
    action = args.action or ""
    matched = None
    for rule in recipe["rules"]:
        if rule.get("event") != args.event:
            continue
        actions = rule.get("actions", [])
        if actions and action not in actions:
            continue
        matched = rule
        break
    if matched is None:
        raise LookupError(f"no gate mapping for event={args.event!r} action={action!r}")
    suite_name = matched["suite"]
    suite = recipe["registry"][suite_name]
    if suite["graph_node"].get("kind") != "check":
        raise ValueError("gate dispatch must use the existing Task Graph check node")
    return {
        "intent": "af.gate.dispatch_intent.v1",
        "destination": "trigger_inbox",
        "declaration_only": True,
        "webhook_execution": False,
        "rule_id": matched["rule_id"],
        "recipe_digest": recipe_digest,
        "trigger": {
            "event_id": args.event_id,
            "event": args.event,
            "action": args.action,
            "payload_ref": args.payload_ref,
            "payload_sha256": args.payload_sha256,
            "dedup_key": args.event_id,
        },
        "work_order": {
            "order_template": "gate-order",
            "evaluation": suite["evaluation"],
            "graph_node": suite["graph_node"],
            "artifact_type": "af.gate_result.v1"
        }
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--recipe", required=True)
    parser.add_argument("--event", required=True)
    parser.add_argument("--action")
    parser.add_argument("--event-id", required=True)
    parser.add_argument("--payload-ref", required=True)
    parser.add_argument("--payload-sha256", required=True)
    args = parser.parse_args()
    if not SHA256.fullmatch(args.payload_sha256):
        parser.error("--payload-sha256 must be 64 lowercase hex characters")
    recipe, recipe_digest = load_recipe(args.recipe)
    try:
        result = intent(recipe, recipe_digest, args)
    except LookupError as error:
        print(str(error), file=sys.stderr)
        return 3
    sys.stdout.write(json.dumps(result, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
