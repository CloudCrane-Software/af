#!/usr/bin/env python3
"""trigger-inbox-src-parity-probe — 两处 `tools/trigger-inbox.py` 同源纪律的机检

发现（2026-11-04 v0.3-T5②/T6③ 触发件返修批，任务书 §四③ 亲核盘面）：
契约 `poc/poc-super-agent/contracts/af.trigger.v1.md` 与 ADR-015 决策 1 定了
**两处 `tools/trigger-inbox.py` 必须逐字同源**（af 仓 `tools/` ＝ 版本受控正本；
工作区级 `tools/` ＝ 在役运行位，keepalive / PoC 自检面 / 生产日志都指向它）。
但**两处之间没有任何机械保障**：
  · 没有 installer（工作区根**不是 git 仓**：`git rev-parse --show-toplevel` → fatal
    ⇒ 工作区那份的改动**连 diff 都没有**，只有 mtime）；
  · 没有探针（改一处忘另一处 ⇒ 两处静默漂移 ⇒ 在役跑的不是受控正本那版）；
  · 既有件对这两处**本就分叉**（`gate-item-emit.py` 405d23f8… vs 4eae9c05…、
    `gate-item-legacy-bridge.py` 不同值、`worktree-recycle.py` /
    `worktree-recycle-selfcheck.py` 只在 af 侧有）⇒ **不能用「两处 tools/ 整体
    一样」当判据**，只能逐件点名对账。

本门＝那件**只读**探针（只 sha256 两个文件，零写入、零网络、零监听、零换码）：
  · A ＝ af 仓版本受控正本 `tools/trigger-inbox.py`（**默认＝本探针同目录**，
    与正本同域 ⇒ 探针与被探件不会分家）；
  · B ＝ 工作区级在役运行位 `tools/trigger-inbox.py`
    （默认 `${AF_WORKSPACE:-/home/pan-ding/workspace/agent黑灯工厂}/tools/trigger-inbox.py`）。

判定（**任一不过 ⇒ 非零退出＝红；不静默 SKIP、不猜绿**）：
  · 两处 sha256 **相等**  ⇒ 绿（逐字同源成立）；
  · 两处 sha256 **不等**  ⇒ 红（漂移：改了一处忘了另一处；在役跑的不是受控正本那版），
    **红文案必须报出两处各自的 sha256**（否则无从判断是谁被换）；
  · 任一件**缺件 / 不可读** ⇒ 红（对账不可证 ⇒ 按红处置，同 all-gates
    「缺件/前置不满足也按红点名对账」口径，不静默放行）。

**本批边界**：本探针**只交本体**。挂载（在役发布面 `tools/all-gates.sh` 登记门项）
**另立一批**——本批禁改既有发布面（`tools/deploy-consistency-probe.sh` 的 `PIN_MD5`
钉桩**逐字未动**；那是 af-jobs-portal 的钉桩，改它会触发钉桩重锚）。挂载方案见
`poc/poc-t5-2-t6-3-trigger-resident/README.md` §8。

退出码：
  0 ＝ 绿（两处逐字同源）
  1 ＝ 红（不一致 / 缺件 / 不可读）——红文案含字面量 `FAIL`，供 all-gates 的
       `grep -m1 -E 'FAIL|Error|error|Traceback|assert'` 摘到可读说明
  2 ＝ 用法错（argparse）

只读性自证：`--selftest` 用**临时目录夹具**（`tempfile.mkdtemp`，测完自清）
证明本探针**会咬人**——①两处内容不同 ⇒ 必须红；②两处内容相同 ⇒ 必须绿；
③差一个字节 ⇒ 必须红；④缺件 ⇒ 必须红。selftest 全部符合预期 ⇒ exit 0。
"""

import argparse
import hashlib
import os
import shutil
import subprocess
import sys
import tempfile

# ── 落点常量（与被探件同域：A 默认＝本探针所在目录）────────────────────────
PROBE_DIR = os.path.dirname(os.path.abspath(__file__))
DEFAULT_A = os.path.join(PROBE_DIR, "trigger-inbox.py")
DEFAULT_WORKSPACE = os.environ.get(
    "AF_WORKSPACE", "/home/pan-ding/workspace/agent黑灯工厂")
DEFAULT_B = os.path.join(DEFAULT_WORKSPACE, "tools", "trigger-inbox.py")

# 退出码
RC_GREEN = 0
RC_RED = 1


def sha256_of(path):
    """只读算 sha256。返回 (sha256hex|None, 错误串|None)。不写、不改、不删。"""
    try:
        h = hashlib.sha256()
        with open(path, "rb") as f:          # 只读打开（'rb'），无写模式
            for chunk in iter(lambda: f.read(1 << 16), b""):
                h.update(chunk)
        return h.hexdigest(), None
    except OSError as e:
        return None, f"{type(e).__name__}: {e}"


def compare(a_path, b_path):
    """两处同源对账。返回 dict（verdict: ALIGNED / DRIFT / UNPROVABLE）。"""
    sa, ea = sha256_of(a_path)
    sb, eb = sha256_of(b_path)
    res = {"a": a_path, "b": b_path, "sha_a": sa, "sha_b": sb,
           "err_a": ea, "err_b": eb}
    if sa is None or sb is None:
        res["verdict"] = "UNPROVABLE"
        res["reason"] = "缺件/不可读 ⇒ 对账不可证 ⇒ 按红处置"
    elif sa != sb:
        res["verdict"] = "DRIFT"
        res["reason"] = "两处内容不一致（sha256 不等）⇒ 漂移：改一处忘另一处"
    else:
        res["verdict"] = "ALIGNED"
        res["reason"] = "两处逐字同源（sha256 相等）"
    return res


