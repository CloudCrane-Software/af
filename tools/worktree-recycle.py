#!/usr/bin/env python3
"""Worktree 三路回收（v0.3 T7 终裁④的可执行口径）。

三路规则（只此三路，删树必经其一）：
  1. merged  → 清：所属分支已合入主线，worktree 目录与登记一并移除；
  2. aborted → 留证 48h：中止的分支保留现场（目录+登记），48 小时后允许清；
  3. timeout → 超时止损：认领 lease 过期的树按止损强制回收（先标记再删）。

铁律：
  - lease TTL 到期 ≠ 删树。TTL 只表示「认领权过期」，重置认领；删树只走上述
    三路。TTL 与留证期是两个钟，不得互相替代。
  - 只回收本脚本登记薄（默认 <repo>/.af-worktree-registry.json）里有登记、且
    状态为 merged/aborted/timeout 的 worktree；未登记的一律不动。
  - 产品主线指针（product/main 等保护分支）永不删除、永不移动。
  - 「合入」判定用 `git merge-base --is-ancestor` 对本仓保护分支做真实判定，
    不采信登记簿里的自报状态（自报 merged 但未真合入 → 不清，改记 mismatch）。

用法（在仓库根或任意目录执行，--repo 指向 af 仓根）：
  worktree-recycle.py --repo /path/to/af mark <name> --state merged|aborted
  worktree-recycle.py --repo /path/to/af mark <name> --state active --ttl-min 120
  worktree-recycle.py --repo /path/to/af heartbeat <name>          # 续 lease（active 才可）
  worktree-recycle.py --repo /path/to/af recycle [--dry-run] [--evidence-hours 48]
  worktree-recycle.py --repo /path/to/af list

设计对齐本仓既有脚本（tools/gate-item-emit.py）的 argparse+stdlib 风格；
零第三方依赖，Python 3.10+。
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

# 保护分支：永不回收其登记（永不删主线指针）。
PROTECTED_BRANCHES = {"product/main", "main"}

# 三路状态机允许的登记状态。active=在用（带 lease）；merged/aborted=可回收前置态；
# timeout=止损态（由 recycle 判定 lease 过期时打上，或人工 mark）。
STATES = {"active", "merged", "aborted", "timeout"}


class RecycleError(Exception):
    pass


def _git(repo: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        capture_output=True,
        text=True,
        check=check,
    )


def _merged_into_protected(repo: Path, branch: str) -> bool:
    """真实合入判定：branch 是否已是任一保护分支的祖先（merge-base 判定）。

    仓里不存在的保护分支直接跳过（那条线上不可能已合入）；存在的保护分支上
    git 报非 0/1 错误才升级为异常（不吞错）。
    """
    for target in sorted(PROTECTED_BRANCHES):
        if not _branch_exists(repo, target):
            continue
        proc = _git(repo, "merge-base", "--is-ancestor", branch, target, check=False)
        if proc.returncode == 0:
            return True
        if proc.returncode not in (0, 1):
            raise RecycleError(f"merge-base 判定失败 {branch}→{target}: {proc.stderr.strip()}")
    return False


def _branch_exists(repo: Path, branch: str) -> bool:
    return _git(repo, "rev-parse", "--verify", "--quiet", branch, check=False).returncode == 0


def _wt_path(repo: Path, name: str) -> Path:
    return repo / ".af-worktrees" / name


def load_registry(repo: Path) -> dict:
    reg = repo / ".af-worktree-registry.json"
    if not reg.is_file():
        return {"worktrees": {}}
    data = json.loads(reg.read_text(encoding="utf-8"))
    if "worktrees" not in data:
        raise RecycleError(f"登记薄损坏（缺 worktrees 键）: {reg}")
    return data


def save_registry(repo: Path, data: dict) -> None:
    reg = repo / ".af-worktree-registry.json"
    reg.write_text(json.dumps(data, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
                   encoding="utf-8")


def cmd_mark(repo: Path, name: str, state: str, ttl_min: int, path: str | None) -> str:
    if state not in STATES:
        raise RecycleError(f"非法状态 {state}（允许：{sorted(STATES)}）")
    data = load_registry(repo)
    entry = data["worktrees"].get(name)
    now = time.time()
    if entry is None:
        entry = {"branch": name, "path": str(_wt_path(repo, name))}
    if path:
        entry["path"] = path
    entry.update({
        "state": state,
        "marked_at": now,
        # TTL 只对 active 有意义：到期=认领权过期（heartbeat 续期），不是删树钟。
        "lease_expires_at": now + ttl_min * 60 if state == "active" else None,
    })
    data["worktrees"][name] = entry
    save_registry(repo, data)
    return f"mark {name} -> {state}（lease={ttl_min}min，仅 active 计时）"


def cmd_heartbeat(repo: Path, name: str, ttl_min: int) -> str:
    data = load_registry(repo)
    entry = data["worktrees"].get(name)
    if entry is None:
        raise RecycleError(f"未登记: {name}")
    if entry.get("state") != "active":
        raise RecycleError(f"{name} 状态为 {entry.get('state')}，只有 active 可续 lease")
    entry["lease_expires_at"] = time.time() + ttl_min * 60
    save_registry(repo, data)
    return f"heartbeat {name}（lease 续 {ttl_min}min）"


def _remove_worktree(repo: Path, name: str, entry: dict, dry: bool) -> list[str]:
    """回收动作：删 worktree 目录 + git worktree remove + 删分支 + 销登记。"""
    actions: list[str] = []
    path = Path(entry.get("path") or _wt_path(repo, name))
    branch = entry.get("branch") or name
    if dry:
        actions.append(f"[dry] remove worktree {path}")
        if _branch_exists(repo, branch):
            actions.append(f"[dry] delete branch {branch}")
        actions.append(f"[dry] 消登记 {name}")
        return actions
    if path.is_dir():
        proc = _git(repo, "worktree", "remove", "--force", str(path), check=False)
        if proc.returncode != 0:
            # 目录已脏/已被手删：rd fallthrough 兜底，确保现场不悬
            import shutil
            shutil.rmtree(path, ignore_errors=True)
            actions.append(f"worktree remove rc={proc.returncode}，已兜底删目录 {path}")
        else:
            actions.append(f"已移除 worktree {path}")
    if _branch_exists(repo, branch) and branch not in PROTECTED_BRANCHES:
        _git(repo, "branch", "-D", branch)
        actions.append(f"已删分支 {branch}")
    return actions


def cmd_recycle(repo: Path, dry: bool, evidence_hours: float) -> str:
    data = load_registry(repo)
    now = time.time()
    evidence_seconds = evidence_hours * 3600
    report: list[str] = []
    for name, entry in sorted(data["worktrees"].items()):
        state = entry.get("state")
        branch = entry.get("branch") or name
        if branch in PROTECTED_BRANCHES:
            report.append(f"SKIP {name}: 保护分支 {branch} 永不回收")
            continue
        if state == "active":
            exp = entry.get("lease_expires_at")
            if exp is None:
                report.append(f"SKIP {name}: active 无 lease 字段（登记不完整，不动）")
            elif now < exp:
                report.append(f"KEEP {name}: active，lease 未到期（TTL≠删树）")
            else:
                # TTL 到期：重置认领为 timeout（止损），但**不删树**——删树只走
                # 下一次 recycle 对 timeout 态的三路判定。
                entry["state"] = "timeout"
                entry["lease_expires_at"] = None
                entry["timeout_at"] = now
                if not dry:
                    save_registry(repo, data)
                report.append(f"RECLAIM {name}: lease 过期 → 标记 timeout 止损（本轮不删树）")
            continue
        if state == "merged":
            # 不采信自报：真实合入才清；未合入改记 mismatch（红队口径：回收器
            # 自身不得成为丢活渠道）。
            if _merged_into_protected(repo, branch):
                actions = _remove_worktree(repo, name, entry, dry)
                if not dry:
                    data["worktrees"].pop(name, None)
                    save_registry(repo, data)
                report.append(f"CLEAN {name}: merged 且已真合入 → 清（{'；'.join(actions)}）")
            else:
                entry["mismatch"] = True
                if not dry:
                    save_registry(repo, data)
                report.append(f"KEEP {name}: 自报 merged 但未真合入（mismatch 留检）")
            continue
        if state == "aborted":
            marked = entry.get("marked_at") or 0
            age_h = (now - marked) / 3600
            if age_h >= evidence_hours:
                actions = _remove_worktree(repo, name, entry, dry)
                if not dry:
                    data["worktrees"].pop(name, None)
                    save_registry(repo, data)
                report.append(
                    f"CLEAN {name}: aborted 留证 {age_h:.1f}h ≥ {evidence_hours}h → 清"
                    f"（{'；'.join(actions)}）")
            else:
                report.append(
                    f"KEEP {name}: aborted 留证中 {age_h:.1f}h < {evidence_hours}h（48h 留证期）")
            continue
        if state == "timeout":
            # 止损路：lease 已过期标记，现场照 aborted 同等留证期保留（止损≠销证）。
            marked = entry.get("timeout_at") or entry.get("marked_at") or 0
            age_h = (now - marked) / 3600
            if age_h >= evidence_hours:
                actions = _remove_worktree(repo, name, entry, dry)
                if not dry:
                    data["worktrees"].pop(name, None)
                    save_registry(repo, data)
                report.append(
                    f"CLEAN {name}: timeout 止损后留证 {age_h:.1f}h ≥ {evidence_hours}h → 清"
                    f"（{'；'.join(actions)}）")
            else:
                report.append(
                    f"KEEP {name}: timeout 止损，现场留证 {age_h:.1f}h < {evidence_hours}h")
            continue
        report.append(f"SKIP {name}: 未知状态 {state!r}（不动）")
    return "\n".join(report) if report else "登记薄为空：无可回收项"


def cmd_list(repo: Path) -> str:
    data = load_registry(repo)
    if not data["worktrees"]:
        return "登记薄为空"
    lines = []
    now = time.time()
    for name, entry in sorted(data["worktrees"].items()):
        exp = entry.get("lease_expires_at")
        lease = f"lease 至 {time.strftime('%F %T', time.localtime(exp))}" if exp else "无 lease"
        lines.append(f"{name}: state={entry.get('state')} {lease} path={entry.get('path')}")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--repo", required=True, help="af 仓根（git worktree list 的主仓）")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_mark = sub.add_parser("mark", help="登记/更新一个 worktree 的三路状态")
    p_mark.add_argument("name")
    p_mark.add_argument("--state", required=True, choices=sorted(STATES))
    p_mark.add_argument("--ttl-min", type=int, default=120, help="active 的 lease TTL 分钟数")
    p_mark.add_argument("--path", default=None, help="worktree 实际路径（默认 <repo>/.af-worktrees/<name>）")

    p_hb = sub.add_parser("heartbeat", help="续 active 的 lease")
    p_hb.add_argument("name")
    p_hb.add_argument("--ttl-min", type=int, default=120)

    p_rc = sub.add_parser("recycle", help="按三路规则执行回收")
    p_rc.add_argument("--dry-run", action="store_true")
    p_rc.add_argument("--evidence-hours", type=float, default=48.0,
                      help="aborted/timeout 现场留证时长（小时）")

    sub.add_parser("list", help="列登记薄")

    args = ap.parse_args(argv)
    repo = Path(args.repo).resolve()
    if not (repo / ".git").exists() and not (repo / ".git").is_file():
        print(f"--repo 不是 git 仓根: {repo}", file=sys.stderr)
        return 2
    try:
        if args.cmd == "mark":
            print(cmd_mark(repo, args.name, args.state, args.ttl_min, args.path))
        elif args.cmd == "heartbeat":
            print(cmd_heartbeat(repo, args.name, args.ttl_min))
        elif args.cmd == "recycle":
            print(cmd_recycle(repo, args.dry_run, args.evidence_hours))
        elif args.cmd == "list":
            print(cmd_list(repo))
    except RecycleError as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
