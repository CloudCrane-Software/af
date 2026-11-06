#!/usr/bin/python3
# -*- coding: utf-8 -*-
"""harness_regression_gate.py — harness 版本级回归门（卡 s17-harness-regression-gate，债 D-54）

跨面落地件（设计稿 30-工件/strategies17-供给面设计-20261106.md §七.1）：st-b-C108 的
被防护对象是 harness **工程过程**（版本回归集+发布阻断阈值）而非任何一次 agent 运行
图——AFB 是单次运行蓝图、其 policies 只约束本图执行——故回归门落 CI 发布纪律面
（制度 §二.1 anolis-build 验收面）。本脚本即该面在役门，挂 ci/run.sh step14 共宿槽
段尾（14e；不新增 step 槽——门数账纪律：numbers.json gates.ci_steps/ci.steps_total=14、
gates.ci_hard=13 零改动，与 14d strategies_check 挂点同型）。

口径来源（S130，harness策略调研/strategies.csv#line:131，body.测试设计原文）：
「每次改动都跑固定回归集（同一批大仓库任务 + 固定模型 + 固定 seed），同时报 resolved
率与 token/题两条曲线，任一劣化超过阈值即阻断发布」。该文实测：Qwen Code CLI 35 个
连续 release 的 resolve rate 在 23.0%–39.0% 间大幅波动（均值 30.5%）且与版本序无
显著趋势（Spearman ρ=0.208, p=0.231）；每版双跑任务级一致 87.7% ⇒ 波动归因 harness
改动而非采样噪声；实录回归 v0.4.1→v0.5.0（OpenAI converter，PR #1235，+346/−8 行）
resolve rate 39.4%→32.5%——「这些回归没有任何现有自动化测试能捕获」。本门即补该缺口：
把回归集口径钉死、把劣化阈值变成发布阻断。

两个门面（均 fail-closed；退出码 0=全绿，1=硬红阻断，2 仅 argparse 用法错）：
  定义面（恒验）：回归集定义件（缺省 ci/baselines/harness-regression-set.json，
    __file__ 推导 root 钉死）必填字段全部钉死——模型/seed/每版跑数/任务集
    （路径+计数+sha256）/双阈值/resolve+token/基线策略/校准锚；任务集文件须在场、
    条目数==钉计数、sha256==钉摘要、条目结构合法且 instance_id 唯一；
    **空回归集（零条目）＝无判据 ⇒ 硬红**；校准锚（S130 实录回归 39.4%→32.5%，
    Δ−6.9pp）必须超过 resolve 阈值——阈值放宽到连实录回归都拦不住即硬红（防弱化）。
  读数面（有数才判）：发布管线把每个 harness release 的读数追加进定义件钉的
    readings_path（JSONL，append-only）：{release,model,seed,runs,resolved,total,
    tokens_per_task_k}。逐条核对口径一致（模型/seed/每版跑数/任务集全覆盖——
    total 必须==钉计数，子集跑分不得过门；release 不得重号）；基线=首条记录，
    当前=末条记录；resolve 率劣化 > resolve_degradation_pp，或 token/题通胀 >
    token_degradation_pct ⇒ 硬红阻断发布（S130 原文「任一劣化超过阈值即阻断」）。
    读数文件缺席：默认模式记 INFO（发布管线首跑前属正常态，定义面仍在役），
    --require-readings 下缺席即硬红（发布管线专用）；在场但零记录 ⇒ 恒硬红。

selftest（--selftest，随 step14 14e 每次 CI 实跑）：负例夹具全套——空回归集红、
任务集计数/摘要漂移红、定义缺字段红、阈值弱化红、注入 resolve 劣化红（带阻断读数）、
注入 token 劣化红、读数口径漂移红、子集跑分红、记录不可解析红、release 重号红、
空读数台账红、--require-readings 缺席红、读数缺席默认不拦、S130 实录回归必拦+
窗口内波动不拦（阈值校准）、真仓定义件 rc=0 在役。夹具全部落临时目录（tempfile），
不触真仓任何树；每夹具以子进程实跑 CLI，断言退出码与阻断读数文案。

路径纪律（红队 P2-2 root 钉死）：缺省定义件由 __file__ 推导；--definition 显式覆盖
只服务于 selftest 夹具/发布管线指定正本；任务集与读数路径相对**定义件所在目录**
解析（sibling 口径）。pyc 纪律：只 import 标准库，ci/ 下不产生 __pycache__
（run.sh 挂点以 python3 -B 调用）。
"""

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

