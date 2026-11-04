#!/usr/bin/env python3
"""integrator 复核器——四项绑定 + 门结论（v0.3-T7② 常设化，af 仓 ci/ 面新增件）。

职责**只有一件**：核对「记录侧」vs「实测侧」，逐项不符即拒绝。
本器是**机器辅助核对**，**不是**合流裁决器（方案裁决：机器仲裁员首版不启用）——
exit 0 只表示「机器侧未发现不符」，合流与否仍由人工 integrator 依
`docs/INTEGRATOR-SOP.md` 决定。

────────────────────────────────────────────────────────────────────────────
四项绑定（语义承 PoC-E `four_binding_check`，af/poc/poc-e-codeadapter/codeadapter.py:144，
只读参考；本件是产品仓的常设化，不改 PoC-E）：

  base_sha      分支基点：分支与目标分支的 fork 点（merge-base）== 记录值
                （分支确从记录的 main 头分出；单提交分支时它就是分支父提交，
                  与 PoC-E 的 `HEAD~1` 同值）
  source_sha    分支头：**远端**该分支头 == 记录值（推送的就是测过的那个提交）
  tested_tree   测过的树：source_sha^{tree} == 记录值（合入内容逐位 = 门跑过的内容）
  suite_digest  门套件：门执行面（ci/）＋门载体（.gitea/workflows/）在树内的
                内容摘要 == 记录值（门没被换）

另查 gate verdict **严格等于** `"pass"`（af-workorder gate_admit 同词汇表，
crates/af-workorder/src/gate_admit.rs:55 逐字语义；大小写敏感——"PASS" 非绿）。

────────────────────────────────────────────────────────────────────────────
★ 纪律（只可变严，不可放宽）：

1. 四项键逐一比对；任一**不符**、**为空串**、**缺字段** ⇒ **拒绝**。
2. verdict 必须**严格等于** `"pass"`；`"PASS"`/`"Fail"`/`""`/未知词 ⇒ **拒绝**。
3. ★ **禁"门不可跑就跳过复核"**：门不可跑（记录件缺席/不可解析/门摘要缺失/
   门摘要与内嵌摘要不符/门摘要里判不出绿/门退出码非零）⇒ **拒绝**，不是跳过。
4. ★ **候选树门覆盖**：落地树（合并树）必须 == 记录侧 tested_tree，
   否则"门跑过的内容"不等于"落地的内容" ⇒ 拒绝，并指向 SOP 的 rebase 条款
   （把候选树重落到当前主线 + **重跑门** + 重写记录件，再复核）。
5. ★ **主线推进面**：门跑完后目标分支头前移 ⇒ 旧记录不再描述当前主线 ⇒ 拒绝。
6. ★ **门摘要回读（自证链闭合）**：复核**必须回读**门摘要文件（`gate_summary_path`
   指向的那份），把「磁盘文件 ↔ 记录件内嵌 sha256 ↔ 记录件内嵌原文」**三方比对**；
   任一不一致 ⇒ `GATE-SUMMARY-DIGEST` 拒绝。**回读是强制的（无开关可绕）**：
   内嵌摘要由记录件作者可控 ⇒ 不回读就等于"门摘要原文"可自编 ⇒ 门结论自证链断裂。
7. ★ **候选头来源透明**：`verify --source <本地提交>` 会把候选头从「远端头」换成本地
   提交 ⇒ 必须显式加 `--allow-local-source` 才生效（否则 `LOCAL-SOURCE-NOT-ALLOWED`）；
   复核输出与记录件都留 `source_origin`（远端头 vs 本地提交）以便事后区分。

────────────────────────────────────────────────────────────────────────────
suite_digest 集合定义（`af.gate_suite_digest.v1`）：

  成员 = 树内满足下列规则的全部跟踪文件（按路径字典序）：
    · `ci/` 下的文件（门执行面：门脚本 + 判据正本——clippy 基线/白名单/词表/
      golden 册/debt 册/门输入钉……全在 ci/ 下）
    · `.gitea/workflows/` 下的文件（F1 快速门载体；两门同属"门"）
  排除：
    · `ci/out/**`——门跑出来的**产物**（每次跑门都变；纳入则摘要恒变，无意义）
    · `ci/__pycache__/**`——字节码缓存（非源）
    · `ci/advisory-db/**`——外部语料（未入库，.gitignore 排除）
    · 末段为 `.bak` 的文件——历史备份件（门执行面不引用）

  为什么是这个集合（理由，逐条）：
  (a) **覆盖完整**：门脚本与判据正本都在 ci/ 下 ⇒「门没被换」不需要逐件枚举
      就不会漏面；少列一件就等于给逃逸留门。
  (b) **免漂移**：成员由**树枚举**得出，不靠手写清单 ⇒ 将来新增门脚本自动入集合，
      不会出现"新门脚本上线但摘要不变"的空子。
  (c) **排除项有据**：ci/out/** 是门产物（随跑随变）、*.bak 是历史备份，
      纳入只会制造"摘要恒变"的假红。
  (d) **与仓内既有纪律同向**：ci/run.sh 头注已明写"改判据正本即改门"，
      本集合把这句话变成可机算的摘要。

  摘要算法（确定性，双方可独立复算）：
      digest = sha256( 逐成员:  path + "\0" + sha256(blob 内容) + "\n"  拼接 )

────────────────────────────────────────────────────────────────────────────
用法：

  # 记录侧：门跑完、门绿之后，由合流执行人写记录件（本器自算四项，禁手抄）
  python3 -I ci/integrator_bind_check.py emit \
      --record ci/out/integrator-record-<branch>-<sha>.json \
      --branch integrate/2026-11-04-batchA --remote origin \
      --base-ref origin/product/main --source <门跑过的那次提交> \
      --gate-id f2-full --gate-command "bash ci/run.sh" --gate-exit 0 \
      --gate-summary ci/out/SUMMARY.txt --recorded-by <人>

  # 实测侧核对：合流动作之前跑（人读输出，机器不裁决）
  #   ★ 复核**强制回读**门摘要文件（磁盘文件 ↔ 内嵌 sha256 ↔ 内嵌原文 三方比对；
  #     无开关可绕；文件读不出 ⇒ 拒绝）——门摘要路径取记录件的 gate_summary_path，
  #     也可用 --gate-summary <路径> 显式指定（内容仍须逐字节相符）。
  python3 -I ci/integrator_bind_check.py verify \
      --record ci/out/integrator-record-<branch>-<sha>.json \
      --branch integrate/2026-11-04-batchA --remote origin \
      --base-ref origin/product/main

  # 辅助：只算实测侧并打印（审计 / 历史合流核验用；**不做判定**）
  python3 -I ci/integrator_bind_check.py actual --base-ref origin/product/main --branch <候选分支>

  # 辅助：打印某棵树的门套件摘要（审计用）
  python3 -I ci/integrator_bind_check.py digest --tree <rev>

退出码：0 = 机器侧核对未发现不符（**不等于**准合流）；1 = 拒绝（fail-closed）；
      2 = 用法错 / 环境错（参数非法、仓不可用）。**任何非 0 都是红。**
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

# ── 常量（判据面：改这里 = 改判据，须同批评审）────────────────────────────────
RECORD_SCHEMA = "af.integrator_bind_record.v1"
SUITE_DIGEST_ALGO = "af.gate_suite_digest.v1"

# af-workorder gate_admit 的 verdict 词汇表：严格 "pass" 才是绿，未知词一律非绿。
# crates/af-workorder/src/gate_admit.rs:55 —— `if item.verdict != "pass"`。
GATE_PASS_VERDICT = "pass"

BIND_KEYS = ("base_sha", "source_sha", "tested_tree", "suite_digest")
HEX40 = re.compile(r"^[0-9a-f]{40}$")

# 门套件集合规则（见头注「suite_digest 集合定义」）
SUITE_INCLUDE_PREFIXES = ("ci/", ".gitea/workflows/")
SUITE_EXCLUDE_PREFIXES = ("ci/out/", "ci/__pycache__/", "ci/advisory-db/")
SUITE_EXCLUDE_SUFFIXES = (".bak",)

# 门摘要里允许的非红 step 状态（ci/run.sh 的 SUMMARY 词表，**逐字对齐**，改动须同批评审）：
#   PASS            步骤 rc=0                          （ci/run.sh:65）
#   PASS-WITH-WARN  step14 机读一致性批 rc=2=advisory WARN（ci/run.sh:615）
#                   ⇒ run.sh:618 **只在 rc==1** 时把 OVERALL 置 1 ⇒ 该状态**门仍绿**（exit 0）
#   ADVISORY-PASS   step9 FV advisory 跑了且判据全绿      （ci/run.sh:477）
#   ADVISORY-WARN   step9 工具缺席（coqc/Kani 不在）      （ci/run.sh:483）
# 非绿（**不在白名单 ⇒ 一律红**，不许因"看着像绿"放过）：
#   FAIL            步骤 rc!=0（硬红；run.sh:68 ⇒ OVERALL=1 ⇒ 门 exit 1）
#   ADVISORY-FAIL   step9 跑了且 9a/9b 判据红（ci/run.sh:480）——仓内纪律：
#                   存在未消化的 ADVISORY-FAIL 不得结案（ci/run.sh:476-489）
STEP_STATUS_ALLOWED = frozenset({"PASS", "PASS-WITH-WARN", "ADVISORY-PASS", "ADVISORY-WARN"})

# 候选树改动面 → 人工必看点（机器不裁决，只提示人眼该看哪）
REVIEW_REQUIRED_PREFIXES = (
    "crates/",
    "ci/",
    "proofs/",
    "strategies/",
    ".gitea/",
    "poc/",
)
REVIEW_REQUIRED_EXACT = frozenset(
    {"Cargo.toml", "Cargo.lock", "deny.toml", "deliverables.yaml", "numbers.json", "doc-claims.txt"}
)
REVIEW_REQUIRED_CI_EXCEPT = ("ci/out/",)


class BindCheckError(RuntimeError):
    """复核器自身错误（用法错/环境错）。判据不符不走这里——走 reject。"""


class Reject(Exception):
    """复核拒绝（fail-closed）。message 第一段是稳定 reason_code。"""

    def __init__(self, reason_code: str, detail: str = "") -> None:
        super().__init__(f"{reason_code}: {detail}" if detail else reason_code)
        self.reason_code = reason_code
        self.detail = detail


# ── git 面（root 钉死：路径一律由 __file__/参数推导，不吃 cwd）────────────────
def repo_root() -> Path:
    """本件所在仓根（ci/ 的父目录）——路径纪律：root 钉死，不依赖 cwd。"""
    return Path(__file__).resolve().parent.parent


def git(repo: Path, args: list[str], *, check: bool = True) -> subprocess.CompletedProcess[str]:
    env = dict(os.environ)
    env["GIT_OPTIONAL_LOCKS"] = "0"   # 只读面不落索引锁
    env["LC_ALL"] = "C"
    env["LANG"] = "C"
    env["GIT_TERMINAL_PROMPT"] = "0"  # 凭据缺席即失败，不许挂起等输入
    proc = subprocess.run(
        ["git", *args],
        cwd=str(repo),
        capture_output=True,
        text=True,
        env=env,
    )
    if check and proc.returncode != 0:
        raise BindCheckError(
            f"git {' '.join(args)} 失败 exit={proc.returncode}: {proc.stderr.strip()[:400]}"
        )
    return proc


def git_out(repo: Path, args: list[str]) -> str:
    return git(repo, args).stdout.strip()


def resolve_commit(repo: Path, rev: str) -> str:
    """把任意 rev 解析成 40 位提交 id（解析不了 ⇒ 拒绝）。"""
    proc = git(repo, ["rev-parse", "--verify", "--quiet", f"{rev}^{{commit}}"], check=False)
    sha = proc.stdout.strip()
    if proc.returncode != 0 or not HEX40.match(sha):
        raise Reject("REF-UNRESOLVED", f"rev 解析不出提交: {rev}")
    return sha


def tree_of(repo: Path, commit: str) -> str:
    proc = git(repo, ["rev-parse", "--verify", "--quiet", f"{commit}^{{tree}}"], check=False)
    tree = proc.stdout.strip()
    if proc.returncode != 0 or not HEX40.match(tree):
        raise Reject("REF-UNRESOLVED", f"树解析不出: {commit}^{{tree}}")
    return tree


def merge_base(repo: Path, a: str, b: str) -> str:
    proc = git(repo, ["merge-base", a, b], check=False)
    base = proc.stdout.strip()
    if proc.returncode != 0 or not HEX40.match(base):
        raise Reject("MERGE-BASE-UNRESOLVED", f"merge-base({a}, {b}) 解析不出")
    return base


def is_ancestor(repo: Path, ancestor: str, descendant: str) -> bool:
    return git(repo, ["merge-base", "--is-ancestor", ancestor, descendant], check=False).returncode == 0


def ls_remote_head(repo: Path, remote: str, branch: str) -> str:
    """远端该分支的头（"推送的就是测过的那个提交"）。0 行/多行 ⇒ 拒绝。"""
    proc = git(repo, ["ls-remote", remote, f"refs/heads/{branch}"], check=False)
    if proc.returncode != 0:
        raise Reject(
            "REMOTE-UNREACHABLE",
            f"ls-remote {remote} 失败 exit={proc.returncode}: {proc.stderr.strip()[:200]}",
        )
    lines = [line for line in proc.stdout.splitlines() if line.strip()]
    if len(lines) != 1:
        raise Reject(
            "REMOTE-HEAD-UNRESOLVED",
            f"远端分支 refs/heads/{branch} 命中 {len(lines)} 行（期望 1 行）",
        )
    sha = lines[0].split()[0]
    if not HEX40.match(sha):
        raise Reject("REMOTE-HEAD-UNRESOLVED", f"远端头不是 40 位 sha: {sha!r}")
    return sha


# ── 门套件集合与摘要 ─────────────────────────────────────────────────────────
def suite_member_paths(repo: Path, tree: str) -> list[str]:
    """树内属于门套件集合的路径（升序）。集合规则见文件头注。"""
    proc = git(repo, ["ls-tree", "-r", "-z", "--full-tree", tree])
    members: list[str] = []
    for entry in proc.stdout.split("\0"):
        if not entry:
            continue
        meta, _, path = entry.partition("\t")
        if not path:
            continue
        if not path.startswith(SUITE_INCLUDE_PREFIXES):
            continue
        if path.startswith(SUITE_EXCLUDE_PREFIXES):
            continue
        if path.endswith(SUITE_EXCLUDE_SUFFIXES):
            continue
        members.append(path)
    members.sort()
    return members


def suite_digest(repo: Path, tree: str) -> tuple[str, int]:
    """门套件集合摘要（确定性算法：见头注）。返回 (hex, 成员数)。"""
    members = suite_member_paths(repo, tree)
    digest = hashlib.sha256()
    for path in members:
        blob = git(repo, ["cat-file", "blob", f"{tree}:{path}"]).stdout.encode("utf-8", "surrogateescape")
        digest.update(path.encode("utf-8"))
        digest.update(b"\0")
        digest.update(hashlib.sha256(blob).hexdigest().encode("ascii"))
        digest.update(b"\n")
    return digest.hexdigest(), len(members)


# ── 门摘要 → verdict（严于"看起来像绿"）──────────────────────────────────────
def derive_verdict(summary_text: str) -> tuple[str, list[str]]:
    """从门 SUMMARY 原文判 verdict：只有全部 step 非红才 "pass"。

    判据 = `STEP_STATUS_ALLOWED`（逐字对齐 `ci/run.sh` 的 SUMMARY 词表）：
    白名单 = PASS / PASS-WITH-WARN / ADVISORY-PASS / ADVISORY-WARN；
    红名单 = FAIL / ADVISORY-FAIL / 任何未知词。

    ★ 与 PoC-E 的机制差异（**已披露**）：PoC-E `codeadapter.py:272` 认的是显式绿标
    `PIPELINE-GREEN`（`verdict = "pass" if "PIPELINE-GREEN" in summary else "fail"`）；
    本器认的是 **step 状态词表**。真实 `ci/run.sh` **从不写** `PIPELINE-GREEN`
    （全仓 `grep PIPELINE-GREEN ci/run.sh .gitea/` 无命中）⇒ 按绿标判会让**真实绿门恒红**
    ⇒ 本器的词表口径是必要适配；四项绑定语义**未放宽**。

    判不出 ⇒ "fail"（fail-closed：门不可跑 ⇒ 不许当绿）。
    """
    reasons: list[str] = []
    steps: list[tuple[str, str]] = []
    for raw in summary_text.splitlines():
        line = raw.strip()
        if not line:
            continue
        if line == "PIPELINE-GREEN":
            continue
        if line.startswith("PIPELINE-RED"):
            reasons.append(f"摘要含 PIPELINE-RED 行: {line}")
            continue
        match = re.match(r"^step\S*\s+(\S+)\s+\d+ms\b", line)
        if match:
            steps.append((line, match.group(1)))
    if not steps:
        reasons.append("摘要里没有 step 判定行（门未跑完/不可核 ⇒ 不许当绿）")
        return "fail", reasons
    for line, status in steps:
        if status not in STEP_STATUS_ALLOWED:
            reasons.append(f"step 非绿: {line}")
    if reasons:
        return "fail", reasons
    return "pass", []


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


# ── 门摘要回读（自证链闭合：磁盘文件 ↔ 内嵌 sha256 ↔ 内嵌原文）───────────────
def resolve_gate_summary_path(record: dict[str, Any], record_path: Path, override: str) -> Path:
    """定位**要回读**的门摘要文件。

    优先用显式 `--gate-summary`；否则用记录件的 `gate_summary_path`。
    定位不到 ⇒ 拒绝（回读不成立 ⇒ 门证据链断 ⇒ 不许跳过）。
    相对路径：先按当前工作目录解，再按记录件同目录解（归档件随目录搬家的情形）。
    """
    raw = (override or "").strip() or str(record.get("gate_summary_path") or "").strip()
    if not raw:
        raise Reject(
            "GATE-SUMMARY-ABSENT",
            "记录件缺 gate_summary_path 且未给 --gate-summary（无法回读门摘要文件）",
        )
    given = Path(raw).expanduser()
    candidates: list[Path] = []
    if given.is_absolute():
        candidates.append(given)
    else:
        candidates.append(Path.cwd() / given)
        candidates.append(record_path.parent / given)
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    raise Reject(
        "GATE-SUMMARY-ABSENT",
        f"门摘要文件读不出（回读不成立）：{raw}"
        f"（试过：{', '.join(str(item) for item in candidates)}）",
    )


def read_gate_summary_file(path: Path) -> str:
    """回读门摘要文件原文（读不出/非 utf-8 ⇒ 拒绝，不是跳过）。"""
    try:
        data = path.read_bytes()
    except OSError as error:
        raise Reject(
            "GATE-SUMMARY-ABSENT", f"门摘要文件读不出（回读不成立）：{path}: {error}"
        ) from error
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError as error:
        raise Reject(
            "GATE-SUMMARY-ABSENT", f"门摘要文件不是合法 utf-8（回读不成立）：{path}: {error}"
        ) from error


# ── 记录件读写 ───────────────────────────────────────────────────────────────
def load_record(path: Path) -> dict[str, Any]:
    """读记录件。缺席/不可解析/无对象 ⇒ 拒绝（不是跳过）。"""
    if not path.is_file():
        raise Reject("RECORD-ABSENT", f"记录件缺席: {path}")
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        raise Reject("RECORD-UNREADABLE", f"记录件读不出: {error}") from error
    try:
        payload = json.loads(text)
    except json.JSONDecodeError as error:
        raise Reject("RECORD-UNPARSEABLE", f"记录件不是合法 JSON: {error}") from error
    if not isinstance(payload, dict):
        raise Reject("RECORD-UNPARSEABLE", "记录件顶层不是对象")
    if payload.get("schema_version") != RECORD_SCHEMA:
        raise Reject(
            "RECORD-SCHEMA",
            f"记录件 schema_version={payload.get('schema_version')!r}，期望 {RECORD_SCHEMA!r}",
        )
    return payload


# ── 实测侧 ───────────────────────────────────────────────────────────────────
def compute_merge_tree(repo: Path, base_commit: str, source_commit: str) -> tuple[str, str]:
    """落地树（合并树）+ 方式。

    · 目标头是候选头的祖先 ⇒ 快进合流：落地树 == 候选头树。
    · 否则 ⇒ 真合并：`git merge-tree --write-tree` 机算 ort 合并结果；
      有冲突 ⇒ 拒绝（须先解冲突、定稿候选树后再复核）。
    """
    if is_ancestor(repo, base_commit, source_commit):
        return tree_of(repo, source_commit), "fast-forward"
    proc = git(repo, ["merge-tree", "--write-tree", base_commit, source_commit], check=False)
    if proc.returncode != 0:
        raise Reject(
            "MERGE-TREE-CONFLICT",
            f"合并树机算失败（冲突/不可合并）exit={proc.returncode}：{proc.stdout.strip()[:300]}",
        )
    tree = proc.stdout.strip().splitlines()[0].strip() if proc.stdout.strip() else ""
    if not HEX40.match(tree):
        raise Reject("MERGE-TREE-UNRESOLVED", f"合并树解析不出: {tree!r}")
    return tree, "merge-commit"


# ── verify ──────────────────────────────────────────────────────────────────
def cmd_verify(args: argparse.Namespace) -> int:
    repo = Path(args.repo).resolve() if args.repo else repo_root()
    record_path = Path(args.record).resolve()
    checks: list[dict[str, Any]] = []

    def check(name: str, ok: bool, detail: str) -> None:
        checks.append({"name": name, "ok": bool(ok), "detail": detail})

    try:
        record = load_record(record_path)
        check("记录件可读且 schema 正确", True, f"{record_path}")

        # —— 记录侧完整性：缺项/空串 ⇒ 拒绝 ——
        for key in BIND_KEYS:
            if key not in record:
                raise Reject("KEY-MISSING", f"记录件缺字段 {key}")
            value = record.get(key)
            if not isinstance(value, str) or value == "":
                raise Reject("KEY-EMPTY", f"记录件字段 {key} 为空或非字符串: {value!r}")
        check("记录侧四项齐全且非空", True, ", ".join(f"{k}={record[k][:12]}…" for k in BIND_KEYS))

        for key in BIND_KEYS:
            if not HEX40.match(record[key]) and not re.match(r"^[0-9a-f]{64}$", record[key]):
                raise Reject("KEY-MALFORMED", f"记录件字段 {key} 不是 sha 形态: {record[key]!r}")

        # —— verdict：严格等于 "pass"（大小写敏感）——
        verdict = record.get("verdict")
        if not isinstance(verdict, str):
            raise Reject("KEY-MISSING", "记录件缺字段 verdict")
        if verdict != GATE_PASS_VERDICT:
            raise Reject(
                "VERDICT-NOT-PASS",
                f"verdict={verdict!r}，必须严格等于 {GATE_PASS_VERDICT!r}（大小写敏感）",
            )
        check("verdict 严格等于 'pass'", True, repr(verdict))

        # —— 门证据自证（门不可跑 ⇒ 拒绝，不是跳过）——
        gate_exit = record.get("gate_exit")
        if not isinstance(gate_exit, int) or isinstance(gate_exit, bool) or gate_exit != 0:
            raise Reject("GATE-EXIT-NONZERO", f"门退出码={gate_exit!r}（期望 0）")
        summary_text = record.get("gate_summary_text")
        if not isinstance(summary_text, str) or not summary_text.strip():
            raise Reject("GATE-SUMMARY-ABSENT", "记录件缺门摘要原文（或为空）")
        summary_sha = record.get("gate_summary_sha256")
        if not isinstance(summary_sha, str) or summary_sha != sha256_text(summary_text):
            raise Reject(
                "GATE-SUMMARY-DIGEST",
                f"门摘要 sha256 不符（记录 {summary_sha!r} vs 实算 {sha256_text(summary_text)!r}）",
            )

        # —— 门摘要**回读**（三方比对：磁盘文件 ↔ 内嵌 sha256 ↔ 内嵌原文）——
        # ★ 强制、无开关可绕：内嵌摘要由记录件作者完全可控 ⇒ 不回读则"门摘要原文"可自编。
        summary_file = resolve_gate_summary_path(record, record_path, args.gate_summary)
        file_text = read_gate_summary_file(summary_file)
        file_sha = sha256_text(file_text)
        if file_sha != summary_sha:
            raise Reject(
                "GATE-SUMMARY-DIGEST",
                f"门摘要回读不符：磁盘文件 sha256 {file_sha!r} != 记录件内嵌 {summary_sha!r}"
                f"（{summary_file}）⇒ 门摘要文件与记录件描述的不是同一份内容",
            )
        if file_text != summary_text:
            raise Reject(
                "GATE-SUMMARY-DIGEST",
                f"门摘要回读不符：磁盘文件原文 != 记录件内嵌原文（{summary_file}）"
                "⇒ 记录件内嵌的'门摘要原文'不是磁盘上那份门摘要",
            )
        check(
            "门摘要回读三方一致（磁盘文件 ↔ 内嵌 sha256 ↔ 内嵌原文）",
            True,
            f"{summary_file}（sha256={file_sha[:16]}…）",
        )

        derived, reasons = derive_verdict(file_text)
        if derived != GATE_PASS_VERDICT:
            raise Reject("GATE-VERDICT", "门摘要判不出绿：" + "; ".join(reasons))
        check("门摘要自证绿（回读原文 + sha256 + step 全非红）", True, "gate_exit=0")

        # —— 实测侧：四项自己算 ——
        if args.source:
            if not args.allow_local_source:
                raise Reject(
                    "LOCAL-SOURCE-NOT-ALLOWED",
                    f"--source {args.source!r} 会把候选头从「远端头」换成本地提交"
                    "（绕过远端绑定）⇒ 须显式加 --allow-local-source 才生效（或不用 --source）",
                )
            source_sha = resolve_commit(repo, args.source)
            source_origin = f"本地提交 {args.source}（--allow-local-source；**未绑定远端头**）"
        else:
            source_sha = ls_remote_head(repo, args.remote, args.branch)
            source_origin = f"{args.remote} refs/heads/{args.branch} 的远端头"
        base_head = resolve_commit(repo, args.base_ref)
        base_sha = merge_base(repo, base_head, source_sha)
        tested_tree = tree_of(repo, source_sha)
        merge_tree, merge_mode = compute_merge_tree(repo, base_head, source_sha)
        merge_suite_digest, merge_members = suite_digest(repo, merge_tree)
        src_suite_digest, src_members = suite_digest(repo, tested_tree)

        actual = {
            "base_sha": base_sha,
            "source_sha": source_sha,
            "tested_tree": tested_tree,
            "suite_digest": merge_suite_digest,
        }
        check(
            "实测侧四项自算完成",
            True,
            f"source_sha={source_sha}（{source_origin}）; base_sha=merge-base({base_ref_head_label(args.base_ref, base_head)}, "
            f"{source_sha[:12]}…)={base_sha[:12]}…; tested_tree={tested_tree[:12]}…; "
            f"合并树={merge_tree[:12]}…({merge_mode}, 门集成员 {merge_members} 件)",
        )

        # —— 主线推进面（门跑完后主线前移 ⇒ 旧记录不再描述当前主线）——
        gate_base = record.get("gate_base_sha")
        if not isinstance(gate_base, str) or not HEX40.match(gate_base):
            raise Reject("KEY-MISSING", "记录件缺字段 gate_base_sha（门跑时的目标分支头）")
        if gate_base != base_head:
            raise Reject(
                "GATE-BASE-DRIFT",
                f"门跑完后目标分支头已前移：记录 {gate_base[:12]}… vs 现 {base_head[:12]}…"
                f"（⇒ 候选树须重落到当前主线并**重跑门**，SOP rebase 条款）",
            )
        check("主线未前移（门跑时的目标头 == 现目标头）", True, gate_base[:12] + "…")

        # —— 四项逐一比对（不符/为空/缺项 ⇒ 拒绝）——
        mismatches: dict[str, dict[str, str]] = {}
        for key in BIND_KEYS:
            want, got = str(record.get(key, "")), str(actual.get(key, ""))
            if want != got:
                mismatches[key] = {"expected": want, "actual": got}
        if mismatches:
            raise Reject(
                "BIND-MISMATCH",
                "四项绑定不符：" + json.dumps(mismatches, ensure_ascii=False),
            )
        check("四项绑定逐项相符", True, "base_sha / source_sha / tested_tree / suite_digest")

        # —— 候选树门覆盖：落地树 == 门跑过的树 ——
        if merge_tree != record["tested_tree"]:
            raise Reject(
                "COVERAGE-TESTED-TREE",
                f"落地树 {merge_tree[:12]}… != 记录侧 tested_tree {record['tested_tree'][:12]}…"
                f"（合流内容未整体过门 ⇒ SOP rebase 条款：重落基点 + **重跑门** + 重写记录件，再复核）",
            )
        check("候选树门覆盖成立（落地树 == 门跑过的树）", True, merge_tree[:12] + "…")

        if src_suite_digest != merge_suite_digest:
            raise Reject(
                "SUITE-DRIFT",
                f"候选树门集摘要 {src_suite_digest[:12]}… != 落地树门集摘要 {merge_suite_digest[:12]}…",
            )
        check(
            "门没被换（候选树与落地树的门执行面逐字节同）",
            True,
            f"suite_digest={merge_suite_digest[:16]}… 成员 {merge_members} 件",
        )

        # —— 人工看点（机器不裁决，只提示人眼该看哪）——
        review: list[str] = []
        diff_paths = git_out(repo, ["diff", "--name-only", base_sha, source_sha]).splitlines()
        hot = [
            path
            for path in diff_paths
            if (path.startswith(REVIEW_REQUIRED_PREFIXES) or path in REVIEW_REQUIRED_EXACT)
            and not path.startswith(REVIEW_REQUIRED_CI_EXCEPT)
        ]
        if hot:
            review.append(f"候选树改动触及 {len(hot)} 件代码/门/判据面文件（人眼要看 diff 与门结论摘要）")
        if "ADVISORY-WARN" in summary_text:
            review.append("门摘要含 ADVISORY-WARN 行（工具缺席类 advisory，人眼确认缺席原因可接受）")
        if "PASS-WITH-WARN" in summary_text:
            review.append(
                "门摘要含 PASS-WITH-WARN 行（step14 机读一致性批 rc=2=advisory WARN；门仍绿，"
                "但人眼要看 ci/out/step14.log 里的 WARN 项）"
            )
        if args.source:
            review.append(
                f"★ 候选头取自**本地提交**（{source_origin}）⇒ 未与远端头绑定；"
                "须人工确认该提交已推送到候选分支（否则远端跑的不是被测的那个提交）"
            )
        if merge_mode != "fast-forward":
            review.append(f"合流方式={merge_mode}（落地树经 ort 机算，人眼确认冲突解法无夹带）")
        if not hot and merge_mode == "fast-forward" and "ADVISORY-WARN" not in summary_text \
                and "PASS-WITH-WARN" not in summary_text:
            review.append("（无）候选树仅动文档/工件面且可快进 ⇒ 可按 SOP 自动判过路径处理（合流决定仍由人作）")

        report = {
            "result": "PASS",
            "reason_code": "OK",
            "record": str(record_path),
            "record_sha256": hashlib.sha256(record_path.read_bytes()).hexdigest(),
            "branch": args.branch,
            "remote": args.remote,
            "base_ref": args.base_ref,
            "source_origin": source_origin,
            "source_is_remote_head": not bool(args.source),
            "gate_summary_path": str(summary_file),
            "gate_summary_readback": "磁盘文件 ↔ 内嵌 sha256 ↔ 内嵌原文 三方一致",
            "expected": {key: record[key] for key in BIND_KEYS},
            "actual": actual,
            "merge_tree": merge_tree,
            "merge_mode": merge_mode,
            "merge_tree_sha256": merge_suite_digest,
            "suite_digest_algo": SUITE_DIGEST_ALGO,
            "suite_members": merge_members,
            "candidate_diff_paths": diff_paths,
            "checks": checks,
            "review_points": review,
            "note": "机器辅助核对结论；机器仲裁员首版不启用——合流与否由人工 integrator 依 docs/INTEGRATOR-SOP.md 决定。",
        }
        emit_report(report, args.json)
        return 0
    except Reject as reject:
        report = {
            "result": "REJECT",
            "reason_code": reject.reason_code,
            "detail": reject.detail,
            "record": str(record_path),
            "checks": checks,
            "note": "fail-closed：任一不符/为空/缺项/verdict≠pass/门不可跑 ⇒ 拒绝，不许跳过复核。",
        }
        emit_report(report, args.json)
        return 1


def base_ref_head_label(base_ref: str, resolved: str) -> str:
    return f"{base_ref}@{resolved[:12]}…"


# ── emit（记录侧）───────────────────────────────────────────────────────────
def cmd_emit(args: argparse.Namespace) -> int:
    repo = Path(args.repo).resolve() if args.repo else repo_root()
    record_path = Path(args.record)
    if record_path.exists() and not args.force:
        raise BindCheckError(f"记录件已存在（禁覆盖，失败重写请先改名或用 --force）：{record_path}")

    if args.source:
        source_sha = resolve_commit(repo, args.source)
        source_origin = f"本地提交 {args.source}（emit 侧显式指定；verify 侧仍独立读远端头比对）"
    else:
        source_sha = ls_remote_head(repo, args.remote, args.branch)
        source_origin = f"{args.remote} refs/heads/{args.branch} 的远端头"
    base_head = resolve_commit(repo, args.base_ref)
    base_sha = merge_base(repo, base_head, source_sha)
    tested_tree = tree_of(repo, source_sha)
    digest, members = suite_digest(repo, tested_tree)

    summary_path = Path(args.gate_summary).resolve()
    if not summary_path.is_file():
        raise BindCheckError(f"门摘要缺席（门不可跑 ⇒ 不许写记录件）：{summary_path}")
    summary_text = summary_path.read_text(encoding="utf-8")
    derived, reasons = derive_verdict(summary_text)
    if derived != GATE_PASS_VERDICT:
        raise BindCheckError("门摘要判不出绿（不许写绿记录件）：" + "; ".join(reasons))
    if args.gate_exit != 0:
        raise BindCheckError(f"门退出码非零（{args.gate_exit}）⇒ 不许写记录件")

    record = {
        "schema_version": RECORD_SCHEMA,
        "suite_digest_algo": SUITE_DIGEST_ALGO,
        "gate_id": args.gate_id,
        "gate_command": args.gate_command,
        "gate_exit": args.gate_exit,
        "gate_summary_path": str(summary_path),
        "gate_summary_sha256": sha256_text(summary_text),
        "gate_summary_text": summary_text,
        "verdict": GATE_PASS_VERDICT,
        "branch": args.branch,
        "remote": args.remote,
        "base_ref": args.base_ref,
        "base_sha": base_sha,
        "gate_base_sha": base_head,
        "source_sha": source_sha,
        "source_origin": source_origin,
        "tested_tree": tested_tree,
        "suite_digest": digest,
        "suite_members": members,
        "recorded_at": datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds"),
        "recorded_by": args.recorded_by,
    }
    record_path.parent.mkdir(parents=True, exist_ok=True)
    record_path.write_text(
        json.dumps(record, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8"
    )
    print(
        json.dumps(
            {
                "result": "RECORD-WRITTEN",
                "record": str(record_path),
                "record_sha256": hashlib.sha256(record_path.read_bytes()).hexdigest(),
                "base_sha": base_sha,
                "source_sha": source_sha,
                "tested_tree": tested_tree,
                "suite_digest": digest,
                "suite_members": members,
                "verdict": record["verdict"],
            },
            ensure_ascii=False,
            sort_keys=True,
        )
    )
    return 0


# ── digest（审计辅助）────────────────────────────────────────────────────────
def cmd_digest(args: argparse.Namespace) -> int:
    repo = Path(args.repo).resolve() if args.repo else repo_root()
    tree = args.tree if args.tree else tree_of(repo, "HEAD")
    digest, members = suite_digest(repo, tree)
    paths = suite_member_paths(repo, tree)
    payload = {
        "tree": tree,
        "suite_digest": digest,
        "suite_members": members,
        "algo": SUITE_DIGEST_ALGO,
        "paths": paths if args.list_paths else paths[:0],
    }
    print(json.dumps(payload, ensure_ascii=False, sort_keys=True, indent=2))
    return 0


# ── actual（实测侧自算读数；不含记录侧 ⇒ 不构成复核结论）──────────────────────
def cmd_actual(args: argparse.Namespace) -> int:
    """只算实测侧并打印（审计 / 历史合流核验用）。

    ★ 本子命令**不做任何判定**：没有记录件就没有结论。它存在的意义是让
    「记录侧怎么产生」有可核的输入，也让历史合流的三项 git 可导键能被复核器
    自己算出来（而不是人手抄）。
    """
    repo = Path(args.repo).resolve() if args.repo else repo_root()
    if args.source:
        source_sha = resolve_commit(repo, args.source)
        source_origin = f"--source 指定本地提交 {args.source}"
    else:
        source_sha = ls_remote_head(repo, args.remote, args.branch)
        source_origin = f"{args.remote} refs/heads/{args.branch} 的远端头"
    base_head = resolve_commit(repo, args.base_ref)
    base_sha = merge_base(repo, base_head, source_sha)
    tested_tree = tree_of(repo, source_sha)
    merge_tree, merge_mode = compute_merge_tree(repo, base_head, source_sha)
    digest, members = suite_digest(repo, merge_tree)
    src_digest, src_members = suite_digest(repo, tested_tree)
    payload = {
        "note": "实测侧自算读数（无记录侧 ⇒ 不构成复核结论）",
        "source_origin": source_origin,
        "branch": args.branch,
        "remote": args.remote,
        "base_ref": args.base_ref,
        "base_head": base_head,
        "base_sha": base_sha,
        "source_sha": source_sha,
        "tested_tree": tested_tree,
        "suite_digest": digest,
        "suite_members": members,
        "suite_digest_of_source_tree": src_digest,
        "suite_members_of_source_tree": src_members,
        "merge_tree": merge_tree,
        "merge_mode": merge_mode,
        "merge_tree_equals_source_tree": merge_tree == tested_tree,
    }
    print(json.dumps(payload, ensure_ascii=False, sort_keys=True, indent=2))
    return 0


def emit_report(report: dict[str, Any], as_json: bool) -> None:
    if as_json:
        print(json.dumps(report, ensure_ascii=False, sort_keys=True, indent=2))
        return
    print("=== integrator 复核（四项绑定 + 门结论）===")
    print(f"结论：{report['result']}  reason_code={report.get('reason_code')}")
    if report.get("detail"):
        print(f"拒绝理由：{report['detail']}")
    for item in report.get("checks", []):
        print(f"  [{'OK ' if item['ok'] else 'RED'}] {item['name']} — {item['detail']}")
    if report.get("expected"):
        print("  记录侧(expected) / 实测侧(actual)：")
        for key in BIND_KEYS:
            print(f"    {key:13s} {report['expected'][key]}")
    if report.get("actual"):
        print("  实测侧四项：")
        for key in BIND_KEYS:
            print(f"    {key:13s} {report['actual'][key]}")
    if report.get("merge_tree"):
        print(f"  落地树(合并树)：{report['merge_tree']}  方式={report.get('merge_mode')}")
    for point in report.get("review_points", []):
        print(f"  人工看点：{point}")
    print(f"  说明：{report.get('note', '')}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0] if __doc__ else None)
    sub = parser.add_subparsers(dest="command", required=True)

    verify = sub.add_parser("verify", help="核对记录侧 vs 实测侧（逐项不符即拒绝）")
    verify.add_argument("--record", required=True, help="记录件路径（门跑完写下的那份）")
    verify.add_argument("--repo", default="", help="仓根（缺省=本件所在仓）")
    verify.add_argument("--branch", default="", help="候选分支名（远端头为 source_sha）")
    verify.add_argument("--remote", default="origin", help="远端名（缺省 origin）")
    verify.add_argument("--base-ref", required=True, help="目标分支 ref（如 origin/product/main）")
    verify.add_argument("--source", default="", help="显式指定候选提交（默认=远端分支头）")
    verify.add_argument(
        "--allow-local-source",
        action="store_true",
        help="显式承认：候选头取自本地提交（--source）而**未绑定远端头**；不给则 --source 被拒",
    )
    verify.add_argument(
        "--gate-summary",
        default="",
        help="要回读的门摘要路径（缺省=记录件的 gate_summary_path；回读本身是强制的）",
    )
    verify.add_argument("--json", action="store_true", help="机读输出")
    verify.set_defaults(func=cmd_verify)

    emit = sub.add_parser("emit", help="写记录侧记录件（四项本器自算，禁手抄）")
    emit.add_argument("--record", required=True)
    emit.add_argument("--repo", default="")
    emit.add_argument("--branch", default="")
    emit.add_argument("--remote", default="origin")
    emit.add_argument("--base-ref", required=True)
    emit.add_argument("--source", default="", help="显式指定候选提交（默认=远端分支头；记录件会记 source_origin）")
    emit.add_argument("--gate-id", required=True, help="门标识（如 f2-full）")
    emit.add_argument("--gate-command", default="", help="门命令原文")
    emit.add_argument("--gate-exit", type=int, required=True, help="门命令退出码（须 0）")
    emit.add_argument("--gate-summary", required=True, help="门 SUMMARY 路径")
    emit.add_argument("--recorded-by", default="", help="记录人")
    emit.add_argument("--force", action="store_true", help="允许覆盖既有记录件")
    emit.set_defaults(func=cmd_emit)

    digest = sub.add_parser("digest", help="打印某棵树的门套件摘要（审计辅助）")
    digest.add_argument("--repo", default="")
    digest.add_argument("--tree", default="", help="树/提交（缺省=HEAD 的树）")
    digest.add_argument("--list-paths", action="store_true")
    digest.set_defaults(func=cmd_digest)

    actual = sub.add_parser("actual", help="只算实测侧并打印（审计/历史核验；不做判定）")
    actual.add_argument("--repo", default="")
    actual.add_argument("--branch", default="")
    actual.add_argument("--remote", default="origin")
    actual.add_argument("--base-ref", required=True)
    actual.add_argument("--source", default="", help="显式指定候选提交（默认=远端分支头）")
    actual.set_defaults(func=cmd_actual)

    args = parser.parse_args(argv)
    try:
        return int(args.func(args))
    except Reject as reject:
        print(
            json.dumps(
                {"result": "REJECT", "reason_code": reject.reason_code, "detail": reject.detail},
                ensure_ascii=False,
                sort_keys=True,
            ),
            file=sys.stderr,
        )
        return 1
    except BindCheckError as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
