#!/usr/bin/env python3
"""fix_scan.py — 同族横切扫描器（G0-C 施工批落地件）
草案出处：计划/研究组/G0-CI机制批-实施规格-20261027/C/1-fix-scan/SPEC.md（子代理 C）
正本锚：红队复盘 §三 C9-4①（:230「修一族漏一族」）+ §四 PP4 升级①（:292）+ §五 5.1（:316）；
       禁裸 grep 管道退出码教训=复盘 C3-3（:137）。

子命令：
  --validate [--config ci/fix-scan.yaml]   schema+白名单校验（任何字段违例 exit 2）
  --run --mode diff|full [--diff-base HEAD~1] [--family ID]
                                          逐族扫描；diff 档=trigger_globs 命中的族才跑，
                                          full 档=全族全量；产物落 redteam/scans/<batch>/
  --verify-artifacts [--config ...]       合流门钩子：最新 batch 各族产物存在且 sha256 一致

退出码：0=校验过+各族零命中（白名单内不计）；1=族扫描有白名单外命中（输出 suggested_debt）；
        2=schema/配置自身错误（含白名单缺债号、双本白名单不一致）；3=双法族两法结论不一致。
root 钉死（红队 P2-2）：所有路径相对调用方 CWD（run.sh 内 cd 到 af 仓根）；禁对旧分支
局部副本（agent黑灯工厂/af/，无 crates suite）跑——会得假绿。
只读扫描 + 只写 redteam/scans/<batch>/ 产物；不跑 cargo、不改代码。
"""
from __future__ import annotations

import argparse
import datetime as dt
import fnmatch
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

try:
    import yaml
except ImportError:  # pragma: no cover
    print("[fix-scan] FATAL: 需要 PyYAML（python3 -c 'import yaml'）", file=sys.stderr)
    sys.exit(2)

FAMILY_ID_RE = re.compile(r"^[a-z][a-z0-9-]*$")
DEBT_ID_RE = re.compile(r"^D-\d+$")
HISTORY_ANCHOR_RE = re.compile(r"W\d+-C-\d+|D-\d+|C\d+(?:-\d+)?")
ANCHOR_PATH_RE = re.compile(r"([\w./-]+\.(?:rs|py)):(\d+)")
COMMENT_RE = re.compile(r"^\s*(//|/\*|\*)")
CFG_TEST_RE = re.compile(r"#\[cfg\(test\)\]")
WHITELIST_CORE_KEYS = ("path", "line", "pattern", "debt")
PRUNE_DIRS = {"target", ".git", "out", "node_modules"}
DEFAULT_CONFIG = "ci/fix-scan.yaml"
DEFAULT_WHITELIST_JSON = "ci/budget-cast-whitelist.json"
DEFAULT_DEBT_REGISTER = "ci/debt-register.md"
ARTIFACT_PREFIX = "redteam/scans/"


def bail(code: int, msg: str) -> None:
    print(f"[fix-scan] FATAL: {msg}", file=sys.stderr)
    sys.exit(code)


def note(msg: str) -> None:
    print(f"[fix-scan] {msg}")


# ---- 路径解析（CWD 优先，回落脚本推定仓根）-------------------------------------
def repo_root() -> Path:
    for cand in (Path.cwd(), Path(__file__).resolve().parent.parent):
        if (cand / "crates").is_dir() and (cand / "ci").is_dir():
            return cand
    return Path.cwd()


def resolve_path(p: str) -> Path:
    q = Path(p)
    return q if q.is_absolute() or q.exists() else repo_root() / p


