#!/usr/bin/env python3
"""Restricted loader and static validator for af.resources/v1 machine recipes."""

from __future__ import annotations

import argparse
import datetime as dt
import ipaddress
import re
import sys
from pathlib import Path, PurePosixPath
from typing import Any
from urllib.parse import urlsplit

import yaml
from yaml.nodes import MappingNode
from yaml.tokens import AliasToken, AnchorToken, TagToken

MAX_DOCUMENT_BYTES = 1024 * 1024
MAX_DEPTH = 32
MAX_ITEMS = 10000
EXPECTED_NODES = {"pandev", "gov", "anolis", "mingda"}
SAFE_NAME = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.-]*$")
INLINE_KEYS = {"script", "inline_script", "command", "shell", "exec"}
SECRET_KEYS = {
    "password",
    "passwd",
    "token",
    "secret",
    "api_key",
    "access_key",
    "private_key",
    "credential",
    "credentials",
}


class RegistryError(ValueError):
    """A recipe violates the restricted format or af.resources/v1 schema."""


class RestrictedLoader(yaml.SafeLoader):
    """SafeLoader plus duplicate-key rejection; aliases/tags are rejected earlier."""


def _construct_mapping(loader: RestrictedLoader, node: MappingNode, deep: bool = False) -> dict[str, Any]:
    if not isinstance(node, MappingNode):
        raise RegistryError("expected a mapping")
    result: dict[str, Any] = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if not isinstance(key, str):
            raise RegistryError("mapping keys must be strings")
        if key in result:
            raise RegistryError(f"duplicate mapping key: {key}")
        result[key] = loader.construct_object(value_node, deep=deep)
    return result


RestrictedLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG,
    _construct_mapping,
)


def _restricted_load(text: str) -> Any:
    try:
        for token in yaml.scan(text):
            if isinstance(token, (AliasToken, AnchorToken, TagToken)):
                raise RegistryError("YAML anchors, aliases, and explicit tags are forbidden")
        return yaml.load(text, Loader=RestrictedLoader)
    except yaml.YAMLError as exc:
        raise RegistryError(f"restricted YAML parse failed: {exc}") from exc


def _fail(path: str, message: str) -> None:
    raise RegistryError(f"{path}: {message}")


def _mapping(value: Any, path: str, required: set[str], optional: set[str] | None = None) -> dict[str, Any]:
    if not isinstance(value, dict):
        _fail(path, "expected mapping")
    optional = optional or set()
    unknown = set(value) - required - optional
    if unknown:
        _fail(path, f"deny_unknown_fields: {sorted(unknown)}")
    missing = required - set(value)
    if missing:
        _fail(path, f"missing required fields: {sorted(missing)}")
    return value


def _string(value: Any, path: str) -> str:
    if not isinstance(value, str) or not value.strip():
        _fail(path, "expected non-empty string")
    return value


def _integer(value: Any, path: str, minimum: int = 0) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        _fail(path, f"expected integer >= {minimum}")
    return value


def _named(value: Any, path: str) -> str:
    text = _string(value, path)
    if not SAFE_NAME.fullmatch(text):
        _fail(path, "expected a logical name, not a path or shell text")
    return text


def _relative_root(value: Any, path: str) -> str:
    text = _string(value, path)
    pure = PurePosixPath(text)
    if pure.is_absolute() or "\\" in text or any(part in {"", ".", ".."} for part in pure.parts):
        _fail(path, "write root must be a clean relative path")
    return text


