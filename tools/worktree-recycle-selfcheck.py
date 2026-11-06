#!/usr/bin/env python3
"""worktree-recycle.py 收敛面自检（删分支收敛 + 强删兜底去除）。

覆盖（任务四条 + 三路语义回归，缺一不可）：
  T1  脏树（未提交改动）→ 工具不得删目录：现场保留、报告写明留检、登记不销；
  T2a 自报 merged 但未真合入 → 不得被删（显式开关下也一样，mismatch 留检）；
  T2b aborted 过留证期、分支未真合入 + 显式开关 → 树清、分支保留（未合入拒绝）；
  T3  merged 且真合入（默认，无开关）→ 树清、分支仍在 git（branch --list 有、
      cat-file -t 读得出）；
  T4  --delete-merged-branch 显式开关 → 已真合入分支被 `git branch -d` 安全删；
  R1-R4 三路语义回归：active lease 未到期 KEEP / 过期 RECLAIM 标 timeout 不删树 /
      aborted 留证期内 KEEP / 保护分支登记 SKIP。

跑法：python3 -I tools/worktree-recycle-selfcheck.py（零第三方依赖；-I 下按
文件路径加载被测脚本，不依赖 sys.path）。全部夹具仓建在临时目录，跑完即清。
"""

from __future__ import annotations

import importlib.util
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from contextlib import redirect_stdout
from pathlib import Path

# -I 模式不把脚本目录进 sys.path，按路径显式加载被测模块。
_HERE = Path(__file__).resolve().parent
_SPEC = importlib.util.spec_from_file_location("worktree_recycle", _HERE / "worktree-recycle.py")
recycle = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(recycle)

RESULTS: list[tuple[bool, str]] = []


def check(label: str, ok: bool, detail: str = "") -> None:
    RESULTS.append((ok, label))
    print(f"[{'PASS' if ok else 'FAIL'}] {label}" + (f" —— {detail}" if detail and not ok else ""))


def sh(repo: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True)


def fixture(root: Path, name: str) -> tuple[Path, str]:
    """建一次性夹具仓：main 上 base，feat 上 work 提交并 --no-ff 合回 main。

    返回 (repo, feat_tip)：feat 已真合入（T1/T3/T4 用）；未合入场景另行checkout
    新分支（T2 用）。
    """
    repo = root / name
    repo.mkdir(parents=True)
    sh(repo, "init", "-q", "-b", "main")
    sh(repo, "config", "user.email", "selfcheck@t")
    sh(repo, "config", "user.name", "selfcheck")
    (repo / "f.txt").write_text("base\n", encoding="utf-8")
    sh(repo, "add", "-A")
    sh(repo, "commit", "-qm", "base")
    sh(repo, "checkout", "-qb", "feat")
    (repo / "g.txt").write_text("work\n", encoding="utf-8")
    sh(repo, "add", "-A")
    sh(repo, "commit", "-qm", "work")
    tip = sh(repo, "rev-parse", "feat").stdout.strip()
    sh(repo, "checkout", "-q", "main")
    sh(repo, "merge", "-q", "--no-ff", "feat", "-m", "merge feat")
    return repo, tip


def add_worktree(repo: Path, branch: str) -> Path:
    wt = repo / ".af-worktrees" / branch
    proc = sh(repo, "worktree", "add", str(wt), branch)
    assert proc.returncode == 0, proc.stderr
    return wt


def run_tool(repo: Path, *args: str) -> tuple[int, str]:
    buf = io.StringIO()
    with redirect_stdout(buf):
        rc = recycle.main(["--repo", str(repo), *args])
    return rc, buf.getvalue()


def branch_exists(repo: Path, branch: str) -> bool:
    return sh(repo, "branch", "--list", branch).stdout.strip() != ""


def mark(repo: Path, name: str, state: str, *extra: str) -> None:
    rc, out = run_tool(repo, "mark", name, "--state", state, *extra)
    assert rc == 0, out


def backdate(repo: Path, name: str, hours: float, key: str = "marked_at") -> None:
    """把登记时间戳拨回 hours 小时前（免睡等的测试脚手架，如实声明）。"""
    data = recycle.load_registry(repo)
    data["worktrees"][name][key] = time.time() - hours * 3600
    recycle.save_registry(repo, data)


def registry_entry(repo: Path, name: str) -> dict | None:
    return recycle.load_registry(repo)["worktrees"].get(name)


def t1_dirty_tree_kept(root: Path) -> None:
    repo, _ = fixture(root, "t1")
    wt = add_worktree(repo, "feat")
    (wt / "f.txt").write_text("base\n未提交的活\n", encoding="utf-8")
    mark(repo, "feat", "merged")
    rc, out = run_tool(repo, "recycle")
    check("T1a 脏树回收：报告写明留检", rc == 0 and "留检保现场" in out, out)
    check("T1b 脏树回收：目录仍在（未强删）", wt.is_dir() and (wt / "f.txt").is_file())
    kept = (wt / "f.txt").read_text(encoding="utf-8") if (wt / "f.txt").is_file() else ""
    check("T1c 脏面未提交改动原样在", "未提交的活" in kept, repr(kept))
    entry = registry_entry(repo, "feat")
    check("T1d 登记未销且记 mismatch 留检", bool(entry) and entry.get("mismatch") is True)


