#!/usr/bin/env python3
"""rubric_unit —— 结构 rubric 验证器（研究报告包 v1，G4 报告包设计 §2.5）。

独立判分子进程（citation_unit 姊妹件，同三原则）：
- argv 契约：[task.json, report.json]（恰两位置参数）；
- 零 LLM、零网络；判定只写 stdout JSON：{passed, rubric[{id,ok}], failures[]}。

断言集（rubric 阈值全部参数化于 task.rubric_params——改 rubric=改任务包）：
  R1 sections id 闭集覆盖（task.rubric_params.section_ids，缺省五段）且各恰一次
  R2 每章 body_md 非空且 >= min_section_chars（缺省 200）
  R3 paper.{url,title,sha256} 在场且 paper.sha256 == task.paper_sha256
  R4 citations >= min_citations 且全部 quote 长度 ∈ [20, 600]（防整段/碰瓷）
  R5 report.md 与 report.json 渲染一致（同一渲染器 tools/render.py 重渲染比对）
"""
import json
import sys
import os

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "tools"))
import render  # noqa: E402  同包渲染器（R5 与单元产 md 同构的唯一正本）

# 红队 W2-C50：加载侧自证——校验**实际执行**的 render 模块本体，而非仅源文件。
# Python 对在场且头部 mtime/size 与源匹配的 __pycache__/render.*.pyc 会优先于
# 源文件加载（-B 只禁写不禁读）；外层 sha256 门锚的是 .py 源码，若加载走的是
# 影子字节码，源锚再真也管不住执行面。自证两点：
#   ① render.__file__ 必须以 render.py 结尾（直连 .pyc 的源外加载也拒）；
#   ② render.__cached__ 若**在盘**即拒——注意 __cached__ 在纯源码加载时也非
#      None（指向"本应使用"的 pyc 路径，文件可以不存在），判据必须是该路径
#      os.path.exists()：源码加载=路径不存在→过；pyc 影子加载=在盘→拒；
#      陈旧 pyc 在盘（本轮未用）也拒——与外层在场性门同一口径，宁可 fail-loud。
# 任一不满足即 stderr 说明 + exit 3——不给影子执行面产出判据的机会。
_render_file = getattr(render, "__file__", "") or ""
_render_cached = getattr(render, "__cached__", None)
if not _render_file.endswith("render.py") or (
    _render_cached is not None and os.path.exists(_render_cached)
):
    print(
        f"rubric_unit: render 模块加载面校验失败 "
        f"__file__={_render_file!r} __cached__={_render_cached!r} "
        f"cached_exists={os.path.exists(_render_cached) if _render_cached else False}"
        "（实际执行的必须为源 render.py 且加载路径上无字节码在盘）",
        file=sys.stderr,
    )
    sys.exit(3)


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: rubric_unit.py <task.json> <report.json>", file=sys.stderr)
        return 2
    task_path, report_path = sys.argv[1], sys.argv[2]
    with open(task_path, encoding="utf-8") as f:
        task = json.load(f)
    with open(report_path, encoding="utf-8") as f:
        report = json.load(f)
    params = task.get("rubric_params", {})
    section_ids = params.get(
        "section_ids", ["summary", "method", "findings", "limitations", "implications"]
    )
    min_section_chars = int(params.get("min_section_chars", 200))
    min_citations = int(params.get("min_citations", 3))

    failures = []

    sections = report.get("sections", [])
    ids = [s.get("id") for s in sections]
    ok_r1 = sorted(ids) == sorted(section_ids) and len(ids) == len(set(ids))
    if not ok_r1:
        failures.append(
            f"R1 section ids {sorted(ids)} != closed set {sorted(section_ids)}"
        )

    ok_r2 = bool(sections)
    for s in sections:
        body = s.get("body_md", "")
        if not body or len(body) < min_section_chars:
            ok_r2 = False
            failures.append(
                f"R2 section {s.get('id')} body_md {len(body)} < {min_section_chars} chars"
            )

    paper = report.get("paper", {})
    registered = task.get("paper_sha256")
    ok_r3 = (
        bool(paper.get("url"))
        and bool(paper.get("title"))
        and bool(paper.get("sha256"))
        and bool(registered)
        and paper.get("sha256") == registered
    )
    if not ok_r3:
        failures.append("R3 paper identity absent or sha256 != task registration")

    citations = report.get("citations", [])
    ok_r4 = len(citations) >= min_citations
    if not ok_r4:
        failures.append(f"R4 citations {len(citations)} < {min_citations}")
    for c in citations:
        qlen = len(c.get("quote", ""))
        if qlen < 20 or qlen > 600:
            ok_r4 = False
            failures.append(f"R4 citation n={c.get('n')} quote length {qlen} outside [20,600]")

    ok_r5 = False
    md_path = os.path.join(os.path.dirname(os.path.abspath(report_path)), "report.md")
    if os.path.isfile(md_path):
        with open(md_path, encoding="utf-8") as f:
            on_disk = f.read()
        ok_r5 = on_disk == render.render_markdown(report)
        if not ok_r5:
            failures.append("R5 report.md differs from renderer output")
    else:
        failures.append("R5 report.md missing next to report.json")

    rubric = [
        {"id": "R1", "ok": ok_r1},
        {"id": "R2", "ok": ok_r2},
        {"id": "R3", "ok": ok_r3},
        {"id": "R4", "ok": ok_r4},
        {"id": "R5", "ok": ok_r5},
    ]
    passed = all(r["ok"] for r in rubric)
    print(json.dumps({"passed": passed, "rubric": rubric, "failures": failures}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
