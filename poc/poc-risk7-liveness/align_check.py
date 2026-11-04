#!/usr/bin/env python3
"""风险#7 活性安灯：Rust 腿（af-order/router_liveness）↔ PoC-D 腿
（tools/event-router.py 的 _trigger_liveness_andon）四场景对齐复算。

对齐正本的取件纪律：`tools/event-router.py` 在外层工作区、本机不在位；
本机版本控制正本 = 仓内镜像 `poc/poc-super-agent/tests/event-router-poCD.diff`
（含 `_trigger_liveness_andon` 全文）。本 harness 从该 diff **逐字提取**函数体
（不重打一遍，防手抄走样；提取件 sha256 记入报告），配齐它引用的模块级
依赖（DRY/log/TRIGGER_* 常量/伪 time）后原样执行。

两腿对同一 fixtures（同 registry.json / 同 inbox 文件 / 同 os.utime 钉死的
mtime / 同调用时刻序列）各跑一遍，逐字段对拍：
status / alarm_code / threshold_min / since_epoch / newest_delivery_age_s /
message / 快照 JSON（tool 字段按设计两腿不同值，单独登记）。

用法：python3 -I align_check.py <repo-root> <rust-demo-bin> <work-dir>
"""
import hashlib
import json
import os
import subprocess
import sys
import tempfile

DIFF_REL = "poc/poc-super-agent/tests/event-router-poCD.diff"
FUNC_NAME = "_trigger_liveness_andon"
T0 = 1_700_000_000.0  # 钉死首跑锚点时刻（两腿同值）

# 四场景（两腿同调用序列）：
#   calls = 传给腿的评估时刻序列（首元素=首跑，写锚点/started_at）
#   delivery_mtime = 收件箱投递文件钉死的 mtime（None = 收件箱空）
SCENARIOS = [
    # ① 收件箱有近投递（mtime=t0+1h，评估于 m+120s）⇒ green，起算=最新投递
    dict(name="s1-recent-delivery", liveness=None,
         delivery_mtime=T0 + 3600.0, calls=[T0, T0 + 3600.0 + 120.0]),
    # ② 收件箱空但刚起跑（单次评估=首跑）⇒ green，不冤枉刚部署
    dict(name="s2-just-started", liveness=None,
         delivery_mtime=None, calls=[T0]),
    # ③ 收件箱空且锚点已久（251min）⇒ amber，起算=首跑锚点
    dict(name="s3-stale-anchor", liveness=None,
         delivery_mtime=None, calls=[T0, T0 + 251.0 * 60.0]),
    # ④a 注册表缺 liveness 段 ⇒ 回落 240（239min 仍 green）
    dict(name="s4a-missing-liveness-fallback-240", liveness=None,
         delivery_mtime=None, calls=[T0, T0 + 239.0 * 60.0]),
    # ④b 注册表阈值=0 ⇒ 立即 amber（0 不是被当缺省）
    dict(name="s4b-zero-threshold-immediate-amber", liveness=0,
         delivery_mtime=None, calls=[T0, T0 + 2.0]),
]


def extract_function(diff_path):
    """从 diff 镜像逐字提取 _trigger_liveness_andon 函数源（含 docstring 与
    内层 import；到下一个 `+def ` 为止）。返回 (source, sha256)。"""
    lines = open(diff_path, encoding="utf-8").read().splitlines()
    start = end = None
    for i, ln in enumerate(lines):
        if ln.startswith(f"+def {FUNC_NAME}("):
            start = i
        elif start is not None and ln.startswith("+def "):
            end = i
            break
    if start is None or end is None:
        raise SystemExit(f"diff 里找不到 {FUNC_NAME} 完整函数体")
    body = [ln[1:] for ln in lines[start:end]]           # 去掉 diff 的 '+' 前缀
    while body and not body[-1].strip():
        body.pop()                                        # 去尾部空行
    source = "\n".join(body) + "\n"
    return source, hashlib.sha256(source.encode()).hexdigest()


class FakeTime:
    """伪 time 模块：time() 返回钉死的评估时刻；strftime 按该时刻的本地时间
    排版（函数只用了这两个面）。"""

    def __init__(self, workdir):
        self.now = T0
        self._real = __import__("time")
        self.workdir = workdir

    def time(self):
        return self.now

    def strftime(self, fmt):
        return self._real.strftime(fmt, self._real.localtime(self.now))


def build_fixtures(root, scenario):
    """造运行位：registry.json（liveness 段可配）+ inbox 投递（mtime 钉死）。
    两腿共用同一份 fixtures。"""
    home = root / scenario["name"] / "triggers"
    (home / "inbox" / "mes-execline").mkdir(parents=True)
    reg = {"schema": "af.trigger.registry.v1",
           "sources": [{"name": "mes-execline", "domain": "mes",
                        "events": ["order_complete"]}]}
    if scenario["liveness"] is not None:
        reg["liveness"] = {"inbox_silent_after_min": scenario["liveness"]}
    (home / "registry.json").write_text(json.dumps(reg, ensure_ascii=False),
                                        encoding="utf-8")
    if scenario["delivery_mtime"] is not None:
        delivery = home / "inbox" / "mes-execline" / "d-1.json"
        delivery.write_text('{"event":"order_complete"}', encoding="utf-8")
        os.utime(delivery, (scenario["delivery_mtime"], scenario["delivery_mtime"]))
    return home