def _scan_scalars(value: Any, path: str = "$", depth: int = 0, counter: list[int] | None = None) -> None:
    if counter is None:
        counter = [0]
    counter[0] += 1
    if counter[0] > MAX_ITEMS:
        _fail(path, "document has too many items")
    if depth > MAX_DEPTH:
        _fail(path, "document nesting is too deep")
    if isinstance(value, dict):
        for key, child in value.items():
            key_path = f"{path}.{key}"
            normalized = key.lower()
            if normalized in SECRET_KEYS or any(normalized.endswith("_" + item) for item in SECRET_KEYS):
                _fail(key_path, "secret-bearing fields are forbidden")
            if normalized in INLINE_KEYS or normalized.endswith("_script") or normalized.endswith("_command"):
                _fail(key_path, "inline script/command fields are forbidden")
            if normalized == "latest":
                _fail(key_path, "latest references are forbidden")
            _scan_scalars(child, key_path, depth + 1, counter)
        return
    if isinstance(value, list):
        for index, child in enumerate(value):
            _scan_scalars(child, f"{path}[{index}]", depth + 1, counter)
        return
    if value is None or isinstance(value, (bool, int, float)):
        return
    if not isinstance(value, str):
        _fail(path, f"unsupported YAML scalar type {type(value).__name__}")
    lowered = value.strip().lower()
    if lowered == "latest" or re.search(r"[:/@]latest$", lowered):
        _fail(path, "floating latest reference is forbidden")
    if value.startswith(("/", "~/")) or re.match(r"^[A-Za-z]:[\\/]", value) or lowered.startswith("file://"):
        _fail(path, "absolute paths are forbidden; use logical mount keys and relative write roots")
    if "\n" in value or "\r" in value or re.search(r"(?m)^\s*#!", value):
        _fail(path, "multiline/inline script content is forbidden")
    if "-----BEGIN" in value or re.search(r"(?i)\bbearer\s+[A-Za-z0-9._~+/-]+", value):
        _fail(path, "secret-like scalar is forbidden")
    if re.search(r"\b(?:sk|ak)-[A-Za-z0-9]{16,}\b", value):
        _fail(path, "secret-like access key is forbidden")
    if "://" in value:
        try:
            parsed = urlsplit(value)
            if parsed.username is not None or parsed.password is not None:
                _fail(path, "credentials in URLs are forbidden")
        except ValueError as exc:
            raise RegistryError(f"{path}: invalid URL") from exc
    if path.lower().endswith("_bytes"):
        _fail(path, "byte quantities must be integer *_bytes values, not strings with units")


def _validate_special(special: Any, path: str) -> dict[str, Any]:
    item = _mapping(
        special,
        path,
        {"id", "kind", "enabled", "state"},
        {"endpoint", "restart", "meter", "note"},
    )
    _named(item["id"], path + ".id")
    _named(item["kind"], path + ".kind")
    if not isinstance(item["enabled"], bool):
        _fail(path + ".enabled", "expected boolean")
    _named(item["state"], path + ".state")
    if "endpoint" in item:
        endpoint = _string(item["endpoint"], path + ".endpoint")
        if not endpoint.startswith(("http://", "https://")):
            _fail(path + ".endpoint", "special endpoint must be HTTP(S)")
    if "restart" in item and item["restart"] != "operator_decision_required":
        _fail(path + ".restart", "only operator_decision_required is supported")
    if "note" in item:
        _string(item["note"], path + ".note")
    if "meter" in item:
        meter = _mapping(item["meter"], path + ".meter", {"kind", "remaining", "purpose"})
        if meter["kind"] != "Stock":
            _fail(path + ".meter.kind", "PoC-G uses Stock, never nested Concurrency, for concurrency ceilings")
        _integer(meter["remaining"], path + ".meter.remaining", 1)
        if meter["purpose"] != "concurrency_ceiling":
            _fail(path + ".meter.purpose", "expected concurrency_ceiling")
    return item


