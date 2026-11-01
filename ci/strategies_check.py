#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""strategies_check.py — strategies/ 共识策略库全量校验（libA-20261029；step14 段尾 14d）

挂点：ci/run.sh step14 机读一致性批段尾（14d；不新增 step 槽——门数账纪律：
numbers.json gates.ci_steps/ci.steps_total=14、gates.ci_hard=13 与 xcheck 门数对账
均不因此改动，符合「禁止堆门」与 G0 批共宿槽先例）。

校验面（对应工单「schema+compilable 条目逐一编译断言+计数与 index 一致」）：
  R1 schema：strategies/ 下每个条目 YAML（st-*.yaml，index.yaml 除外）逐字段
     校验——必填键、枚举闭集（tier∈{A,B}；dsl_state 三态；experiment.status
     五态）、三态↔字段互斥规则（compilable⇒dsl_snippet；needs_syntax⇒
     needs_syntax_note；not_applicable⇒na_reason，且互斥字段必须缺席/为 "-"）、
     id↔文件名 stem 一致。格式契约两线一致，故对全库条目（含 B 线 st-b-* 命名
     空间在飞文件）同尺校验。
  R2 index 完备性（硬对账）：磁盘上每个条目 YAML 必须登记进 strategies/index.yaml
     （并集登记簿：甲 A 档 11 + 乙 B 档 48；乙线 libB 提交 243fa0f 明示
     index/README/CI 归甲线合并——本检查即该合并面的守护），index 登记的每个
     file 必须在场。
  R3 计数对账：index 登记值 counts.{total,tiers,dsl_states} 与按 index 条目
     （含任何已登记的 B 线条目）机算值逐一比对；index 条目的 tier/dsl_state
     还须与条目文件实测值一致（登记面↔内容面双向）。
  R4 编译断言：compilable 条目的 dsl_snippet 须过 af-dsl 全链——经既有测试
     装载路径 `cargo test -p af-dsl --test strategy_library_test`（该测试对
     strategies/ 全库条目逐一 load_str+validate，compilable 零违例；沙箱无
     strategies/ 时如实 SKIP——中立性门隔离臂失败集契约不受影响）。退出码
     非零即本门红。

