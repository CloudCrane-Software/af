#!/usr/bin/env python3
"""render —— research_report 的确定性 markdown 渲染器（报告包设计 §2.8/§3.2-R5）。

双正本纪律：report.json 是判定正本；report.md 是同一渲染器的纯函数产物
（人类审阅面）。rubric R5 用本渲染器重渲染比对——md 不是判定源。

CLI：render.py <report.json> → stdout（markdown 全文）。
"""
import json
import sys


def render_markdown(report: dict) -> str:
    paper = report.get("paper", {})
    lines = [f"# {paper.get('title', '')}", ""]
    for s in report.get("sections", []):
        lines.append(f"## {s.get('title', '')}")
        lines.append("")
        lines.append(s.get("body_md", ""))
        lines.append("")
    lines.append("## Citations")
    lines.append("")
    for c in report.get("citations", []):
        lines.append(f"[{c.get('n')}] {c.get('url', '')} — {c.get('snapshot_ref', '')}")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: render.py <report.json>", file=sys.stderr)
        return 2
    with open(sys.argv[1], encoding="utf-8") as f:
        report = json.load(f)
    sys.stdout.write(render_markdown(report))
    return 0


if __name__ == "__main__":
    sys.exit(main())
