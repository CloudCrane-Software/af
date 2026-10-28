#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""criteria_check.py — criteria.yaml schema 校验 + 终验清单机生成（G0-CI 机制批 · 子代理 D，SPEC-3 实施）

对应复盘 PP2 升级①（判据落 criteria.yaml，schema 必填六要素，CI 校验完备性，终验清单
从 criteria.yaml 机生成）+ PP5/C7（观察窗 machine-validated block 六/七要素）+ PP2 升级②
（每 gate 判据数软上限 ≤12，超出须运营者裁决）。

六要素（PP2 口径，每条判据必填，缺一不得进终验清单）：
  numerator(分子) / denominator(分母) / source_field(正本字段) / count_rule(计数规则) /
  no_data_clause(无数据=未达成) / producer+owner+schedule(生产者+属主+排期)
观察窗六要素（C7/PP5 口径，observation_windows[] 每条必填）：
  producer / owner / schedule / collect_surface / expiry / escalation
  （+第七要素 env_guard 可选——复盘 PP5 升级②口径，列则校验其子字段）

数值边界（SPEC ⑧.3）：本校验器只管 schema 完备性；分子分母的数值真伪归 step12
numbers.json 复算门（单一职责、一套正本）。与 numbers.json 的关系：source_field 指向
正本（文书路径/CI 产物/numbers.json 键名）。

退出码：0=schema 全合规（六要素/观察窗要素/软上限/豁免指针）；
        1=任一 schema 违例；或判据数超 12 且无 operator_waiver；
        2=仅 WARN（软上限超限但 waiver 在场降 WARN / yaml 库缺席文本降级）——由调用方
          定义是否改 OVERALL（run.sh step14 落 SUMMARY WARN 行不改 OVERALL）。
用法：python3 criteria_check.py [--criteria PATH] [--emit-checklist PATH] [--text-mode]
  --text-mode：yaml 库缺席时退化为文本校验（必填键逐条 grep），如实降级标注。
  --src-root：operator_waiver 的 ADR 指针解析根（缺省=criteria 文件所在仓库根）。
