#!/usr/bin/env python3
"""T7② integrator 复核器自证：正常记录件 + 削弱变体逐个跑（逐个红）。

跑法（工作树根下）：
    python3 -I poc/poc-t7-2-integrator/selftest.py --out poc/poc-t7-2-integrator/raw

自证面：
  ① 正常记录件 ⇒ 复核器 exit 0（绿）。
  ② 削弱变体逐个 ⇒ 复核器 exit 非 0（红）。变体清单：
       base_sha 改一位 / source_sha 改一位 / tested_tree 换成另一棵树的 /
       suite_digest 改一位 / verdict="fail" / verdict="PASS"（大小写差异）/
       某项为空串 / 某项缺字段 / 门摘要被抹（门不可跑）/ 门摘要被改（摘要不自证）/
       门退出码非零 / 记录件缺席
  ③ 夹具说明：门脚本是**真跑**的（fixture 的 ci/run.sh 真执行、真写 SUMMARY），
     git 读数全来自夹具仓真实提交，不是手编的常量。

★ 纪律：任何一个变体仍绿 ⇒ 本脚本自身判红（exit 1）——不许为让变体绿而放宽判定。
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]
CHECKER = REPO_ROOT / "ci" / "integrator_bind_check.py"

FIXTURE_RUN_SH = """#!/usr/bin/env bash
# 夹具门（真跑）：step1 判内容、step2 判存在，两步都过 ⇒ PIPELINE-GREEN。
set -u -o pipefail
OUT="$1"
mkdir -p "$OUT"
rc=0
if grep -q "FIXTURE-OK" crates/demo/lib.rs 2>/dev/null; then
  echo "step1  PASS  1ms  fixture fmt" >> "$OUT/SUMMARY.txt"
else
  echo "step1  FAIL  1ms  fixture fmt" >> "$OUT/SUMMARY.txt"; rc=1
fi
if [ -f ci/clippy_gate.py ]; then
  echo "step2  PASS  1ms  fixture clippy" >> "$OUT/SUMMARY.txt"
else
  echo "step2  FAIL  1ms  fixture clippy" >> "$OUT/SUMMARY.txt"; rc=1
fi
if [ "$rc" -eq 0 ]; then
  echo "PIPELINE-GREEN" >> "$OUT/SUMMARY.txt"
else
  echo "PIPELINE-RED (exit 1)" >> "$OUT/SUMMARY.txt"
