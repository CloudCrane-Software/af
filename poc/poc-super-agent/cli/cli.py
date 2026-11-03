#!/usr/bin/env python3
"""cli/cli.py — Router 最小皮·CLI 骨架（PoC-D）

首版只对订单/事件说话（T6.1 裁决：**无订单纯对话不被允许**——纯对话回无
admit 路径，人类非订单交互走现有渠道）。三个入口：

  decide --dispatch <派单记录.json>   Slow Path 六步（生产路径：Reviser/Revise）
  probe  --dispatch <派单记录.json> --role R --decision D
                                     负例探针（预期内核四层校验必拒；皮层
                                     allow_probe 显式声明，产物标 probe 不留档）
  status                             触发层观测：注册表/收件箱活性安灯/派单台账
  chat <任意文本>                    **必拒**（T6.1 机械面：无订单不对话）

用法（沙箱自检形态）：
  AF_TRIGGER_HOME=<home> python3 cli/cli.py decide \
      --dispatch <home>/dispatch/<order_id>.json
"""
import glob
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
POC = os.path.dirname(HERE)
sys.path.insert(0, os.path.join(POC, "admit"))
sys.path.insert(0, os.path.join(POC, "router"))
import guard            # noqa: E402
import slow_path        # noqa: E402

WS = "/home/pan-ding/workspace/agent黑灯工厂"
TRIGGER_HOME = os.environ.get("AF_TRIGGER_HOME", WS + "/af-state/triggers")


def _load(path):
    return json.load(open(path, encoding="utf-8"))


def cmd_decide(argv):
    import argparse
    ap = argparse.ArgumentParser(prog="cli decide")
    ap.add_argument("--dispatch", required=True, help="派单记录 JSON（事件→订单锚点）")
    ap.add_argument("--out-root", default=os.path.join(TRIGGER_HOME, "slowpath"))
    a = ap.parse_args(argv)
    disp = _load(a.dispatch)
    guard.check_context(disp)                 # 无订单不对话（皮层第一道）
    reg = _load(os.path.join(TRIGGER_HOME, "registry.json"))
    trail = slow_path.run_slow_path(disp, reg, a.out_root)
    print(json.dumps(trail, ensure_ascii=False, indent=1))
    return 0 if trail.get("result") == "admitted" else 1


def cmd_probe(argv):
    import argparse
    ap = argparse.ArgumentParser(prog="cli probe")
    ap.add_argument("--dispatch", required=True)
    ap.add_argument("--role", required=True,
                    choices=["Approver", "Arbiter", "Reader", "Reviser"])
    ap.add_argument("--decision", required=True,
                    choices=["Approve", "Reject", "Revise", "Arbitrate"])
    a = ap.parse_args(argv)
    disp = _load(a.dispatch)
    reg = _load(os.path.join(TRIGGER_HOME, "registry.json"))
    trail = slow_path.run_slow_path(disp, reg, "/tmp/pocd-slowpath-probe",
                                    allow_probe=True, probe_role=a.role,
                                    probe_decision=a.decision)
    print(json.dumps(trail, ensure_ascii=False, indent=1))
    # 探针的成功条件=内核拒绝（layer4 红）——探针绿=内核失守，rc=2
    if trail.get("result") == "rejected-at-layer4":
        return 0
    return 2


def cmd_status(_argv):
    reg_p = os.path.join(TRIGGER_HOME, "registry.json")
    reg = _load(reg_p) if os.path.exists(reg_p) else None
    inbox = sorted(glob.glob(os.path.join(TRIGGER_HOME, "inbox", "*", "*.json")))
    disp = sorted(glob.glob(os.path.join(TRIGGER_HOME, "dispatch", "*.json")))
    andon_p = os.path.join(WS, "状态观测", "trigger-inbox-liveness.json")
    andon = _load(andon_p) if os.path.exists(andon_p) else None
    print(json.dumps({
        "registry": "in-place" if reg else "missing",
        "sources": [s["name"] for s in (reg or {}).get("sources", [])],
        "inbox_deliveries": len(inbox),
        "dispatch_records": len(disp),
        "dispatch_ids": [os.path.basename(p)[:-5] for p in disp[-5:]],
        "liveness": andon,
    }, ensure_ascii=False, indent=1))
    return 0


def cmd_chat(argv):
    """T6.1 机械面：无订单纯对话必拒——不进决策、不进 admit，直接 3 号退出。"""
    text = " ".join(argv)
    print(json.dumps({
        "refused": True,
        "reason": ("无订单纯对话不被允许（v0.3 T6.1 终裁）：Router 首版只对订单/事件"
                   "说话，纯对话无 admit 路径；人类非订单交互走现有渠道。"),
        "your_input": text[:120],
        "allowed_entrypoints": ["decide --dispatch <派单记录>", "status"],
    }, ensure_ascii=False, indent=1))
    return 3


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, rest = argv[0], argv[1:]
    return {"decide": cmd_decide, "probe": cmd_probe, "status": cmd_status,
            "chat": cmd_chat}.get(cmd, lambda r: (print(f"未知子命令: {cmd}"), 2)[1])(rest)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
