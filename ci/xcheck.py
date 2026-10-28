#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""xcheck.py — 枚举跨文书一致性断言（G0-CI 机制批 · 子代理 D，SPEC-2 实施）

对应复盘 C2 prevention-5 / 5.1「枚举跨文书一致性」行：
  ① AGENT.md 门数（九门）↔ ci/run.sh step 实数（含 step1-8 硬门区间自洽）
  ② packages/README 组件计数（十包 37 件及逐包分解）↔ components.json 实数
  ③ 方案 §1.3 预期红名单项数/条目 ↔ ci/neutrality-expected-fail.txt 实数/条目
  ④ 观察期期满日期四文书一致（runbook §六-3 权威 ↔ README/00-总纲/组件建设状态）
  ⑤（advisory）ci/run.sh 头注「N 步验收管道」↔ step 实数——现状活漂移探针

覆盖面（按 SPEC ⑧ 口径交卷）：**3/4 组对账**——①门数/②packages/③词表 三组硬对账；
第 4 组「码位/债号/ADR 三方对账」（E 码位↔error.rs↔RESULT 自述，复盘 §5.1 第二行）
按 SPEC ⑧.2 与总对账 §六.2 **登记遗留未实现**（正本 error.rs 码位表摸排工作量未在
本批全量落地），每次运行以 [LEGACY] 行显式登记，不静默缺席。④日期组为 SPEC-2 草案
同批交付（复盘 §5.1 枚举行第四项）。

G0 批对账（合流演进）：run.sh 出现 step10-14（机制批槽位）时，基础「九门」口径照常
硬对账；AGENT.md 勘误行（G0-A 合流清单项：「九门+G0 五门=14 step」，照 W1-C53 先例
不改历史行）在场则硬核其算术，缺席只发 advisory WARN（合流前不误红、合流后不失守）。

退出码：0=硬项全过且 advisory 全过；1=任一硬项红（门数/计数/名单/日期漂移）；
        2=仅 advisory 红（头注漂移/G0 勘误未同步/句式漂移提示）——由调用方定义
        是否改 OVERALL（run.sh step14 落 SUMMARY WARN 行不改 OVERALL）。