def _validate_node(node: Any, path: str) -> dict[str, Any]:
    item = _mapping(
        node,
        path,
        {"node_id", "admin_state", "roles", "trust_level", "endpoint", "capacity", "disks", "network", "special"},
    )
    node_id = _named(item["node_id"], path + ".node_id")
    if item["admin_state"] not in {"active", "drained", "disabled", "quarantined"}:
        _fail(path + ".admin_state", "unknown administrative state")
    if item["trust_level"] not in {"T0", "T1", "T2"}:
        _fail(path + ".trust_level", "expected T0, T1, or T2")
    if not isinstance(item["roles"], list) or not item["roles"]:
        _fail(path + ".roles", "expected non-empty list")
    for index, role in enumerate(item["roles"]):
        _named(role, f"{path}.roles[{index}]")

    endpoint = _mapping(item["endpoint"], path + ".endpoint", {"kind", "address"})
    if endpoint["kind"] not in {"local", "ssh"}:
        _fail(path + ".endpoint.kind", "expected local or ssh")
    _named(endpoint["address"], path + ".endpoint.address")
    if (node_id == "pandev") != (endpoint["kind"] == "local"):
        _fail(path + ".endpoint", "pandev is local; all other PoC-G nodes use ssh")

    capacity = _mapping(item["capacity"], path + ".capacity", {"cpu", "memory", "hard_commit"})
    cpu = _mapping(capacity["cpu"], path + ".capacity.cpu", {"total_cores", "allocatable"})
    _integer(cpu["total_cores"], path + ".capacity.cpu.total_cores", 1)
    if cpu["allocatable"] is not None:
        _fail(path + ".capacity.cpu.allocatable", "static declaration must remain null until dynamically derived")
    memory = _mapping(capacity["memory"], path + ".capacity.memory", {"total_bytes", "allocatable"})
    _integer(memory["total_bytes"], path + ".capacity.memory.total_bytes", 1)
    if memory["allocatable"] is not None:
        _fail(path + ".capacity.memory.allocatable", "static declaration must remain null until dynamically derived")
    hard = _mapping(
        capacity["hard_commit"],
        path + ".capacity.hard_commit",
        {"new_run_mechanism", "new_run_memory_bytes", "new_run_pids_limit", "existing_load"},
    )
    if hard["new_run_mechanism"] != "container":
        _fail(path + ".capacity.hard_commit.new_run_mechanism", "PoC-G admits only container hard commits for new runs")
    _integer(hard["new_run_memory_bytes"], path + ".capacity.hard_commit.new_run_memory_bytes", 1)
    _integer(hard["new_run_pids_limit"], path + ".capacity.hard_commit.new_run_pids_limit", 1)
    if hard["existing_load"] != "metered_only":
        _fail(path + ".capacity.hard_commit.existing_load", "existing load must be explicitly metered_only")

    if not isinstance(item["disks"], list) or not item["disks"]:
        _fail(path + ".disks", "expected non-empty list")
    mounts: set[str] = set()
    for index, raw_disk in enumerate(item["disks"]):
        disk_path = f"{path}.disks[{index}]"
        disk = _mapping(
            raw_disk,
            disk_path,
            {"mount", "write_roots", "max_post_used_pct", "min_post_free_bytes", "enforcement"},
        )
        mount = _named(disk["mount"], disk_path + ".mount")
        if mount in mounts:
            _fail(disk_path + ".mount", "duplicate logical mount")
        mounts.add(mount)
        if not isinstance(disk["write_roots"], list):
            _fail(disk_path + ".write_roots", "expected list")
        for root_index, root in enumerate(disk["write_roots"]):
            _relative_root(root, f"{disk_path}.write_roots[{root_index}]")
        _integer(disk["max_post_used_pct"], disk_path + ".max_post_used_pct", 1)
        if disk["max_post_used_pct"] > 100:
            _fail(disk_path + ".max_post_used_pct", "percentage cannot exceed 100")
        _integer(disk["min_post_free_bytes"], disk_path + ".min_post_free_bytes")
        if disk["enforcement"] != "preflight_metered":
            _fail(disk_path + ".enforcement", "expected preflight_metered")
    if "root" not in mounts:
        _fail(path + ".disks", "root mount must be independently declared")
    if node_id == "anolis" and not {"root", "data"}.issubset(mounts):
        _fail(path + ".disks", "anolis root and data mounts must be independent")

    network = _mapping(item["network"], path + ".network", {"tailnet_ip", "ssh_alias", "egress"})
    try:
        ipaddress.ip_address(network["tailnet_ip"])
    except ValueError:
        _fail(path + ".network.tailnet_ip", "invalid IP address")
    _named(network["ssh_alias"], path + ".network.ssh_alias")
    _named(network["egress"], path + ".network.egress")

    if not isinstance(item["special"], list):
        _fail(path + ".special", "expected list")
    special_ids: set[str] = set()
    for index, raw_special in enumerate(item["special"]):
        special = _validate_special(raw_special, f"{path}.special[{index}]")
        if special["id"] in special_ids:
            _fail(f"{path}.special[{index}].id", "duplicate special id")
        special_ids.add(special["id"])
    return item