"""
import argparse
import re
import sys
from pathlib import Path

CRIT_REQUIRED = ["id", "claim", "numerator", "denominator", "source_field",
                 "count_rule", "no_data_clause", "producer", "owner", "schedule"]
OBS_REQUIRED = ["id", "claim", "producer", "owner", "schedule", "collect_surface",
                "expiry", "escalation", "no_data_clause", "source_field"]
ENV_GUARD_SUB = ["machine_truth", "red_lines"]
WAIVER_REQUIRED = ["waiver_adr", "reason", "approved_by", "approved_date"]
ADR_PATH_RE = re.compile(r"^docs/adr/\d{3}-[a-z0-9-]+\.md$")
SOFT_CAP = 12
GATE_ID_RE = re.compile(r"^G\d+$")
CRIT_ID_RE = re.compile(r"^G\d+(-[A-Z0-9]+)+$")


def load_yaml(path, text_mode):
    text = Path(path).read_text(encoding="utf-8")
    if text_mode:
        return ("text", text)
    try:
        import yaml  # type: ignore

        return ("yaml", yaml.safe_load(text))
    except ImportError:
        return ("text", text)


def check_yaml(mode, data, src_root):
    errs, warns, gates_out = [], [], []
    if not isinstance(data, dict) or data.get("schema") != "criteria-v1":
        errs.append("顶层 schema 字段必须为 criteria-v1")
        return errs, warns, gates_out
    gates = data.get("gates") or []
    if not gates:
        errs.append("gates[] 为空——无判据的 criteria.yaml 不受理（反空过）")
    for g in gates:
        gid = g.get("gate", "?")
        if not GATE_ID_RE.match(str(gid)):
            errs.append(f"gate id {gid!r} 须形如 G0-G5")
        crits = g.get("criteria") or []
        n = len(crits)
        if n == 0:
            errs.append(f"gate {gid}: criteria[] 为空（空清单=空过，拒收）")
        if n > SOFT_CAP:
            waiver = g.get("operator_waiver")
            if waiver:
                # SPEC ⑧.2 裁决点 1（实施批补）：豁免须附 ADR 指针（与 deliverables.yaml
                # 的 descope_adr 共用「ADR 指针必须可解析」纪律，接 C2 指针注册表）。
                if not isinstance(waiver, dict):
                    errs.append(f"gate {gid}: operator_waiver 须为映射（含 waiver_adr 等字段）")
                else:
                    for k in WAIVER_REQUIRED:
                        if k not in waiver or waiver[k] in (None, "", [], {}):
                            errs.append(f"gate {gid}: operator_waiver 缺字段 {k}（豁免须可追溯：ADR 指针+理由+批准人+日期）")
                    adr = waiver.get("waiver_adr")
                    if isinstance(adr, str) and adr:
                        if not ADR_PATH_RE.match(adr):
                            errs.append(f"gate {gid}: waiver_adr {adr!r} 须形如 docs/adr/NNN-slug.md")
                        elif not (src_root / adr).is_file():
                            errs.append(f"gate {gid}: waiver_adr 指针不可解析——{adr} 不在盘（ADR 指针必须可解析）")
                warns.append(f"gate {gid}: 判据 {n} 条 >{SOFT_CAP} 软上限，operator_waiver 在场降为 WARN")
            else:
                errs.append(f"gate {gid}: 判据 {n} 条超软上限 {SOFT_CAP}——须运营者裁决（operator_waiver）或拆分 gate")
        ids = set()
        for c in crits:
            cid = c.get("id", "?")
            if not CRIT_ID_RE.match(str(cid)):
                errs.append(f"判据 id {cid!r} 须形如 G4-1（gate 前缀+序段）")
            if cid in ids:
                errs.append(f"判据 id 重复：{cid}")
            ids.add(cid)
            for k in CRIT_REQUIRED:
                if k not in c or c[k] in (None, "", [], {}):
                    errs.append(f"{cid}: 缺六要素字段 {k}（PP2 口径，缺一不得进终验清单）")
            nd = str(c.get("no_data_clause", ""))
            # 反空过条款字面核（防「换个说法绕过」）：必须同时含「无数据」与「未达成」。
            if nd and ("无数据" not in nd or "未达成" not in nd):
                errs.append(f"{cid}: no_data_clause 必须含『无数据…未达成』反空过语义")
            for half in ("numerator", "denominator"):
                v = c.get(half)
                if isinstance(v, dict) and "definition" not in v and "value" not in v:
                    errs.append(f"{cid}: {half} 必须带 definition（或 value 常数）——禁止无定义计数")
        for w in g.get("observation_windows") or []:
            wid = w.get("id", "?")
            for k in OBS_REQUIRED:
                if k not in w or w[k] in (None, "", [], {}):
                    errs.append(f"观察窗 {wid}: 缺 C7/PP5 要素 {k}")
            eg = w.get("env_guard")
            if eg is not None:
                for k in ENV_GUARD_SUB:
                    if k not in eg:
                        errs.append(f"观察窗 {wid}: env_guard 列场（第七要素）但缺 {k}")
        gates_out.append((gid, crits, g.get("observation_windows") or []))
    return errs, warns, gates_out


def check_text(text):
    """yaml 库缺席时的文本降级校验：必填键在场性 + 软上限近似计数 + 反空过条款抽查。"""
    errs, warns = [], []
    if "schema: criteria-v1" not in text:
        errs.append("文本模式：未见『schema: criteria-v1』头")
    n_crit = len(re.findall(r"^\s+- id: G\d+-", text, re.M))
    n_gates = len(re.findall(r"^\s*- gate: G\d+", text, re.M))
    if n_gates == 0:
        errs.append("文本模式：未识别到 gate 块")
    if n_crit > SOFT_CAP * max(n_gates, 1):
        errs.append(f"文本模式：判据总数 {n_crit} 超软上限近似（{SOFT_CAP}×{max(n_gates,1)}）——需逐 gate 精算复核")
    for k in CRIT_REQUIRED:
        # 必填键可为 `- id: …`（列表项首字段）或 `  claim: …`（后续字段）两种行形
        if not re.search(rf"^\s*(?:- )?{k}:", text, re.M):
            errs.append(f"文本模式：全文未见必填键『{k}:』（可能某判据缺席该要素）")
    for k in OBS_REQUIRED:
        if "observation_windows" in text and not re.search(rf"^\s*(?:- )?{k}:", text, re.M):
            errs.append(f"文本模式：观察窗块未见要素『{k}:』")
    if "无数据=未达成" not in text:
        errs.append("文本模式：未见反空过条款『无数据=未达成』")
    warns.append("文本模式：yaml 库缺席，结构完整性为降级校验（逐键在场性，非逐条目精配）——如实标注")
    return errs, warns, None


def emit_checklist(gates_out, path, source):
    lines = [
        "# 终验清单（criteria.yaml 机生成——禁止手写，复盘 PP2：手写清单即 W1-C06 总账失真载体）",
        f"# 源：{source}；schema 校验过后方可生成；清单条目数==criteria 条目数",
        "",
    ]
    n_rows = 0
    for gid, crits, obs in gates_out:
        lines.append(f"## gate {gid}（{len(crits)} 判据）")
        lines.append("| # | 判据 | 分子 | 分母 | 正本 | 无数据=未达成 |")
        lines.append("|---|---|---|---|---|---|")
        for i, c in enumerate(crits, 1):
            num = c.get("numerator")
            num = num.get("definition", num.get("value")) if isinstance(num, dict) else num
            den = c.get("denominator")
            den = den.get("value", den.get("definition")) if isinstance(den, dict) else den
            lines.append(f"| {i} | {c.get('claim','')} | {num} | {den} | {c.get('source_field','')} | 是 |")
            n_rows += 1
        for w in obs:
            lines.append(f"- 观察窗 {w.get('id')}: {w.get('claim')}（采集面 {len(w.get('collect_surface', []))} 项；期满 {w.get('expiry')}）")
        lines.append("")
    Path(path).write_text("\n".join(lines), encoding="utf-8")
    return n_rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--criteria", default=None, help="criteria.yaml 路径（缺省=本脚本所在仓库 ci/criteria.yaml）")
    ap.add_argument("--emit-checklist", default=None, help="终验清单机生成输出路径")
    ap.add_argument("--text-mode", action="store_true", help="yaml 库缺席时文本降级校验")
    ap.add_argument("--src-root", default=None, help="operator_waiver ADR 指针解析根（缺省=criteria 文件的仓库根）")
    args = ap.parse_args()
    repo = Path(__file__).resolve().parent.parent
    criteria_path = Path(args.criteria) if args.criteria else repo / "ci" / "criteria.yaml"
    # ADR 指针解析根缺省=校验脚本所在仓库根（criteria 文件可能是 /tmp 探针副本，
    # 以其位置推导会把在册 ADR 误判为悬空——root 钉死到本仓）。
    src_root = Path(args.src_root).resolve() if args.src_root else repo

    mode, data = load_yaml(criteria_path, args.text_mode)
    if mode == "text":
        errs, warns, gates_out = check_text(data)
    else:
        errs, warns, gates_out = check_yaml(mode, data, src_root)
    for e in errs:
        print(f"[FAIL] {e}")
    for w in warns:
        print(f"[WARN] {w}")
    if not errs and not warns:
        print("[PASS] criteria.yaml schema 校验过（六要素/观察窗要素/软上限全合规）")
    elif not errs:
        print("[PASS-WITH-WARN] schema 过，含 WARN 项")
    if args.emit_checklist and gates_out:
        n_rows = emit_checklist(gates_out, args.emit_checklist, criteria_path)
        n_crit = sum(len(g[1]) for g in gates_out)
        print(f"== 终验清单机生成：{args.emit_checklist}（判据条目 {n_rows}==criteria {n_crit}）")
        if n_rows != n_crit:  # 防御：机生成条目数必须与登记数一致（SPEC ⑤.3）
            print(f"[FAIL] 清单条目数 {n_rows} != criteria 条目数 {n_crit}")
            if not errs:
                errs.append(f"清单条目数 {n_rows} != criteria 条目数 {n_crit}")
    if errs:
        return 1
    if warns:
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
