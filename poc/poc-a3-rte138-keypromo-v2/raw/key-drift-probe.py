#!/usr/bin/env python3
"""两侧取键漂移探针（A3 v2 必修 2·选项 b）。

镜像：
  - 投影器算法：ledger.jsonl 首行 stable_id（缺席 → 空串）
    工厂 af-otel-projector.py:90 / 写入 af.work_order_id :408/:437
  - Rust / tracetree 算法：全部行现有 stable_id 的排序集合最小值
    crates/af-execline/src/tracetree.rs BTreeSet::iter().next()

两侧独立，容许不等。不等即 WARN（exit 0，计数落 stdout JSON）。
不改 P2 红绿；工厂 tools/otel/otlp-assert.py 本批零改（判据面零弱化）。

用法：
  python3 -I key-drift-probe.py <order_dir> [<order_dir> ...]
"""
from __future__ import annotations

import json
import sys
from pathlib import Path


def probe(order_dir: Path) -> dict:
    path = order_dir / "ledger.jsonl"
    projector_first = ""
    rust_ids: set[str] = set()
    seen_first = False
    if path.is_file():
        for raw in path.read_text(encoding="utf-8").splitlines():
            line = raw.strip()
            if not line:
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError:
                continue
            as_proj = row.get("stable_id", "") if isinstance(row, dict) else ""
            if as_proj is None:
                as_proj = ""
            as_proj = str(as_proj)
            if not seen_first:
                projector_first = as_proj
                seen_first = True
            if isinstance(row, dict) and "stable_id" in row:
                rust_ids.add(str(row.get("stable_id") or ""))
    rust_min = sorted(rust_ids)[0] if rust_ids else ""
    drifted = projector_first != rust_min
    return {
        "order_dir": str(order_dir),
        "projector_first_row": projector_first,
        "rust_set_min": rust_min,
        "drifted": drifted,
        "warn": 1 if drifted else 0,
    }


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print("用法：key-drift-probe.py <order_dir> [...]", file=sys.stderr)
        return 2
    results = [probe(Path(a)) for a in argv[1:]]
    warns = sum(r["warn"] for r in results)
    print(json.dumps({"warns": warns, "probes": results}, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
