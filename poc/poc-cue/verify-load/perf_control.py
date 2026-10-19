#!/usr/bin/env python3
"""对照实验：同规模规则但去掉 combo kind（从而没有 or() 引用完整性析取）。

若仍超线性 → 8 路 kind 析取/统一化是根因；若回到线性 → or() 是根因，
完整性检查应移到装载器（E2001），CUE 侧保持快速。3 runs/cell。
"""
import statistics
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
CUE = HERE.parent.parent.parent / "bin" / "cue"
RULES_SCHEMA = HERE.parent / "rules" / "rules.cue"
RUNS = 3
KINDS_NO_COMBO = ["threshold", "ratio", "timeWindow", "dwell", "count", "fingerprint", "escalation"]


def rule(i: int) -> str:
    kind = KINDS_NO_COMBO[i % len(KINDS_NO_COMBO)]
    rid = f"rule-{i}"
    if kind == "threshold":
        params = f'{{metric: "m{i % 20}_sec", op: ">", value: {100 + (i % 50)}}}'
    elif kind == "ratio":
        warn = 0.01 + (i % 5) * 0.005
        crit = round(warn * 2, 4)
        params = f'{{numerator: "ng_count", denominator: "total_count", warn: {warn:.3f}, critical: {crit}}}'
    elif kind == "timeWindow":
        params = '{start: "22:00", end: "06:00"}'
    elif kind == "dwell":
        params = f'{{metric: "m{i % 20}_c", op: ">", value: {60 + (i % 30)}, samples: 3}}'
    elif kind == "count":
        params = '{metric: "unit_fail", op: ">=", atLeast: 3, within: "10m"}'
    elif kind == "fingerprint":
        params = '{fields: ["station", "error_code"], ttl: "30m"}'
    else:
        params = ('{chain: [{afterMs: 300000, notify: ["班组长"], action: "notify"},'
                  ' {afterMs: 900000, notify: ["值班经理"], action: "page"}]}')
    return f'\t{{id: "{rid}", kind: "{kind}", desc: "gen {kind} #{i}", params: {params}}},\n'


def gen(n: int) -> Path:
    out = HERE / "perf" / f"rules-nocombo-{n}.cue"
    body = "".join(rule(i) for i in range(n))
    text = (
        f"// 对照：{n} 条规则，无 combo（无 or() 析取）\n"
        "package rules\n\n"
        "_ruleDefs: [...#Rule]\n"
        f"_ruleDefs: [\n{body}]\n\n"
        "rules: _ruleDefs\n"  # 不做 per-rule comprehension，纯透传
    )
    out.write_text(text)
    return out


def timed(cmd: list[str]) -> float:
    t0 = time.perf_counter()
    r = subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    dt = (time.perf_counter() - t0) * 1000
    if r.returncode != 0:
        raise RuntimeError(f"cmd failed ({r.returncode}): {' '.join(cmd)}\n{r.stderr.decode()[:500]}")
    return dt


def main() -> None:
    print(f"对照组（无 combo / 无 or()），{RUNS} runs/cell")
    print("N\tvet_min\tvet_p50\texp_min\texp_p50")
    rows = []
    for n in (10, 100, 500, 1000):
        inst = gen(n)
        vt = sorted(timed([str(CUE), "vet", str(RULES_SCHEMA), str(inst)]) for _ in range(RUNS))
        et = sorted(timed([str(CUE), "export", str(RULES_SCHEMA), str(inst)]) for _ in range(RUNS))
        rows.append((n, vt, et))
        print(f"{n}\t{vt[0]:.1f}\t{statistics.median(vt):.1f}\t{et[0]:.1f}\t{statistics.median(et):.1f}", flush=True)
    print("\n增长: N → vet ×, export ×")
    for (n0, v0, e0), (n1, v1, e1) in zip(rows, rows[1:]):
        print(f"{n0}→{n1}: N×{n1/n0:.0f} → vet ×{statistics.median(v1)/statistics.median(v0):.1f}, "
              f"export ×{statistics.median(e1)/statistics.median(e0):.1f}")


if __name__ == "__main__":
    main()
