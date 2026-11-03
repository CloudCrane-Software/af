#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""branches_check.py — branches.yaml 分支覆盖夹具断言（G0-CI 机制批 · 子代理 D）

SPEC-1（计划/研究组/G0-CI机制批-实施规格-20261027/D/1-branches/SPEC.md）实施批落地：
校验脚本本体属实施批产出（SPEC ⑧.3）。本文件实现 ① 节断言规则的**常跑面**与
schema/scope 面；nightly 面（llvm-cov 分支报告对账）仅在 coverage 报告在场时判定。

规则↔实现（逐条对应 SPEC ①）：
  R0 schema/scope：顶层 schema 必须 branches-v1；scope.include_globs 每个 glob
     ≥1 命中（零命中=scope 缩水 → 红）。
  R1 锚有效性（常跑秒级）：每条 branches[] 的 file 存在（相对 src-root）、line 为
     正整数、file:line 行文本以 anchor_prefix 开头或包含之——行号漂移/锚文本改
     即红（stale_anchor_is_red，防静默失效）。
  R2 夹具实跑对账（常跑秒级）：covered 条目 positive≥min_positive 且
     negative≥min_negative（新分支无夹具即红）；每个 fixture 的 test 名
     （crate::file::fn 三段）须落在 cargo test 名单的对应测试文件段内（缺席即红
     ——防夹具名虚记，C10 类登记虚记在夹具面的镜像）；fixture path:line 落在 fn 行。
  R3 pending 治理：pending/pending-negative 必带 gap_reason+gap_ticket；
     pending-negative 的 positive 仍须 ≥min_positive；宽限窗（grace_until）后
     pending 仍红；宽限窗缺失而又有 pending 条目 → 红。
  R5 不产码说明：error_codes 为空 ⇒ semantics 须含「不产码/无码/不产错误码」
     字样（YAML 注释机不可读，理由必须进正文）。
  R4 无裸分支/分支级覆盖（nightly 面）：仅当 coverage 报告在场时判定——登记分支
     有正例但该行分支覆盖计数全 0 → 红（夹具名存实亡，C3 字面满足的夹具版）；
     报告内 scope 分支区域数 vs 登记数的差只作 INFO（逐支对账待 nightly 首跑用
     真实 llvm-cov 输出校准后升红，SPEC ⑧.1/⑧.5 两阶段设计）。报告格式不可识别
     → WARN（不误红）。报告缺席（常跑）= INFO，不判。

退出码：
  0 = 全过（锚有效、无 covered 缺夹具、夹具名在名单、scope glob 有命中、
       pending 在宽限窗内、coverage 面无红）
  1 = 任一红项（见上各规则）
  2 = 仅 WARN（--text-mode 降级 / coverage 报告在场但格式不可识别）——由调用方
      定义是否改 OVERALL（run.sh step14 落 SUMMARY WARN 行、不改 OVERALL；
       禁两态死代码：WARN 必须显式登记）。

用法：
  python3 ci/branches_check.py [--branches PATH] [--src-root PATH]
      [--test-list-file PATH] [--coverage-report PATH] [--out PATH] [--text-mode]
  缺省路径全部相对本脚本所在仓库根（__file__ 推导——root 钉死到本工作树；红队
  P2-2 纪律：不得对 agent黑灯工厂/af/ 旧分支局部副本跑，会出假绿）。
  --test-list-file：`cargo test --workspace -- --list` 输出存档（缺省时本脚本
     自跑该命令；step3 已全域构建后仅执行各测试二进制 --list，秒级）。
  --text-mode：yaml 库缺席时的文本降级校验（必填键逐条在场性 + 锚三元组正则抽取
     核对；夹具名单对账降级为「名在文书在场」——如实标 WARN）。