# ---- 通用小件 ------------------------------------------------------------------
def sha256_of(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_debt_register(path: Path | None) -> set[str]:
    if path is None or not path.is_file():
        return set()
    ids = set()
    for m in re.finditer(r"^\|\s*(D-\d+)\s*\|", path.read_text(encoding="utf-8"), re.M):
        ids.add(m.group(1))
    return ids


def next_debt_id(debt_ids: set[str]) -> str:
    nums = [int(d[2:]) for d in debt_ids if d.startswith("D-") and d[2:].isdigit()]
    return f"D-{(max(nums) + 1) if nums else 1}"


def glob_match(rel: str, pattern: str) -> bool:
    return fnmatch.fnmatch(rel, pattern)


def is_excluded(rel: str, excludes: list[str]) -> bool:
    for g in excludes:
        if glob_match(rel, g):
            return True
        base = g.rstrip("*").rstrip("/")
        if base and (rel == base or rel.startswith(base + "/")):
            return True
    return False


def walk_files(root: Path, excludes: list[str]):
    """os.walk + 剪枝（PRUNE_DIRS 构件名 + meta.excludes  glob）。relpath 用 posix 分隔。"""
    for dirpath, dirnames, filenames in os.walk(root):
        rel_dir = os.path.relpath(dirpath, root).replace(os.sep, "/")
        kept = []
        for d in sorted(dirnames):
            child = d if rel_dir == "." else f"{rel_dir}/{d}"
            if d in PRUNE_DIRS or is_excluded(child, excludes):
                continue
            kept.append(d)
        dirnames[:] = kept
        for f in sorted(filenames):
            yield f"{rel_dir}/{f}" if rel_dir != "." else f


def production_lines(lines: list[str]) -> list[tuple[int, str]]:
    """生产码区：注释行剔除、#[cfg(test)] 之后剔除（与 budget-cast-lint 同口径，
    SPEC 3 ⑧-3 测试域豁免——两门各自内联同款小件，保持门间独立）。"""
    out, in_test = [], False
    for no, line in enumerate(lines, 1):
        if CFG_TEST_RE.search(line):
            in_test = True
        if in_test or COMMENT_RE.match(line):
            continue
        out.append((no, line))
    return out


def git_commit(root: Path) -> str:
    try:
        return subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=root, capture_output=True, text=True,
            check=True).stdout.strip()
    except Exception:
        return "unknown"


def changed_files(root: Path, base: str) -> set[str] | None:
    """git diff --name-only base...HEAD；base 不可解析返回 None（调用方按 full 保守处理）。"""
    try:
        r = subprocess.run(["git", "rev-parse", "--verify", base], cwd=root,
                           capture_output=True, text=True)
        if r.returncode != 0:
            return None
        r = subprocess.run(["git", "diff", "--name-only", f"{base}...HEAD"], cwd=root,
                           capture_output=True, text=True)
        if r.returncode != 0:
            return None
        return {ln.strip() for ln in r.stdout.splitlines() if ln.strip()}
    except Exception:
        return None


# ---- 配置加载与 schema 校验 -----------------------------------------------------
def load_config(path: Path) -> dict:
    if not path.is_file():
        bail(2, f"配置文件不在盘: {path}")
    try:
        cfg = yaml.safe_load(path.read_text(encoding="utf-8"))
    except yaml.YAMLError as e:
        bail(2, f"YAML 解析失败: {path}: {e}")
    if not isinstance(cfg, dict):
        bail(2, f"配置顶层必须为 mapping: {path}")
    return cfg


def _check_whitelist_entries(entries, full_globs: list[str], where: str,
                             debt_ids: set[str]) -> list[str]:
    errs = []
    if not isinstance(entries, list):
        return [f"{where}: whitelist 必须为 list"]
    for i, e in enumerate(entries):
        if not isinstance(e, dict):
            errs.append(f"{where}: 白名单第 {i} 条必须为 mapping")
            continue
        for k in ("path", "line", "pattern", "debt", "reason", "expires"):
            if k not in e or not str(e[k]).strip():
                errs.append(f"{where}: 白名单第 {i} 条缺必填字段 {k}（无债号条目=清单自身红）")
        if "debt" in e and not DEBT_ID_RE.match(str(e["debt"])):
            errs.append(f"{where}: 白名单第 {i} 条 debt={e['debt']!r} 不匹配 ^D-\\d+$")
        if "line" in e and not isinstance(e["line"], int):
            errs.append(f"{where}: 白名单第 {i} 条 line={e['line']!r} 非 int")
        if debt_ids and "debt" in e and str(e["debt"]) not in debt_ids:
            errs.append(f"{where}: 白名单第 {i} 条债号 {e['debt']} 不在债务登记册在册集（指针失联即红）")
        if "path" in e and full_globs and not any(glob_match(str(e["path"]), g) for g in full_globs):
            errs.append(f"{where}: 白名单第 {i} 条 path={e['path']} 不在 full_scan_globs 覆盖面内")
    return errs