GATE = Path(__file__).resolve()
REPO = GATE.parent.parent
DEFAULT_DEFINITION = GATE.parent / "baselines" / "harness-regression-set.json"
SCHEMA = "af.harness-regression-set/1"
BASELINE_POLICY = "first_complete_run"
HEX64_RE = re.compile(r"^[0-9a-f]{64}$")
TAG = "[harness-gate]"

RC_PASS = 0
RC_BLOCK = 1

_LOG = {"errors": [], "infos": []}


def err(msg: str) -> None:
    _LOG["errors"].append(msg)
    print(f"{TAG} RED {msg}")


def info(msg: str) -> None:
    _LOG["infos"].append(msg)
    print(f"{TAG} INFO {msg}")


def ok(msg: str) -> None:
    print(f"{TAG} OK {msg}")


def verdict(rc: int, extra: str) -> None:
    if rc == RC_PASS:
        print(f"== harness_regression_gate: 全绿——{extra}")
    else:
        n = len(_LOG["errors"])
        print(f"== harness_regression_gate: 硬红 {n} 项——阻断（{extra}）")


def sha256_file(p: Path) -> str:
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def _is_int(v) -> bool:
    return isinstance(v, int) and not isinstance(v, bool)


def _is_num(v) -> bool:
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _nonempty_str(v) -> bool:
    return isinstance(v, str) and bool(v.strip())


