#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""门输入钉面双向零差·门侧新判据测试（台账 RT-E6 清偿批，2026-11-04）。

只新增断言：被测门 = ci/gate_inputs_check.sh（子进程真跑，--root 指临时根，
不触真树任何文件）。CONSUMED = 逐件审计后的「run.sh 管道被消费判据正本」清单
（消费者+钉层逐件注记；增删判据正本须同批改本表 + GATE_INPUTS + update 重钉，
与 subgates.list 名册同一纪律）。覆盖：
  双向零差 A（无外挂）—— gate-inputs.sha256 每条钉行 ∈ CONSUMED（钉面不得虚胖）；
  双向零差 B（无漏）  —— CONSUMED 每件的注记钉层实钉在场（摘牌通道=零）；
  清单一致 C          —— GATE_INPUTS 数组与钉文件集合一致（增删=同批评审面）；
  自证咬人 D          —— 临时根篡改任一钉内正本（含本批新钉 9a/9b 两件）而不重钉
                          → check 以 exit 1 红且点名该件（改判据正本必须同批重钉）；
  对照 E              —— 未篡改临时根 check 过（不误红）。
跑法：python3 -I ci/test_gate_inputs_rte6_pins.py
"""
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent          # ci/
REPO = HERE.parent
SCRIPT = HERE / "gate_inputs_check.sh"
PIN = HERE / "baselines" / "gate-inputs.sha256"

# run.sh 管道被消费判据正本 →（消费者，钉层）。钉层取值：gate-inputs（族钉）/
# mutations-registry（专用钉）——clippy-baseline.txt 双钉（族钉+clippy-baseline.sha256）。
CONSUMED = {
    "ci/neutrality-expected-fail.txt":     ("step8 neutrality_gate.sh:42 + xcheck ③ 词表对账", "gate-inputs"),
    "ci/neutrality-wordlist.txt":          ("step8 neutrality_gate.sh:43", "gate-inputs"),
    "ci/neutrality-golden-registry.tsv":   ("step8 neutrality_gate.sh:44 golden 闭集登记", "gate-inputs"),
    "ci/scorechain-manifest.txt":          ("step1 k5-path-lint.py:60 判分链逐件 sha256 登记表", "gate-inputs"),
    "ci/clippy-baseline.txt":              ("step2 clippy_gate.py:28 警告基线（另专用钉 clippy-baseline.sha256 双钉）", "gate-inputs"),
    "ci/arm-wiring.yaml":                  ("step13b arm_wiring_check.py:41 三臂防线", "gate-inputs"),
    "ci/branches.yaml":                    ("step14a branches_check.py 分支覆盖夹具", "gate-inputs"),
    "ci/criteria.yaml":                    ("step14c criteria_check.py 六要素 schema", "gate-inputs"),
    "ci/fix-scan.yaml":                    ("step13a fix_scan.py:47 族横切扫描", "gate-inputs"),
    "ci/budget-cast-whitelist.json":       ("step13c budget_cast_lint.py:196 cast 白名单", "gate-inputs"),
    "ci/debt-register.md":                 ("step13b/13c 债号在册判定（arm_wiring_check.py:42 等）", "gate-inputs"),
    "ci/subgates.list":                    ("step14 子门名册（xcheck 子门级断言 + step12 ④名册绑定）", "gate-inputs"),
    "ci/baselines/kani-inv-harness.manifest":          ("step9 9b Kani harness 清单（baseline_calc.sh verify-kani + run.sh 9b count 行）", "gate-inputs"),
    "proofs/rocq-规范库-v0/theorems-manifest.txt":     ("step9 9a 定理清单（run.sh 9a-2 名集+陈述哈希对账 + baseline_calc.sh verify-rocq）", "gate-inputs"),
    "mutations/registry.yaml":             ("step0 mutation_runner.sh check-registry 探针清单", "mutations-registry"),
}


def pin_entries(pin_path=PIN):
    """钉行 → {相对路径: sha}。兼容两形态：gate-inputs 两列（sha  路径）与
    mutations-registry 三列（sha  计数  路径）——统一取行内最末字段为路径
    （本仓钉面路径均无空格；sha 取首字段）。"""
    out = {}
    for ln in pin_path.read_text(encoding="utf-8").splitlines():
        ln = ln.strip()
        if not ln or ln.startswith("#"):
            continue
        fields = ln.split()
        if len(fields) < 2:
            continue
        out[fields[-1]] = fields[0]
    return out


def gate_inputs_array():
    m = re.search(r"GATE_INPUTS=\((.*?)^\)", SCRIPT.read_text(encoding="utf-8"),
                  re.M | re.S)
    assert m, "GATE_INPUTS 数组未解析到"
    return re.findall(r'"([^"]+)"', m.group(1))


def make_tmp_root(tmp):
    """临时根：复制全部被消费正本 + 钉文件（保持相对路径），供 --root 真跑。"""
    root = Path(tmp) / "root"
    for rel in CONSUMED:
        dst = root / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(REPO / rel, dst)
    (root / "ci" / "baselines").mkdir(parents=True, exist_ok=True)
    shutil.copy2(PIN, root / "ci" / "baselines" / "gate-inputs.sha256")
    return root


def run_check(root):
    return subprocess.run(["bash", str(SCRIPT), "check", "--root", str(root)],
                          capture_output=True, text=True, timeout=120)


class GateInputsZeroDiffTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="gate-inputs-t-")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_a_pin_has_no_stray_entries(self):
        stray = sorted(set(pin_entries()) - set(CONSUMED))
        self.assertEqual(stray, [], f"钉面外挂（钉了未被消费）：{stray}")



    def test_b_consumed_all_pinned(self):
        missing = []
        for rel, (_consumer, layer) in CONSUMED.items():
            if layer == "gate-inputs" and rel not in pin_entries():
                missing.append(rel)
            if layer == "mutations-registry" and rel not in pin_entries(
                    HERE / "baselines" / "mutations-registry.sha256"):
                missing.append(rel)
        self.assertEqual(missing, [], f"被消费而未钉（摘牌通道）：{missing}")

    def test_c_array_equals_pin_file(self):
        arr, pins = set(gate_inputs_array()), set(pin_entries())
        self.assertEqual(sorted(arr - pins), [], "GATE_INPUTS 清单有未重钉件（须 update 同批）")
        self.assertEqual(sorted(pins - arr), [], "钉文件有清单外条目（禁手改，须 update 重钉）")

    def test_d_tamper_without_repin_bites(self):
        for rel in ("ci/baselines/kani-inv-harness.manifest",       # 本批新钉 9b
                    "proofs/rocq-规范库-v0/theorems-manifest.txt",  # 本批新钉 9a
                    "ci/debt-register.md"):                          # 既有件（c7f48c4 事故同型）
            with self.subTest(tampered=rel):
                root = make_tmp_root(self.tmp)
                p = root / rel
                p.write_text(p.read_text(encoding="utf-8") + "\n<!-- 篡改探针：改判据正本不重钉 -->\n",
                             encoding="utf-8")
                r = run_check(root)
                self.assertEqual(r.returncode, 1, f"{rel} 篡改未重钉应 exit 1：{r.stdout}{r.stderr}")
                self.assertIn(rel, r.stdout + r.stderr)

    def test_e_control_copy_passes(self):
        r = run_check(make_tmp_root(self.tmp))
        self.assertEqual(r.returncode, 0, f"未篡改临时根应过：{r.stdout}{r.stderr}")


if __name__ == "__main__":
    unittest.main(verbosity=2)
