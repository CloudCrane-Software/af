#!/usr/bin/env python3
"""trace_attribution_batch — 离线轨迹归因批（D-55 / st-b-C073 跨面，§七.2）。

策略 st-b-C073（`strategies/st-b-C073.yaml`）：用失败模式聚类（而非总分）驱动
工具改进——把「工具使用错误」「语法错」从其他失败里单独拆出来归因。其输入是
**已完结轨迹集合**（离线批处理面）+ **人工对齐率标注**（数据/人工面）；AFB 是
单次运行蓝图，无离线批处理语义（na_reason 原文）。判官臂 `route.judge@1` 已注册
（`af/packages/components.json`），缺的是**批调度 + 标注数据面**——本件即那条腿。

本件=确定性离线批（零网络、零 LLM 调用、零真树写入，只读轨迹集合）：
  report 模式（聚类分布报表，acceptance ①）：
    · 读 records.jsonl / results.jsonl / mes-events.jsonl 导出（见下「导出规约」）；
    · 失败模式判官把每条**失败**轨迹判入三值闭集
      {tool_usage_error 工具使用错误 | syntax_error 语法错 | other 其他}
      （判据=结构化 EX 码 + status/note 关键词，映射表可审计，见 EX_CODE_CLASS）；
    · 落聚类分布报表（resolved / failed 及 failed 三类计数+占比 milli）；
    · **零轨迹=fail-closed 红**（acceptance ①：源在场但无可解析轨迹 ⇒ rc=1 红）。
  align 模式（人工对齐率，acceptance ②）：
    · 读标注夹具（trace_id → human_label ∈ 三值闭集）；
    · 对每条标注轨迹跑同款判官，算 judge-vs-human 一致率（alignment_rate_milli）；
    · 落逐类一致/混淆/分歧清单；
    · **无标注=红**（acceptance ②：夹具缺件/空/零有效标注 ⇒ rc=1 红）。
  selftest 模式：临时目录夹具，**子进程跑本体**（证可执行面，不证函数）。

退出码（对齐 tools/mes-ttl-audit-consumer.py 家规：rc=0 绿 / rc=1 红 / rc=2 不可证）：
  0 = 绿（report 落出非空分布；align 算出对齐率）
  1 = 红（零轨迹 / 无标注 / 夹具不匹配 / 自证不咬人——红文案含字面量 FAIL）
  2 = 不可证（源缺件/坏 JSON/用法错——不猜绿）

导出规约（records.jsonl / mes，逐行 JSON object）：
  · **result 行**（results.jsonl）：键含 task_id 且（suite | error_code | verifier_note）——
    任务级终局，failure 信号=status/error_code/verifier_note/reward(verdict)。
  · **record 行**（records.jsonl）：键含 item_id 或（node_id & plan_id）——
    节点级执行读数，failure 信号=verdict/gate/status/note（`record.rs:213-244` 形状）。
  · **mes 行**（mes-events.jsonl）：键含 work_order_id & kind——生命周期事件
    （release/start/complete/close，实测仅此四种）；按 work_order_id 聚合，
    reached complete|close ⇒ 一条 resolved 轨迹（只贡献分母，无失败模式可归因）。
  行形状无法识别 ⇒ 计入 unparsed，不静默当轨迹。

纪律：本件**零改判据件/门**（`ci/strategies_check.py`、`ci/criteria.yaml`、
`ci/baselines/gate-inputs.sha256` 一律不碰，dev 型 bot 卡禁项同款，制度 §六.4）；
只读轨迹集合；EX 码→类别映射以 `crates/af-execline/src/error.rs:87-95` 语义为据。
"""
from __future__ import print_function

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
WS = os.path.dirname(HERE)  # af 仓根

RC_GREEN = 0
RC_RED = 1
RC_UNPROVABLE = 2

# 失败模式三值闭集（st-b-C073 / S067 固定，禁增删）
CLASSES = ("tool_usage_error", "syntax_error", "other")
CLASS_ZH = {
    "tool_usage_error": "工具使用错误",
    "syntax_error": "语法错",
    "other": "其他",
}