# ---------------------------------------------------------------- 定义面
def check_definition(definition_path: Path):
    """校验定义件+任务集。返回 (definition|None, definition_dir|None, errors)。
    definition None = 定义件缺席/不可解析/根类型错（定义面硬红，读数面不判）。"""
    errors: list[str] = []
    if not definition_path.is_file():
        errors.append(f"定义件缺席：{definition_path}——回归集口径无正本，fail-closed")
        return None, None, errors
    try:
        d = json.loads(definition_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as e:
        errors.append(f"定义件不可解析：{e}")
        return None, None, errors
    if not isinstance(d, dict):
        errors.append("定义件根类型非 JSON 对象")
        return None, None, errors

    if d.get("schema") != SCHEMA:
        errors.append(f"定义件 schema={d.get('schema')!r} != 钉值 {SCHEMA!r}——口径版本漂移")

    if not _nonempty_str(d.get("model")):
        errors.append("定义缺必填字段或为空：model（固定模型钉）")
    if not _is_int(d.get("seed")) or d.get("seed", -1) < 0:
        errors.append(f"seed 须为非负整数（实 {d.get('seed')!r}）")
    if not _is_int(d.get("runs_per_release")) or d.get("runs_per_release", 0) < 1:
        errors.append(f"runs_per_release 须为 ≥1 整数（实 {d.get('runs_per_release')!r}）")

    # —— 任务集（含空回归集 fail-closed 与摘要钉）——
    task_count = None
    ts = d.get("task_set")
    if not isinstance(ts, dict):
        errors.append("定义缺必填字段或类型错：task_set{path,count,sha256}")
    else:
        rel, count, digest = ts.get("path"), ts.get("count"), ts.get("sha256")
        if not _nonempty_str(rel):
            errors.append("task_set.path 须为非空串（相对定义件目录）")
        if not _is_int(count) or count <= 0:
            errors.append(f"task_set.count 须为 >0 整数（实 {count!r}）——零条目回归集无判据")
        else:
            task_count = count
        if not isinstance(digest, str) or not HEX64_RE.match(digest):
            errors.append(f"task_set.sha256 须为 64 位小写十六进制（实 {digest!r}）")
        if _nonempty_str(rel):
            ts_path = (definition_path.parent / rel).resolve()
            if not ts_path.is_file():
                errors.append(f"任务集缺席：{ts_path}——回归集正本不在场，fail-closed")
            else:
                try:
                    entries = json.loads(ts_path.read_text(encoding="utf-8"))
                except (OSError, json.JSONDecodeError) as e:
                    entries = None
                    errors.append(f"任务集不可解析：{e}")
                if entries is not None:
                    if not isinstance(entries, list):
                        errors.append("任务集根类型非 JSON 数组")
                    else:
                        if len(entries) == 0:
                            errors.append(
                                "空回归集：任务集在场但零条目——回归集未定义即无判据，fail-closed 阻断")
                        if task_count is not None and len(entries) != task_count:
                            errors.append(
                                f"任务集计数漂移：钉 {task_count} ≠ 实 {len(entries)}")
                        actual = sha256_file(ts_path)
                        if isinstance(digest, str) and actual != digest:
                            errors.append(
                                f"任务集摘要漂移：钉 {digest[:12]}… ≠ 实算 {actual[:12]}…——回归集被静默替换")
                        bad = [e for e in entries if not isinstance(e, dict)]
                        if bad:
                            errors.append(f"任务集 {len(bad)} 个条目非 JSON 对象")
                        dicts = [e for e in entries if isinstance(e, dict)]
                        ids = [e.get("instance_id") for e in dicts]
                        if any(not _nonempty_str(i) for i in ids):
                            errors.append("任务集有条目缺 instance_id（非空串）")
                        else:
                            dup = sorted({i for i in ids if ids.count(i) > 1})
                            if dup:
                                errors.append(f"任务集 instance_id 重复：{dup[:3]}…")
                        for k in ("repo", "tier"):
                            if any(not _nonempty_str(e.get(k)) for e in dicts):
                                errors.append(f"任务集有条目缺 {k}（非空串）")

    # —— 双阈值 ——
    resolve_pp = token_pct = None
    th = d.get("thresholds")
    if not isinstance(th, dict):
        errors.append("定义缺必填字段或类型错：thresholds{resolve_degradation_pp,token_degradation_pct}")
    else:
        resolve_pp, token_pct = th.get("resolve_degradation_pp"), th.get("token_degradation_pct")
        if not _is_num(resolve_pp) or resolve_pp <= 0:
            errors.append(f"thresholds.resolve_degradation_pp 须为 >0 数值（实 {resolve_pp!r}）")
        if not _is_num(token_pct) or token_pct <= 0:
            errors.append(f"thresholds.token_degradation_pct 须为 >0 数值（实 {token_pct!r}）")

    if d.get("baseline_policy") != BASELINE_POLICY:
        errors.append(
            f"baseline_policy 须为 {BASELINE_POLICY!r}（实 {d.get('baseline_policy')!r}）"
            f"——本门只实现该基线策略（append-only 台账首条为基线）")

    if not _nonempty_str(d.get("readings_path")):
        errors.append("定义缺必填字段或为空：readings_path（读数台账，相对定义件目录）")

    # —— 校准锚（S130 实录回归）+ 防阈值弱化 ——
    cal = d.get("calibration")
    if not isinstance(cal, dict):
        errors.append("定义缺必填字段：calibration（S130 实录回归锚）")
    else:
        fr, to, src = cal.get("from_rate"), cal.get("to_rate"), cal.get("source")
        if not (_is_num(fr) and _is_num(to)):
            errors.append("calibration.from_rate/to_rate 须为数值")
        else:
            if fr <= to:
                errors.append(
                    f"校准锚方向错：from_rate {fr} ≤ to_rate {to}（实录回归必须 from>to）")
            elif _is_num(resolve_pp) and (fr - to) <= resolve_pp:
                errors.append(
                    f"阈值弱化：S130 实录回归 Δ{fr - to:.1f}pp ≤ 阈值 {resolve_pp}pp"
                    f"——连真实回归都拦不住，禁")
        if not _nonempty_str(src):
            errors.append("calibration.source 须为非空串（实录回归出处）")

    return d, definition_path.parent, errors


# ---------------------------------------------------------------- 读数面
def check_readings(readings_path: Path, d: dict, task_count: int) -> list[str]:
    """读数台账逐条口径核对 + 阈值阻断判定。返回 errors（空=读数面绿）。"""
    errors: list[str] = []
    try:
        text = readings_path.read_text(encoding="utf-8")
    except OSError as e:
        return [f"读数台账不可读：{e}"]
    lines = [ln for ln in text.splitlines() if ln.strip()]
    if not lines:
        errors.append(
            f"读数台账为空：{readings_path.name} 在场但零记录——无读数即无判据，fail-closed")
        return errors

    model, seed, runs = d["model"], d["seed"], d["runs_per_release"]
    resolve_pp = d["thresholds"]["resolve_degradation_pp"]
    token_pct = d["thresholds"]["token_degradation_pct"]

    records: list[tuple[int, dict]] = []
    for i, ln in enumerate(lines, 1):
        try:
            rec = json.loads(ln)
        except json.JSONDecodeError as e:
            errors.append(f"读数记录 #{i} 不可解析：{e}")
            continue
        if not isinstance(rec, dict):
            errors.append(f"读数记录 #{i} 根类型非 JSON 对象")
            continue
        records.append((i, rec))
    if not records:
        return errors

    seen: dict[str, int] = {}
    for i, rec in records:
        rel = rec.get("release")
        if not _nonempty_str(rel):
            errors.append(f"读数记录 #{i} 缺 release（harness 版本标识）")
        else:
            if rel in seen:
                errors.append(
                    f"读数记录 #{i} release 重号：{rel}（#{seen[rel]} 已记）——append-only 台账不得重号")
            seen[rel] = i
        if rec.get("model") != model:
            errors.append(
                f"读数口径漂移：记录 #{i} model={rec.get('model')!r} ≠ 钉模型 {model!r}")
        if rec.get("seed") != seed:
            errors.append(
                f"读数口径漂移：记录 #{i} seed={rec.get('seed')!r} ≠ 钉 seed {seed!r}")
        if rec.get("runs") != runs:
            errors.append(
                f"读数口径漂移：记录 #{i} runs={rec.get('runs')!r} ≠ 钉每版跑数 {runs!r}")
        resolved, total = rec.get("resolved"), rec.get("total")
        if not _is_int(resolved) or resolved < 0:
            errors.append(f"读数记录 #{i} resolved 须为非负整数（实 {resolved!r}）")
        if not _is_int(total) or total <= 0:
            errors.append(f"读数记录 #{i} total 须为 >0 整数（实 {total!r}）")
        elif _is_int(resolved) and resolved > total:
            errors.append(f"读数记录 #{i} resolved {resolved} > total {total}")
        elif total != task_count:
            errors.append(
                f"回归集未全覆盖：记录 #{i} total={total} ≠ 钉任务数 {task_count}——子集跑分不得过门")
        tpk = rec.get("tokens_per_task_k")
        if tpk is not None and (not _is_num(tpk) or tpk <= 0):
            errors.append(f"读数记录 #{i} tokens_per_task_k 须为 >0 数值（实 {tpk!r}）")

    if errors:
        return errors  # 口径已红，不再叠判劣化（诊断一次拿全）

    if len(records) == 1:
        only = records[0][1]
        info(f"读数面：单记录 {only.get('release')}——基线即当前，无跨版本比较面")
        return errors

    base_i, base = records[0]
    cur_i, cur = records[-1]
    base_rate = base["resolved"] / base["total"]
    cur_rate = cur["resolved"] / cur["total"]
    delta_pp = (base_rate - cur_rate) * 100
    if delta_pp > resolve_pp:
        errors.append(
            f"劣化阻断：resolve rate 基线 {base_rate * 100:.1f}%"
            f"（#{base_i} {base.get('release')}）→ 当前 {cur_rate * 100:.1f}%"
            f"（#{cur_i} {cur.get('release')}），劣化 {delta_pp:.1f}pp > 阈值 {resolve_pp}pp"
            f"——S130 口径「任一劣化超过阈值即阻断发布」")
    b_t, c_t = base.get("tokens_per_task_k"), cur.get("tokens_per_task_k")
    infl = None
    if _is_num(b_t) and _is_num(c_t) and b_t > 0:
        infl = (c_t - b_t) / b_t * 100
        if infl > token_pct:
            errors.append(
                f"劣化阻断：token/题 基线 {b_t:.1f}K → 当前 {c_t:.1f}K，通胀 {infl:.1f}% > "
                f"阈值 {token_pct}%——S130 口径「两条曲线任一劣化即阻断」")
    if not errors:
        infl_txt = f"{infl:.1f}%" if infl is not None else "未报（两条曲线之一缺席）"
        info(f"读数面：劣化 {delta_pp:.1f}pp ≤ 阈值 {resolve_pp}pp"
             f"（token/题通胀 {infl_txt} ≤ 阈值 {token_pct}%）——未触发阻断")
    return errors


# ---------------------------------------------------------------- 判定入口
def evaluate(definition_path: Path, readings_override: str | None = None,
             require_readings: bool = False) -> int:
    d, def_dir, derrs = check_definition(definition_path)
    if derrs or d is None or def_dir is None:
        for e in derrs:
            err(e)
        verdict(RC_BLOCK, "定义面硬红")
        return RC_BLOCK

    cal = d["calibration"]
    ok(f"定义面在役：模型 {d['model']} / seed {d['seed']} / 任务集 {d['task_set']['count']} 题"
       f" / 每版 {d['runs_per_release']} 跑 / 阈值 resolve {d['thresholds']['resolve_degradation_pp']}pp"
       f" + token {d['thresholds']['token_degradation_pct']}%")
    info(f"校准锚（S130 实录回归 {cal['case']}）：{cal['from_rate']}%→{cal['to_rate']}%"
         f"（Δ{cal['from_rate'] - cal['to_rate']:.1f}pp）必被本门捕获——阈值再放宽即红（防弱化）")

    if readings_override:
        rp = Path(readings_override)
        readings_path = rp if rp.is_absolute() else (def_dir / rp)
    else:
        readings_path = def_dir / d["readings_path"]

    if not readings_path.is_file():
        if require_readings:
            err(f"--require-readings：读数台账缺席 {readings_path}——发布管线必须首跑回归集后放行")
            verdict(RC_BLOCK, "读数面硬红")
            return RC_BLOCK
        info(f"读数面待首跑：{readings_path.name} 缺席（发布管线首跑后落账；"
             f"--require-readings 下缺席即红）")
        verdict(RC_PASS, "定义面在役，读数面待首跑")
        return RC_PASS

    rerrs = check_readings(readings_path, d, d["task_set"]["count"])
    for e in rerrs:
        err(e)
    if rerrs:
        verdict(RC_BLOCK, "读数面硬红")
        return RC_BLOCK
    verdict(RC_PASS, "定义面+读数面均在役，未触发劣化阈值")
    return RC_PASS


# ---------------------------------------------------------------- selftest 夹具
def _deep_merge(base: dict, over: dict) -> dict:
    out = json.loads(json.dumps(base))
    for k, v in over.items():
        if isinstance(v, dict) and isinstance(out.get(k), dict):
            out[k] = _deep_merge(out[k], v)
        else:
            out[k] = v
    return out


def _fixture_def(**over) -> dict:
    return _deep_merge({
        "schema": SCHEMA,
        "model": "fixture-model@v1",
        "seed": 20261029,
        "runs_per_release": 2,
        "task_set": {"path": "tasks.json", "count": 10, "sha256": None},
        "thresholds": {"resolve_degradation_pp": 5.0, "token_degradation_pct": 15.0},
        "baseline_policy": BASELINE_POLICY,
        "readings_path": "readings.jsonl",
        "calibration": {"case": "fixture 校准锚", "from_rate": 39.4, "to_rate": 32.5,
                        "expect": "block", "source": "fixture"},
    }, over)


def _write_tasks(tmp: Path, n: int, name: str = "tasks.json") -> Path:
    entries = [{"instance_id": f"fixture__t-{i:04d}", "repo": "fixture/repo", "tier": "D1"}
               for i in range(n)]
    p = tmp / name
    p.write_text(json.dumps(entries, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    return p


def _write_readings(tmp: Path, records: list[dict], name: str = "readings.jsonl") -> Path:
    p = tmp / name
    p.write_text("".join(json.dumps(r, ensure_ascii=False) + "\n" for r in records),
                 encoding="utf-8")
    return p


def _rec(release: str, resolved: int, total: int, **over) -> dict:
    rec = {"release": release, "model": "fixture-model@v1", "seed": 20261029,
           "runs": 2, "resolved": resolved, "total": total, "tokens_per_task_k": 400.0}
    rec.update(over)
    return rec


def _run_gate(argv: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, "-B", str(GATE)] + argv,
                          capture_output=True, text=True, timeout=120)


class _Fixture:
    """一个 selftest 夹具：临时目录 + 期望退出码 + 期望读数文案。"""

    def __init__(self, name: str, expect_rc: int, expects: list[str]):
        self.name = name
        self.expect_rc = expect_rc
        self.expects = expects
        self.argv: list[str] = []

    def build(self, tmp: Path) -> None:  # 由各夹具覆写
        raise NotImplementedError


def _def_with_tasks(tmp: Path, n: int, **over) -> tuple[Path, Path]:
    """写任务集 + 定义件（摘要自动钉），返回 (定义件路径, 任务集路径)。"""
    tp = _write_tasks(tmp, n)
    d = _fixture_def(task_set={"path": tp.name, "count": n,
                               "sha256": sha256_file(tp)}, **over)
    dp = tmp / "definition.json"
    dp.write_text(json.dumps(d, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    return dp, tp


def selftest() -> int:
    passed, failed = 0, 0
    tmp_root = Path(tempfile.mkdtemp(prefix="harness-gate-selftest-"))
    try:
        fixtures = _build_fixtures(tmp_root)
        print(f"{TAG} SELFTEST 负例夹具 {len(fixtures)} 枚（临时根 {tmp_root}）")
        for fx in fixtures:
            sub = tmp_root / re.sub(r"[^A-Za-z0-9_.-]+", "-", fx.name)
            sub.mkdir(parents=True, exist_ok=True)
            fx.build(sub)
            r = _run_gate(fx.argv)
            out = r.stdout + r.stderr
            missing = [s for s in fx.expects if s not in out]
            if r.returncode == fx.expect_rc and not missing:
                passed += 1
                print(f"{TAG} SELFTEST PASS  {fx.name}（rc={r.returncode}）")
            else:
                failed += 1
                print(f"{TAG} SELFTEST FAIL  {fx.name}：期望 rc={fx.expect_rc} 实 {r.returncode}"
                      f"；缺读数串 {missing}")
                for ln in out.splitlines():
                    print(f"{TAG}   | {ln}")
    finally:
        shutil.rmtree(tmp_root, ignore_errors=True)
    if failed:
        print(f"== harness_regression_gate: selftest 硬红——{failed}/{len(fixtures)} 枚夹具不符预期")
        return RC_BLOCK
    print(f"== harness_regression_gate: selftest 全绿——{passed} 枚夹具全部符预期"
          f"（含空回归集/注入劣化/口径漂移负例）")
    return RC_PASS


def _build_fixtures(tmp_root: Path) -> list[_Fixture]:
    fx: list[_Fixture] = []

    # —— 正例：定义面+读数面全绿（劣化 0.0pp ≤ 5.0pp；token 通胀 +2.5% ≤ 15%）——
    class PosFull(_Fixture):
        def build(self, tmp):
            dp, _ = _def_with_tasks(tmp, 10)
            _write_readings(tmp, [_rec("rel-a", 4, 10, tokens_per_task_k=400.0),
                                  _rec("rel-b", 4, 10, tokens_per_task_k=410.0)])
            self.argv = ["--definition", str(dp)]
    fx.append(PosFull("正例·定义面+读数面全绿（劣化 0.0pp）", RC_PASS,
                      ["全绿", "定义面在役", "读数面"]))

    # —— 正例：单记录（基线即当前，无跨版本比较面）——
    class PosSingle(_Fixture):
        def build(self, tmp):
            dp, _ = _def_with_tasks(tmp, 10)
            _write_readings(tmp, [_rec("rel-a", 4, 10)])
            self.argv = ["--definition", str(dp)]
    fx.append(PosSingle("正例·单记录（基线即当前）", RC_PASS, ["全绿", "单记录"]))

    # —— 负例：空回归集（任务集零条目）——
    class NegEmptySet(_Fixture):
        def build(self, tmp):
            dp, _ = _def_with_tasks(tmp, 0)
            self.argv = ["--definition", str(dp)]
    fx.append(NegEmptySet("负例·空回归集 fail-closed", RC_BLOCK,
                          ["空回归集", "fail-closed"]))

    # —— 负例：任务集计数漂移（钉 10 实 11）——
    class NegCountDrift(_Fixture):
        def build(self, tmp):
            dp, tp = _def_with_tasks(tmp, 10)
            _write_tasks(tmp, 11)  # 覆写 tasks.json 为 11 条（计数+摘要双漂移）
            self.argv = ["--definition", str(dp)]
    fx.append(NegCountDrift("负例·任务集计数漂移", RC_BLOCK, ["计数漂移"]))

    # —— 负例：任务集摘要漂移（同计数不同内容）——
    class NegDigestDrift(_Fixture):
        def build(self, tmp):
            dp, tp = _def_with_tasks(tmp, 10)
            entries = json.loads(tp.read_text(encoding="utf-8"))
            entries[0], entries[1] = entries[1], entries[0]  # 同计数、内容换序 ⇒ 摘要变
            tp.write_text(json.dumps(entries, ensure_ascii=False, indent=1) + "\n",
                          encoding="utf-8")
            self.argv = ["--definition", str(dp)]
    fx.append(NegDigestDrift("负例·任务集摘要漂移", RC_BLOCK, ["摘要漂移"]))

    # —— 负例：定义缺 seed ——
    class NegNoSeed(_Fixture):
        def build(self, tmp):
            dp, _ = _def_with_tasks(tmp, 10, seed=None)
            d = json.loads(dp.read_text(encoding="utf-8"))
            del d["seed"]
            dp.write_text(json.dumps(d, ensure_ascii=False), encoding="utf-8")
            self.argv = ["--definition", str(dp)]
    fx.append(NegNoSeed("负例·定义缺 seed", RC_BLOCK, ["seed"]))

    # —— 负例：阈值弱化（10pp > S130 实录回归 6.9pp ⇒ 拦不住即红）——
    class NegLaxThreshold(_Fixture):
        def build(self, tmp):
            dp, _ = _def_with_tasks(tmp, 10,
                                    thresholds={"resolve_degradation_pp": 10.0,
                                                "token_degradation_pct": 15.0})
            self.argv = ["--definition", str(dp)]
    fx.append(NegLaxThreshold("负例·阈值弱化（实录回归拦不住）", RC_BLOCK, ["阈值弱化"]))

    # —— 负例：注入 resolve 劣化（40.0%→20.0%，Δ20.0pp > 5.0pp）——
    class NegResolveDegrade(_Fixture):
        def build(self, tmp):
            dp, _ = _def_with_tasks(tmp, 10)
            _write_readings(tmp, [_rec("rel-a", 4, 10), _rec("rel-b", 2, 10)])
            self.argv = ["--definition", str(dp)]
    fx.append(NegResolveDegrade("负例·注入 resolve 劣化（实测阻断读数）", RC_BLOCK,
                                ["劣化阻断", "20.0pp", "阈值 5.0pp"]))

    # —— 负例：注入 token 劣化（400K→600K，+50.0% > 15%）——
    class NegTokenDegrade(_Fixture):
        def build(self, tmp):
            dp, _ = _def_with_tasks(tmp, 10)
            _write_readings(tmp, [_rec("rel-a", 4, 10, tokens_per_task_k=400.0),
                                  _rec("rel-b", 4, 10, tokens_per_task_k=600.0)])
            self.argv = ["--definition", str(dp)]
    fx.append(NegTokenDegrade("负例·注入 token/题劣化", RC_BLOCK,
                              ["劣化阻断", "token/题", "50.0%"]))

    # —— 负例：读数口径漂移（模型被换）——
    class NegConfigDrift(_Fixture):
        def build(self, tmp):
            dp, _ = _def_with_tasks(tmp, 10)
            _write_readings(tmp, [_rec("rel-a", 4, 10),
                                  _rec("rel-b", 4, 10, model="other-model@v9")])
            self.argv = ["--definition", str(dp)]
    fx.append(NegConfigDrift("负例·读数口径漂移（模型）", RC_BLOCK, ["口径漂移"]))

    # —— 负例：回归集未全覆盖（子集跑分 total=8≠10）——
    class NegPartial(_Fixture):
        def build(self, tmp):
            dp, _ = _def_with_tasks(tmp, 10)
            _write_readings(tmp, [_rec("rel-a", 4, 10), _rec("rel-b", 3, 8)])
            self.argv = ["--definition", str(dp)]
    fx.append(NegPartial("负例·回归集未全覆盖（子集跑分）", RC_BLOCK, ["未全覆盖"]))

    # —— 负例：读数记录不可解析 ——
    class NegBadJson(_Fixture):
        def build(self, tmp):
            dp, _ = _def_with_tasks(tmp, 10)
            p = tmp / "readings.jsonl"
            p.write_text('{"release": "rel-a", "model": "fixture-model@v1", "seed": 20261029,'
                         ' "runs": 2, "resolved": 4, "total": 10}\n{not json}\n',
                         encoding="utf-8")
            self.argv = ["--definition", str(dp)]
    fx.append(NegBadJson("负例·读数记录不可解析", RC_BLOCK, ["不可解析"]))

    # —— 负例：release 重号 ——
    class NegDupRelease(_Fixture):
        def build(self, tmp):
            dp, _ = _def_with_tasks(tmp, 10)
            _write_readings(tmp, [_rec("rel-a", 4, 10), _rec("rel-a", 5, 10)])
            self.argv = ["--definition", str(dp)]
    fx.append(NegDupRelease("负例·release 重号", RC_BLOCK, ["重号"]))

    # —— 负例：读数台账在场但零记录 ——
    class NegEmptyLedger(_Fixture):
        def build(self, tmp):
            dp, _ = _def_with_tasks(tmp, 10)
            (tmp / "readings.jsonl").write_text("", encoding="utf-8")
            self.argv = ["--definition", str(dp)]
    fx.append(NegEmptyLedger("负例·空读数台账", RC_BLOCK, ["台账为空", "fail-closed"]))

    # —— 负例：--require-readings 且读数缺席 ——
    class NegRequireAbsent(_Fixture):
        def build(self, tmp):
            dp, _ = _def_with_tasks(tmp, 10)
            self.argv = ["--definition", str(dp), "--require-readings"]
    fx.append(NegRequireAbsent("负例·--require-readings 缺席即红", RC_BLOCK,
                               ["require-readings", "缺席"]))

    # —— 正例：读数缺席默认 INFO 不拦 ——
    class PosAbsent(_Fixture):
        def build(self, tmp):
            dp, _ = _def_with_tasks(tmp, 10)
            self.argv = ["--definition", str(dp)]
    fx.append(PosAbsent("正例·读数缺席默认不拦（INFO）", RC_PASS,
                        ["全绿", "读数面待首跑"]))

    # —— 校准：S130 实录回归必拦（39.4%→32.5%，Δ6.9pp > 5.0pp；1000 题夹具保真百分比）——
    class CalRealCaseBlock(_Fixture):
        def build(self, tmp):
            n = 1000
            tp = _write_tasks(tmp, n, name="cal-tasks.json")
            d = _fixture_def(task_set={"path": tp.name, "count": n,
                                       "sha256": sha256_file(tp)})
            dp = tmp / "definition.json"
            dp.write_text(json.dumps(d, ensure_ascii=False), encoding="utf-8")
            _write_readings(tmp, [_rec("v0.4.1", 394, n, tokens_per_task_k=561.0),
                                  _rec("v0.5.0", 325, n, tokens_per_task_k=561.0)])
            self.argv = ["--definition", str(dp)]
    fx.append(CalRealCaseBlock("校准·S130 实录回归 39.4%→32.5% 必拦", RC_BLOCK,
                               ["劣化阻断", "6.9pp", "v0.4.1", "v0.5.0"]))

    # —— 校准：窗口内波动不拦（30.5%→28.0%，Δ2.5pp ≤ 5.0pp）——
    class CalInWindowPass(_Fixture):
        def build(self, tmp):
            n = 1000
            tp = _write_tasks(tmp, n, name="cal-tasks.json")
            d = _fixture_def(task_set={"path": tp.name, "count": n,
                                       "sha256": sha256_file(tp)})
            dp = tmp / "definition.json"
            dp.write_text(json.dumps(d, ensure_ascii=False), encoding="utf-8")
            _write_readings(tmp, [_rec("rel-x", 305, n, tokens_per_task_k=500.0),
                                  _rec("rel-y", 280, n, tokens_per_task_k=505.0)])
            self.argv = ["--definition", str(dp)]
    fx.append(CalInWindowPass("校准·窗口内波动 30.5%→28.0% 不拦", RC_PASS, ["全绿"]))

    # —— 在役：真仓缺省定义件 rc=0 ——
    class LiveRepo(_Fixture):
        def build(self, tmp):
            self.argv = []  # 缺省定义件=仓内 ci/baselines/harness-regression-set.json
    fx.append(LiveRepo("在役·真仓定义件 rc=0", RC_PASS, ["全绿", "定义面在役"]))

    return fx


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(
        prog="harness_regression_gate",
        description="harness 版本级回归门（D-54）：固定模型+任务集+seed 回归集定义 + "
                    "劣化阈值阻断发布；空回归集 fail-closed；selftest 负例夹具 --selftest")
    ap.add_argument("--definition", default=str(DEFAULT_DEFINITION),
                    help="回归集定义件（缺省=仓内 ci/baselines/harness-regression-set.json）")
    ap.add_argument("--readings", default=None,
                    help="读数台账覆盖（绝对路径，或相对定义件目录）")
    ap.add_argument("--require-readings", action="store_true",
                    help="读数台账缺席即硬红（发布管线模式）")
    ap.add_argument("--report", default=None, help="判定报告 JSON 输出路径（可选）")
    ap.add_argument("--selftest", action="store_true",
                    help="负例夹具全套（临时目录，不触真仓任何树）")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    rc = evaluate(Path(args.definition), readings_override=args.readings,
                  require_readings=args.require_readings)
    if args.report:
        rp = Path(args.report)
        rp.parent.mkdir(parents=True, exist_ok=True)
        rp.write_text(json.dumps({
            "gate": "harness_regression_gate",
            "card": "s17-harness-regression-gate（D-54）",
            "definition": str(args.definition),
            "verdict": "green" if rc == RC_PASS else "block",
            "errors": _LOG["errors"],
            "infos": _LOG["infos"],
        }, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    return rc


if __name__ == "__main__":
    sys.exit(main())
