#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""任务包组包覆盖面门（G3 债：组包路径全覆盖核查缺）——生产写入点全集对账 + 组包回环。

背景（30-工件/剩余欠账总表-20261104 G3 行）：
  「任务包 manifest 缺席 = 不核对的未组包破窗（组包路径全覆盖核查缺）」。
  crate 内 package_manifest.json（E1 缺口②/G3 收口）只钉 task.json/plan.json 两件
  内容 hash——它不回答三件事：① 组包路径上还有没有别的生产写入点没人管；
  ② 任务包目录里多出来的文件（盘面有、manifest 无）谁发现；③ 「组包路径写入点
  全集」本身有没有被任何清单核对。本门把这三件都收口（fail-closed）。

机制（承 ci/gate_inputs_check.sh 同款「清单+对账」模式；两层 manifest 设计）：
  层1 运行时 manifest（与产出同域，已在 crate 内，禁改面）：
      <task_dir>/package_manifest.json——逐包产物身份（task/plan sha256 +
      plan_id + compiler_version），由组包写入接线本身产出（construction.rs
      assemble_construction_package → package.rs write_package_manifest）。
  层2 判据面注册表（本批新增正本）：
      ci/baselines/task-package-writepoints.json——全仓**生产写入点全集**
      （crates/*/src 下全部磁盘产物写入调用点，逐 (file, fn) 造册 + 域分类；
      任务包组包域逐产物字面量造册）。sha256 自钉于下方 REGISTRY_SHA256
      （改表=脚本 diff 同批评审，与 GATE_INPUTS 清单同纪律）。

  check 五段（任一红即 exit 1）：
    ① 注册表自钉对账（表缺席/坏形/哈希漂移=红——改判据即改门）；
    ② 写入点全集双向对账：扫描面内每个生产写入点必须在册（**覆盖不到=红**，
       本门核心）；在册条目必须仍对应真实写入点（陈年/摘除未销项=红）；
    ③ 组包域闭包：组包域产物全集 == 层1运行时 manifest 钉的面 ∪ {manifest 自身}
       （层1 钉面从 package.rs file_digest(task_dir.join("X")) 字面量机析——
       组包新增产物而层1 不钉它=半覆盖红）；
    ④ 组包回环（真实执行）：provenance ensure 后 cargo 现场组一个真任务包，
       盘面文件集 ↔ 注册表组包产物集**双向**逐位对账 + 层1 manifest 逐字段
       身份核对（sha256 逐位、compiler_version==af-compiler 版本、键集闭）；
    ⑤ verify-dir 同核对可独立复用（对任意既有任务包目录跑 ④ 的核对面）。

  扫描面（磁盘产物创建 API 全集；.write_all/.persist 追加写经其创建点覆盖，
  stdin/socket 管道写非盘面 I/O 不入面——边界见 poc/poc-g3-manifest/README.md）：
    fs::write / File::create / fs::create_dir_all / OpenOptions::new( /
    fs::rename / fs::copy（crates/*/src/**.rs 生产区；#[cfg(test)] 区与经
    lib.rs #[cfg(test)] mod 挂载的独立测试文件剔除）。

用法：
  task_package_manifest_check.py check [--root DIR]        全量（①-④）
  task_package_manifest_check.py verify-dir <dir> [--root DIR]  既有任务包核对（⑤）
  task_package_manifest_check.py inventory [--root DIR]    打印扫描面写入点全集
  task_package_manifest_check.py pin [--root DIR]          打印注册表 sha256（重钉用）
退出码：0=一致；1=红（覆盖不到/缺件/多件/身份不符/表漂移）；2=用法/环境错。
"""
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

PROG = "task_package_manifest_check"
REGISTRY_REL = "ci/baselines/task-package-writepoints.json"
REGISTRY_FORMAT_VERSION = 1
# 注册表自钉（本批生成；改表须同批更新此常量——脚本 diff 即评审面）
REGISTRY_SHA256 = "e4bbf44479d6b89ffa07fff74e850ea84127404fea63e9d41283253f004846a0"

# 组包域常量（与 crate 判据正本同源的名字，construction.rs/package.rs）
RUNTIME_MANIFEST_FILE = "package_manifest.json"
PACKAGE_RS = "crates/af-compiler/src/package.rs"
CONSTRUCTION_RS = "crates/af-compiler/src/construction.rs"
AF_COMPILER_CARGO_TOML = "crates/af-compiler/Cargo.toml"
AF_PACKAGES_REGISTRY = "af/packages"
# 层1 运行时 manifest 的键集（TaskPackageManifest schema，deny_unknown_fields）
RUNTIME_MANIFEST_KEYS = {
    "format_version", "task_sha256", "plan_sha256", "plan_id", "compiler_version",
}

# 磁盘产物创建 API 面（生产写入点判定口径）
WRITE_API_RES = [
    re.compile(r"\bfs::write\s*\("),
    re.compile(r"\bFile::create\s*\("),
    re.compile(r"\bfs::create_dir_all\s*\("),
    re.compile(r"\bOpenOptions\s*::\s*new\s*\("),
    re.compile(r"\bfs::rename\s*\("),
    re.compile(r"\bfs::copy\s*\("),
]
FN_RE = re.compile(
    r'''^\s*(?:pub(?:\([a-z =_"',]*\))?\s+)?'''
    r'''(?:const\s+|async\s+|unsafe\s+|extern\s+"[^"]*"\s+)*fn\s+([A-Za-z0-9_]+)'''
)
CFGT_TEST_ATTR_RE = re.compile(r"^\s*#\[[cC]fg\(test\)\]")
TEST_FN_ATTR_RE = re.compile(r"^\s*#\[\s*(?:[a-z_]+::)*test\s*\]")
MOD_DECL_RE = re.compile(r'''^\s*(?:pub(?:\([a-z =_"',]*\))?\s+)?mod\s+([A-Za-z0-9_]+)\s*[;{]''')
CONST_STR_RE = re.compile(r'''(?:pub\s+)?const\s+([A-Z0-9_]+)\s*:\s*&str\s*=\s*"([^"]*)"''')
JOIN_LITERAL_RE = re.compile(r'''\.join\(\s*"([^"]+)"\s*\)''')
JOIN_CONST_RE = re.compile(r'''\.join\(\s*([A-Z0-9_]+)\s*\)''')


def usage() -> "int":
    print(__doc__, file=sys.stderr)
    return 2


# ---------------------------------------------------------------- Rust 源扫描
def strip_rust_noise(line: str) -> str:
    """行内注释剔除（字符串里的 // 不剔——保守起见只剔行尾无引号争议的段；
    误剔只会少报写入点外的内容，本扫描只做 API 命中，行注释里的样例代码
    由「行首是否 //」整行注释剔除兜底）。"""
    if line.lstrip().startswith("//"):
        return ""
    return line


def _block_end(lines: "list[str]", decl_idx: int) -> int:
    """自声明行（mod X { / fn name(..) {）起，深度跟踪到块闭合（1-based 行号）。
    跨行续态：块注释/原始字符串内的 {} 不计入。"""
    depth, started = 0, False
    state, raw_hashes = "code", 0
    k = decl_idx
    n = len(lines)
    while k < n:
        chars, state, raw_hashes = _iter_code_chars(lines[k], state, raw_hashes)
        for _pos, ch, st in chars:
            if st != "code":
                continue
            if ch == "{":
                depth += 1
                started = True
            elif ch == "}":
                depth -= 1
        if started and depth <= 0:
            return k + 1
        k += 1
    return n


def scan_test_regions(lines: "list[str]") -> "list[tuple[int, int]]":
    """测试区 (起,止) 行区间（1-based，含）——两类形态：
    ① #[cfg(test)] mod X { … }（内联块）；
    ② 裸 #[test] fn / #[cfg(test)] fn（仓内实有形态：mod tests 闭合后顶层再挂
       裸 #[test] 函数，engine.rs:494 起在案）。"""
    regions: "list[tuple[int,int]]" = []
    i, n = 0, len(lines)
    while i < n:
        is_cfg_test = CFGT_TEST_ATTR_RE.match(lines[i])
        is_test_fn = TEST_FN_ATTR_RE.match(lines[i])
        if not (is_cfg_test or is_test_fn):
            i += 1
            continue
        # 跳过后续属性行，找 mod/fn 声明
        j = i + 1
        while j < n and lines[j].lstrip().startswith("#["):
            j += 1
        if j >= n:
            i += 1
            continue
        mod_m = MOD_DECL_RE.match(lines[j])
        if is_cfg_test and mod_m:
            if lines[j].rstrip().endswith(";"):
                i = j + 1  # mod X; 外挂文件——文件本身经 gated_test_files 剔除
                continue
            end = _block_end(lines, j)
            regions.append((i + 1, end))
            i = end
            continue
        if FN_RE.match(lines[j]):
            end = _block_end(lines, j)
            regions.append((i + 1, end))
            i = end
            continue
        i += 1
    return regions


def _iter_code_chars(line: str, state: str = "code", raw_hashes: int = 0):
    """逐字符迭代 (索引, 字符, 状态)；状态 ∈ code/block_comment/string/char/
    raw_string。返回终止态与 raw 井号数（跨行续态由调用方传递——跨行块注释/
    原始字符串内的 {} 不计入深度）。字符串/字符/注释内的字符一律标非 code。"""
    out = []
    i, n = 0, len(line)
    while i < n:
        ch = line[i]
        nxt = line[i + 1] if i + 1 < n else ""
        if state == "code":
            if ch == "/" and nxt == "/":
                break  # 行注释余段全略
            if ch == "/" and nxt == "*":
                state = "block_comment"; i += 2; continue
            if ch == "r":
                # 原始字符串起始 r"…"/r#"…"#（后随 0..n 个 # 再 "）
                j = i + 1
                while j < n and line[j] == "#":
                    j += 1
                if j < n and line[j] == '"':
                    raw_hashes = j - i - 1
                    state = "raw_string"
                    i = j + 1
                    continue
            if ch == '"':
                state = "string"; i += 1; continue
            if ch == "'":
                # 字符字面量或生命周期：后随 ' 或转义判字符，否则生命周期标（非状态）
                if nxt == "\\":
                    state = "char"; i += 1; continue
                if i + 2 < n and line[i + 2] == "'":
                    state = "char"; i += 1; continue
                out.append((i, ch, "code")); i += 1; continue
            out.append((i, ch, "code")); i += 1; continue
        if state == "block_comment":
            if ch == "*" and nxt == "/":
                state = "code"; i += 2; continue
            i += 1; continue
        if state == "string":
            if ch == "\\":
                i += 2; continue
            if ch == '"':
                state = "code"
            i += 1; continue
        if state == "raw_string":
            if ch == '"':
                hashes = 0
                k = i + 1
                while k < n and line[k] == "#" and hashes < raw_hashes:
                    hashes += 1
                    k += 1
                if hashes == raw_hashes:
                    state = "code"
                    i = k
                    continue
            i += 1; continue
        # state == "char"
        if ch == "\\":
            i += 2; continue
        if ch == "'":
            state = "code"
        i += 1
    return out, state, raw_hashes


def gated_test_files(root: Path) -> "set[Path]":
    """经 lib.rs/main.rs `#[cfg(test)]\nmod X;` 挂载的独立测试文件（文件内无标记）。"""
    gated: "set[Path]" = set()
    for lib in list(root.glob("crates/*/src/lib.rs")) + list(root.glob("crates/*/src/main.rs")):
        try:
            lines = lib.read_text(encoding="utf-8").splitlines()
        except OSError:
            continue
        for idx in range(len(lines) - 1):
            if CFGT_TEST_ATTR_RE.match(lines[idx]):
                m = MOD_DECL_RE.match(lines[idx + 1])
                if m:
                    gated.add(lib.parent / f"{m.group(1)}.rs")
    return gated


def file_string_consts(lines: "list[str]") -> "dict[str, str]":
    consts: "dict[str, str]" = {}
    for line in lines:
        m = CONST_STR_RE.search(line)
        if m:
            consts[m.group(1)] = m.group(2)
    return consts


def statement_window(lines: "list[str]", start_idx: int, max_len: int = 8) -> "list[str]":
    """自 API 命中行起、至括号平衡的窗口（截 max_len 行）——多行 fs::write 的
    产物字面量（task_dir.join("task.json") 在下一行）也在窗内。"""
    window: "list[str]" = []
    depth = 0
    state, raw_hashes = "code", 0
    for off in range(max_len):
        idx = start_idx + off
        if idx >= len(lines):
            break
        window.append(lines[idx])
        chars, state, raw_hashes = _iter_code_chars(lines[idx], state, raw_hashes)
        depth += sum(1 if ch == "(" else -1 for _p, ch, st in chars if st == "code" and ch in "()")
        if off > 0 and depth <= 0:
            break
        if off == 0 and depth <= 0:
            break
    return window


def scan_write_points(root: Path) -> "dict[tuple[str, str], dict]":
    """扫描 crates/*/src 生产区全部磁盘产物写入点。
    返回 {(rel_file, fn): {"lines": [..], "literals": {产物名, ..}}}。"""
    gated = gated_test_files(root)
    points: "dict[tuple[str, str], dict]" = {}
    for path in sorted(root.glob("crates/*/src/**/*.rs")):
        if path in gated:
            continue
        rel = str(path.relative_to(root))
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except OSError:
            continue
        test_regions = scan_test_regions(lines)
        consts = file_string_consts(lines)

        def in_test(idx0: int) -> "bool":
            return any(lo <= idx0 + 1 <= hi for lo, hi in test_regions)

        cur_fn = "<top>"
        for idx, raw in enumerate(lines):
            m = FN_RE.match(raw)
            if m:
                cur_fn = m.group(1)
            line = strip_rust_noise(raw)
            if not line or in_test(idx):
                continue
            if not any(rx.search(line) for rx in WRITE_API_RES):
                continue
            key = (rel, cur_fn)
            entry = points.setdefault(key, {"lines": [], "literals": set()})
            entry["lines"].append(idx + 1)
            for wline in statement_window(lines, idx):
                for lm in JOIN_LITERAL_RE.finditer(wline):
                    entry["literals"].add(lm.group(1))
                for cm in JOIN_CONST_RE.finditer(wline):
                    if cm.group(1) in consts:
                        entry["literals"].add(consts[cm.group(1)])
    return points


# ---------------------------------------------------------------- 层1 机析
def parse_runtime_pinned(root: Path) -> "set[str] | None":
    """package.rs build_package_manifest 的 file_digest(&task_dir.join("X")) 字面量
    → 层1 运行时 manifest 实钉产物面。 None = 解析面失效（闭包按红处理）。"""
    p = root / PACKAGE_RS
    if not p.is_file():
        return None
    text = p.read_text(encoding="utf-8")
    names = set(re.findall(r'''file_digest\(&task_dir\.join\("([^"]+)"\)\)''', text))
    return names or None


def parse_af_compiler_version(root: Path) -> "str | None":
    p = root / AF_COMPILER_CARGO_TOML
    if not p.is_file():
        return None
    m = re.search(r'^version\s*=\s*"([^"]+)"', p.read_text(encoding="utf-8"), re.M)
    return m.group(1) if m else None


# ---------------------------------------------------------------- 注册表
def load_registry(root: Path) -> "tuple[dict | None, list[str]]":
    errs: "list[str]" = []
    path = root / REGISTRY_REL
    if not path.is_file():
        return None, [f"{PROG}: 注册表缺席（{REGISTRY_REL}）——覆盖面正本未造册，fail-closed 红"]
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as err:
        return None, [f"{PROG}: 注册表坏形（{REGISTRY_REL}）: {err}"]
    if not isinstance(data, dict):
        return None, [f"{PROG}: 注册表坏形：顶层须为对象"]
    if data.get("format_version") != REGISTRY_FORMAT_VERSION:
        errs.append(
            f"{PROG}: 注册表 format_version={data.get('format_version')!r} 非 "
            f"{REGISTRY_FORMAT_VERSION}（拒绝式：整份拒绝）"
        )
    sha = hashlib.sha256(path.read_bytes()).hexdigest()
    if REGISTRY_SHA256.startswith("PENDING") :
        errs.append(f"{PROG}: 注册表自钉未落（脚本常量 REGISTRY_SHA256 未初始化）")
    elif sha != REGISTRY_SHA256:
        errs.append(
            f"{PROG}: 注册表漂移：实得 sha256={sha[:12]}… 钉 {REGISTRY_SHA256[:12]}…"
            "——改覆盖面正本须同批更新脚本常量并同批评审"
        )
    entries = data.get("entries")
    if not isinstance(entries, list) or not entries:
        errs.append(f"{PROG}: 注册表 entries 缺席/空——写入点全集未造册")
        entries = []
    for i, e in enumerate(entries):
        if not (isinstance(e, dict) and isinstance(e.get("file"), str)
                and isinstance(e.get("fn"), str) and isinstance(e.get("domain"), str)
                and isinstance(e.get("artifacts"), list)):
            errs.append(f"{PROG}: 注册表 entries[{i}] 坏形（file/fn/domain/artifacts 必填）")
    return data, errs


# ---------------------------------------------------------------- ①-③ 静态对账
def static_checks(root: Path, data: dict, scanned: "dict[tuple[str,str], dict]") -> "list[str]":
    errs: "list[str]" = []
    entries = data.get("entries") or []
    reg_keys = {(e["file"], e["fn"]) for e in entries if isinstance(e, dict) and e.get("file")}
    # ② 正向：扫描面内每个生产写入点必须在册（核心：覆盖不到=红）
    for (rel, fn), info in sorted(scanned.items()):
        if (rel, fn) not in reg_keys:
            errs.append(
                f"{PROG}: 生产写入点未造册（覆盖不到=破窗）：{rel}::{fn}"
                f"（写 API 行 {info['lines']}）——须同批入册 {REGISTRY_REL} 并重钉"
            )
    # ② 反向：在册条目必须仍对应真实写入点（陈年/摘除未销项=红）
    scan_keys = set(scanned)
    for e in entries:
        if not isinstance(e, dict):
            continue
        key = (e.get("file"), e.get("fn"))
        if key not in scan_keys:
            errs.append(
                f"{PROG}: 造册条目无对应写入点（陈年/摘除未销项）：{e.get('file')}::{e.get('fn')}"
                "——须同批评审销项或改锚"
            )
    # ③ 组包域闭包 + 字面量对账
    pkg_artifacts: "set[str]" = set()
    for e in entries:
        if isinstance(e, dict) and e.get("domain") == "package":
            for a in e.get("artifacts", []):
                if isinstance(a, str):
                    pkg_artifacts.add(a)
    if not pkg_artifacts:
        errs.append(f"{PROG}: 组包域（domain=package）零条目——任务包组包路径未造册")
    pinned = parse_runtime_pinned(root)
    if pinned is None:
        errs.append(
            f"{PROG}: 层1 钉面机析失效（{PACKAGE_RS} 无 file_digest(&task_dir.join(\"…\")) "
            "字面量命中）——解析面失效按红处理，须同批修门"
        )
        pinned = set()
    want_pinned = pkg_artifacts - {RUNTIME_MANIFEST_FILE}
    if want_pinned != pinned:
        errs.append(
            f"{PROG}: 组包域闭包不成立：注册表组包产物−{{{RUNTIME_MANIFEST_FILE}}}="
            f"{sorted(want_pinned)} ≠ 层1 运行时 manifest 实钉面 {sorted(pinned)}"
            "——组包新增产物而运行时 manifest 不钉它=半覆盖破窗（层1 须同步扩钉）"
        )
    if RUNTIME_MANIFEST_FILE not in pkg_artifacts:
        errs.append(
            f"{PROG}: 组包产物面缺 {RUNTIME_MANIFEST_FILE} 自身——运行时 manifest 落盘"
            "本身是组包产物，须在册"
        )
    # 组包域逐条目字面量对账：写点可析出的产物字面量必须 ⊆ 该条目册面
    reg_by_key = {(e.get("file"), e.get("fn")): e for e in entries if isinstance(e, dict)}
    for (rel, fn), info in sorted(scanned.items()):
        e = reg_by_key.get((rel, fn))
        if not e or e.get("domain") != "package":
            continue
        registered = set(a for a in e.get("artifacts", []) if isinstance(a, str))
        extra = info["literals"] - registered
        if extra:
            errs.append(
                f"{PROG}: 组包写入点产出未入册产物：{rel}::{fn} 写 {sorted(extra)}"
                "（不在册面）——组包路径产物全集须与注册表逐位一致"
            )
    return errs


# ---------------------------------------------------------------- ④ 组包回环
def assemble_package(root: Path, out_dir: Path) -> "tuple[bool, str]":
    """cargo 现场组一个真任务包（真实执行——不摆设）。先过本树 provenance 门
    （承 strategies_check/branches_check 自跑 cargo 同一纪律）。"""
    ensure = subprocess.run(
        ["bash", str(root / "ci" / "provenance_check.sh"), "ensure", "--root", str(root)],
        capture_output=True, text=True,
    )
    if ensure.returncode != 0:
        return False, f"provenance ensure 红（共享 target 跨树污染且清不动）: {ensure.stderr.strip()[:300]}"
    cmd = [
        "cargo", "run", "--quiet", "-p", "af-compiler", "--bin", "af-compiler-construct", "--",
        "--registry", str(root / AF_PACKAGES_REGISTRY),
        "--task-id", "G3-GATE-ROUNDTRIP",
        "--statement", "G3 组包回环探针：产出即核，盘面与清单双向对账。",
        "--acceptance", "ci/task_package_manifest_check.py check 全绿。",
        str(out_dir),
    ]
    try:
        proc = subprocess.run(cmd, cwd=str(root), capture_output=True, text=True, timeout=1800)
    except FileNotFoundError:
        return False, "cargo 不在 PATH——组包回环不可跑（环境缺件）"
    except subprocess.TimeoutExpired:
        return False, "组包回环超时（cargo run 1800s）"
    if proc.returncode != 0:
        return False, f"af-compiler-construct 非零退出 {proc.returncode}: {proc.stderr.strip()[-400:]}"
    return True, proc.stdout.strip()


def verify_package_dir(root: Path, data: dict, pkg_dir: Path) -> "list[str]":
    """⑤=④核对面：盘面文件集 ↔ 注册表组包产物集双向 + 层1 manifest 逐字段身份。"""
    errs: "list[str]" = []
    pkg_artifacts: "set[str]" = set()
    for e in data.get("entries", []):
        if isinstance(e, dict) and e.get("domain") == "package":
            pkg_artifacts.update(a for a in e.get("artifacts", []) if isinstance(a, str))
    if not pkg_artifacts:
        return [f"{PROG}: 注册表组包域空——无可核对清单面"]
    if not pkg_dir.is_dir():
        return [f"{PROG}: 任务包目录缺席（{pkg_dir}）——盘面无产物，fail-closed 红"]
    # 双向集合对账（递归枚举，路径相对包目录）
    on_disk: "set[str]" = {
        str(p.relative_to(pkg_dir)) for p in sorted(pkg_dir.rglob("*")) if p.is_file()
    }
    missing = pkg_artifacts - on_disk
    extra = on_disk - pkg_artifacts
    for name in sorted(missing):
        errs.append(
            f"{PROG}: 清单有条目、盘面无产物：{name}（组包产物缺席——漏产物破窗）"
        )
    for name in sorted(extra):
        errs.append(
            f"{PROG}: 盘面有产物、清单无条目：{name}（未造册产物——多产物破窗）"
        )
    # 层1 manifest 身份核对
    mpath = pkg_dir / RUNTIME_MANIFEST_FILE
    if not mpath.is_file():
        return errs
    try:
        manifest = json.loads(mpath.read_text(encoding="utf-8"))
    except ValueError as err:
        errs.append(f"{PROG}: {RUNTIME_MANIFEST_FILE} 坏形: {err}")
        return errs
    if not isinstance(manifest, dict):
        errs.append(f"{PROG}: {RUNTIME_MANIFEST_FILE} 顶层须为对象")
        return errs
    if set(manifest) != RUNTIME_MANIFEST_KEYS:
        errs.append(
            f"{PROG}: {RUNTIME_MANIFEST_FILE} 键集漂移: {sorted(manifest)} ≠ "
            f"{sorted(RUNTIME_MANIFEST_KEYS)}（deny_unknown_fields 语义）"
        )
        return errs
    if manifest.get("format_version") != 1:
        errs.append(f"{PROG}: {RUNTIME_MANIFEST_FILE} format_version={manifest.get('format_version')!r} 非 1")
    if not str(manifest.get("plan_id", "")).strip():
        errs.append(f"{PROG}: {RUNTIME_MANIFEST_FILE} plan_id 空/缺席——代际身份缺失")
    for field, fname in (("task_sha256", "task.json"), ("plan_sha256", "plan.json")):
        want = manifest.get(field)
        actual_path = pkg_dir / fname
        if not actual_path.is_file():
            continue  # 缺件已在双向对账点名
        actual = hashlib.sha256(actual_path.read_bytes()).hexdigest()
        if want != actual:
            errs.append(
                f"{PROG}: 产物身份不符：{fname} 实得 sha256={actual[:12]}… ≠ "
                f"{RUNTIME_MANIFEST_FILE} 钉 {str(want)[:12]}…——内容被改/钉错，红"
            )
    ver = parse_af_compiler_version(root)
    if ver is None:
        errs.append(f"{PROG}: af-compiler 版本不可析（{AF_COMPILER_CARGO_TOML}）——身份核对失效按红")
    elif manifest.get("compiler_version") != ver:
        errs.append(
            f"{PROG}: 产物代际不符：{RUNTIME_MANIFEST_FILE} compiler_version="
            f"{manifest.get('compiler_version')!r} ≠ af-compiler {ver!r}"
        )
    return errs


# ---------------------------------------------------------------- 命令
def cmd_check(root: Path) -> int:
    errs: "list[str]" = []
    data, reg_errs = load_registry(root)
    errs += reg_errs
    scanned = scan_write_points(root)
    if data is None:
        for e in errs:
            print(e, file=sys.stderr)
        return 1
    errs += static_checks(root, data, scanned)
    # ④ 组包回环（真实执行）
    tmp = Path(tempfile.mkdtemp(prefix="g3-manifest-roundtrip-"))
    try:
        ok, out = assemble_package(root, tmp / "task")
        if not ok:
            errs.append(f"{PROG}: 组包回环失败：{out}")
        else:
            errs += verify_package_dir(root, data, tmp / "task")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    if errs:
        for e in errs:
            print(e, file=sys.stderr)
        return 1
    n_entries = len(data.get("entries", []))
    n_pkg = sum(1 for e in data["entries"] if e.get("domain") == "package")
    print(
        f"{PROG}: 任务包组包覆盖面对账一致：生产写入点在册 {n_entries} 点"
        f"（组包域 {n_pkg} 点、产物 {sorted(e for e in _pkg_artifacts(data))}），"
        f"扫描面实得 {len(scanned)} 点，回环核对绿（{REGISTRY_REL}）"
    )
    return 0


def _pkg_artifacts(data: dict) -> "set[str]":
    arts: "set[str]" = set()
    for e in data.get("entries", []):
        if isinstance(e, dict) and e.get("domain") == "package":
            arts.update(a for a in e.get("artifacts", []) if isinstance(a, str))
    return arts


def cmd_verify_dir(root: Path, pkg_dir: Path) -> int:
    data, reg_errs = load_registry(root)
    if data is None:
        for e in reg_errs:
            print(e, file=sys.stderr)
        return 1
    errs = reg_errs + verify_package_dir(root, data, pkg_dir)
    if errs:
        for e in errs:
            print(e, file=sys.stderr)
        return 1
    print(f"{PROG}: 任务包盘面核对一致：{pkg_dir}（组包产物 {sorted(_pkg_artifacts(data))} 双向逐位）")
    return 0


def cmd_inventory(root: Path) -> int:
    scanned = scan_write_points(root)
    for (rel, fn), info in sorted(scanned.items()):
        lits = ",".join(sorted(info["literals"])) or "-"
        print(f"{rel}\t{fn}\t{info['lines']}\t{lits}")
    print(f"# 共 {len(scanned)} 生产写入点（(file, fn) 计）", file=sys.stderr)
    return 0


def cmd_pin(root: Path) -> int:
    path = root / REGISTRY_REL
    if not path.is_file():
        print(f"{PROG}: 注册表缺席（{REGISTRY_REL}）", file=sys.stderr)
        return 2
    print(hashlib.sha256(path.read_bytes()).hexdigest())
    return 0


def main(argv: "list[str]") -> int:
    if len(argv) < 2:
        return usage()
    cmd, rest = argv[1], argv[2:]
    root = Path(__file__).resolve().parent.parent
    args: "list[str]" = []
    i = 0
    while i < len(rest):
        a = rest[i]
        if a == "--root":
            if i + 1 >= len(rest):
                print(f"{PROG}: --root 缺参", file=sys.stderr)
                return 2
            root = Path(rest[i + 1]).resolve()
            i += 2
            continue
        args.append(a)
        i += 1
    if cmd == "check":
        return cmd_check(root)
    if cmd == "inventory":
        return cmd_inventory(root)
    if cmd == "pin":
        return cmd_pin(root)
    if cmd == "verify-dir":
        if len(args) != 1:
            print(f"{PROG}: verify-dir <dir>", file=sys.stderr)
            return 2
        return cmd_verify_dir(root, Path(args[0]).resolve())
    return usage()


if __name__ == "__main__":
    sys.exit(main(sys.argv))
