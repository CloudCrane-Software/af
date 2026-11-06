#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""template-supply-gate.py — 模板供给对账门（wave-03 D-61 新门；裁决10 装载机制的守卫面）

语义（fail-closed，对应 af-execline `pkg::prompts::resolve_template` 装载期门）：
  对 strategies/ 全库每个 distinct template_ref（`pkg.<包>.<名>[@<版本>]`）：
    G1 引用必有盘文件：af/packages/<包>/prompts/<名>.md 必须在盘；
    G2 manifest 校验通过：该包 prompts/MANIFEST.sha256 须在册、含 `# manifest_version:`
       头、且登记 prompts/<名>.md 的 sha256 与文件实测逐位一致（版本段给出时与清单版本核对）；
    G3 清单自洽：每个在册条目的文件都在盘且 sha256 相符（无孤儿/无篡改）。
  任一违例 → rc=1 点名（引用/包/缺件或漂移）；全过 → rc=0。

背景：template_ref 不经 af-dsl/af-compiler 绑定层核对（写错名 L1-L4 全放行），
  本门是执行面启动期 EX4001-4003 拒绝纪律的**构建期对账补偿面**——编译门绿 ≠ 跑得起。
路径纪律（同 ci/strategies_check.py）：仓内侧默认值由 __file__ 推导仓库根，不对局部副本跑。
pyc 纪律：只 import 标准库；由 `python3 -B` 调用，不产 __pycache__。
"""

import argparse
import glob
import hashlib
import os
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
PKG_ROOT = REPO_ROOT / "af" / "packages"
STRATEGIES_GLOB = str(REPO_ROOT / "strategies" / "st-*.yaml")
REF_RE = re.compile(r"template_ref:\s*(\S+)")
MANIFEST_VERSION_RE = re.compile(r"^#\s*manifest_version:\s*(\S+)\s*$")


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def parse_template_ref(ref):
    """镜像 af-execline prompts::parse_template_ref（纯函数；路径注入防护）。"""
    if not ref.startswith("pkg."):
        return None
    rest = ref[len("pkg."):]
    if "@" in rest:
        name_part, version = rest.split("@", 1)
        if version == "" or "@" in version:
            return None
    else:
        name_part, version = rest, None
    if "." not in name_part:
        return None
    package, name = name_part.split(".", 1)
    if not package or not name:
        return None
    for seg in (package, name):
        if "/" in seg or "\\" in seg or ".." in seg:
            return None
    return package, name, version


def load_manifest(prompts_dir):
    """返回 (manifest_version, {rel_path: sha})；非法返回 (None, None)。"""
    man = prompts_dir / "MANIFEST.sha256"
    if not man.is_file():
        return None, None
    version = None
    entries = {}
    for raw in man.read_text(encoding="utf-8").splitlines():
        line = raw.rstrip()
        if line.strip() == "":
            continue
        m = MANIFEST_VERSION_RE.match(line)
        if m:
            version = m.group(1)
            continue
        if line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) != 2 or len(parts[0]) != 64 or not parts[1]:
            return None, None  # 清单行不合法 → fail-closed
        entries[parts[1]] = parts[0]
    if not version:
        return None, None  # 缺 manifest_version 头 → fail-closed
    return version, entries


def collect_refs():
    refs = {}
    for f in sorted(glob.glob(STRATEGIES_GLOB)):
        for line in open(f, encoding="utf-8"):
            m = REF_RE.search(line)
            if m:
                ref = m.group(1).rstrip(",")
                if ref:
                    refs.setdefault(ref, 0)
                    refs[ref] += 1
    return refs


def main(argv=None):
    ap = argparse.ArgumentParser(description="模板供给对账门（fail-closed）")
    ap.add_argument("--json", action="store_true", help="输出结构化 JSON")
    args = ap.parse_args(argv)

    if not PKG_ROOT.is_dir():
        msg = "af/packages 根缺失：%s" % PKG_ROOT
        print("template-supply-gate: FAIL — %s" % msg)
        return 2

    refs = collect_refs()
    violations = []          # (kind, ref, detail)
    manifest_cache = {}      # pkg -> (version, entries) 或 (None, None)
    manifest_files_checked = set()

    def get_manifest(pkg):
        if pkg not in manifest_cache:
            manifest_cache[pkg] = load_manifest(PKG_ROOT / pkg / "prompts")
        return manifest_cache[pkg]

    # G1 + G2：每个引用必有盘文件 + manifest 登记 sha 相符
    for ref in sorted(refs):
        parsed = parse_template_ref(ref)
        if parsed is None:
            violations.append(("ref-invalid", ref, "template_ref 语法非法"))
            continue
        pkg, name, _ref_version = parsed
        man_version, entries = get_manifest(pkg)
        if man_version is None:
            violations.append(("manifest-missing", ref, "包 %s 缺/非法 MANIFEST.sha256" % pkg))
            continue
        rel = "prompts/%s.md" % name
        fpath = PKG_ROOT / pkg / "prompts" / (name + ".md")
        if not fpath.is_file():
            violations.append(("file-missing", ref, "盘文件缺失：%s" % fpath))
            continue
        if rel not in entries:
            violations.append(("unregistered", ref, "%s 未登记进 %s 的 MANIFEST.sha256" % (rel, pkg)))
            continue
        actual = sha256_file(fpath)
        if actual != entries[rel]:
            violations.append(("sha-drift", ref, "文件 sha256 %s ≠ 清单登记 %s（%s）" % (actual, entries[rel], rel)))
            continue

    # 版本段核对（若引用带 @版本，须等于该包 manifest_version）
    for ref in sorted(refs):
        if "@" not in ref:
            continue
        pkg = ref[len("pkg."):].split("@", 1)[0].split(".", 1)[0]
        ver = ref.split("@", 1)[1]
        man_version, _ = get_manifest(pkg)
        if man_version is not None and ver != man_version:
            violations.append(("version-drift", ref, "引用版本段 %s ≠ 清单版本 %s" % (ver, man_version)))

    # G3：清单自洽——每个在册条目文件在盘且 sha 相符（无孤儿/无篡改）
    for man in sorted(glob.glob(str(PKG_ROOT / "*" / "prompts" / "MANIFEST.sha256"))):
        pkg = Path(man).parent.parent.name
        manifest_files_checked.add(pkg)
        version, entries = load_manifest(Path(man).parent)
        if version is None:
            violations.append(("manifest-missing", "pkg.%s.*" % pkg, "包 %s 的 MANIFEST.sha256 非法" % pkg))
            continue
        for rel, sha in entries.items():
            # rel 形如 prompts/<名>.md，相对**包目录**（同 af-execline 解析约定）；
            # man 位于 <包>/prompts/MANIFEST.sha256 → 包目录 = man.parent.parent
            fpath = Path(man).parent.parent / rel
            if not fpath.is_file():
                violations.append(("orphan-entry", rel, "清单登记但盘文件缺失：%s" % fpath))
                continue
            actual = sha256_file(fpath)
            if actual != sha:
                violations.append(("sha-drift", rel, "清单条目 sha 与文件不符：%s" % fpath))

    ok = not violations
    result = {
        "gate": "template-supply-gate",
        "ok": ok,
        "distinct_template_refs": len(refs),
        "packages_with_manifest": len(manifest_files_checked),
        "violations": [
            {"kind": k, "ref": r, "detail": d} for (k, r, d) in violations
        ],
    }

    if args.json:
        import json
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        if ok:
            print("template-supply-gate: PASS — %d 个 distinct template_ref 全部有盘文件且 manifest 校验通过"
                  % len(refs))
            print("  （覆盖 %d 个包的 prompts/MANIFEST.sha256，逐条目 sha256 相符）" % len(manifest_files_checked))
        else:
            print("template-supply-gate: FAIL — %d 处违例：" % len(violations))
            for k, r, d in violations:
                print("  [%s] %s — %s" % (k, r, d))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
