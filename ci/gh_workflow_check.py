#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ci/gh_workflow_check.py — GitHub 端 PR 反馈面静态自检器（G1 批新增件）。

被测对象：`<repo>/.github/workflows/*.yml`（GitHub 端 F1 快速门 workflow）。
参照正本：`af/.gitea/workflows/f1.yml`（Gitea 端 F1 快速门）——语义面 1:1 对等，
本检查器把「对等 + 不得弱化」逐条变成可执行判定。

★ 判据面（只增严、不放松）：每条规则都来自 Gitea 端 F1 的语义面或本仓纪律，
  任何一条红 ⇒ exit 非 0。
  R01 面在场        —— .github/workflows/ 存在且非空（G1 债本身不得再欠）
  R02 解析          —— YAML 解析成功（解析不了 = fail-closed 红，不放行）；
                       重复键（映射内同名键）= 解析歧义 ⇒ 同样 fail-closed 判红
  R03 无提权触发    —— pull_request_target 缺席（base 仓上下文跑不可信 PR 代码）
  R04 触发面唯一    —— 只允许 pull_request，且必须带 base 分支约束（Gitea 端同）
  R05 权限只读      —— 顶层 + 每个 job 的 permissions 均无 write
  R06 凭据面        —— 只许 GitHub 自动注入的 token；禁任何硬编码凭据/其他 secret
  R07 零 action     —— 不引任何 action（对等 Gitea 端「零外部 action 依赖」纪律，
                       连第一方 actions/checkout 也不引——最严口径）
  R08 fmt 语义件    —— cargo fmt --check --all 在位
  R09 clippy 语义件 —— cargo clippy --workspace --all-targets -- -D warnings 在位，
                       且不得降级成基线门（禁 clippy_gate.py / --cap-lints / -A warnings）
  R10 快速测试语义件—— cargo test -p af-dsl -p af-compiler 封闭子集在位
  R11 F2 不进 Actions —— 全量门（ci/run.sh / cargo test --workspace / cosign /
                       unshare / netns / cargo deny）不得出现在本 workflow
  R12 超时止损      —— 每个 job 有 timeout-minutes，且 ≤ 45（Gitea 端同值，不得放宽）
  R13 并发策略      —— 顶层 concurrency 在位（group + cancel-in-progress: true）
  R14 诊断一次收全  —— clippy 与快速测试 step 带 always()（一次 run 收全三路诊断）
  R15 runner 在位   —— 每个 job 有 runs-on
  R16 工具链供给    —— 钉住 channel + rustfmt/clippy 组件供给在位（否则语义件跑不了）
  R17 step 形态     —— 每个 step 都有 run 体（零 action 形态）
  R18 语义 step 无 if: 抑制 —— fmt/clippy/快速测试 step 只许 always() 恒跑形态
  R19 job 级 if: 不得恒假 —— 恒假 job = 整门被悄悄关掉
  R20 触发面不得收窄 —— on.pull_request 禁 types / paths / paths-ignore
  R21 吞红写法一律红 —— || true / || : / ; true / set +e
  R22 判据面自检 step 在位且恒跑 —— F1 面不得删掉自检 step

★ 零第三方依赖：内建 YAML 子集解析器（零依赖、可在 GitHub 托管 runner 上直接跑）。
  PyYAML 在场时**双解析交叉核对**：两套解析器结构不一致即红（解析器分歧不放行）。

用法：
  python3 -I ci/gh_workflow_check.py                 # 检 <repo>/.github/workflows/ 全面
  python3 -I ci/gh_workflow_check.py --root DIR      # 检指定根
  python3 -I ci/gh_workflow_check.py --workflow FILE  # 检单件
  python3 -I ci/gh_workflow_check.py --selftest      # 削弱变体负向自证（逐变体跑子进程）
  python3 -I ci/gh_workflow_check.py --list-rules
  python3 -I ci/gh_workflow_check.py --json
退出码：0=全绿；1=有红（任一规则失败）；2=用法/环境错。

★ 门位（本批不挂 ci/run.sh：禁新增 step 编号 + ci/ 下既有件零 diff）：
  ① GitHub 端 PR 门 = `.github/workflows/f1.yml` 的「判据面自检」step 恒跑本脚本
     （谁把判据面改弱 ⇒ 同一次 PR run 红；通道恢复后即生效）；
  ② 本脚本本体落在 ci/ 门码面 ⇒ 被既有跨树收口门 `ci/fix_verify.py` 机制③
     「门族全集单源」自动纳入逐树双核对账（提交面 oid + 工作树面 sha256）
     ⇒ 本体被删/被改弱 ⇒ 该既有门红（机器面，非人工自觉）。
     ★ 生效前提：对账面 = **基线 ref 上的 ci/ 门码面**；本件是新件、尚未并入任何
       基线 ref（实测 `git ls-tree phase2 ci/ | grep -c gh_workflow` = 0）⇒
       **合并进主干基线 ref 之前，机制③ 覆盖不到本件本体**（见 docs/CI-BOUNDARY.md §6.2）；
  ③ SOP 面见 docs/CI-BOUNDARY.md §六（谁在何时跑）。
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# ---------------------------------------------------------------- 规则表 ----
RULES = [
    ("R01", "面在场：.github/workflows/ 存在且非空"),
    ("R02", "YAML 解析成功（解析不了 = fail-closed 红）"),
    ("R03", "禁用 pull_request_target（提权触发面）"),
    ("R04", "触发面唯一：仅 pull_request 且带 base 分支约束"),
    ("R05", "permissions 只读（顶层 + 每个 job 均无 write）"),
    ("R06", "凭据面：只许自动注入 token；禁硬编码凭据与其它 secret"),
    ("R07", "零 action 依赖（不引任何 uses:）"),
    ("R08", "语义件 fmt --check 在位"),
    ("R09", "语义件 clippy -D warnings 在位（且不降级成基线门）"),
    ("R10", "语义件 af-dsl+af-compiler 快速测试在位"),
    ("R11", "F2 全量门不得进 Actions"),
    ("R12", "timeout-minutes 在位且 ≤ 45"),
    ("R13", "concurrency 在位（group + cancel-in-progress: true）"),
    ("R14", "诊断一次收全（clippy/快速测试 step 带 always()）"),
    ("R15", "runs-on 在位"),
    ("R16", "工具链供给在位（钉住 channel + rustfmt/clippy）"),
    ("R17", "每个 step 都有 run 体（零 action 形态）"),
    ("R18", "语义 step 无 if: 抑制（fmt/clippy/快速测试 step 只许 always() 恒跑形态）"),
    ("R19", "job 级 if: 不得恒假（恒假 job = 整门被悄悄关掉）"),
    ("R20", "触发面不得收窄（on.pull_request 禁 types / paths / paths-ignore）"),
    ("R21", "吞红写法一律红（|| true / || : / ; true / set +e）"),
    ("R22", "判据面自检 step 在位且恒跑（F1 面不得删掉自检 step）"),
]
RULE_TITLES = dict(RULES)

