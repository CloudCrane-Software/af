#!/usr/bin/env python3
"""mes-ttl-audit-consumer — mes-ttl 审计结论的机器消费腿（RT-E58 拦截④）。

债（阶段欠账台账:39 / 逃逸问题台账 RT-E58）：拦截①②③已落在 tools/mes-ttl-sweep.py
（①钉死清单数量下界红牌 audit-shrink；②审计失败/缩水 rc 上抛，现口径 4/5；
③水位行恒带 审计表数/承诺表数），但审计 rc/红牌此前**零机器消费者**
（grep -c ttl tools/all-gates.sh=0；mes-ttl-decision-point.py 不读 otel）。
观测面产物无人消费 = 没有观测。

本件=那条只读消费腿（一次性进程、零新增守护、零网络、零写入）：
  check 模式（门姿势）——近 N 行决策点 + 近 N 条 otel 审计证据：
    · 决策点 状态观测/mes-ttl-decision-point.jsonl 近 N 行有红牌 / rc 非零 ⇒ 红；
    · 证据   工具状态备份/mes-ttl-sweep-evidence.jsonl 近 N 条 kind=otel-readonly-audit
      行：action ∈ {audit-ttl-drift, audit-shrink, audit-error} /
      extra.otel_red_reasons 非空 / 审计表数 < 承诺表数 / 两数字缺席 ⇒ 红
      （消费拦截①②③ 已落码的机读字段，判据只增不减）；
    · 缺件 / 坏 JSON / 零可解析行 / 证据面零 otel 审计行 ⇒ rc=2 不可证不猜绿。
  主锚 5221 不可达（sweep 总 rc=1、decision-point 的 mes-5221-decision-point 行）
  **不是** otel 审计红牌——不把 OBS-D7 死锚误判成本门红。

  selftest 模式：临时目录夹具，**子进程跑本体**（证可执行面，不证函数）：
    往决策点注一条红牌 ⇒ 必须红；干净决策点+干净证据 ⇒ 必须绿；
    证据缩水/两数字缺席/缺件/坏 JSON 同向必红。

退出码（对齐 all-gates.sh run_item :360-365：rc=0 ⇒ PASS、rc=2 ⇒ FAIL、其余非零 ⇒ FAIL）：
  0 ＝ 绿（近 N 行无红牌、无非零审计 rc）
  1 ＝ 红（红牌 / 审计 rc 非零 / 数量下界破）——红文案含字面量 FAIL
  2 ＝ 不可证（缺件/坏 JSON/零审计行）或用法错

纪律：本件只读消费面，**不复制** mes-ttl-sweep.py 的审计判据（TTL 回读/钉死表对账
仍在 sweep 一处）；sweep 侧判据变更只需保持 evidence/decision-point 字段契约。
禁改 mes-ttl-sweep.py 本体（本批硬约束）。
"""
from __future__ import print_function

import argparse
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
WS = os.path.dirname(HERE)
DEFAULT_DECISION = os.path.join(WS, "状态观测", "mes-ttl-decision-point.jsonl")
DEFAULT_EVIDENCE = os.path.join(WS, "工具状态备份", "mes-ttl-sweep-evidence.jsonl")
DEFAULT_N = 16

RC_GREEN = 0
RC_RED = 1
RC_UNPROVABLE = 2

RED_ACTIONS = frozenset({"audit-ttl-drift", "audit-shrink", "audit-error"})
OTEL_RED_RC = frozenset({4, 5})  # sweep 2026-11-03：otel 红牌单独=4；双红=5
OTEL_KIND = "otel-readonly-audit"


def _load_jsonl(path, last_n, kind=None):
    """只读 JSONL。返回 (objects, err)。缺件/不可读/无任何可解析对象 → err。

    kind 非空时先按 obj.kind 过滤再取近 N 条——证据面 otel 审计行写在对象行之前，
    按「全文件近 N 行」会把审计行挤出窗口（假绿/假不可证）。
    """
    try:
        with open(path, "r", encoding="utf-8") as fh:
            raw_lines = fh.read().splitlines()
    except OSError as e:
        return None, "缺件/不可读：%s（%s）" % (path, e)
    objects = []
    bad = []
    for i, line in enumerate(raw_lines, 1):
        s = line.strip()
        if not s:
            continue
        try:
            obj = json.loads(s)
        except ValueError as e:
            bad.append("L%d:%s" % (i, e))
            continue
        if isinstance(obj, dict):
            objects.append(obj)
        else:
            bad.append("L%d:非对象" % i)
    if bad and not objects:
        return None, "坏 JSON（零可解析对象）：%s —— %s" % (path, "; ".join(bad[:3]))
    if not objects:
        return None, "零可解析行：%s（空文件或全空行——审计消费面不可证，不猜绿）" % path
    if kind is not None:
        objects = [o for o in objects if o.get("kind") == kind]
        if not objects:
            return None, "零 %s 行：%s" % (kind, path)
    if last_n > 0:
        objects = objects[-last_n:]
    return objects, None


