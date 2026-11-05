#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""fixture_r5_crosstree.py — R5 跨树集合比较腿夹具（RT-E82 厂内重建批自写）。

在 af 工作树内对 ci/strategies_check.py 新增的 R5 腿做实况正反例（既有 R0-R4
判定不受影响；本夹具只构造 strategies/ 集合与 HEAD 树的差异并断言 R5 读数）：

  G0  基线一致态   ：全量门 exit 0，R5 INFO「跨树集合一致：65 件」
  V1  worktree-only：strategies/ 新增未入库文件（暂存）→ exit 1 且指名 only-in-worktree
  V2  head-only    ：git rm HEAD 有的辅助文件 → exit 1 且指名 only-in-head
  V3  content-diff ：HEAD 有的辅助文件内容改写 → exit 1 且指名 content-diff
  V4  改名漂移     ：git mv 同内容辅助文件 → exit 1 且指名两侧 + 改名漂移配对
  V5a 条目同型-新增：新增合法条目 YAML（暂存）→ exit 1 且指名 only-in-worktree
  V5b 条目同型-缺失：git rm 已登记条目 → exit 1 且指名 only-in-head
  V6  非 git 切片  ：拷贝树去 .git 且无 MANIFEST → exit 1 且指名 fail-closed
  V7  非 git 切片  ：同拷贝树提供 MANIFEST.json → exit 0 且 INFO 走 manifest 基线
  V7b 非 git 切片  ：MANIFEST 经 AF_CROSSTREE_MANIFEST 环境变量定位 → 同上
  G8  还原复绿     ：全量门 exit 0；同树两次全量门输出逐字节一致

选材说明：V1-V4 用 strategies/ 下的非条目辅助文件（README.md / 临时 txt），
除 R5 外不触发 R1-R3（R1 只glob st-*.yaml、R2/R3 只对 index.yaml 登记条目），
从而单腿隔离取证；V5a/V5b 用条目 YAML，演示条目级漂移同样被 R5 指名（此形态
必然伴随 R2/R3 红——index 登记面同步失效，属预期，不断言其缺席）。

