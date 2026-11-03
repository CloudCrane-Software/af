#!/usr/bin/env python3
"""install_registry.py — 触发层注册表安装（正本→运行位）

正本=poc/poc-super-agent/registry/trigger-registry.v1.json（版本控制）；
运行位=<AF_TRIGGER_HOME>/registry.json（工具默认读取处）。安装时算定
recipe_digest（=除 recipe_digest 与 $comment 外内容的规范 JSON sha256）写进
运行位——order_id 确定性生成吃它（同注册表内容 ⇒ 同 digest ⇒ 同单号）。
幂等：重复安装覆盖为最新正本。"""
import hashlib
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CANON = os.path.join(os.path.dirname(HERE), "registry", "trigger-registry.v1.json")


def build(seed_path, out_path):
    reg = json.load(open(seed_path, encoding="utf-8"))
    body = {k: v for k, v in reg.items() if k not in ("recipe_digest", "$comment")}
    rd = hashlib.sha256(
        json.dumps(body, ensure_ascii=False, sort_keys=True).encode("utf-8")).hexdigest()
    body["recipe_digest"] = rd
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    tmp = out_path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(body, f, ensure_ascii=False, indent=1)
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, out_path)
    return rd


if __name__ == "__main__":
    seed = sys.argv[1] if len(sys.argv) > 1 else CANON
    out = sys.argv[2] if len(sys.argv) > 2 else os.path.join(
        os.environ.get("AF_TRIGGER_HOME",
                       "/home/pan-ding/workspace/agent黑灯工厂/af-state/triggers"),
        "registry.json")
    rd = build(seed, out)
    print(json.dumps({"installed": out, "seed": seed, "recipe_digest": rd}))
