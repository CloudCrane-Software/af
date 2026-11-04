#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""任务包组包覆盖面门·测试（G3 债收口批，2026-11-04）。

只新增断言：不改任何既有断言/门/基线。被测门 = ci/task_package_manifest_check.py
（importlib 装载做单元断言 + 子进程 verify-dir 做盘面对账断言；夹具 = 临时目录
构造的最小 Rust 源树与合成任务包，不触真仓任何树、不跑 cargo——组包回环由
run.sh step0 与 poc/poc-g3-manifest/ 破窗自证覆盖）。

覆盖：
  扫描器  —— 生产写入点命中与函数归属；#[cfg(test)] 内联块/裸 #[test] 函数/
             lib.rs 挂载 mod 剔除；行注释噪声免疫；常量名产物解析；多行
             fs::write 的产物字面量窗口提取；
  verify-dir —— 正例（三件套齐+钉逐位一致+代际一致）绿；
             负例① 缺件（清单有、盘面无）红；负例② 多件（盘面有、清单无）红；
             负例③ 身份不符（内容改、钉不动）红；负例④ 代际不符
             （compiler_version 漂移）红；负例⑤ manifest 键集漂移红；
             负例⑥ manifest 缺席红；
  注册表  —— 真仓注册表自钉对账绿（改常量即红——防「钉未初始化」形态混过）。

