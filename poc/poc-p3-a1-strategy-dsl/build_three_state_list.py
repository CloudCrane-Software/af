#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""build_three_state_list.py — P3-A1 三态清单生成器（机器可读 + 人读表）。

口径：strategies/index.yaml 为全集正本（61 条）；每条的三态以条目文件实测
dsl_state 为准（ci/strategies_check.py R2 双向对账保真）。判定依据三源：
  · compilable     —— 既有编译门实测行（raw/strategy_library_test.nocapture.txt
                      的 COMPILE-OK 行 = af_dsl::load_str+validate 零违例的
                      逐条留痕）；
  · needs_syntax   —— 条目 needs_syntax_note（缺什么语法，首 60 字摘录）；
  · not_applicable —— 条目 na_reason（为何不适用，首 60 字摘录）。
61 vs 59：tier∈{A,B} 且 rank≤59 = 台账「59 簇」（libA 11 + libB 48）；
tier C 两条（循环产线终裁回写，v1.2 契约收编）为超出台账口径者。
"""
import json
import re
import sys
from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parents[2]
STRATS = REPO / "strategies"
POC = REPO / "poc" / "poc-p3-a1-strategy-dsl"
RAW = POC / "raw" / "strategy_library_test.nocapture.txt"


def main() -> int:
    nocap = RAW.read_text(encoding="utf-8")
    compiled_ok = set(re.findall(r"COMPILE-OK (\S+)", nocap))
    idx = yaml.safe_load((STRATS / "index.yaml").read_text(encoding="utf-8"))
    entries = []
    for e in idx["entries"]:
        doc = yaml.safe_load((STRATS / e["file"]).read_text(encoding="utf-8"))
        state = doc["dsl_state"]
        assert state == e["dsl_state"], f"{e['id']} index/文件 dsl_state 漂移"
        if state == "compilable":
            basis = (
                f"编译门实测 COMPILE-OK（load_str+validate 零违例）"
                if e["id"] in compiled_ok
                else "!! 编译门实测行缺席（需复查）"
            )
            basis += f"；dsl_snippet {len(doc['dsl_snippet'].splitlines())} 行"
        elif state == "needs_syntax":
            note = re.sub(r"\s+", " ", doc["needs_syntax_note"])[:80]
            basis = f"needs_syntax_note：{note}…"
        else:
            reason = re.sub(r"\s+", " ", doc["na_reason"])[:80]
            basis = f"na_reason：{reason}…"
        entries.append(
            {
                "id": e["id"],
                "file": e["file"],
                "tier": e["tier"],
                "topic": e["topic"],
                "rank": e["rank"],
                "dsl_state": state,
                "in_59_clusters": e["tier"] in ("A", "B") and e["rank"] <= 59,
                "basis": basis,
                "experiment_status": doc.get("experiment", {}).get("status"),
            }
        )
    dist = {s: sum(1 for x in entries if x["dsl_state"] == s)
            for s in ("compilable", "needs_syntax", "not_applicable")}
    out = {
        "generated_from": "strategies/index.yaml + 条目文件实测 + raw/strategy_library_test.nocapture.txt",
        "baseline": "origin/product/main 56ff52a",
        "total": len(entries),
        "distribution": dist,
        "clusters_59": {
            "definition": "tier A 11（libA-20261029）+ tier B 48（libB-20261029）= 59 簇（台账口径）",
            "ids_over_59": [x["id"] for x in entries if not x["in_59_clusters"]],
            "note": "超出两条均为 tier C 实验承载档：循环产线按实验终裁自动回写建条"
                    "（st-C122←EXP-1 终裁 inconclusive；st-C103←EXP-2 同型），"
                    "strategies_check.py v1.2 契约修订（tier 闭集扩 A|B|C|D）收编。",
        },
        "entries": entries,
    }
    (POC / "three-state-list.json").write_text(
        json.dumps(out, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")

    # 人读表
    lines = [
        "# P3-A1 三态清单（61 条人读表）",
        "",
        f"全集口径：`strategies/index.yaml`（{len(entries)} 条）。分布：compilable "
        f"{dist['compilable']} / needs_syntax {dist['needs_syntax']} / "
        f"not_applicable {dist['not_applicable']}。",
        "",
        "| # | rank | id | tier | topic | 三态 | 判定依据 | 59簇 |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for i, x in enumerate(entries, 1):
        basis = x["basis"].replace("|", "/")
        mark = "Y" if x["in_59_clusters"] else "N(C档回写)"
        lines.append(
            f"| {i} | {x['rank']} | `{x['id']}` | {x['tier']} | {x['topic']} | "
            f"{x['dsl_state']} | {basis} | {mark} |")
    lines += ["", f"超出 59 簇口径的两条：{out['clusters_59']['ids_over_59']}——"
              + out["clusters_59"]["note"]]
    (POC / "three-state-list.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"OK：{len(entries)} 条；分布 {dist}；"
          f"59 簇外 = {out['clusters_59']['ids_over_59']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
