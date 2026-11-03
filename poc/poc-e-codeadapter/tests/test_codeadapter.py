#!/usr/bin/env python3
"""PoC-E 代码域适配器单测（poc-supply 同款自跑风格，python3 -I 直接执行）。

只测纯函数与生命周期语义的机械面（四项绑定复核、改动清单校验、补丁件哈希、
reap 决策），lease/git/gitea 的真实面由端到端单验证（见 30-工件 pocE e2e）。
"""

from __future__ import annotations

import json
import sys
import tempfile
from pathlib import Path

ADAPTER_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ADAPTER_DIR))

from codeadapter import (  # noqa: E402
    AdapterError,
    apply_edits,
    four_binding_check,
    sha256_file,
)


def test_four_binding_all_four_match_and_verdict_pass_is_green():
    expected = {
        "base_sha": "b" * 40,
        "source_sha": "s" * 40,
        "tested_tree": "t" * 40,
        "suite_digest": "d" * 64,
        "verdict": "pass",
    }
    result = four_binding_check(expected, dict(expected))
    assert result["admit"] is True, result
    assert result["mismatches"] == {}
    assert result["verdict"] == "pass"


def test_four_binding_any_mismatch_refuses_merge():
    expected = {
        "base_sha": "b" * 40,
        "source_sha": "s" * 40,
        "tested_tree": "t" * 40,
        "suite_digest": "d" * 64,
        "verdict": "pass",
    }
    # 门跑过的树 ≠ 要合入的树（tested_tree 漂移）
    actual = dict(expected, tested_tree="x" * 40)
    result = four_binding_check(expected, actual)
    assert result["admit"] is False
    assert "tested_tree" in result["mismatches"]

    # 套件被换（suite_digest 漂移=门没被换脚本就没法绿着过来）
    result = four_binding_check(expected, dict(expected, suite_digest="y" * 64))
    assert result["admit"] is False
    assert "suite_digest" in result["mismatches"]

    # 分支基点漂移
    result = four_binding_check(expected, dict(expected, base_sha="z" * 40))
    assert result["admit"] is False
    assert "base_sha" in result["mismatches"]

    # 缺项（空串）一律拒
    result = four_binding_check(expected, {k: v for k, v in expected.items() if k != "source_sha"})
    assert result["admit"] is False


def test_four_binding_verdict_vocabulary_is_strict_pass():
    """af-workorder gate_admit 词汇表：只有小写 pass 是绿，其余一律非绿。"""
    base = {"base_sha": "b", "source_sha": "s", "tested_tree": "t", "suite_digest": "d"}
    for verdict in ("fail", "PASS", "pass ", "unknown", ""):
        result = four_binding_check(dict(base, verdict="pass"), dict(base, verdict=verdict))
        assert result["admit"] is False, verdict
        assert result["verdict"] == verdict


def test_apply_edits_append_and_replace_with_safety_rejections():
    with tempfile.TemporaryDirectory() as tmp:
        worktree = Path(tmp)
        target = worktree / "src" / "lib.rs"
        target.parent.mkdir()
        target.write_text("pub fn a() -> u32 { 1 }\n", encoding="utf-8")

        touched = apply_edits(worktree, [
            {"action": "append", "path": "src/lib.rs", "text": "pub fn b() -> u32 { 2 }\n"},
            {"action": "replace", "path": "src/lib.rs", "old": "pub fn a() -> u32 { 1 }", "text": "pub fn a() -> u32 { 11 }"},
        ])
        assert touched == ["src/lib.rs"]
        text = target.read_text(encoding="utf-8")
        assert "pub fn a() -> u32 { 11 }" in text
        assert text.endswith("pub fn b() -> u32 { 2 }\n")

        # 非法 action / 目录穿越 / 目标缺席 / replace 锚点不唯一 —— 全部拒
        for bad in (
            {"action": "delete", "path": "src/lib.rs", "text": "x"},
            {"action": "append", "path": "../escape.rs", "text": "x"},
            {"action": "append", "path": "src/absent.rs", "text": "x"},
            {"action": "replace", "path": "src/lib.rs", "old": "pub fn", "text": "x"},
        ):
            try:
                apply_edits(worktree, [bad])
            except AdapterError:
                pass
            else:
                raise AssertionError(f"应拒绝: {bad}")


def test_sha256_file_matches_known_vector():
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "probe.txt"
        path.write_bytes(b"af-poc-e\n")
        # 外部对照：printf 'af-poc-e\n' | sha256sum（2026-11-04 实测）
        assert sha256_file(path) == "276c0cd946310dabb0c7913c575ead6a5736959bfe47974ed02d07d5b6807e78"


def test_gate_result_shape_carries_four_binding_fields():
    """gate_result 形件必须带四项绑定字段与 pass/fail verdict（供复核制消费）。"""
    required = ("schema_version", "gate_id", "suite", "suite_digest", "verdict",
                "base_sha", "source_sha", "tested_tree")
    source = (ADAPTER_DIR / "codeadapter.py").read_text(encoding="utf-8")
    for field in required:
        assert f'"{field}"' in source, f"gate_result 缺字段 {field}"


def main() -> int:
    tests = [value for name, value in sorted(globals().items()) if name.startswith("test_")]
    failed = 0
    for test in tests:
        try:
            test()
        except Exception as error:  # noqa: BLE001
            failed += 1
            print(f"FAIL {test.__name__}: {error}")
        else:
            print(f"PASS {test.__name__}")
    print(f"{len(tests) - failed}/{len(tests)} passed")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