# EX 语义码 → 失败类别。依据 crates/af-execline/src/error.rs:87-95（亲读）：
#   EX4001-4006/4008 = 计划装订/消费面结构·契约·模板·图形态错误 ⇒ 蓝图本身
#     「语法/结构」错（AFB 轨迹对 S067「语法错」的同构映射）；
#   EX4007 = 预算熔断、EX4009 = 节点墙钟超时 ⇒ 资源/时限 ⇒ other。
EX_CODE_CLASS = {
    "EX4001": "syntax_error",  # 计划装订-模板引用非法
    "EX4002": "syntax_error",  # 计划装订-模板未登记
    "EX4003": "syntax_error",  # 计划装订-模板漂移
    "EX4004": "syntax_error",  # 计划装订-registry_digest 不符
    "EX4005": "syntax_error",  # 计划装订-一致性核对失败
    "EX4006": "syntax_error",  # 计划消费-envelope 交接违例
    "EX4008": "syntax_error",  # 计划消费-图形态不支持
    "EX4007": "other",         # 计划消费-预算熔断
    "EX4009": "other",         # 计划消费-节点墙钟超时
}

# status 视为失败终局（对齐 record.rs / results.jsonl 实测 status 面）
FAIL_STATUSES = frozenset({
    "setup_failed", "run_failed", "failed", "error", "tool_error",
    "unit_error", "tool_call_failed", "bad_tool_args", "aborted",
})
RESOLVED_STATUSES = frozenset({
    "completed", "complete", "ok", "resolved", "passed", "success", "done", "close",
})

# 无映射 EX 码时，按 status/note 关键词兜底判类（可审计；EX 码优先）。
# 关键词均为**特定短语**（禁裸单词，防 "digest" 咬中 "args_digest" 之类假阳性——
# selftest ⑧ 亲测：裸 "digest" 曾把工具参数错误判成语法错，故 ASCII 词走 \b 词界，
# CJK 走子串）。三类各计命中数，取最大；平分 ⇒ other（保守不当类）。
SYNTAX_KW = (
    "计划装订", "一致性核对", "envelope", "交接违例", "模板", "引用非法", "未登记",
    "漂移", "强制集", "不符类型", "required 字段", "缺 required", "registry_digest",
    "图形态", "解析失败", "语法错",
)
TOOL_KW = (
    "工具调用", "工具使用", "工具错误", "参数错", "参数不符", "off-interface",
    "接口不符", "调用失败", "重试耗尽", "tool call", "tool error", "tool usage",
    "bad args", "tool_call",
)
OTHER_KW = (
    "预算熔断", "墙钟超时", "配额", "预算", "超时", "timeout", "budget", "quota",
    "oom", "out of memory", "network", "网络",
)


def _kw_score(text, kws):
    """关键词命中数。ASCII 关键词用 \\b 词界（防子串假阳性）；CJK 用子串。"""
    n = 0
    for k in kws:
        if k.isascii():
            if re.search(r"\b" + re.escape(k) + r"\b", text, re.IGNORECASE):
                n += 1
        elif k in text:
            n += 1
    return n


def _as_bool(v):
    """reward/verdict 归一为 True/False/None。null=供给面失败（sweng.rs:680-681）。"""
    if isinstance(v, bool):
        return v
    if v is None:
        return None
    if isinstance(v, (int, float)):
        return bool(v)
    if isinstance(v, str):
        s = v.strip().lower()
        if s in ("true", "pass", "passed", "ok", "resolved", "1"):
            return True
        if s in ("false", "fail", "failed", "null", "none", "0"):
            return False
    return None


def _norm_status(v):
    return str(v).strip().lower() if v is not None else ""


def _norm_gate(v):
    return str(v).strip().lower() if v is not None else ""


def _text_of(obj):
    """汇聚可判类的自由文本（note / verifier_note / error / detail）。"""
    parts = []
    for k in ("verifier_note", "note", "error", "detail", "message"):
        v = obj.get(k)
        if isinstance(v, str) and v:
            parts.append(v)
    return " ".join(parts)


