#!/usr/bin/env python3
"""cue vet / cue export 在 10/100/500/1000 条规则三档（+10 基准）的耗时测量。

每个 (N, 命令) 组合跑 5 次，报 min/p50/mean；输出 TSV 与增长比。
"""
import statistics
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
CUE = HERE.parent.parent.parent / "bin" / "cue"  # af/poc/poc-cue/bin/cue
RULES_SCHEMA = HERE.parent / "rules" / "rules.cue"
RUNS = 5


def gen(n: int) -> Path:
    out = HERE / "perf" / f"rules-{n}.cue"
    subprocess.run([sys.executable, str(HERE / "gen_rules.py"), str(n), str(out)],
                   check=True, cwd=HERE)
    return out


def timed(cmd: list[str]) -> float:
    t0 = time.perf_counter()
    r = subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    dt = (time.perf_counter() - t0) * 1000
    if r.returncode != 0:
        raise RuntimeError(f"cmd failed ({r.returncode}): {' '.join(cmd)}\n{r.stderr.decode()[:500]}")
    return dt


def bench(n: int) -> dict:
    inst = gen(n)
    row = {"n": n, "file_kb": inst.stat().st_size / 1024}
    for label, argv in [
        ("vet", [str(CUE), "vet", str(RULES_SCHEMA), str(inst)]),
        ("export", [str(CUE), "export", str(RULES_SCHEMA), str(inst)]),
    ]:
        ts = sorted(timed(argv) for _ in range(RUNS))
        row[label] = {"min": ts[0], "p50": statistics.median(ts), "mean": sum(ts) / len(ts)}
    return row


def main() -> None:
    rows = [bench(n) for n in (10, 100, 500, 1000)]
    print(f"cue v0.17.1, {RUNS} runs/cell, schema={RULES_SCHEMA}")
    print("N\tfileKB\tvet_min\tvet_p50\tvet_mean\texp_min\texp_p50\texp_mean")
    for r in rows:
        print(f"{r['n']}\t{r['file_kb']:.1f}\t"
              f"{r['vet']['min']:.1f}\t{r['vet']['p50']:.1f}\t{r['vet']['mean']:.1f}\t"
              f"{r['export']['min']:.1f}\t{r['export']['p50']:.1f}\t{r['export']['mean']:.1f}")
    print("\n增长（相对前一档的倍数）: N → vet ×, export ×")
    for prev, cur in zip(rows, rows[1:]):
        vn = cur["vet"]["p50"] / prev["vet"]["p50"]
        en = cur["export"]["p50"] / prev["export"]["p50"]
        print(f"{prev['n']}→{cur['n']}: N×{cur['n']/prev['n']:.0f} → vet ×{vn:.1f}, export ×{en:.1f}")


if __name__ == "__main__":
    main()
