#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""make_overlay_registry.py — 构建覆盖层组件注册表（af/packages 的临时全量拷贝
+ 补上策略条目引用而生产注册表缺失的 prompt 模板）。

为什么需要覆盖层：生产注册表 af/packages 只有 8 份模板（sweng 6 + reportkit 2），
而 32 条 compilable 条目引用的 `pkg.*_system` 模板无一在场（闭环断点的量化
读数之一，见 README §四）。真执行器 planbind 装订④对模板逐件 sha256 清单核对，
缺件即 EX4xxx 拒执行。本脚本不改动生产注册表一个字节——拷贝到 run/registry
（证据目录内），仅在拷贝上补 st-b-C037 需要的两件：

  pkg.explore.explorer_system → <overlay>/explore/prompts/explorer_system.md
  pkg.sweng.coder_system      → <overlay>/sweng/prompts/coder_system.md（追加进
                                 拷贝的 MANIFEST.sha256）

组件声明清单（components.json）零改动 ⇒ ComponentRegistry::registry_digest
（= 全量组件声明规范 JSON 数组的 sha256，与 prompts 无关）在覆盖层与生产
注册表逐位一致——编译期 digest 与执行期核对天然同源。
"""
import hashlib
import json
import shutil
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SRC = REPO / "af" / "packages"
OVERLAY = REPO / "poc" / "poc-p3-a1-strategy-dsl" / "run" / "registry"

EXPLORER_SYSTEM = """# explorer system（P3-A1 覆盖层模板——st-b-C037 explorer 节点）

You are the repository-exploration agent (low-cost tier) of the
explore_then_fix pipeline. Your only job: locate where the issue lives.

- Use the read-only tools (locate/view) to find candidate files.
- Report hits as file paths, plus a confidence_milli (0-1000) for the set.
- Do NOT edit anything. Do NOT attempt fixes. The fixer node owns edits.
- Deliver your report as the final envelope: {"report": {"hits": [...],
  "confidence_milli": <int>}}.
"""

CODER_SYSTEM = """# coder system（P3-A1 覆盖层模板——st-b-C037 fixer 节点）

You are the fixer agent (main tier) of the explore_then_fix pipeline.
The explorer has already located candidate files; you only apply the change.

- Use the edit tool on the located files. Keep the diff minimal.
- After editing, deliver the patch envelope: {"patch": {"ref": <string>,
  "diff_ref": <string>}}.
- Do not re-explore the repository; trust the located hits handed to you.
"""

# st-b-C102（coder_role_task_graph 五角色图）引用的 4 份 pkg.coder.* 模板。
CODER_ANALYZE_SYSTEM = """# analyze system（P3-A1 覆盖层模板——st-b-C102 analyze 节点，route.strong）

You are the analysis role of the coder role task graph. Read the issue and
produce the root cause and suspect files.

- Deliver the envelope: {"analysis": {"root_cause": <string>,
  "suspect_files": [<string>, ...]}}.
"""

CODER_LOCATE_SYSTEM = """# locate system（P3-A1 覆盖层模板——st-b-C102 locate 节点，route.cheap）

You are the locate role. Use the locate tool to confirm which suspect files
actually contain the root cause.

- Deliver the envelope: {"report": {"hits": [<string>, ...]}}.
"""

CODER_PATCH_SYSTEM = """# patch system（P3-A1 覆盖层模板——st-b-C102 patch_role 节点，route.strong）

You are the patch role. Apply the minimal edit to the located hits.

- Deliver the envelope: {"patch": {"ref": <string>, "diff_ref": <string>}}.
"""

CODER_REVIEW_SYSTEM = """# review system（P3-A1 覆盖层模板——st-b-C102 review 节点，route.mid）

You are the review role. Inspect the patch; if acceptable, hand it through
unchanged for verification.

- Deliver the envelope: {"reviewed": {"ref": <string>, "diff_ref": <string>}}.
"""

# st-b-C102 引用的两档模型路由（route.strong / route.mid）——镜像根级
# factory-routes 端点声明（GLM-5.3 / GLM-5.3-Flash 同端点面），声明进覆盖层
# 根 components.json。注入 runner 零模型消耗，端点仅作档位元数据被读。
ROUTE_STRONG = {
    "class": "model_route",
    "name": "route.strong",
    "version": "1",
    "impl_ref": "packages/factory-routes@1/model_route/strong.py",
    "endpoint": {
        "protocol": "anthropic-messages",
        "base_url": "https://open.bigmodel.cn/api/anthropic",
        "model": "GLM-5.3",
        "key_ref": "zhipu-coding-plan",
        "reasoning_tier": "off",
        "stream": False,
        "auth": "bearer",
    },
}
ROUTE_MID = {
    "class": "model_route",
    "name": "route.mid",
    "version": "1",
    "impl_ref": "packages/factory-routes@1/model_route/mid.py",
    "endpoint": {
        "protocol": "anthropic-messages",
        "base_url": "https://open.bigmodel.cn/api/anthropic",
        "model": "GLM-5.3-Flash",
        "key_ref": "zhipu-coding-plan",
        "reasoning_tier": "off",
        "stream": False,
        "auth": "bearer",
    },
}


def sha256_file(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def write_with_manifest(dir_path: Path, name: str, body: str) -> None:
    prompts = dir_path / "prompts"
    prompts.mkdir(parents=True, exist_ok=True)
    target = prompts / f"{name}.md"
    target.write_text(body, encoding="utf-8")
    manifest = prompts / "MANIFEST.sha256"
    lines = []
    if manifest.is_file():
        lines = [l for l in manifest.read_text(encoding="utf-8").splitlines()
                 if l and not l.startswith("#")]
    lines.append(f"{sha256_file(target)}  prompts/{name}.md")
    lines.sort(key=lambda l: l.split("  ")[-1])
    manifest.write_text(
        "# manifest_version: 1\n" + "\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    if OVERLAY.exists():
        shutil.rmtree(OVERLAY)
    shutil.copytree(SRC, OVERLAY)
    # explore 包：新包目录（无 components.json——装载器按规则跳过，不进 digest）。
    write_with_manifest(OVERLAY / "explore", "explorer_system", EXPLORER_SYSTEM)
    # sweng 包：既有拷贝上追加 coder_system。
    write_with_manifest(OVERLAY / "sweng", "coder_system", CODER_SYSTEM)
    # coder 包（st-b-C102 五角色图的 4 份模板）。
    for name, body in (
        ("analyze_system", CODER_ANALYZE_SYSTEM),
        ("locate_system", CODER_LOCATE_SYSTEM),
        ("patch_system", CODER_PATCH_SYSTEM),
        ("review_system", CODER_REVIEW_SYSTEM),
    ):
        write_with_manifest(OVERLAY / "coder", name, body)
    # 根 components.json：追加 route.strong / route.mid（st-b-C102 引用）。
    root_decl = OVERLAY / "components.json"
    data = json.loads(root_decl.read_text(encoding="utf-8"))
    have = {c["name"] for c in data["components"]}
    for route in (ROUTE_STRONG, ROUTE_MID):
        if route["name"] not in have:
            data["components"].append(route)
    data["components"].sort(key=lambda c: c["name"])
    root_decl.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n",
                         encoding="utf-8")
    n = sum(1 for _ in OVERLAY.rglob("*.md"))
    print(f"覆盖层注册表就绪：{OVERLAY}（{n} 份 .md 模板；+route.strong/route.mid；"
          f"生产正本 {SRC} 零改动）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