def validate_config(cfg: dict, debt_ids: set[str], wl_json: Path | None) -> list[str]:
    errs: list[str] = []
    if cfg.get("version") != 1:
        errs.append(f"version 必须为 int 1（实得 {cfg.get('version')!r}）")
    fams = cfg.get("families")
    if not isinstance(fams, list) or len(fams) < 1:
        errs.append("families 必须为 ≥1 的 list")
        fams = []
    meta = cfg.get("meta")
    if not isinstance(meta, dict):
        errs.append("meta 必须为 mapping")
        meta = {}
    excludes = meta.get("excludes") or []
    for req in ("target/**", "ci/out/**", ".git/**"):
        if req not in excludes:
            errs.append(f"meta.excludes 必须含 {req}")
    if meta.get("artifact_hash") != "sha256":
        errs.append("meta.artifact_hash 必须为 sha256")
    if not str(meta.get("debt_pointer_rule") or "").strip():
        errs.append("meta.debt_pointer_rule 必须非空")
    if not isinstance(meta.get("scan_roots_default"), list) or not meta.get("scan_roots_default"):
        errs.append("meta.scan_roots_default 必须为非空 list")
    expected = meta.get("families_expected")
    if not isinstance(expected, list) or not expected:
        errs.append("meta.families_expected 必须为非空 list（在册族清单钉死，删族即红）")

    seen_ids = set()
    for fam in fams:
        if not isinstance(fam, dict):
            errs.append("families[] 每项必须为 mapping")
            continue
        fid = fam.get("id")
        if not isinstance(fid, str) or not FAMILY_ID_RE.match(fid):
            errs.append(f"族 id={fid!r} 不匹配 ^[a-z][a-z0-9-]*$")
            continue
        if fid in seen_ids:
            errs.append(f"族 id 重复: {fid}")
        seen_ids.add(fid)
        desc = str(fam.get("description") or "")
        if not desc.strip():
            errs.append(f"{fid}: description 必须非空")
        elif not HISTORY_ANCHOR_RE.search(desc):
            errs.append(f"{fid}: description 须含同族史锚（finding 号/债号，如 W3-C-23/D-50）")
        trig = fam.get("trigger_globs")
        if not isinstance(trig, list) or len(trig) < 1:
            errs.append(f"{fid}: trigger_globs 必须 ≥1")
            trig = []
        full = fam.get("full_scan_globs")
        if not isinstance(full, list) or len(full) < 1:
            errs.append(f"{fid}: full_scan_globs 必须 ≥1")
            full = []
        # 合流面不得窄于触发面：每个触发 glob 取样本路径，须被某个 full glob 覆盖
        for g in trig:
            sample = g.replace("**", "x/y").replace("*", "x")
            if full and not any(glob_match(sample, fg) for fg in full):
                errs.append(f"{fid}: full_scan_globs 未覆盖触发面 {g}（合流面窄于触发面）")
        scan = fam.get("scan") or {}
        cmd = str(scan.get("command") or "")
        if not cmd.startswith("python3 ci/"):
            errs.append(f"{fid}: scan.command 必须以 'python3 ci/' 开头（禁裸 grep 管道，复盘 C3-3）")
        ma, mb = scan.get("method_a"), scan.get("method_b")
        dual = ma is not None or mb is not None
        if dual:
            if not (isinstance(ma, dict) and isinstance(mb, dict)):
                errs.append(f"{fid}: 双法族 method_a/method_b 两条都必须在")
            if scan.get("cross_check") != "both_zero_or_identical_sets":
                errs.append(f"{fid}: cross_check 必须为 both_zero_or_identical_sets")
        else:
            if ma is not None or mb is not None:
                errs.append(f"{fid}: 单法族不得出现 method_a/method_b")
        pats = scan.get("patterns")
        if not dual:
            if not isinstance(pats, list) or len(pats) < 1:
                errs.append(f"{fid}: 单法族 scan.patterns 必须 ≥1")
                pats = []
            for p in pats or []:
                try:
                    re.compile(p)
                except re.error as e:
                    errs.append(f"{fid}: pattern 不可编译: {p!r}: {e}")
        for p in scan.get("compliant_patterns") or []:
            try:
                re.compile(p)
            except re.error as e:
                errs.append(f"{fid}: compliant_pattern 不可编译: {p!r}: {e}")
        if fam.get("expect", {}).get("hits") != 0:
            errs.append(f"{fid}: expect.hits 只允许 0（实得 {fam.get('expect', {}).get('hits')!r}）")
        art = str(fam.get("artifact") or "")
        if not art.startswith(ARTIFACT_PREFIX) or not art.endswith(".json"):
            errs.append(f"{fid}: artifact 必须落在 {ARTIFACT_PREFIX}<batch>/ 前缀下且 .json 结尾（实得 {art!r}）")
        # 白名单：sat-cast 共读 JSON 单一正本（SPEC 1 ⑧-6），其余族用 yaml 内联
        wl = fam.get("whitelist")
        if wl is None:
            errs.append(f"{fid}: whitelist 必须显式给出（可空表）")
            wl = []
        errs += _check_whitelist_entries(wl, full, fid, debt_ids)
        if fid == "sat-cast" and wl_json is not None and wl_json.is_file():
            try:
                jdata = json.loads(wl_json.read_text(encoding="utf-8"))
                jentries = jdata.get("entries", jdata) if isinstance(jdata, dict) else jdata
            except json.JSONDecodeError as e:
                errs.append(f"{fid}: 白名单 JSON 单一正本解析失败: {wl_json}: {e}")
                jentries = []
            jcore = sorted(tuple(str(e.get(k)) for k in WHITELIST_CORE_KEYS) for e in jentries)
            ycore = sorted(tuple(str(e.get(k)) for k in WHITELIST_CORE_KEYS) for e in wl)
            if jcore != ycore:
                errs.append(f"{fid}: yaml 白名单与单一正本 {wl_json} 不一致（path/line/pattern/debt 四键对账；"
                            f"防两份白名单漂移，SPEC 1 ⑧-6）")
    if isinstance(expected, list):
        actual = sorted(seen_ids)
        if actual != sorted(str(x) for x in expected):
            errs.append(f"在册族集合与 meta.families_expected 不一致（实得 {actual} ↔ 期望 "
                        f"{sorted(str(x) for x in expected)}）——删族/加族必须同步钉死清单（SPEC 1 ⑤-4）")
    return errs