路径纪律（红队 P2-2 root 钉死）：仓内侧默认值一律由 __file__ 推导到**本脚本所在
仓库根**（工作树参数化），不得对 agent黑灯工厂/af/ 旧分支局部副本跑（会假绿）；
工作区侧文书（AGENT.md/方案/四观察期文书）住工作区、默认值为绝对路径，可 --doc-* 覆写。
"""
import argparse
import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CN_NUM = {"一": 1, "二": 2, "三": 3, "四": 4, "五": 5, "六": 6, "七": 7, "八": 8, "九": 9, "十": 10}
HARD, SOFT = [], []  # (check_name, ok, detail_lines)
INFO_LINES = []
G0_BATCH_SIZE = 5    # 机制批 step10-14 槽位数（总对账 §二 编号统稿裁定）
G0_ERRATUM_RE = re.compile(r"九门\s*[+＋]\s*G0\s*五门\s*[=＝]\s*(\d+)\s*step")


def rec(name, ok, detail, soft=False):
    (SOFT if soft else HARD).append((name, ok, detail))


def info(msg):
    INFO_LINES.append(msg)


def read(p):
    return Path(p).read_text(encoding="utf-8")


def read_or_none(p):
    try:
        return read(p)
    except OSError:
        return None


# ---------------------------------------------------------------- ① 门数对账
def count_run_sh_steps(run_sh_text):
    """门计数。硬门 = `^step N "..."` 助手行（step1-8/10-13 形）或 `^echo "== stepN …"`
    bespoke 块行（step9/14 形）中**非 advisory** 者——bespoke 形态必须计数：退出码 2
    要落 WARN 行不改 OVERALL 的 step（如 step14 机读一致性批）用 bespoke 块表达，
    只数助手行会让新门对账眼盲（本机制自身的逃逸形态）。
    advisory 门 = echo 行带 advisory 字样（step9 FV）或 fv_advisory() 探测。
    返回 (hard_ids, advisory_n)。"""
    step_nums = set()
    for m in re.finditer(r'^step\s+(\d+)\s+"', run_sh_text, re.M):
        step_nums.add(int(m.group(1)))
    for m in re.finditer(r'^echo\s+"==\s*step(\d+)\b', run_sh_text, re.M):
        step_nums.add(int(m.group(1)))
    advisory_nums = set()
    # advisory 判定须排除连字符复合词（step7 的「advisory-db」是 cargo-deny 漏洞库，
    # 不是 advisory 门——误分类会把硬门算成 advisory、门数账失真）。
    for m in re.finditer(r'^echo\s+"==\s*step(\d+)\b[^\n]*advisory(?![\w-])', run_sh_text, re.M):
        advisory_nums.add(int(m.group(1)))
    if "fv_advisory()" in run_sh_text:
        advisory_nums.add(9)
    return sorted(step_nums - advisory_nums), len(advisory_nums)


def check_gates(agent_md, run_sh):
    a = read(agent_md)
    r = read(run_sh)
    hard_ids, advisory = count_run_sh_steps(r)
    # AGENT.md 硬纪律 5 行：取「N门」主张 + stepA-B 区间主张
    m = re.search(r"([一二三四五六七八九十])门\*{0,2}（step(\d+)-(\d+)\s*硬门", a)
    if not m:
        rec("gates", False, ["AGENT.md 硬纪律5 未匹配『N门（stepA-B 硬门…』模式——口径句改形，xcheck 需同步"],
            soft=True)  # 句式漂移属脚本维护项，先 advisory 提示
        return
    claimed = CN_NUM[m.group(1)]
    lo, hi = int(m.group(2)), int(m.group(3))
    base_hard = [i for i in hard_ids if lo <= i <= hi]        # 基础九门的硬门（step1-8）
    g0_hard = [i for i in hard_ids if i > hi]                 # G0 机制批新 step（step10-14 槽）
    d = [f"AGENT.md 主张 {claimed} 门（step{lo}-{hi} 硬门 + step9 FV advisory）",
         f"run.sh 实测 {len(base_hard)} 基础硬门 step{base_hard} + {advisory} advisory = {len(base_hard) + advisory} 门（基础口径）"]
    if g0_hard:
        d.append(f"run.sh 另见 G0 机制批新 step{g0_hard}（step10-14 槽位；当前合计 {len(hard_ids) + advisory} step）")
    # 基础口径硬对账：主张门数 == 基础硬门 + advisory，且硬门 id 连续且 == stepA-B 区间
    ok = (claimed == len(base_hard) + advisory) and (base_hard == list(range(lo, hi + 1))) and (advisory == 1)
    if not ok:
        d.append(f"红：门数漂移（{claimed} vs {len(base_hard) + advisory}）或硬门区间不连续（step{lo}-{hi} vs 实测 {base_hard}）——复盘 C2 六门/九门史复发形态")
    rec("gates", ok, d)

    # ---- G0 批 ↔ AGENT.md 勘误行对账（合流演进面） ----
    if not g0_hard:
        return
    total_now = len(hard_ids) + advisory
    em = G0_ERRATUM_RE.search(a)
    if not em:
        rec("gates-g0-erratum", False,
            [f"G0 机制批新 step{g0_hard} 已在 run.sh，AGENT.md 勘误行未同步——合流清单项："
             f"G0-A 按 W1-C53 先例走勘误行（『九门+G0 五门={9 + G0_BATCH_SIZE} step』口径，不改历史行原貌）",
             "advisory 红：合流前按基础九门口径对账（不误红），合流后须兑现勘误行"],
            soft=True)
        return
    claimed_total = int(em.group(1))
    d2 = [f"AGENT.md 勘误行主张九门+G0 五门={claimed_total} step；run.sh 实测 G0 批 {len(g0_hard)}/{G0_BATCH_SIZE} 个新 step，合计 {total_now} step"]
    if len(g0_hard) == G0_BATCH_SIZE:
        ok2 = claimed_total == total_now
        if not ok2:
            d2.append(f"红：G0 批勘误口径与 run.sh 实数不符（{claimed_total} vs {total_now}）——合流批门数账失真")
    else:
        ok2 = True
        d2.append(f"advisory：G0 批未齐（{len(g0_hard)}/{G0_BATCH_SIZE}，分组合流中），勘误口径为合流后目标值 {claimed_total}——合流齐后硬核")
        rec("gates-g0-erratum", ok2, d2, soft=True)
        return
    rec("gates-g0-erratum", ok2, d2)


# ------------------------------------------------------- ② packages 计数对账
def check_packages(readme, components_json):
    rm = read(readme)
    cj = json.loads(read(components_json))
    entries = cj["components"] if isinstance(cj, dict) else cj
    per_pkg = {}
    for it in entries:
        name = it.get("name", "") if isinstance(it, dict) else ""
        if name.startswith("pkg."):
            seg = name.split(".")
            per_pkg[seg[1]] = per_pkg.get(seg[1], 0) + 1
    actual_total = sum(per_pkg.values())
    # README 主张：『十包实为 **N 件**——a 2 / b 8 / …』（分解可跨行，取匹配点后 300 字符窗口）
    m = re.search(r"十包实为\s*\*{0,2}(\d+)\s*件\*{0,2}——", rm)
    if not m:
        rec("packages", False, ["README 未匹配『十包实为 N 件——…』主张句"], soft=True)
        return
    claimed_total = int(m.group(1))
    window = re.sub(r"\s+", " ", rm[m.end():m.end() + 300])
    claimed = {}
    for pm in re.finditer(r"([a-z]+)\s+(\d+)(?=[\s/，,]|$)", window):
        claimed[pm.group(1)] = int(pm.group(2))
    d = [f"README 主张十包 {claimed_total} 件；components.json 实测 {actual_total} 件（根级 pkg.* 段）",
         f"README 分解 {claimed}", f"实测分解 {per_pkg}"]
    ok = (claimed_total == actual_total) and (claimed == per_pkg)
    if not ok:
        bad = {k: (claimed.get(k), per_pkg.get(k, 0)) for k in set(claimed) | set(per_pkg)
               if claimed.get(k) != per_pkg.get(k, 0)}
        d.append(f"红：组件计数/分解漂移 {bad}——复盘 C2『35 vs 37』形态")
    rec("packages", ok, d)


# --------------------------------------------------- ③ 预期红名单（词表）对账
def check_redlist(plan_doc, expected_fail):
    doc = read(plan_doc)
    ef = [ln.strip() for ln in read(expected_fail).splitlines()
          if ln.strip() and not ln.strip().startswith("#")]
    # 方案 §1.3 节：标题主张数 + 表体行数 + 表体条目
    sec = re.search(r"^###\s*1\.3[^\n]*$", doc, re.M)
    if not sec:
        rec("redlist", False, ["方案文书未找到 §1.3 节标题"], soft=True)
        return
    tail = doc[sec.end():]
    nxt = re.search(r"^##+\s", tail, re.M)
    body = tail[:nxt.start()] if nxt else tail
    rows = re.findall(r"^\|\s*(\d+)\s*\|\s*([a-z-]+)\s*\|\s*`([^`]+)`", body, re.M)
    hm = re.search(r"1\.3[^\n]*?(\d+)\s*项", doc[sec.start():sec.end()])
    claimed = int(hm.group(1)) if hm else None
    doc_names = {f"{c}::{t}" for _, c, t in rows}
    ef_names = set(ef)
    d = [f"方案 §1.3 标题主张 {claimed} 项；表体 {len(rows)} 行；expected-fail.txt 实测 {len(ef)} 行",
         f"条目集合差：仅文书有 {sorted(doc_names - ef_names)}；仅名单有 {sorted(ef_names - doc_names)}"]
    ok = (claimed == len(rows) == len(ef)) and doc_names == ef_names
    if not ok:
        d.append("红：红名单跨文书漂移——复盘 C2『19→22→23 三代/只改标题不改表体』形态（W1-C67/W2-C-53）")
    rec("redlist", ok, d)


# ----------------------------------------------------- ④ 观察期日期四文书一致
def check_obs_dates(docs):
    """docs: {标签: 路径}；权威=runbook。规则：各文书『观察期』且含 至/期满 的行上
    提取的日期时间必须一致（runbook §六-3 为人权威，numbers.json obs.* 为机正本——本
    版先做四文书一致，机正本接 step12 numbers.json 复算后并轨，SPEC ⑧.4）。
    文书缺席=缺文件即未达成（复盘 C7 口径）——不静默跳过。"""
    found = {}
    for tag, p in docs.items():
        text = read_or_none(p)
        if text is None:
            rec("obs-dates", False, [f"{tag}: 文书缺席（{p}）——缺文件=未达成（复盘 C7 口径），四文书一致无法成立"])
            return
        ds = set()
        for line in text.splitlines():
            if "观察期" not in line:
                continue
            for dm in re.finditer(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}(?::\d{2})?", line):
                # 仅取前文带 期满/至 的日期（排除同行的 切流/起算 等相邻时间）
                if re.search(r"期满|至", line[max(0, dm.start() - 16):dm.start()]):
                    ds.add(dm.group(0)[:16])  # 归一到分钟
        found[tag] = ds
    allsets = [s for s in found.values() if s]
    d = [f"{tag}: {sorted(s) if s else '（无观察期日期行）'}" for tag, s in found.items()]
    common = set.intersection(*allsets) if allsets else set()
    # 收紧语义：四文书全在场，且各自观察期期满时间（分钟粒度）存在公共值且无互相矛盾的期满值
    ok = len(allsets) == len(found) and bool(common) and all(len(s & common) >= 1 and not (s - common) for s in allsets)
    if not ok:
        d.append("红：观察期期满日期跨文书不一致——复盘 C7/W2-C-17『四文书三个日期』形态")
    rec("obs-dates", ok, d)


# ------------------------------------- ⑤ advisory：run.sh 头注「N 步」活漂移探针
def check_runsh_header(run_sh):
    r = read(run_sh)
    hard_ids, advisory = count_run_sh_steps(r)
    total = len(hard_ids) + advisory
    m = re.search(r"^#\s*([一二三四五六七八九十]|\d+)步验收管道", r, re.M)
    if not m:
        rec("runsh-header", True, ["头注无『N 步验收管道』字样（可接受）"], soft=True)
        return
    tok = m.group(1)
    claimed = CN_NUM.get(tok, None) or (int(tok) if tok.isdigit() else None)
    d = [f"run.sh 头注主张 {claimed} 步；实数 {total} 门（{len(hard_ids)} 硬 + {advisory} advisory）"]
    ok = claimed == total
    if not ok:
        d.append("advisory 红：头注步数漂移（现状活例：run.sh:5『六步验收管道』vs 实数——复盘 C2 同模式）")
    rec("runsh-header", ok, d, soft=True)


# ------------------------------------------------------ ⑥ 码位组：遗留登记（不实现）
def register_codepoints_legacy():
    info("[LEGACY] codepoints 码位/债号/ADR 三方对账未实现（SPEC ⑧.2 / 总对账 §六.2：正本 "
         "error.rs 码位表摸排工作量未在本批全量落地）——机制覆盖面按 3/4 组对账交卷"
         "（门数/packages/词表）；不静默缺席")


def main():
    ap = argparse.ArgumentParser(description="枚举跨文书一致性断言（复盘 C2 / 5.1 枚举行）")
    # 仓内侧（__file__ 推导，root 钉死本工作树）
    ap.add_argument("--run-sh", default=str(REPO / "ci" / "run.sh"))
    ap.add_argument("--readme", default=str(REPO / "af" / "packages" / "README.md"))
    ap.add_argument("--components", default=str(REPO / "af" / "packages" / "components.json"))
    ap.add_argument("--expected-fail", default=str(REPO / "ci" / "neutrality-expected-fail.txt"))
    # 工作区侧文书（住工作区，默认绝对路径、可覆写）
    ap.add_argument("--agent-md", default="/home/pan-ding/workspace/agent黑灯工厂/AGENT.md")
    ap.add_argument("--plan-doc", default="/home/pan-ding/workspace/agent黑灯工厂/计划/研究组/G4-任务中立性机器检查方案.md")
    ap.add_argument("--doc-readme", default="/home/pan-ding/workspace/agent黑灯工厂/README.md")
    ap.add_argument("--doc-zong", default="/home/pan-ding/workspace/agent黑灯工厂/计划/00-总纲.md")
    ap.add_argument("--doc-status", default="/home/pan-ding/workspace/agent黑灯工厂/状态/组件建设状态.md")
    ap.add_argument("--doc-runbook", default="/home/pan-ding/workspace/agent黑灯工厂/计划/研究组/旧bench退役归档-runbook.md")
    ap.add_argument("--out", default=None, help="对账明细输出路径（缺省 <repo>/ci/out/xcheck-report.txt）")
    args = ap.parse_args()

    check_gates(args.agent_md, args.run_sh)
    check_packages(args.readme, args.components)
    check_redlist(args.plan_doc, args.expected_fail)
    check_obs_dates({"README": args.doc_readme, "00-总纲": args.doc_zong,
                     "组件建设状态": args.doc_status, "runbook§六-3": args.doc_runbook})
    check_runsh_header(args.run_sh)
    register_codepoints_legacy()

    hard_red = [x for x in HARD if not x[1]]
    soft_red = [x for x in SOFT if not x[1]]
    for name, ok, detail in HARD + SOFT:
        print(f"[{'PASS' if ok else 'FAIL'}] {name}")
        for line in detail:
            print(f"       {line}")
    for line in INFO_LINES:
        print(line)
    print(f"== xcheck: 硬项 {len(HARD) - len(hard_red)}/{len(HARD)} 过；advisory {len(SOFT) - len(soft_red)}/{len(SOFT)} 过"
          f"（覆盖面 3/4：门数/packages/词表；码位组遗留登记见 [LEGACY] 行）")
    out_path = Path(args.out) if args.out else REPO / "ci" / "out" / "xcheck-report.txt"
    try:
        out_path.parent.mkdir(parents=True, exist_ok=True)
        body = ["# xcheck 对账明细（ci/xcheck.py 机生成）", ""]
        for name, ok, detail in HARD + SOFT:
            body.append(f"[{'PASS' if ok else 'FAIL'}] {name}")
            body.extend(f"       {ln}" for ln in detail)
        body.extend(INFO_LINES)
        body.append(f"== 硬项 {len(HARD) - len(hard_red)}/{len(HARD)} 过；advisory {len(SOFT) - len(soft_red)}/{len(SOFT)} 过")
        out_path.write_text("\n".join(body) + "\n", encoding="utf-8")
        print(f"== 对账明细落档：{out_path}")
    except OSError as e:
        print(f"[WARN] 对账明细落档失败（不影响判定）：{e}")
    if hard_red:
        return 1
    if soft_red:
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
