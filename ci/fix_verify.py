#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""跨树收口门（RT-E49 / RT-E97「修复耐久性未跨树收口」的机器收口面）。

背景（红队终局轮 RT-E49 / RT-E97 确证，本门为其拦截机制①②③的实现）：
  RT-E49「整槽连摘」修复（ci/xcheck.py gates-g0-erratum 批未齐分支改硬核
  numbers.json ci.steps_total 登记值；ci/gate_inputs_check.sh 门输入全集钉）
  与 RT-E6 判据正本钉一度只存于 g0merge / sil-writeback 两条线；SIL 守护回写
  基线 wb_base=phase2 与主干基线均不含该修复——守护自带回滚指令
  （tools/self-improve-loop.py：`git -C af branch -D sil-writeback` +
  `git worktree remove ~/workspace/af-wt-sil-writeback`）一旦执行，活跃树退回
  无 fix 态；守护再按 wb_base=phase2 重建工作树时重建出的是**无修复**的树
  ——「回滚即灭失」「重建即回到无 fix 态」。RT-E97 修复对账栏自记
  「跨树回放收口门（机制①③）未见」。

本门 = 该收口义务的机器实现（不弱化任何断言，红项一律硬红）：
  机制① 跨树回放：对名册在列的每棵**在役树**，把该树 ci/run.sh 的 step10/11/12
    三对账硬门槽**整槽摘除**（段注释 + echo 登记行 + step 调用行一并摘除，与红队
    2026-10-31 原样攻击同形），跑该树自己的 ci/xcheck.py → 必须 exit=1 且
    [FAIL] gates-g0-erratum（全红才判该树带修复）；对照臂（未篡改 run.sh）必须
    不红（exit≠1）。篡改件只落临时目录，仓内正本零改动。
  机制③ 单源对账：ci/xcheck.py 以**主干基线 ref**（--base-ref，缺省 phase2）为
    正本，各在役树 ci/xcheck.py 逐件 sha256 对账，分叉即红；非在役（存档/并行）
    树若带门码，其 sha256 必须在名册显式钉住（钉值漂移=静默改门即红）。
  机制② 落库收口：修复本体（门码 + 本收口门本体）必须在主干基线 ref 上在场
    ——内容级判定（跨树 cherry-pick 后 commit 拓扑不同、内容同，拓扑判定会误红）：
    基线含修复 ⇒ SIL 回写树按 wb_base 重建时必带修复 ⇒ 回滚不再灭失。

名册（ci/fix-verify.duty，正本=主干基线 ref 的已提交副本；--roster 仅测试覆写）：
  逐工作树登记 类别(duty/archived)、门码钉值、理由。**仓内每棵工作树必须在名册
  在列**（漏登记即红——防「缩名册把在役树摘出去」旁路）；archived 行必须带非空
  理由与门码钉值（无钉=未登记分叉即红）。名册改动必须走基线提交——工作树未提交
  编辑对门不可见（RT-E48「版本面外修复」模式在名册面上结构性排除）。

