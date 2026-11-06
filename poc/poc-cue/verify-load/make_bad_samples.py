#!/usr/bin/env python3
"""错误分类学坏样例生成：从 good-bundle.json 逐类变异，写 samples/bad-<code>-*.json。

每类一个坏样例，对应 loader/tests/taxonomy.rs 的断言表。
"""
import copy
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SAMPLES = HERE / "samples"


def rule(b, rid):
    return next(r for r in b["rules"] if r["id"] == rid)


MUTATIONS = [
    # (文件名, 变异函数, 预期错误码)
    ("bad-e0001-version-unsupported.json",
     lambda b: b.update(schemaVersion=2), "E0001"),
    ("bad-e0002-version-missing.json",
     lambda b: b.pop("schemaVersion"), "E0002"),
    ("bad-e0002-version-wrongtype.json",
     lambda b: b.update(schemaVersion="1"), "E0002"),
    ("bad-e0003-envelope-unknown-field.json",
     lambda b: b.update(owner="someone"), "E0003"),
    ("bad-e1001-unknown-field.json",
     lambda b: b["line"]["tools"]["plcGate"].update(timeoutSec=5), "E1001"),
    ("bad-e1002-type-mismatch.json",
     lambda b: b["line"]["evalFunctions"]["cycleTimeSla"].update(timeoutMs="2000"), "E1002"),
    ("bad-e1003-missing-field.json",
     lambda b: b["line"]["andonRules"][0].pop("notify"), "E1003"),
    ("bad-e1004-unknown-variant.json",
     lambda b: b["line"]["tools"]["plcGate"].update(driver="canbus"), "E1004"),
    ("bad-e1007-kind-params-mismatch.json",
     lambda b: rule(b, "rule-0").update(
         params={"numerator": "ng", "denominator": "total", "warn": 0.01, "critical": 0.02}), "E1007"),
    ("bad-e1008-unknown-kind.json",
     lambda b: rule(b, "rule-1").update(kind="quantum"), "E1008"),
    ("bad-e2001-dangling-combo.json",
     lambda b: rule(b, "rule-7")["params"].update(allOf=["rule-ghost"]), "E2001"),
    ("bad-e2001-dangling-trigger.json",
     lambda b: b["line"]["andonRules"][0].update(trigger="noSuchFn"), "E2001"),
    ("bad-e2001-dangling-required.json",
     lambda b: b["line"]["artifactSchema"]["required"].append("ghostField"), "E2001"),
    ("bad-e2002-critical-below-warn.json",
     lambda b: rule(b, "rule-1")["params"].update(critical=0.001), "E2002"),
    ("bad-e2002-escalation-chain-flat.json",
     lambda b: rule(b, "rule-6")["params"]["chain"][1].update(afterMs=300000), "E2002"),
    ("bad-e2003-duplicate-id.json",
     lambda b: rule(b, "rule-2").update(id="rule-0"), "E2003"),
    ("bad-e2004-empty-notify.json",
     lambda b: b["line"]["andonRules"][0].update(notify=[]), "E2004"),
]


def main() -> None:
    good = json.loads((SAMPLES / "good-bundle.json").read_text())
    for name, mutate, code in MUTATIONS:
        b = copy.deepcopy(good)
        mutate(b)
        p = SAMPLES / name
        p.write_text(json.dumps(b, ensure_ascii=False, indent=1) + "\n")
        print(f"wrote {p.name:45s} expect {code}")
    assert all(m[2].startswith("E") for m in MUTATIONS)


if __name__ == "__main__":
    sys.exit(main())
