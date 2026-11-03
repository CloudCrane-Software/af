#!/usr/bin/env python3
"""pyc_header_check.py — pyc 影子执行面「法 B」结构化识别（G0-C 施工批落地件）
草案出处：计划/研究组/G0-CI机制批-实施规格-20261027/C/1-fix-scan/SPEC.md ④（双法交叉自验）
正本锚：红队复盘 C10-4②（:245「双方法交叉自验：find 枚举与 struct 解 pyc 头比对
        src_mtime/src_size 两法结论必须一致」）+ 5.1「双方法交叉自验」行（:323）；
        W2-C-50→W3-C-34 族史（有效 pyc 优先于被锚源文件）。

与「法 A」（fix_scan.py 内按文件名枚举 *.pyc / __pycache__）的分工：
  法 A 按**名字**找；本法按**内容**找——逐文件读头部 magic 三轨识别（当前解释器
  MAGIC_NUMBER + 经典区间 + 新格式区间护栏，见下），把 .pyc 改名成 .dat 也照样失口。
  对时间戳格式 pyc 进一步解 src_mtime/src_size 与同目录 .py 比对，给出
  stale（缓存旧于源）/orphan（源不在场）明细。
  两法路径集交给 fix_scan.py 交叉对账（both_zero_or_identical_sets）。

输出（--json）：{method, root, roots, pyc_by_content[], stale[], orphan[], scanned_files}
退出码：0=执行完成（有无命中由调用方判定）；2=参数/执行体错误。
只读；不跑 cargo、不改文件。
"""
from __future__ import annotations

import argparse
import json
import os
import struct
import sys
from pathlib import Path

# CPython pyc 头部 magic（4 字节小端）识别三轨（3.14.4 亲测 importlib.util.MAGIC_NUMBER
# =168627755=0x0A0D0E2B——**不在** 3.8-3.13 的经典顺序区间 3300-3700 内，单轨区间会
# 对现行解释器的 pyc 全盲；故按「当前解释器 magic + 经典区间 + 新格式区间护栏」三轨识别，
# 并要求 flags/时间戳结构合理——双法交叉的意义正在于此：任一轨漏判即两法不一致=红）：
import importlib.util
CURRENT_MAGIC = struct.unpack("<I", importlib.util.MAGIC_NUMBER)[0]
CLASSIC_MIN, CLASSIC_MAX = 3300, 3700            # CPython 3.8-3.13 顺序 magic
NEWSCHEME_MIN, NEWSCHEME_MAX = 0x0A0D0000, 0x0A0DFFFF  # 3.14 新格式 magic 观测区间
VALID_FLAGS = {0, 1, 3}                          # PEP 552：0=时间戳 / 1=哈希 / 3=哈希+checked
EPOCH_MIN, EPOCH_MAX = 946684800, 4102444800     # 2000-01-01 ~ 2100-01-01
PRUNE_DIRS = {"target", ".git", "out", "node_modules"}


def looks_like_pyc(head: bytes) -> tuple[bool, int, int]:
    """(是否 pyc, magic, flags)。magic 三轨命中 + flags 合法 +（时间戳格式下）
    src_mtime/src_size 落合理域——四者全中才判 pyc，防普通二进制误触（误触=双法
    不一致红，虽 fail-loud 方向正确但会成噪音）。"""
    if len(head) < 16:
        return False, 0, 0
    magic = struct.unpack("<I", head[0:4])[0]
    flags = struct.unpack("<I", head[4:8])[0]
    if flags not in VALID_FLAGS:
        return False, 0, 0
    if flags == 0:
        mtime, size = struct.unpack("<II", head[8:16])
        if not (EPOCH_MIN <= mtime <= EPOCH_MAX and 0 < size < (1 << 31)):
            return False, 0, 0
    if (magic == CURRENT_MAGIC or CLASSIC_MIN <= magic <= CLASSIC_MAX
            or NEWSCHEME_MIN <= magic <= NEWSCHEME_MAX):
        return True, magic, flags
    return False, 0, 0


def is_excluded(rel: str, excludes: list[str]) -> bool:
    import fnmatch
    for g in excludes:
        if fnmatch.fnmatch(rel, g):
            return True
        base = g.rstrip("*").rstrip("/")
        if base and (rel == base or rel.startswith(base + "/")):
            return True
    return False


