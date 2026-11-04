#!/usr/bin/env python3
"""T7② integrator 复核器自证：正常记录件 + 削弱变体逐个跑（逐个红）。

跑法（工作树根下）：
    python3 -I poc/poc-t7-2-integrator/selftest.py --out poc/poc-t7-2-integrator/raw --tag -r2
    （--tag 决定落盘文件名 selftest-readings<tag>.json；v0.3-T7② 初版的读数
      raw/selftest-readings.json 与 raw/selftest-v1.py **原样保留、未改**）

自证面：
  ① 正常记录件 ⇒ 复核器 exit 0（绿）。
  ② 削弱变体逐个 ⇒ 复核器 exit 非 0（红）。变体清单：
       base_sha 改一位 / source_sha 改一位 / tested_tree 换成另一棵树的 /
       suite_digest 改一位 / verdict="fail" / verdict="PASS"（大小写差异）/
       某项为空串 / 某项缺字段 / 门摘要被抹（门不可跑）/ 门摘要被改（摘要不自证）/
       门退出码非零 / 记录件缺席
  ③ 夹具说明：门脚本是**真跑**的（fixture 的 ci/run.sh 真执行、真写 SUMMARY），
     git 读数全来自夹具仓真实提交，不是手编的常量。
  ④ ★ 真实词表用例（v0.3-T7② 返修批新增）：按 `ci/run.sh` 的真实 SUMMARY 词表逐词判绿/红
       绿 = PASS / ADVISORY-PASS / ADVISORY-WARN / PASS-WITH-WARN
       红 = FAIL / ADVISORY-FAIL
  ⑤ ★ 门摘要自证链三方比对用例（返修批新增）：磁盘文件 ↔ 内嵌 sha256 ↔ 内嵌原文
     任一不一致 ⇒ 必须红（GATE-SUMMARY-DIGEST / GATE-SUMMARY-ABSENT）。
  ⑥ ★ `--source` 绕口用例（返修批新增）：不带 `--allow-local-source` 必须红。

★ 纪律：任何一个变体仍绿 ⇒ 本脚本自身判红（exit 1）——不许为让变体绿而放宽判定。
"""

from __future__ import annotations