def run_python_leg(func, namespace, scenario, home):
    """PoC-D 腿：对同一 fixtures 按同一时刻序列调用提取的正本函数。
    返回末次读数（snap dict）与 andon 快照落盘路径。"""
    andon_dir = home.parent / "py-andon"
    namespace["TRIGGER_INBOX_D"] = str(home / "inbox")
    namespace["TRIGGER_ANDON_DIR"] = str(andon_dir)
    namespace["TRIGGER_ANDON_F"] = str(andon_dir / "trigger-inbox-liveness.json")
    namespace["TRIGGER_ALARMS_F"] = str(andon_dir / "alarms.log")
    namespace["TRIGGER_ANDON_REFRESH_S"] = 6 * 3600
    reg = json.loads((home / "registry.json").read_text(encoding="utf-8"))
    snap = None
    st = {}
    for moment in scenario["calls"]:
        namespace["time"].now = moment
        snap = func(reg, st)
    return snap, namespace["TRIGGER_ANDON_F"]


def run_rust_leg(demo, scenario, home):
    """Rust 腿：对同一 fixtures 按同一时刻序列跑 demo（首跑写锚点文件）。
    返回末次 stdout 解析 + 快照落盘路径。"""
    out = home.parent / "rust-snap" / "trigger-inbox-liveness.json"
    reading = None
    for moment in scenario["calls"]:
        proc = subprocess.run(
            [str(demo), str(home), str(out), repr(moment)],
            capture_output=True, text=True, check=True,
        )
        reading = dict(
            pair.split("=", 1)
            for pair in proc.stdout.strip().split()
            if "=" in pair
        )
    return reading, out


def main():
    repo, demo, workdir = (os.path.abspath(p) for p in sys.argv[1:4])
    os.makedirs(workdir, exist_ok=True)
    source, digest = extract_function(os.path.join(repo, DIFF_REL))
    print(f"[extract] {FUNC_NAME} 自 diff 镜像逐字提取，sha256={digest}")
    print(f"[extract] 首行: {source.splitlines()[0]}")

    import pathlib
    root = pathlib.Path(tempfile.mkdtemp(dir=workdir, prefix="fixtures-"))
    rows = []
    failures = 0
    try:
        for scenario in SCENARIOS:
            home = build_fixtures(root, scenario)

            # ── Python 腿（diff 镜像正本函数，逐字执行）──────────────
            fake_time = FakeTime(root)
            namespace = {
                "os": os, "json": json, "time": fake_time,
                "DRY": False,  # 让它真写自己的 andon 快照（对拍用）
                "log": lambda msg: print(f"  [py-log] {msg}"),
            }
            exec(compile(source, f"<{FUNC_NAME}@diff-mirror>", "exec"),  # noqa: S102
                 namespace)
            py_snap, py_andon_f = run_python_leg(namespace[FUNC_NAME],
                                                 namespace, scenario, home)
            py_andon = json.loads(open(py_andon_f, encoding="utf-8").read())

            # ── Rust 腿（af-order/router_liveness demo）───────────────
            rust, rust_snap_f = run_rust_leg(demo, scenario, home)
            rs_andon = json.loads(open(rust_snap_f, encoding="utf-8").read())

            # ── 对拍（读数面 + 快照面）────────────────────────────────
            # silent_s 原值函数不外回，按钉死时刻精确复算（since 为整秒）。
            py_silent_exact = scenario["calls"][-1] - float(py_snap["since_epoch"])
            py_anchor = (
                "newest_delivery"
                if scenario["delivery_mtime"] is not None
                and float(py_snap["since_epoch"]) == scenario["delivery_mtime"]
                else "first_run_anchor"
            )
            checks = {
                "status": (py_snap["status"], rust["status"]),
                "alarm_code": (py_snap["alarm_code"], rust["alarm_code"]),
                "threshold_min": (
                    py_snap["threshold_min"], int(rust["threshold_min"])),
                "since_epoch": (
                    int(py_snap["since_epoch"]), int(float(rust["since_epoch_s"]))),
                "newest_delivery_age_s": (
                    py_snap["newest_delivery_age_s"], int(float(rust["silent_s"]))),
                "silent_s(浮点)": (round(py_silent_exact, 6),
                                   round(float(rust["silent_s"]), 6)),
                "message": (py_snap["message"], rs_andon["message"]),
                "anchor_source": (py_anchor, rust["anchor_source"]),
                "快照tool(设计差)": (py_andon["tool"], rs_andon["tool"]),
                "快照ts_epoch": (py_andon["ts_epoch"], rs_andon["ts_epoch"]),
                "快照ts": (py_andon["ts"], rs_andon["ts"]),
            }
            for field, (want, got) in checks.items():
                ok = want == got
                # 快照 tool 是**登记的设计差**（两腿各自署名，互覆时可归因；
                # 字段语义=写者标识，非判据）——不算失败，单独标 DIFF。
                designed = field.startswith("快照tool")
                if not ok and not designed:
                    failures += 1
                mark = "PASS" if ok else ("DIFF(设计)" if designed else "**FAIL**")
                rows.append((scenario["name"], field, want, got, mark))
        # 结果表
        print(f"\n{'场景':<38}{'字段':<22}{'PoC-D':<28}{'Rust':<28}结果")
        for name, field, want, got, mark in rows:
            print(f"{name:<40}{field:<24}{str(want):<30}{str(got):<30}{mark}")
        total = len(rows)
        print(f"\n对拍结论: 判据字段 {total - 5} 项全部逐字一致；"
              f"快照 tool 字段 5 项为登记的设计差（两腿各自署名）。"
              f"{'✅ 对齐' if failures == 0 else f'❌ {failures} 项判据不一致'}")
        return 0 if failures == 0 else 1
    finally:
        print(f"[fixtures] 留档 {root}")


if __name__ == "__main__":
    sys.exit(main())
