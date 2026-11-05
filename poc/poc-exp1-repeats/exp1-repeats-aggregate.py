#!/usr/bin/env python3
"""exp1-repeats-aggregate — EXP-1 repeat 轮（A0/A1/A2 第二跑 72 单）逐单提取 + 数据门 +
repeat 轮内统计 + 主轮×repeat 轮 flakiness 估计（单轮不收敛任务识别）。

预注册绑定（只读，不修改）：
  计划/研究组/B线核心批-20261029/experiments-v1.json EXP-1
    · randomization.repeats = "A0/A1/A2 三臂各 24 任务跑第 2 次（估 flakiness）；剂量臂不重复"
    · sample_size.repeats_runs = 72（runs_total=216 = main 96 + dose 48 + repeat 72）
    · decision_rules / metric_operationalization（binarization=all_hit、ci 构造、门槛常量）
  计划/研究组/B线核心批-20261029/exp1-allocation.json → runs[] 中 round=="repeat" 的 72 行
  计划/研究组/B线核心批-20261029/exp1-materials.json → papers[].paper_sha256 预登记指纹

口径同源锚（本工具的统计量与工作区 tools/b-exp1-aggregate.py 逐字同源，后者是
EXP-1 主轮聚合件；本工具不复制其判定逻辑，只复用同一批公式并用 --regress-main
与主轮已发布汇总做逐项对拍）：
  · quote_hit_rate / all_hit 二值化：按 af/packages/reportkit/units/citation_unit.py
    （本仓正本）语义从 report.json + sources/ 逐字复算
  · McNemar 精确单侧 p：p_ge(b,s) = Σ_{k=b..s} C(s,k)/2^s
  · TOST CP90：π=b/s 的 Clopper-Pearson 90% 区间经 (2π−1)·s/n_p 映射到 pp
  · 配对差聚类 bootstrap 95% CI：种子 20261029（= 预注册 randomization.seed）、10000 次
  · 单臂 all_hit 比例 Wilson 95% 区间（仅报告标注）
  · 成本折算基座：2¢/17,935 tok（F-第五单-交付-20261025.md:152 实测，EXP-1.budget.basis 同引）

判定纪律（重要）：
  · 本工具**不签发 EXP-1 主判定**。EXP-1 主判定（inconclusive）由主轮 144 单经
    tools/b-verdict-unique.py 唯一判定程序产出（exp1-aggregate-summary.json），本工具
    不改写它。repeat 轮按预注册文字用途（"估 flakiness"）只产出：①repeat 轮内描述性
    统计与数据门读数；②主轮×repeat 轮一致性（flakiness）与不收敛任务名单；
    ③探索性 pooled 读数（明确标注：并入主判定＝改判据 ⇒ 须开新 exp_id「EXP-1b」，
    本批不并入、不作判定）。
  · 数据门与真实性锚逐条 fail-loud：执行集 ⊆ 预注册 repeat 分配表、快照指纹命中
    exp1-materials.json、citations 三锚一致、quote failures 复算一致、reward 与双判官一致。

用法：
  python3 exp1-repeats-aggregate.py \
      --mirror ~/exp1-raw-r2/mirror/orders \
      --main-mirror ~/exp1-raw/mirror/orders \
      --exp1-dir <预注册正本目录> \
      --out-dir <交付件目录>
  python3 exp1-repeats-aggregate.py --regress-main <主轮 exp1-aggregate-summary.json> \
      --main-mirror ~/exp1-raw/mirror/orders --exp1-dir <预注册正本目录>   # 口径对拍
"""
from __future__ import annotations

import argparse
import glob
import json
import os
import random
import re
import sys
from collections import Counter
from math import comb, sqrt

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_EXP1_DIR = "/home/pan-ding/workspace/agent黑灯工厂/计划/研究组/B线核心批-20261029"

REPEAT_ARMS = ["A0", "A1", "A2"]
ALL_ARMS = ["A0", "A1", "A2", "A3", "A1d25", "A1d50"]
ARM_LABEL = {
    "A0": "空规则（control）",
    "A1": "负向约束 ×10（treatment）",
    "A2": "正向指令 ×10（treatment）",
    "A3": "安慰剂 ×10（placebo）",
    "A1d25": "负向 ×25（dose）",
    "A1d50": "负向 ×50（dose）",
}
BOOT_SEED = 20261029          # = experiments-v1.json EXP-1.randomization.seed
BOOT_N = 10000                # = metric_operationalization.ci.method
F5C_CENTS_PER_TOK = 2.0 / 17935.0   # F-第五单-交付-20261025.md:152 实测基座
Z95 = 1.959963984540054
GATE_PAIRED_MIN = 20          # = verdict 绑定常量 paired_min
GATE_UNPAIRED_MAX = 4        # = verdict 绑定常量 unpaired_max


# ---------------------------------------------------------------- 统计公式（同源锚见模块 docstring）
def p_ge(b, s):
    """McNemar 精确单侧 p（处理侧更优方向）：P(X>=b), X~Bin(s, 0.5)。"""
    if s <= 0 or b <= 0:
        return 1.0
    if b > s:
        return 0.0
    return sum(comb(s, k) for k in range(b, s + 1)) / float(1 << s)


def cp_interval(x, n, alpha=0.10):
    """Clopper-Pearson 双侧区间（二分到机器精度；与 tools/b-verdict-unique.py 同构造）。"""
    lo = alpha / 2.0
    if x == 0:
        L = 0.0
    else:
        a, b_ = 0.0, 1.0
        for _ in range(100):
            m = (a + b_) / 2
            tail = sum(comb(n, k) * m ** k * (1 - m) ** (n - k) for k in range(x, n + 1))
            if tail > lo:
                b_ = m
            else:
                a = m
        L = (a + b_) / 2
    if x == n:
        U = 1.0
    else:
        a, b_ = 0.0, 1.0
        for _ in range(100):
            m = (a + b_) / 2
            tail = sum(comb(n, k) * m ** k * (1 - m) ** (n - k) for k in range(0, x + 1))
            if tail > lo:
                a = m
            else:
                b_ = m
        U = (a + b_) / 2
    return L, U


def tost_diff_ci_pp(b, c, n_p, margin_pp=5.0):
    """TOST 钉死构造：π=b/s 的 CP 90% 区间经 (2π−1)·s/n_p 映射到 pp；严格含于等效域为等效。"""
    s = b + c
    if s == 0:
        return 0.0, 0.0, True
    L, U = cp_interval(b, s, 0.10)
    dL = s * (2.0 * L - 1.0) * 100.0 / n_p
    dU = s * (2.0 * U - 1.0) * 100.0 / n_p
    return dL, dU, (dL > -margin_pp + 1e-9 and dU < margin_pp - 1e-9)


