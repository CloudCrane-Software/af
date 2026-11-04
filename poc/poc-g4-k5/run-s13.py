#!/usr/bin/env python3
# =============================================================================
# G4 返修批 S13 补证跑批器（复核 R5/R5b：部分臂协同收缩，REVIEW.md P1）。
#
# 本跑批器为返修批**新造夹具**（未复制施工方 S0-S12 沙箱，亦未复制复核方
# 沙箱；沙箱在 evidence/sandboxes/ 下就地构造，真树零改动）：
#   S13-R5    report 臂协同收缩：删 citation_unit.py 成员文件 + 同批删其登记行
#   S13-R5b   swev  臂协同收缩：删 judge.rs 成员文件 + 同批删其登记行
#   S13-R5bak 复核 R5b 原形态：citation_unit.py 改名 .bak + 同批删其登记行
# 每场景在同一沙箱根内四读（前两读单门、后两读管道级）：
#   ① 管道级对照（未攻击）：真 ci/gate_inputs_check.sh check --root 沙箱
#      ⇒ 须绿「12 件」（证沙箱表钉面忠实：后续红必来自攻击本身）
#   ② k5 单门改前 = before/k5-path-lint.orig.py（手工清单版）
#   ③ k5 单门改后 = 仓内 ci/k5-path-lint.py（派生版）
#   ④ 管道级攻击后：同批表改动过 gate_inputs_check 表钉面 ⇒ 须红 exit=1
# 沙箱 = 既有 S0-S12 同款最小 k5 面（manifest + judge.rs + reportkit 包）
#      + step0 表钉面（GATE_INPUTS 其余 11 件正本 + gate-inputs.sha256 钉）。
# =============================================================================
import pathlib
import shutil
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
BEFORE = HERE / "before" / "k5-path-lint.orig.py"
AFTER = REPO / "ci" / "k5-path-lint.py"
GATE_CHECK = REPO / "ci" / "gate_inputs_check.sh"
SANDBOX_BASE = HERE / "evidence" / "sandboxes"
LOGS = HERE / "evidence" / "logs"

# k5 单门最小面（与既有 run-scenarios.py 的 COPY_UNITS 同款）
K5_UNITS = [
    "ci/scorechain-manifest.txt",
    "crates/af-execline/src/judge.rs",
    "af/packages/reportkit",  # 整包（判分臂载体）
]
# step0 表钉面（GATE_INPUTS 12 件中 k5 面之外的 11 件正本 + 钉文件）
PIN_UNITS = [
    "ci/neutrality-expected-fail.txt",
    "ci/neutrality-wordlist.txt",
    "ci/neutrality-golden-registry.tsv",
    "ci/clippy-baseline.txt",
    "ci/arm-wiring.yaml",
    "ci/branches.yaml",
    "ci/criteria.yaml",
    "ci/fix-scan.yaml",
    "ci/budget-cast-whitelist.json",
    "ci/debt-register.md",
    "ci/subgates.list",
    "ci/baselines/gate-inputs.sha256",
]


def build_sandbox(name: str) -> pathlib.Path:
    sb = SANDBOX_BASE / name
    if sb.exists():
        shutil.rmtree(sb)
    sb.mkdir(parents=True)
    for rel in K5_UNITS + PIN_UNITS:
        src = REPO / rel
        dst = sb / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        if src.is_dir():
            shutil.copytree(src, dst)
        else:
            shutil.copy2(src, dst)
    # 两版 lint 都放到沙箱 ci/ 下（REPO_ROOT 按 __file__ 派生）
    shutil.copy2(BEFORE, sb / "ci" / "k5-path-lint.py")
    return sb


def run_cmd(cmd: list, sb: pathlib.Path, log_name: str,
            banner: str) -> tuple:
    proc = subprocess.run(cmd, cwd=sb, capture_output=True, text=True)
    LOGS.mkdir(parents=True, exist_ok=True)
    (LOGS / log_name).write_text(
        f"$ {banner}  （cwd={sb.name}）\n"
        f"exit={proc.returncode}\n--- stdout ---\n{proc.stdout}"
        f"--- stderr ---\n{proc.stderr}",
        encoding="utf-8")
    out = (proc.stdout + proc.stderr).strip().splitlines()
    conclusion = out[0] if out else "(无输出)"
    return proc.returncode, conclusion


