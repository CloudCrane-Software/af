#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""strategies_check.py — strategies/ 共识策略库全量校验（libA-20261029；step14 段尾 14d）

挂点：ci/run.sh step14 机读一致性批段尾（14d；不新增 step 槽——门数账纪律：
numbers.json gates.ci_steps/ci.steps_total=14、gates.ci_hard=13 与 xcheck 门数对账
均不因此改动，符合「禁止堆门」与 G0 批共宿槽先例）。

校验面（对应工单「schema+compilable 条目逐一编译断言+计数与 index 一致」）：
  R0 入库对账（2026-11-02 耐久性断口修复批新增）：被测对象（strategies/ 全库
     + af-dsl 编译断言测试件 crates/af-dsl/tests/strategy_library_test.rs）必须
     在 git 跟踪面内——未跟踪工作树文件=任何 checkout/clean/reset 即静默灭失，
     干净 checkout 跑已提交的门直接红而本树工作树面照绿（绿灯掩盖，RT-E48/E83
     同型）。非 git 树形态（导出 tarball/拷贝树）如实 INFO 跳过。
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
  R5 跨树集合对账（RT-E82）：工作树 strategies/ 磁盘集合与 HEAD 提交树集合
     逐项比对，only-in-worktree（新增/未入库）/ only-in-head（缺失/旧树）/
     content-diff（同名内容改写）/ 改名漂移（only 两侧同摘要配对）四类差异
     逐条点名，非空即硬红。动因：R0 只判「在场且被 git 跟踪」（ls-files 跟踪
     面），_head_files 取到的 HEAD 树集合仅用于 INFO 提醒——「跟踪面有」≠
     「与提交树一致」：已 add 未 commit 的改写、工作树删件、未入库新增件都能
     让门对着与 HEAD 不同的集合照绿。基线取 git HEAD 提交树（ls-tree 路径集
     + 逐文件内容 sha256）；非 git 切片回退 MANIFEST.json 的 files[]；两者
     都不可用即 fail-closed 红（无法证明同一份集合≠一致，不静默放行）。