def iter_files(scan_root: Path, root: Path, excludes: list[str]):
    """独立于 fix_scan.py 的遍历实现（双法独立性的另一半：遍历代码路径也分开）。"""
    for dirpath, dirnames, filenames in os.walk(scan_root):
        rel_dir = os.path.relpath(dirpath, root).replace(os.sep, "/")
        kept = []
        for d in sorted(dirnames):
            child = d if rel_dir == "." else f"{rel_dir}/{d}"
            if d in PRUNE_DIRS or is_excluded(child, excludes):
                continue
            kept.append(d)
        dirnames[:] = kept
        for f in sorted(filenames):
            yield f"{rel_dir}/{f}" if rel_dir != "." else f


def source_of(pyc_rel: str) -> str | None:
    """__pycache__/<name>.cpython-3XX.pyc → 同目录 <name>.py；非法 pyc 命名返回 None。"""
    parts = pyc_rel.split("/")
    if len(parts) < 2 or parts[-2] != "__pycache__":
        return None
    fname = parts[-1]
    stem = fname.split(".")[0]
    if not stem:
        return None
    return "/".join(parts[:-2] + [stem + ".py"])


def main() -> int:
    ap = argparse.ArgumentParser(description="pyc 内容 magic 识别（fix-scan 法 B）")
    ap.add_argument("--root", default=".", help="仓根（输出 relpath 的基准 + excludes 基准）")
    ap.add_argument("--roots", nargs="*", default=["."], help="扫描根（相对 --root 或绝对路径）")
    ap.add_argument("--excludes", nargs="*",
                    default=["target/**", "ci/out/**", ".git/**"])
    ap.add_argument("--json", action="store_true", help="JSON 输出（默认人读）")
    args = ap.parse_args()

    root = Path(args.root).resolve()
    if not root.is_dir():
        print(f"[pyc-check] FATAL: --root {root} 不在盘", file=sys.stderr)
        return 2
    pyc_by_content: list[str] = []
    stale: list[dict] = []
    orphan: list[dict] = []
    scanned = 0
    for r in args.roots:
        scan_root = Path(r)
        if not scan_root.is_absolute():
            scan_root = root / scan_root
        if not scan_root.is_dir():
            continue
        for rel in iter_files(scan_root, root, args.excludes):
            f = root / rel
            try:
                with open(f, "rb") as fh:
                    head = fh.read(16)
            except OSError:
                continue
            scanned += 1
            is_pyc, magic, flags = looks_like_pyc(head)
            if not is_pyc:
                continue
            pyc_by_content.append(rel)
            src = source_of(rel)
            if flags & 1 or src is None:
                if src is None:
                    orphan.append({"pyc": rel, "magic": magic,
                                   "note": "非 __pycache__/<stem>.cpython-*.pyc 命名或源路径不可推定"})
                continue
            src_path = root / src
            if not src_path.is_file():
                orphan.append({"pyc": rel, "magic": magic, "source": src, "note": "源文件不在场"})
                continue
            src_mtime, src_size = struct.unpack("<II", head[8:16])
            st = src_path.stat()
            if int(st.st_mtime) & 0xFFFFFFFF != src_mtime or st.st_size != src_size:
                stale.append({"pyc": rel, "magic": magic, "source": src,
                              "pyc_src_mtime": src_mtime, "pyc_src_size": src_size,
                              "actual_mtime": int(st.st_mtime) & 0xFFFFFFFF, "actual_size": st.st_size})
    payload = {
        "method": "B-content-magic",
        "root": str(root), "roots": args.roots,
        "pyc_by_content": sorted(pyc_by_content),
        "stale": stale, "orphan": orphan, "scanned_files": scanned,
    }
    if args.json:
        print(json.dumps(payload, ensure_ascii=False))
    else:
        print(f"法 B（内容 magic 三轨：当前 {CURRENT_MAGIC} / 经典 {CLASSIC_MIN}-{CLASSIC_MAX} / "
              f"新格式 0x{NEWSCHEME_MIN:08X}-0x{NEWSCHEME_MAX:08X}）：扫 {scanned} 文件，"
              f"pyc 内容命中 {len(pyc_by_content)}，stale {len(stale)}，orphan {len(orphan)}")
        for p in payload["pyc_by_content"]:
            print(f"  PYC {p}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
