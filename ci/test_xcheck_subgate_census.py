#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""共宿槽子门字母缀计数·门侧新判据测试（台账 RT-E1 清偿批，2026-11-04）。

只新增断言：被测门 = ci/xcheck.py 的 count_run_sh_subgates /
check_cohosted_subgates（直调臂，不跑全量 main——外派机无工作区侧文书
（AGENT.md/方案/四观察期），全量跑会因缺件红，与 census 判据无关）。
夹具 = 仓内正本 ci/run.sh 的临时副本变异，不触真树任何文件。覆盖：
  正例    —— 正本 run.sh census == CO_HOSTED_SUBGATES（13→[a,b,c]）→ PASS；
  负例①  —— 删 step 13b/13c 执行行（留 13a 与槽登记行）→ 硬红 且点名「缺字母缀」
             （修前该形态 xcheck 硬项全绿、step12 逐键复算被同批改账吸收——
             RT-E1「摘门保牌」残留洞复现面）；
  负例②  —— 整槽摘除 step13（登记行+执行行一并删，即「整槽连摘」最强形态）→
             硬红 且点名「整槽缺席」；
  负例③  —— 加 `step 13d "…"` 未登记字母缀 → 硬红 且点名「多字母缀」；
  负例④  —— 新字母缀共宿槽（step 5a）不在登记表 → 硬红 且点名「未登记共宿槽」；
  负例⑤  —— run.sh 缺席 → fail-closed 硬红；
  单元    —— count_run_sh_subgates 映射正确；注释行/`step() {` 函数定义行/
             登记行（echo "== step13 …"）不算字母缀执行事实。
跑法：python3 -I ci/test_xcheck_subgate_census.py
"""
import importlib.util
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent          # ci/
REPO = HERE.parent
_spec = importlib.util.spec_from_file_location("xcheck_under_test", HERE / "xcheck.py")
xc = importlib.util.module_from_spec(_spec)
sys.modules["xcheck_under_test"] = xc
_spec.loader.exec_module(xc)

RUN_SH = (REPO / "ci" / "run.sh").read_text(encoding="utf-8")


def _drop(lines, prefixes=(), contains=()):
    return [ln for ln in lines
            if not ln.startswith(prefixes) and not (contains and any(c in ln for c in contains))]


def _mutated_run_sh(tmp, *, drop_13bc=False, drop_slot13=False, extra_lines=()):
    lines = RUN_SH.splitlines(keepends=True)
    if drop_13bc:   # 摘门保牌：删 13b/13c 调用行+续行，留 13a 与槽登记行
        lines = _drop(lines,
                      prefixes=("step 13b \"", "step 13c \""),
                      contains=("arm_wiring_check.py --check", "budget_cast_lint.py --root"))
    if drop_slot13:  # 整槽连摘：登记行+13a/13b/13c+续行一并删
        lines = _drop(lines,
                      prefixes=("echo \"== step13", "step 13a \"", "step 13b \"", "step 13c \""),
                      contains=("fix_scan.py --run", "arm_wiring_check.py --check",
                                "budget_cast_lint.py --root"))
    if extra_lines:
        lines = lines + list(extra_lines)
    p = Path(tmp) / "run.sh"
    p.write_text("".join(lines), encoding="utf-8")
    return p


def _verdict(run_sh_path):
    """直调臂并返回 (ok, 细节行)——每例前清 HARD/SOFT/INFO 全局账。"""
    xc.HARD.clear(); xc.SOFT.clear(); xc.INFO_LINES.clear()
    xc.check_cohosted_subgates(Path(run_sh_path))
    hard = [x for x in xc.HARD if x[0] == "gates-subgate-census"]
    assert len(hard) == 1, f"census 臂应恰登记 1 条硬项，实得 {len(hard)}"
    return hard[0][1], hard[0][2]


class SubgateCensusTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="xcheck-census-t-")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_control_pristine_run_sh_passes(self):
        ok, d = _verdict(REPO / "ci" / "run.sh")
        self.assertTrue(ok, "\n".join(d))
        self.assertTrue(any("13" in ln and "一致" in ln for ln in d), d)

    def test_drop_13bc_leaving_banner_goes_red(self):
        ok, d = _verdict(_mutated_run_sh(self.tmp, drop_13bc=True))
        self.assertFalse(ok)
        self.assertTrue(any("缺字母缀 ['b', 'c']" in ln for ln in d), d)

    def test_whole_slot13_removal_goes_red(self):
        ok, d = _verdict(_mutated_run_sh(self.tmp, drop_slot13=True))
        self.assertFalse(ok)
        self.assertTrue(any("整槽缺席" in ln for ln in d), d)

    def test_unregistered_letter_suffix_goes_red(self):
        ok, d = _verdict(_mutated_run_sh(
            self.tmp, extra_lines=('step 13d "未登记子门" -- true\n',)))
        self.assertFalse(ok)
        self.assertTrue(any("多字母缀 ['d']" in ln for ln in d), d)

    def test_unregistered_cohosted_slot_goes_red(self):
        ok, d = _verdict(_mutated_run_sh(
            self.tmp, extra_lines=('step 5a "未登记共宿槽" -- true\n',)))
        self.assertFalse(ok)
        self.assertTrue(any("未登记共宿槽 [5]" in ln for ln in d), d)

    def test_missing_run_sh_fail_closed(self):
        ok, d = _verdict(Path(self.tmp) / "nope.sh")
        self.assertFalse(ok)
        self.assertTrue(any("fail-closed" in ln for ln in d), d)

    def test_count_run_sh_subgates_unit(self):
        text = (
            'step 13a "甲" -- true\n'          # 执行事实（字母缀 a）
            'step 13b "乙" -- true\n'          # 执行事实（字母缀 b）
            'step 1 "无缀" -- true\n'          # 无缀 → 不入 census
            '# step 13z "注释样例" -- true\n'  # 注释 → 不算
            'step() { # step 13q "函数定义内样例"\n'  # 函数定义行 → 不算
            'echo "== step13 横切与对称"\n'    # 登记行 → 不算执行事实
            '  bash -c "cd \'$AF_ROOT\' && python3 ci/arm_wiring_check.py"\n'  # 续行 → 不算
        )
        self.assertEqual(xc.count_run_sh_subgates(text), {13: ["a", "b"]})


if __name__ == "__main__":
    unittest.main(verbosity=2)
