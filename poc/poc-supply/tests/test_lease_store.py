#!/usr/bin/env python3
from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

POC_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(POC_DIR))

from disk_post_used_preflight import active_reserved_bytes  # noqa: E402
from lease_store import LeaseError, LeaseStore  # noqa: E402


def profile(node_id: str) -> dict:
    return {"schema_version": "af.resources/v1", "recipe_namespace": "user-01", "node_id": node_id}


class LeaseStoreTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.db_path = Path(self.temporary.name) / "leases.db"
        self.store = LeaseStore(self.db_path)

    def tearDown(self) -> None:
        self.store.close()
        self.temporary.cleanup()

    def acquire(self, lease_id: str, node: str, mount: str = "root", interval: str = "worktree:test", ttl: int = 100) -> dict:
        return self.store.acquire(lease_id, f"{node}/{mount}", interval, profile(node), "pocg", 1024, ttl, 1000)

    def test_local_and_anolis_acquire_return_tuple(self) -> None:
        for lease_id, node in (("pandev-lease", "pandev"), ("anolis-lease", "anolis")):
            with self.subTest(node=node):
                lease = self.acquire(lease_id, node, interval=f"worktree:{node}")
                self.assertEqual(lease["resource"], f"{node}/root")
                self.assertEqual(lease["work_interval"], f"worktree:{node}")
                self.assertEqual(lease["profile"]["node_id"], node)
                self.assertEqual(lease["ttl_ms"], 100)
                self.assertEqual(lease["lease_until_ms"], 1100)
                self.assertEqual(lease["state"], "active")
                self.assertFalse(self.store.return_lease(lease_id, "wrong-holder", 1050))
                self.assertTrue(self.store.return_lease(lease_id, "pocg", 1050))
                self.assertEqual(self.store.get(lease_id)["state"], "returned")
        self.assertEqual(active_reserved_bytes(self.db_path, "pandev/root", 1050), 0)
        self.assertEqual(active_reserved_bytes(self.db_path, "anolis/root", 1050), 0)

    def test_active_interval_is_unique(self) -> None:
        self.acquire("first", "anolis")
        with self.assertRaises(LeaseError):
            self.acquire("second", "anolis")

    def test_heartbeat_is_holder_and_expiry_guarded(self) -> None:
        self.acquire("heartbeat", "pandev")
        self.assertFalse(self.store.heartbeat("heartbeat", "wrong-holder", 100, 1050))
        self.assertTrue(self.store.heartbeat("heartbeat", "pocg", 200, 1050))
        self.assertEqual(self.store.get("heartbeat")["lease_until_ms"], 1250)
        self.assertFalse(self.store.heartbeat("heartbeat", "pocg", 200, 1250))

    def test_reap_expires_and_frees_interval_and_reservation(self) -> None:
        self.acquire("expiry", "anolis", ttl=10)
        self.assertEqual(self.store.reap(1009), [])
        self.assertEqual(self.store.reap(1010), [])
        self.assertEqual(self.store.reap(1011), ["expiry"])
        self.assertEqual(self.store.get("expiry")["state"], "expired")
        self.assertEqual(active_reserved_bytes(self.db_path, "anolis/root", 1011), 0)
        replacement = self.store.acquire(
            "replacement",
            "anolis/root",
            "worktree:test",
            profile("anolis"),
            "pocg",
            2048,
            100,
            1011,
        )
        self.assertEqual(replacement["state"], "active")

    def test_acquire_lazily_reaps_expired_lease(self) -> None:
        self.acquire("lazy-old", "pandev", ttl=10)
        replacement = self.store.acquire(
            "lazy-new",
            "pandev/root",
            "worktree:test",
            profile("pandev"),
            "pocg",
            1024,
            100,
            1011,
        )
        self.assertEqual(replacement["state"], "active")
        self.assertEqual(self.store.get("lazy-old")["state"], "expired")


if __name__ == "__main__":
    unittest.main(verbosity=2)
