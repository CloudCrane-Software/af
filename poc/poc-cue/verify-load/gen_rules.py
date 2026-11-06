#!/usr/bin/env python3
"""千条规则配方生成器 —— 在 poc-cue/rules/rules.cue 的规则结构上扩展到 N 条。

用法: python3 gen_rules.py N [OUT]
生成 OUT（默认 perf/rules-N.cue）：package rules 的实例文件，
规则 kind 按 8 类轮转（threshold/ratio/timeWindow/dwell/count/fingerprint/escalation/combo），
combo.allOf 引用已存在的规则 id（引用完整性用 rules.cue 里的 or() 收紧）。
"""
import sys
from pathlib import Path

KINDS = ["threshold", "ratio", "timeWindow", "dwell", "count", "fingerprint", "escalation", "combo"]


def rule(i: int) -> str:
    kind = KINDS[i % len(KINDS)]
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
    elif kind == "escalation":
        params = ('{chain: [{afterMs: 300000, notify: ["班组长"], action: "notify"},'
                  ' {afterMs: 900000, notify: ["值班经理"], action: "page"}]}')
    else:  # combo：引用第 0 条（已存在，or() 析取可静态查）
        params = '{allOf: ["rule-0"]}'
    return f'\t{{id: "{rid}", kind: "{kind}", desc: "gen {kind} #{i}", params: {params}}},\n'


def main() -> None:
    n = int(sys.argv[1])
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(f"perf/rules-{n}.cue")
    body = "".join(rule(i) for i in range(n))
    text = (
        f"// 自动生成：{n} 条规则（gen_rules.py），schema 见 ../../rules/rules.cue\n"
        "package rules\n\n"
        "_ruleDefs: [...#Rule]\n"
        f"_ruleDefs: [\n{body}]\n\n"
        "// 与 rules-10.cue 相同的导出视图 + 引用完整性\n"
        "_ruleIds: or([for r in _ruleDefs {r.id}])\n"
        "rules: [for r in _ruleDefs {\n"
        "\tr\n"
        "\tif r.kind == \"combo\" {params: allOf: [..._ruleIds]}\n"
        "}]\n"
    )
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text)
    print(f"wrote {out}: {n} rules")


if __name__ == "__main__":
    main()
