#!/usr/bin/env python3
"""改前/改后行为对照驱动器（poc 证据采集，非测试件）。

用法：python3 -I behavior-diff.py --script <worktree-recycle.py 路径> [--title 标题]
对同一组一次性夹具仓跑四组地雷场景，采「树目录 / 未提交改动 / 分支 / 提交对象 /
报告原文」读数，输出 markdown。改前脚本用
`git show 66be37c:tools/worktree-recycle.py` 取回归位。

场景：
  E1 脏树（真合入 merged + 未提交改动）——活雷①：--force 连脏面一起删 + -D 删分支；
  E2 未合入分支（aborted 过 48h）——活雷②：-D 未合入也删；
  E3 merged 干净树（默认，无开关）——活雷③：默认就删分支；
  E4 误登记路径（--path 指向非 worktree 的普通目录）——活雷④：remove 失败走
     rmtree 兜底，整目录连带无关文件被无声抹掉。
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path


def sh(repo: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True)


def tool(script: Path, repo: Path, *args: str) -> str:
    proc = subprocess.run([sys.executable, "-I", str(script), "--repo", str(repo), *args],
                          capture_output=True, text=True)
    return (proc.stdout + proc.stderr).strip()


def fixture(root: Path, name: str, merged: bool = True) -> tuple[Path, Path, str]:
    repo = root / name
    repo.mkdir(parents=True)
    sh(repo, "init", "-q", "-b", "main")
    sh(repo, "config", "user.email", "poc@t")
    sh(repo, "config", "user.name", "poc")
    (repo / "f.txt").write_text("base\n", encoding="utf-8")
    sh(repo, "add", "-A")
    sh(repo, "commit", "-qm", "base")
    sh(repo, "checkout", "-qb", "feat")
    (repo / "g.txt").write_text("work\n", encoding="utf-8")
    sh(repo, "add", "-A")
    sh(repo, "commit", "-qm", "work")
    tip = sh(repo, "rev-parse", "feat").stdout.strip()
    sh(repo, "checkout", "-q", "main")
    if merged:
        sh(repo, "merge", "-q", "--no-ff", "feat", "-m", "merge feat")
        sh(repo, "worktree", "add", str(repo / ".af-worktrees" / "feat"), "feat")
    return repo, repo / ".af-worktrees" / "feat", tip


def branch_exists(repo: Path, branch: str) -> bool:
    return sh(repo, "branch", "--list", branch).stdout.strip() != ""


def tip_readable(repo: Path, tip: str) -> str:
    proc = sh(repo, "cat-file", "-t", tip)
    return proc.stdout.strip() if proc.returncode == 0 else f"读不出（{proc.stderr.strip()}）"


def readings(repo: Path, wt: Path, tip: str) -> str:
    dirty = ""
    f = wt / "f.txt"
    if f.is_file():
        dirty = "保留，内容=" + repr(f.read_text(encoding="utf-8"))
    elif wt.is_dir():
        dirty = "目录在但 f.txt 不见了"
    else:
        dirty = "（目录已不存在）"
    return (f"  - 树目录: {'仍在' if wt.is_dir() else '已被删'}\n"
            f"  - 未提交改动: {dirty}\n"
            f"  - 分支 feat: {'仍在' if branch_exists(repo, 'feat') else '已被删'}\n"
            f"  - feat 提交对象 cat-file -t: {tip_readable(repo, tip)}")


def backdate(repo: Path, name: str, hours: float) -> None:
    reg = repo / ".af-worktree-registry.json"
    data = json.loads(reg.read_text(encoding="utf-8"))
    data["worktrees"][name]["marked_at"] = time.time() - hours * 3600
    reg.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--script", required=True, type=Path)
    ap.add_argument("--title", default="读数")
    args = ap.parse_args()
    root = Path(tempfile.mkdtemp(prefix="recycle-poc-"))
    out = [f"## {args.title}（script={args.script}）"]

    # E1 脏树：真合入 merged + 未提交改动
    repo, wt, tip = fixture(root, "e1")
    (wt / "f.txt").write_text("base\n未提交的活\n", encoding="utf-8")
    tool(args.script, repo, "mark", "feat", "--state", "merged")
    rep = tool(args.script, repo, "recycle")
    out += ["### E1 脏树（真合入 merged + f.txt 未提交改动）",
            f"```\n{rep}\n```", readings(repo, wt, tip)]

    # E2 未合入分支：aborted 过 48h 留证期
    repo, wt, tip = fixture(root, "e2", merged=False)
    sh(repo, "worktree", "add", str(wt), "feat")  # feat 未合入 main
    tool(args.script, repo, "mark", "feat", "--state", "aborted")
    backdate(repo, "feat", 49.0)
    rep = tool(args.script, repo, "recycle")
    out += ["### E2 未合入分支（aborted 已过 48h 留证期）",
            f"```\n{rep}\n```", readings(repo, wt, tip)]

    # E3 merged 干净树，默认无开关
    repo, wt, tip = fixture(root, "e3")
    tool(args.script, repo, "mark", "feat", "--state", "merged")
    rep = tool(args.script, repo, "recycle")
    out += ["### E3 merged 干净树（默认，无任何开关）",
            f"```\n{rep}\n```", readings(repo, wt, tip)]

    # E4 误登记路径：--path 指向非 worktree 的普通目录
    repo, _, _ = fixture(root, "e4", merged=False)
    stray = repo / "innocent-dir"
    stray.mkdir()
    (stray / "无关文件.txt").write_text("与 git 无关的活\n", encoding="utf-8")
    tool(args.script, repo, "mark", "stray", "--state", "aborted", "--path", str(stray))
    backdate(repo, "stray", 49.0)
    rep = tool(args.script, repo, "recycle")
    kept = (stray / "无关文件.txt").is_file()
    out += ["### E4 误登记路径（--path 指向普通目录 innocent-dir/，内有无关文件）",
            f"```\n{rep}\n```",
            f"  - innocent-dir: {'仍在' if stray.is_dir() else '已被删'}",
            f"  - 无关文件.txt: {'仍在' if kept else '已被删'}"]

    print("\n".join(out))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