"""
import argparse
import glob as globlib
import json
import re
import subprocess
import sys
from datetime import date, datetime
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
NO_CODE_MARKERS = ("不产码", "无码", "不产错误码", "不产出错误码", "不产错误")
ID_RE = re.compile(r"^[A-Z0-9-]+$")
FIXTURE_NAME_RE = re.compile(r"^[a-z0-9-]+::[a-z0-9_]+::[a-z0-9_]+$")
STATUSES = ("covered", "pending", "pending-negative")


def rec(errs, warns, infos, kind, msg):
    {"err": errs, "warn": warns, "info": infos}[kind].append(msg)


# ---------------------------------------------------------------- yaml 载入
def load_branches(path, text_mode):
    text = Path(path).read_text(encoding="utf-8")
    if text_mode:
        return "text", text
    try:
        import yaml  # type: ignore

        return "yaml", yaml.safe_load(text)
    except ImportError:
        return "text", text


# ---------------------------------------------------------------- cargo 名单
def get_test_list(src_root, list_file):
    """返回 ({target: set(test 名)}, note)。target = tests/x.rs / src/lib.rs 等。"""
    if list_file:
        return parse_test_list(Path(list_file).read_text(encoding="utf-8")), \
            f"名单来源：--test-list-file {list_file}"
    cmd = ["cargo", "test", "--workspace", "--", "--list"]
    try:
        # 注意：`Running <target> (path)` 段头走 stderr、`name: test` 行走 stdout
        # （cargo 进度面在 stderr）——必须合并捕获，否则名单段为空、夹具名全误红。
        out = subprocess.run(
            cmd, cwd=src_root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, timeout=900,
        ).stdout
    except (OSError, subprocess.TimeoutExpired) as e:
        return {}, f"cargo test --list 调用失败：{e}（夹具名对账无法执行）"
    return parse_test_list(out), f"名单来源：cargo test --workspace -- --list（{src_root}）"


def parse_test_list(text):
    """解析 `cargo test -- --list` 输出：`Running [unittests] <target> (path)` 起段，
    `name: test` 行入段。Doc-tests 段不含 'Running … (' 头，自然跳过。"""
    sections = {}
    current = None
    for line in text.splitlines():
        m = re.match(r"\s*Running (?:\S+ )?(\S+) \(", line)
        if m:
            current = m.group(1)
            sections.setdefault(current, set())
            continue
        m = re.match(r"^(\S+): test\s*$", line)
        if m and current is not None:
            sections[current].add(m.group(1))
    return sections


def fixture_in_list(name, sections):
    """crate::file::fn 三段名 → 是否落在名单对应测试文件段。返回 (ok, detail)。"""
    if not FIXTURE_NAME_RE.match(name):
        return False, f"夹具名 {name!r} 须为 crate::file::fn 三段形"
    _crate, tfile, fn = name.split("::")
    cands = [
        t
        for t in sections
        if t in (f"tests/{tfile}.rs", f"src/{tfile}.rs", f"src/bin/{tfile}.rs")
        or (tfile == "lib" and t == "src/lib.rs")
    ]
    if not cands:
        return False, f"cargo test 名单无 tests/{tfile}.rs（或 src/{tfile}.rs）段"
    names = set().union(*(sections[t] for t in cands))
    if fn not in names:
        return False, f"test {fn} 不在名单段 {sorted(cands)} 内"
    return True, ""


# ---------------------------------------------------------------- coverage（nightly 面）
def scan_coverage(obj):
    """容错扫描 llvm-cov JSON 导出：收集含 "filename" 的 dict 及其分支计数。
    识别形状（nightly 首跑须用真实 llvm-cov --show-branch-count 导出校准后收紧）：
      - 分支列表：dict["branches"] = [{"line": L, "col": C, "count": N}, ...]
      - 文件级计数：dict["branch_count"] / dict["count"]（int）
    返回 {filename: {"per_branch": {line: [count,...]}, "total": int|None}}；
    不可识别（无任何 filename 结构）→ None。"""
    out = {}
    seen_file = False

    def walk(o):
        nonlocal seen_file
        if isinstance(o, dict):
            fn = o.get("filename")
            if isinstance(fn, str):
                seen_file = True
                ent = out.setdefault(fn, {"per_branch": {}, "total": None})
                brs = o.get("branches")
                if isinstance(brs, list):
                    for b in brs:
                        if isinstance(b, dict) and isinstance(b.get("count"), int):
                            ln = b.get("line", b.get("lineno"))
                            if isinstance(ln, int):
                                ent["per_branch"].setdefault(ln, []).append(b["count"])
                for k in ("branch_count", "count"):
                    v = o.get(k)
                    if isinstance(v, int):
                        ent["total"] = v
                        break
            for v in o.values():
                walk(v)
        elif isinstance(o, list):
            for v in o:
                walk(v)

    walk(obj)
    return out if seen_file else None


# ---------------------------------------------------------------- 文本降级模式
def check_text(text, src_root, errs, warns, infos):
    if "schema: branches-v1" not in text:
        errs.append("文本模式：未见『schema: branches-v1』头")
    entries = re.split(r"\n  - id: ", text)[1:]
    if not entries:
        errs.append("文本模式：未识别到 branches[] 条目")
    n_pos = n_neg = 0
    for b in entries:
        bid = b.split("\n")[0].strip()
        if not ID_RE.match(bid):
            errs.append(f"文本模式：分支 id {bid!r} 不合 ^[A-Z0-9-]+$")
        f = re.search(r"\n    file: (\S+)", b)
        ln = re.search(r"\n    line: (\d+)", b)
        ap = re.search(r"\n    anchor_prefix: (?:'([^']*)'|\"([^\"]*)\")", b)
        st = re.search(r"\n    status: (\S+)", b)
        for label, m in (("file", f), ("line", ln), ("anchor_prefix", ap), ("status", st)):
            if not m:
                errs.append(f"文本模式：{bid} 缺字段 {label}")
        if not (f and ln and ap):
            continue
        p = (src_root / f.group(1))
        if not p.is_file():
            errs.append(f"文本模式：{bid} file 不存在 {f.group(1)}")
            continue
        row = p.read_text(encoding="utf-8").splitlines()
        i = int(ln.group(1))
        if not (1 <= i <= len(row)):
            errs.append(f"文本模式：{bid} line {i} 越界 {f.group(1)}")
            continue
        prefix = (ap.group(1) if ap.group(1) is not None else ap.group(2)).strip()
        if not (row[i - 1].strip().startswith(prefix) or prefix in row[i - 1]):
            errs.append(f"文本模式：{bid} 锚失配 {f.group(1)}:{i}（anchor_prefix 不在行文本）")
        n_pos += len(re.findall(r"- test: \S+", b.split("negative:")[0]))
        n_neg += len(re.findall(r"- test: \S+", b.split("negative:")[-1])) if "negative:" in b else 0
        if st and st.group(1).startswith("pending"):
            if "gap_reason" not in b or "gap_ticket" not in b:
                errs.append(f"文本模式：{bid} pending 缺 gap_reason/gap_ticket")
    infos.append(f"文本模式：登记 {len(entries)} 分支（正例条目 ~{n_pos}、负例条目 ~{n_neg}）")
    warns.append(
        "文本模式：yaml 库缺席，结构完整性为降级校验（锚三元组已核；夹具名 cargo "
        "test 名单对账与 scope glob 命中未执行）——如实标注"
    )


# ---------------------------------------------------------------- 主校验
def main():
    ap = argparse.ArgumentParser(description="branches.yaml 分支覆盖夹具断言（复盘 C4 升级⑤）")
    ap.add_argument("--branches", default=str(REPO / "ci" / "branches.yaml"))
    ap.add_argument("--src-root", default=str(REPO))
    ap.add_argument("--test-list-file", default=None,
                    help="cargo test --workspace -- --list 输出存档（缺省自跑）")
    ap.add_argument("--coverage-report", default=None,
                    help="llvm-cov 分支报告 JSON（nightly 面；缺省 <src-root>/ci/out/branch-coverage.json，"
                         "文件缺席则 nightly 面不判）")
    ap.add_argument("--out", default=None, help="对账明细报告输出路径（缺省 <src-root>/ci/out/branches-report.txt）")
    ap.add_argument("--text-mode", action="store_true", help="yaml 库缺席时文本降级校验")
    args = ap.parse_args()
    src_root = Path(args.src_root).resolve()
    cov_path = Path(args.coverage_report) if args.coverage_report else src_root / "ci" / "out" / "branch-coverage.json"
    out_path = Path(args.out) if args.out else src_root / "ci" / "out" / "branches-report.txt"

    errs, warns, infos, lines = [], [], [], []
    mode, data = load_branches(args.branches, args.text_mode)

    if mode == "text":
        check_text(data, src_root, errs, warns, infos)
    else:
        if not isinstance(data, dict) or data.get("schema") != "branches-v1":
            errs.append("顶层 schema 字段必须为 branches-v1")
            data = {}
        scope = data.get("scope") or {}
        scope_files = []
        for g in scope.get("include_globs") or []:
            hits = globlib.glob(str(src_root / g))
            if not hits:
                errs.append(f"scope glob 零命中（防 scope 缩水）：{g}")
            scope_files.extend(hits)
        infos.append(f"判定面文件集：{len(scope_files)} 个（globs {len(scope.get('include_globs') or [])} 条）")
        rules = data.get("rules") or {}
        min_pos = int(rules.get("min_positive", 1))
        min_neg = int(rules.get("min_negative", 1))
        grace = (rules.get("pending_gap_policy") or {}).get("grace_until")

        entries = data.get("branches") or []
        if not entries:
            errs.append("branches[] 为空——无登记的覆盖夹具不受理（反空过）")
        ids = [e.get("id") for e in entries]
        for i in ids:
            if not (isinstance(i, str) and ID_RE.match(i)):
                errs.append(f"分支 id {i!r} 不合 ^[A-Z0-9-]+$")
        dup = {i for i in ids if ids.count(i) > 1}
        if dup:
            errs.append(f"分支 id 重复：{sorted(dup)}")

        sections, note = get_test_list(src_root, args.test_list_file)
        infos.append(note + f"（测试文件段 {len(sections)} 个）")
        if not sections and not args.test_list_file:
            errs.append("cargo test 名单为空——夹具名对账无法执行（环境缺 cargo？）")

        pendings = []
        for e in entries:
            bid = e.get("id", "?")
            f = e.get("file")
            ln = e.get("line")
            ap = e.get("anchor_prefix", "")
            st = e.get("status")
            sem = (e.get("semantics") or "").strip()
            codes = e.get("error_codes")
            fx = e.get("fixtures") or {}
            pos = fx.get("positive") or []
            neg = fx.get("negative") or []
            row_txt = ""
            # —— R1 锚有效性 ——
            if not isinstance(f, str) or not f:
                errs.append(f"{bid}: 缺 file")
            elif Path(f).is_absolute() or ".." in Path(f).parts:
                errs.append(f"{bid}: file 必须为相对 src-root 路径（{f}）")
            elif not (src_root / f).is_file():
                errs.append(f"{bid}: file 不存在（锚失效）{f}")
            elif not (isinstance(ln, int) and ln >= 1):
                errs.append(f"{bid}: line 必须为正整数（{ln!r}）")
            else:
                rows = (src_root / f).read_text(encoding="utf-8").splitlines()
                if ln > len(rows):
                    errs.append(f"{bid}: line {ln} 越界 {f}（共 {len(rows)} 行）")
                else:
                    row_txt = rows[ln - 1]
                    p = str(ap).strip()
                    if not p:
                        errs.append(f"{bid}: 缺 anchor_prefix")
                    elif not (row_txt.strip().startswith(p) or p in row_txt):
                        errs.append(
                            f"{bid}: 锚失配 {f}:{ln}（anchor_prefix={p!r} 不在行文本；"
                            f"实际行：{row_txt.strip()[:60]!r}）——行号漂移即红"
                        )
            # —— R5 不产码说明 ——
            if isinstance(codes, list) and not codes:
                if not any(m in sem for m in NO_CODE_MARKERS):
                    errs.append(f"{bid}: error_codes 为空但 semantics 未说明不产码原因（须含「不产码/无码/不产错误码」）")
            elif not isinstance(codes, list):
                errs.append(f"{bid}: error_codes 必须为列表")
            if not sem:
                errs.append(f"{bid}: semantics 不能空")
            # —— R2 夹具对账（名在名单 + path:line 在 fn 行）——
            for kind, items, need in (("positive", pos, min_pos), ("negative", neg, min_neg)):
                for it in items:
                    t = it.get("test", "")
                    ok, detail = fixture_in_list(t, sections) if sections else (False, "名单缺失")
                    if not ok:
                        errs.append(f"{bid}: {kind} 夹具 {t} —— {detail}")
                    p = it.get("path", "")
                    fl = it.get("line")
                    if not isinstance(p, str) or not (src_root / p).is_file():
                        errs.append(f"{bid}: {kind} 夹具 path 不存在 {p!r}")
                    elif not (isinstance(fl, int) and fl >= 1):
                        errs.append(f"{bid}: {kind} 夹具 line 必须为正整数（{fl!r}）")
                    else:
                        frows = (src_root / p).read_text(encoding="utf-8").splitlines()
                        if fl > len(frows) or f"fn {t.split('::')[-1]}" not in frows[fl - 1]:
                            errs.append(
                                f"{bid}: {kind} 夹具锚失配 {p}:{fl}（该行不是 fn {t.split('::')[-1]}）"
                            )
            # —— R2/R3 状态与缺口治理 ——
            if st not in STATUSES:
                errs.append(f"{bid}: status {st!r} 不在 {STATUSES}")
            if st == "covered":
                if len(pos) < min_pos or len(neg) < min_neg:
                    errs.append(
                        f"{bid}: covered 缺夹具（正例 {len(pos)}/{min_pos}、负例 {len(neg)}/{min_neg}）——新分支无夹具即红"
                    )
            elif st in ("pending", "pending-negative"):
                pendings.append(bid)
                if not (e.get("gap_reason") or "").strip():
                    errs.append(f"{bid}: pending 缺 gap_reason")
                if not (e.get("gap_ticket") or "").strip():
                    errs.append(f"{bid}: pending 缺 gap_ticket（接 C2 债号指针注册表）")
                if st == "pending-negative" and len(pos) < min_pos:
                    errs.append(f"{bid}: pending-negative 的正例仍须 ≥{min_pos}（现 {len(pos)}）")
            lines.append(
                f"{'OK  ' if not any(x.startswith(bid + ':') or x.startswith(bid + ' ') for x in errs) else 'BAD '}"
                f"{bid:34s} [{st}] 正{len(pos)}/负{len(neg)}  锚={f}:{ln}"
            )
        if pendings:
            if not grace:
                errs.append(f"pending 条目 {pendings} 在场但 rules.pending_gap_policy.grace_until 未定义")
            else:
                try:
                    gd = datetime.strptime(str(grace), "%Y-%m-%d").date()
                except ValueError:
                    errs.append(f"grace_until {grace!r} 不可解析（须 YYYY-MM-DD）")
                else:
                    if date.today() > gd:
                        errs.append(
                            f"pending 超宽限窗（{pendings}；grace_until={grace}，今日 {date.today()}）——复盘 C4：矩阵不从判定分支导出就仍是换层皮"
                        )
                    else:
                        infos.append(f"pending 条目 {pendings} 在宽限窗内（至 {grace}）")

        # —— R4 coverage  nightly 面 ——
        if cov_path.is_file():
            try:
                cov = json.loads(cov_path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError) as ex:
                warns.append(f"coverage 报告在场但不可解析：{ex}（nightly 面不判，待校准）")
            else:
                scanned = scan_coverage(cov)
                if scanned is None:
                    warns.append(
                        "coverage 报告格式不可识别（无 filename 结构）——nightly 面对账格式待真实 llvm-cov "
                        "--show-branch-count 导出校准（SPEC ⑧.1/⑧.5 两阶段设计：常跑对名、nightly 对覆盖）"
                    )
                else:
                    scope_rel = {str(Path(s).resolve().relative_to(src_root)) for s in scope_files}
                    n_regions = 0
                    for e in entries:
                        f = e.get("file")
                        if f not in scanned:
                            infos.append(f"{e.get('id')}: coverage 报告未覆盖 {f}（nightly 人工核）")
                            continue
                        ent = scanned[f]
                        n_regions += len(ent["per_branch"])
                        ln = e.get("line")
                        counts = ent["per_branch"].get(ln, [])
                        if counts and all(c == 0 for c in counts) and (e.get("fixtures") or {}).get("positive"):
                            errs.append(
                                f"{e.get('id')}: 正例夹具在场但 {f}:{ln} 分支覆盖计数全 0——夹具名存实亡（C3 字面满足的夹具版）"
                            )
                    for f in sorted(scope_rel & set(scanned)):
                        n_regions += len(scanned[f]["per_branch"])
                    infos.append(
                        f"coverage 面：报告覆盖 scope 文件 {len(scope_rel & set(scanned))}/{len(scope_rel)}，"
                        f"可识别分支区域 {n_regions} 个 vs 登记 {len(entries)} 个——逐支裸分支对账待 nightly 校准后升红"
                    )
        else:
            infos.append("coverage 报告缺席（ci/out/branch-coverage.json）——nightly 面不判（常跑对名、nightly 对覆盖）")

    # ---- 输出 ----
    print("== branches_check：branches.yaml 分支覆盖夹具对账（SPEC-1 常跑面）")
    for ln in lines:
        print("   " + ln)
    for i in infos:
        print(f"[INFO] {i}")
    for w in warns:
        print(f"[WARN] {w}")
    for e in errs:
        print(f"[FAIL] {e}")
    n_branches = len(data.get("branches") or []) if isinstance(data, dict) else 0
    print(f"== branches_check: 登记 {n_branches} 分支；红项 {len(errs)}；WARN {len(warns)}")
    if out_path:
        out_path.parent.mkdir(parents=True, exist_ok=True)
        body = [
            "# branches 对账明细（ci/branches_check.py 机生成）",
            f"# branches={args.branches} src-root={src_root}",
            *lines,
            "",
            *[f"[INFO] {i}" for i in infos],
            *[f"[WARN] {w}" for w in warns],
            *[f"[FAIL] {e}" for e in errs],
            f"== 登记 {n_branches} 分支；红项 {len(errs)}；WARN {len(warns)}",
        ]
        out_path.write_text("\n".join(body) + "\n", encoding="utf-8")
        print(f"== 对账明细落档：{out_path}")
    if errs:
        return 1
    if warns:
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