跑法：python3 -I ci/test_task_package_manifest_check.py
"""
import hashlib
import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent          # ci/
REPO = HERE.parent
GATE = HERE / "task_package_manifest_check.py"

_spec = importlib.util.spec_from_file_location("tpmc", GATE)
tpmc = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(tpmc)


def make_pkg_dir(base: Path) -> Path:
    """合成一个钉面逐位一致的最小任务包（compiler_version 取真仓 af-compiler 版本）。"""
    d = base / "task"
    d.mkdir(parents=True)
    (d / "plan.json").write_text('{"plan": "g3-fixture"}\n', encoding="utf-8")
    (d / "task.json").write_text('{"task_id": "G3-TEST"}\n', encoding="utf-8")
    ver = tpmc.parse_af_compiler_version(REPO)
    assert ver, "真仓 af-compiler 版本应可析"
    manifest = {
        "format_version": 1,
        "task_sha256": hashlib.sha256((d / "task.json").read_bytes()).hexdigest(),
        "plan_sha256": hashlib.sha256((d / "plan.json").read_bytes()).hexdigest(),
        "plan_id": "g3-fixture-plan",
        "compiler_version": ver,
    }
    (d / "package_manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    return d


class ScannerTest(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="g3-scan-t-"))

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _rs(self, rel: str, text: str) -> None:
        p = self.tmp / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="utf-8")

    def test_prod_write_point_detected_and_attributed(self):
        self._rs(
            "crates/af-x/src/lib.rs",
            'pub fn assemble(pkg: &Path) {\n'
            '    std::fs::write(pkg.join("plan.json"), "{}");\n'
            "}\n",
        )
        pts = tpmc.scan_write_points(self.tmp)
        self.assertIn(("crates/af-x/src/lib.rs", "assemble"), pts)
        self.assertEqual(pts[("crates/af-x/src/lib.rs", "assemble")]["literals"], {"plan.json"})

    def test_inline_cfg_test_mod_and_bare_test_fn_excluded(self):
        # mod tests 闭合后顶层再挂裸 #[test] 函数（engine.rs 实有形态）
        self._rs(
            "crates/af-x/src/lib.rs",
            "#[cfg(test)]\nmod tests {\n"
            '    fn helper() { std::fs::write("t1", "x"); }\n'
            "}\n"
            "#[test]\n"
            'fn bare_test_writes() {\n'
            '    std::fs::write("t2", "x");\n'
            "}\n",
        )
        pts = tpmc.scan_write_points(self.tmp)
        self.assertEqual(pts, {})

    def test_mid_file_test_mod_does_not_swallow_later_prod_code(self):
        # af-bench/main.rs 实有形态：测试块在文件中部，其后仍有生产写入点
        self._rs(
            "crates/af-x/src/main.rs",
            "fn early() {}\n"
            "#[cfg(test)]\nmod tests {\n"
            "    #[test]\n    fn a() { let _ = 0; }\n"
            "}\n"
            "fn run() {\n"
            '    std::fs::create_dir_all("runs");\n'
            "}\n",
        )
        pts = tpmc.scan_write_points(self.tmp)
        self.assertIn(("crates/af-x/src/main.rs", "run"), pts)

    def test_lib_gated_mod_file_excluded(self):
        self._rs(
            "crates/af-x/src/lib.rs",
            "#[cfg(test)]\nmod tests;\n",
        )
        self._rs(
            "crates/af-x/src/tests.rs",
            'fn scratch() { std::fs::create_dir_all("d"); }\n',
        )
        pts = tpmc.scan_write_points(self.tmp)
        self.assertEqual(pts, {})

    def test_line_comment_noise_immune(self):
        self._rs(
            "crates/af-x/src/lib.rs",
            '// std::fs::write("commented", "x");\n'
            "pub fn real() {}\n",
        )
        pts = tpmc.scan_write_points(self.tmp)
        self.assertEqual(pts, {})

    def test_const_resolved_artifact_name(self):
        self._rs(
            "crates/af-x/src/lib.rs",
            'pub const MANIFEST_FILE: &str = "package_manifest.json";\n'
            "pub fn writer(d: &Path) {\n"
            "    std::fs::write(d.join(MANIFEST_FILE), \"{}\");\n"
            "}\n",
        )
        pts = tpmc.scan_write_points(self.tmp)
        self.assertEqual(
            pts[("crates/af-x/src/lib.rs", "writer")]["literals"], {"package_manifest.json"}
        )

    def test_multiline_write_literal_in_window(self):
        self._rs(
            "crates/af-x/src/lib.rs",
            "pub fn assemble(task_dir: &Path) {\n"
            "    std::fs::write(\n"
            '        task_dir.join("task.json"),\n'
            '        "{}",\n'
            "    );\n"
            "}\n",
        )
        pts = tpmc.scan_write_points(self.tmp)
        self.assertEqual(
            pts[("crates/af-x/src/lib.rs", "assemble")]["literals"], {"task.json"}
        )

    def test_raw_string_braces_do_not_corrupt_test_region(self):
        self._rs(
            "crates/af-x/src/lib.rs",
            "#[cfg(test)]\nmod tests {\n"
            "    fn t() {\n"
            '        let s = r#"{ "kind": "Gate" }"#;\n'
            "        let _ = s;\n"
            "    }\n"
            "}\n"
            "fn prod() {\n"
            '    std::fs::write("p", "x");\n'
            "}\n",
        )
        pts = tpmc.scan_write_points(self.tmp)
        self.assertIn(("crates/af-x/src/lib.rs", "prod"), pts)


class VerifyDirTest(unittest.TestCase):
    """verify-dir 盘面对账（子进程，--root 真仓只读；夹具目录全在临时区）。"""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="g3-verify-t-"))

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _run(self, pkg: Path):
        return subprocess.run(
            [sys.executable, "-I", str(GATE), "verify-dir", str(pkg), "--root", str(REPO)],
            capture_output=True, text=True,
        )

    def test_green_on_well_formed_package(self):
        d = make_pkg_dir(self.tmp)
        proc = self._run(d)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("核对一致", proc.stdout)

    def test_red_missing_artifact(self):
        d = make_pkg_dir(self.tmp)
        (d / "task.json").unlink()
        proc = self._run(d)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("清单有条目、盘面无产物", proc.stderr)
        self.assertIn("task.json", proc.stderr)

    def test_red_extra_artifact(self):
        d = make_pkg_dir(self.tmp)
        (d / "meta.json").write_text("{}", encoding="utf-8")
        proc = self._run(d)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("盘面有产物、清单无条目", proc.stderr)
        self.assertIn("meta.json", proc.stderr)

    def test_red_tampered_content(self):
        d = make_pkg_dir(self.tmp)
        (d / "plan.json").write_text('{"plan": "tampered"}\n', encoding="utf-8")
        proc = self._run(d)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("产物身份不符", proc.stderr)
        self.assertIn("plan.json", proc.stderr)

    def test_red_compiler_version_drift(self):
        d = make_pkg_dir(self.tmp)
        m = json.loads((d / "package_manifest.json").read_text(encoding="utf-8"))
        m["compiler_version"] = "0.0.0-not-current"
        (d / "package_manifest.json").write_text(
            json.dumps(m, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        proc = self._run(d)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("产物代际不符", proc.stderr)

    def test_red_manifest_key_drift(self):
        d = make_pkg_dir(self.tmp)
        m = json.loads((d / "package_manifest.json").read_text(encoding="utf-8"))
        m["extra_field"] = "x"
        (d / "package_manifest.json").write_text(
            json.dumps(m, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        proc = self._run(d)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("键集漂移", proc.stderr)

    def test_red_manifest_absent(self):
        d = make_pkg_dir(self.tmp)
        (d / "package_manifest.json").unlink()
        proc = self._run(d)
        # 层1 缺席：键集核对名缺席；且 plan/task 仍在册双向面（manifest 不在
        # 注册表组包产物面之外——缺席由「清单有条目、盘面无产物」点名）
        self.assertEqual(proc.returncode, 1)
        self.assertIn("清单有条目、盘面无产物", proc.stderr)
        self.assertIn("package_manifest.json", proc.stderr)


class RegistryPinTest(unittest.TestCase):
    def test_real_registry_pin_green(self):
        data, errs = tpmc.load_registry(REPO)
        self.assertIsNotNone(data)
        self.assertEqual(errs, [])

    def test_drift_detected_when_pin_differs(self):
        # 常量与盘面不一致（模拟注册表被改而脚本未重钉）——in-process 改常量即可
        old = tpmc.REGISTRY_SHA256
        try:
            tpmc.REGISTRY_SHA256 = "0" * 64
            _data, errs = tpmc.load_registry(REPO)
            self.assertTrue(any("注册表漂移" in e for e in errs), errs)
        finally:
            tpmc.REGISTRY_SHA256 = old

    def test_pending_pin_rejected(self):
        old = tpmc.REGISTRY_SHA256
        try:
            tpmc.REGISTRY_SHA256 = "PENDING-INITIAL-PIN"
            _data, errs = tpmc.load_registry(REPO)
            self.assertTrue(any("自钉未落" in e for e in errs), errs)
        finally:
            tpmc.REGISTRY_SHA256 = old


class StaticClosureTest(unittest.TestCase):
    """③ 组包域闭包：真仓 package.rs 钉面机析与注册表组包产物面闭包一致。"""

    def test_closure_on_real_repo(self):
        data, errs = tpmc.load_registry(REPO)
        self.assertEqual(errs, [])
        scanned = tpmc.scan_write_points(REPO)
        errs = tpmc.static_checks(REPO, data, scanned)
        self.assertEqual(errs, [], "真仓静态三段（自钉外）应零红")

    def test_closure_red_when_registry_adds_unpinned_artifact(self):
        data, errs = tpmc.load_registry(REPO)
        self.assertEqual(errs, [])
        # 仿「注册表多造一件层1 不钉的产物」：闭包必须红（半覆盖破窗）
        import copy
        data2 = copy.deepcopy(data)
        for e in data2["entries"]:
            if e["domain"] == "package" and e["fn"] == "assemble_construction_package":
                e["artifacts"] = e["artifacts"] + ["meta.json"]
        scanned = tpmc.scan_write_points(REPO)
        errs = tpmc.static_checks(REPO, data2, scanned)
        self.assertTrue(any("组包域闭包不成立" in e for e in errs), errs)

    def test_unregistered_site_red(self):
        data, errs = tpmc.load_registry(REPO)
        self.assertEqual(errs, [])
        # 仿「新生产写入点未造册」：核心红腿（覆盖不到=破窗）
        import copy
        scanned2 = dict(tpmc.scan_write_points(REPO))
        scanned2[("crates/af-x/src/new_writer.rs", "sneaky")] = {"lines": [1], "literals": set()}
        errs = tpmc.static_checks(REPO, data, scanned2)
        self.assertTrue(any("生产写入点未造册" in e for e in errs), errs)
        self.assertTrue(any("sneaky" in e for e in errs))


if __name__ == "__main__":
    unittest.main(verbosity=2)