import argparse
import hashlib
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
    parser.add_argument("--tag", default="", help="读数文件名后缀（落盘 selftest-readings<tag>.json）")
    parser.add_argument("--keep", action="store_true", help="保留夹具目录（调试用）")
    args = parser.parse_args(argv)

    out_dir = Path(args.out).resolve() if args.out else Path(tempfile.mkdtemp(prefix="t7-2-raw-"))
    out_dir.mkdir(parents=True, exist_ok=True)
    out_file = out_dir / f"selftest-readings{args.tag}.json"

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

        def verify(
            tag: str,
            mutated: dict[str, Any] | None,
            *,
            expect_zero: bool,
            extra_args: list[str] | None = None,
            expect_reason: str = "",
            record_override: Path | None = None,
        ) -> dict[str, Any]:
            """跑一次复核器并记账。mutated=None ⇒ 用原记录件。"""
            target = record_override if record_override is not None else record_path
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
                    *(extra_args or []),
                ],
                cwd=REPO_ROOT,
            )
            ok = (code == 0) if expect_zero else (code != 0)
            reason = ""
            try:
                reason = json.loads(stdout).get("reason_code", "") or json.loads(stderr).get("reason_code", "")
            except (json.JSONDecodeError, ValueError):
                reason = (stderr.strip().splitlines() or [""])[0][:120]
            if expect_reason and reason != expect_reason:
                ok = False
            entry = {
                "case": tag,
                "expect": "exit 0（绿）" if expect_zero else "exit 非 0（红）",
                "expect_reason": expect_reason,
                "exit": code,
                "reason_code": reason,
                "pass": ok,
                "stdout": stdout[:2000],
                "stderr": stderr[:2000],
            }
            try:
                parsed = json.loads(stdout)
                if isinstance(parsed, dict):
                    entry["report"] = parsed
            except json.JSONDecodeError:
                pass
            readings.append(entry)
            flag = "OK " if ok else "FAIL"
            print(f"[自证] {flag} {tag:28s} exit={code} reason_code={reason}")
            if not ok:
                failures.append(f"变体 {tag} 未按预期（exit={code}，期望 {entry['expect']}"
                                f"{'/' + expect_reason if expect_reason else ''}）")
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

        # ── ④ ★ 真实词表用例（ci/run.sh 的 SUMMARY 词表，逐词判绿/红）─────────
        def sha256_text(text: str) -> str:
            return hashlib.sha256(text.encode("utf-8")).hexdigest()

        def make_summary(word: str) -> str:
            """按真实 run.sh 的行格式造一份摘要：`step<N>  <词>  <ms>ms  <名>`。"""
            if word == "PASS":
                return "step1  PASS  1ms  fixture fmt\nstep14  PASS  1ms  fixture machine batch\n"
            if word == "PASS-WITH-WARN":
                return "step1  PASS  1ms  fixture fmt\nstep14  PASS-WITH-WARN  2ms  fixture machine batch\n"
            if word in ("ADVISORY-PASS", "ADVISORY-WARN", "ADVISORY-FAIL"):
                return (
                    "step1  PASS  1ms  fixture fmt\n"
                    f"step9  {word}  1ms  fixture advisory\n"
                    "step14  PASS  1ms  fixture machine batch\n"
                )
            if word == "FAIL":
                return "step1  PASS  1ms  fixture fmt\nstep14  FAIL  1ms  fixture machine batch\n"
            raise AssertionError(f"未定义的词表项: {word}")

        def note(case: str, expect: str, code: int, reason: str, stdout: str, stderr: str, ok: bool) -> None:
            readings.append({
                "case": case,
                "expect": expect,
                "expect_reason": "",
                "exit": code,
                "reason_code": reason,
                "pass": ok,
                "stdout": stdout[:2000],
                "stderr": stderr[:2000],
            })
            print(f"[自证] {'OK ' if ok else 'FAIL'} {case:28s} exit={code} reason_code={reason}")
            if not ok:
                failures.append(f"用例 {case} 未按预期（exit={code}，期望 {expect}）")

        vocab_dir = root / "vocab"
        vocab_dir.mkdir(parents=True, exist_ok=True)

        def emit_with(summary_text: str, tag: str) -> tuple[int, str, str, Path]:
            summary_file = vocab_dir / f"SUMMARY-{tag}.txt"
            summary_file.write_text(summary_text, encoding="utf-8")
            record_file = vocab_dir / f"record-{tag}.json"
            code, stdout, stderr = run_checker(
                [
                    "emit",
                    "--record", str(record_file),
                    "--repo", str(repo),
                    "--branch", fx["branch"],
                    "--remote", str(repo),
                    "--base-ref", fx["base_ref"],
                    "--source", fx["cand_commit"],
                    "--gate-id", "fixture-fast",
                    "--gate-command", "bash ci/run.sh",
                    "--gate-exit", "0",
                    "--gate-summary", str(summary_file),
                    "--recorded-by", "t7-2-selftest",
                ],
                cwd=REPO_ROOT,
            )
            return code, stdout, stderr, record_file

        green_words = ("PASS", "ADVISORY-PASS", "ADVISORY-WARN", "PASS-WITH-WARN")
        red_words = ("FAIL", "ADVISORY-FAIL")
        for word in green_words:
            code, stdout, stderr, rec = emit_with(make_summary(word), f"vocab-{word}")
            reason = "" if code == 0 else (stderr.strip().splitlines() or [""])[0][:120]
            note(f"词表绿-emit-{word}", "exit 0（绿：真绿状态必须能写出记录件）",
                 code, reason, stdout, stderr, code == 0)
            if code == 0:
                verify(f"词表绿-verify-{word}", None, expect_zero=True, record_override=rec)
        for word in red_words:
            code, stdout, stderr, rec = emit_with(make_summary(word), f"vocab-{word}")
            reason = "" if code == 0 else (stderr.strip().splitlines() or [""])[0][:120]
            note(f"词表红-emit-{word}", "exit 非 0（红：门红 ⇒ 不许写绿记录件）",
                 code, reason, stdout, stderr, code != 0)
            # 手编记录件：内嵌红词摘要 + **同步重算 sha256** + 指向同一文件
            # ⇒ 回读三方一致（读的是同一份红摘要）⇒ 只能靠词表判红 ⇒ GATE-VERDICT
            red_text = make_summary(word)
            mutated = dict(record)
            mutated["gate_summary_text"] = red_text
            mutated["gate_summary_sha256"] = sha256_text(red_text)
            mutated["gate_summary_path"] = str(vocab_dir / f"SUMMARY-vocab-{word}.txt")
            verify(f"词表红-verify-{word}", mutated, expect_zero=False, expect_reason="GATE-VERDICT")

        # ── ⑤ ★ 门摘要自证链三方比对（磁盘文件 ↔ 内嵌 sha256 ↔ 内嵌原文）────────
        good_dir = root / "goodsum"
        good_dir.mkdir(parents=True, exist_ok=True)
        good_sum = good_dir / "SUMMARY.txt"
        good_sum.write_text(summary.read_text(encoding="utf-8"), encoding="utf-8")
        good_text = good_sum.read_text(encoding="utf-8")
        good_record = dict(record)
        good_record["gate_summary_path"] = str(good_sum)
        good_record["gate_summary_text"] = good_text
        good_record["gate_summary_sha256"] = sha256_text(good_text)

        verify("三方比对-基线一致（文件↔内嵌sha↔内嵌原文）", good_record, expect_zero=True)

        tampered = "step1  PASS  1ms  fixture fmt\nstep2  PASS  1ms  fixture clippy\n"
        tampered_file = good_dir / "SUMMARY-tampered.txt"
        tampered_file.write_text(tampered, encoding="utf-8")

        rec_b = dict(good_record)
        rec_b["gate_summary_path"] = str(tampered_file)
        verify("三方比对-磁盘文件被改（记录件不动）", rec_b, expect_zero=False, expect_reason="GATE-SUMMARY-DIGEST")

        rec_c = dict(good_record)
        rec_c["gate_summary_text"] = tampered
        rec_c["gate_summary_sha256"] = sha256_text(tampered)
        verify("三方比对-内嵌摘要被改且sha同步重算", rec_c, expect_zero=False, expect_reason="GATE-SUMMARY-DIGEST")

        rec_d = dict(good_record)
        rec_d["gate_summary_sha256"] = "0" * 64
        verify("三方比对-内嵌sha256被改", rec_d, expect_zero=False, expect_reason="GATE-SUMMARY-DIGEST")

        rec_e = dict(good_record)
        rec_e.pop("gate_summary_path")
        verify("三方比对-记录件缺摘要路径", rec_e, expect_zero=False, expect_reason="GATE-SUMMARY-ABSENT")

        rec_f = dict(good_record)
        rec_f["gate_summary_path"] = str(good_dir / "SUMMARY-absent.txt")
        verify("三方比对-摘要文件缺席", rec_f, expect_zero=False, expect_reason="GATE-SUMMARY-ABSENT")

        verify("三方比对-显式gate-summary指向异内容", good_record, expect_zero=False,
               expect_reason="GATE-SUMMARY-DIGEST",
               extra_args=["--gate-summary", str(tampered_file)])

        # ── ⑥ ★ `--source` 绕口（本地提交顶替「远端该分支头」）───────────────
        verify("source绕口-未加allow-local-source", good_record, expect_zero=False,
               expect_reason="LOCAL-SOURCE-NOT-ALLOWED",
               extra_args=["--source", fx["cand_commit"]])
        entry = verify("source绕口-加allow-local-source", good_record, expect_zero=True,
                       extra_args=["--source", fx["cand_commit"], "--allow-local-source"])
        report = entry.get("report") or {}
        origin = str(report.get("source_origin", ""))
        if not origin.startswith("本地提交") or report.get("source_is_remote_head") is not False:
            failures.append(
                f"verify --allow-local-source 未留 source_origin=本地提交（实得 {origin!r}）"
            )
        else:
            print(f"[自证] OK  source-origin-留痕             source_origin={origin}")

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
        (out_file).write_text(
            json.dumps(raw, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8"
        )
        print(f"\n[自证] 结论：{raw['verdict']}；读数落盘 {out_file}")
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
