#!/usr/bin/env python3
"""exp1-repeats-mkspec — 生成 EXP-1 repeat 轮 af-order submit 的 spec JSON。

与 计划/研究组/B线核心批-20261029/exp1-tools/exp1-mkspec.py 同源（主轮版），
本件为 repeat 轮专用：note 段标注 round=repeat / attempt=2，便于事后在 MES
ledger 与工件里区分主轮与第二跑（不改 spec 字段闭集，不改任何判据）。

spec 字段闭集（af-order crates/af-order/src/spec.rs:13-33）：
    order_id / line / budget / note
order_id 同时是 state/orders/ 目录名（路径安全：无分隔符、无空白、非 . / ..）。

用法：
    exp1-repeats-mkspec.py <spec_path> <order_id> <pkg> <line_id> <line_version> <budget>
"""
import json
import sys


def main():
    if len(sys.argv) != 7:
        print("用法: exp1-repeats-mkspec.py <spec_path> <order_id> <pkg> <line_id> <line_version> <budget>",
              file=sys.stderr)
        return 2
    spec_path, order_id, pkg, line_id, line_version, budget = sys.argv[1:7]
    spec = {
        "order_id": order_id,
        "line": {"line_id": line_id, "line_version": line_version},
        "budget": int(budget),
        "note": (f"EXP-1 repeat 轮（round=repeat attempt=2，A0/A1/A2 第二跑）"
                 f" arm-pkg={pkg} unit=report profile=glm-cp"),
    }
    with open(spec_path, "w", encoding="utf-8") as f:
        json.dump(spec, f, ensure_ascii=False)
    return 0


if __name__ == "__main__":
    sys.exit(main())