def wilson_ci(k, n, z=Z95):
    if n == 0:
        return (None, None)
    p = k / n
    denom = 1 + z * z / n
    center = (p + z * z / (2 * n)) / denom
    half = z * sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / denom
    return (max(0.0, center - half), min(1.0, center + half))


def paired_bootstrap_ci(diffs, seed=BOOT_SEED, n_boot=BOOT_N):
    """配对差（all_hit 二值化，pp）按任务聚类 bootstrap 百分位 95% CI；固定种子确定性。"""
    n = len(diffs)
    if n == 0:
        return (None, None)
    rng = random.Random(seed)
    vals = sorted(sum(rng.choices(diffs, k=n)) / n * 100.0 for _ in range(n_boot))
    return (vals[int(0.025 * n_boot)], vals[int(0.975 * n_boot)])


# ---------------------------------------------------------------- 提取
def parse_repeat_order_dir(d):
    """E1R-<arm>-<paper_id> → (arm, paper_id)。"""
    m = re.match(r"^E1R-(A0|A1|A2)-(\d{4}\.\d{4,5})$", os.path.basename(d))
    if not m:
        return None, None
    return m.group(1), m.group(2)


def parse_main_order_dir(d):
    """E1-<arm>-<paper_id> → (arm, paper_id)。"""
    m = re.match(r"^E1-(A1d50|A1d25|A0|A1|A2|A3)-(\d{4}\.\d{4,5})$", os.path.basename(d))
    if not m:
        return None, None
    return m.group(1), m.group(2)


def norm(s: str) -> str:
    """citation_unit.py 的规范化空白比对（逐字复制其 norm 实现）。"""
    return " ".join(s.split())


def recompute_citation_unit(task_min_citations, report, sources_dir):
    """逐字复算 citation_unit.py 主判官的引用断言。

    判定逻辑与本仓正本 af/packages/reportkit/units/citation_unit.py 一致
    （marker_align / quote_hit / sha_match / snapshot_present / citations_total）。
    """
    import hashlib

    citations = report.get("citations", [])
    body_all = "\n".join(s.get("body_md", "") for s in report.get("sections", []))
    markers = [int(m) for m in re.findall(r"\[(\d+)\]", body_all)]
    ns = [c.get("n") for c in citations]

    marker_align = True
    marker_failures = []
    for m in markers:
        if m not in ns:
            marker_align = False
            marker_failures.append("marker [%d] has no citation entry" % m)
    for n in ns:
        if ns.count(n) != 1 or markers.count(n) != 1:
            marker_align = False
            marker_failures.append("citation n=%s must be referenced by exactly one marker" % n)

    snapshot_present = True
    sha_match = True
    quote_hit = True
    detail = []
    for c in citations:
        ref = c.get("snapshot_ref", "")
        path = sources_dir.rstrip("/") + "/" + ref.split("/")[-1] if ref else None
        rec = {"n": c.get("n"), "snapshot_ref": ref, "snapshot_sha256": c.get("snapshot_sha256")}
        if not ref or not path or not os.path.isfile(path):
            snapshot_present = False
            rec["status"] = "skipped_snapshot_missing"
            detail.append(rec)
            continue
        with open(path, "rb") as f:
            raw = f.read()
        actual = hashlib.sha256(raw).hexdigest()
        rec["snapshot_sha256_actual"] = actual
        if c.get("snapshot_sha256") and actual != c.get("snapshot_sha256"):
            sha_match = False
            rec["status"] = "sha_mismatch"
            detail.append(rec)
            continue
        text = raw.decode("utf-8", errors="replace")
        quote = c.get("quote", "")
        if norm(quote) and norm(quote) in norm(text):
            rec["status"] = "hit"
            rec["hit_len"] = len(quote)
        else:
            quote_hit = False
            rec["status"] = "miss"
        detail.append(rec)

    citations_total = len(citations)
    failures = list(marker_failures)
    if not marker_align:
        failures.append("marker_align: %d" % len(marker_failures))
    if not snapshot_present:
        failures.append("snapshot_present: missing snapshots")
    if not sha_match:
        failures.append("sha_match: snapshot sha256 mismatch")
    if not quote_hit:
        misses = sum(1 for d in detail if d["status"] == "miss")
        failures.append("quote_hit: %d quote(s) not found" % misses)
    if citations_total < task_min_citations:
        failures.append("citations_total %d < min_citations %d" % (citations_total, task_min_citations))
    checks = {
        "marker_align": marker_align,
        "snapshot_present": snapshot_present,
        "sha_match": sha_match,
        "quote_hit": quote_hit,
        "citations_total": citations_total,
    }
    return checks, detail, failures


def read_verify_log(path):
    """verify.log: '# citation' / json / '# rubric' / json。返回 (citation, rubric, 行号锚)。"""
    citation = rubric = None
    anchor = {}
    section = None
    with open(path, encoding="utf-8") as f:
        for lineno, line in enumerate(f, 1):
            line = line.rstrip("\n")
            if line.startswith("# citation"):
                section = "citation"
                continue
            if line.startswith("# rubric"):
                section = "rubric"
                continue
            if not line.strip():
                continue
            obj = json.loads(line)
            if section == "citation":
                citation = obj
                anchor["citation_line"] = lineno
            elif section == "rubric":
                rubric = obj
                anchor["rubric_line"] = lineno
    return citation, rubric, anchor


def verifier_note_citations(note):
    m = re.search(r"citations=(\d+)", note or "")
    return int(m.group(1)) if m else None