fi
exit "$rc"
"""


def git(repo: Path, *args: str, env: dict[str, str] | None = None) -> str:
    proc = subprocess.run(
        ["git", *args],
        cwd=str(repo),
        capture_output=True,
        text=True,
        env=env or {**os.environ, "GIT_AUTHOR_NAME": "t7-2", "GIT_AUTHOR_EMAIL": "t7-2@local",
                    "GIT_COMMITTER_NAME": "t7-2", "GIT_COMMITTER_EMAIL": "t7-2@local"},
    )
    if proc.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} 失败: {proc.stderr.strip()}")
    return proc.stdout


def build_fixture(root: Path) -> dict[str, Any]:
    """搭一个最小但真实的夹具仓：主线 main + 候选分支 feat（可快进）+ 真跑的门。"""
    repo = root / "fixture-repo"
    (repo / "ci" / "baselines").mkdir(parents=True)
    (repo / "crates" / "demo").mkdir(parents=True)
    (repo / "ci" / "run.sh").write_text(FIXTURE_RUN_SH, encoding="utf-8")
    (repo / "ci" / "clippy_gate.py").write_text("# 夹具 clippy 门\nprint('FIXTURE-OK')\n", encoding="utf-8")
    (repo / "ci" / "baselines" / "gate-inputs.sha256").write_text("0000  ci/clippy_gate.py\n", encoding="utf-8")
    (repo / "crates" / "demo" / "lib.rs").write_text("// FIXTURE-OK\n", encoding="utf-8")
    git(repo, "init", "-b", "main")
    git(repo, "add", "-A")
    git(repo, "commit", "-m", "fixture: 基线主线")
    base_commit = git(repo, "rev-parse", "HEAD").strip()

    git(repo, "checkout", "-b", "feat/2026-11-04-batchA")
    (repo / "crates" / "demo" / "extra.rs").write_text("// 候选改动\n", encoding="utf-8")
    git(repo, "add", "-A")
    git(repo, "commit", "-m", "fixture: 候选提交")
    cand_commit = git(repo, "rev-parse", "HEAD").strip()
    return {
        "repo": repo,
        "base_commit": base_commit,
        "cand_commit": cand_commit,
        "branch": "feat/2026-11-04-batchA",
        "base_ref": "main",
    }


def run_checker(args: list[str], cwd: Path) -> tuple[int, str, str]:
    proc = subprocess.run(
        [sys.executable, "-I", str(CHECKER), *args],
        cwd=str(cwd),
        capture_output=True,
        text=True,
    )
    return proc.returncode, proc.stdout, proc.stderr


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0] if __doc__ else None)
    parser.add_argument("--out", default="", help="原始读数落盘目录")
    parser.add_argument("--keep", action="store_true", help="保留夹具目录（调试用）")
    args = parser.parse_args(argv)

    out_dir = Path(args.out).resolve() if args.out else Path(tempfile.mkdtemp(prefix="t7-2-raw-"))
    out_dir.mkdir(parents=True, exist_ok=True)

    root = Path(tempfile.mkdtemp(prefix="t7-2-fixture-"))
    readings: list[dict[str, Any]] = []
    failures: list[str] = []
    try:
        fx = build_fixture(root)
        repo: Path = fx["repo"]

        # ── 门真跑（fixture 的 ci/run.sh 真执行）────────────────────────────
        gate_out = root / "gate-out"
        gate_proc = subprocess.run(
            ["bash", "ci/run.sh", str(gate_out)], cwd=str(repo), capture_output=True, text=True
        )
        summary = gate_out / "SUMMARY.txt"
        print(f"[夹具] 门跑完 exit={gate_proc.returncode}；SUMMARY={summary}")
        if gate_proc.returncode != 0 or not summary.is_file():
            failures.append("夹具门没跑绿/没产 SUMMARY——自证前提不成立")

        # ── 记录侧（emit）───────────────────────────────────────────────────
        record_path = root / "record.json"
        code, stdout, stderr = run_checker(
            [
                "emit",
                "--record", str(record_path),
                "--repo", str(repo),
                "--branch", fx["branch"],
                "--remote", str(repo),          # 夹具仓自身即"远端"（ls-remote 读本地 ref）
                "--base-ref", fx["base_ref"],
                "--source", fx["cand_commit"],
                "--gate-id", "fixture-fast",
                "--gate-command", "bash ci/run.sh",
                "--gate-exit", str(gate_proc.returncode),
                "--gate-summary", str(summary),
                "--recorded-by", "t7-2-selftest",
            ],
            cwd=REPO_ROOT,
        )
        print(f"[夹具] emit exit={code} {stdout.strip()} {stderr.strip()}")
        if code != 0:
            failures.append(f"emit 失败 exit={code}: {stderr.strip()}")
        record = json.loads(record_path.read_text(encoding="utf-8"))

        def verify(tag: str, mutated: dict[str, Any] | None, *, expect_zero: bool) -> dict[str, Any]:
            """跑一次复核器并记账。mutated=None ⇒ 用原记录件。"""
            target = record_path
            tmp = None
            if mutated is not None:
                tmp = root / f"record-{tag}.json"
                tmp.write_text(json.dumps(mutated, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8")
                target = tmp
            code, stdout, stderr = run_checker(
                [
                    "verify",
                    "--record", str(target),
                    "--repo", str(repo),
                    "--branch", fx["branch"],
                    "--remote", str(repo),
                    "--base-ref", fx["base_ref"],
                    "--json",
                ],
                cwd=REPO_ROOT,
            )
            ok = (code == 0) if expect_zero else (code != 0)
            reason = ""
            try:
                reason = json.loads(stdout).get("reason_code", "") or json.loads(stderr).get("reason_code", "")
            except (json.JSONDecodeError, ValueError):
                reason = (stderr.strip().splitlines() or [""])[0][:120]
            entry = {
                "case": tag,
                "expect": "exit 0（绿）" if expect_zero else "exit 非 0（红）",
                "exit": code,
                "reason_code": reason,
                "pass": ok,
                "stdout": stdout[:2000],
                "stderr": stderr[:2000],
            }
            readings.append(entry)
            flag = "OK " if ok else "FAIL"
            print(f"[自证] {flag} {tag:28s} exit={code} reason_code={reason}")
            if not ok:
                failures.append(f"变体 {tag} 未按预期（exit={code}，期望 {entry['expect']}）")
            return entry

        # ── ① 正常记录件 ⇒ 绿 ───────────────────────────────────────────────
        verify("baseline-normal", None, expect_zero=True)

        # ── ② 削弱变体逐个 ⇒ 红 ─────────────────────────────────────────────
        def flip(value: str) -> str:
            """改一位（末位十六进制加一，保持长度与形态）。"""
            last = value[-1]
            return value[:-1] + ("0" if last != "0" else "1")

        base_tree = git(repo, "rev-parse", f"{fx['base_commit']}^{{tree}}").strip()

        variants: list[tuple[str, dict[str, Any]]] = []
        for key in ("base_sha", "source_sha", "suite_digest"):
            mutated = dict(record)
            mutated[key] = flip(record[key])
            variants.append((f"{key}-改一位", mutated))

        mutated = dict(record)
        mutated["tested_tree"] = base_tree          # 换成另一棵树的（基线树的）
        variants.append(("tested_tree-换另一棵树", mutated))

        mutated = dict(record)
        mutated["verdict"] = "fail"
        variants.append(('verdict-"fail"', mutated))

        mutated = dict(record)
        mutated["verdict"] = "PASS"                  # 大小写差异也必须红
        variants.append(('verdict-"PASS"（大小写）', mutated))

        mutated = dict(record)
        mutated["verdict"] = ""
        variants.append(('verdict-空串', mutated))

        for key in ("base_sha", "source_sha", "tested_tree", "suite_digest"):
            mutated = dict(record)
            mutated[key] = ""
            variants.append((f"{key}-空串", mutated))
            mutated = dict(record)
            mutated.pop(key)
            variants.append((f"{key}-缺字段", mutated))

        mutated = dict(record)
        mutated["gate_summary_text"] = ""            # 门不可跑（摘要被抹）
        variants.append(("门摘要-抹掉（门不可跑）", mutated))

        mutated = dict(record)
        mutated["gate_summary_text"] = mutated["gate_summary_text"].replace("PASS", "FAIL")
        variants.append(("门摘要-被改（不自证）", mutated))

        mutated = dict(record)
        mutated["gate_exit"] = 1
        variants.append(("门退出码-非零", mutated))

        for tag, mutated in variants:
            verify(tag, mutated, expect_zero=False)

        # ── ③ 记录件缺席 ⇒ 红 ───────────────────────────────────────────────
        missing = root / "record-absent.json"
        code, stdout, stderr = run_checker(
            [
                "verify",
                "--record", str(missing),
                "--repo", str(repo),
                "--branch", fx["branch"],
                "--remote", str(repo),
                "--base-ref", fx["base_ref"],
                "--json",
            ],
            cwd=REPO_ROOT,
        )
        ok = code != 0
        readings.append({
            "case": "记录件-缺席",
            "expect": "exit 非 0（红）",
            "exit": code,
            "reason_code": "RECORD-ABSENT" if "RECORD-ABSENT" in (stdout + stderr) else "",
            "pass": ok,
        })
        print(f"[自证] {'OK ' if ok else 'FAIL'} 记录件-缺席              exit={code}")
        if not ok:
            failures.append("记录件缺席未按预期红")

        # ── 落盘原始读数 ────────────────────────────────────────────────────
        raw = {
            "fixture": {
                "repo": str(repo),
                "base_commit": fx["base_commit"],
                "cand_commit": fx["cand_commit"],
                "branch": fx["branch"],
                "base_ref": fx["base_ref"],
                "gate_summary": summary.read_text(encoding="utf-8"),
                "gate_exit": gate_proc.returncode,
            },
            "record": record,
            "readings": readings,
            "failures": failures,
            "verdict": "SELFTEST-GREEN" if not failures else "SELFTEST-RED",
        }
        (out_dir / "selftest-readings.json").write_text(
            json.dumps(raw, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8"
        )
        print(f"\n[自证] 结论：{raw['verdict']}；读数落盘 {out_dir / 'selftest-readings.json'}")
        if failures:
            for item in failures:
                print(f"  RED: {item}")
            return 1
        return 0
    finally:
        if not args.keep:
            shutil.rmtree(root, ignore_errors=True)
        else:
            print(f"[夹具] 保留目录 {root}")


if __name__ == "__main__":
    raise SystemExit(main())