def _reasons_of(obj):
    extra = obj.get("extra") if isinstance(obj.get("extra"), dict) else {}
    reasons = extra.get("otel_red_reasons")
    if isinstance(reasons, list):
        return [str(x) for x in reasons if x]
    return []


def is_red_card(obj):
    """一条 JSON 对象算不算红牌 / 非零审计 rc。纯函数（selftest 钉语义）。

    红：
      · action ∈ RED_ACTIONS（拦截①缩水 / TTL 漂移 / 审计失败）；
      · extra.otel_red_reasons 非空（sweep 机器消费面字段）；
      · rc ∈ {4, 5}（otel 红牌退出码）；
      · kind=otel-readonly-audit 且 rc 为非零 int（含旧口径 rc=1）；
      · 决策点显式 status/red 红标。
    不红：mes-5221-decision-point 无 rc 字段（死锚取证行，属 OBS-D7，非本门）。
    """
    if not isinstance(obj, dict):
        return False, None
    action = obj.get("action")
    kind = str(obj.get("kind") or "")
    extra = obj.get("extra") if isinstance(obj.get("extra"), dict) else {}
    reasons = _reasons_of(obj)
    rc = obj.get("rc")

    if action in RED_ACTIONS:
        return True, "action=%s" % action
    if reasons:
        return True, "otel_red_reasons=%s" % ",".join(reasons)
    if isinstance(rc, int) and rc in OTEL_RED_RC:
        return True, "rc=%d（otel 红牌退出码）" % rc
    if kind == OTEL_KIND and isinstance(rc, int) and rc != 0:
        return True, "kind=%s rc=%d（审计非零 rc）" % (kind, rc)
    status = obj.get("status")
    if isinstance(status, str) and status.lower() in ("red", "fail"):
        return True, "status=%s" % status
    if obj.get("red") is True:
        return True, "red=true"
    return False, None


def latest_count_gate(obj):
    """拦截①③ 只钉**最新**一条 otel 审计行（历史行落码前无两数字，拿近 N 会假红）。"""
    if not isinstance(obj, dict) or obj.get("kind") != OTEL_KIND:
        return True, "最新证据行不是 otel-readonly-audit"
    extra = obj.get("extra") if isinstance(obj.get("extra"), dict) else {}
    promised = extra.get("promised_table_count")
    audited = extra.get("audited_table_count")
    if not isinstance(promised, int) or not isinstance(audited, int):
        return True, "最新 otel 审计行缺 审计表数/承诺表数（拦截③ 两数字缺席）"
    if promised <= 0:
        return True, "promised_table_count=%s 非正（钉死清单形同虚设）" % promised
    if audited < promised:
        return True, "审计表数 %d < 承诺表数 %d（拦截① 数量下界红牌）" % (
            audited, promised)
    return False, None


def judge(decision_objs, evidence_objs):
    """两面近 N 行 → (rc, 文案)。纯函数。"""
    reds = []
    for i, obj in enumerate(decision_objs, 1):
        hit, why = is_red_card(obj)
        if hit:
            reds.append("decision-point[%d] %s kind=%s" % (
                i, why, obj.get("kind")))
    otel_rows = [o for o in evidence_objs if o.get("kind") == OTEL_KIND]
    if not otel_rows:
        return (RC_UNPROVABLE,
                "FAIL mes-ttl 审计消费面不可证：证据近窗零 otel-readonly-audit 行"
                "（sweep 从未产出审计行，或窗口被非审计行挤出）——不猜绿")
    for i, obj in enumerate(otel_rows, 1):
        hit, why = is_red_card(obj)
        if hit:
            reds.append("evidence.otel[%d] %s action=%s" % (
                i, why, obj.get("action")))
    count_hit, count_why = latest_count_gate(otel_rows[-1])
    if count_hit:
        reds.append("evidence.otel[latest] %s" % count_why)
    if reds:
        return (RC_RED,
                "FAIL mes-ttl 审计红牌：近窗 %d 条红 —— %s" % (
                    len(reds), "；".join(reds[:6])))
    last = otel_rows[-1]
    extra = last.get("extra") if isinstance(last.get("extra"), dict) else {}
    return (RC_GREEN,
            "PASS mes-ttl 审计消费面绿：decision-point %d 行无红牌/无非零审计 rc；"
            "otel-audit %d 行 action=%s 审计表数 %s/承诺表数 %s red_reasons=%s"
            % (len(decision_objs), len(otel_rows), last.get("action"),
               extra.get("audited_table_count"), extra.get("promised_table_count"),
               extra.get("otel_red_reasons")))


