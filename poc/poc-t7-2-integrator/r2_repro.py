#!/usr/bin/env python3
"""★ v0.3-T7② 返修批：复核员实测场景的**独立复现**脚本（两项必修补 + 三项建议）。

跑法（工作树根下）：
    python3 -I poc/poc-t7-2-integrator/r2_repro.py --out poc/poc-t7-2-integrator/raw

本脚本**只复现复核员点名的场景**并逐条给读数（不改判据、不造绿）：
  复现 1（必修①）：门摘要 `step1 PASS / step9 ADVISORY-PASS / step14 PASS-WITH-WARN`
            ⇒ **初版 emit exit=2「门摘要判不出绿」**（合法绿门被当红）⇒ 现在必须 exit=0。
  复现 2（必修②）：把记录件内嵌摘要洗成全 PASS 行并**同步重算 sha256**（磁盘文件不动）
            ⇒ **初版 verify exit=0 / reason=OK**（门摘要原文可被记录件作者完全控制）
            ⇒ 现在必须 exit=1 / GATE-SUMMARY-DIGEST。
  复现 3（必修②）：把**磁盘上的**门摘要文件改掉（记录件不动）⇒ 必须红 GATE-SUMMARY-DIGEST。
  复现 4（建议①）：`verify --source <本地提交>`（不带 --allow-local-source）
            ⇒ 初版 exit=0（绕开「远端该分支头」绑定）⇒ 现在必须红 LOCAL-SOURCE-NOT-ALLOWED。
  复现 5（建议①）：`verify --source <本地提交> --allow-local-source` ⇒ 绿，且输出必须留
            `source_origin=本地提交…`（事后可区分远端头 vs 本地提交）。
  复现 6（词表）：`step14 FAIL` / `step9 ADVISORY-FAIL` ⇒ emit 必须**拒写**记录件（红）。

夹具与门都是**真跑**的（复用 selftest.py 的夹具构造：fixture 的 ci/run.sh 真执行、真写 SUMMARY）。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
# ★ 本脚本 import 同目录的 selftest（复用夹具构造）⇒ 必须在 import **之前**关掉字节码写入，
#   否则会落 `poc/poc-t7-2-integrator/__pycache__/`（污染工作树、违反"零 __pycache__"）。
sys.dont_write_bytecode = True
import selftest  # noqa: E402  （复用夹具构造，保证两脚本夹具同源）

REPO_ROOT = Path(__file__).resolve().parents[2]
CHECKER = REPO_ROOT / "ci" / "integrator_bind_check.py"

GREEN_SUMMARY = (
    "step1  PASS  1ms  门摘要复现\n"
    "step9  ADVISORY-PASS  1ms  FV advisory\n"
    "step14  PASS-WITH-WARN  2ms  机读一致性批\n"
)
WASHED_SUMMARY = (
    "step1  PASS  1ms  门摘要复现\n"
    "step9  ADVISORY-PASS  1ms  FV advisory\n"
    "step14  PASS  1ms  机读一致性批\n"
)


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def run(args: list[str]) -> tuple[int, str, str]:
    proc = subprocess.run(
        [sys.executable, "-I", str(CHECKER), *args],
        cwd=str(REPO_ROOT),
        capture_output=True,
        text=True,
    )
    return proc.returncode, proc.stdout, proc.stderr


def reason_of(stdout: str, stderr: str) -> str:
    for blob in (stdout, stderr):
        try:
            payload = json.loads(blob)
        except json.JSONDecodeError:
            continue
        if isinstance(payload, dict) and payload.get("reason_code"):
            return str(payload["reason_code"])
    return (stderr.strip().splitlines() or [""])[0][:160]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0] if __doc__ else None)
    parser.add_argument("--out", default="", help="原始读数落盘目录")
    args = parser.parse_args(argv)

    out_dir = Path(args.out).resolve() if args.out else Path(tempfile.mkdtemp(prefix="t7-2-r2-"))
    out_dir.mkdir(parents=True, exist_ok=True)

    root = Path(tempfile.mkdtemp(prefix="t7-2-r2-fixture-"))
    cases: list[dict[str, Any]] = []
    failures: list[str] = []
    lines: list[str] = []

    def record(name: str, note: str, code: int, reason: str, expect_zero: bool, stdout: str, stderr: str) -> None:
        ok = (code == 0) if expect_zero else (code != 0)
        cases.append({
            "case": name,
            "note": note,
            "exit": code,
            "reason_code": reason,
            "expect": "exit 0（绿）" if expect_zero else "exit 非 0（红）",
            "pass": ok,
        })
        lines.append(
            f"[复现] {'OK ' if ok else 'FAIL'} {name}\n"
            f"        说明：{note}\n"
            f"        命令退出码={code}  reason_code={reason or '（无/绿）'}  期望={cases[-1]['expect']}"
        )
        if not ok:
            failures.append(f"{name} 未按预期（exit={code}）")

    try:
        fx = selftest.build_fixture(root)
        repo: Path = fx["repo"]
        cand: str = fx["cand_commit"]
        branch: str = fx["branch"]
        base_ref: str = fx["base_ref"]

        summary_file = root / "SUMMARY.txt"
        summary_file.write_text(GREEN_SUMMARY, encoding="utf-8")
        record_path = root / "record.json"

        # ── 复现 1：合法绿门（step14 PASS-WITH-WARN）必须能写记录件 ──────────
        code, stdout, stderr = run([
            "emit", "--record", str(record_path), "--repo", str(repo),
            "--branch", branch, "--remote", str(repo), "--base-ref", base_ref,
            "--source", cand, "--gate-id", "repro", "--gate-command", "bash ci/run.sh",
            "--gate-exit", "0", "--gate-summary", str(summary_file), "--recorded-by", "r2-repro",
        ])
        record(
            "复现1-emit-PASS-WITH-WARN（初版 exit=2 假红）",
            "门摘要 step1 PASS / step9 ADVISORY-PASS / step14 PASS-WITH-WARN（run.sh:615 rc=2 ⇒ 门仍绿）",
            code, reason_of(stdout, stderr), True, stdout, stderr,
        )
        if code != 0:
            raise SystemExit("复现 1 未通过：记录件没写出来，后续用例无法进行")

        code, stdout, stderr = run([
            "verify", "--record", str(record_path), "--repo", str(repo),
            "--branch", branch, "--remote", str(repo), "--base-ref", base_ref, "--json",
        ])
        record(
            "复现1-verify-同一记录件（回读通过⇒绿）",
            "复核**强制回读**门摘要文件：磁盘文件 ↔ 内嵌 sha256 ↔ 内嵌原文 三方一致 ⇒ 绿",
            code, reason_of(stdout, stderr), True, stdout, stderr,
        )

        # ── 复现 2：洗内嵌摘要（同步重算 sha256）必须红 ──────────────────────
        payload = json.loads(record_path.read_text(encoding="utf-8"))
        payload["gate_summary_text"] = WASHED_SUMMARY
        payload["gate_summary_sha256"] = sha256_text(WASHED_SUMMARY)
        washed = root / "record-washed.json"
        washed.write_text(json.dumps(payload, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        code, stdout, stderr = run([
            "verify", "--record", str(washed), "--repo", str(repo),
            "--branch", branch, "--remote", str(repo), "--base-ref", base_ref, "--json",
        ])
        record(
            "复现2-洗内嵌摘要并重算sha（初版 exit=0/OK）",
            "记录件作者可控的'门摘要原文'必须被回读比对挡住（磁盘文件未动）",
            code, reason_of(stdout, stderr), False, stdout, stderr,
        )

        # ── 复现 3：改磁盘上的门摘要文件（记录件不动）必须红 ────────────────
        tampered = root / "SUMMARY-tampered.txt"
        tampered.write_text(WASHED_SUMMARY, encoding="utf-8")
        payload2 = json.loads(record_path.read_text(encoding="utf-8"))
        payload2["gate_summary_path"] = str(tampered)
        moved = root / "record-pointing-tampered.json"
        moved.write_text(json.dumps(payload2, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        code, stdout, stderr = run([
            "verify", "--record", str(moved), "--repo", str(repo),
            "--branch", branch, "--remote", str(repo), "--base-ref", base_ref, "--json",
        ])
        record(
            "复现3-磁盘门摘要文件被改（记录件不动）",
            "门摘要文件被换 ⇒ 回读三方不一致 ⇒ 红",
            code, reason_of(stdout, stderr), False, stdout, stderr,
        )

        # ── 复现 4：--source 绕口（不带 --allow-local-source）必须红 ────────
        code, stdout, stderr = run([
            "verify", "--record", str(record_path), "--repo", str(repo),
            "--branch", branch, "--remote", str(repo), "--base-ref", base_ref,
            "--source", cand, "--json",
        ])
        record(
            "复现4-verify--source本地提交（初版 exit=0 绕开远端绑定）",
            "候选头取自本地提交 ⇒ 必须显式 --allow-local-source 才生效",
            code, reason_of(stdout, stderr), False, stdout, stderr,
        )

        # ── 复现 5：显式 --allow-local-source ⇒ 绿 + 留 source_origin ──────
        code, stdout, stderr = run([
            "verify", "--record", str(record_path), "--repo", str(repo),
            "--branch", branch, "--remote", str(repo), "--base-ref", base_ref,
            "--source", cand, "--allow-local-source", "--json",
        ])
        record(
            "复现5-verify--source--allow-local-source（显式承认）",
            "显式承认本地来源 ⇒ 绿；输出须留 source_origin=本地提交…",
            code, reason_of(stdout, stderr), True, stdout, stderr,
        )
        origin = ""
        try:
            origin = str(json.loads(stdout).get("source_origin", ""))
        except json.JSONDecodeError:
            origin = ""
        if not origin.startswith("本地提交"):
            failures.append(f"复现 5：source_origin 未留本地来源（实得 {origin!r}）")
            lines.append(f"[复现] FAIL 复现5-source_origin 留痕：实得 {origin!r}（应为 '本地提交 …'）")
        else:
            lines.append(f"[复现] OK  复现5-source_origin 留痕：{origin}")

        # ── 复现 6：红词表（FAIL / ADVISORY-FAIL）必须拒写记录件 ────────────
        for word, summary_text in (
            ("FAIL", "step1  PASS  1ms  门摘要复现\nstep14  FAIL  1ms  机读一致性批\n"),
            ("ADVISORY-FAIL", "step1  PASS  1ms  门摘要复现\nstep9  ADVISORY-FAIL  1ms  FV advisory\n"),
        ):
            red_file = root / f"SUMMARY-red-{word}.txt"
            red_file.write_text(summary_text, encoding="utf-8")
            code, stdout, stderr = run([
                "emit", "--record", str(root / f"record-red-{word}.json"), "--repo", str(repo),
                "--branch", branch, "--remote", str(repo), "--base-ref", base_ref,
                "--source", cand, "--gate-id", "repro", "--gate-command", "bash ci/run.sh",
                "--gate-exit", "0", "--gate-summary", str(red_file), "--recorded-by", "r2-repro",
            ])
            record(
                f"复现6-emit-{word}（红词表必须拒写记录件）",
                f"词表项 {word} 不在白名单 ⇒ 门红 ⇒ emit 拒写绿记录件",
                code, reason_of(stdout, stderr), False, stdout, stderr,
            )

        raw = {
            "fixture": {
                "repo": str(repo),
                "base_commit": fx["base_commit"],
                "cand_commit": cand,
                "branch": branch,
                "base_ref": base_ref,
                "gate_summary": GREEN_SUMMARY,
            },
            "cases": cases,
            "failures": failures,
            "verdict": "R2-REPRO-GREEN" if not failures else "R2-REPRO-RED",
        }
        (out_dir / "r2-repro-readings.json").write_text(
            json.dumps(raw, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8"
        )
        text = (
            "=== v0.3-T7② 返修批：复核员实测场景复现读数 ===\n"
            f"命令：python3 -I poc/poc-t7-2-integrator/r2_repro.py --out {out_dir}\n"
            f"夹具：{repo}（门摘要真文件）\n"
            f"夹具门摘要原文：\n{GREEN_SUMMARY}"
            "──────────────\n"
            + "\n".join(lines)
            + f"\n──────────────\n结论：{raw['verdict']}；failures={failures}\n"
        )
        (out_dir / "r2-repro-readings.txt").write_text(text, encoding="utf-8")
        print(text)
        if failures:
            return 1
        return 0
    finally:
        shutil.rmtree(root, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