def normalize_row(obj, source):
    """一行 JSON object → 规范化 trace dict；形状不识 ⇒ None。

    trace 至少含：trace_id / source / kind / status / verdict / gate /
    error_code / text。kind ∈ {result, record}。mes 行不在此（另行聚合）。
    """
    if not isinstance(obj, dict):
        return None
    status = _norm_status(obj.get("status"))
    verdict = _as_bool(obj.get("verdict", obj.get("reward")))
    gate = _norm_gate(obj.get("gate"))
    error_code = obj.get("error_code")
    error_code = str(error_code).strip() if error_code else ""
    text = _text_of(obj)

    if "task_id" in obj and ("suite" in obj or "error_code" in obj
                             or "verifier_note" in obj):
        kind = "result"
        trace_id = str(obj.get("task_id"))
    elif "item_id" in obj or ("node_id" in obj and "plan_id" in obj):
        kind = "record"
        base = str(obj.get("item_id") or obj.get("plan_id"))
        node = obj.get("node_id")
        att = obj.get("attempt")
        trace_id = base
        if node:
            trace_id += "/" + str(node)
        if att is not None:
            trace_id += "#" + str(att)
    else:
        return None

    return {
        "trace_id": trace_id,
        "source": source,
        "kind": kind,
        "status": status,
        "verdict": verdict,
        "gate": gate,
        "error_code": error_code,
        "text": text,
    }


def load_traces(paths):
    """读轨迹集合。返回 (traces, per_source, err)。

    缺件/不可读 ⇒ err 非空（调用方判 rc=2 不可证）。mes 文件按 work_order_id 聚合，
    reached complete|close ⇒ 一条 resolved 轨迹（kind=mes，只贡献分母）。
    """
    traces = []
    per_source = []
    for path in paths:
        if not os.path.exists(path):
            return None, None, "源缺件：%s" % path
        try:
            with open(path, "r", encoding="utf-8") as fh:
                raw = fh.read().splitlines()
        except OSError as exc:
            return None, None, "源不可读：%s（%s）" % (path, exc)

        rows = 0
        unparsed = 0
        is_mes_name = os.path.basename(path).startswith("mes")
        mes_wo_terminal = {}
        mes_wo_seen = {}
        for line in raw:
            s = line.strip()
            if not s:
                continue
            rows += 1
            try:
                obj = json.loads(s)
            except (ValueError, TypeError):
                unparsed += 1
                continue
            if is_mes_name and isinstance(obj, dict) and "work_order_id" in obj \
                    and "kind" in obj:
                wo = str(obj.get("work_order_id"))
                k = _norm_status(obj.get("kind"))
                mes_wo_seen[wo] = True
                if k in ("complete", "close"):
                    mes_wo_terminal[wo] = True
                continue
            tr = normalize_row(obj, path)
            if tr is None:
                unparsed += 1
                continue
            traces.append(tr)

        if is_mes_name:
            for wo in sorted(mes_wo_terminal):
                traces.append({
                    "trace_id": wo + ":terminal",
                    "source": path,
                    "kind": "mes",
                    "status": "complete",
                    "verdict": True,
                    "gate": "ok",
                    "error_code": "",
                    "text": "",
                })
        per_source.append({
            "path": path,
            "rows": rows,
            "unparsed": unparsed,
            "mes_work_orders_seen": len(mes_wo_seen) if is_mes_name else 0,
        })
    return traces, per_source, None


def is_failure(tr):
    """轨迹是否为失败（判官只对失败判类）。"""
    if tr["error_code"]:
        return True
    if tr["verdict"] is False:
        return True
    if tr["status"] in FAIL_STATUSES:
        return True
    if tr["gate"] and tr["gate"] != "ok":
        return True
    # verdict 缺失且状态非 resolved ⇒ 视为未决失败（fail-closed：不当成功）
    if tr["verdict"] is None and tr["status"] and tr["status"] not in RESOLVED_STATUSES:
        return True
    return False


def is_resolved(tr):
    return (tr["verdict"] is True and (not tr["gate"] or tr["gate"] == "ok")
            and not tr["error_code"] and (not tr["status"] or tr["status"] in RESOLVED_STATUSES))