def validate_document(document: Any) -> dict[str, Any]:
    _scan_scalars(document)
    root = _mapping(
        document,
        "$",
        {"schema_version", "registry_id", "registry_version", "recipe_namespace", "as_of", "nodes"},
    )
    if root["schema_version"] != "af.resources/v1":
        _fail("$.schema_version", "expected af.resources/v1")
    if root["registry_id"] != "machines" or root["registry_version"] != "v1":
        _fail("$", "expected machines/v1 registry identity")
    _named(root["recipe_namespace"], "$.recipe_namespace")
    as_of = _string(root["as_of"], "$.as_of")
    try:
        parsed = dt.datetime.fromisoformat(as_of.replace("Z", "+00:00"))
    except ValueError:
        _fail("$.as_of", "expected ISO-8601 timestamp")
    if parsed.tzinfo is None:
        _fail("$.as_of", "timestamp must include timezone")
    if not isinstance(root["nodes"], list):
        _fail("$.nodes", "expected list")
    nodes = [_validate_node(node, f"$.nodes[{index}]") for index, node in enumerate(root["nodes"])]
    node_ids = [node["node_id"] for node in nodes]
    if len(set(node_ids)) != len(node_ids):
        _fail("$.nodes", "duplicate node_id")
    if set(node_ids) != EXPECTED_NODES:
        _fail("$.nodes", f"expected exactly {sorted(EXPECTED_NODES)}")

    specials = {(node["node_id"], special["id"]): special for node in nodes for special in node["special"]}
    for special_id in ("vendor_sandbox", "step"):
        special = specials.get(("pandev", special_id))
        if special is None:
            _fail("$.nodes", f"pandev must declare {special_id}")
        if special["enabled"] is not False or special.get("restart") != "operator_decision_required":
            _fail("$.nodes", f"{special_id} must be disabled and require operator restart adjudication")
    kimi = specials.get(("pandev", "kimi"))
    if kimi is None or kimi["state"] != "restricted":
        _fail("$.nodes", "kimi must be declared state=restricted")
    ollama = specials.get(("pandev", "ollama"))
    if ollama is None or ollama.get("endpoint") != "http://127.0.0.1:11434" or not ollama["enabled"]:
        _fail("$.nodes", "ollama must be enabled on pandev at http://127.0.0.1:11434")
    if any(special["id"] == "ollama" for node in nodes if node["node_id"] == "anolis" for special in node["special"]):
        _fail("$.nodes", "anolis must not claim the corrected local ollama supply")
    return root


def load_registry(path: str | Path) -> dict[str, Any]:
    source = Path(path)
    raw = source.read_bytes()
    if len(raw) > MAX_DOCUMENT_BYTES:
        raise RegistryError("recipe exceeds 1 MiB restricted-parser limit")
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise RegistryError("recipe must be UTF-8") from exc
    return validate_document(_restricted_load(text))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("registry", help="machines.v1.yaml path")
    args = parser.parse_args(argv)
    try:
        document = load_registry(args.registry)
    except (OSError, RegistryError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    print(f"OK {args.registry} nodes={len(document['nodes'])} schema={document['schema_version']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
