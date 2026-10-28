#!/usr/bin/env python3
"""arm_wiring_check.py — 三臂防线逐格断言器（G0-C 施工批落地件）
草案出处：计划/研究组/G0-CI机制批-实施规格-20261027/C/2-arm-wiring/SPEC.md（子代理 C）
正本锚：红队复盘 C6 升级①（:186「每臂×每条防线一行，CI 逐格断言在场，删任一格即红」）
        + C1-6（:105）+ PP4 升级③（:292）+ §5.2-5（:334）。

子命令：
  --validate <yaml>   schema 校验（rows 臂数/臂集合、id 唯一、evidence 锚点+断言串、
                      absent 债号在册、present/variant 不得挂 debt、在册清单钉死）→ 违例 exit 2
  --check <yaml> [--report ci/out/arm-wiring-report.json]
                      逐格断言：present/variant 格每个 `文件:行号` 锚点 ±5 行内必须命中
                      至少一个 `反引号断言串`（漂移 >5 行 WARN、>30 行红）；absent 格债号
                      在册即绿 → 全绿 exit 0，任一格红 exit 1
债册：--debt-register（默认 ci/debt-register.md 仓内镜像；env DEBT_REGISTER 覆盖）。
root 钉死（红队 P2-2）：路径相对调用方 CWD（run.sh 内 cd 到 af 仓根）；只读，不跑 cargo。
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path

try:
    import yaml
except ImportError:  # pragma: no cover
    print("[arm-wiring] FATAL: 需要 PyYAML（python3 -c 'import yaml'）", file=sys.stderr)
    sys.exit(2)

DEF_ID_RE = re.compile(r"^D\d+-[a-z0-9-]+$")
G_ID_RE = re.compile(r"^G\d+-[a-z0-9-]+$")
DEBT_ID_RE = re.compile(r"^D-\d+$")
ANCHOR_RE = re.compile(r"([\w./-]+\.(?:rs|py)):(\d+)")
TOKEN_RE = re.compile(r"`([^`]+)`")
STATUSES = {"present", "variant", "absent", "pending-debt"}
WINDOW = 5        # 锚点 ±5 行邻域（SPEC ②）
WARN_DRIFT = 30   # 漂移 >30 行即红（SPEC ⑧-4 双阈值）
SRC_SEARCH_ROOT = "crates/af-execline/src"
DEFAULT_CONFIG = "ci/arm-wiring.yaml"
DEFAULT_DEBT_REGISTER = "ci/debt-register.md"
DEFAULT_REPORT = "ci/out/arm-wiring-report.json"


def bail(code: int, msg: str) -> None:
    print(f"[arm-wiring] FATAL: {msg}", file=sys.stderr)
    sys.exit(code)


def repo_root() -> Path:
    for cand in (Path.cwd(), Path(__file__).resolve().parent.parent):
        if (cand / "crates").is_dir() and (cand / "ci").is_dir():
            return cand
    return Path.cwd()


def resolve_path(p: str) -> Path:
    q = Path(p)
    return q if q.is_absolute() or q.exists() else repo_root() / p


def load_debt_register(path: Path | None) -> set[str]:
    if path is None or not path.is_file():
        return set()
    ids = set()
    for m in re.finditer(r"^\|\s*(D-\d+)\s*\|", path.read_text(encoding="utf-8"), re.M):
        ids.add(m.group(1))
    return ids


def parse_evidence(evidence: str) -> tuple[list[tuple[str, int, list[str]]], list[str]]:
    """evidence → (锚点[(file, line, 绑定断言串)…], 全部断言串)。

    位置绑定：每个 `文件:行号` 锚点捕获其之后、下一锚点之前的全部反引号 token
    （防「同格多锚点共用一个泛泛 token、别处残留同名串掩蔽删格」——干跑实测：
    report_unit.rs pyc 块整块删除后，:136 锚点仍被文档注释里的 `__pycache__`
    满足而假绿；绑定唯一拒收文案 `判分目录不得存在字节码` 后删块即红）。
    首个锚点之前的 token 归入第一个锚点（防御性）。"""
    matches = list(ANCHOR_RE.finditer(evidence))
    all_tokens = [t.strip() for t in TOKEN_RE.findall(evidence) if t.strip()]
    if not matches:
        return [], all_tokens
    anchors: list[tuple[str, int, list[str]]] = []
    first_start = matches[0].start()
    pre_tokens = [t.strip() for t in TOKEN_RE.findall(evidence[:first_start]) if t.strip()]
    for i, m in enumerate(matches):
        seg_start = m.end()
        seg_end = matches[i + 1].start() if i + 1 < len(matches) else len(evidence)
        toks = [t.strip() for t in TOKEN_RE.findall(evidence[seg_start:seg_end]) if t.strip()]
        if i == 0 and pre_tokens:
            toks = pre_tokens + toks
        anchors.append((m.group(1), int(m.group(2)), toks))
    return anchors, all_tokens


def resolve_source(name: str, root: Path, errs: list[str], where: str) -> Path | None:
    """带目录路径→仓根相对；裸文件名→crates/af-execline/src 下唯一匹配。"""
    if "/" in name:
        p = root / name
        if not p.is_file():
            errs.append(f"{where}: 指称文件不在盘: {name}")
            return None
        return p
    hits = sorted((root / SRC_SEARCH_ROOT).rglob(name))
    if len(hits) == 1:
        return hits[0]
    if not hits:
        errs.append(f"{where}: 裸文件名 {name} 在 {SRC_SEARCH_ROOT} 下无匹配")
    else:
        errs.append(f"{where}: 裸文件名 {name} 在 {SRC_SEARCH_ROOT} 下不唯一: {[str(h) for h in hits]}")
    return None


def check_row(row: dict, root: Path, debt_ids: set[str], where: str) -> dict:
    """单格断言。present/variant→逐锚点窗口 grep；absent→债号在册。"""
    status = row.get("status")
    evidence = str(row.get("evidence") or "")
    anchors, tokens = parse_evidence(evidence)
    cell = {"where": where, "arm": row.get("arm"), "status": status,
            "anchors": [], "verdict": "PASS", "note": ""}
    if status == "absent":
        debt = str(row.get("debt") or "")
        if not DEBT_ID_RE.match(debt):
            cell["verdict"] = "FAIL"
            cell["note"] = f"absent 格缺合法债号（debt={debt!r}）——absent 无债号=本门 fail-loud"
        elif debt_ids and debt not in debt_ids:
            cell["verdict"] = "FAIL"
            cell["note"] = f"absent 格债号 {debt} 不在债务登记册在册集（指针失联）"
        else:
            cell["note"] = f"absent 格债号 {debt} 可解析（放行；v0.2 补齐后翻 present）"
        return cell
    # present / variant：逐锚点 ±5 行窗口 grep 其绑定断言串；全锚点过才绿（合取）
    if not anchors:
        cell["verdict"] = "FAIL"
        cell["note"] = "evidence 缺 `文件:行号` 锚点"
        return cell
    if not tokens:
        cell["verdict"] = "FAIL"
        cell["note"] = "evidence 缺反引号断言串（无可抽取断言=不可机检）"
        return cell
    for name, line, bound in anchors:
        src = resolve_source(name, root, [], where)
        entry = {"file": name, "line": line, "tokens": bound, "matched_line": None,
                 "drift": None, "verdict": "FAIL", "note": ""}
        if not bound:
            entry["note"] = "锚点未绑定反引号断言串"
            cell["anchors"].append(entry)
            cell["verdict"] = "FAIL"
            continue
        if src is None:
            entry["note"] = "指称文件不可解析"
            cell["anchors"].append(entry)
            cell["verdict"] = "FAIL"
            continue
        lines = src.read_text(encoding="utf-8", errors="replace").splitlines()
        lo, hi = max(1, line - WINDOW), min(len(lines), line + WINDOW)
        hit_line = None
        for ln in range(lo, hi + 1):
            text = lines[ln - 1]
            if any(tok in text for tok in bound):
                hit_line = ln
                break
        if hit_line is not None:
            entry.update(matched_line=hit_line, drift=hit_line - line, verdict="PASS",
                         note="窗口内命中")
        else:
            # 窗口外全文找：漂移 >5 WARN、>30 红（SPEC ⑧-4 双阈值）
            far = None
            for ln, text in enumerate(lines, 1):
                if any(tok in text for tok in bound):
                    far = ln
                    break
            if far is None:
                entry["note"] = "绑定断言串全文零命中——防线代码被删/改写（删格即红）"
                cell["verdict"] = "FAIL"
            else:
                drift = abs(far - line)
                entry.update(matched_line=far, drift=far - line)
                if drift > WARN_DRIFT:
                    entry.update(verdict="FAIL", note=f"绑定断言串漂移 {drift} 行 >{WARN_DRIFT}——疑被删改后别处重现")
                    cell["verdict"] = "FAIL"
                else:
                    entry.update(verdict="WARN", note=f"绑定断言串漂移 {drift} 行（>{WINDOW}）——防线应在，请回填行号")
                    if cell["verdict"] != "FAIL":
                        cell["verdict"] = "WARN"
        cell["anchors"].append(entry)
    return cell


def cmd_validate(args, root: Path, cfg: dict, debt_ids: set[str]) -> int:
    errs: list[str] = []
    if cfg.get("version") != 1:
        errs.append(f"version 必须为 int 1（实得 {cfg.get('version')!r}）")
    defs = cfg.get("defs") or {}
    arms = defs.get("arms")
    if not isinstance(arms, dict) or not arms:
        errs.append("defs.arms 必须为非空 mapping（三臂实现位声明）")
        arms = {}
    statuses = defs.get("statuses")
    if not isinstance(statuses, list) or not set(STATUSES) <= set(statuses):
        errs.append(f"defs.statuses 必须含 {sorted(STATUSES)}")
    defenses = cfg.get("defenses")
    if not isinstance(defenses, list) or not defenses:
        errs.append("defenses 必须为非空 list")
        defenses = []
    seen = set()
    for d in defenses:
        if not isinstance(d, dict):
            errs.append("defenses[] 每项必须为 mapping")
            continue
        did = d.get("id")
        if not isinstance(did, str) or not DEF_ID_RE.match(did):
            errs.append(f"防线 id={did!r} 不匹配 ^D\\d+-[a-z0-9-]+$")
            continue
        if did in seen:
            errs.append(f"防线 id 重复: {did}")
        seen.add(did)
        rows = d.get("rows")
        if not isinstance(rows, list) or len(rows) != len(arms):
            errs.append(f"{did}: rows 必须恰 {len(arms)} 条（每臂一条；少一格=断言面缩水）")
            continue
        row_arms = [r.get("arm") for r in rows]
        if sorted(map(str, row_arms)) != sorted(arms):
            errs.append(f"{did}: rows 臂集合 {row_arms} 与 defs.arms {sorted(arms)} 不一致")
        for r in rows:
            if not isinstance(r, dict):
                errs.append(f"{did}: rows[] 每项必须为 mapping")
                continue
            arm, status = r.get("arm"), r.get("status")
            where = f"{did}/{arm}"
            if arm not in arms:
                errs.append(f"{where}: arm 不在 defs.arms 声明集")
            if status not in STATUSES:
                errs.append(f"{where}: status={status!r} 不在 {sorted(STATUSES)}")
                continue
            ev = str(r.get("evidence") or "")
            if not ev.strip():
                errs.append(f"{where}: evidence 必须非空")
            anchors, tokens = parse_evidence(ev)
            if status in ("present", "variant"):
                if r.get("debt"):
                    errs.append(f"{where}: present/variant 格不得挂 debt（{r.get('debt')!r}）")
                if not anchors:
                    errs.append(f"{where}: evidence 缺 `文件:行号` 锚点")
                if not tokens:
                    errs.append(f"{where}: evidence 缺反引号断言串")
                for name, _ln, btok in anchors:
                    resolve_source(name, root, errs, where)
                    if not btok:
                        errs.append(f"{where}: 锚点 {name}:{_ln} 未绑定反引号断言串（位置绑定，防同名串掩蔽）")
            elif status == "absent":
                debt = str(r.get("debt") or "")
                if not DEBT_ID_RE.match(debt):
                    errs.append(f"{where}: absent 格必须挂 ^D-\\d+$ 债号（实得 {debt!r}）")
                elif debt_ids and debt not in debt_ids:
                    errs.append(f"{where}: absent 格债号 {debt} 不在册（指针失联）")
                if not any(nm for nm, _ln, _bt in anchors) and not any(
                        seg for seg in re.findall(r"[\w./-]+\.(?:rs|py)", ev)):
                    errs.append(f"{where}: absent 格 evidence 须指称相关源文件（可追溯性）")
    gspec = cfg.get("graph_specific") or []
    gseen = set()
    for g in gspec:
        if not isinstance(g, dict):
            errs.append("graph_specific[] 每项必须为 mapping")
            continue
        gid = g.get("id")
        if not isinstance(gid, str) or not G_ID_RE.match(gid):
            errs.append(f"graph_specific id={gid!r} 不匹配 ^G\\d+-[a-z0-9-]+$")
            continue
        if gid in gseen:
            errs.append(f"graph_specific id 重复: {gid}")
        gseen.add(gid)
        status = g.get("status")
        if status not in STATUSES:
            errs.append(f"{gid}: status={status!r} 不合法")
            continue
        anchors, tokens = parse_evidence(str(g.get("evidence") or ""))
        if status in ("present", "variant"):
            if not anchors or not tokens:
                errs.append(f"{gid}: present/variant 须含锚点+断言串")
            for name, _ln, btok in anchors:
                resolve_source(name, root, errs, gid)
                if not btok:
                    errs.append(f"{gid}: 锚点 {name}:{_ln} 未绑定反引号断言串")
        else:
            debt = str(g.get("debt") or "")
            if not DEBT_ID_RE.match(debt) or (debt_ids and debt not in debt_ids):
                errs.append(f"{gid}: absent/pending-debt 须挂在册债号（实得 {debt!r}）")
    meta = cfg.get("meta") or {}
    exp_d = meta.get("defenses_expected")
    if isinstance(exp_d, list) and sorted(map(str, exp_d)) != sorted(seen):
        errs.append(f"在册防线与 meta.defenses_expected 不一致（实得 {sorted(seen)} ↔ 期望 "
                    f"{sorted(map(str, exp_d))}）——删线/删格必须同步钉死清单（SPEC 2 ⑤-4）")
    exp_g = meta.get("graph_specific_expected")
    if isinstance(exp_g, list) and sorted(map(str, exp_g)) != sorted(gseen):
        errs.append(f"graph_specific 与 meta.graph_specific_expected 不一致（实得 {sorted(gseen)} ↔ 期望 "
                    f"{sorted(map(str, exp_g))}）")
    for k in ("assert_rule", "debt_pointer", "review_hook"):
        if not str(meta.get(k) or "").strip():
            errs.append(f"meta.{k} 必须非空")
    if errs:
        for e in errs:
            print(f"  SCHEMA-FAIL {e}")
        print(f"== arm-wiring --validate: {len(errs)} 项违例")
        return 2
    n_cells = sum(len(d.get("rows") or []) for d in defenses) + len(gspec)
    print(f"== arm-wiring --validate: schema 通过（{len(defenses)} 防线 × {len(arms)} 臂 + "
          f"{len(gspec)} graph 特有 = {n_cells} 格；债册在册 {len(debt_ids)} 号）")
    return 0


def cmd_check(args, root: Path, cfg: dict, debt_ids: set[str]) -> int:
    cells: list[dict] = []
    for d in cfg.get("defenses") or []:
        for r in d.get("rows") or []:
            cells.append(check_row(r, root, debt_ids, f"{d['id']}/{r.get('arm')}"))
    for g in cfg.get("graph_specific") or []:
        cells.append(check_row(g, root, debt_ids, g["id"]))
    n_fail = sum(1 for c in cells if c["verdict"] == "FAIL")
    n_warn = sum(1 for c in cells if c["verdict"] == "WARN")
    n_pass = sum(1 for c in cells if c["verdict"] == "PASS")
    for c in cells:
        mark = {"PASS": "OK  ", "WARN": "WARN", "FAIL": "FAIL"}[c["verdict"]]
        detail = c["note"]
        # 非 PASS 格优先打印问题锚点（合取下首锚点可能仍命中，直接打印会误导）
        bad = [a for a in c["anchors"] if a["verdict"] != "PASS"]
        show = bad[0] if bad and c["verdict"] != "PASS" else (c["anchors"][0] if c["anchors"] else None)
        if show:
            if show["matched_line"]:
                detail += (f" | 锚点 {show['file']}:{show['line']} → 命中行 {show['matched_line']}"
                           f"（漂移 {show['drift']}）{show['note']}")
            else:
                detail += f" | 锚点 {show['file']}:{show['line']} {show['note']}"
        print(f"  [{mark}] {c['where']:<44} {c['status']:<8} {detail}")
    report = {
        "ts": __import__("datetime").datetime.now(
            __import__("datetime").timezone.utc).isoformat(timespec="seconds"),
        "cells": cells,
        "summary": {"total": len(cells), "pass": n_pass, "warn": n_warn, "fail": n_fail},
    }
    rp = resolve_path(args.report)
    rp.parent.mkdir(parents=True, exist_ok=True)
    rp.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"== arm-wiring --check: {len(cells)} 格 = {n_pass} PASS / {n_warn} WARN / {n_fail} FAIL"
          f" → 报告 {rp}")
    return 1 if n_fail else 0


def main() -> int:
    ap = argparse.ArgumentParser(description="三臂防线逐格断言器（G0-C 落地件）")
    ap.add_argument("config_pos", nargs="?", default=None,
                    help="配置路径（位置参数形式，兼容 SPEC ② diff 草案命令；与 --config 等价）")
    ap.add_argument("--validate", action="store_true")
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--config", default=DEFAULT_CONFIG)
    ap.add_argument("--report", default=DEFAULT_REPORT)
    ap.add_argument("--debt-register", default=DEFAULT_DEBT_REGISTER,
                    help="债务登记册（仓内镜像 ci/debt-register.md；env DEBT_REGISTER 覆盖）")
    args = ap.parse_args()
    if args.config_pos:
        args.config = args.config_pos
    if os.environ.get("DEBT_REGISTER"):
        args.debt_register = os.environ["DEBT_REGISTER"]
    root = repo_root()
    cfg_path = resolve_path(args.config)
    if not cfg_path.is_file():
        bail(2, f"配置不在盘: {cfg_path}")
    try:
        cfg = yaml.safe_load(cfg_path.read_text(encoding="utf-8"))
    except yaml.YAMLError as e:
        bail(2, f"YAML 解析失败: {cfg_path}: {e}")
    if not isinstance(cfg, dict):
        bail(2, "配置顶层必须为 mapping")
    debt_ids = load_debt_register(resolve_path(args.debt_register))
    if args.validate:
        return cmd_validate(args, root, cfg, debt_ids)
    if args.check:
        return cmd_check(args, root, cfg, debt_ids)
    ap.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
