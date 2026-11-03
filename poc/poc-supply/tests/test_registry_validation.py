#!/usr/bin/env python3
from __future__ import annotations

import copy
import subprocess
import sys
import unittest
from pathlib import Path

POC_DIR = Path(__file__).resolve().parents[1]
REPO_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(POC_DIR))

from validate_machines import RegistryError, load_registry, validate_document  # noqa: E402

REGISTRY = REPO_ROOT / "af/data/recipes/user-01/machines.v1.yaml"
FIXTURES = POC_DIR / "tests/fixtures/registry_invalid"
VALIDATOR = POC_DIR / "validate_machines.py"


class RegistryValidationTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.valid = load_registry(REGISTRY)

    def test_valid_registry_and_cli(self) -> None:
        self.assertEqual({n["node_id"] for n in self.valid["nodes"]}, {"pandev", "gov", "anolis", "mingda"})
        result = subprocess.run(
            [sys.executable, "-I", str(VALIDATOR), str(REGISTRY)],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("nodes=4", result.stdout)

    def test_every_negative_file_is_red(self) -> None:
        fixtures = sorted(FIXTURES.glob("*.yaml"))
        self.assertGreaterEqual(len(fixtures), 7)
        for fixture in fixtures:
            with self.subTest(fixture=fixture.name):
                result = subprocess.run(
                    [sys.executable, "-I", str(VALIDATOR), str(fixture)],
                    text=True,
                    capture_output=True,
                    check=False,
                )
                self.assertNotEqual(result.returncode, 0, fixture.name)
                self.assertIn("ERROR:", result.stderr)

    def test_semantic_negatives_are_red(self) -> None:
        def node(document: dict, node_id: str) -> dict:
            return next(item for item in document["nodes"] if item["node_id"] == node_id)

        cases = {
            "unknown_nested_field": lambda d: node(d, "pandev")["capacity"].update(unknown=True),
            "missing_required_field": lambda d: node(d, "gov").pop("capacity"),
            "allocatable_must_be_null": lambda d: node(d, "pandev")["capacity"]["memory"].update(allocatable=1),
            "bytes_cannot_be_unit_string": lambda d: node(d, "pandev")["capacity"]["memory"].update(total_bytes="20G"),
            "concurrency_meter_forbidden": lambda d: node(d, "pandev")["special"][0]["meter"].update(kind="Concurrency"),
            "anolis_data_mount_required": lambda d: node(d, "anolis").update(
                disks=[disk for disk in node(d, "anolis")["disks"] if disk["mount"] != "data"]
            ),
            "absolute_write_root_forbidden": lambda d: node(d, "pandev")["disks"][0]["write_roots"].append("/tmp"),
            "secret_nested_forbidden": lambda d: node(d, "pandev")["network"].update(api_token="secret"),
            "local_ollama_endpoint_pinned": lambda d: node(d, "pandev")["special"][0].update(
                endpoint="http://100.64.0.7:11434"
            ),
            "sandbox_must_remain_disabled": lambda d: node(d, "pandev")["special"][1].update(enabled=True),
            "kimi_must_be_restricted": lambda d: node(d, "pandev")["special"][3].update(state="in_service"),
        }
        for name, mutate in cases.items():
            with self.subTest(case=name):
                document = copy.deepcopy(self.valid)
                mutate(document)
                with self.assertRaises(RegistryError, msg=name):
                    validate_document(document)


if __name__ == "__main__":
    unittest.main(verbosity=2)