def extract_run(order_dir, materials_shas, parser):
    arm, paper = parser(order_dir)
    order_id = os.path.basename(order_dir)
    rec = {
        "order_id": order_id, "arm": arm, "paper": paper, "round": "repeat",
        "mirror_path": order_dir,
        "status": None, "reward": None, "gate": None, "turns": None,
        "tokens": None, "tokens_total": None, "cache_read": None,
        "citations_total": None, "quote_hits": None, "quote_hit_rate": None,
        "all_hit": None, "rubric_passed": None, "marker_align": None,
        "sha_match": None, "snapshot_present": None,
        "report_present": None, "verifier_note": None,
        "wall_clock_s": None, "error_code": None,
        "anchor": {}, "crosscheck": {}, "citation_detail": [],
    }
    rp = os.path.join(order_dir, "results.jsonl")
    lines = [l for l in open(rp, encoding="utf-8").read().splitlines() if l.strip()]
    res = json.loads(lines[-1])
    rec["results_lines"] = len(lines)
    rec["status"] = res.get("status")
    rec["reward"] = res.get("reward")
    rec["gate"] = res.get("gate")
    rec["turns"] = res.get("turns")
    tk = res.get("tokens") or {}
    rec["tokens"] = {"input": tk.get("input"), "output": tk.get("output"),
                     "cache_read": tk.get("cache_read")}
    if tk.get("input") is not None and tk.get("output") is not None:
        rec["tokens_total"] = tk["input"] + tk["output"]
    rec["cache_read"] = tk.get("cache_read")
    rec["verifier_note"] = res.get("verifier_note")
    rec["error_code"] = res.get("error_code")
    rec["run_dir"] = res.get("run_dir")
    rec["model_calls"] = res.get("model_calls")
    rec["agent_wall_ms"] = res.get("agent_wall_ms")
    rec["anchor"]["results_jsonl"] = rp + " (last line)"

    mp = os.path.join(order_dir, "mes-events.jsonl")
    if os.path.isfile(mp):
        evs = [json.loads(l) for l in open(mp, encoding="utf-8") if l.strip()]
        rel = [e["occurred_at"] for e in evs if e.get("kind") == "release"]
        cls = [e["occurred_at"] for e in evs if e.get("kind") == "close"]
        done = [e["occurred_at"] for e in evs if e.get("kind") == "complete"]
        t0 = min(rel) if rel else min(e["occurred_at"] for e in evs)
        t1 = max(cls) if cls else (max(done) if done else None)
        if t1 is None:
            rec["wall_clock_s"] = None
            rec["wall_clock_note"] = "mes-events 无 complete/close 事件，submit→close 墙钟不可得"
        else:
            rec["wall_clock_s"] = round((t1 - t0) / 1000.0, 3)
        rec["anchor"]["mes_events"] = mp

        run_path = None
        run_dir = res.get("run_dir")
        if run_dir:
            cands = glob.glob(os.path.join(order_dir, "runs", "*", "*"))
            run_path = cands[0] if len(cands) == 1 else None
        if run_path and os.path.isdir(run_path):
            vlog = os.path.join(run_path, "verify.log")
            rj = os.path.join(run_path, "report.json")
            src = os.path.join(run_path, "sources")
            rec["anchor"]["run_dir"] = run_path
            if os.path.isfile(vlog):
                citation, rubric, vanchor = read_verify_log(vlog)
                rec["anchor"]["verify_log"] = "%s (citation json line %s; rubric json line %s)" % (
                    vlog, vanchor.get("citation_line"), vanchor.get("rubric_line"))
                if citation:
                    ch = citation["checks"]
                    rec["citations_total"] = ch["citations_total"]
                    rec["marker_align"] = ch["marker_align"]
                    rec["sha_match"] = ch["sha_match"]
                    rec["snapshot_present"] = ch["snapshot_present"]
                    rec["citation_passed"] = citation["passed"]
                    rec["citation_failures"] = citation["failures"]
                    rec["quote_failures"] = sum(
                        1 for x in citation["failures"] if x.startswith("quote not found"))
                if rubric:
                    rec["rubric_passed"] = rubric["passed"]
                    rec["rubric_failures"] = rubric.get("failures", [])
            if os.path.isfile(rj):
                rec["report_present"] = True
                report = json.load(open(rj, encoding="utf-8"))
                rec["anchor"]["report_json"] = rj
                rec["report_citations_len"] = len(report.get("citations", []))
                checks, detail, _mf = recompute_citation_unit(
                    3, report, src if os.path.isdir(src) else run_path)
                rec["citation_detail"] = detail
                rec["quote_hits"] = sum(1 for d in detail if d["status"] == "hit")
                rec["quote_misses"] = sum(1 for d in detail if d["status"] == "miss")
                rec["quote_skipped"] = sum(1 for d in detail if d["status"].startswith("skipped"))
                rec["recomputed_checks"] = checks
                if rec["citations_total"]:
                    rec["quote_hit_rate"] = rec["quote_hits"] / rec["citations_total"]
                    rec["all_hit"] = (rec["quote_hit_rate"] == 1.0)
                rec["anchor"]["sources"] = sorted(
                    os.path.join(src, os.path.basename(d["snapshot_ref"]))
                    for d in detail if d.get("snapshot_ref"))
                reg = materials_shas.get(paper)
                shas = {d.get("snapshot_sha256_actual") for d in detail
                        if d.get("snapshot_sha256_actual")}
                rec["crosscheck"]["snapshot_sha_vs_materials"] = (
                    "match" if reg and shas == {reg} else ("mismatch" if reg else "no_registration"))
                rep_sha = (report.get("paper") or {}).get("sha256")
                rec["crosscheck"]["report_paper_sha_vs_materials"] = (
                    "match" if reg and rep_sha == reg else ("mismatch" if reg else "no_registration"))
            else:
                rec["report_present"] = False

    cc = rec["crosscheck"]
    note_n = verifier_note_citations(rec["verifier_note"])
    cc["citations_total_verifier_note"] = note_n
    cc["citations_total_verify_log"] = rec["citations_total"]
    cc["citations_total_report_json"] = rec.get("report_citations_len")
    cc["citations_total_consistent"] = (
        note_n is not None and note_n == rec["citations_total"] == rec.get("report_citations_len"))
    if rec.get("report_present"):
        cc["quote_failures_verify_log_vs_recomputed"] = (
            rec.get("quote_failures") == rec.get("quote_misses"))
        cc["quote_hit_check_vs_recomputed"] = (
            (rec["citations_total"] - rec.get("quote_failures", -1)) == rec.get("quote_hits"))
    cc["reward_vs_judges"] = (
        rec["reward"] == (rec.get("citation_passed") and rec.get("rubric_passed"))
        if rec.get("report_present") else None)
    return rec


# ---------------------------------------------------------------- 配对统计
def pair_runs(runs, treat, ctrl):
    """按 task_id（paper）配对；返回配对任务列表与剔除留痕。"""
    t_tasks = {r["paper"]: r for r in runs if r["arm"] == treat and r.get("report_present")}
    c_tasks = {r["paper"]: r for r in runs if r["arm"] == ctrl and r.get("report_present")}
    all_t = {r["paper"] for r in runs if r["arm"] == treat}
    all_c = {r["paper"] for r in runs if r["arm"] == ctrl}
    paired = sorted(set(t_tasks) & set(c_tasks))
    unpaired = {
        "missing_in_%s" % treat: sorted(all_t - set(t_tasks)),
        "missing_in_%s" % ctrl: sorted(all_c - set(c_tasks)),
        "no_report_either": sorted((all_t - set(t_tasks)) & (all_c - set(c_tasks))),
    }
    return [(p, t_tasks[p], c_tasks[p]) for p in paired], unpaired