退出码：0=全过；1=任一硬红（schema/index/计数/编译断言）。
路径纪律（红队 P2-2 root 钉死）：仓内侧默认值一律由 __file__ 推导到本脚本
所在仓库根，不对旧分支局部副本跑。
pyc 纪律：本脚本只 import 标准库与 site-packages（yaml），不在 ci/ 下产生
__pycache__；run.sh 挂点仍以 python3 -B 调用（pyc-shadow 族防线双保险）。
"""

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path

try:
    import yaml  # type: ignore
except ImportError:
    print("[strategies] FATAL: 需要 PyYAML（python3 -c 'import yaml'）", file=sys.stderr)
    sys.exit(2)

REPO = Path(__file__).resolve().parent.parent
STRATS = REPO / "strategies"
INDEX = STRATS / "index.yaml"

# 2026-10-30 v1.2 契约修订（SIL 全周期收口裁决）：tier 闭集扩 A|B|C|D——C/D 为
# 「实验承载档」，仅当 experiment.status != none 时允许入库：自我改进循环把实验
# 终裁自动建条目回写（实测实验主断言卡常落 tier C 单卡簇，v1 闭集导致真实裁决回写被回滚）。
TIERS = {"A", "B", "C", "D"}
TIERS_NEED_EXPERIMENT = {"C", "D"}
DSL_STATES = {"compilable", "needs_syntax", "not_applicable"}
EXP_STATUS = {"none", "inconclusive", "confirmed", "refuted", "pending"}
ID_RE = re.compile(r"^st-(b-)?C[0-9]+$")
# B 线命名空间前缀（2026-10-29 实测在飞约定：B 线批次以 st-b-<簇id> 投影 B 档簇）。
B_NS_PREFIX = "st-b-"

errors: list[str] = []
infos: list[str] = []


def err(msg: str) -> None:
    errors.append(msg)


def info(msg: str) -> None:
    infos.append(msg)


def nonempty_str(v) -> bool:
    return isinstance(v, str) and bool(v.strip())


def dash_or_empty(v) -> bool:
    """互斥字段合规形态：缺席、None、或 "-"/空白。"""
    return v is None or (isinstance(v, str) and v.strip() in ("", "-"))


# ---------------------------------------------------------------- R1 schema
def check_entry_schema(path: Path) -> str | None:
    """校验单条目；返回其 dsl_state（失败返回 None）。"""
    rel = path.relative_to(REPO)
    try:
        doc = yaml.safe_load(path.read_text(encoding="utf-8"))
    except yaml.YAMLError as e:
        err(f"R1 {rel}: YAML 解析失败：{e}")
        return None
    if not isinstance(doc, dict):
        err(f"R1 {rel}: 顶层须为 mapping")
        return None

    for key in ("id", "name", "tier", "claim", "claimed_value", "sources",
                "consensus", "experiment", "dsl_state"):
        if key not in doc:
            err(f"R1 {rel}: 缺必填键 {key}")

    eid = doc.get("id")
    if not nonempty_str(eid):
        err(f"R1 {rel}: id 须为非空字符串")
    else:
        if not ID_RE.match(eid):
            err(f"R1 {rel}: id={eid!r} 不符 ^st-(b-)?C[0-9]+$")
        if path.stem != eid:
            err(f"R1 {rel}: id 与文件名 stem 不一致（{eid!r} vs {path.stem!r}）")

    if not nonempty_str(doc.get("name")):
        err(f"R1 {rel}: name 须为非空字符串")
    if doc.get("tier") not in TIERS:
        err(f"R1 {rel}: tier={doc.get('tier')!r} 不在闭集 {sorted(TIERS)}")
    elif doc.get("tier") in TIERS_NEED_EXPERIMENT and doc.get("experiment", {}).get("status") in (None, "none"):
        err(f"R1 {rel}: tier={doc.get('tier')} 为实验承载档，experiment.status 不得为空/none")
    if not nonempty_str(doc.get("claim")):
        err(f"R1 {rel}: claim 须为非空字符串（策略册一句话断言）")

    cv = doc.get("claimed_value")
    if not isinstance(cv, dict):
        err(f"R1 {rel}: claimed_value 须为 mapping")
    else:
        if not nonempty_str(cv.get("metric")):
            err(f"R1 {rel}: claimed_value.metric 须为非空字符串")
        if cv.get("direction") not in ("up", "down"):
            err(f"R1 {rel}: claimed_value.direction={cv.get('direction')!r} 不在 up|down")
        if not nonempty_str(cv.get("context")):
            err(f"R1 {rel}: claimed_value.context（适用边界）须为非空字符串")

    srcs = doc.get("sources")
    if not (isinstance(srcs, list) and srcs and all(nonempty_str(s) for s in srcs)):
        err(f"R1 {rel}: sources 须为非空字符串列表（支撑卡 id）")

    cons = doc.get("consensus")
    if not isinstance(cons, dict):
        err(f"R1 {rel}: consensus 须为 mapping")
    else:
        for key in ("corpora", "cards"):
            v = cons.get(key)
            if not (isinstance(v, int) and not isinstance(v, bool) and v >= 1):
                err(f"R1 {rel}: consensus.{key}={v!r} 须为 ≥1 的整数")

    exp = doc.get("experiment")
    if not isinstance(exp, dict):
        err(f"R1 {rel}: experiment 须为 mapping")
    else:
        st = exp.get("status")
        if st not in EXP_STATUS:
            err(f"R1 {rel}: experiment.status={st!r} 不在闭集 {sorted(EXP_STATUS)}")
        eid_ = exp.get("exp_id")
        if st == "none":
            if eid_ != "-":
                err(f"R1 {rel}: status=none 时 exp_id 须为 '-'（实得 {eid_!r}）")
        else:
            if not nonempty_str(eid_) or eid_ == "-":
                err(f"R1 {rel}: status={st!r} 时 exp_id 须为非 '-' 的实验编号")
        if not isinstance(exp.get("note"), str):
            err(f"R1 {rel}: experiment.note 须为字符串（无注记用 '-'）")

    state = doc.get("dsl_state")
    if state not in DSL_STATES:
        err(f"R1 {rel}: dsl_state={state!r} 不在三态闭集 {sorted(DSL_STATES)}")
        return None
    # 三态↔字段互斥（R1 与 Rust 装载测试同尺；两面互备）
    if state == "compilable":
        if not nonempty_str(doc.get("dsl_snippet")):
            err(f"R1 {rel}: dsl_state=compilable 但 dsl_snippet 缺席/为空")
        if not dash_or_empty(doc.get("needs_syntax_note")):
            err(f"R1 {rel}: compilable 条目不得带 needs_syntax_note")
        if not dash_or_empty(doc.get("na_reason")):
            err(f"R1 {rel}: compilable 条目不得带 na_reason")
    elif state == "needs_syntax":
        if not nonempty_str(doc.get("needs_syntax_note")):
            err(f"R1 {rel}: dsl_state=needs_syntax 但 needs_syntax_note 缺席/为空（缺什么语法+示例）")
        if not dash_or_empty(doc.get("dsl_snippet")):
            err(f"R1 {rel}: needs_syntax 条目不得带 dsl_snippet")
        if not dash_or_empty(doc.get("na_reason")):
            err(f"R1 {rel}: needs_syntax 条目不得带 na_reason")
    else:  # not_applicable
        if not nonempty_str(doc.get("na_reason")):
            err(f"R1 {rel}: dsl_state=not_applicable 但 na_reason 缺席/为空（为何不适用）")
        if not dash_or_empty(doc.get("dsl_snippet")):
            err(f"R1 {rel}: not_applicable 条目不得带 dsl_snippet")
        if not dash_or_empty(doc.get("needs_syntax_note")):
            err(f"R1 {rel}: not_applicable 条目不得带 needs_syntax_note")
    return state


# ---------------------------------------------------------------- R2/R3 index
def check_index(disk_states: dict[str, str], disk_tiers: dict[str, str]) -> None:
    try:
        idx = yaml.safe_load(INDEX.read_text(encoding="utf-8"))
    except yaml.YAMLError as e:
        err(f"R2 strategies/index.yaml: YAML 解析失败：{e}")
        return
    if not isinstance(idx, dict):
        err("R2 strategies/index.yaml: 顶层须为 mapping")
        return
    entries = idx.get("entries")
    if not isinstance(entries, list) or not entries:
        err("R2 strategies/index.yaml: entries 须为非空列表")
        return

    listed: set[str] = set()
    for i, e in enumerate(entries):
        if not isinstance(e, dict):
            err(f"R2 index entries[{i}]: 须为 mapping")
            continue
        eid = e.get("id")
        f = e.get("file")
        if not nonempty_str(eid) or not nonempty_str(f):
            err(f"R2 index entries[{i}]: id/file 须为非空字符串")
            continue
        listed.add(eid)
        p = STRATS / f
        if not p.is_file():
            err(f"R2 index 条目 {eid}: file={f} 不在场（{p}）")
            continue
        # 登记面↔内容面：tier/dsl_state 须与条目文件实测一致
        if eid in disk_states:
            if e.get("dsl_state") != disk_states[eid]:
                err(f"R2 index 条目 {eid}: dsl_state 登记 {e.get('dsl_state')!r} ≠ 文件实测 {disk_states[eid]!r}")
            if e.get("tier") != disk_tiers.get(eid):
                err(f"R2 index 条目 {eid}: tier 登记 {e.get('tier')!r} ≠ 文件实测 {disk_tiers.get(eid)!r}")

    # 全条目完备性（硬）：磁盘每个条目必须登记（并集登记簿，甲 A 档 + 乙 B 档）。
    a_ns = [e for e in sorted(disk_states) if not e.startswith(B_NS_PREFIX)]
    b_ns = [e for e in sorted(disk_states) if e.startswith(B_NS_PREFIX)]
    for eid in a_ns + b_ns:
        if eid not in listed:
            err(f"R2 磁盘条目 {eid} 未登记进 index——全条目契约（甲 A 档 {len(a_ns)} + 乙 B 档 {len(b_ns)}）")
    info(f"命名空间分解：A 线（st-C*）{len(a_ns)} 条，B 线（st-b-*）{len(b_ns)} 条，"
         f"全部登记={all(e in listed for e in a_ns + b_ns)}")

    # R3 计数对账：登记值 vs 按 index 条目机算。
    # 口径：非零机算键必须登记且逐键相等；登记键允许显式 0（三态计数全列示时
    # 的零值键），但任何键的登记值≠机算值（缺键按 0）即红。
    counts = idx.get("counts")
    if not isinstance(counts, dict):
        err("R3 index.counts 须为 mapping（total/tiers/dsl_states）")
        return
    calc_total = len(listed)
    calc_tiers: dict[str, int] = {}
    calc_states: dict[str, int] = {}
    for e in entries:
        if not isinstance(e, dict):
            continue
        t, s = e.get("tier"), e.get("dsl_state")
        calc_tiers[t] = calc_tiers.get(t, 0) + 1
        calc_states[s] = calc_states.get(s, 0) + 1

    def cmp_counts(label: str, registered, computed: dict) -> None:
        if not isinstance(registered, dict):
            err(f"R3 counts.{label} 登记 {registered!r} 须为 mapping")
            return
        for k, v in registered.items():
            if not (isinstance(v, int) and not isinstance(v, bool)):
                err(f"R3 counts.{label}.{k} 登记 {v!r} 须为整数")
            elif computed.get(k, 0) != v:
                err(f"R3 counts.{label}.{k} 登记 {v!r} ≠ 机算 {computed.get(k, 0)}")
        for k, v in computed.items():
            if v > 0 and k not in registered:
                err(f"R3 counts.{label} 缺非零键 {k!r}（机算 {v}）——登记面缩水")

    reg_total = counts.get("total")
    if reg_total != calc_total:
        err(f"R3 counts.total 登记 {reg_total!r} ≠ 机算 {calc_total}（index 条目数）")
    cmp_counts("tiers", counts.get("tiers"), calc_tiers)
    cmp_counts("dsl_states", counts.get("dsl_states"), calc_states)
    if not errors:
        info(f"index 计数对账 OK：total={calc_total} tiers={calc_tiers} dsl_states={calc_states}")


# ---------------------------------------------------------------- R4 编译断言
def provenance_ensure() -> "str | None":
    """R4 二次 cargo 调用前的本树 provenance 门（2026-11-01 step14 跨工作树假绿
    实证修复）：共享 CARGO_TARGET_DIR 残留他树测试二进制时，cargo freshness
    退化为 mtime 比较，原样执行外来产物即报 exit 0——假绿（红队终局轮实测：
    已删树 af-wt-upd 的 strategy_library_test 二进制被本检查执行，本树坏
    dsl_snippet 零验证）。复用 step3 同门 ci/provenance_check.sh ensure
    （判脏即清+盖章，语义与 run.sh:171 完全一致）；TARGET 缺省由该脚本取
    环境 CARGO_TARGET_DIR，与紧随其后的 cargo 调用同一定位。返回 None=过；
    非 None=诊断串（调用方转硬红——绝不带脏 target 跑 cargo）。"""
    script = REPO / "ci" / "provenance_check.sh"
    if not script.is_file():
        return f"provenance 门脚本缺席：{script}"
    try:
        proc = subprocess.run(
            ["bash", str(script), "ensure", "--root", str(REPO)],
            cwd=str(REPO), stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, timeout=600,
        )
    except (OSError, subprocess.TimeoutExpired) as e:
        return f"provenance 门 ensure 调用失败：{e}"
    if proc.returncode != 0:
        tail = "\n".join((proc.stdout or "").strip().splitlines()[-10:])
        return (f"provenance 门 ensure 红（exit={proc.returncode}；判脏即清失败，"
                f"带脏 target 不跑 cargo）：\n{tail}")
    return None


def check_compile() -> None:
    if shutil.which("cargo") is None:
        err("R4 cargo 不在 PATH——编译断言无法执行（fail-loud，不静默跳过）")
        return
    pv = provenance_ensure()
    if pv is not None:
        err(f"R4 编译断言前 provenance 门红——{pv}")
        return
    cmd = ["cargo", "test", "-p", "af-dsl", "--test", "strategy_library_test"]
    try:
        proc = subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, timeout=900)
    except subprocess.TimeoutExpired:
        err(f"R4 编译断言超时（900s）：{' '.join(cmd)}")
        return
    out = (proc.stdout or "") + (proc.stderr or "")
    if proc.returncode != 0:
        tail = "\n".join(out.strip().splitlines()[-30:])
        err(f"R4 编译断言失败（exit={proc.returncode}）：{' '.join(cmd)}\n{tail}")
        return
    # 从输出摘录三态汇总行（缺行=装载测试改形，门照过但留 INFO 痕迹）
    for line in out.splitlines():
        if "strategy-library] 条目" in line:
            info(f"编译断言 OK：{line.split('] ', 1)[-1].strip()}")


def main() -> int:
    ap = argparse.ArgumentParser(description="strategies/ 策略库全量校验（schema+index 计数+编译断言）")
    ap.add_argument("--skip-compile", action="store_true",
                    help="跳过 R4  cargo 编译断言（仅 schema/index 面；CI 常跑不用）")
    args = ap.parse_args()

    if not STRATS.is_dir():
        err(f"strategies/ 缺席（{STRATS}）——缺文件=未达成（复盘 C7 口径）")
        print("[FAIL] strategies/ 目录不存在")
        return 1
    if not INDEX.is_file():
        err(f"strategies/index.yaml 缺席（{INDEX}）——缺文件=未达成（复盘 C7 口径）")

    files = sorted(p for p in STRATS.glob("st-*.yaml") if p.is_file())
    if not files:
        err("strategies/ 无条目 YAML——库为空（空库=覆盖面缩水）")

    disk_states: dict[str, str] = {}
    disk_tiers: dict[str, str] = {}
    for p in files:
        state = check_entry_schema(p)
        if state is not None:
            disk_states[p.stem] = state
            d = yaml.safe_load(p.read_text(encoding="utf-8"))
            disk_tiers[p.stem] = d.get("tier")

    if INDEX.is_file():
        check_index(disk_states, disk_tiers)

    if not args.skip_compile:
        # 编译断言常跑（schema/index 面已红时也跑——一次拿全量诊断，承 run.sh
        # 「所有步骤仍会跑完」管道哲学；装载测试对全库条目逐一断言）。
        check_compile()
    else:
        info("R4 编译断言已按 --skip-compile 跳过（非常跑形态）")

    for line in infos:
        print(f"[INFO] {line}")
    if errors:
        for line in errors:
            print(f"[FAIL] {line}")
        print(f"== strategies_check: 红 {len(errors)} 项（schema/index 计数/编译断言）")
        return 1
    n_compile = sum(1 for s in disk_states.values() if s == "compilable")
    print(f"== strategies_check: 全绿——磁盘条目 {len(disk_states)}（compilable={n_compile}），"
          f"index 计数对账一致，编译断言经 af-dsl 装载测试")
    return 0


if __name__ == "__main__":
    sys.exit(main())