def judge(tr):
    """失败模式判官：失败轨迹 → 三值闭集之一。仅对 is_failure 为真调用。

    判据优先级（可审计）：①显式工具错误 status（tool_error 等）→ ②结构化 EX 码
    （EX_CODE_CLASS，编译/执行面落的确定信号）→ ③status/note 关键词三分类计分取最大
    （平分 ⇒ other，保守不当类）。EX 码优先于自由文本；关键词仅兜底无码失败。
    """
    code = tr["error_code"].upper()
    status = tr["status"]
    if status in ("tool_error", "unit_error", "tool_call_failed", "bad_tool_args"):
        return "tool_usage_error"
    if code in EX_CODE_CLASS:
        return EX_CODE_CLASS[code]
    text = tr["text"]
    scores = {
        "syntax_error": _kw_score(text, SYNTAX_KW),
        "tool_usage_error": _kw_score(text, TOOL_KW),
        "other": _kw_score(text, OTHER_KW),
    }
    best = max(scores, key=lambda c: scores[c])
    if scores[best] == 0:
        return "other"  # 无任何命中 ⇒ 保守归 other（不猜工具/语法）
    # 平分（含与 other 平分）⇒ other：唯一天然决胜类，避免把歧义失败误判成具体类
    top = [c for c in CLASSES if scores[c] == scores[best]]
    if len(top) > 1:
        return "other"
    return best