退出码：0=全绿；1=有红项（任一红项硬红，不降级为 advisory）。
用法：python3 ci/fix_verify.py [--repo DIR] [--base-ref REF] [--roster FILE]
"""
import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# E49 修复在 ci/xcheck.py 的内容级锚点（机制② 基线在场判定用）：
# ① read_steps_total_registered —— 登记值读取器（fail-closed 的读取侧）
# ② 整槽连摘 —— 硬红文案锚（漂移即红的判据侧）
ANCHOR_CODE = "def read_steps_total_registered("
ANCHOR_RED = "整槽连摘"

# 整槽连摘的槽位集合（红队 2026-10-31 原样攻击：step10/11/12 三对账硬门槽）
SLOTS = (10, 11, 12)
# 篡改后必须硬红的门名（ci/xcheck.py 的硬项名）
MUST_RED = "gates-g0-erratum"

RED = []
GREEN = []
WARN = []


def rec_red(msg):
    RED.append(msg)


def rec_green(msg):
    GREEN.append(msg)


def rec_warn(msg):
    WARN.append(msg)


def sh(cmd, cwd=None):
    p = subprocess.run(cmd, cwd=cwd, stdout=subprocess.PIPE,
                       stderr=subprocess.STDOUT, text=True)
    return p.returncode, p.stdout or ""


def git_show_bytes(repo, spec):
    """git show <spec> 的原始字节（字节级 sha256 对账用——文本模式会改换行）。"""
    p = subprocess.run(["git", "-C", repo, "show", spec],
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if p.returncode != 0:
        return None
    return p.stdout


def sha256_file(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def sha256_bytes(b):
    return hashlib.sha256(b).hexdigest()


# ---------- 名册 ----------
def parse_roster_text(text):
    """名册行格式：分支 | 类别 | 门码sha256(NONE) | 理由
    （'#' 开头与空行忽略；理由不得为空——空理由=未交代的缩名单册）。
    正本=主干基线 ref 的已提交副本（git show <base-ref>:ci/fix-verify.duty）——
    名册改动必须走提交：工作树未提交编辑对门不可见（RT-E48「工作树面外修复」
    模式在名册面上结构性排除）。"""
    out = {}
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = [x.strip() for x in line.split("|")]
        if len(parts) != 4:
            rec_red("名册行格式错（须 4 段「分支 | 类别 | 门码sha256 | 理由」）：%s" % raw)
            continue
        br, cls, sha, why = parts
        if cls not in ("duty", "archived"):
            rec_red("名册类别非法（duty/archived）：%s" % raw)
            continue
        if not why:
            rec_red("名册理由为空（缩名单册=旁路面）：%s" % raw)
            continue
        out[br] = {"class": cls, "sha": sha, "why": why}
    return out


# ---------- 整槽连摘篡改件构造 ----------
def build_tampered_run_sh(text):
    """把 step10/11/12 三槽整槽摘除（段注释 + echo 登记行 + step 调用行+续行）。

    返回 (新文本, 摘除槽位列表, 构造是否成功)。构造不成功=探针源漂移
    （run.sh 措辞/结构变了），调用方必须判红——探针自身失效不得静默放过。
    """
    lines = text.splitlines(keepends=True)
    spans = []
    ok = True
    for slot in SLOTS:
        hdr = echo = step = None
        for i, ln in enumerate(lines):
            if hdr is None and re.match(r"^#\s*-{2,}\s*%d\." % slot, ln):
                hdr = i
            if echo is None and re.match(r'^echo\s+"==\s*step%d\b' % slot, ln):
                echo = i
            if step is None and re.match(r'^step\s+%d\s+"' % slot, ln):
                step = i
        if echo is None or step is None:
            ok = False
            continue
        # step 调用块的结束：以 \ 结尾的行属同一调用，末行不带 \ 即块尾
        j = step
        while j < len(lines) and lines[j].rstrip("\n").endswith("\\"):
            j += 1
        start = hdr if (hdr is not None and hdr < echo) else echo
        spans.append((slot, start, j))
    for slot, s, e in sorted(spans, key=lambda t: -t[1]):
        del lines[s:e + 1]
    return "".join(lines), [s[0] for s in spans], ok


def probe_tree(tree, tmpdir, label):
    """在役树跨树回放（机制①）：篡改臂必须硬红、对照臂必须不红。

    返回 (ok, 详情行列表)。"""
    details = []
    ok = True
    run_sh = os.path.join(tree, "ci", "run.sh")
    xcheck = os.path.join(tree, "ci", "xcheck.py")
    if not (os.path.isfile(run_sh) and os.path.isfile(xcheck)):
        rec_red("[%s] 在役树缺 ci/run.sh 或 ci/xcheck.py——门族不在场，修复无从谈起" % label)
        return False, details
    text = Path(run_sh).read_text(encoding="utf-8")
    tampered, slots, built = build_tampered_run_sh(text)
    if not built:
        rec_red("[%s] 篡改件构造失败（run.sh 里 step10/11/12 槽位形态漂移）——探针源失效，"
                "按红处理（不得静默放过）" % label)
        return False, details
    if set(slots) != set(SLOTS):
        rec_red("[%s] 篡改件只摘到槽位 %s（应 %s）——探针构造不完整，红" % (label, slots, list(SLOTS)))
        return False, details
    tampered_path = os.path.join(tmpdir, "run-sh-tampered-%s.sh" % label)
    Path(tampered_path).write_text(tampered, encoding="utf-8")
    # 篡改臂：必须 exit=1 且 [FAIL] gates-g0-erratum
    rc, out = sh([sys.executable, "-I", xcheck, "--run-sh", tampered_path,
                  "--out", os.path.join(tmpdir, "xcheck-tampered-%s.txt" % label)],
                 cwd=tree)
    red_lines = [ln for ln in out.splitlines() if ln.startswith("[FAIL]")]
    hit = any(ln.strip() == "[FAIL] %s" % MUST_RED for ln in red_lines)
    if rc == 1 and hit:
        rec_green("[%s] 篡改臂：整槽连摘 step%s → exit=1 且 [FAIL] %s（修复在该树在场）"
                  % (label, "/".join(str(s) for s in slots), MUST_RED))
    else:
        ok = False
        rec_red("[%s] 篡改臂：整槽连摘 step%s → exit=%d，%s 硬红=%s——该树修复缺席"
                "（摘门保牌通道在该树敞开）" % (label, "/".join(str(s) for s in slots), rc,
                                            MUST_RED, "有" if hit else "无"))
        for ln in red_lines[:6]:
            details.append("        " + ln)
    # 对照臂：未篡改的 ci/run.sh 必须不红（exit≠1）
    rc2, out2 = sh([sys.executable, "-I", xcheck, "--run-sh", run_sh,
                    "--out", os.path.join(tmpdir, "xcheck-control-%s.txt" % label)],
                   cwd=tree)
    if rc2 == 1:
        ok = False
        rec_red("[%s] 对照臂：未篡改 ci/run.sh 本身红（exit=1）——先修正本再谈收口" % label)
        for ln in [x for x in out2.splitlines() if x.startswith("[FAIL]")][:6]:
            details.append("        " + ln)
    else:
        rec_green("[%s] 对照臂：未篡改 ci/run.sh exit=%d（不误红）" % (label, rc2))
    return ok, details


def main():
    ap = argparse.ArgumentParser(description="跨树收口门（RT-E49/E6 修复耐久性·机制①②③）")
    ap.add_argument("--repo", default=str(REPO), help="af 仓根（缺省本门所在树）")
    ap.add_argument("--base-ref", default=os.environ.get("FIX_VERIFY_BASE_REF", "phase2"),
                    help="主干基线 ref（门码正本来源；缺省 phase2）")
    ap.add_argument("--roster", default=None, help="在役树名册覆写（缺省读基线 ref 已提交副本 ci/fix-verify.duty）")
    args = ap.parse_args()

    repo = os.path.abspath(args.repo)

    # ---------- 树枚举 ----------
    rc, out = sh(["git", "-C", repo, "worktree", "list", "--porcelain"])
    if rc != 0:
        print("[FAIL] fix-verify: git worktree list 失败：\n" + out)
        return 1
    trees = []
    cur = {}
    for ln in out.splitlines():
        if ln.startswith("worktree "):
            if cur:
                trees.append(cur)
            cur = {"path": ln[len("worktree "):].strip()}
        elif ln.startswith("HEAD "):
            cur["head"] = ln[len("HEAD "):].strip()
        elif ln.startswith("branch "):
            cur["branch"] = ln[len("branch "):].strip().replace("refs/heads/", "")
    if cur:
        trees.append(cur)

    # 名册正本=主干基线 ref 的已提交副本（--roster 仅测试覆写用）：
    # 名册改动必须走基线提交，工作树未提交编辑对门不可见（RT-E48 模式结构性排除）。
    roster_text = None
    if args.roster:
        if os.path.isfile(args.roster):
            roster_text = Path(args.roster).read_text(encoding="utf-8")
        else:
            rec_red("--roster 指定文件缺席：%s" % args.roster)
    else:
        roster_bytes = git_show_bytes(repo, "%s:ci/fix-verify.duty" % args.base_ref)
        if roster_bytes is None:
            rec_red("在役树名册未落主干基线 %s:ci/fix-verify.duty——收口义务无版本面"
                    "正本（名册缺席或未提交到基线），红" % args.base_ref)
        else:
            roster_text = roster_bytes.decode("utf-8", "replace")
    roster = parse_roster_text(roster_text) if roster_text is not None else {}
    if roster_text is None:
        roster = {}

    # ---------- 名册覆盖（漏登记即红） ----------
    for t in trees:
        br = t.get("branch")
        key = br if br else os.path.basename(t["path"])
        if br is None or br not in roster:
            rec_red("工作树漏登记名册：%s（分支 %s）——缩名册即旁路，红"
                    % (t["path"], br or "(detached)"))

    # ---------- 机制②：主干基线 ref 在场（内容级） ----------
    base_xcheck = git_show_bytes(repo, "%s:ci/xcheck.py" % args.base_ref)
    if base_xcheck is None:
        rec_red("主干基线 ref %s 上无 ci/xcheck.py——门码正本不在基线，"
                "重建/回滚即回到无 fix 态，红" % args.base_ref)
        canonical_sha = None
    else:
        canonical_sha = sha256_bytes(base_xcheck)
        base_text = base_xcheck.decode("utf-8", "replace")
        if ANCHOR_CODE not in base_text or ANCHOR_RED not in base_text:
            rec_red("主干基线 ref %s 的 ci/xcheck.py 不含 E49 修复锚点"
                    "（%s / %s）——修复未落基线：SIL 守护按 wb_base 重建工作树即重建出"
                    "无 fix 态，红" % (args.base_ref, ANCHOR_CODE.strip(), ANCHOR_RED))
        else:
            rec_green("机制② 主干基线 %s 的 ci/xcheck.py 含 E49 修复锚点（sha256=%s…）"
                      % (args.base_ref, canonical_sha[:12]))
    base_numbers = git_show_bytes(repo, "%s:numbers.json" % args.base_ref)
    if base_numbers is None:
        rec_red("主干基线 ref %s 上无 numbers.json——E49 修复的登记值锚缺席（fail-closed 前提缺失），红"
                % args.base_ref)
    else:
        try:
            val = json.loads(base_numbers.decode("utf-8")).get("numbers", {}) \
                .get("ci.steps_total", {}).get("value")
        except Exception as e:  # noqa: BLE001
            val = None
            rec_red("主干基线 ref %s 的 numbers.json 不可解析（%s），红" % (args.base_ref, e))
        if val is None:
            rec_red("主干基线 ref %s 的 numbers.json 缺 ci.steps_total 登记键"
                    "——E49 修复的登记值锚缺席（修复会 fail-closed 永久红），红" % args.base_ref)
        else:
            rec_green("主干基线 %s 的 numbers.json ci.steps_total=%s（登记值锚在场）"
                      % (args.base_ref, val))
    # 本收口门本体也须在基线在场（否则门自身会被 checkout/merge 抹掉）
    base_gate = git_show_bytes(repo, "%s:ci/fix_verify.py" % args.base_ref)
    if base_gate is None:
        rec_red("主干基线 ref %s 上无 ci/fix_verify.py——收口门本体未落基线，"
                "下一次 checkout/merge 即灭失，红" % args.base_ref)
    else:
        # 比对对象=运行中的门自身（__file__）——本门可带 --repo 指向不含门本体的树
        # （如 guardrail-fix2 主检出），此时 <repo>/ci/fix_verify.py 缺席属正常，
        # 不参与本对账（该树是否带门族由名册 archived/duty 行裁定）。
        mine = sha256_file(str(Path(__file__).resolve()))
        if sha256_bytes(base_gate) != mine:
            rec_red("主干基线 %s 的 ci/fix_verify.py 与本门不同源（sha256 不等）——"
                    "门本体分叉即红" % args.base_ref)
        else:
            rec_green("机制② 收口门本体已落主干基线 %s（sha256 与本门同源）" % args.base_ref)

    # ---------- 在役树回放 + 单源对账 ----------
    tmpdir = tempfile.mkdtemp(prefix="fix-verify-")
    try:
        for t in trees:
            br = t.get("branch")
            if br is None:
                continue
            ent = roster.get(br)
            if not ent:
                continue  # 漏登记已在上方判红
            tree = t["path"]
            label = br
            xcheck = os.path.join(tree, "ci", "xcheck.py")
            if ent["class"] == "duty":
                # 机制③：门码单源（以主干基线为正本）
                if os.path.isfile(xcheck):
                    got = sha256_file(xcheck)
                    if canonical_sha and got != canonical_sha:
                        rec_red("[%s] 门码分叉：ci/xcheck.py sha256=%s… ≠ 主干基线正本 %s…"
                                "——分叉即红（机制③）" % (label, got[:12], canonical_sha[:12]))
                    else:
                        rec_green("[%s] 门码单源：ci/xcheck.py sha256=%s…（与主干基线正本一致）"
                                  % (label, got[:12]))
                # 机制①：跨树回放
                probe_tree(tree, tmpdir, label)
            else:
                # archived：门码须被名册显式钉住（钉值漂移=静默改门即红）
                if not os.path.isfile(xcheck):
                    if ent["sha"] not in ("NONE", "-", ""):
                        rec_red("[%s] 名册钉了门码 sha256=%s… 但该树已无 ci/xcheck.py"
                                "——钉与实物不符，红" % (label, ent["sha"][:12]))
                    else:
                        rec_green("[%s] 存档树：无 G0 门族（无 ci/xcheck.py），名册登记一致"
                                  % label)
                else:
                    got = sha256_file(xcheck)
                    if ent["sha"] in ("NONE", "-", ""):
                        rec_red("[%s] 该树带 ci/xcheck.py（sha256=%s…）但名册未钉——"
                                "未登记的分叉即红（机制③）" % (label, got[:12]))
                    elif ent["sha"] != got:
                        rec_red("[%s] 存档树门码漂移：sha256=%s… ≠ 名册钉值 %s…——"
                                "静默改门即红（机制③）" % (label, got[:12], ent["sha"][:12]))
                    else:
                        rec_green("[%s] 存档树门码与名册钉值一致（sha256=%s…，修复已由基线承担）"
                                  % (label, got[:12]))
    finally:
        try:
            for f in os.listdir(tmpdir):
                os.unlink(os.path.join(tmpdir, f))
            os.rmdir(tmpdir)
        except OSError:
            pass

    # ---------- 汇总 ----------
    for m in GREEN:
        print("[PASS] " + m)
    for m in WARN:
        print("[WARN] " + m)
    for m in RED:
        print("[FAIL] " + m)
    print("== fix-verify: 绿 %d / 警告 %d / 红 %d（工作树 %d 棵，名册 %d 条）"
          % (len(GREEN), len(WARN), len(RED), len(trees), len(roster)))
    return 1 if RED else 0


if __name__ == "__main__":
    sys.exit(main())