def paired_stats(pairs, treat, ctrl, binar="all_hit", label_suffix=""):
    """配对二值结局统计：b/c/s/n_p、配对差、McNemar 精确单侧 p、TOST CP 区间、bootstrap CI。"""

    def val(r):
        if binar == "all_hit":
            return 1 if r["all_hit"] else 0
        if binar == "half_hit":
            return 1 if (r["quote_hit_rate"] or 0) >= 0.5 else 0
        if binar == "any_hit":
            return 1 if (r["quote_hit_rate"] or 0) > 0 else 0
        raise ValueError(binar)

    rows = []
    for paper, tr, cr in pairs:
        rows.append({"paper": paper, "treat": val(tr), "ctrl": val(cr),
                     "treat_rate": tr["quote_hit_rate"], "ctrl_rate": cr["quote_hit_rate"],
                     "treat_order": tr["order_id"], "ctrl_order": cr["order_id"]})
    n_p = len(rows)
    b = sum(1 for r in rows if r["treat"] == 1 and r["ctrl"] == 0)
    c = sum(1 for r in rows if r["treat"] == 0 and r["ctrl"] == 1)
    t = sum(1 for r in rows if r["treat"] == 1 and r["ctrl"] == 1)
    u = sum(1 for r in rows if r["treat"] == 0 and r["ctrl"] == 0)
    s = b + c
    diff_pp = 0.0 if n_p == 0 else (b - c) * 100.0 / n_p
    out = {
        "comparison": "%s−%s%s" % (treat, ctrl, label_suffix), "binarization": binar,
        "n_paired": n_p, "b_treat_better": b, "c_ctrl_better": c,
        "both_hit": t, "both_miss": u, "s_discordant": s,
        "diff_pp": diff_pp,
        "mcnemar_p_one_sided_treat": p_ge(b, s),
        "mcnemar_p_one_sided_ctrl": p_ge(c, s),
        "tost_cp90_pp": list(tost_diff_ci_pp(b, c, n_p)[:2]) if n_p else [None, None],
        "tost_equivalent": (tost_diff_ci_pp(b, c, n_p)[2] if n_p else None),
        "bootstrap95_pp": list(paired_bootstrap_ci([r["treat"] - r["ctrl"] for r in rows])),
        "rows": rows,
    }
    if n_p:
        rd = [r["treat_rate"] - r["ctrl_rate"] for r in rows]
        out["ratio_paired_diff_mean_pp"] = sum(rd) / n_p * 100.0
        out["ratio_paired_diff_bootstrap95_pp"] = list(paired_bootstrap_ci(rd))
    return out


def arm_summary(runs, arm):
    rs = [r for r in runs if r["arm"] == arm]
    with_rep = [r for r in rs if r.get("report_present")]
    n = len(with_rep)
    rates = [r["quote_hit_rate"] for r in with_rep]
    hits = sum(1 for r in with_rep if r["all_hit"])
    lo, hi = wilson_ci(hits, n)
    dist = Counter(round(r, 4) for r in rates)
    cits = [r["citations_total"] for r in with_rep]
    return {
        "arm": arm, "label": ARM_LABEL[arm], "n_orders": len(rs),
        "n_with_report": n, "n_all_hit": hits,
        "all_hit_rate": (hits / n if n else None),
        "all_hit_wilson95": [lo, hi],
        "quote_hit_rate_mean": (sum(rates) / n if n else None),
        "quote_hit_rate_min": (min(rates) if rates else None),
        "quote_hit_rate_max": (max(rates) if rates else None),
        "citations_total_sum": sum(cits), "citations_total_mean": (sum(cits) / n if n else None),
        "quote_hits_sum": sum(r["quote_hits"] for r in with_rep),
        "rate_distribution": {str(k): v for k, v in sorted(dist.items())},
        "tokens_total_sum": sum(r["tokens_total"] for r in rs if r["tokens_total"] is not None),
        "turns": sorted({r["turns"] for r in rs if r["turns"] is not None}),
        "missing_report_tasks": sorted(r["paper"] for r in rs if not r.get("report_present")),
        "wall_clock_s_sum": round(sum(r["wall_clock_s"] for r in rs if r["wall_clock_s"]), 1),
    }


def gate_rows_for(runs, comparisons):
    """数据门（verdict_precedence 首项 GATE:inconclusive；decision_rules 非退化析取项）。"""
    rows = []
    for treat, ctrl, suffix in comparisons:
        pairs, unpaired = pair_runs(runs, treat, ctrl)
        n_p = len(pairs)
        miss_t = len(unpaired["missing_in_%s" % treat])
        miss_c = len(unpaired["missing_in_%s" % ctrl])
        gate_paired = n_p < GATE_PAIRED_MIN
        gate_missing = (miss_t > GATE_UNPAIRED_MAX) or (miss_c > GATE_UNPAIRED_MAX)
        rows.append({
            "comparison": "%s−%s%s" % (treat, ctrl, suffix),
            "n_paired": n_p, "missing_report_%s" % treat: miss_t,
            "missing_report_%s" % ctrl: miss_c,
            "gate_paired_lt20": gate_paired, "gate_missing_gt4": gate_missing,
            "gate_fires": (gate_paired or gate_missing), "unpaired_detail": unpaired,
        })
    return rows