def build_report(traces):
    """聚类分布报表（确定性；占比用 milli 整数，禁浮点漂移）。"""
    resolved = [t for t in traces if is_resolved(t)]
    failed = [t for t in traces if is_failure(t) and not is_resolved(t)]
    indeterminate = len(traces) - len(resolved) - len(failed)
    dist = {c: 0 for c in CLASSES}
    by_code = {}
    by_source = {}
    per_trace = []
    for t in failed:
        c = judge(t)
        dist[c] += 1
        code = t["error_code"] or "(none)"
        by_code[code] = by_code.get(code, 0) + 1
        src = t["source"]
        by_source.setdefault(src, {c: 0 for c in CLASSES})
        by_source[src][c] += 1
        per_trace.append({
            "trace_id": t["trace_id"], "class": c, "error_code": t["error_code"],
            "status": t["status"], "source": t["source"],
        })
    n_fail = len(failed)
    dist_milli = {}
    for c in CLASSES:
        dist_milli[c] = (dist[c] * 1000 // n_fail) if n_fail else 0
    return {
        "population": {
            "total_traces": len(traces),
            "resolved": len(resolved),
            "failed": n_fail,
            "indeterminate": indeterminate,
        },
        "failure_distribution": {
            c: {"count": dist[c], "share_milli": dist_milli[c]} for c in CLASSES
        },
        "by_error_code": dict(sorted(by_code.items())),
        "by_source": by_source,
        "failed_traces": per_trace,
    }


def load_annotations(path):
    """读标注夹具。返回 (rows, err)。每行 {trace_id, human_label ∈ CLASSES}。"""
    if not os.path.exists(path):
        return None, "标注夹具缺件：%s" % path
    try:
        with open(path, "r", encoding="utf-8") as fh:
            raw = fh.read().splitlines()
    except OSError as exc:
        return None, "标注夹具不可读：%s（%s）" % (path, exc)
    rows = []
    bad = 0
    for line in raw:
        s = line.strip()
        if not s:
            continue
        try:
            obj = json.loads(s)
        except (ValueError, TypeError):
            bad += 1
            continue
        if not isinstance(obj, dict):
            bad += 1
            continue
        tid = obj.get("trace_id")
        label = obj.get("human_label")
        if not tid or label not in CLASSES:
            bad += 1
            continue
        rows.append({"trace_id": str(tid), "human_label": label})
    return {"rows": rows, "bad": bad}, None


def build_alignment(traces, ann):
    """人工对齐率：judge-vs-human 一致率 + 逐类/混淆/分歧/孤儿。"""
    by_id = {}
    for t in traces:
        by_id.setdefault(t["trace_id"], t)
    agree = 0
    valid = 0
    orphans = []
    per_class = {c: {"n": 0, "agree": 0} for c in CLASSES}
    confusion = {h: {j: 0 for j in CLASSES} for h in CLASSES}
    disagreements = []
    for a in ann["rows"]:
        t = by_id.get(a["trace_id"])
        if t is None:
            orphans.append(a["trace_id"])
            continue
        if not is_failure(t):
            # 标注指向一条 resolved 轨迹=夹具不一致（不给成功轨迹标失败模式）
            orphans.append(a["trace_id"] + "(resolved)")
            continue
        j = judge(t)
        h = a["human_label"]
        valid += 1
        per_class[h]["n"] += 1
        confusion[h][j] += 1
        if j == h:
            agree += 1
            per_class[h]["agree"] += 1
        else:
            disagreements.append({
                "trace_id": a["trace_id"], "human": h, "judge": j,
                "error_code": t["error_code"],
            })
    rate_milli = (agree * 1000 // valid) if valid else 0
    return {
        "labeled": len(ann["rows"]),
        "valid": valid,
        "agree": agree,
        "alignment_rate_milli": rate_milli,
        "per_class": per_class,
        "confusion": confusion,
        "disagreements": disagreements,
        "orphans": orphans,
        "annotation_bad_rows": ann["bad"],
    }


def _fmt_report(rep, per_source):
    lines = []
    p = rep["population"]
    lines.append("== 离线轨迹归因批 · 聚类分布报表 ==")
    lines.append("总轨迹=%d  resolved=%d  failed=%d  indeterminate=%d" % (
        p["total_traces"], p["resolved"], p["failed"], p["indeterminate"]))
    lines.append("失败模式分布（failed 内占比）：")
    for c in CLASSES:
        d = rep["failure_distribution"][c]
        lines.append("  %-18s count=%-4d share=%.1f%%" % (
            CLASS_ZH[c] + "(" + c + ")", d["count"], d["share_milli"] / 10.0))
    if rep["by_error_code"]:
        lines.append("按 EX 码：" + "  ".join(
            "%s=%d" % (k, v) for k, v in rep["by_error_code"].items()))
    for ps in per_source:
        lines.append("源 %s：rows=%d unparsed=%d" % (
            ps["path"], ps["rows"], ps["unparsed"]))
    return "\n".join(lines)


def _fmt_alignment(al):
    lines = []
    lines.append("== 人工对齐率（judge vs human）==")
    lines.append("标注=%d  有效=%d  一致=%d  对齐率=%.1f%%" % (
        al["labeled"], al["valid"], al["agree"], al["alignment_rate_milli"] / 10.0))
    for c in CLASSES:
        pc = al["per_class"][c]
        lines.append("  %-18s n=%-3d agree=%-3d" % (CLASS_ZH[c] + "(" + c + ")", pc["n"], pc["agree"]))
    if al["disagreements"]:
        lines.append("分歧：")
        for d in al["disagreements"]:
            lines.append("  %s human=%s judge=%s (%s)" % (
                d["trace_id"], d["human"], d["judge"], d["error_code"] or "-"))
    if al["orphans"]:
        lines.append("孤儿标注（collection 无此失败轨迹）：" + ", ".join(al["orphans"]))
    return "\n".join(lines)


def cmd_report(sources, json_out):
    traces, per_source, err = load_traces(sources)
    if err:
        print("FAIL 不可证：%s" % err, file=sys.stderr)
        return RC_UNPROVABLE
    # acceptance ①：源在场但零可解析轨迹 ⇒ fail-closed 红
    if not traces:
        print("FAIL 零轨迹：源在场但无可解析轨迹（%s）——fail-closed 红" % ", ".join(sources),
              file=sys.stderr)
        return RC_RED
    rep = build_report(traces)
    rep["sources"] = per_source
    print(_fmt_report(rep, per_source))
    if json_out:
        with open(json_out, "w", encoding="utf-8") as fh:
            fh.write(json.dumps(rep, ensure_ascii=False, indent=2) + "\n")
        print("报表已落：%s" % json_out)
    return RC_GREEN


def cmd_align(sources, annotations, json_out, min_align_milli):
    traces, per_source, err = load_traces(sources)
    if err:
        print("FAIL 不可证：%s" % err, file=sys.stderr)
        return RC_UNPROVABLE
    ann, aerr = load_annotations(annotations)
    if aerr:
        # acceptance ②：无标注=红（夹具缺件即数据面缺席，区别于一般缺件 rc=2）
        print("FAIL 无标注：%s——fail-closed 红" % aerr, file=sys.stderr)
        return RC_RED
    if not ann["rows"]:
        print("FAIL 无标注：标注夹具零有效标注行（bad=%d）——fail-closed 红" % ann["bad"],
              file=sys.stderr)
        return RC_RED
    al = build_alignment(traces, ann)
    if al["valid"] == 0:
        print("FAIL 无标注：全部标注为孤儿（collection 无匹配失败轨迹）——fail-closed 红",
              file=sys.stderr)
        return RC_RED
    print(_fmt_alignment(al))
    if json_out:
        with open(json_out, "w", encoding="utf-8") as fh:
            fh.write(json.dumps(al, ensure_ascii=False, indent=2) + "\n")
        print("对齐读数已落：%s" % json_out)
    if min_align_milli > 0 and al["alignment_rate_milli"] < min_align_milli:
        print("FAIL 对齐率 %.1f%% < 下界 %.1f%%" % (
            al["alignment_rate_milli"] / 10.0, min_align_milli / 10.0), file=sys.stderr)
        return RC_RED
    return RC_GREEN


# ---------------- selftest（子进程跑本体，证可执行面） ----------------

def _run_self(argv):
    cmd = [sys.executable, "-I", os.path.abspath(__file__)] + argv
    p = subprocess.run(cmd, capture_output=True, text=True)
    return p.returncode, (p.stdout or "") + (p.stderr or "")


def _write_jsonl(path, rows):
    with open(path, "w", encoding="utf-8") as fh:
        for r in rows:
            fh.write(json.dumps(r, ensure_ascii=False) + "\n")


def _rec_result(tid, status, code, note, verdict=None):
    return {"suite": "graph", "task_id": tid, "status": status,
            "error_code": code, "verifier_note": note, "reward": verdict}


def selftest():
    d = tempfile.mkdtemp(prefix="trace-attribution-selftest-")
    cases = []
    try:
        recs = os.path.join(d, "records.jsonl")
        ann = os.path.join(d, "annotations.jsonl")

        # 干净集合：resolved + 三类失败各一
        clean = [
            _rec_result("t-ok", "completed", "", "", verdict=True),
            _rec_result("t-syn", "setup_failed", "EX4005",
                        "[EX4005] 计划装订-一致性核对失败: 预算条目不在执行面强制集内"),
            _rec_result("t-tool", "run_failed", "EXTOOL1",
                        "工具调用失败：searchkit.assess 参数错 args_digest 不符"),
            _rec_result("t-other", "run_failed", "EX4007",
                        "[EX4007] 计划消费-预算熔断: cost_cap 累计 ≥ 帽"),
        ]
        _write_jsonl(recs, clean)
        good_ann = [
            {"trace_id": "t-syn", "human_label": "syntax_error"},
            {"trace_id": "t-tool", "human_label": "tool_usage_error"},
            {"trace_id": "t-other", "human_label": "other"},
        ]
        _write_jsonl(ann, good_ann)

        # ① report 干净集合 ⇒ 绿 + 落出分布
        rc, out = _run_self(["--report", "--sources", recs])
        cases.append(("report 干净集合 ⇒ 绿", rc == RC_GREEN, rc, out))

        # ② report 零轨迹（空文件）⇒ 红（acceptance ①）
        empty = os.path.join(d, "empty.jsonl")
        open(empty, "w").close()
        rc, out = _run_self(["--report", "--sources", empty])
        bite = rc == RC_RED and "FAIL" in out
        cases.append(("report 零轨迹 ⇒ 红（fail-closed）", bite, rc, out))

        # ③ align 干净集合+有效标注 ⇒ 绿 + 对齐率 100%
        rc, out = _run_self(["--align", "--sources", recs, "--annotations", ann])
        cases.append(("align 有效标注 ⇒ 绿", rc == RC_GREEN and "100.0%" in out, rc, out))

        # ④ align 无标注（缺件）⇒ 红（acceptance ②）
        missing = os.path.join(d, "no-ann.jsonl")
        rc, out = _run_self(["--align", "--sources", recs, "--annotations", missing])
        bite = rc == RC_RED and "FAIL" in out
        cases.append(("align 无标注（缺件）⇒ 红（fail-closed）", bite, rc, out))

        # ⑤ align 空标注文件 ⇒ 红
        emptyann = os.path.join(d, "empty-ann.jsonl")
        open(emptyann, "w").close()
        rc, out = _run_self(["--align", "--sources", recs, "--annotations", emptyann])
        bite = rc == RC_RED and "FAIL" in out
        cases.append(("align 空标注 ⇒ 红（fail-closed）", bite, rc, out))

        # ⑥ align 孤儿标注（trace_id 不在集合）⇒ 红
        orphan = os.path.join(d, "orphan-ann.jsonl")
        _write_jsonl(orphan, [{"trace_id": "ghost", "human_label": "other"}])
        rc, out = _run_self(["--align", "--sources", recs, "--annotations", orphan])
        bite = rc == RC_RED and "FAIL" in out
        cases.append(("align 孤儿标注 ⇒ 红（fail-closed）", bite, rc, out))

        # ⑦ report 源缺件 ⇒ rc=2 不可证
        rc, out = _run_self(["--report", "--sources", os.path.join(d, "nope.jsonl")])
        cases.append(("report 源缺件 ⇒ rc=2 不可证", rc == RC_UNPROVABLE, rc, out))

        # ⑧ 判官判别力：三类失败各判对各的类（测纯函数，防判官退化成恒一类）
        verdicts_ok = True
        for tid, expect in (("t-syn", "syntax_error"), ("t-tool", "tool_usage_error"),
                            ("t-other", "other")):
            row = [r for r in clean if r["task_id"] == tid][0]
            tr = normalize_row(row, "x")
            got = judge(tr) if is_failure(tr) else "resolved"
            if got != expect:
                verdicts_ok = False
        cases.append(("判官判别力：syn/tool/other 各判对各的类", verdicts_ok, 0, ""))

        ok = all(c[1] for c in cases)
        print("== trace_attribution_batch --selftest（判别力自证）==")
        for name, passed, rc, out in cases:
            print("[%s] %s（实测 rc=%d）" % ("PASS" if passed else "FAIL", name, rc))
            if not passed:
                for ln in out.splitlines()[:8]:
                    print("  | " + ln)
        print("SELFTEST_VERDICT=%s" % ("BATCH_BITES" if ok else "BATCH_DOES_NOT_BITE"))
        return RC_GREEN if ok else RC_RED
    finally:
        shutil.rmtree(d, ignore_errors=True)


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    g = p.add_mutually_exclusive_group(required=True)
    g.add_argument("--report", action="store_true", help="聚类分布报表（acceptance ①）")
    g.add_argument("--align", action="store_true", help="人工对齐率（acceptance ②）")
    g.add_argument("--selftest", action="store_true", help="夹具自证（子进程跑本体）")
    p.add_argument("--sources", action="append", nargs="+", default=[],
                   help="轨迹集合源（records.jsonl / results.jsonl / mes-events.jsonl，可重复/多值）")
    p.add_argument("--annotations", help="人工对齐率标注夹具 JSONL（--align 用）")
    p.add_argument("--json-out", help="报表/对齐读数 JSON 落盘路径")
    p.add_argument("--min-align-milli", type=int, default=0,
                   help="对齐率下界（milli，默认 0=仅要求夹具在场；>0 时低于即红）")
    args = p.parse_args(argv)

    if args.selftest:
        return selftest()
    sources = [s for group in (args.sources or []) for s in group]
    if not sources:
        print("FAIL 用法错：--report/--align 须给 --sources", file=sys.stderr)
        return RC_UNPROVABLE
    if args.report:
        return cmd_report(sources, args.json_out)
    if args.align:
        if not args.annotations:
            print("FAIL 用法错：--align 须给 --annotations", file=sys.stderr)
            return RC_UNPROVABLE
        return cmd_align(sources, args.annotations, args.json_out,
                         args.min_align_milli)
    return RC_UNPROVABLE


if __name__ == "__main__":
    raise SystemExit(main())