退出码：0=全过；1=任一硬红（schema/index/计数/编译断言/跨树集合）。
路径纪律（红队 P2-2 root 钉死）：仓内侧默认值一律由 __file__ 推导到本脚本
所在仓库根，不对旧分支局部副本跑。
pyc 纪律：本脚本只 import 标准库与 site-packages（yaml），不在 ci/ 下产生
__pycache__；run.sh 挂点仍以 python3 -B 调用（pyc-shadow 族防线双保险）。
"""

import argparse
import hashlib
import json
import os
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


# ---------------------------------------------------------------- R0 入库对账
GIT_TEST_REL = "crates/af-dsl/tests/strategy_library_test.rs"


def _git_tracked(rel_paths: set[str]) -> tuple[set[str] | None, str | None]:
    """返回 (跟踪面内的路径集合, 诊断串)。诊断串非 None = 无法判定（fail-closed）。"""
    if shutil.which("git") is None:
        return None, "git 不在 PATH——非 git 树形态（导出 tarball/拷贝树），跳过 R0"
    try:
        probe = subprocess.run(
            ["git", "-C", str(REPO), "rev-parse", "--is-inside-work-tree"],
            capture_output=True, text=True, timeout=60,
        )
    except (OSError, subprocess.TimeoutExpired) as e:
        return None, f"git 调用失败：{e}——非 git 树形态，跳过 R0"
    if probe.returncode != 0 or probe.stdout.strip() != "true":
        return None, "非 git 工作树（导出 tarball/拷贝树）——跳过 R0"
    try:
        ls = subprocess.run(
            ["git", "-C", str(REPO), "ls-files", "-z", "--", "strategies", GIT_TEST_REL],
            capture_output=True, text=True, timeout=120,
        )
    except (OSError, subprocess.TimeoutExpired) as e:
        return None, f"git ls-files 调用失败：{e}"
    if ls.returncode != 0:
        return None, f"git ls-files 失败（exit={ls.returncode}）——无法确认被测对象入库状态"
    return {p for p in ls.stdout.split("\0") if p}, None


def check_durability() -> None:
    """R0 被测对象入库对账（2026-11-02 耐久性断口修复批）。

    逃逸形态（本门自身复现的实测断口）：门脚本 ci/strategies_check.py 已随批
    入库，而被它检查的 strategies/ 全库与 af-dsl 编译断言测试件仍是**未跟踪
    工作树文件**——任何 checkout/clean/reset 即静默灭失，干净 checkout 跑已
    提交的门直接红（实测：git archive HEAD 的树内 strategies/ 缺席 →
    「[FAIL] strategies/ 目录不存在」→ GATE_RC=1），而本树工作树面照绿：
    绿灯掩盖（RT-E48/E83 同型，本域新面）。

    本检查把「被测对象必须被 git 跟踪」变成门内硬红：在场却未跟踪 = 门在假
    工作树上绿、在任何真实 checkout 上红（静默灭失通道当场显形）。

    严重度分档（避免与提交动作循环）：
      · 未跟踪（不在 git 跟踪面）→ 硬红：git clean/checkout 即灭失。
      · 已暂存未提交 → INFO：checkout/clean 不灭失，但 CI 以提交态 checkout
        复跑，仍须随批提交（提交态缺件由 R1「strategies/ 缺席」当场红）。
      · 非 git 树（导出 tarball/拷贝树、无 git 可执行文件）→ INFO 跳过：
        R1-R4 判定语义不变（本门在非 git 形态下仍可用）。
    """
    tracked, diag = _git_tracked(set())
    if tracked is None:
        info(f"R0 入库对账跳过：{diag}（不改变 R1-R4 判定）")
        return

    # 盘面集合：strategies/ 下全部文件 + af-dsl 编译断言测试件
    on_disk = {p.relative_to(REPO).as_posix() for p in STRATS.rglob("*") if p.is_file()}
    test_path = REPO / GIT_TEST_REL
    if test_path.is_file():
        on_disk.add(GIT_TEST_REL)
    else:
        err(f"R0 af-dsl 编译断言测试件缺席：{GIT_TEST_REL}——"
            f"cargo test --test strategy_library_test 无此靶（编译断言面失效）")

    untracked = sorted(rel for rel in on_disk if rel not in tracked)
    for rel in untracked:
        err(f"R0 被测对象未入库（未跟踪工作树文件）：{rel}——门脚本已入库而数据未入库="
            f"任何 checkout/clean/reset 即静默灭失，干净 checkout 跑已提交的门直接红"
            f"（RT-E48/E83 同型）；修法=git add 后随批提交")
    if not untracked:
        info(f"R0 入库对账 OK：被测对象 {len(on_disk)} 件全部在 git 跟踪面内"
             f"（strategies/ 全库 + {GIT_TEST_REL}）")
    head = _head_files()
    staged_only = sorted(rel for rel in on_disk if rel in tracked and rel not in head)
    if staged_only:
        info(f"R0 提醒：{len(staged_only)} 件被测对象已暂存未提交（{staged_only[:3]}…）"
             f"——随批提交后方入 CI 提交态 checkout")


def _head_files() -> set[str]:
    """HEAD 提交树内的路径集合（失败时返回空集——只影响 INFO 提醒，不影响判定）。"""
    try:
        proc = subprocess.run(
            ["git", "-C", str(REPO), "ls-tree", "-r", "--name-only", "HEAD"],
            capture_output=True, text=True, timeout=120,
        )
    except (OSError, subprocess.TimeoutExpired):
        return set()
    if proc.returncode != 0:
        return set()
    return {p for p in proc.stdout.splitlines() if p}


# ---------------------------------------------------------------- R5 跨树集合对账
# RT-E82：把 HEAD 提交树集合从「INFO 提醒面」升格为「判定基线」。R0 的 ls-files
# 跟踪面只回答「文件会不会被 checkout 灭失」，不回答「工作树的集合与提交树是否
# 同一份」——已暂存未提交的改写/删件/新增都能在 R0 眼下全绿而 HEAD 树早已不同。
# 本腿对 strategies/ 全文件（非仅 st-*.yaml 条目）逐项对账，四类差异逐条指名。
R5_SCOPE = "strategies"
CROSSTREE_MANIFEST_ENV = "AF_CROSSTREE_MANIFEST"
_DIGEST_CHUNK = 1 << 20


def _digest_file(path: Path) -> str:
    """文件内容 sha256（分块读，大文件不整读进内存）。"""
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(_DIGEST_CHUNK), b""):
            h.update(chunk)
    return h.hexdigest()


def _worktree_digests() -> dict[str, str]:
    """工作树 strategies/ 全部文件的 {仓内相对路径: 内容 sha256}。

    集合口径与 R0 的盘面集合一致（strategies/ 下一切文件，含 index.yaml 与
    非条目辅助文件）；排序遍历保证同一棵树两次读数逐字节一致。
    """
    out: dict[str, str] = {}
    if not STRATS.is_dir():
        return out
    for p in sorted(STRATS.rglob("*")):
        if p.is_file():
            out[p.relative_to(REPO).as_posix()] = _digest_file(p)
    return out


def _git_head_digests() -> tuple[dict[str, str] | None, str]:
    """HEAD 提交树 strategies/ 的 {路径: 内容 sha256}；不可用返 (None, 诊断)。

    基线取「HEAD 提交树」而非索引/工作树：CI 以提交态 checkout 复跑，判定必须
    对着将被检出复现的那份集合。ls-tree 取路径集，逐文件 git show 取内容后算
    sha256（与工作树侧同一种摘要，跨侧可比）。
    """
    if shutil.which("git") is None:
        return None, "git 不在 PATH"
    try:
        probe = subprocess.run(
            ["git", "-C", str(REPO), "rev-parse", "--is-inside-work-tree"],
            capture_output=True, text=True, timeout=60,
        )
    except (OSError, subprocess.TimeoutExpired) as e:
        return None, f"git rev-parse 调用失败：{e}"
    if probe.returncode != 0 or probe.stdout.strip() != "true":
        return None, "非 git 工作树"
    try:
        rev = subprocess.run(
            ["git", "-C", str(REPO), "rev-parse", "HEAD"],
            capture_output=True, text=True, timeout=60,
        )
    except (OSError, subprocess.TimeoutExpired) as e:
        return None, f"git rev-parse HEAD 调用失败：{e}"
    if rev.returncode != 0:
        return None, f"git HEAD 不可解析（exit={rev.returncode}）"
    head_rev = rev.stdout.strip()
    try:
        ls = subprocess.run(
            ["git", "-C", str(REPO), "ls-tree", "-r", "-z", "--name-only",
             "HEAD", "--", R5_SCOPE],
            capture_output=True, text=True, timeout=120,
        )
    except (OSError, subprocess.TimeoutExpired) as e:
        return None, f"git ls-tree 调用失败：{e}"
    if ls.returncode != 0:
        return None, f"git ls-tree HEAD 失败（exit={ls.returncode}）"
    digests: dict[str, str] = {}
    for rel in sorted(p for p in ls.stdout.split("\0") if p):
        try:
            blob = subprocess.run(
                ["git", "-C", str(REPO), "show", f"HEAD:{rel}"],
                capture_output=True, timeout=60,
            )
        except (OSError, subprocess.TimeoutExpired) as e:
            return None, f"git show HEAD:{rel} 调用失败：{e}"
        if blob.returncode != 0:
            return None, f"git show HEAD:{rel} 失败（exit={blob.returncode}）"
        digests[rel] = hashlib.sha256(blob.stdout).hexdigest()
    return digests, f"git HEAD {head_rev[:12]}"


def _manifest_candidates() -> list[Path]:
    """MANIFEST 基线候选路径：环境变量优先，其次仓根、父目录（导出切片形态）。"""
    candidates: list[Path] = []
    env = os.environ.get(CROSSTREE_MANIFEST_ENV)
    if env:
        candidates.append(Path(env))
    candidates.append(REPO / "MANIFEST.json")
    candidates.append(REPO.parent / "MANIFEST.json")
    return candidates


def _manifest_digests() -> tuple[dict[str, str] | None, str]:
    """MANIFEST.json 基线（非 git 切片回退）：files[] 内 strategies/ 逐文件 sha256。"""
    tried: list[str] = []
    for path in _manifest_candidates():
        tried.append(str(path))
        if not path.is_file():
            continue
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as e:
            return None, f"MANIFEST 读取失败 {path}：{e}"
        files = data.get("files")
        if not isinstance(files, list):
            return None, f"MANIFEST 缺 files[]：{path}"
        mapping: dict[str, str] = {}
        for item in files:
            if not isinstance(item, dict):
                continue
            rel = item.get("path")
            digest = item.get("sha256")
            if (isinstance(rel, str) and isinstance(digest, str)
                    and rel.startswith(f"{R5_SCOPE}/")):
                mapping[rel] = digest.strip().lower()
        if not mapping:
            return None, f"MANIFEST 无 {R5_SCOPE}/ 条目：{path}"
        base = str(data.get("base_sha", "?"))[:12]
        return mapping, f"MANIFEST {path} base_sha={base}"
    return None, f"未找到 MANIFEST.json（尝试：{'、'.join(tried)}）"


def _crosstree_baseline() -> tuple[dict[str, str] | None, str]:
    """HEAD 基线解析：git 提交树优先；非 git 切片回退 MANIFEST；都不可用=None。"""
    git_map, git_diag = _git_head_digests()
    if git_map is not None:
        return git_map, git_diag
    man_map, man_diag = _manifest_digests()
    if man_map is not None:
        return man_map, f"{man_diag}（git 不可用：{git_diag}）"
    return None, f"无 HEAD 基线（git：{git_diag}；manifest：{man_diag}）"


def check_crosstree() -> None:
    """R5 工作树 strategies/ 与 HEAD 提交树集合逐项对账（RT-E82）。

    差异逐条指名、非空即硬红（既有 R0-R4 判定零改动，本腿只做增量）：
      · only-in-worktree：工作树有、HEAD 无（新增/未入库文件，未经 HEAD 核对）
      · only-in-head    ：HEAD 有、工作树无（工作树缺件或旧树，判定对着残缺集合）
      · content-diff    ：同名内容 sha256 不同（工作树改写未随 HEAD）
      · 改名漂移       ：only 两侧同摘要配对（集合不等照红，配对只作解读标注）
    基线来源与遍历序写入输出；无 HEAD 基线时 fail-closed 红，不静默跳过。
    """
    worktree = _worktree_digests()
    head, label = _crosstree_baseline()
    if head is None:
        err(f"R5 跨树集合无法判定（fail-closed，不静默放行）：{label}")
        return

    only_wt = sorted(set(worktree) - set(head))
    only_head = sorted(set(head) - set(worktree))
    content_diff = sorted(
        rel for rel in set(worktree) & set(head) if worktree[rel] != head[rel]
    )

    # 改名漂移：only 两侧按内容摘要配对（路径集合已漂移，照红并点名配对）
    wt_by_digest: dict[str, list[str]] = {}
    for rel in only_wt:
        wt_by_digest.setdefault(worktree[rel], []).append(rel)
    head_by_digest: dict[str, list[str]] = {}
    for rel in only_head:
        head_by_digest.setdefault(head[rel], []).append(rel)
    rename_pairs: list[tuple[list[str], list[str], str]] = []
    for digest in sorted(wt_by_digest):
        peers = head_by_digest.get(digest)
        if peers:
            rename_pairs.append((wt_by_digest[digest], peers, digest))

    for rel in only_wt:
        err(f"R5 only-in-worktree：{rel}——工作树有、HEAD 提交树无"
            f"（新增/未入库文件，未经 HEAD 树基线核对）")
    for rel in only_head:
        err(f"R5 only-in-head：{rel}——HEAD 提交树有、工作树无"
            f"（工作树缺件或旧树，当前判定对着残缺集合）")
    for rel in content_diff:
        err(f"R5 content-diff：{rel}——同名内容不同"
            f"（HEAD={head[rel][:12]} 工作树={worktree[rel][:12]}）")
    for wts, hds, digest in rename_pairs:
        err(f"R5 改名漂移：{'、'.join(hds)}（HEAD）→ {'、'.join(wts)}（工作树）"
            f"——内容摘要相同（{digest[:12]}），路径集合已漂移")

    if not (only_wt or only_head or content_diff):
        info(f"R5 跨树集合一致：{len(worktree)} 件（基线 {label}；"
             f"only-in-worktree=0 only-in-head=0 content-diff=0 改名漂移=0）")


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

    check_durability()
    check_crosstree()

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
        print(f"== strategies_check: 红 {len(errors)} 项（schema/index 计数/编译断言/跨树集合）")
        return 1
    n_compile = sum(1 for s in disk_states.values() if s == "compilable")
    # 诚实化（红队终局轮 RT-E81 修复批，2026-11-02）：--skip-compile 形态下**不得**再
    # 宣称「编译断言经 af-dsl 装载测试」——R4 编译断言面未跑就是未跑，无条件宣称即
    # 假陈述（该假陈述与 run.sh 接线弱化同型：门输出自称覆盖了实际没覆盖面）。
    if args.skip_compile:
        print(f"== strategies_check: 全绿（schema/index 面）——磁盘条目 {len(disk_states)}"
              f"（compilable={n_compile}），index 计数对账一致，R5 跨树集合一致；"
              f"R4 af-dsl 编译断言本形态**未执行**（--skip-compile 跳过，非 CI 常跑形态）")
    else:
        print(f"== strategies_check: 全绿——磁盘条目 {len(disk_states)}（compilable={n_compile}），"
              f"index 计数对账一致，R5 跨树集合一致，编译断言经 af-dsl 装载测试")
    return 0


if __name__ == "__main__":
    sys.exit(main())