# ---- 族扫描 ---------------------------------------------------------------------
def scan_single(fam: dict, root: Path, whitelist: list[dict], debt_ids: set[str]) -> dict:
    """单法族（patterns 正则）：全量扫 full_scan_globs 覆盖面；compliant 形态与白名单剔除。"""
    full = fam.get("full_scan_globs") or ["**"]
    excludes = (load_config_meta_excludes_cache or {}).get("excludes", [])
    pats = [re.compile(p) for p in (fam.get("scan") or {}).get("patterns") or []]
    compliant = [re.compile(p) for p in (fam.get("scan") or {}).get("compliant_patterns") or []]
    wl_index = {(e["path"], int(e["line"])): e for e in whitelist}
    hits, wl_applied, used = [], [], set()
    for rel in walk_files(root, excludes):
        if not any(glob_match(rel, g) for g in full):
            continue
        f = root / rel
        try:
            lines = f.read_text(encoding="utf-8").splitlines()
        except (UnicodeDecodeError, OSError):
            continue
        for no, line in production_lines(lines):
            if not any(p.search(line) for p in pats):
                continue
            if any(c.search(line) for c in compliant):
                continue
            ent = wl_index.get((rel, no))
            if ent:
                used.add((rel, no))
                if debt_ids and ent["debt"] not in debt_ids:
                    hits.append({"path": rel, "line": no, "reason": f"白名单债号 {ent['debt']} 不在册（指针失联）",
                                 "text": line.strip()})
                    continue
                if line.strip() != str(ent["pattern"]).strip():
                    hits.append({"path": rel, "line": no,
                                 "reason": f"白名单 pattern 与实测行不一致（锚定旧行/旧形态）: "
                                           f"期望 {ent['pattern']!r} 实得 {line.strip()!r}",
                                 "text": line.strip()})
                    continue
                wl_applied.append({"path": rel, "line": no, "debt": ent["debt"], "text": line.strip()})
                continue
            hits.append({"path": rel, "line": no,
                         "reason": f"族模式命中（suggested_debt: {next_debt_id(debt_ids)}）",
                         "text": line.strip()})
    for e in whitelist:
        if (e["path"], int(e["line"])) not in used:
            note(f"WL-STALE {e['path']}:{e['line']} 白名单条目未被本族模式命中（{e['debt']}）"
                 f"——或由共读方 budget-cast-lint 消费（两门检测强度不同属设计），或站点已修/行号漂移请销项")
    return {"hits": hits, "whitelist_applied": wl_applied}