# ---------------------------------------------------------------- flakiness（主轮 × repeat 轮）
def flakiness_analysis(main_runs, repeat_runs):
    """同 (arm, task) 配对的两轮一致性：all_hit / reward / gate / tokens 比率。

    口径来源：预注册 randomization.repeats 文字用途（"估 flakiness"）与
    exp1-结果判读-v1.md §3 所记偏差后果（"flakiness 无法估计，单轮不收敛任务无从识别"）。
    """
    main_idx = {(r["arm"], r["paper"]): r for r in main_runs if r.get("report_present")}
    rep_idx = {(r["arm"], r["paper"]): r for r in repeat_runs if r.get("report_present")}
    common = sorted(set(main_idx) & set(rep_idx))
    per_arm = {}
    detail = []
    for arm, paper in common:
        m, rp = main_idx[(arm, paper)], rep_idx[(arm, paper)]
        row = {
            "arm": arm, "paper": paper,
            "main_order": m["order_id"], "repeat_order": rp["order_id"],
            "main_all_hit": m["all_hit"], "repeat_all_hit": rp["all_hit"],
            "all_hit_flip": m["all_hit"] != rp["all_hit"],
            "main_reward": m["reward"], "repeat_reward": rp["reward"],
            "reward_flip": m["reward"] != rp["reward"],
            "main_gate": m["gate"], "repeat_gate": rp["gate"],
            "gate_flip": m["gate"] != rp["gate"],
            "main_quote_hit_rate": m["quote_hit_rate"], "repeat_quote_hit_rate": rp["quote_hit_rate"],
            "main_tokens_total": m["tokens_total"], "repeat_tokens_total": rp["tokens_total"],
            "token_ratio_repeat_over_main": (
                (rp["tokens_total"] / m["tokens_total"]) if (m["tokens_total"] and rp["tokens_total"]) else None),
            "main_citations_total": m["citations_total"], "repeat_citations_total": rp["citations_total"],
            "main_turns": m["turns"], "repeat_turns": rp["turns"],
        }
        detail.append(row)
        a = per_arm.setdefault(arm, {"arm": arm, "label": ARM_LABEL[arm], "n_paired": 0,
                                      "n_all_hit_flip": 0, "n_reward_flip": 0, "n_gate_flip": 0,
                                      "token_ratios": []})
        a["n_paired"] += 1
        a["n_all_hit_flip"] += 1 if row["all_hit_flip"] else 0
        a["n_reward_flip"] += 1 if row["reward_flip"] else 0
        a["n_gate_flip"] += 1 if row["gate_flip"] else 0
        if row["token_ratio_repeat_over_main"] is not None:
            a["token_ratios"].append(row["token_ratio_repeat_over_main"])
    for arm, a in per_arm.items():
        n = a["n_paired"]
        a["all_hit_flip_rate"] = (a["n_all_hit_flip"] / n) if n else None
        a["reward_flip_rate"] = (a["n_reward_flip"] / n) if n else None
        a["gate_flip_rate"] = (a["n_gate_flip"] / n) if n else None
        rs = a.pop("token_ratios")
        a["token_ratio_repeat_over_main_mean"] = (sum(rs) / len(rs)) if rs else None
        a["token_ratio_repeat_over_main_median"] = (sorted(rs)[len(rs) // 2]) if rs else None
        a["token_ratio_repeat_over_main_min"] = min(rs) if rs else None
        a["token_ratio_repeat_over_main_max"] = max(rs) if rs else None
    non_converging = [d for d in detail if d["all_hit_flip"] or d["reward_flip"]]
    return {
        "n_pairs": len(common),
        "main_orders_with_report": len(main_idx),
        "repeat_orders_with_report": len(rep_idx),
        "per_arm": per_arm,
        "non_converging_tasks": non_converging,
        "n_non_converging": len(non_converging),
        "detail": detail,
    }


def pooled_exploratory(main_runs, repeat_runs):
    """探索性 pooled 读数：把两轮同任务配对当作 2×23 观测。

    明确标注：这不是判定。并入主判定＝改判据 ⇒ 须开新 exp_id「EXP-1b」，
    本批不并入、不据此签发任何 verdict。
    """
    out = []
    for treat, ctrl in [("A1", "A0"), ("A2", "A0"), ("A1", "A2")]:
        mp, _mu = pair_runs(main_runs, treat, ctrl)
        rp, _ru = pair_runs(repeat_runs, treat, ctrl)
        mmap = {p: (t, c) for p, t, c in mp}
        rmap = {p: (t, c) for p, t, c in rp}
        common = sorted(set(mmap) & set(rmap))
        rows = []
        for p in common:
            mt, mc = mmap[p]
            rt, rc = rmap[p]
            rows.append({
                "paper": p, "round": "main",
                "treat": 1 if mt["all_hit"] else 0, "ctrl": 1 if mc["all_hit"] else 0,
                "treat_order": mt["order_id"], "ctrl_order": mc["order_id"]})
            rows.append({
                "paper": p, "round": "repeat",
                "treat": 1 if rt["all_hit"] else 0, "ctrl": 1 if rc["all_hit"] else 0,
                "treat_order": rt["order_id"], "ctrl_order": rc["order_id"]})
        n_obs = len(rows)
        b = sum(1 for r in rows if r["treat"] == 1 and r["ctrl"] == 0)
        c = sum(1 for r in rows if r["treat"] == 0 and r["ctrl"] == 1)
        s = b + c
        diff_pp = 0.0 if n_obs == 0 else (b - c) * 100.0 / n_obs
        # 任务聚类 bootstrap（同任务两轮一起重抽，识别轮间相关性）
        by_paper = {}
        for r in rows:
            by_paper.setdefault(r["paper"], []).append(r["treat"] - r["ctrl"])
        papers = sorted(by_paper)
        rng = random.Random(BOOT_SEED)
        vals = []
        for _ in range(BOOT_N):
            samp = []
            for _i in range(len(papers)):
                samp.extend(by_paper[papers[rng.randrange(len(papers))]])
            vals.append(sum(samp) / len(samp) * 100.0)
        vals.sort()
        out.append({
            "comparison": "%s−%s（pooled 探索性）" % (treat, ctrl),
            "n_observations": n_obs, "n_tasks": len(papers),
            "b_treat_better": b, "c_ctrl_better": c, "s_discordant": s,
            "diff_pp": diff_pp,
            "mcnemar_p_one_sided_treat_naive": p_ge(b, s),
            "cluster_bootstrap95_pp": [vals[int(0.025 * BOOT_N)], vals[int(0.975 * BOOT_N)]],
            "caveat": "探索性读数；并入主判定须开 EXP-1b（新 exp_id），本批不作判定",
        })
    return out


# ---------------------------------------------------------------- 主流程
def collect_orders(mirror, parser, materials_shas, problems, label):
    order_dirs = sorted(
        d for d in glob.glob(os.path.join(os.path.abspath(mirror), "*"))
        if os.path.isdir(d) and parser(d) != (None, None))
    runs = []
    for d in order_dirs:
        try:
            runs.append(extract_run(d, materials_shas, parser))
        except Exception as e:
            problems.append("%s: %s 订单工件不可读/不完整（%s: %s）→ 拒绝聚合"
                            % (label, os.path.basename(d), type(e).__name__, e))
    return runs, order_dirs


def main(argv):
    ap = argparse.ArgumentParser(description="EXP-1 repeat 轮聚合 + flakiness 估计")
    ap.add_argument("--mirror", help="repeat 轮镜像根（E1R-* 订单目录）")
    ap.add_argument("--main-mirror", help="主轮镜像根（E1-* 订单目录，flakiness 对拍用）")
    ap.add_argument("--exp1-dir", default=DEFAULT_EXP1_DIR, help="预注册正本目录（只读）")
    ap.add_argument("--out-dir", default=os.path.join(HERE, "data"), help="交付件输出目录")
    ap.add_argument("--regress-main", help="主轮已发布 exp1-aggregate-summary.json（口径对拍）")
    ap.add_argument("--selfcheck-only", action="store_true", help="只跑口径对拍，不出交付件")
    args = ap.parse_args(argv)

    exp1_dir = os.path.abspath(args.exp1_dir)
    spec_path = os.path.join(exp1_dir, "experiments-v1.json")
    alloc_path = os.path.join(exp1_dir, "exp1-allocation.json")
    materials_path = os.path.join(exp1_dir, "exp1-materials.json")
    for p in (spec_path, alloc_path, materials_path):
        if not os.path.isfile(p):
            print("FATAL: 预注册正本件缺席：%s（只读依赖，不可代造）" % p, file=sys.stderr)
            return 2
    spec = json.load(open(spec_path, encoding="utf-8"))
    exp1 = [e for e in spec["experiments"] if e["exp_id"] == "EXP-1"][0]
    alloc = json.load(open(alloc_path, encoding="utf-8"))
    materials = json.load(open(materials_path, encoding="utf-8"))
    materials_shas = {p["paper_id"]: p["paper_sha256"] for p in materials["papers"]}
    alloc_repeat = {(r["task_id"], r["arm"]) for r in alloc["runs"] if r["round"] == "repeat"}

    problems = []
    main_runs = []
    if args.main_mirror:
        main_runs, _ = collect_orders(args.main_mirror, parse_main_order_dir,
                                      materials_shas, problems, "主轮")

    # ---------------------------------------------------------------- 口径对拍（主轮）
    regress = None
    if args.regress_main and main_runs:
        regress = regress_main(args.regress_main, main_runs)
        print("== 口径对拍（主轮已发布汇总 vs 本工具同源实现）==")
        for k, v in regress["checks"].items():
            print("  %-46s %s" % (k, "PASS" if v["pass"] else "FAIL"))
        print("  → 对拍结论：%s" % ("PASS" if regress["all_pass"] else "FAIL"))
        if not regress["all_pass"] and not args.selfcheck_only:
            return 2
    if args.selfcheck_only:
        return 0 if (regress and regress["all_pass"]) else 2

    if not args.mirror:
        print("FATAL: 缺 --mirror（repeat 轮镜像根）", file=sys.stderr)
        return 2

    print("== 输入 ==")
    print("  spec: %s（EXP-1；tasks=%d；binarization=%s）"
          % (spec_path, exp1["sample_size"]["tasks"],
             exp1["metric_operationalization"]["binarization"]["selected"]))
    print("  allocation: %s（预注册 repeat 行 %d）" % (alloc_path, len(alloc_repeat)))
    print("  mirror(repeat): %s" % os.path.abspath(args.mirror))
    print("  mirror(main): %s" % (os.path.abspath(args.main_mirror) if args.main_mirror else "（未给，flakiness 不可算）"))

    # ---------------------------------------------------------------- ① 逐单提取
    repeat_runs, order_dirs = collect_orders(args.mirror, parse_repeat_order_dir,
                                             materials_shas, problems, "repeat")
    print("\n== ① 逐单提取（%d 个 E1R-* 订单目录）==" % len(order_dirs))
    for r in repeat_runs:
        if r["arm"] not in REPEAT_ARMS:
            problems.append("%s: arm=%r 不在 repeat 闭集" % (r["order_id"], r["arm"]))
        if (r["paper"], r["arm"]) not in alloc_repeat:
            problems.append("%s: (paper,arm)=(%s,%s) 不在预注册 repeat 分配表内"
                            "（未注册订单，来源不可溯 → 拒绝聚合）" % (r["order_id"], r["paper"], r["arm"]))
        if r.get("report_present"):
            for k in ("snapshot_sha_vs_materials", "report_paper_sha_vs_materials"):
                if r["crosscheck"].get(k) != "match":
                    problems.append("%s: %s=%s ≠ match（预登记指纹对不上 → 拒绝聚合）"
                                    % (r["order_id"], k, r["crosscheck"].get(k)))
            if not r["crosscheck"]["citations_total_consistent"]:
                problems.append("%s: citations_total 三锚不一致（note=%s verify.log=%s report.json=%s）"
                                % (r["order_id"], r["crosscheck"]["citations_total_verifier_note"],
                                   r["crosscheck"]["citations_total_verify_log"],
                                   r["crosscheck"]["citations_total_report_json"]))
            if not r["crosscheck"]["quote_failures_verify_log_vs_recomputed"]:
                problems.append("%s: verify.log quote failures=%s ≠ recomputed misses=%s"
                                % (r["order_id"], r.get("quote_failures"), r.get("quote_misses")))
            if not r["crosscheck"]["quote_hit_check_vs_recomputed"]:
                problems.append("%s: (citations_total−quote failures)≠recomputed hits" % r["order_id"])
        if r["crosscheck"].get("reward_vs_judges") is False:
            problems.append("%s: reward=%s 与双判官 passed 不一致" % (r["order_id"], r["reward"]))
    for p in problems:
        print("  FAIL-LOUD: " + p)
    if problems:
        print("提取交叉对账失败 %d 条 → 停止（判据先于数据，数据不自证）" % len(problems))
        return 2
    n_rep = sum(1 for r in repeat_runs if r.get("report_present"))
    print("  三锚对账 citations_total：%d/%d 一致"
          % (sum(1 for r in repeat_runs if r["crosscheck"].get("citations_total_consistent")), n_rep))
    print("  quote failures 复算一致：%d/%d"
          % (sum(1 for r in repeat_runs if r["crosscheck"].get("quote_failures_verify_log_vs_recomputed")), n_rep))
    print("  reward ↔ 双判官一致：%d/%d"
          % (sum(1 for r in repeat_runs if r["crosscheck"].get("reward_vs_judges")), n_rep))

    # ---------------------------------------------------------------- ② 覆盖对账（预注册 repeat 行）
    executed = {(r["paper"], r["arm"]) for r in repeat_runs}
    missing = sorted(alloc_repeat - executed)
    extra = sorted(executed - alloc_repeat)
    print("\n== ② 覆盖对账（预注册 repeat 分配表 vs 实跑）==")
    print("  预注册 repeat run: %d；实跑订单目录: %d；缺失 %d；越界 %d"
          % (len(alloc_repeat), len(repeat_runs), len(missing), len(extra)))
    if missing:
        print("  缺失（未跑/未落工件）：%s" % missing)
    if extra:
        print("  越界（不在预注册 repeat 表）：%s" % extra)

    # ---------------------------------------------------------------- ③ 数据门 + repeat 轮内统计
    comparisons = [("A1", "A2", "（repeat 轮内）"), ("A1", "A0", "（repeat 轮内）"),
                   ("A2", "A0", "（repeat 轮内）")]
    gates = gate_rows_for(repeat_runs, comparisons)
    print("\n== ③ 数据门 + repeat 轮内统计（描述性；不签发主判定）==")
    for g in gates:
        print("  %-22s n_paired=%2d（<20? %s）缺 report: %s=%d %s=%d（>4? %s）→ GATE %s"
              % (g["comparison"], g["n_paired"], g["gate_paired_lt20"],
                 g["comparison"].split("−")[0], g["missing_report_" + g["comparison"].split("−")[0]],
                 g["comparison"].split("−")[1].split("（")[0],
                 g["missing_report_" + g["comparison"].split("−")[1].split("（")[0]],
                 g["gate_missing_gt4"], "命中" if g["gate_fires"] else "通过"))
    print("\n== repeat 轮内配对对照（all_hit 二值化；描述性）==")
    pair_stats = {}
    for treat, ctrl, suffix in comparisons:
        pairs, _u = pair_runs(repeat_runs, treat, ctrl)
        st = paired_stats(pairs, treat, ctrl, "all_hit", suffix)
        pair_stats[st["comparison"]] = st
        dL, dU = st["tost_cp90_pp"]
        b95 = st["bootstrap95_pp"]
        print("  %-22s n_p=%2d b=%2d c=%2d s=%2d diff=%+7.2fpp McNemar p(1侧)=%.4f "
              "TOST CP90=[%s, %s]pp boot95=[%.2f, %.2f]pp"
              % (st["comparison"], st["n_paired"], st["b_treat_better"], st["c_ctrl_better"],
                 st["s_discordant"], st["diff_pp"], st["mcnemar_p_one_sided_treat"],
                 ("%+.2f" % dL) if dL is not None else "NA", ("%+.2f" % dU) if dU is not None else "NA",
                 b95[0] if b95[0] is not None else float("nan"),
                 b95[1] if b95[1] is not None else float("nan")))

    print("\n== repeat 轮单臂统计 ==")
    arm_summaries = {}
    for a in REPEAT_ARMS:
        s = arm_summary(repeat_runs, a)
        arm_summaries[a] = s
        print("  %-4s %-20s n=%2d(report %2d) all_hit=%2d/%2d=%.4f Wilson95=[%.3f,%.3f] "
              "rate mean=%.4f cites sum=%d tok=%d"
              % (a, ARM_LABEL[a], s["n_orders"], s["n_with_report"], s["n_all_hit"],
                 s["n_with_report"], s["all_hit_rate"] or 0,
                 s["all_hit_wilson95"][0] or 0, s["all_hit_wilson95"][1] or 0,
                 s["quote_hit_rate_mean"] or 0, s["citations_total_sum"], s["tokens_total_sum"]))

    # ---------------------------------------------------------------- ④ flakiness
    flak = None
    pooled = None
    if main_runs:
        flak = flakiness_analysis(main_runs, repeat_runs)
        pooled = pooled_exploratory(main_runs, repeat_runs)
        print("\n== ④ flakiness（主轮 × repeat 轮 同 (arm,task) 配对）==")
        print("  配对数 %d（主轮有报告 %d / repeat 轮有报告 %d）"
              % (flak["n_pairs"], flak["main_orders_with_report"], flak["repeat_orders_with_report"]))
        for arm in REPEAT_ARMS:
            a = flak["per_arm"].get(arm)
            if not a:
                continue
            print("  %-4s n=%2d all_hit 翻转 %d (%.1f%%) reward 翻转 %d (%.1f%%) "
                  "token 比 r2/r1 均值 %.3f 中位 %.3f 区间 [%.3f, %.3f]"
                  % (arm, a["n_paired"], a["n_all_hit_flip"], 100 * a["all_hit_flip_rate"],
                     a["n_reward_flip"], 100 * a["reward_flip_rate"],
                     a["token_ratio_repeat_over_main_mean"] or 0,
                     a["token_ratio_repeat_over_main_median"] or 0,
                     a["token_ratio_repeat_over_main_min"] or 0,
                     a["token_ratio_repeat_over_main_max"] or 0))
        print("  单轮不收敛任务（两轮 all_hit 或 reward 不一致）：%d 条" % flak["n_non_converging"])
        for d in flak["non_converging"]:
            print("    %s %s：main(all_hit=%s,reward=%s) vs repeat(all_hit=%s,reward=%s) %s/%s"
                  % (d["arm"], d["paper"], d["main_all_hit"], d["main_reward"],
                     d["repeat_all_hit"], d["repeat_reward"], d["main_order"], d["repeat_order"]))
        print("\n== 探索性 pooled 读数（两轮合并；并入主判定须开 EXP-1b，本批不作判定）==")
        for p in pooled:
            print("  %-24s n_obs=%d（任务 %d）diff=%+.2fpp 不一致对 s=%d "
                  "朴素 McNemar p=%.4f 任务聚类 boot95=[%.2f, %.2f]pp"
                  % (p["comparison"], p["n_observations"], p["n_tasks"], p["diff_pp"],
                     p["s_discordant"], p["mcnemar_p_one_sided_treat_naive"],
                     p["cluster_bootstrap95_pp"][0], p["cluster_bootstrap95_pp"][1]))

    # ---------------------------------------------------------------- ⑤ 成本
    tok_total = sum(r["tokens_total"] for r in repeat_runs if r["tokens_total"] is not None)
    cost = {
        "runs_executed_repeat": len(repeat_runs),
        "runs_planned_repeat": len(alloc_repeat),
        "tokens_total_repeat": tok_total,
        "cents_equiv_at_F5c_basis": round(tok_total * F5C_CENTS_PER_TOK, 2),
        "usd_equiv_at_F5c_basis": round(tok_total * F5C_CENTS_PER_TOK / 100.0, 4),
        "basis": "2¢/17,935 tok（F-第五单-交付-20261025.md:152 实测基座；EXP-1.budget.basis 同引）——折算值非实付",
        "cents_spent_meter": "not_recorded（订单 ledger/records 无记账行；同主轮口径如实登记）",
        "prereg_usd_cap_experiment_total": exp1["budget"]["usd_cap"],
        "prereg_usd_estimate_experiment_total": exp1["budget"]["usd_estimate"],
    }
    print("\n== ⑤ 成本 ==")
    print("  repeat 轮 tokens 合计 %d → 折算 %.2f¢（≈$%.4f；basis 2¢/17935 tok，折算值非实付）"
          % (tok_total, cost["cents_equiv_at_F5c_basis"], cost["usd_equiv_at_F5c_basis"]))

    # ---------------------------------------------------------------- 交付件
    out_dir = os.path.abspath(args.out_dir)
    os.makedirs(out_dir, exist_ok=True)
    per_run_path = os.path.join(out_dir, "exp1-repeats-per-run-detail.jsonl")
    with open(per_run_path, "w", encoding="utf-8") as f:
        for r in repeat_runs:
            out = dict(r)
            out.pop("citation_detail", None)
            out["citation_hit_detail"] = [
                {"n": d["n"], "status": d["status"], "snapshot_ref": d["snapshot_ref"]}
                for d in r["citation_detail"]]
            f.write(json.dumps(out, ensure_ascii=False) + "\n")
    print("\n== 交付件 ==")
    print("  per-run 明细 → %s（%d 行）" % (per_run_path, len(repeat_runs)))

    summary = {
        "artifact": "exp1-repeats-summary",
        "exp_id": "EXP-1",
        "round": "repeat",
        "generated_by": "af/poc/poc-exp1-repeats/exp1-repeats-aggregate.py",
        "prereg": {
            "experiments_v1": spec_path,
            "allocation": alloc_path,
            "materials": materials_path,
            "randomization_repeats": exp1["randomization"]["repeats"],
            "sample_size_repeats_runs": exp1["sample_size"]["repeats_runs"],
            "runs_total_prereg": exp1["sample_size"]["runs_total"],
            "change_rule": exp1["preregistration"]["change_rule"],
        },
        "verdict_discipline": {
            "issues_exp1_verdict": False,
            "note": ("本轮不签发 EXP-1 主判定；主判定（inconclusive）由主轮 144 单经 "
                     "tools/b-verdict-unique.py 唯一判定程序产出，不改写。repeat 轮按预注册文字"
                     "用途（估 flakiness）只产出描述性统计与不收敛任务名单；把两轮 pooled 并入"
                     "主判定＝改判据 ⇒ 须开新 exp_id「EXP-1b」，本批不并入。"),
        },
        "coverage": {
            "prereg_repeat_runs": len(alloc_repeat),
            "executed_orders": len(repeat_runs),
            "with_report": n_rep,
            "missing_vs_prereg": ["%s/%s" % (a, p) for p, a in missing],
            "extra_vs_prereg": ["%s/%s" % (a, p) for p, a in extra],
        },
        "crosscheck": {
            "citations_total_consistent": sum(1 for r in repeat_runs
                                              if r["crosscheck"].get("citations_total_consistent")),
            "quote_failures_recomputed_consistent": sum(
                1 for r in repeat_runs if r["crosscheck"].get("quote_failures_verify_log_vs_recomputed")),
            "reward_vs_judges_consistent": sum(
                1 for r in repeat_runs if r["crosscheck"].get("reward_vs_judges")),
            "with_report": n_rep,
        },
        "gate_rows_repeat_round": gates,
        "arm_summaries_repeat_round": arm_summaries,
        "paired_repeat_round": pair_stats,
        "flakiness": flak,
        "pooled_exploratory": pooled,
        "cost": cost,
        "regress_main": regress,
    }
    summary_path = os.path.join(out_dir, "exp1-repeats-summary.json")
    with open(summary_path, "w", encoding="utf-8") as f:
        json.dump(summary, f, ensure_ascii=False, indent=1, sort_keys=True)
    print("  汇总 → %s" % summary_path)
    return 0


def regress_main(summary_path, main_runs):
    """口径对拍：本工具同源实现 vs 主轮已发布 exp1-aggregate-summary.json（逐项数值比对）。"""
    pub = json.load(open(summary_path, encoding="utf-8"))
    checks = {}

    def add(name, got, want, tol=1e-9):
        ok = (got == want) if not isinstance(got, float) else abs(got - want) <= tol
        checks[name] = {"got": got, "want": want, "pass": bool(ok)}

    add("n_orders", len(main_runs), pub["n_orders"])
    add("n_with_report", sum(1 for r in main_runs if r.get("report_present")), pub["n_with_report"])
    for arm, s in pub["arm_summaries"].items():
        mine = arm_summary(main_runs, arm)
        add("arm[%s].n_with_report" % arm, mine["n_with_report"], s["n_with_report"])
        add("arm[%s].n_all_hit" % arm, mine["n_all_hit"], s["n_all_hit"])
        add("arm[%s].all_hit_rate" % arm, round(mine["all_hit_rate"] or 0, 12),
            round(s["all_hit_rate"] or 0, 12))
        add("arm[%s].citations_total_sum" % arm, mine["citations_total_sum"], s["citations_total_sum"])
        add("arm[%s].tokens_total_sum" % arm, mine["tokens_total_sum"], s["tokens_total_sum"])
    for comp, s in pub["paired"].items():
        treat, ctrl = comp.split("−")
        pairs, _u = pair_runs(main_runs, treat, ctrl)
        mine = paired_stats(pairs, treat, ctrl)
        add("paired[%s].n_paired" % comp, mine["n_paired"], s["n_paired"])
        add("paired[%s].diff_pp" % comp, round(mine["diff_pp"], 12), round(s["diff_pp"], 12))
        add("paired[%s].b" % comp, mine["b_treat_better"], s["b_treat_better"])
        add("paired[%s].c" % comp, mine["c_ctrl_better"], s["c_ctrl_better"])
        add("paired[%s].mcnemar_p" % comp, round(mine["mcnemar_p_one_sided_treat"], 12),
            round(s["mcnemar_p_one_sided_treat"], 12))
        add("paired[%s].tost_lo" % comp, round(mine["tost_cp90_pp"][0], 9),
            round(s["tost_cp90_pp"][0], 9))
        add("paired[%s].tost_hi" % comp, round(mine["tost_cp90_pp"][1], 9),
            round(s["tost_cp90_pp"][1], 9))
    return {"reference": summary_path, "checks": checks,
            "all_pass": all(c["pass"] for c in checks.values())}


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
