#!/usr/bin/env python3
from __future__ import annotations

import json
import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path

POC_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(POC_DIR))

from disk_post_used_preflight import (  # noqa: E402
    PreflightError,
    active_reserved_bytes,
    evaluate_post_watermark,
)

FIXTURE = json.loads((POC_DIR / "tests/fixtures/post_used_cases.json").read_text(encoding="utf-8"))


class DiskPreflightTest(unittest.TestCase):
    def test_required_and_threshold_fixtures(self) -> None:
        for case in FIXTURE["cases"]:
            with self.subTest(case=case["name"]):
                verdict = evaluate_post_watermark(
                    {
                        "min_post_free_bytes": case["min_post_free_bytes"],
                        "max_post_used_pct": case["max_post_used_pct"],
                    },
                    {
                        "total_bytes": case["total_bytes"],
                        "available_bytes": case["available_bytes"],
                    },
                    case["lease_reserved_bytes"],
                    case["request_bytes"],
                )
                self.assertEqual(verdict["admit"], case["expected_admit"])
                self.assertEqual(verdict["post_free_bytes"], case["expected_post_free_bytes"])
                self.assertEqual([reason["code"] for reason in verdict["reasons"]], case["expected_reasons"])

    def test_named_required_rejection_is_exact(self) -> None:
        case = FIXTURE["cases"][0]
        self.assertEqual(case["name"], "required_reject_20g_avail_5g_reserved_10g_new_10g_min")
        verdict = evaluate_post_watermark(
            {"min_post_free_bytes": 10_000_000_000, "max_post_used_pct": 100},
            {"total_bytes": 100_000_000_000, "available_bytes": 20_000_000_000},
            5_000_000_000,
            10_000_000_000,
        )
        self.assertFalse(verdict["admit"])
        self.assertEqual(verdict["post_free_bytes"], 5_000_000_000)
        self.assertEqual(verdict["reasons"][0]["code"], "MIN_POST_FREE_BYTES")

    def test_data_free_space_cannot_cover_root(self) -> None:
        case = FIXTURE["independent_mounts"]
        policy = {
            "min_post_free_bytes": case["min_post_free_bytes"],
            "max_post_used_pct": case["max_post_used_pct"],
        }
        root = evaluate_post_watermark(
            policy,
            {"total_bytes": case["total_bytes"], "available_bytes": case["root_available_bytes"]},
            0,
            case["request_bytes"],
        )
        data = evaluate_post_watermark(
            policy,
            {"total_bytes": case["total_bytes"], "available_bytes": case["data_available_bytes"]},
            0,
            case["request_bytes"],
        )
        self.assertFalse(root["admit"])
        self.assertEqual(root["post_free_bytes"], case["expected_root_post_free_bytes"])
        self.assertTrue(data["admit"])
        self.assertEqual(data["post_free_bytes"], case["expected_data_post_free_bytes"])

    def test_reservations_are_exact_resource_and_unexpired_only(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            db_path = Path(directory) / "leases.db"
            with sqlite3.connect(db_path) as connection:
                connection.execute(
                    "CREATE TABLE resource_lease "
                    "(resource TEXT, state TEXT, lease_until_ms INTEGER, reserved_bytes INTEGER)"
                )
                connection.executemany(
                    "INSERT INTO resource_lease VALUES (?, ?, ?, ?)",
                    [
                        ("anolis/root", "active", 2000, 5_000_000_000),
                        ("anolis/root", "active", 999, 50_000_000_000),
                        ("anolis/root", "returned", 2000, 50_000_000_000),
                        ("anolis/data", "active", 2000, 90_000_000_000),
                    ],
                )
            self.assertEqual(active_reserved_bytes(db_path, "anolis/root", 1000), 5_000_000_000)

    def test_bad_reading_fails_closed(self) -> None:
        with self.assertRaises(PreflightError):
            evaluate_post_watermark(
                {"min_post_free_bytes": 0, "max_post_used_pct": 90},
                {"total_bytes": 100, "available_bytes": 101},
                0,
                0,
            )


if __name__ == "__main__":
    unittest.main(verbosity=2)
