#!/usr/bin/env python3
"""端到端拼包：CUE schema → cue export → JSON → 加 schemaVersion 信封 → bundle。

用法: python3 build_bundle.py N OUT.json
line 部分来自 line/line.cue + line/line-a.cue 的 cue export（取 lineA 键），
rules 部分来自 rules/rules.cue + perf/rules-N.cue 的 cue export（取 rules 键）。
"""
import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
POC = HERE.parent  # poc-cue/
CUE = POC.parent.parent / "bin" / "cue"


def export(cue_files: list[Path]) -> dict:
    r = subprocess.run([str(CUE), "export", *[str(f) for f in cue_files]],
                       capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"cue export 失败: {r.stderr}")
    return json.loads(r.stdout)


def main() -> None:
    n = int(sys.argv[1])
    out = Path(sys.argv[2])
    line_doc = export([POC / "line" / "line.cue", POC / "line" / "line-a.cue"])
    rules_doc = export([POC / "rules" / "rules.cue", HERE / "perf" / f"rules-{n}.cue"])
    bundle = {
        "schemaVersion": 1,
        "line": line_doc["lineA"],
        "rules": rules_doc["rules"],
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(bundle, ensure_ascii=False, indent=1) + "\n")
    print(f"wrote {out}: schemaVersion=1 rules={len(bundle['rules'])} "
          f"bytes={out.stat().st_size}")


if __name__ == "__main__":
    main()
