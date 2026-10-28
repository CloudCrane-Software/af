#!/usr/bin/env python3
"""budget_cast_lint.py — 预算字段 cast 直转全仓 lint + 预算面 `_ =>` 兜底臂 fail-loud 门
（G0-C 施工批落地件；草案=计划/研究组/G0-CI机制批-实施规格-20261027/C/3-budget-cast-lint/。
出处：红队复盘 C11-4①②（计划/研究组/红队复盘-逃逸分析与防再发-20261027.md:260）+ C9-4③（:230），
开发计划 §2.2 先修项（report_unit.rs:1112/1262 已由 G0-C 前置修复饱和化）。

两道子门：
  A. 预算字段名单上的 `as i64|as i32|as u32` 直转 = 红（饱和/钳位合规形态除外，白名单条目须挂债号）。
  B. 预算上下文里的 `_ => {}` / `_ => continue` 兜底臂 = 红（必须显式 fail-loud：return Err/拒收/panic）。

退出码：0=干净；1=有违规或债号指针失联（附 suggested_debt 建议号）；2=白名单/配置自身错误。
白名单单一正本：ci/budget-cast-whitelist.json（fix-scan.yaml sat-cast 族共读——见 SPEC 1 ⑧-6，
防两份白名单漂移）。债册：ci/debt-register.md（正本 计划/技术债登记.md 的仓内镜像+sha256 对账，
见 SPEC 2/3 ⑧-1；正本在 agent黑灯工厂 工作区、不在 af 仓，CI 只能消费镜像）。

root 钉死（红队第二轮 P2-2）：--root 必须指向真实 af 仓工作树（G0-C=本树绝对路径由调用方给定）；
对工作区根 agent黑灯工厂/af/ 旧分支局部副本（无 crates suite）跑会得 0 违规**假绿**，禁止。
只读扫描，不改任何文件；不跑 cargo、不跑门。
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

# ---- 预算字段名单（复盘 C9-4③:230 底表 + 盘上补充 swev_unit.rs:63/69/196-198、
#      report_unit.rs:803-810、graph_unit.rs:375 亲测字段）-------------------
BUDGET_FIELDS = [
    "cost_cap", "spend_cap_cents", "budget_ledger_cents", "model_calls",
    "cents_used", "ring_spent_cents", "turns", "tokens_input", "tokens_output",
    "tokens_cache_read", "tokens_total", "clock_ms", "node_spend_cap",
]

CAST_RE = re.compile(r"\bas\s+(i64|i32|u32)\b")
# 合规形态：饱和换算 / 先 min 钳位再转（盘上对照 report_unit.rs:737 / :754；
# G0-C 前置修复后 :1117/:1267 两处迁入本形态）
COMPLIANT_RES = [
    re.compile(r"::try_from\([^)]*\)\s*\.unwrap_or\((?:i64|u32)::MAX\)"),
    re.compile(r"\.min\((?:i64|u32)::MAX(?:\s+as\s+u64)?\)\s+as\s+(?:i64|u32)"),
    re.compile(r"u32::MAX\s+as\s+u64"),          # 钳位常量自转（report_unit.rs:754 左半）
    re.compile(r"i64::MAX(?:\.min\([^)]*\))?"),  # 饱和哨兵参与 min（report_unit.rs:985）
]
FALLBACK_ARM_RE = re.compile(r"_\s*=>\s*(?:\{\}|\(\)|continue\b)")
# 兜底臂视为 fail-loud 的证据（同 match 块内出现即不算违规）
FAIL_LOUD_RE = re.compile(r"return\s+(Err|self\.fail)|panic!|unreachable!|EX3\d{3}|OR1\d{3}")
BUDGET_CTX_RE = re.compile(
    r"[Bb]udget|[Ss]pend|BudgetDimension|\bcap\b|_cap|cost_cap|cents|tokens_|clock_ms|预算|配额|帽"
)
BUDGET_CTX_WINDOW = 40  # 兜底臂向上下文找预算关键词的窗口（行）
MATCH_RE = re.compile(r"match\s+([^{\n]+)\{?")
COMMENT_RE = re.compile(r"^\s*(//|/\*|\*)")
CFG_TEST_RE = re.compile(r"#\[cfg\(test\)\]")

DEBT_ID_RE = re.compile(r"^D-\d+$")
SCAN_GLOBS = ["*.rs"]
EXCLUDE_DIRS = {"target", ".git", "out", "node_modules", "tests"}  # tests 域豁免：饱和/回绕夹具属意为之（SPEC ⑧-3）


def iter_rs_files(root: Path):
    for p in sorted(root.rglob("*.rs")):
        if any(part in EXCLUDE_DIRS for part in p.parts):
            continue
        yield p


def production_lines(lines: list[str]) -> list[tuple[int, str]]:
    """(行号, 行) 生产码区：注释行剔除、`#[cfg(test)]` 之后剔除（干跑校准：graph_unit.rs
    :2781 等饱和/回绕夹具属测试意为之，:222/:1403 等为注释行散文命中）。"""
    out = []
    in_test = False
    for no, line in enumerate(lines, 1):
        if CFG_TEST_RE.search(line):
            in_test = True
        if in_test or COMMENT_RE.match(line):
            continue
        out.append((no, line))
    return out


def sha256_of(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_whitelist(path: Path) -> list[dict]:
    """白名单 JSON：[{path, line, pattern, debt, reason, expires}]；字段缺失/债号非法 → exit 2。"""
    if not path.is_file():
        return []  # 空白名单合法（默认即全红现状）；文件存在但坏 = 2
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as e:
        bail(2, f"白名单 JSON 解析失败: {path}: {e}")
    entries = data.get("entries", data) if isinstance(data, dict) else data
    if not isinstance(entries, list):
        bail(2, f"白名单结构错误（应为 list 或 {{entries: []}}）: {path}")
    for i, e in enumerate(entries):
        for k in ("path", "line", "pattern", "debt", "reason", "expires"):
            if k not in e or not str(e[k]).strip():
                bail(2, f"白名单第 {i} 条缺必填字段 {k}（无债号/无理由/无窗口条目=lint 自身红）")
        if not DEBT_ID_RE.match(str(e["debt"])):
            bail(2, f"白名单第 {i} 条 debt={e['debt']!r} 不匹配 ^D-\\d+$（无债号条目不受理）")
        if not isinstance(e["line"], int):
            bail(2, f"白名单第 {i} 条 line={e['line']!r} 非 int")
    return entries


def load_debt_register(path: Path | None) -> set[str]:
    """债册解析：`| D-xx |` 行首表格行 → 在册号集；债册缺席时跳过在册校验（见 SPEC ⑧-1）。"""
    if path is None or not path.is_file():
        return set()
    ids = set()
    for m in re.finditer(r"^\|\s*(D-\d+)\s*\|", path.read_text(encoding="utf-8"), re.M):
        ids.add(m.group(1))
    return ids


def next_debt_id(debt_ids: set[str]) -> str:
    nums = [int(d[2:]) for d in debt_ids if d.startswith("D-") and d[2:].isdigit()]
    return f"D-{(max(nums) + 1) if nums else 1}"


def bail(code: int, msg: str) -> None:
    print(f"[budget-cast-lint] FATAL: {msg}", file=sys.stderr)
    sys.exit(code)


def scan(root: Path, whitelist: list[dict], debt_ids: set[str], part_b: bool):
    violations, wl_hits = [], []
    wl_index = {(e["path"], int(e["line"])): e for e in whitelist}
    used: set[tuple[str, int]] = set()
    for f in iter_rs_files(root):
        rel = str(f.relative_to(root))
        try:
            lines = f.read_text(encoding="utf-8").splitlines()
        except (UnicodeDecodeError, OSError):
            continue
        for no, line in production_lines(lines):
            # ---- 子门 A：预算字段直转 ----
            if CAST_RE.search(line) and not any(r.search(line) for r in COMPLIANT_RES):
                ctx = " ".join(lines[max(0, no - 4):no + 2])
                if any(re.search(rf"\b{re.escape(fld)}\b", ctx) for fld in BUDGET_FIELDS):
                    ent = wl_index.get((rel, no))
                    if ent:
                        used.add((rel, no))
                        if debt_ids and ent["debt"] not in debt_ids:
                            violations.append(
                                f"{rel}:{no}: 白名单债号 {ent['debt']} 不在债务登记册在册集——指针失联即红")
                            continue
                        if line.strip() != str(ent["pattern"]).strip():
                            bail(2, f"白名单条目 ({rel}, {no}) pattern 与实测行不一致"
                                    f"（白名单锚定旧行/旧形态=配置错误）: 期望 {ent['pattern']!r} 实得 {line.strip()!r}")
                        wl_hits.append(f"{rel}:{no}: 白名单生效（{ent['debt']}）: {line.strip()}")
                        continue
                    violations.append(
                        f"{rel}:{no}: 预算字段 `as i64/i32/u32` 直转（suggested_debt: {next_debt_id(debt_ids)}）: {line.strip()}")
            # ---- 子门 B：预算上下文 `_ => {}` / `_ => continue` ----
            # 收紧（干跑校准）：最近 match 审查串本身须含预算关键词——否则 report_unit.rs:349
            # 的 HTML 剥标签 `_ => {}` 会因窗口内偶现预算词被误报。
            if part_b and FALLBACK_ARM_RE.search(line):
                scrutinee = ""
                # G0-C 加固：注释行跳过——注释里出现 "match" 字样会劫持审查串致假阴
                # （干跑实测：构造假体注释含 match 时漏报）；真实码区审查串均为代码行，
                # 本加固对在册结果零扰动。另修草案 off-by-one：回溯自 no-1 起（兜底臂
                # 紧贴 match 行时草案从 no-2 起会漏掉真正的审查串）。
                for back in range(no - 1, max(0, no - 16), -1):
                    if COMMENT_RE.match(lines[back - 1]):
                        continue
                    m = MATCH_RE.search(lines[back - 1])
                    if m:
                        scrutinee = m.group(1)
                        break
                if not BUDGET_CTX_RE.search(scrutinee):
                    continue
                window = " ".join(lines[max(0, no - 3):no + 4])
                if not FAIL_LOUD_RE.search(window):
                    ent = wl_index.get((rel, no))
                    if ent:
                        used.add((rel, no))
                        wl_hits.append(f"{rel}:{no}: 兜底臂白名单生效（{ent['debt']}）: {line.strip()}")
                        continue
                    violations.append(
                        f"{rel}:{no}: 预算上下文 `_ => {{}}`/`_ => continue` 非 fail-loud（静默丢弃=免帽，C11-4②）: {line.strip()}")
    # 未命中任何站点的白名单条目 = 陈旧条目（站点已修/行号漂移）——WARN 不致命，但可见
    for e in whitelist:
        if (e["path"], int(e["line"])) not in used:
            print(f"  WL-STALE {e['path']}:{e['line']} 白名单条目未命中任何站点"
                  f"（{e['debt']}）——站点已修复或行号漂移，请销项")
    return violations, wl_hits


def main() -> int:
    ap = argparse.ArgumentParser(description="预算字段 cast lint（G0-C 落地件）")
    ap.add_argument("--root", default=".", help="扫描根（af 仓根；G0-C=本工作树，禁指旧分支局部副本）")
    ap.add_argument("--whitelist", default="ci/budget-cast-whitelist.json")
    ap.add_argument("--debt-register", default=None,
                    help="债务登记册路径（ci/debt-register.md 仓内镜像或正本 计划/技术债登记.md）")
    ap.add_argument("--no-part-b", action="store_true", help="跳过子门 B（兜底臂门）")
    ap.add_argument("--validate-only", action="store_true", help="只校验白名单与债册指针，不扫描")
    args = ap.parse_args()

    root = Path(args.root).resolve()
    if not (root / "crates").is_dir():
        bail(2, f"--root {root} 不像 af 仓（无 crates/）——root 钉死纪律：禁对旧分支局部副本跑（假绿）")
    wl = load_whitelist(Path(args.whitelist)) if Path(args.whitelist).is_file() else []
    if not Path(args.whitelist).is_file() and not args.validate_only:
        print(f"[budget-cast-lint] NOTE: 白名单 {args.whitelist} 不在盘——按零白名单跑（现状即全红面）")
    ids = load_debt_register(Path(args.debt_register) if args.debt_register else None)
    # 债号逐条对账（SPEC ①/④-反例②）：每条白名单条目（不论是否命中）的 debt 都须在册
    if ids:
        for i, e in enumerate(wl):
            if e["debt"] not in ids:
                bail(1, f"白名单第 {i} 条（{e['path']}:{e['line']}）债号 {e['debt']} 不在债务登记册在册集——指针失联即红")
    if args.validate_only:
        print(f"白名单 {len(wl)} 条，债册在册 {len(ids)} 号：校验通过")
        return 0

    violations, wl_hits = scan(root, wl, ids, part_b=not args.no_part_b)
    for h in wl_hits:
        print(f"  WL {h}")
    for v in violations:
        print(f"  VIOLATION {v}")
    print(f"== budget-cast-lint: {len(violations)} 违规 / {len(wl_hits)} 白名单生效"
          f"（root={root}, whitelist={args.whitelist}）")
    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main())