def check_files(decision_path, evidence_path, last_n=DEFAULT_N):
    d_objs, d_err = _load_jsonl(decision_path, last_n)
    if d_err:
        return RC_UNPROVABLE, "FAIL mes-ttl 决策点不可证：%s——摘门必须显式红，不静默放行" % d_err
    e_objs, e_err = _load_jsonl(evidence_path, last_n, kind=OTEL_KIND)
    if e_err:
        return RC_UNPROVABLE, "FAIL mes-ttl 审计证据不可证：%s——摘门必须显式红，不静默放行" % e_err
    return judge(d_objs, e_objs)


def _run_self(argv):
    cmd = [sys.executable, "-I", os.path.abspath(__file__)] + argv
    p = subprocess.run(cmd, capture_output=True, text=True)
    out = (p.stdout or "") + (p.stderr or "")
    return p.returncode, out


def _write_jsonl(path, rows):
    with open(path, "w", encoding="utf-8") as fh:
        for row in rows:
            fh.write(json.dumps(row, ensure_ascii=False) + "\n")


def _clean_decision():
    """现役 mes-5221-decision-point 形态（无 rc / 无红牌）——现势生产行应绿。"""
    return {
        "ts": "2026-10-29 10:20:23",
        "kind": "mes-5221-decision-point",
        "anchor": {"port_5221_listening": False, "pid_207700_alive": False,
                   "data_dir_present": True},
        "sweep_evidence": {
            "log_last_line": "mes-ttl-sweep: CH 不可达或取证失败（URLError）"
        },
        "decision_paths": ["(a) 只读拉起导出 / (b) 正式放弃"],
    }


def _clean_otel():
    return {
        "ts": "2026-11-05 04:30:00",
        "kind": OTEL_KIND,
        "action": "observe",
        "extra": {
            "otel_red_reasons": [],
            "promised_table_count": 8,
            "audited_table_count": 8,
            "ttl_drift_tables": [],
            "error": None,
        },
    }


def _red_card():
    """往决策点注的红牌（TTL 漂移，rc=4）——夹具必红。"""
    return {
        "ts": "2026-11-05 12:00:00",
        "kind": OTEL_KIND,
        "rc": 4,
        "action": "audit-ttl-drift",
        "extra": {
            "otel_red_reasons": ["ttl-drift"],
            "promised_table_count": 8,
            "audited_table_count": 8,
            "ttl_drift_tables": ["otel_traces"],
        },
    }