def scan_dual(fam: dict, root: Path, whitelist: list[dict], debt_ids: set[str]) -> dict:
    """双法族（pyc-shadow）：法 A=文件名枚举；法 B=pyc_header_check.py 内容 magic 识别。
    两法路径集必须 both_zero_or_identical_sets，否则 exit 3。"""
    excludes = (load_config_meta_excludes_cache or {}).get("excludes", [])
    roots = (load_config_meta_excludes_cache or {}).get("scan_roots_default", ["."])
    # —— 法 A：文件名枚举（*.pyc + __pycache__ 目录；等价草案 find 命令 + excludes 剪枝）——
    set_a: set[str] = set()
    for rel in walk_files(root, excludes):
        base = os.path.basename(rel)
        if base.endswith(".pyc"):
            set_a.add(rel)
        elif base == "__pycache__":
            set_a.add(rel + "/")
    # —— 法 B：内容 magic 识别（独立代码路径：ci/pyc_header_check.py 子进程）——
    checker = root / "ci" / "pyc_header_check.py"
    if not checker.is_file():
        bail(2, f"双法族缺少法 B 执行体: {checker}")
    cmd = [sys.executable, str(checker), "--root", str(root), "--roots", *roots, "--json"]
    try:
        r = subprocess.run(cmd, cwd=root, capture_output=True, text=True, timeout=300)
    except subprocess.TimeoutExpired:
        bail(3, "法 B（pyc_header_check.py）超时 300s——双法交叉无法完成，按红处置")
    if r.returncode != 0:
        bail(3, f"法 B 非零退出（rc={r.returncode}）: {r.stderr.strip()[:500]}")
    try:
        bdata = json.loads(r.stdout)
    except json.JSONDecodeError:
        bail(3, f"法 B 输出非 JSON: {r.stdout[:300]}")
    set_b = set(bdata.get("pyc_by_content", []))
    agree = (set_a == set_b)
    hits = []
    if not agree:
        only_a = sorted(set_a - set_b)
        only_b = sorted(set_b - set_a)
        note(f"双法不一致：仅法 A {only_a}；仅法 B {only_b}（改名规避枚举=红，复盘 C10-4②）")
    else:
        for rel in sorted(set_a):
            ent = None
            hits.append({"path": rel.rstrip("/"), "line": 0,
                         "reason": "pyc 影子执行面在场（双法一致确认）", "text": rel})
    return {
        "hits": hits, "whitelist_applied": [],
        "method_a_summary": {"method": "A-name-enumeration", "count": len(set_a), "paths": sorted(set_a)},
        "method_b_summary": {"method": "B-content-magic", "count": len(set_b),
                             "paths": sorted(set_b), "stale": bdata.get("stale", []),
                             "orphan": bdata.get("orphan", []), "scanned_files": bdata.get("scanned_files")},
        "methods_agree": agree,
    }


load_config_meta_excludes_cache: dict = {}


def write_artifact(fam: dict, root: Path, batch: str, result: dict, mode: str,
                   git_sha: str, scan_roots: list[str]) -> tuple[Path, Path]:
    art_tpl = fam["artifact"]
    rel = art_tpl.replace("<batch>", batch)
    out = root / rel
    out.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "family": fam["id"], "mode": mode, "batch": batch,
        "ts": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
        "git_commit": git_sha, "scan_roots": scan_roots,
        "hits": result["hits"], "whitelist_applied": result["whitelist_applied"],
        "method_a_summary": result.get("method_a_summary"),
        "method_b_summary": result.get("method_b_summary"),
    }
    out.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    side = out.with_suffix(out.suffix + ".sha256")
    side.write_text(f"{sha256_of(out)}  {out.name}\n", encoding="utf-8")
    return out, side