纪律：每个用例 finally 强制还原；开跑前与收尾校验 strategies/ 工作树干净；
日志落本目录 logs/（--log-dir 覆盖）。退出码 0=全部断言通过。
"""
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent  # <worktree>/poc/poc-r5-crosstree -> <worktree>
GATE = ["ci/strategies_check.py"]
STRATS_REL = "strategies"
AUX_README = "strategies/README.md"        # HEAD 内、非条目、R1-R3 不碰
STRAY_REL = "strategies/zz-r5-fixture-stray.txt"
MOVED_REL = "strategies/README-r5-moved.md"
ENTRY_NEW_REL = "strategies/st-b-C999.yaml"
ENTRY_NEW_SRC = "strategies/st-b-C060.yaml"  # 既有合法 B 线 compilable 条目
ENTRY_RM_REL = "strategies/st-C005.yaml"     # index 已登记 A 线条目
NOGIT_COPY = Path("/data/bench/r5-nogit-copy")
ALT_MANIFEST = Path("/data/bench/r5-manifest-alt.json")

results: list[tuple[str, bool, str]] = []


def sh(*cmd, cwd=ROOT, check=False):
    return subprocess.run(cmd, cwd=str(cwd), capture_output=True, text=True, check=check)


def run_gate(log_dir: Path, name: str, skip_compile=True, cwd=ROOT, env_extra=None):
    cmd = [sys.executable, "-B", *GATE]
    if skip_compile:
        cmd.append("--skip-compile")
    env = dict(os.environ)
    if env_extra:
        env.update(env_extra)
    proc = subprocess.run(cmd, cwd=str(cwd), capture_output=True, text=True, env=env)
    text = (proc.stdout or "") + (proc.stderr or "")
    log_dir.mkdir(parents=True, exist_ok=True)
    (log_dir / f"{name}.log").write_text(
        f"$ {' '.join(cmd)}   # cwd={cwd}\nexit={proc.returncode}\n\n{text}", encoding="utf-8")
    print(f"---- {name}: exit={proc.returncode}（日志 logs/{name}.log）")
    for line in text.splitlines():
        if line.startswith(("[INFO] R5", "[FAIL] R5")) or "== strategies_check" in line:
            print(f"    {line}")
    return proc.returncode, text


def check(name: str, ok: bool, detail: str = ""):
    results.append((name, ok, detail))
    print(f"    [{'PASS' if ok else 'FAIL'}] {name}" + (f" —— {detail}" if detail and not ok else ""))


def restore_strats():
    """strategies/ 回到 HEAD 态：还原改动、清暂存、清未跟踪新增。"""
    sh("git", "checkout", "HEAD", "--", STRATS_REL)
    sh("git", "reset", "-q", "--", STRATS_REL)
    sh("git", "clean", "-fdq", STRATS_REL)


def strats_porcelain() -> str:
    return sh("git", "status", "--porcelain", "--", STRATS_REL).stdout.strip()


def head_strategies_digests() -> tuple[str, list[dict]]:
    rev = sh("git", "rev-parse", "HEAD").stdout.strip()
    ls = sh("git", "ls-tree", "-r", "--name-only", "HEAD", "--", STRATS_REL)
    files = []
    for rel in sorted(p for p in ls.stdout.splitlines() if p):
        blob = subprocess.run(["git", "-C", str(ROOT), "show", f"HEAD:{rel}"],
                              capture_output=True, check=True)
        files.append({"path": rel, "sha256": hashlib.sha256(blob.stdout).hexdigest()})
    return rev, files


def case_v1(log_dir):
    """worktree-only：新增未入库文件（暂存，隔离 R0 未跟踪红）。"""
    print("== V1 worktree-only（strategies/ 新增暂存文件）")
    try:
        (ROOT / STRAY_REL).write_text("r5 fixture stray\n", encoding="utf-8")
        sh("git", "add", STRAY_REL)
        rc, text = run_gate(log_dir, "V1-worktree-only")
        check("V1 exit=1", rc == 1, f"exit={rc}")
        check("V1 指名 only-in-worktree",
              f"R5 only-in-worktree：{STRAY_REL}" in text, text[-400:])
    finally:
        restore_strats()


def case_v2(log_dir):
    """head-only：HEAD 有、工作树无（辅助文件，隔离 R1-R3）。"""
    print("== V2 head-only（git rm 辅助文件 README.md）")
    try:
        sh("git", "rm", "-q", AUX_README)
        rc, text = run_gate(log_dir, "V2-head-only")
        check("V2 exit=1", rc == 1, f"exit={rc}")
        check("V2 指名 only-in-head",
              f"R5 only-in-head：{AUX_README}" in text, text[-400:])
        check("V2 不误伤 R1-R3（单腿红）",
              text.count("[FAIL]") == 1 and "R2" not in text and "R3" not in text,
              text[-400:])
    finally:
        restore_strats()


def case_v3(log_dir):
    """content-diff：同名内容改写（未暂存）。"""
    print("== V3 content-diff（辅助文件内容改写）")
    try:
        with (ROOT / AUX_README).open("a", encoding="utf-8") as fh:
            fh.write("\n<!-- r5 fixture touch -->\n")
        rc, text = run_gate(log_dir, "V3-content-diff")
        check("V3 exit=1", rc == 1, f"exit={rc}")
        check("V3 指名 content-diff",
              f"R5 content-diff：{AUX_README}" in text, text[-400:])
        check("V3 单腿红（不误伤 R1-R3）",
              text.count("[FAIL]") == 1, text[-400:])
    finally:
        restore_strats()


def case_v4(log_dir):
    """改名漂移：同内容换路径（git mv，暂存态）。"""
    print("== V4 改名漂移（git mv 辅助文件）")
    try:
        sh("git", "mv", AUX_README, MOVED_REL)
        rc, text = run_gate(log_dir, "V4-rename-drift")
        check("V4 exit=1", rc == 1, f"exit={rc}")
        check("V4 指名 only-in-head 旧路径",
              f"R5 only-in-head：{AUX_README}" in text, text[-400:])
        check("V4 指名 only-in-worktree 新路径",
              f"R5 only-in-worktree：{MOVED_REL}" in text, text[-400:])
        check("V4 点名漂移配对",
              "R5 改名漂移" in text and AUX_README in text and MOVED_REL in text,
              text[-400:])
    finally:
        restore_strats()


def case_v5a(log_dir):
    """条目同型-新增：合法条目 YAML 暂存（R2/R3 同步红属预期）。"""
    print("== V5a 条目同型-新增（st-b-C999.yaml 暂存）")
    try:
        src = (ROOT / ENTRY_NEW_SRC).read_text(encoding="utf-8")
        (ROOT / ENTRY_NEW_REL).write_text(src.replace("st-b-C060", "st-b-C999"), encoding="utf-8")
        sh("git", "add", ENTRY_NEW_REL)
        rc, text = run_gate(log_dir, "V5a-entry-new")
        check("V5a exit=1", rc == 1, f"exit={rc}")
        check("V5a 指名 only-in-worktree（条目级）",
              f"R5 only-in-worktree：{ENTRY_NEW_REL}" in text, text[-400:])
        check("V5a 伴随 R2 未登记红（预期）",
              "R2 磁盘条目 st-b-C999 未登记进 index" in text, text[-400:])
    finally:
        restore_strats()


def case_v5b(log_dir):
    """条目同型-缺失：git rm 已登记条目（R2/R3 同步红属预期）。"""
    print("== V5b 条目同型-缺失（git rm st-C005.yaml）")
    try:
        sh("git", "rm", "-q", ENTRY_RM_REL)
        rc, text = run_gate(log_dir, "V5b-entry-rm")
        check("V5b exit=1", rc == 1, f"exit={rc}")
        check("V5b 指名 only-in-head（条目级）",
              f"R5 only-in-head：{ENTRY_RM_REL}" in text, text[-400:])
        check("V5b 伴随 R2 缺席红（预期）",
              "R2 index 条目 st-C005" in text, text[-400:])
    finally:
        restore_strats()


def _prepare_nogit_copy() -> Path:
    if NOGIT_COPY.exists():
        shutil.rmtree(NOGIT_COPY)
    shutil.copytree(ROOT, NOGIT_COPY,
                    ignore=shutil.ignore_patterns(".git", "target", "logs"))
    shutil.rmtree(NOGIT_COPY / ".git", ignore_errors=True)
    return NOGIT_COPY


def case_v6(log_dir):
    """非 git 切片无 MANIFEST：fail-closed 红。"""
    print("== V6 非 git 切片 fail-closed（无 .git 无 MANIFEST）")
    copy = _prepare_nogit_copy()
    manifest = copy / "MANIFEST.json"
    if manifest.exists():
        manifest.unlink()
    env = dict(os.environ)
    env.pop("AF_CROSSTREE_MANIFEST", None)
    cmd = [sys.executable, "-B", *GATE, "--skip-compile"]
    proc = subprocess.run(cmd, cwd=str(copy), capture_output=True, text=True, env=env)
    text = (proc.stdout or "") + (proc.stderr or "")
    (log_dir / "V6-nogit-failclosed.log").write_text(
        f"$ {' '.join(cmd)}   # cwd={copy}\nexit={proc.returncode}\n\n{text}", encoding="utf-8")
    print(f"---- V6: exit={proc.returncode}（日志 logs/V6-nogit-failclosed.log）")
    for line in text.splitlines():
        if line.startswith(("[INFO] R0", "[INFO] R5", "[FAIL] R5")) or "== strategies_check" in line:
            print(f"    {line}")
    check("V6 exit=1", proc.returncode == 1, f"exit={proc.returncode}")
    check("V6 fail-closed 指名",
          "R5 跨树集合无法判定（fail-closed，不静默放行）" in text, text[-400:])
    check("V6 诊断含双源不可用",
          "无 HEAD 基线（git：" in text and "manifest：" in text, text[-400:])
    check("V6 R0 如实跳过（非 git 形态）",
          "R0 入库对账跳过" in text, text[-400:])


def _write_manifest(target: Path) -> dict:
    rev, files = head_strategies_digests()
    payload = {"base_sha": rev, "files": files}
    target.write_text(json.dumps(payload, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    return payload


def _run_manifest_case(log_dir, name, copy: Path, env_extra=None, expect_manifest_line=True):
    env = dict(os.environ)
    env.pop("AF_CROSSTREE_MANIFEST", None)
    if env_extra:
        env.update(env_extra)
    cmd = [sys.executable, "-B", *GATE, "--skip-compile"]
    proc = subprocess.run(cmd, cwd=str(copy), capture_output=True, text=True, env=env)
    text = (proc.stdout or "") + (proc.stderr or "")
    (log_dir / f"{name}.log").write_text(
        f"$ {' '.join(cmd)}   # cwd={copy} env={env_extra or {}}\nexit={proc.returncode}\n\n{text}",
        encoding="utf-8")
    print(f"---- {name}: exit={proc.returncode}（日志 logs/{name}.log）")
    for line in text.splitlines():
        if line.startswith(("[INFO] R0", "[INFO] R5")) or "== strategies_check" in line:
            print(f"    {line}")
    check(f"{name} exit=0", proc.returncode == 0, f"exit={proc.returncode}\n{text[-400:]}")
    check(f"{name} R5 一致（manifest 基线）",
          "R5 跨树集合一致：65 件" in text, text[-400:])
    if expect_manifest_line:
        check(f"{name} 基线来源=MANIFEST", "基线 MANIFEST" in text, text[-400:])


def case_v7(log_dir):
    """非 git 切片 + 仓根 MANIFEST.json：回退基线绿。"""
    print("== V7 非 git 切片 MANIFEST 回退（仓根 MANIFEST.json）")
    copy = _prepare_nogit_copy()
    payload = _write_manifest(copy / "MANIFEST.json")
    check("V7 MANIFEST 覆盖 65 件", len(payload["files"]) == 65, str(len(payload["files"])))
    _run_manifest_case(log_dir, "V7-manifest-fallback", copy)


def case_v7b(log_dir):
    """非 git 切片 + AF_CROSSTREE_MANIFEST 环境变量定位：回退基线绿。"""
    print("== V7b 非 git 切片 MANIFEST 回退（环境变量定位）")
    copy = _prepare_nogit_copy()
    _write_manifest(ALT_MANIFEST)
    _run_manifest_case(log_dir, "V7b-manifest-env", copy,
                       env_extra={"AF_CROSSTREE_MANIFEST": str(ALT_MANIFEST)})


def case_g8(log_dir):
    """还原复绿 + 同树两次全量门输出逐字节一致。"""
    print("== G8 还原复绿 + 双跑一致性（全量门，含 R4）")
    check("G8 开跑前 strategies/ 干净", strats_porcelain() == "", strats_porcelain())
    rc1, text1 = run_gate(log_dir, "G8-restored-green", skip_compile=False)
    check("G8 全量门 exit=0", rc1 == 0, text1[-400:])
    check("G8 R5 一致 INFO", "R5 跨树集合一致：65 件" in text1, text1[-400:])
    check("G8 R4 编译断言在列", "编译断言经 af-dsl 装载测试" in text1, text1[-400:])
    cmd = [sys.executable, "-B", *GATE]
    p1 = subprocess.run(cmd, cwd=str(ROOT), capture_output=True, text=True)
    p2 = subprocess.run(cmd, cwd=str(ROOT), capture_output=True, text=True)
    same = p1.stdout == p2.stdout and p1.returncode == p2.returncode
    (log_dir / "G8-twice-run-a.log").write_text(
        f"$ {' '.join(cmd)}\nexit={p1.returncode}\n\n{p1.stdout}", encoding="utf-8")
    (log_dir / "G8-twice-run-b.log").write_text(
        f"$ {' '.join(cmd)}\nexit={p2.returncode}\n\n{p2.stdout}", encoding="utf-8")
    check("G8 同树两次输出逐字节一致", same,
          f"rc={p1.returncode}/{p2.returncode} identical={p1.stdout == p2.stdout}")
    r5a = [l for l in p1.stdout.splitlines() if "R5" in l]
    r5b = [l for l in p2.stdout.splitlines() if "R5" in l]
    check("G8 两次 R5 读数一致", r5a == r5b, f"{r5a} vs {r5b}")
    check("G8 收尾 strategies/ 干净", strats_porcelain() == "", strats_porcelain())


def main() -> int:
    ap = argparse.ArgumentParser(description="R5 跨树集合比较腿夹具（RT-E82）")
    ap.add_argument("--log-dir", default=str(HERE / "logs"))
    args = ap.parse_args()
    log_dir = Path(args.log_dir)

    if strats_porcelain():
        print(f"[fixture] 拒绝开跑：strategies/ 工作树不干净：\n{strats_porcelain()}")
        return 2
    gate_file = ROOT / "ci" / "strategies_check.py"
    if "check_crosstree" not in gate_file.read_text(encoding="utf-8"):
        print("[fixture] 拒绝开跑：ci/strategies_check.py 无 R5 腿（check_crosstree 缺席）")
        return 2

    print(f"== G0 基线一致态（全量门，含 R4）  root={ROOT}")
    rc0, text0 = run_gate(log_dir, "G0-baseline-green", skip_compile=False)
    check("G0 exit=0", rc0 == 0, text0[-400:])
    check("G0 R5 一致 INFO", "R5 跨树集合一致：65 件" in text0, text0[-400:])

    for case in (case_v1, case_v2, case_v3, case_v4, case_v5a, case_v5b,
                 case_v6, case_v7, case_v7b, case_g8):
        try:
            case(log_dir)
        finally:
            restore_strats()

    bad = [r for r in results if not r[1]]
    print(f"\n== 夹具汇总：{len(results) - len(bad)}/{len(results)} 断言通过")
    for name, ok, detail in results:
        print(f"   [{'PASS' if ok else 'FAIL'}] {name}")
    if NOGIT_COPY.exists():
        shutil.rmtree(NOGIT_COPY, ignore_errors=True)
    if ALT_MANIFEST.exists():
        ALT_MANIFEST.unlink()
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