def run_lint(sb: pathlib.Path, log_name: str) -> tuple:
    return run_cmd(
        [sys.executable, "-I", str(sb / "ci" / "k5-path-lint.py")],
        sb, log_name, "python3 -I ci/k5-path-lint.py")


def run_gate_check(sb: pathlib.Path, log_name: str) -> tuple:
    return run_cmd(
        ["bash", str(GATE_CHECK), "check", "--root", str(sb)],
        sb, log_name,
        f"bash ci/gate_inputs_check.sh check --root {sb.name}（真脚本+沙箱根）")


def manifest_drop_row(sb: pathlib.Path, relpath: str) -> None:
    mp = sb / "ci" / "scorechain-manifest.txt"
    lines = [
        ln for ln in mp.read_text(encoding="utf-8").splitlines()
        if not ln.startswith(relpath)
    ]
    mp.write_text("\n".join(lines) + "\n", encoding="utf-8")


CITATION = "af/packages/reportkit/units/citation_unit.py"
JUDGE = "crates/af-execline/src/judge.rs"


def atk_r5(sb):
    """复核 R5：删 report 臂成员文件 + 同批删其登记行（部分臂协同收缩）。"""
    (sb / CITATION).unlink()
    manifest_drop_row(sb, CITATION)


def atk_r5b(sb):
    """任务书 R5b：swev 臂协同收缩变体——删 judge.rs 门本体 + 同批删其登记行。"""
    (sb / JUDGE).unlink()
    manifest_drop_row(sb, JUDGE)


def atk_r5bak(sb):
    """复核 R5b 原形态：成员文件改名 .bak（脱离派生面）+ 同批删其登记行。"""
    (sb / CITATION).rename(sb / (CITATION + ".bak"))
    manifest_drop_row(sb, CITATION)


SCENARIOS = [
    ("S13-R5-report-co-shrink", atk_r5,
     "R5：删 citation_unit.py 文件+删登记行同批（单门改后不红、防线 2→1；管道级表钉红）"),
    ("S13-R5b-swev-co-shrink", atk_r5b,
     "R5b：删 judge.rs 文件+删登记行同批（单成员臂删空 ⇒ fail-closed 仍红；管道级表钉红）"),
    ("S13-R5bak-report-bak-coshink", atk_r5bak,
     "复核 R5b 原形态：citation_unit.py 改名 .bak+删登记行同批（同 R5 结论）"),
]


def main() -> int:
    rows = []
    for name, attack, note in SCENARIOS:
        sb = build_sandbox(name)
        # ① 管道级对照（未攻击）：沙箱表钉面须先绿，红才可归因于攻击
        ctl_rc, ctl_msg = run_gate_check(
            sb, f"{name}.pipeline-control.log")
        # ②/③ 攻击后同沙箱双跑 k5 单门（改前 → 改后，同槽位保证 REPO_ROOT 一致）
        attack(sb)
        b_rc, b_msg = run_lint(sb, f"{name}.before.log")
        shutil.copy2(AFTER, sb / "ci" / "k5-path-lint.py")
        a_rc, a_msg = run_lint(sb, f"{name}.after.log")
        # ④ 管道级攻击后：同批表改动过 step0 表钉面 ⇒ 须红
        p_rc, p_msg = run_gate_check(sb, f"{name}.pipeline.log")
        rows.append((name, b_rc, b_msg, a_rc, a_msg, ctl_rc, ctl_msg,
                     p_rc, p_msg, note))
        print(f"[{name}]")
        print(f"  管道对照({ctl_rc}): {ctl_msg}")
        print(f"  单门改前({b_rc}): {b_msg}")
        print(f"  单门改后({a_rc}): {a_msg}")
        print(f"  管道攻击({p_rc}): {p_msg}")
        print()
    out = HERE / "evidence" / "s13-scenarios.tsv"
    with out.open("w", encoding="utf-8") as f:
        f.write("场景\t改前exit\t改前结论\t改后exit\t改后结论\t"
                "管道对照exit\t管道对照结论\t管道攻击exit\t管道攻击结论\t说明\n")
        for r in rows:
            f.write("\t".join(str(c) for c in r) + "\n")
    print(f"对照表落 {out.relative_to(REPO)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