def t2a_unmerged_selfreport(root: Path) -> None:
    repo, _ = fixture(root, "t2a")
    sh(repo, "checkout", "-qb", "orphan", "main")  # 未合入：main 之外的新提交
    (repo / "x.txt").write_text("x\n", encoding="utf-8")
    sh(repo, "add", "-A")
    sh(repo, "commit", "-qm", "x")
    sh(repo, "checkout", "-q", "main")
    add_worktree(repo, "orphan")
    mark(repo, "orphan", "merged")  # 自报 merged（谎报）
    rc, out = run_tool(repo, "recycle", "--delete-merged-branch")
    check("T2a1 自报 merged 未真合入（开关下）→ mismatch 留检",
          rc == 0 and "mismatch 留检" in out, out)
    check("T2a2 未合入分支未被删", branch_exists(repo, "orphan"))


def t2b_unmerged_aborted(root: Path) -> None:
    repo, _ = fixture(root, "t2b")
    sh(repo, "checkout", "-qb", "leftover", "main")
    (repo / "y.txt").write_text("y\n", encoding="utf-8")
    sh(repo, "add", "-A")
    sh(repo, "commit", "-qm", "y")
    sh(repo, "checkout", "-q", "main")
    wt = add_worktree(repo, "leftover")
    mark(repo, "leftover", "aborted")
    backdate(repo, "leftover", 49.0)  # 过 48h 留证期
    rc, out = run_tool(repo, "recycle", "--delete-merged-branch")
    check("T2b1 aborted 过留证期 → 树清", rc == 0 and not wt.exists()
          and "CLEAN leftover" in out, out)
    check("T2b2 未真合入分支在开关下仍被拒删",
          branch_exists(repo, "leftover") and "只删已合入分支" in out, out)


def t3_merged_default_keeps_branch(root: Path) -> None:
    repo, tip = fixture(root, "t3")
    wt = add_worktree(repo, "feat")
    mark(repo, "feat", "merged")
    rc, out = run_tool(repo, "recycle")
    check("T3a merged 真合入（默认）→ 树清", rc == 0 and not wt.exists()
          and "CLEAN feat" in out, out)
    check("T3b 报告写明默认保分支", "默认保分支保提交" in out, out)
    check("T3c 分支仍在 git branch --list", branch_exists(repo, "feat"))
    cat = sh(repo, "cat-file", "-t", tip)
    check("T3d 提交对象仍可读（cat-file -t）", cat.returncode == 0
          and cat.stdout.strip() == "commit", cat.stdout + cat.stderr)
    check("T3e 登记已销（清路完整）", registry_entry(repo, "feat") is None)


def t4_switch_deletes_merged_only(root: Path) -> None:
    repo, tip = fixture(root, "t4")
    wt = add_worktree(repo, "feat")
    mark(repo, "feat", "merged")
    rc, out = run_tool(repo, "recycle", "--delete-merged-branch")
    check("T4a 开关下 merged 真合入 → 树清", rc == 0 and not wt.exists(), out)
    check("T4b 已合入分支被 -d 安全删", "已删分支 feat" in out and "-d 安全删" in out
          and not branch_exists(repo, "feat"), out)
    cat = sh(repo, "cat-file", "-t", tip)
    check("T4c 分支删后提交对象仍可读（保提交）",
          cat.returncode == 0 and cat.stdout.strip() == "commit", cat.stdout + cat.stderr)


def regress_three_paths(root: Path) -> None:
    repo, _ = fixture(root, "r1")
    wt = add_worktree(repo, "feat")
    mark(repo, "feat", "active", "--ttl-min", "120")
    rc, out = run_tool(repo, "recycle")
    check("R1 active lease 未到期 → KEEP 且不删树",
          rc == 0 and "KEEP feat" in out and "lease 未到期" in out and wt.is_dir(), out)

    repo, _ = fixture(root, "r2")
    wt = add_worktree(repo, "feat")
    mark(repo, "feat", "active", "--ttl-min", "0")
    time.sleep(0.05)
    rc, out = run_tool(repo, "recycle")
    entry = registry_entry(repo, "feat")
    check("R2 lease 过期 → RECLAIM 标 timeout 且不删树（TTL≠删树）",
          rc == 0 and "RECLAIM feat" in out and wt.is_dir()
          and entry is not None and entry.get("state") == "timeout", out)

    repo, _ = fixture(root, "r3")
    wt = add_worktree(repo, "feat")
    mark(repo, "feat", "aborted")
    rc, out = run_tool(repo, "recycle")
    check("R3 aborted 留证期内 → KEEP 现场保留",
          rc == 0 and "KEEP feat" in out and "留证中" in out and wt.is_dir(), out)

    repo, _ = fixture(root, "r4")
    mark(repo, "ml", "merged")
    data = recycle.load_registry(repo)
    data["worktrees"]["ml"]["branch"] = "main"  # 保护分支登记
    recycle.save_registry(repo, data)
    rc, out = run_tool(repo, "recycle", "--delete-merged-branch")
    check("R4 保护分支登记 → SKIP 永不回收",
          rc == 0 and "SKIP ml" in out and "保护分支" in out and branch_exists(repo, "main"), out)


def main() -> int:
    os.environ.setdefault("LANG", "C.UTF-8")
    root = Path(tempfile.mkdtemp(prefix="recycle-selfcheck-"))
    try:
        t1_dirty_tree_kept(root)
        t2a_unmerged_selfreport(root)
        t2b_unmerged_aborted(root)
        t3_merged_default_keeps_branch(root)
        t4_switch_deletes_merged_only(root)
        regress_three_paths(root)
    finally:
        shutil.rmtree(root, ignore_errors=True)
    failed = [label for ok, label in RESULTS if not ok]
    print(f"\n共 {len(RESULTS)} 项：{'全绿' if not failed else f'{len(failed)} 项红'}")
    for label in failed:
        print(f"  RED: {label}")
    return 0 if not failed else 1


if __name__ == "__main__":
    raise SystemExit(main())
