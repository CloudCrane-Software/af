#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""在飞工作树登记面·门侧新判据测试（2026-11-04 批）。

只新增断言：不改任何既有断言/golden 快照/clippy 基线；被测门 = ci/fix_verify.py
（子进程全量跑，行级断言；夹具 = 临时 git 仓 + 临时租约面，不触真仓任何树）。
覆盖：
  正例    —— 在飞树 + 活租约（租约面背书成立）→ 漏登记红不触发；
  负例①  —— 在飞树无登记 → 既有漏登记红腿原样（文案逐字）；
  负例②  —— 登记有行但租约已过期 → 红 且点名「超期未回收」；
  负例③  —— 登记有行但租约 id 在租约面不存在 → 红 且点名「超期未回收」；
  负例④  —— duty/archived 既有语义不变（归档行无理由仍红、带门码无钉仍红）；
  防伪自测 —— 抄他人活租约字段到别的树槽位 → 槽位对不上红；租约面缺席/不可读
             → fail-closed 红；登记行格式错 → 红 且不豁免。

跑法：python3 -I ci/test_fix_verify_inflight.py
"""
import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent          # ci/
GATE = HERE / "fix_verify.py"
LEASE_STORE_DIR = HERE.parent / "poc" / "poc-supply"
sys.path.insert(0, str(LEASE_STORE_DIR))
import lease_store  # noqa: E402  （既有租约面，禁新造租约机制——直接复用）


def _now_ms():
    return time.time_ns() // 1_000_000


class InflightGateTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="fv-inflight-t-")
        self.addCleanup(shutil.rmtree, self.tmp, True)
        # 夹具仓：单分支 main 单提交，主检出即被测树
        self.repo = os.path.join(self.tmp, "repo")
        os.makedirs(self.repo)
        self._git("init", "-b", "main")
        self._git("config", "user.email", "t@example.invalid")
        self._git("config", "user.name", "t")
        Path(self.repo, "README.md").write_text("fixture\n", encoding="utf-8")
        self._git("add", ".")
        self._git("commit", "-m", "fixture")
        self.db = os.path.join(self.tmp, "leases.db")
        self.inflight = os.path.join(self.tmp, "inflight-worktrees.list")
        self.roster = os.path.join(self.tmp, "roster.duty")

    # ---------- 夹具工具 ----------
    def _git(self, *args):
        subprocess.run(["git", "-C", self.repo] + list(args), check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    def _acquire(self, lease_id, branch, holder="worker-a", resource="pandev/tmp",
                 ttl_ms=600000, now_ms=0, interval=None):
        """经既有 lease_store.acquire 领真租约（profile 满足 af.resources/v1）。"""
        with lease_store.LeaseStore(self.db) as s:
            return s.acquire(lease_id, resource,
                             interval if interval is not None else "worktree:" + branch,
                             {"schema_version": "af.resources/v1",
                              "node_id": resource.split("/", 1)[0]},
                             holder, 0, ttl_ms, now_ms)

    def _write_inflight(self, branch, lease_id, until_ms, holder="worker-a",
                        resource="pandev/tmp", batch="batch-20261104-roster",
                        why="在飞批次临时树，租约期内登记"):
        at_ms = _now_ms()
        line = " | ".join(["inflight", branch, self.repo, batch, lease_id,
                           str(until_ms), str(at_ms), holder, resource, why])
        Path(self.inflight).write_text(
            "# 在飞登记面（测试夹具）\n%s\n" % line, encoding="utf-8")
        return line

    def _run_gate(self, with_roster=False, roster_text=None):
        cmd = [sys.executable, "-I", str(GATE), "--repo", self.repo,
               "--base-ref", "main", "--inflight", self.inflight,
               "--leases-db", self.db]
        if with_roster:
            Path(self.roster).write_text(roster_text or "", encoding="utf-8")
            cmd += ["--roster", self.roster]
        p = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                           text=True)
        return p.returncode, p.stdout

    def _fails(self, out):
        return [ln for ln in out.splitlines() if ln.startswith("[FAIL]")]

    def _fail_with(self, out, needle):
        hits = [ln for ln in self._fails(out) if needle in ln]
        self.assertTrue(hits, "应有一行 [FAIL] 含 %r\n实际输出：\n%s" % (needle, out))
        return hits[0]

    def _no_fail_with(self, out, needle):
        hits = [ln for ln in self._fails(out) if needle in ln]
        self.assertEqual(hits, [], "不应有 [FAIL] 含 %r\n实际输出：\n%s" % (needle, out))

    # ---------- 正例 ----------
    def test_正例_活租约背书_漏登记红不触发(self):
        lease = self._acquire("wt-lease-pos-01", "main", ttl_ms=600000)
        self._write_inflight("main", "wt-lease-pos-01", lease["lease_until_ms"])
        rc, out = self._run_gate()
        # 漏登记红不触发（既有红腿文案对本树缺席）
        self._no_fail_with(out, "工作树漏登记名册：%s（分支 main）" % self.repo)
        # 背书成立的绿行在场
        self.assertTrue(any(ln.startswith("[PASS] [main] 在飞登记背书成立")
                            and "wt-lease-pos-01" in ln for ln in out.splitlines()),
                        "应有 [PASS] [main] 在飞登记背书成立…行\n输出：\n%s" % out)
        self.assertNotIn("超期未回收", out)

    # ---------- 负例① ----------
    def test_负例1_无登记_既有漏登记红腿原样(self):
        Path(self.inflight).write_text("# 空面（只注释）\n", encoding="utf-8")
        rc, out = self._run_gate()
        self._fail_with(out, "工作树漏登记名册：%s（分支 main）——缩名册即旁路，红"
                        % self.repo)

    # ---------- 负例② ----------
    def test_负例2_租约已过期_红并点名超期未回收(self):
        # 领一张已过期租约：leased_at=now-120s、ttl=60s → until=now-60s（不睡眠）
        lease = self._acquire("wt-lease-exp-01", "main", ttl_ms=60000,
                              now_ms=_now_ms() - 120_000)
        self._write_inflight("main", "wt-lease-exp-01", lease["lease_until_ms"])
        rc, out = self._run_gate()
        line = self._fail_with(out, "超期未回收")
        self.assertIn("已到期", line)
        # 背书不成立时换点名红，既有漏登记文案不再出现（判据②/④各走各支）
        self._no_fail_with(out, "工作树漏登记名册：%s（分支 main）" % self.repo)

    # ---------- 负例③ ----------
    def test_负例3_租约id不存在_红并点名超期未回收(self):
        # 先领一张无关真租约把租约面物化（面可读），登记行再引用不存在的 id
        self._acquire("wt-lease-unrelated-01", "other-tree", holder="worker-b",
                      interval="worktree:other-tree")
        self._write_inflight("main", "wt-lease-ghost-01", _now_ms() + 600_000)
        rc, out = self._run_gate()
        line = self._fail_with(out, "超期未回收")
        self.assertIn("wt-lease-ghost-01", line)
        self.assertIn("不存在", line)

    # ---------- 负例④（duty/archived 既有语义不变） ----------
    def test_负例4a_归档行无理由_仍红(self):
        rc, out = self._run_gate(with_roster=True,
                                 roster_text="main | archived | NONE | \n")
        self._fail_with(out, "名册理由为空（缩名单册=旁路面）")

    def test_负例4b_树带门码而归档行无钉_仍红_且名册先行不走在飞判定(self):
        # 夹具树带 ci/xcheck.py（内容任意，archived 分支按盘面 sha256 实测）
        os.makedirs(os.path.join(self.repo, "ci"), exist_ok=True)
        Path(self.repo, "ci", "xcheck.py").write_text("# fixture gate\n", encoding="utf-8")
        # 名册 archived 行（有理由、无钉）+ 在飞登记行（租约已过期）同面在场：
        # 判定顺序①名册先行 → 按 archived 既有语义判「未钉即红」，在飞过期判定
        # 不参与（无「超期未回收」行），漏登记红也不触发（名册有行）。
        self._acquire("wt-lease-exp-4b", "main", ttl_ms=60000,
                      now_ms=_now_ms() - 120_000)
        self._write_inflight("main", "wt-lease-exp-4b",
                             _now_ms() - 60_000, why="本行应被名册先行屏蔽")
        rc, out = self._run_gate(
            with_roster=True, roster_text="main | archived | NONE | 存档树登记\n")
        self._fail_with(out, "但名册未钉")
        self._no_fail_with(out, "超期未回收")
        self._no_fail_with(out, "工作树漏登记名册：%s（分支 main）" % self.repo)

    # ---------- 防伪自测（③：豁免必须由租约面背书，裸登记/抄租约均不豁免） ----------
    def test_防伪_抄他人活租约到别的树槽位_槽位对不上红(self):
        # worker-b 给「别的树」领了一张真实活租约（槽位 worktree:other-tree）；
        # 伪造者把该租约的全部字段（id/until/holder/resource）原样抄进本树（main）
        # 的登记行——租约真实存在且未过期，但槽位不是本树 → 必须不豁免。
        lease = self._acquire("wt-lease-borrow-01", "other-tree", holder="worker-b",
                              interval="worktree:other-tree")
        self._write_inflight("main", "wt-lease-borrow-01", lease["lease_until_ms"],
                             holder="worker-b")
        rc, out = self._run_gate()
        line = self._fail_with(out, "超期未回收")
        self.assertIn("worktree:main", line)   # 槽位对不上：租约钉在 other-tree
        self.assertIn("worktree:other-tree", line)

    def test_防伪_租约面缺席_fail_closed红(self):
        self._write_inflight("main", "wt-lease-nodb-01", _now_ms() + 600_000)
        missing_db = os.path.join(self.tmp, "no-such", "leases.db")
        cmd = [sys.executable, "-I", str(GATE), "--repo", self.repo,
               "--base-ref", "main", "--inflight", self.inflight,
               "--leases-db", missing_db]
        p = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                           text=True)
        line = self._fail_with(p.stdout, "超期未回收")
        self.assertIn("租约面不可读", line)

    def test_登记行格式错_红且不豁免(self):
        # 9 段坏行（少 resource 段）：fail-closed——坏行红、且不能据此豁免本树
        bad = "inflight | main | %s | batch-x | wt-lease-bad-01 | %d | %d | worker-a | 理由" % (
            self.repo, _now_ms() + 600_000, _now_ms())
        Path(self.inflight).write_text(bad + "\n", encoding="utf-8")
        rc, out = self._run_gate()
        self._fail_with(out, "在飞登记行格式错")
        self._fail_with(out, "工作树漏登记名册：%s（分支 main）" % self.repo)


if __name__ == "__main__":
    unittest.main(verbosity=2)
