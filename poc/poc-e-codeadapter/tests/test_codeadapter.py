#!/usr/bin/env python3
"""PoC-E 代码域适配器单测（poc-supply 同款自跑风格，python3 -I 直接执行）。

只测纯函数与生命周期语义的机械面（四项绑定复核、改动清单校验、补丁件哈希、
lease 四态、多编辑原子性、错 holder 释放、门红订单态），lease/git/gitea 的
真实面由端到端单验证（见 30-工件 pocE e2e）。
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

ADAPTER_DIR = Path(__file__).resolve().parent.parent
SUPPLY_DIR = ADAPTER_DIR.parent / "poc-supply"
sys.path.insert(0, str(ADAPTER_DIR))
sys.path.insert(0, str(SUPPLY_DIR))

from codeadapter import (  # noqa: E402
    AdapterError,
    adapter_order_state,
    apply_edits,
    assert_closed_has_three_records,
    cmd_release,
    cmd_run,
    four_binding_check,
    load_adapter_ledger,
    redact_userinfo,
    require_active_lease,
    run_checked,
    sample_integrator_actual,
    sha256_file,
)
from lease_store import LeaseStore  # noqa: E402


def _profile(node_id: str = "anolis") -> dict:
    return {"schema_version": "af.resources/v1", "recipe_namespace": "user-01", "node_id": node_id}


def _active_lease(
    tmp: Path,
    lease_id: str = "lease-ok",
    holder: str = "poc-e",
    ttl: int = 60_000,
    now: int = 1_000,
    interval: str | None = None,
) -> dict:
    db = tmp / "leases.db"
    store = LeaseStore(db)
    store.acquire(
        lease_id,
        "anolis/root",
        interval or f"worktree:{lease_id}",
        _profile(),
        holder,
        1024,
        ttl,
        now,
    )
    store.close()
    return {"leases_db": db, "lease_id": lease_id, "holder": holder, "now_ms": now}


def _tree(tmp: Path) -> Path:
    worktree = tmp / "worktree"
    (worktree / "src").mkdir(parents=True)
    (worktree / "src" / "lib.rs").write_text("pub fn a() -> u32 { 1 }\n", encoding="utf-8")
    (worktree / "src" / "other.rs").write_text("pub fn c() -> u32 { 3 }\n", encoding="utf-8")
    return worktree


def _git_repo(path: Path) -> Path:
    path.mkdir(parents=True, exist_ok=True)
    (path / "src").mkdir(exist_ok=True)
    if not (path / "src" / "lib.rs").exists():
        (path / "src" / "lib.rs").write_text("pub fn a() -> u32 { 1 }\n", encoding="utf-8")
    subprocess.run(["git", "init", "-b", "main"], cwd=path, check=True, capture_output=True)
    subprocess.run(["git", "config", "user.email", "t@t"], cwd=path, check=True, capture_output=True)
    subprocess.run(["git", "config", "user.name", "t"], cwd=path, check=True, capture_output=True)
    subprocess.run(["git", "add", "-A"], cwd=path, check=True, capture_output=True)
    subprocess.run(["git", "commit", "-m", "init"], cwd=path, check=True, capture_output=True)
    return path


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
        tmp_path = Path(tmp)
        worktree = _tree(tmp_path)
        ctx = _active_lease(tmp_path)
        target = worktree / "src" / "lib.rs"

        touched = apply_edits(worktree, [
            {"action": "append", "path": "src/lib.rs", "text": "pub fn b() -> u32 { 2 }\n"},
            {"action": "replace", "path": "src/lib.rs", "old": "pub fn a() -> u32 { 1 }", "text": "pub fn a() -> u32 { 11 }"},
        ], lease_ctx=ctx)
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
                apply_edits(worktree, [bad], lease_ctx=ctx)
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


def test_apply_edits_second_failure_leaves_no_residue():
    """P0-1：双编辑第二项必红，修后树无前序 append 残留。"""
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        worktree = _tree(tmp_path)
        ctx = _active_lease(tmp_path)
        lib = worktree / "src" / "lib.rs"
        other = worktree / "src" / "other.rs"
        lib_before = lib.read_text(encoding="utf-8")
        other_before = other.read_text(encoding="utf-8")
        try:
            apply_edits(worktree, [
                {"action": "append", "path": "src/lib.rs", "text": "pub fn b() -> u32 { 2 }\n"},
                {"action": "replace", "path": "src/other.rs", "old": "NO_SUCH_ANCHOR", "text": "x"},
            ], lease_ctx=ctx)
        except AdapterError as error:
            assert "PA07" in str(error)
        else:
            raise AssertionError("第二项必须失败")
        assert lib.read_text(encoding="utf-8") == lib_before
        assert other.read_text(encoding="utf-8") == other_before


def test_lease_four_states_fail_closed():
    """P0-2：无约 / 过期 / 已还 / 错 holder 一律拒，树不写。"""
    source = (ADAPTER_DIR / "codeadapter.py").read_text(encoding="utf-8")
    assert source.count("require_active_lease(") >= 5
    assert "lease_ctx" in source

    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        edit = [{"action": "append", "path": "src/lib.rs", "text": "// mutated\n"}]

        # 无约
        worktree = _tree(tmp_path / "no-lease")
        before = (worktree / "src" / "lib.rs").read_text(encoding="utf-8")
        try:
            apply_edits(worktree, edit)
        except AdapterError as error:
            assert "PA17" in str(error)
        else:
            raise AssertionError("无约应拒")
        assert (worktree / "src" / "lib.rs").read_text(encoding="utf-8") == before

        # 过期（ttl 过，show 仍可能是 active 行）
        worktree = _tree(tmp_path / "expired")
        ctx = _active_lease(tmp_path / "expired", lease_id="lease-exp", ttl=10, now=1000)
        ctx["now_ms"] = 1011
        before = (worktree / "src" / "lib.rs").read_text(encoding="utf-8")
        try:
            apply_edits(worktree, edit, lease_ctx=ctx)
        except AdapterError as error:
            assert "PA19" in str(error)
        else:
            raise AssertionError("过期应拒")
        assert (worktree / "src" / "lib.rs").read_text(encoding="utf-8") == before

        # 已还
        worktree = _tree(tmp_path / "returned")
        ctx = _active_lease(tmp_path / "returned", lease_id="lease-ret", ttl=60_000, now=1000)
        store = LeaseStore(ctx["leases_db"])
        assert store.return_lease("lease-ret", "poc-e", 1500)
        store.close()
        before = (worktree / "src" / "lib.rs").read_text(encoding="utf-8")
        try:
            apply_edits(worktree, edit, lease_ctx=ctx)
        except AdapterError as error:
            assert "PA18" in str(error)
        else:
            raise AssertionError("已还应拒")
        assert (worktree / "src" / "lib.rs").read_text(encoding="utf-8") == before

        # 错 holder
        worktree = _tree(tmp_path / "holder")
        ctx = _active_lease(tmp_path / "holder", lease_id="lease-hold", holder="poc-e")
        ctx["holder"] = "intruder"
        before = (worktree / "src" / "lib.rs").read_text(encoding="utf-8")
        try:
            apply_edits(worktree, edit, lease_ctx=ctx)
        except AdapterError as error:
            assert "PA20" in str(error)
        else:
            raise AssertionError("错 holder 应拒")
        assert (worktree / "src" / "lib.rs").read_text(encoding="utf-8") == before

        # 四态复核函数自身
        try:
            require_active_lease(None)
        except AdapterError as error:
            assert "PA17" in str(error)
        else:
            raise AssertionError("空上下文应拒")


def test_cmd_release_holder_guard_two_states():
    """P0-3：错 holder 树与租约都不动；对 holder 则 return 后 rmtree。"""
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        ctx = _active_lease(tmp_path, lease_id="lease-rel", holder="poc-e", now=0)
        run_dir = tmp_path / "lease-rel"
        run_dir.mkdir()
        (run_dir / "keep.txt").write_text("keep\n", encoding="utf-8")
        (run_dir / "lease-binding.json").write_text(
            json.dumps({"lease_id": "lease-rel", "run_dir": str(run_dir)}),
            encoding="utf-8",
        )

        try:
            cmd_release(SimpleNamespace(
                run_dir=str(run_dir),
                leases_db=str(ctx["leases_db"]),
                holder="intruder",
            ))
        except AdapterError as error:
            assert "PA20" in str(error)
        else:
            raise AssertionError("错 holder 释放应拒")
        assert run_dir.exists(), "错 holder 不得 rmtree"
        assert (run_dir / "keep.txt").read_text(encoding="utf-8") == "keep\n"
        store = LeaseStore(ctx["leases_db"])
        assert store.get("lease-rel")["state"] == "active"
        store.close()

        rc = cmd_release(SimpleNamespace(
            run_dir=str(run_dir),
            leases_db=str(ctx["leases_db"]),
            holder="poc-e",
        ))
        assert rc == 0
        assert not run_dir.exists()
        store = LeaseStore(ctx["leases_db"])
        assert store.get("lease-rel")["state"] == "returned"
        store.close()


def test_gate_red_not_closed_and_closed_needs_three_records():
    """P0-4：门红场景复放——ledger 记 gate_result=red，状态非 Closed；Closed 必须三记录。"""
    failed = adapter_order_state([
        {"kind": "patch", "sha256": "aa"},
        {"kind": "gate", "gate_result": "red", "verdict": "fail"},
    ])
    assert failed == "Failed"
    closed = adapter_order_state([
        {"kind": "patch"},
        {"kind": "gate", "gate_result": "green", "verdict": "pass"},
        {"kind": "merge", "pr": 1},
    ])
    assert closed == "Closed"
    try:
        assert_closed_has_three_records("Closed", [{"kind": "patch"}])
    except AdapterError as error:
        assert "PA21" in str(error)
    else:
        raise AssertionError("缺三记录的 Closed 应拒")

    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        worktree = _git_repo(tmp_path / "wt")
        repo = worktree
        evidence = tmp_path / "evidence"
        evidence.mkdir()
        run_root = tmp_path / "runs"
        run_root.mkdir()
        gate = tmp_path / "gate_fail.sh"
        gate.write_text(
            "#!/bin/bash\n"
            "OUT=\"$2\"\n"
            "mkdir -p \"$OUT\"\n"
            "echo 'step1  FAIL' > \"$OUT/SUMMARY.txt\"\n"
            "echo 'PIPELINE-RED (exit 1)' >> \"$OUT/SUMMARY.txt\"\n"
            "exit 1\n",
            encoding="utf-8",
        )
        os.chmod(gate, 0o755)
        plan = {
            "order_id": "WO-GATE-RED",
            "lease_id": "lease-gr",
            "branch": "poc-e/wo-gate-red",
            "registry": str(tmp_path / "reg"),
            "leases_db": str(tmp_path / "leases.db"),
            "gate_script": str(gate),
            "af_order_bin": "/bin/true",
            "order_spec": str(tmp_path / "spec.json"),
            "order_state_dir": str(tmp_path / "state"),
            "workspace": str(tmp_path),
            "target_dir": str(tmp_path / "target"),
            "node_id": "anolis",
            "mount": "root",
            "resource": "anolis/root",
            "reserved_bytes": 1024,
            "holder": "poc-e",
            "ttl_ms": 60_000,
            "repo_url": "http://example.invalid/repo.git",
            "base_branch": "main",
            "edits": [{"action": "append", "path": "src/lib.rs", "text": "// gate-red\n"}],
            "change_summary": "gate-red-replay",
            "gitea_api_base": "http://127.0.0.1:1",
            "gitea_credentials": str(tmp_path / "creds"),
            "gitea_host_suffix": "example.invalid",
            "gitea_repo": "org/repo",
        }
        plan_path = tmp_path / "plan.json"
        plan_path.write_text(json.dumps(plan), encoding="utf-8")
        args = SimpleNamespace(
            plan=str(plan_path),
            run_root=str(run_root),
            evidence_dir=str(evidence),
        )
        import codeadapter as ca

        constructed = {"gitea": 0}

        class Boom:
            def __init__(self, *a, **k):
                constructed["gitea"] += 1
                raise AssertionError("门红不得构造 GiteaClient")

        with mock.patch.object(ca, "collect_observation", return_value=tmp_path / "obs.json"), \
             mock.patch.object(ca, "preflight", return_value={"post_free_bytes": 1, "admit": True}), \
             mock.patch.object(ca, "open_worktree", return_value=(repo, worktree)), \
             mock.patch.object(ca, "GiteaClient", Boom):
            try:
                cmd_run(args)
            except AdapterError as error:
                assert "PA14" in str(error)
            else:
                raise AssertionError("门红 cmd_run 应失败")

        assert constructed["gitea"] == 0
        ledger_path = evidence / "WO-GATE-RED.ledger.jsonl"
        records = load_adapter_ledger(ledger_path)
        kinds = [item.get("kind") for item in records]
        assert "patch" in kinds
        assert "gate" in kinds
        assert "merge" not in kinds
        gate_rec = next(item for item in records if item.get("kind") == "gate")
        assert gate_rec.get("gate_result") == "red"
        assert adapter_order_state(records) == "Failed"
        e2e = json.loads((evidence / "WO-GATE-RED.e2e.json").read_text(encoding="utf-8"))
        assert e2e["order"]["state"] != "Closed"
        assert e2e["gate_result"] == "red"
        assert e2e["merge_commit_sha"] is None


def test_pa10_error_redacts_userinfo():
    """P1-5：PA10 错误路径不得打 userinfo。"""
    canary = "http://user:CANARYSECRET@127.0.0.1:1/repo.git"
    assert redact_userinfo(canary) == "http://***:***@127.0.0.1:1/repo.git"
    with tempfile.TemporaryDirectory() as tmp:
        dest = Path(tmp) / "clone-dest"
        try:
            run_checked(["git", "clone", canary, str(dest)])
        except AdapterError as error:
            text = str(error)
            assert "PA10" in text
            assert "CANARYSECRET" not in text
            assert "user:CANARYSECRET" not in text
        else:
            raise AssertionError("canary clone 应失败")


def test_integrator_actual_is_independently_sampled():
    """P1-6：suite_digest/verdict/base_sha/source_sha 独立取样，不得用自算值充数。"""
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        worktree = _git_repo(tmp_path / "wt")
        (worktree / "src" / "lib.rs").write_text("pub fn a() -> u32 { 2 }\n", encoding="utf-8")
        subprocess.run(["git", "add", "-A"], cwd=worktree, check=True, capture_output=True)
        subprocess.run(["git", "commit", "-m", "edit"], cwd=worktree, check=True, capture_output=True)
        script = tmp_path / "gate.sh"
        script.write_text("#!/bin/bash\necho hi\n", encoding="utf-8")
        gate_out = tmp_path / "gate-out"
        gate_out.mkdir()
        (gate_out / "SUMMARY.txt").write_text("PIPELINE-RED (exit 1)\n", encoding="utf-8")
        actual = sample_integrator_actual(worktree, script, gate_out)
        head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=worktree, text=True).strip()
        parent = subprocess.check_output(["git", "rev-parse", "HEAD~1"], cwd=worktree, text=True).strip()
        tree = subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], cwd=worktree, text=True).strip()
        assert actual["source_sha"] == head
        assert actual["base_sha"] == parent
        assert actual["tested_tree"] == tree
        assert actual["suite_digest"] == sha256_file(script)
        assert actual["verdict"] == "fail"
        fake = {
            "suite_digest": "0" * 64,
            "verdict": "pass",
            "base_sha": "b" * 40,
            "source_sha": "s" * 40,
            "tested_tree": "t" * 40,
        }
        assert actual["suite_digest"] != fake["suite_digest"]
        assert actual["verdict"] != fake["verdict"]
        assert actual["base_sha"] != fake["base_sha"]
        assert actual["source_sha"] != fake["source_sha"]
        source = (ADAPTER_DIR / "codeadapter.py").read_text(encoding="utf-8")
        assert "actual = sample_integrator_actual(" in source
        assert '"suite_digest": gate_result["suite_digest"]' not in source
        assert '"verdict": gate_result["verdict"]' not in source.split("actual = sample_integrator_actual", 1)[1][:400]


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