MAX_TIMEOUT_MINUTES = 45          # Gitea 端 F1 同值；判据面不得放宽
FAST_TEST_CRATES = ("af-dsl", "af-compiler")

# 硬编码凭据形态（零正文落盘：只做形态判定，不打印命中内容）
CRED_PATTERNS = [
    (re.compile(r"gh[pousr]_[A-Za-z0-9]{16,}"), "GitHub PAT 形态字面量"),
    (re.compile(r"github_pat_[A-Za-z0-9_]{16,}"), "GitHub fine-grained PAT 形态字面量"),
    (re.compile(r"AKIA[0-9A-Z]{16}"), "AWS AKIA 形态字面量"),
    # PEM 头形态：源码里用拼接写，**不留连续形态串**（镜像披露终扫按形态命中）。
    (re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY" + "-" * 5), "私钥 PEM 头形态字面量"),
    (re.compile(r"x-access-token:(?!\s*$)[A-Za-z0-9_\-]{8,}"), "内嵌 access token 字面量"),
]
CRED_ENV_KEY = re.compile(r"(?i)(token|password|passwd|secret|api[_-]?key|credential)")
# F2 全量门依赖面（不得进 Actions）。★ 判「命令位」不判字面量：step 里 echo 一句
#   「F2 全量门：ci/run.sh 不进 Actions」是声明不是调用；真调用在命令位。
F2_COMMAND_PATTERNS = [
    (re.compile(r"^(?:sudo\s+)?(?:ba|z|k|da)?sh\s+.*\bci/run\.sh\b"), "ci/run.sh", "F2 全量门管道脚本"),
    (re.compile(r"^(?:sudo\s+)?(?:\./)?ci/run\.sh\b"), "ci/run.sh", "F2 全量门管道脚本"),
    (re.compile(r"^cargo\s+test\s+.*(?:\s--workspace(?:\s|$)|\s--all(?:\s|$))"),
     "cargo test --workspace/--all", "全量 cargo test"),
    (re.compile(r"^cargo\s+deny\b"), "cargo deny", "cargo deny（供应链门）"),
    (re.compile(r"^(?:sudo\s+)?cosign\b"), "cosign", "cosign 验签"),
    (re.compile(r"^(?:sudo\s+)?unshare\b"), "unshare", "netns 隔离"),
    (re.compile(r"^(?:sudo\s+)?(?:ip\s+netns|nsenter)\b"), "netns", "netns 隔离"),
]

FMT_CMD = "cargo fmt --check --all"
CLIPPY_HEAD = "cargo clippy"
CLIPPY_WORKSPACE = "--workspace"
CLIPPY_ALL_TARGETS = "--all-targets"
CLIPPY_DENY = re.compile(r"-D\s+warnings")
CLIPPY_BASELINE_MARKERS = ["clippy_gate.py", "--cap-lints", "-A warnings", "--allow warnings"]

# ★ 命令位判定（R08/R09/R10 与 R11 同款口径：逐行剥注释、只判行首命令）。
#   ★ 语义件的「齐备」= **命令位**上真的跑着那条命令；写在 echo / 注释里不算。
FMT_CMD_RE = re.compile(r"^cargo\s+fmt\b")
FMT_CHECK_RE = re.compile(r"^cargo\s+fmt\s+--check\b")
CLIPPY_CMD_RE = re.compile(r"^cargo\s+clippy\b")
TEST_CMD_RE = re.compile(r"^cargo\s+test\b")
# `--all` 必须独立成词（`--all-targets` 不算 `--all`）
ALL_FLAG_RE = re.compile(r"(?<![\w-])--all(?![\w-])")
CLIPPY_WORKSPACE_RE = re.compile(r"(?<![\w-])--workspace(?![\w-])")
CLIPPY_ALL_TARGETS_RE = re.compile(r"(?<![\w-])--all-targets(?![\w-])")
TEST_PKG_RE = {c: re.compile(r"(?<![\w-])-p\s+%s(?![\w-])" % re.escape(c))
               for c in ("af-dsl", "af-compiler")}
# 行首 env 赋值前缀（`FOO=1 cargo fmt …` 仍是命令位 ⇒ 剥掉前缀再判）
_ENV_ASSIGN_RE = re.compile(r"^(?:[A-Za-z_][A-Za-z0-9_]*=(?:'[^']*'|\"[^\"]*\"|\S*)\s+)+")

# ★ 吞红写法（命令失败被吞 ⇒ 硬门恒绿 = 判据弱化）：一律判红。
SWALLOW_PATTERNS = [
    (re.compile(r"\|\|\s*true\b"), "`|| true`"),
    (re.compile(r"\|\|\s*:(?:\s|$)"), "`|| :`"),
    (re.compile(r";\s*true\b"), "`; true`"),
    (re.compile(r"(?<![\w-])set\s+\+e\b"), "`set +e`"),
]

# 判据面自检脚本（本检查器本体）——被 workflow 调用处必须恒跑
SELFCHECK_SCRIPT = "gh_workflow_check.py"


class ParseError(Exception):
    """YAML 子集解析失败（fail-closed）。"""


# ------------------------------------------------------- YAML 子集解析器 ----
def _strip_comment(line: str) -> str:
    """去行尾注释（尊重引号）；引号内的 # 不当注释。"""
    out = []
    quote = None
    i = 0
    while i < len(line):
        ch = line[i]
        if quote:
            if ch == quote:
                if quote == "'" and line[i + 1:i + 2] == "'":
                    out.append(ch)
                    i += 1
                    out.append(line[i])
                    i += 1
                    continue
                quote = None
            out.append(ch)
            i += 1
            continue
        if ch in "'\"":
            quote = ch
            out.append(ch)
            i += 1
            continue
        if ch == "#" and (not out or out[-1] in " \t"):
            break
        out.append(ch)
        i += 1
    return "".join(out).rstrip()


def _indent_of(raw: str) -> int:
    return len(raw) - len(raw.lstrip(" "))


def _split_key(content: str):
    """切出 key / value；不是键行返回 (None, None)。"""
    quote = None
    i = 0
    while i < len(content):
        ch = content[i]
        if quote:
            if ch == quote:
                if quote == "'" and content[i + 1:i + 2] == "'":
                    i += 2
                    continue
                quote = None
            i += 1
            continue
        if ch in "'\"":
            quote = ch
            i += 1
            continue
        if ch in "[{":
            return None, None
        if ch == ":" and (i + 1 == len(content) or content[i + 1] in " \t"):
            return content[:i].strip(), content[i + 1:].strip()
        i += 1
    return None, None


def _unquote(text: str) -> str:
    t = text.strip()
    if len(t) >= 2 and t[0] == t[-1] and t[0] in "'\"":
        body = t[1:-1]
        if t[0] == '"':
            return body.replace('\\"', '"').replace("\\n", "\n").replace("\\\\", "\\")
        return body.replace("''", "'")
    return t


def _split_flow(inner: str):
    """流式集合的顶层逗号切分（不切嵌套括号内）。"""
    parts, depth, quote, cur = [], 0, None, []
    for ch in inner:
        if quote:
            cur.append(ch)
            if ch == quote:
                quote = None
            continue
        if ch in "'\"":
            quote = ch
            cur.append(ch)
            continue
        if ch in "[{":
            depth += 1
        elif ch in "]}":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append("".join(cur).strip())
            cur = []
            continue
        cur.append(ch)
    if "".join(cur).strip():
        parts.append("".join(cur).strip())
    return parts


def _parse_scalar(text: str):
    t = text.strip()
    if t.startswith("[") and t.endswith("]"):
        inner = t[1:-1].strip()
        if not inner:
            return []
        return [_parse_scalar(x) for x in _split_flow(inner)]
    if t.startswith("{") and t.endswith("}"):
        inner = t[1:-1].strip()
        out = {}
        if inner:
            for piece in _split_flow(inner):
                k, v = _split_key(piece)
                if k is None:
                    raise ParseError("流式映射项不是键值对: %r" % piece)
                out[_unquote(k)] = _parse_scalar(v)
        return out
    if len(t) >= 2 and t[0] == t[-1] and t[0] in "'\"":
        return _unquote(t)
    low = t.lower()
    if low in ("true", "yes"):
        return True
    if low in ("false", "no"):
        return False
    if low in ("null", "~", ""):
        return None
    if re.fullmatch(r"[-+]?\d+", t):
        return int(t)
    if re.fullmatch(r"[-+]?\d*\.\d+", t):
        return float(t)
    return t


def _lines_of(text: str):
    out = []
    for raw in text.splitlines():
        out.append({"raw": raw, "indent": _indent_of(raw), "content": _strip_comment(raw).strip()})
    return out


def _next_content(lines, idx):
    while idx < len(lines):
        if lines[idx]["content"] == "":
            idx += 1
            continue
        return idx
    return None


def _parse_block_scalar(lines, idx, parent_indent, header):
    style = header[0]
    chomp = ""
    for ch in header[1:]:
        if ch in "+-":
            chomp = ch
    body = []
    idx += 1
    block_indent = None
    while idx < len(lines):
        raw = lines[idx]["raw"]
        if raw.strip() == "":
            body.append("")
            idx += 1
            continue
        ind = _indent_of(raw)
        if ind <= parent_indent:
            break
        if block_indent is None:
            block_indent = ind
        if ind < block_indent:
            break
        body.append(raw[block_indent:])
        idx += 1
    while body and body[-1] == "":
        body.pop()
    text = "\n".join(body)
    if style == ">":
        text = " ".join(x.strip() for x in text.splitlines() if x.strip())
    if chomp != "-":
        text += "\n"
    return text, idx


def _parse_mapping(lines, idx, indent):
    result = {}
    while True:
        j = _next_content(lines, idx)
        if j is None:
            return result, len(lines)
        line = lines[j]
        if line["indent"] < indent:
            return result, j
        if line["indent"] > indent:
            raise ParseError("映射内出现更深缩进的孤立内容: %r" % line["content"][:60])
        content = line["content"]
        if content.startswith("- ") or content == "-":
            return result, j
        key, rest = _split_key(content)
        if key is None:
            raise ParseError("不是键行: %r" % content[:60])
        key = _unquote(key)
        rest = rest.strip()
        # ★ 重复键 = 解析歧义（PyYAML「后者覆盖前者」，GitHub 侧则直接判 workflow 非法）
        #   ⇒ fail-closed 判红，不放行「靠重复键夹带一个被后面覆盖的 `if:`」。
        if key in result:
            raise ParseError("映射内重复键: %r（重复键 = 解析歧义，fail-closed 不放行）" % key)
        if rest == "":
            k = _next_content(lines, j + 1)
            if k is not None and lines[k]["indent"] > indent:
                value, idx = _parse_block(lines, k, lines[k]["indent"])
            elif k is not None and lines[k]["indent"] == indent and (
                lines[k]["content"].startswith("- ") or lines[k]["content"] == "-"
            ):
                value, idx = _parse_sequence(lines, k, indent)
            else:
                value, idx = None, j + 1
        elif rest[0] in "|>" and (len(rest) == 1 or rest[1] in "+-"):
            value, idx = _parse_block_scalar(lines, j, indent, rest)
        else:
            value = _parse_scalar(rest)
            idx = j + 1
        result[key] = value
        idx = max(idx, j + 1)
    return result, idx


def _parse_sequence(lines, idx, indent):
    items = []
    while True:
        j = _next_content(lines, idx)
        if j is None:
            return items, len(lines)
        line = lines[j]
        if line["indent"] < indent:
            return items, j
        if line["indent"] > indent:
            raise ParseError("序列内出现更深缩进的孤立内容: %r" % line["content"][:60])
        content = line["content"]
        if not (content == "-" or content.startswith("- ")):
            return items, j
        rest = content[1:].strip()
        col = line["indent"] + 1
        if content.startswith("- "):
            col += len(content[1:]) - len(content[1:].lstrip(" "))
        if rest == "":
            k = _next_content(lines, j + 1)
            if k is not None and lines[k]["indent"] > indent:
                value, idx = _parse_block(lines, k, lines[k]["indent"])
            else:
                value, idx = None, j + 1
            items.append(value)
            continue
        key, _ = _split_key(rest)
        if key is None:
            items.append(_parse_scalar(rest))
            idx = j + 1
            continue
        lines[j] = {"raw": line["raw"], "indent": col, "content": rest}
        value, idx = _parse_mapping(lines, j, col)
        items.append(value)
    return items, idx


def _parse_block(lines, idx, indent):
    content = lines[idx]["content"] if idx < len(lines) else ""
    if content.startswith("- ") or content == "-":
        return _parse_sequence(lines, idx, indent)
    return _parse_mapping(lines, idx, indent)


def parse_yaml_subset(text: str):
    """YAML 子集解析入口；不支持的构造抛 ParseError（调用方 fail-closed 判红）。"""
    lines = _lines_of(text)
    first = _next_content(lines, 0)
    if first is None:
        return {}
    if lines[first]["indent"] != 0:
        raise ParseError("顶层内容未从第 0 列开始")
    doc, _ = _parse_block(lines, first, 0)
    return doc


# --------------------------------------------------- 双解析交叉核对规范化 ----
def _normalize(node):
    """规范化（键序无关；YAML 1.1 的 `on` 键被 PyYAML 读成 True ⇒ 归一为 'on'）。"""
    if isinstance(node, dict):
        out = {}
        for k, v in node.items():
            key = "on" if k is True else ("on" if k == "on" else k)
            out[str(key)] = _normalize(v)
        return out
    if isinstance(node, list):
        return [_normalize(x) for x in node]
    if isinstance(node, bool):
        return node
    if isinstance(node, (int, float)):
        return node
    if node is None:
        return None
    return str(node).strip()


def _pyyaml_parse(text: str):
    try:
        import yaml  # type: ignore
    except Exception:
        return None
    try:
        return yaml.safe_load(text)
    except Exception:
        return None


# ------------------------------------------------------------- 规则判定 ----
class Report:
    def __init__(self):
        self.fails = []           # (rule_id, message)
        self.passes = []          # (rule_id, message)

    def fail(self, rule_id, message):
        self.fails.append((rule_id, message))

    def ok(self, rule_id, message=""):
        self.passes.append((rule_id, message))


def _runs_of(job):
    out = []
    for step in (job.get("steps") or []):
        if isinstance(step, dict):
            out.append(step)
    return out


def _step_run(step):
    run = step.get("run")
    return run if isinstance(run, str) else ""


def _command_lines(body):
    """逐行剥注释 → 只留非空行 → 剥行首 env 赋值前缀 ⇒ [(原始行, 命令位文本)]。

    ★ 与 R11 同款口径：判「命令位」不判字面量（写在 echo / 注释里的命令不是调用）。
    """
    out = []
    for raw in (body or "").splitlines():
        s = _strip_comment(raw).strip()
        if not s:
            continue
        cmd = _ENV_ASSIGN_RE.sub("", s).strip()
        if not cmd:
            continue
        out.append((s, cmd))
    return out


def _swallow_label(line):
    """吞红写法命中则返回其标签，否则 None（`|| true` / `|| :` / `; true` / `set +e`）。"""
    for pat, label in SWALLOW_PATTERNS:
        if pat.search(line or ""):
            return label
    return None


def _always_false(cond) -> bool:
    """job 级 `if:` 是否恒假（`false` / `${{ false }}` / `false && …`）。"""
    if cond is False:
        return True
    if not isinstance(cond, str):
        return False
    t = cond.strip()
    if t.startswith("${{") and t.endswith("}}"):
        t = t[3:-2].strip()
    t = t.lower()
    if t == "false":
        return True
    if t.startswith("false &&") or t.startswith("false and "):
        return True
    return False


def _is_f1_face(doc) -> bool:
    """本 workflow 是否 F1 快速门面（name 含 F1）——F1 面必须有判据面自检 step。"""
    nm = doc.get("name")
    return isinstance(nm, str) and "F1" in nm


def check_workflow(path: Path, text: str, report: Report) -> None:
    """对单件 workflow 跑全部规则；违规写 report.fail。"""
    rel = path.as_posix()
    no_comment = "\n".join(_strip_comment(l) for l in text.splitlines())

    # R02 解析
    try:
        doc = parse_yaml_subset(text)
    except ParseError as exc:
        report.fail("R02", "%s: YAML 子集解析失败（fail-closed）：%s" % (rel, exc))
        return
    report.ok("R02", "%s: 解析成功" % rel)

    # 交叉核对：PyYAML 在场时两套解析器必须一致
    alt = _pyyaml_parse(text)
    if alt is not None:
        if _normalize(alt) != _normalize(doc):
            report.fail("R02", "%s: 双解析交叉核对不一致（内建解析器 vs PyYAML）——不放行" % rel)
        else:
            report.ok("R02", "%s: 双解析交叉核对一致" % rel)

    if not isinstance(doc, dict):
        report.fail("R02", "%s: 顶层不是映射" % rel)
        return

    # R03 提权触发面（只判「生效文本」：注释里写「禁用 pull_request_target」是文档，
    #               不是触发面；判据按生效面判，不按注释字面量判）
    if "pull_request_target" in no_comment:
        report.fail("R03", "%s: 生效文本出现 pull_request_target（拿 base 仓上下文跑不可信 PR 代码 = 提权面）"
                    % rel)
    else:
        report.ok("R03", "%s: 无 pull_request_target（生效面）" % rel)

    # R04 触发面唯一 + base 分支约束
    on = doc.get("on", doc.get(True))          # YAML 1.1: `on` → True
    if on is None:
        report.fail("R04", "%s: 无 on: 触发面" % rel)
    elif isinstance(on, str):
        report.fail("R04", "%s: on: 触发面为标量 %r（须显式且唯一）" % (rel, on))
    elif isinstance(on, dict):
        keys = sorted(str(k) for k in on)
        if keys != ["pull_request"]:
            report.fail("R04", "%s: 触发面=%s（只允许 pull_request；push/schedule/"
                               "workflow_dispatch/workflow_run 等一律禁）" % (rel, keys))
        else:
            pr = on.get("pull_request")
            branches = None
            if isinstance(pr, dict):
                branches = pr.get("branches")
                # R20 触发面不得被收窄：types / paths / paths-ignore 都会把
                #   「同分支重复推送即重跑」削掉（如 types:[opened] ⇒ 后续 push 不再触发）。
                narrowed = [k for k in ("types", "paths", "paths-ignore") if k in pr]
                if narrowed:
                    report.fail("R20", "%s: on.pull_request 带 %s（收窄触发面 = 削弱「同分支重复推送即重跑」语义"
                                       "；对等 Gitea 端同款语义不得被削）" % (rel, ",".join(narrowed)))
            if not isinstance(branches, list) or not branches or not all(
                isinstance(b, str) and b.strip() for b in branches
            ):
                report.fail("R04", "%s: pull_request 缺 base 分支约束（对等 Gitea 端 base=product/main）" % rel)
            else:
                report.ok("R04", "%s: 触发面=仅 pull_request，base 分支约束=%s" % (rel, branches))
    else:
        report.fail("R04", "%s: on: 结构不可解析" % rel)

    # R05 permissions 只读
    def _perm_bad(perms, where):
        if perms is None:
            return "%s: 无 permissions 声明（fail-closed：GitHub 默认权限面不可假定只读）" % where
        if not isinstance(perms, dict):
            return "%s: permissions 不是映射" % where
        bad = []
        for scope, val in perms.items():
            if not isinstance(val, str) or val.strip().lower() not in ("read", "none"):
                bad.append(str(scope))
        if bad:
            return "%s: 非只读权限项=%s（只许 read/none）" % (where, ",".join(sorted(bad)))
        return None

    top_perm = doc.get("permissions")
    bad = _perm_bad(top_perm, "%s 顶层" % rel)
    if bad:
        report.fail("R05", bad)
    else:
        contents = (top_perm or {}).get("contents")
        if contents != "read":
            report.fail("R05", "%s 顶层: contents 权限=%r（须 read）" % (rel, contents))
        else:
            report.ok("R05", "%s: 顶层 permissions 只读" % rel)

    jobs = doc.get("jobs")
    if not isinstance(jobs, dict) or not jobs:
        report.fail("R05", "%s: 无 jobs（空面）" % rel)
        jobs = {}
    for jname, job in jobs.items():
        if not isinstance(job, dict):
            report.fail("R05", "%s: job %s 结构不可解析" % (rel, jname))
            continue
        bad = _perm_bad(job.get("permissions"), "%s job[%s]" % (rel, jname))
        if bad:
            report.fail("R05", bad)

    # R06 凭据面
    for name in sorted(set(re.findall(r"secrets\.([A-Za-z0-9_]+)", no_comment))):
        if name != "GITHUB_TOKEN":
            report.fail("R06", "%s: 引用 secrets.%s（只许 GitHub 自动注入的 GITHUB_TOKEN）" % (rel, name))
    for pat, label in CRED_PATTERNS:
        if pat.search(no_comment):
            report.fail("R06", "%s: 出现 %s 字面量（凭据禁落盘）" % (rel, label))
    for m in re.finditer(r"^\s*([A-Za-z0-9_\-]+)\s*:\s*(.+)$", no_comment, re.M):
        key, val = m.group(1), m.group(2).strip()
        if CRED_ENV_KEY.search(key) and "${{" not in val and val not in ("", "''", '""'):
            report.fail("R06", "%s: 环境变量 %s 赋值非表达式字面量（凭据不得硬编码）" % (rel, key))
    if not any(r[0] == "R06" for r in report.fails):
        report.ok("R06", "%s: 凭据面干净（无硬编码凭据、无其它 secret）" % rel)

    # R07 零 action
    uses_hits = re.findall(r"^\s*(?:-\s*)?uses\s*:\s*(\S+)", no_comment, re.M)
    if uses_hits:
        report.fail("R07", "%s: 引入 action 依赖=%s（对等 Gitea 端零外部 action 依赖纪律，最严口径："
                          "连第一方 actions/checkout 也不引）" % (rel, ",".join(sorted(set(uses_hits)))))
    else:
        report.ok("R07", "%s: 零 action 依赖" % rel)

    # R08/R09/R10/R11/R14/R16/R17/R18/R21/R22：逐 step 语义
    #   ★ R08/R09/R10 判**命令位**（复用 R11 口径：逐行剥注释、只判行首命令）——
    #     写在 echo / 注释里的命令不算「语义件在位」。
    fmt_ok = clippy_ok = test_ok = False
    selfcheck_found = False
    for job in jobs.values():
        if not isinstance(job, dict):
            continue
        for step in _runs_of(job):
            run = _step_run(step)
            body = run
            cmds = _command_lines(body)
            fmt_hits = [c for c in cmds if FMT_CMD_RE.match(c[1])]
            clippy_hits = [c for c in cmds if CLIPPY_CMD_RE.match(c[1])]
            test_hits = [c for c in cmds if TEST_CMD_RE.match(c[1])]
            # R21 吞红写法（任何 step 的命令位）：|| true / || : / ; true / set +e
            for raw_line, _cmd in cmds:
                label = _swallow_label(raw_line)
                if label:
                    report.fail("R21", "%s: step 命令位出现 %s（吞红写法：命令失败被吞 ⇒ 硬门恒绿 = 判据弱化）"
                                % (rel, label))
            # R08 fmt 语义件（命令位）
            for raw_line, cmd in fmt_hits:
                if (FMT_CHECK_RE.match(cmd) and ALL_FLAG_RE.search(cmd)
                        and not _swallow_label(raw_line)):
                    fmt_ok = True
                else:
                    report.fail("R08", "%s: fmt step 命令位 `%s` 不满足 `%s`"
                                       "（--check/--all 缺失或被吞红 = 判据弱化）"
                               % (rel, cmd[:70], FMT_CMD))
            # R09 clippy 语义件（命令位）
            for raw_line, cmd in clippy_hits:
                if (CLIPPY_WORKSPACE_RE.search(cmd) and CLIPPY_ALL_TARGETS_RE.search(cmd)
                        and CLIPPY_DENY.search(cmd) and not _swallow_label(raw_line)):
                    clippy_ok = True
                else:
                    report.fail("R09", "%s: clippy step 命令位 `%s` 未见 `--workspace --all-targets -- -D warnings`"
                                       "（不得弱于 Gitea 端 F1；不得降级成基线门）" % (rel, cmd[:90]))
            if clippy_hits:
                for marker in CLIPPY_BASELINE_MARKERS:
                    if marker in body:
                        report.fail("R09", "%s: clippy step 出现 %s（-D warnings 被降级成基线门 = 判据弱化）"
                                           % (rel, marker))
            # R10 快速测试语义件（命令位）
            for raw_line, cmd in test_hits:
                if (all(TEST_PKG_RE[c].search(cmd) for c in FAST_TEST_CRATES)
                        and not _swallow_label(raw_line)):
                    test_ok = True
                else:
                    report.fail("R10", "%s: 快速测试 step 命令位 `%s` 未覆盖封闭子集 %s（对等 Gitea 端）"
                                       % (rel, cmd[:90], "+".join(FAST_TEST_CRATES)))
            # R18 语义 step 不得带 if: 抑制条件（只许 always() 恒跑形态）
            if fmt_hits or clippy_hits or test_hits:
                cond = step.get("if")
                if cond is not None and not (isinstance(cond, str) and "always()" in cond):
                    kinds = []
                    if fmt_hits:
                        kinds.append("fmt")
                    if clippy_hits:
                        kinds.append("clippy")
                    if test_hits:
                        kinds.append("快速测试")
                    report.fail("R18", "%s: 语义 step（%s）带 `if:` 抑制条件 %r"
                                       "（只许 `always()` 恒跑形态；`if: false` / `success()` 等 ⇒ 硬门被悄悄关掉 = 判据弱化）"
                               % (rel, "/".join(kinds), cond))
            # R22 判据面自检 step：被调用处必须恒跑（always()）
            if any(SELFCHECK_SCRIPT in cmd for _raw, cmd in cmds):
                selfcheck_found = True
                cond = step.get("if")
                if not (isinstance(cond, str) and "always()" in cond):
                    report.fail("R22", "%s: 判据面自检 step 缺 `always()`"
                                       "（前序步骤红不得遮蔽本判定 = 恒跑）" % rel)
            # R11 F2 全量门不得进（判命令位）
            for _raw, cmd in cmds:
                for pat, marker, label in F2_COMMAND_PATTERNS:
                    if pat.search(cmd):
                        report.fail("R11", "%s: step 命令位出现 %s（%s）——F2 全量门不进 Actions"
                                           % (rel, marker, label))
                        break
            # R14 always()
            if clippy_hits or test_hits:
                cond = step.get("if")
                if not (isinstance(cond, str) and "always()" in cond):
                    report.fail("R14", "%s: clippy/快速测试 step 缺 always()（一次 run 收全三路诊断，"
                                       "对等 Gitea 端）" % rel)
            # R16 工具链供给
            if "rustup" in body and "rustfmt" in body and "clippy" in body:
                pass  # 由汇总位统一记
            # R17 step 形态
            if "uses" in step:
                report.fail("R17", "%s: step 含 uses（零 action 形态要求每步都有 run 体）" % rel)
            if not run.strip():
                report.fail("R17", "%s: step 缺 run 体" % rel)

    if not selfcheck_found and _is_f1_face(doc):
        report.fail("R22", "%s: F1 面缺「判据面自检」step（自检 step 被删 = 防弱化机器面形同虚设 = 判据弱化）" % rel)

    if fmt_ok:
        report.ok("R08", "%s: fmt --check 在位" % rel)
    else:
        report.fail("R08", "R08: %s: 未见 `%s`（语义件缺失 = 判据弱化）" % (rel, FMT_CMD))
    if clippy_ok:
        report.ok("R09", "%s: clippy -D warnings 在位" % rel)
    else:
        report.fail("R09", "R09: %s: 未见 clippy 语义件 `cargo clippy --workspace --all-targets -- -D warnings`"
                          "（语义件缺失 = 判据弱化：删掉该 step = 硬门缺席且检查器放行）" % rel)
    if test_ok:
        report.ok("R10", "%s: af-dsl+af-compiler 快速测试在位" % rel)
    else:
        report.fail("R10", "R10: %s: 未见快速测试语义件 `cargo test -p af-dsl -p af-compiler`"
                          "（语义件缺失 = 判据弱化：删掉该 step = 硬门缺席且检查器放行）" % rel)

    tool_ok = any(
        "rustup" in _step_run(s) and "rustfmt" in _step_run(s) and "clippy" in _step_run(s)
        for job in jobs.values() if isinstance(job, dict) for s in _runs_of(job)
    )
    if tool_ok:
        report.ok("R16", "%s: 工具链供给在位（rustup + rustfmt + clippy 组件）" % rel)
    else:
        report.fail("R16", "%s: 未见工具链供给（rustup 装钉住 channel 的 rustfmt/clippy 组件）——"
                          "否则 fmt/clippy 语义件跑不起来" % rel)

    # R12 超时
    for jname, job in jobs.items():
        if not isinstance(job, dict):
            continue
        tm = job.get("timeout-minutes")
        if not isinstance(tm, int) or isinstance(tm, bool) or tm <= 0:
            report.fail("R12", "%s job[%s]: timeout-minutes 缺失或非正整数（对等 Gitea 端 45）"
                           % (rel, jname))
        elif tm > MAX_TIMEOUT_MINUTES:
            report.fail("R12", "%s job[%s]: timeout-minutes=%d > %d（判据面不得放宽）"
                           % (rel, jname, tm, MAX_TIMEOUT_MINUTES))
        else:
            report.ok("R12", "%s job[%s]: timeout-minutes=%d" % (rel, jname, tm))

    # R19 job 级 if: 不得恒假（恒假 job = 整个门被悄悄关掉）
    for jname, job in jobs.items():
        if not isinstance(job, dict):
            continue
        cond = job.get("if")
        if cond is not None and _always_false(cond):
            report.fail("R19", "%s job[%s]: job 级 `if:` 恒假（%r）"
                           "——job 恒不跑 = 整门被悄悄关掉 = 判据弱化" % (rel, jname, cond))

    # R13 并发
    conc = doc.get("concurrency")
    if not isinstance(conc, dict):
        report.fail("R13", "%s: 顶层 concurrency 缺失" % rel)
    else:
        group = conc.get("group")
        cancel = conc.get("cancel-in-progress")
        if not (isinstance(group, str) and group.strip()):
            report.fail("R13", "%s: concurrency.group 缺失/空" % rel)
        elif cancel is not True:
            report.fail("R13", "%s: concurrency.cancel-in-progress=%r（须 true：同分支只保最新一次）"
                           % (rel, cancel))
        else:
            report.ok("R13", "%s: concurrency 在位（同分支只保最新）" % rel)

    # R15 runs-on
    for jname, job in jobs.items():
        if not isinstance(job, dict):
            continue
        ro = job.get("runs-on")
        if ro is None or (isinstance(ro, str) and not ro.strip()) or (
                isinstance(ro, list) and not ro):
            report.fail("R15", "%s job[%s]: runs-on 缺失" % (rel, jname))
        else:
            report.ok("R15", "%s job[%s]: runs-on 在位" % (rel, jname))


def _workflow_files(root: Path):
    d = root / ".github" / "workflows"
    if not d.is_dir():
        return None
    files = sorted([p for p in d.iterdir() if p.suffix in (".yml", ".yaml") and p.is_file()])
    return files


def run_check(root: Path, explicit=None, quiet=False):
    """跑检查；返回 (exit_code, report, files)。"""
    report = Report()
    files = []
    if explicit:
        files = [Path(explicit)]
    else:
        got = _workflow_files(root)
        if got is None:
            report.fail("R01", "%s: .github/workflows/ 目录不存在（GitHub 端面缺席 = G1 债未消）" % root)
            return 1, report, []
        files = got
    if not files:
        report.fail("R01", "%s: .github/workflows/ 下无 .yml/.yaml（GitHub 端面缺席）" % root)
        return 1, report, []
    report.ok("R01", "面在场：%d 件 workflow（%s）" % (len(files), ", ".join(p.name for p in files)))
    for p in files:
        try:
            text = p.read_text(encoding="utf-8")
        except OSError as exc:
            report.fail("R02", "%s: 读不到（%s）" % (p, exc))
            continue
        check_workflow(p, text, report)
    failed_rules = sorted({rid for rid, _ in report.fails})
    if not quiet:
        for rid, msg in report.passes:
            print("[OK]   %s %s" % (rid, msg))
        for rid, msg in report.fails:
            print("[FAIL] %s %s" % (rid, msg))
        print("SUMMARY: files=%d rules=%d fail=%d failed_rules=%s"
              % (len(files), len(RULES), len(report.fails),
                 ",".join(failed_rules) if failed_rules else "-"))
    return (1 if report.fails else 0), report, files


# ------------------------------------------------------- 削弱变体负向自证 ----
def _variants(base_text: str):
    """(变体 id, 说明, 变异函数)；每条都必须是「判据面被削弱」的形态。"""
    def sub(old, new, count=1):
        def fn(text):
            if old not in text:
                raise AssertionError("变异锚点不存在: %r" % old[:60])
            return text.replace(old, new, count)
        return fn

    fmt_step = (
        "      - name: fmt --check（硬门；主线已归零，红即如实亮红）\n"
        "        run: |\n"
        "          export PATH=\"$HOME/.cargo/bin:$PATH\"\n"
        "          cargo fmt --check --all\n\n"
    )
    # ★ V22/V23：整块删掉 clippy / 快速测试**语义 step**。
    #   这两条是「R09/R10 缺 else 分支 ⇒ fail-open」的正面覆盖：
    #   只有 R08（旧 V06）带 else ⇒ 旧 21 变体覆盖不到「删掉语义 step」这条路径。
    clippy_step = (
        "      - name: clippy -D warnings（硬门；快速门用 -D 全量拦，比 F2 的基线门更严）\n"
        "        # fmt 红不遮蔽 clippy/test 判定——一次 run 收全三路诊断（对齐 ci/run.sh\n"
        "        # 「步骤失败不中断、跑完全部步骤收集完整诊断」哲学）；job 结论仍任一红即红。\n"
        "        if: ${{ always() }}\n"
        "        run: |\n"
        "          export PATH=\"$HOME/.cargo/bin:$PATH\"\n"
        "          cargo clippy --workspace --all-targets -- -D warnings\n\n"
    )
    test_step = (
        "      - name: 快速测试（af-dsl + af-compiler；封闭子集，不碰 F2 资产面）\n"
        "        if: ${{ always() }}\n"
        "        run: |\n"
        "          export PATH=\"$HOME/.cargo/bin:$PATH\"\n"
        "          cargo test -p af-dsl -p af-compiler\n\n"
    )
    variants = [
        ("V01", "引入 pull_request_target（提权触发面）",
         sub("on:\n  pull_request:\n", "on:\n  pull_request_target:\n"), "R03"),
        ("V02", "顶层 permissions 放宽为 write",
         sub("permissions:\n  contents: read\n\n# 同一 PR",
             "permissions:\n  contents: write\n\n# 同一 PR"), "R05"),
        ("V03", "job 级 permissions 放宽为 write",
         sub("    permissions:\n      contents: read\n", "    permissions:\n      contents: write\n"),
         "R05"),
        ("V04", "引入第三方 action（actions/checkout）",
         sub("      - name: 分工声明（F1 快速门在此跑；F2 全量门不走 Actions）\n",
             "      - uses: actions/checkout@v4\n"
             "      - name: 分工声明（F1 快速门在此跑；F2 全量门不走 Actions）\n"), "R07"),
        ("V05", "clippy 降级：去掉 -D warnings，改接基线门",
         sub("cargo clippy --workspace --all-targets -- -D warnings",
             "cargo clippy --workspace --all-targets && python3 ci/clippy_gate.py"), "R09"),
        ("V06", "删掉 fmt --check 语义件",
         sub(fmt_step, ""), "R08"),
        ("V07", "快速测试换成全量 workspace（F2 混入）",
         sub("cargo test -p af-dsl -p af-compiler", "cargo test --workspace"), "R10"),
        ("V08", "删掉 timeout-minutes",
         sub("    timeout-minutes: 45\n", ""), "R12"),
        ("V09", "删掉 concurrency",
         sub("concurrency:\n  group: f1-${{ github.event.pull_request.head.ref || github.ref_name }}\n"
             "  cancel-in-progress: true\n\n", ""), "R13"),
        ("V10", "引用非自动注入的 secret",
         sub("      CARGO_BUILD_JOBS: \"4\"",
             "      AF_PAT: ${{ secrets.MY_PAT }}\n      CARGO_BUILD_JOBS: \"4\""), "R06"),
        ("V11", "触发面放宽到 push",
         sub("on:\n  pull_request:\n", "on:\n  push:\n    branches:\n      - main\n  pull_request:\n"),
         "R04"),
        ("V12", "去掉 base 分支约束",
         sub("    branches:\n      - main\n", ""), "R04"),
        ("V13", "clippy step 去掉 always()（诊断不再一次收全）",
         sub("        if: ${{ always() }}\n        run: |\n          export PATH=\"$HOME/.cargo/bin:$PATH\"\n"
             "          cargo clippy",
             "        run: |\n          export PATH=\"$HOME/.cargo/bin:$PATH\"\n          cargo clippy"),
         "R14"),
        ("V14", "快速测试封闭子集缩水（漏 af-compiler）",
         sub("cargo test -p af-dsl -p af-compiler", "cargo test -p af-dsl"), "R10"),
        ("V15", "把 F2 全量门塞进 Actions",
         sub("      - name: 判据面自检（GitHub 端面防弱化；本仓零外部 action 依赖）\n",
             "      - name: F2 全量门（不该进 Actions）\n        run: |\n          bash ci/run.sh\n\n"
             "      - name: 判据面自检（GitHub 端面防弱化；本仓零外部 action 依赖）\n"), "R11"),
        ("V16", "fmt 门降级：去掉 --check（改成改写而非判定）",
         sub("cargo fmt --check --all", "cargo fmt"), "R08"),
        ("V17", "clippy 门降级：-D warnings 换成 -D clippy::all",
         sub("cargo clippy --workspace --all-targets -- -D warnings",
             "cargo clippy --workspace --all-targets -- -D clippy::all"), "R09"),
        ("V18", "超时止损放宽：45 → 120",
         sub("    timeout-minutes: 45\n", "    timeout-minutes: 120\n"), "R12"),
        ("V19", "并发策略放宽：cancel-in-progress 改 false",
         sub("  cancel-in-progress: true\n", "  cancel-in-progress: false\n"), "R13"),
        ("V20", "job 级 permissions 整块删掉（依赖继承=不可假定只读）",
         sub("    permissions:\n      contents: read\n", ""), "R05"),
        ("V21", "凭据面：环境变量硬编码 PAT 字面量",
         # ★ 夹具字面量在运行时拼装：源码里**不得**留 PAT 形态的连续串
         #   （镜像披露终扫会按形态命中；本仓产物零凭据形态）。
         sub("      CARGO_BUILD_JOBS: \"4\"",
             "      AF_PAT: " + ("ghp_" + "a" * 36) + "\n      CARGO_BUILD_JOBS: \"4\""),
         "R06"),
        ("V22", "整块删掉 clippy -D warnings 语义 step（语义件缺失 = 判据弱化）",
         sub(clippy_step, ""), "R09"),
        ("V23", "整块删掉快速测试（af-dsl + af-compiler）语义 step（语义件缺失 = 判据弱化）",
         sub(test_step, ""), "R10"),
    ]
    return variants


def selftest(root: Path, quiet=False):
    """负向自证：正例须绿；每个削弱变体须红且命中预期规则。"""
    files = _workflow_files(root)
    if not files:
        print("[FAIL] selftest: %s: .github/workflows/ 缺席——无正例可测" % root)
        return 1
    base = files[0]
    base_text = base.read_text(encoding="utf-8")

    print("== 正例（现役面）==")
    rc, _rep, _ = run_check(root, quiet=quiet)
    print("POSITIVE %s exit=%d（期望 0）" % (base.as_posix(), rc))
    if rc != 0:
        print("[FAIL] selftest: 正例就红——检查器不可用")
        return 1

    print("== 负例（削弱变体，逐个须红且命中预期规则）==")
    bad = 0
    tmp = Path(tempfile.mkdtemp(prefix="gh-wf-selftest-"))
    try:
        for vid, desc, mutate, expect in _variants(base_text):
            try:
                mutated = mutate(base_text)
            except AssertionError as exc:
                bad += 1
                print("%s MUTATION-ANCHOR-MISSING exit=- expected_rule=%s | %s"
                      % (vid, expect, desc))
                print("      | %s" % exc)
                continue
            if mutated == base_text:
                bad += 1
                print("%s MUTATION-NOOP exit=- expected_rule=%s | %s" % (vid, expect, desc))
                continue
            vdir = tmp / vid
            (vdir / ".github" / "workflows").mkdir(parents=True, exist_ok=True)
            target = vdir / ".github" / "workflows" / base.name
            target.write_text(mutated, encoding="utf-8")
            proc = subprocess.run(
                [sys.executable, "-I", str(Path(__file__).resolve()), "--root", str(vdir), "--json"],
                capture_output=True, text=True,
            )
            failed_rules = []
            try:
                data = json.loads(proc.stdout)
                failed_rules = [f["rule"] for f in data.get("failures", [])]
            except Exception:
                pass
            ok = proc.returncode != 0 and expect in failed_rules
            if not ok:
                bad += 1
            print("%s exit=%d expected_rule=%s hit=%s | %s"
                  % (vid, proc.returncode, expect,
                     "yes" if expect in failed_rules else "no", desc))
            if not ok:
                print("      | 变体未按预期判红：期望规则 %s 未命中或退出码为 0" % expect)
    finally:
        import shutil
        shutil.rmtree(tmp, ignore_errors=True)
    print("SELFTEST SUMMARY: variants=%d unexpected=%d" % (len(_variants(base_text)), bad))
    return 1 if bad else 0


def main(argv=None):
    ap = argparse.ArgumentParser(description="GitHub 端 PR 反馈面静态自检器")
    ap.add_argument("--root", default=str(REPO_ROOT), help="仓根（默认本脚本所在仓根）")
    ap.add_argument("--workflow", default=None, help="只检单件 workflow 文件")
    ap.add_argument("--selftest", action="store_true", help="削弱变体负向自证")
    ap.add_argument("--list-rules", action="store_true", help="打印规则表")
    ap.add_argument("--json", action="store_true", help="机读输出")
    ap.add_argument("--quiet", action="store_true", help="不打印逐条明细")
    args = ap.parse_args(argv)

    if args.list_rules:
        for rid, title in RULES:
            print("%s\t%s" % (rid, title))
        return 0

    root = Path(args.root).resolve()
    if args.selftest:
        return selftest(root, quiet=args.quiet)

    rc, report, files = run_check(root, explicit=args.workflow, quiet=args.quiet or args.json)
    if args.json:
        print(json.dumps({
            "root": str(root),
            "files": [p.as_posix() for p in files],
            "failures": [{"rule": rid, "message": msg} for rid, msg in report.fails],
            "passes": [{"rule": rid, "message": msg} for rid, msg in report.passes],
            "exit": rc,
        }, ensure_ascii=False, indent=2, sort_keys=True))
    return rc


if __name__ == "__main__":
    sys.exit(main())