def selftest():
    """夹具自证：子进程跑本体。零网络、零真树污染（临时目录）。"""
    d = tempfile.mkdtemp(prefix="mes-ttl-audit-consumer-selftest-")
    try:
        decision = os.path.join(d, "decision-point.jsonl")
        evidence = os.path.join(d, "evidence.jsonl")
        cases = []

        def run_check(dp, ev):
            return _run_self(["--check", "--decision-point", dp, "--evidence", ev,
                              "--last-n", "16"])

        # ① 干净决策点 + 干净证据 ⇒ 必须绿
        _write_jsonl(decision, [_clean_decision(), _clean_decision()])
        _write_jsonl(evidence, [_clean_otel()])
        rc, out = run_check(decision, evidence)
        cases.append(("干净决策点+干净证据 ⇒ 必须绿", rc == RC_GREEN, rc, out))

        # ② 往决策点注一条红牌 ⇒ 必须红（任务书 ★ 自证会咬人）
        _write_jsonl(decision, [_clean_decision(), _red_card()])
        _write_jsonl(evidence, [_clean_otel()])
        rc, out = run_check(decision, evidence)
        bite = rc == RC_RED and "FAIL" in out
        cases.append(("往决策点注一条红牌 ⇒ 必须红", bite, rc, out))

        # ③ 证据面 audit-shrink（拦截①）⇒ 必须红
        shrink = _clean_otel()
        shrink["action"] = "audit-shrink"
        shrink["extra"]["otel_red_reasons"] = ["pinned-missing"]
        shrink["extra"]["audited_table_count"] = 7
        shrink["extra"]["pinned_missing_tables"] = ["otel_logs"]
        _write_jsonl(decision, [_clean_decision()])
        _write_jsonl(evidence, [shrink])
        rc, out = run_check(decision, evidence)
        cases.append(("证据 audit-shrink ⇒ 必须红", rc == RC_RED, rc, out))

        # ④ 证据两数字缺席（拦截③）⇒ 必须红
        missing_nums = _clean_otel()
        missing_nums["extra"].pop("promised_table_count", None)
        missing_nums["extra"].pop("audited_table_count", None)
        _write_jsonl(decision, [_clean_decision()])
        _write_jsonl(evidence, [missing_nums])
        rc, out = run_check(decision, evidence)
        cases.append(("证据缺 审计表数/承诺表数 ⇒ 必须红", rc == RC_RED, rc, out))

        # ⑤ 数量下界破但 action 仍 observe（拦截① 即使旧码漏置 action）
        silent_shrink = _clean_otel()
        silent_shrink["extra"]["audited_table_count"] = 7
        _write_jsonl(decision, [_clean_decision()])
        _write_jsonl(evidence, [silent_shrink])
        rc, out = run_check(decision, evidence)
        cases.append(("审计表数<承诺表数（action=observe）⇒ 必须红", rc == RC_RED, rc, out))

        # ⑥ 决策点缺件 ⇒ rc=2 不可证
        missing_dp = os.path.join(d, "no-such-decision.jsonl")
        _write_jsonl(evidence, [_clean_otel()])
        rc, out = run_check(missing_dp, evidence)
        cases.append(("决策点缺件 ⇒ rc=2 不可证", rc == RC_UNPROVABLE, rc, out))

        # ⑦ 证据缺件 ⇒ rc=2
        missing_ev = os.path.join(d, "no-such-evidence.jsonl")
        _write_jsonl(decision, [_clean_decision()])
        rc, out = run_check(decision, missing_ev)
        cases.append(("证据缺件 ⇒ rc=2 不可证", rc == RC_UNPROVABLE, rc, out))

        # ⑧ 决策点坏 JSON ⇒ rc=2
        with open(decision, "w", encoding="utf-8") as fh:
            fh.write("{not json\n")
        _write_jsonl(evidence, [_clean_otel()])
        rc, out = run_check(decision, evidence)
        cases.append(("决策点坏 JSON ⇒ rc=2 不可证", rc == RC_UNPROVABLE, rc, out))

        # ⑨ 显式 rc=0 的 otel 观察行不得当红（防恒红退化）
        zero_rc = _clean_otel()
        zero_rc["rc"] = 0
        _write_jsonl(decision, [_clean_decision()])
        _write_jsonl(evidence, [zero_rc])
        rc, out = run_check(decision, evidence)
        cases.append(("otel rc=0 observe ⇒ 必须绿", rc == RC_GREEN, rc, out))

        ok = True
        print("== mes-ttl-audit-consumer --selftest（判别力自证）==")
        for name, passed, rc, out in cases:
            print("[%s] %s（实测 rc=%d）" % ("PASS" if passed else "FAIL", name, rc))
            if not passed:
                ok = False
                for ln in out.splitlines()[:8]:
                    print("  | " + ln)
        print("SELFTEST_VERDICT=%s" % (
            "CONSUMER_BITES" if ok else "CONSUMER_DOES_NOT_BITE"))
        return RC_GREEN if ok else RC_RED
    finally:
        import shutil
        shutil.rmtree(d, ignore_errors=True)


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__)
    g = p.add_mutually_exclusive_group(required=True)
    g.add_argument("--check", action="store_true",
                   help="消费决策点+审计证据（门姿势，零网络）")
    g.add_argument("--selftest", action="store_true",
                   help="夹具自证（进 all-gates L3；红牌必红 / 干净必绿）")
    p.add_argument("--decision-point", default=DEFAULT_DECISION,
                   help="决策点 JSONL（默认 %s）" % DEFAULT_DECISION)
    p.add_argument("--evidence", default=DEFAULT_EVIDENCE,
                   help="sweep 证据 JSONL（默认 %s）" % DEFAULT_EVIDENCE)
    p.add_argument("--last-n", type=int, default=DEFAULT_N,
                   help="近 N 行（默认 %d）" % DEFAULT_N)
    args = p.parse_args(argv)
    if args.last_n <= 0:
        print("FAIL 用法错：--last-n 须为正整数", file=sys.stderr)
        return RC_UNPROVABLE
    if args.selftest:
        return selftest()
    rc, msg = check_files(args.decision_point, args.evidence, args.last_n)
    print(msg)
    return rc


if __name__ == "__main__":
    raise SystemExit(main())