def render(res, as_json=False):
    """出报告。末行给机器可读摘要（all-gates 取 tail -1）。"""
    sha_a = res["sha_a"] or "MISSING"
    sha_b = res["sha_b"] or "MISSING"
    if as_json:
        import json
        out = dict(res)
        out["sha_a"] = res["sha_a"]
        out["sha_b"] = res["sha_b"]
        return json.dumps(out, ensure_ascii=False, sort_keys=True)
    lines = []
    if res["verdict"] == "ALIGNED":
        lines.append("[GREEN] trigger-inbox 两处同源对账通过：sha256 相等")
        lines.append(f"  A(af 仓正本)    = {res['a']}  sha256={sha_a}")
        lines.append(f"  B(工作区在役位) = {res['b']}  sha256={sha_b}")
        lines.append(f"VERDICT=ALIGNED sha_a={sha_a} sha_b={sha_b} a={res['a']} b={res['b']}")
    else:
        lines.append(f"[FAIL] trigger-inbox 两处同源对账红：{res['reason']}")
        if res["err_a"]:
            lines.append(f"  A(af 仓正本)    = {res['a']}  <不可读: {res['err_a']}>")
        else:
            lines.append(f"  A(af 仓正本)    = {res['a']}  sha256={sha_a}")
        if res["err_b"]:
            lines.append(f"  B(工作区在役位) = {res['b']}  <不可读: {res['err_b']}>")
        else:
            lines.append(f"  B(工作区在役位) = {res['b']}  sha256={sha_b}")
        lines.append("  处置：以 af 仓正本为上游，把工作区在役运行位同步成同一份字节"
                     "（改 af 仓 ⇒ 同步工作区；改工作区 ⇒ 回收进 af 仓）。")
        lines.append(f"VERDICT={res['verdict']} FAIL sha_a={sha_a} sha_b={sha_b} "
                     f"a={res['a']} b={res['b']}")
    return "\n".join(lines)


def _run_self(argv):
    """以子进程跑本探针（-I 隔离模式），取退出码——证明**可执行体**本身会红会绿。"""
    cmd = [sys.executable, "-I", os.path.abspath(__file__)] + argv
    p = subprocess.run(cmd, capture_output=True, text=True)
    return p.returncode, p.stdout


def selftest():
    """判别力自证：夹具四例，全部用**子进程**跑本体（不是调函数）⇒ 证的是可执行面。"""
    d = tempfile.mkdtemp(prefix="trigger-inbox-parity-selftest-")
    try:
        a = os.path.join(d, "a.py")
        b = os.path.join(d, "b.py")
        cases = []

        # ① 两处内容不同 ⇒ 必须红
        with open(a, "w", encoding="utf-8") as f:
            f.write("# upstream\nprint('upstream')\n")
        with open(b, "w", encoding="utf-8") as f:
            f.write("# upstream\nprint('DRIFTED')\n")
        rc, out = _run_self(["--a", a, "--b", b])
        cases.append(("两处内容不同 ⇒ 必须红", rc == RC_RED, rc, out))

        # ② 两处内容相同（不同文件名）⇒ 必须绿
        c = os.path.join(d, "c.py")
        with open(c, "w", encoding="utf-8") as f:
            f.write("# upstream\nprint('upstream')\n")
        rc, out = _run_self(["--a", a, "--b", c])
        cases.append(("两处内容相同 ⇒ 必须绿", rc == RC_GREEN, rc, out))

        # ③ 差一个字节 ⇒ 必须红
        dfile = os.path.join(d, "d.py")
        with open(a, "rb") as f:
            raw = f.read()
        with open(dfile, "wb") as f:
            f.write(raw + b" ")
        rc, out = _run_self(["--a", a, "--b", dfile])
        cases.append(("差一个字节 ⇒ 必须红", rc == RC_RED, rc, out))

        # ④ 缺件 ⇒ 必须红（对账不可证不猜绿）
        missing = os.path.join(d, "not-there.py")
        rc, out = _run_self(["--a", a, "--b", missing])
        cases.append(("缺件 ⇒ 必须红（不可证不猜绿）", rc == RC_RED, rc, out))

        ok = True
        print("== trigger-inbox-src-parity-probe --selftest（判别力自证）==")
        for name, passed, rc, out in cases:
            print(f"[{'PASS' if passed else 'FAIL'}] {name}（实测 rc={rc}）")
            if not passed:
                ok = False
                print("  --- 探针输出 ---")
                for ln in out.splitlines():
                    print("  " + ln)
        print(f"SELFTEST_VERDICT={'ALIGNED_PROBE_WORKS' if ok else 'PROBE_DOES_NOT_BITE'}")
        return RC_GREEN if ok else RC_RED
    finally:
        shutil.rmtree(d, ignore_errors=True)


def main(argv=None):
    ap = argparse.ArgumentParser(
        description="两处 tools/trigger-inbox.py 同源纪律的只读 sha256 对账探针")
    ap.add_argument("--a", default=DEFAULT_A,
                    help=f"A 侧路径（af 仓版本受控正本）；缺省 {DEFAULT_A}")
    ap.add_argument("--b", default=DEFAULT_B,
                    help=f"B 侧路径（工作区级在役运行位）；缺省 {DEFAULT_B}")
    ap.add_argument("--selftest", action="store_true",
                    help="判别力自证：夹具两处不同⇒红、一致⇒绿、差一字节⇒红、缺件⇒红")
    ap.add_argument("--json", action="store_true", help="机器可读（JSON）输出")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    res = compare(args.a, args.b)
    print(render(res, as_json=args.json))
    return RC_GREEN if res["verdict"] == "ALIGNED" else RC_RED


if __name__ == "__main__":
    sys.exit(main())