def cmd_validate(args) -> int:
    root = repo_root()
    cfg = load_config(resolve_path(args.config))
    wl_json = resolve_path(args.whitelist_json)
    debt_ids = load_debt_register(resolve_path(args.debt_register))
    errs = validate_config(cfg, debt_ids, wl_json)
    if errs:
        for e in errs:
            print(f"  SCHEMA-FAIL {e}")
        print(f"== fix-scan --validate: {len(errs)} 项违例")
        return 2
    fams = [f["id"] for f in cfg["families"]]
    print(f"== fix-scan --validate: schema 通过（{len(fams)} 族: {', '.join(fams)}；"
          f"债册在册 {len(debt_ids)} 号）")
    return 0


def cmd_run(args) -> int:
    root = repo_root()
    global load_config_meta_excludes_cache
    cfg = load_config(resolve_path(args.config))
    meta = cfg.get("meta") or {}
    load_config_meta_excludes_cache = {
        "excludes": meta.get("excludes") or [],
        "scan_roots_default": meta.get("scan_roots_default") or ["."],
    }
    wl_json = resolve_path(args.whitelist_json)
    debt_ids = load_debt_register(resolve_path(args.debt_register))
    errs = validate_config(cfg, debt_ids, wl_json)
    if errs:
        for e in errs:
            print(f"  SCHEMA-FAIL {e}")
        print(f"== fix-scan: 配置校验红（{len(errs)} 项），不进入扫描")
        return 2
    scan_roots = load_config_meta_excludes_cache["scan_roots_default"]

    mode = args.mode
    triggered_note = "全族全量（full 档）"
    if mode == "diff":
        changed = changed_files(root, args.diff_base)
        if changed is None:
            note(f"WARN: diff-base {args.diff_base} 不可解析——保守按 full 全族跑（宁多扫不漏扫）")
            mode = "full"
            triggered_note = "diff-base 缺失回落 full"
        else:
            fams_in = [f for f in cfg["families"]
                       if any(glob_match(c, g) for c in changed for g in (f.get("trigger_globs") or []))]
            skipped = [f["id"] for f in cfg["families"] if f["id"] not in {x["id"] for x in fams_in}]
            triggered_note = (f"diff 档触发 {len(fams_in)}/{len(cfg['families'])} 族"
                              f"（base={args.diff_base}，changed={len(changed)} 文件；"
                              f"跳过: {skipped or '无'}）")
            cfg = dict(cfg)
            cfg["families"] = fams_in
    note(f"mode={mode}：{triggered_note}")

    if args.family:
        cfg = dict(cfg)
        cfg["families"] = [f for f in cfg["families"] if f["id"] == args.family]
        if not cfg["families"]:
            bail(2, f"--family {args.family} 不在触发/在册族集合内")

    git_sha = git_commit(root)
    batch = f"{dt.datetime.now(dt.timezone.utc):%Y-%m-%d}-{git_sha[:8]}"
    rc = 0
    for fam in cfg["families"]:
        # 白名单来源：sat-cast 共读 JSON 单一正本；其余族用 yaml 内联
        if fam["id"] == "sat-cast" and wl_json.is_file():
            jdata = json.loads(wl_json.read_text(encoding="utf-8"))
            wl = jdata.get("entries", jdata) if isinstance(jdata, dict) else jdata
        else:
            wl = fam.get("whitelist") or []
        scan = fam.get("scan") or {}
        if scan.get("method_a") or scan.get("method_b"):
            result = scan_dual(fam, root, wl, debt_ids)
            if not result["methods_agree"]:
                print(f"  DUAL-METHOD-DISAGREE {fam['id']}: 两法结论不一致（单方法结论不受理，复盘 C10-4②）")
                rc = 3
        else:
            result = scan_single(fam, root, wl, debt_ids)
        art, side = write_artifact(fam, root, batch, result, mode, git_sha, scan_roots)
        n_hits = len(result["hits"])
        n_wl = len(result["whitelist_applied"])
        if n_hits:
            for h in result["hits"]:
                print(f"  VIOLATION {fam['id']} {h['path']}:{h['line']}: {h['reason']}: {h['text']}")
            rc = rc or 1
        print(f"== fix-scan {fam['id']}: {n_hits} 白名单外命中 / {n_wl} 白名单生效 → 产物 {art.relative_to(root)} (+.sha256)")
    print(f"== fix-scan --run: batch={batch} mode={mode} exit={rc}")
    return rc


