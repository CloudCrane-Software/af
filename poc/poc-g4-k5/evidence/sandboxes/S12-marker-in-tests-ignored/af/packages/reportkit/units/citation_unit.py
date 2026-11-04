#!/usr/bin/env python3
"""citation_unit —— 引用核验器（研究报告包 v1，G4 报告包设计 §2.4）。

独立判分子进程（judge.rs 三原则的报告面平行件）：
- argv 契约钉死：[task.json, report.json, sources_dir]（恰三位置参数）；
- 零 LLM、零网络（快照核验全本地）——「判分可离线」（ADR-006:13）；
- 判定只写 stdout JSON：{passed, checks{...}, failures[]}；
- 字节冻结由执行面指纹门守（ReportJudgeLocator；裁决6：锚=落盘实测，
  spec 与本脚本同 commit 原子生成）。

断言集（全机器可判）：
  marker_align     正文 [n] 标记与 citations[].n 双向对齐（每条引用恰被
                   一个标记引用、每个标记有同 n 条目）
  quote_hit        每条引用 quote 是其快照文本的子串（规范化空白后比对）
  sha_match        快照文件重算 SHA-256 == citation.snapshot_sha256
  snapshot_present 全部 snapshot_ref 文件在场（缺件=fail，不虚列）
  citations_total  引用总数 >= task.rubric_params.min_citations（缺省 3）
"""
import hashlib
import json
import os
import re
import sys


def norm(s: str) -> str:
    return " ".join(s.split())


def main() -> int:
    if len(sys.argv) != 4:
        print("usage: citation_unit.py <task.json> <report.json> <sources_dir>", file=sys.stderr)
        return 2
    task_path, report_path, sources_dir = sys.argv[1], sys.argv[2], sys.argv[3]
    with open(task_path, encoding="utf-8") as f:
        task = json.load(f)
    with open(report_path, encoding="utf-8") as f:
        report = json.load(f)
    params = task.get("rubric_params", {})
    min_citations = int(params.get("min_citations", 3))

    failures = []

    citations = report.get("citations", [])
    sections = report.get("sections", [])
    body_all = "\n".join(s.get("body_md", "") for s in sections)

    # citations_total
    citations_total = len(citations)
    if citations_total < min_citations:
        failures.append(
            f"citations_total {citations_total} < min_citations {min_citations}"
        )

    # marker_align（双向）
    markers = [int(m) for m in re.findall(r"\[(\d+)\]", body_all)]
    ns = [c.get("n") for c in citations]
    marker_align = True
    for m in markers:
        if m not in ns:
            marker_align = False
            failures.append(f"marker [{m}] has no citation entry")
    for n in ns:
        if ns.count(n) != 1 or markers.count(n) != 1:
            marker_align = False
            failures.append(
                f"citation n={n} must be referenced by exactly one marker"
            )
    if not ns and not markers and citations_total == 0 and min_citations == 0:
        marker_align = True

    # snapshot_present / sha_match / quote_hit
    snapshot_present = True
    sha_match = True
    quote_hit = True
    for c in citations:
        ref = c.get("snapshot_ref", "")
        path = sources_dir.rstrip("/") + "/" + ref.split("/")[-1] if ref else None
        if not ref or not path or not os.path.isfile(path):
            snapshot_present = False
            failures.append(f"snapshot missing: {ref}")
            continue
        with open(path, "rb") as f:
            data = f.read()
        actual_sha = hashlib.sha256(data).hexdigest()
        if actual_sha != c.get("snapshot_sha256"):
            sha_match = False
            failures.append(f"sha256 mismatch for {ref}")
            continue
        try:
            snap_text = data.decode("utf-8")
        except UnicodeDecodeError:
            snap_text = data.decode("utf-8", "replace")
        if norm(c.get("quote", "")) not in norm(snap_text):
            quote_hit = False
            failures.append(f"quote not found in snapshot {ref}")

    passed = (
        citations_total >= min_citations
        and marker_align
        and quote_hit
        and sha_match
        and snapshot_present
    )
    print(
        json.dumps(
            {
                "passed": passed,
                "checks": {
                    "citations_total": citations_total,
                    "marker_align": marker_align,
                    "quote_hit": quote_hit,
                    "sha_match": sha_match,
                    "snapshot_present": snapshot_present,
                },
                "failures": failures,
            },
            ensure_ascii=False,
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
