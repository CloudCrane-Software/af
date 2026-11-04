#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""probe-p01-p09.py — G1 返修批：削弱面探针（P01–P09 ＋ 补充 P10–P13）。

用途：把复核员的九个探针形态（注释掉命令行 / `|| true` / 语义 step `if:false` /
job 级 `if:false` / `types:[opened]` / `paths-ignore`）逐条落到现役 workflow 上，
**实跑** `ci/gh_workflow_check.py`，逐条给「红 / 仍绿」读数与命中规则。

用法（工作树根下）：
    python3 -I poc/poc-g1-github-pr-surface/probe-p01-p09.py

判读口径：
  * 「红」= 检查器 exit≠0（判据面被削弱 ⇒ 机器抓住）；
  * 「仍绿」= 检查器 exit 0（如实写仍绿，不含糊）；
  * `hit=` 给出该变体命中的规则号（多规则按序列出）。
"""
from __future__ import annotations

import json
import pathlib
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parent.parent.parent
CHECKER = REPO / "ci" / "gh_workflow_check.py"
WF = REPO / ".github" / "workflows" / "f1.yml"

FMT_LINE = "          cargo fmt --check --all"
CLIPPY_LINE = "          cargo clippy --workspace --all-targets -- -D warnings"
TEST_LINE = "          cargo test -p af-dsl -p af-compiler"
CLIPPY_STEP_HEAD = "      - name: clippy -D warnings（硬门；快速门用 -D 全量拦，比 F2 的基线门更严）\n"
SELFCHECK_STEP = (
    "      - name: 判据面自检（GitHub 端面防弱化；本仓零外部 action 依赖）\n"
    "        # 机器面：谁把本文件的判据面改弱（去掉 clippy -D warnings / 放宽 permissions /\n"
    "        # 引入 action / 去掉超时或并发 / 改成 pull_request_target …），本 step 即红。\n"
    "        # 恒跑（always）：前序步骤红不遮蔽本判定。\n"
    "        if: ${{ always() }}\n"
    "        run: |\n"
    "          python3 -I ci/gh_workflow_check.py\n"
)


def sub(old, new, count=1):
    def fn(text):
        if old not in text:
            raise AssertionError("变异锚点不存在: %r" % old[:70])
        return text.replace(old, new, count)
    return fn


PROBES = [
    ("P01", "注释掉 fmt 命令行（语义件被注释掉 = 判据弱化）",
     sub(FMT_LINE, "          # " + FMT_LINE.strip()), "R08"),
    ("P02", "注释掉 clippy 命令行",
     sub(CLIPPY_LINE, "          # " + CLIPPY_LINE.strip()), "R09"),
    ("P03", "注释掉快速测试命令行",
     sub(TEST_LINE, "          # " + TEST_LINE.strip()), "R10"),
    ("P04", "fmt 命令行吞红：`|| true`",
     sub(FMT_LINE, FMT_LINE + " || true"), "R08"),
    ("P05", "clippy 命令行吞红：`|| true`",
     sub(CLIPPY_LINE, CLIPPY_LINE + " || true"), "R09"),
    ("P06", "语义 step 带 `if: false` 抑制条件（把 always() 换成 false）",
     sub(CLIPPY_STEP_HEAD + "        # fmt 红不遮蔽 clippy/test 判定——一次 run 收全三路诊断（对齐 ci/run.sh\n"
         "        # 「步骤失败不中断、跑完全部步骤收集完整诊断」哲学）；job 结论仍任一红即红。\n"
         "        if: ${{ always() }}\n",
         CLIPPY_STEP_HEAD + "        # fmt 红不遮蔽 clippy/test 判定——一次 run 收全三路诊断（对齐 ci/run.sh\n"
         "        # 「步骤失败不中断、跑完全部步骤收集完整诊断」哲学）；job 结论仍任一红即红。\n"
         "        if: false\n"), "R18"),
    ("P07", "job 级 `if: false`（整门恒不跑）",
     sub("  f1-fast-gate:\n", "  f1-fast-gate:\n    if: false\n"), "R19"),
    ("P08", "on.pull_request 收窄：`types: [opened]`",
     sub("on:\n  pull_request:\n    branches:\n      - main\n",
         "on:\n  pull_request:\n    types: [opened]\n    branches:\n      - main\n"), "R20"),
    ("P09", "on.pull_request 收窄：`paths-ignore`",
     sub("on:\n  pull_request:\n    branches:\n      - main\n",
         "on:\n  pull_request:\n    paths-ignore:\n      - 'docs/**'\n    branches:\n      - main\n"), "R20"),
    # ── 以下为返修批自行追加的补充探针（超出复核员九项，用于覆盖同类 fail-open 路径）──
    ("P10", "【补充】fmt 命令行吞红：`; true`",
     sub(FMT_LINE, FMT_LINE + " ; true"), "R08"),
    ("P11", "【补充】clippy step 里 `set +e`（关掉 errexit ⇒ R21 命中；命令本身未被改故 R09 仍在位）",
     sub(CLIPPY_LINE, "          set +e\n" + CLIPPY_LINE), "R21"),
    ("P12", "【补充】整块删掉「判据面自检」step（自检面形同虚设）",
     sub(SELFCHECK_STEP, ""), "R22"),
    ("P13", "【补充】自检命令吞红：`|| true`",
     sub("          python3 -I ci/gh_workflow_check.py\n",
         "          python3 -I ci/gh_workflow_check.py || true\n"), "R21"),
    ("P14", "【补充】重复键夹带：在语义 step 里塞第二个 `if:`（后者覆盖前者）",
     sub(CLIPPY_STEP_HEAD + "        # fmt 红不遮蔽",
         CLIPPY_STEP_HEAD + "        if: false\n        # fmt 红不遮蔽"), "R02"),
]


def main() -> int:
    base = WF.read_text(encoding="utf-8")
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="g1-probe-"))
    try:
        print("== 正例（现役面）==")
        proc = subprocess.run([sys.executable, "-I", str(CHECKER)],
                              capture_output=True, text=True)
        print("POSITIVE exit=%d（期望 0）" % proc.returncode)
        bad = 0
        print("== 探针（削弱面 ⇒ 必须红；仍绿就如实写仍绿）==")
        for pid, desc, mutate, expect in PROBES:
            try:
                mutated = mutate(base)
            except AssertionError as exc:
                bad += 1
                print("%s MUTATION-ANCHOR-MISSING expected_rule=%s | %s | %s"
                      % (pid, expect, desc, exc))
                continue
            if mutated == base:
                bad += 1
                print("%s MUTATION-NOOP expected_rule=%s | %s" % (pid, expect, desc))
                continue
            vdir = tmp / pid
            (vdir / ".github" / "workflows").mkdir(parents=True, exist_ok=True)
            (vdir / ".github" / "workflows" / WF.name).write_text(mutated, encoding="utf-8")
            proc = subprocess.run(
                [sys.executable, "-I", str(CHECKER), "--root", str(vdir), "--json"],
                capture_output=True, text=True)
            failed_rules = []
            try:
                failed_rules = [f["rule"] for f in json.loads(proc.stdout).get("failures", [])]
            except Exception:
                pass
            color = "红" if proc.returncode != 0 else "仍绿"
            hit = "yes" if expect in failed_rules else "no"
            if proc.returncode == 0:
                bad += 1
            print("%s %s exit=%d expected_rule=%s hit=%s failed_rules=%s | %s"
                  % (pid, color, proc.returncode, expect, hit,
                     ",".join(failed_rules) if failed_rules else "-", desc))
        print("PROBE SUMMARY: probes=%d still_green=%d" % (len(PROBES), bad))
        return 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