def cmd_verify_artifacts(args) -> int:
    """合流门钩子：最新 batch 的各族产物必须存在且 sha256 与 sidecar 一致（缺产物=blocked）。"""
    root = repo_root()
    cfg = load_config(resolve_path(args.config))
    scans_dir = root / "redteam" / "scans"
    if not scans_dir.is_dir():
        print(f"== fix-scan --verify-artifacts: {scans_dir} 不在盘——缺产物=合流 blocked")
        return 1
    batches = sorted(p.name for p in scans_dir.iterdir() if p.is_dir())
    if not batches:
        print("== fix-scan --verify-artifacts: 无任何 batch 目录——缺产物=合流 blocked")
        return 1
    batch = batches[-1]
    rc = 0
    for fam in cfg["families"]:
        rel = fam["artifact"].replace("<batch>", batch)
        art = root / rel
        side = art.with_suffix(art.suffix + ".sha256")
        if not art.is_file() or not side.is_file():
            print(f"  BLOCKED {fam['id']}: 缺产物 {rel}（或 .sha256）——合流 blocked")
            rc = 1
            continue
        want = side.read_text(encoding="utf-8").split()[0]
        got = sha256_of(art)
        if want != got:
            print(f"  BLOCKED {fam['id']}: {rel} sha256 不一致（sidecar {want[:12]}… ↔ 实算 {got[:12]}…）")
            rc = 1
        else:
            print(f"  OK {fam['id']}: {rel} 存在且 sha256 一致（{got[:12]}…，batch={batch}）")
    print(f"== fix-scan --verify-artifacts: batch={batch} exit={rc}")
    return rc


def main() -> int:
    ap = argparse.ArgumentParser(description="同族横切扫描器（G0-C 落地件）")
    ap.add_argument("--validate", action="store_true", help="schema+白名单校验（违例 exit 2）")
    ap.add_argument("--run", action="store_true", help="逐族扫描并落产物")
    ap.add_argument("--verify-artifacts", action="store_true", help="合流门钩子：最新 batch 产物+sha256 核查")
    ap.add_argument("--config", default=DEFAULT_CONFIG)
    ap.add_argument("--mode", choices=["diff", "full"], default="diff")
    # 基点缺省 HEAD~1（本次改动面）——与同仓 mutations 门 MUT_BASE 缺省同款（run.sh
    # 「基点缺省 HEAD~1；基点不可解析时 fail-safe 判 full」先例）；origin/main 是
    # fix3 代际对照线，取它当基点=diff 档路径过滤失效。不可解析时 changed_files
    # 返 None → 回落 full（宁多扫不漏扫），fail-safe 语义保持不变。
    ap.add_argument("--diff-base", default="HEAD~1")
    ap.add_argument("--family", default=None, help="只跑指定族（配合 --run）")
    ap.add_argument("--whitelist-json", default=DEFAULT_WHITELIST_JSON,
                    help="sat-cast 白名单单一正本（ci/budget-cast-whitelist.json）")
    ap.add_argument("--debt-register", default=DEFAULT_DEBT_REGISTER,
                    help="债务登记册（仓内镜像 ci/debt-register.md；env DEBT_REGISTER 可覆盖）")
    args = ap.parse_args()
    if os.environ.get("DEBT_REGISTER"):
        args.debt_register = os.environ["DEBT_REGISTER"]
    if args.validate:
        return cmd_validate(args)
    if args.verify_artifacts:
        return cmd_verify_artifacts(args)
    if args.run:
        return cmd_run(args)
    ap.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
