#!/usr/bin/env python3
# =============================================================================
# G4（K5 判分链下界门自派生）改前/改后对照跑批器。
#
# 每个场景在独立沙箱（证据目录内）构造同一份仓面样本，分别用
#   before = poc/poc-g4-k5/before/k5-path-lint.orig.py（基线 origin/product/main 版）
#   after  = 仓内 ci/k5-path-lint.py（G4 派生版）
# 以 `python3 -I` 跑同一沙箱（lint 只读沙箱，双跑互不污染），逐场景记
# exit + 首行结论，汇成对照表（TSV）。
# 预期：既有保护面逐场景结论一致；仅「新成员自动入面」类场景按设计分叉
# （改后更严——见 README §3）。
# =============================================================================
import hashlib
import pathlib
import shutil
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
BEFORE = HERE / "before" / "k5-path-lint.orig.py"
AFTER = REPO / "ci" / "k5-path-lint.py"
SANDBOX_BASE = HERE / "evidence" / "sandboxes"

# 沙箱所需的最小仓面（lint 只消费这些路径）
COPY_UNITS = [
    "ci/scorechain-manifest.txt",
    "crates/af-execline/src/judge.rs",
    "af/packages/reportkit",  # 整包（判分臂载体）
]


def build_sandbox(name: str) -> pathlib.Path:
    sb = SANDBOX_BASE / name
    if sb.exists():
        shutil.rmtree(sb)
    sb.mkdir(parents=True)
    for rel in COPY_UNITS:
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


def run_lint(sb: pathlib.Path, script: pathlib.Path,
             log_name: str) -> tuple:
    proc = subprocess.run(
        [sys.executable, "-I", str(script)],
        cwd=sb, capture_output=True, text=True,
    )
    logs = HERE / "evidence" / "logs"
    logs.mkdir(parents=True, exist_ok=True)
    (logs / log_name).write_text(
        f"$ python3 -I ci/k5-path-lint.py  （cwd={sb.name}）\n"
        f"exit={proc.returncode}\n--- stdout ---\n{proc.stdout}"
        f"--- stderr ---\n{proc.stderr}",
        encoding="utf-8")
    out = (proc.stdout + proc.stderr).strip().splitlines()
    conclusion = out[0] if out else "(无输出)"
    return proc.returncode, conclusion


def manifest_drop_row(sb: pathlib.Path, prefix: str) -> None:
    mp = sb / "ci" / "scorechain-manifest.txt"
    lines = [
        ln for ln in mp.read_text(encoding="utf-8").splitlines()
        if not ln.startswith(prefix)
    ]
    mp.write_text("\n".join(lines) + "\n", encoding="utf-8")


def manifest_set_row(sb: pathlib.Path, relpath: str, sha: str) -> None:
    """按路径前缀替换/追加登记行（路径 <TAB> sha <TAB> 用途）。"""
    mp = sb / "ci" / "scorechain-manifest.txt"
    lines = mp.read_text(encoding="utf-8").splitlines()
    row = f"{relpath}\t{sha}\tG4 沙箱场景行"
    done = False
    for i, ln in enumerate(lines):
        if ln.startswith(relpath + "\t"):
            lines[i] = row
            done = True
            break
    if not done:
        lines.append(row)
    mp.write_text("\n".join(lines) + "\n", encoding="utf-8")


def sha256_file(p: pathlib.Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


# 场景 =（名，构造器）。构造器在沙箱内做攻击/演化动作。
def sc_pristine(sb):
    pass


def sc_mut007_replay(sb):
    """MUT-007 原样重放：删 judge.rs 登记行（登记表 4→3 收缩）。"""
    manifest_drop_row(sb, "crates/af-execline/src/judge.rs")


def sc_shrink_report_row(sb):
    manifest_drop_row(sb, "af/packages/reportkit/units/citation_unit.py")


def sc_drift_member_bytes(sb):
    p = sb / "af/packages/reportkit/units/rubric_unit.py"
    p.write_text(p.read_text(encoding="utf-8") + "# 红队漂移注入\n",
                 encoding="utf-8")


def sc_delete_member_file(sb):
    (sb / "af/packages/reportkit/tools/render.py").unlink()


def sc_empty_manifest(sb):
    mp = sb / "ci" / "scorechain-manifest.txt"
    lines = [
        ln for ln in mp.read_text(encoding="utf-8").splitlines()
        if ln.lstrip().startswith("#") or not ln.strip()
    ]
    mp.write_text("\n".join(lines) + "\n", encoding="utf-8")


def sc_new_unregistered_member(sb):
    new = sb / "af/packages/reportkit/units/new_unit.py"
    new.write_text("# 新判分单元（未登记）\n", encoding="utf-8")


def sc_new_member_registered(sb):
    new = sb / "af/packages/reportkit/units/new_unit.py"
    new.write_text("# 新判分单元（已登记新锚）\n", encoding="utf-8")
    manifest_set_row(sb, "af/packages/reportkit/units/new_unit.py",
                     sha256_file(new))


def sc_excluded_artifacts(sb):
    pych = sb / "af/packages/reportkit/units/__pycache__"
    pych.mkdir()
    (pych / "rubric_unit.cpython-311.pyc").write_bytes(b"\x00pyc")
    (sb / "af/packages/reportkit/units/rubric_unit.py.bak").write_text(
        "旧快照\n", encoding="utf-8")


def sc_gate_body_marker_stripped(sb):
    """协同收缩攻击：judge.rs 改名 script_sha256 字段（锚声明被摘）+ 登记行
    同步换新锚（腿1 全绿）——手写清单版照绿（清单仍点名 judge.rs 且在册），
    派生版 swev 臂算不出门本体即红。"""
    p = sb / "crates/af-execline/src/judge.rs"
    p.write_text(
        p.read_text(encoding="utf-8").replace(
            "script_sha256", "script_digest"),
        encoding="utf-8")
    manifest_set_row(sb, "crates/af-execline/src/judge.rs", sha256_file(p))


def sc_reportkit_pkg_absent(sb):
    """协同灭件攻击：判分包整目录删除 + 三行登记行同批摘除。"""
    shutil.rmtree(sb / "af/packages/reportkit")
    for rel in ("af/packages/reportkit/units/citation_unit.py",
                "af/packages/reportkit/units/rubric_unit.py",
                "af/packages/reportkit/tools/render.py"):
        manifest_drop_row(sb, rel)


def sc_name_collision_nonmember(sb):
    """撞名陷阱：af-credential/src/judge.rs（权限交集判定器，无判分锚标记）
    进沙箱——内容型派生不得误伤。"""
    src = REPO / "crates/af-credential/src/judge.rs"
    dst = sb / "crates/af-credential/src/judge.rs"
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dst)


def sc_marker_in_tests_ignored(sb):
    """tests/ 面出现 script_sha256（测试构造 locator 非门本体）——不在
    生产源扫描面，不得入下界。"""
    dst = sb / "crates/af-execline/tests/p3_probe.rs"
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_text(
        "let locator = JudgeLocator { script_sha256: \"0\".repeat(64), .. };\n",
        encoding="utf-8")


SCENARIOS = [
    ("S0-pristine", sc_pristine,
     "原样基线（改前改后必须同绿同文）"),
    ("S1-mut007-replay", sc_mut007_replay,
     "MUT-007 原样重放：删 judge.rs 登记行（必须双红「被收缩」）"),
    ("S2-shrink-report-row", sc_shrink_report_row,
     "删 citation_unit.py 登记行（必须双红「被收缩」）"),
    ("S3-drift-member-bytes", sc_drift_member_bytes,
     "在册成员字节漂移（必须双红「sha256 漂移」）"),
    ("S4-delete-member-file", sc_delete_member_file,
     "删在册成员文件（必须双红「文件缺席」）"),
    ("S5-empty-manifest", sc_empty_manifest,
     "登记表只剩注释（必须双红「登记表为空」）"),
    ("S6-new-unregistered-member", sc_new_unregistered_member,
     "判分包内新增 .py 未登记（改前漏钉照绿 → 改后红：G4 修复点）"),
    ("S7-new-member-registered", sc_new_member_registered,
     "新增成员走正道登记新锚（必须双绿 5 件）"),
    ("S8-excluded-artifacts", sc_excluded_artifacts,
     "__pycache__/*.pyc 与 *.bak 快照（显式排除项，不扩面：双绿 4 件）"),
    ("S9-gate-body-marker-stripped", sc_gate_body_marker_stripped,
     "协同收缩：judge.rs 摘锚声明字段+登记行换新锚（改前照绿 → 改后红：fail-closed）"),
    ("S10-reportkit-pkg-absent", sc_reportkit_pkg_absent,
     "判分包整目录删除+行同批摘（双红：手写版「被收缩」/派生版「载体包缺席」）"),
    ("S11-name-collision-nonmember", sc_name_collision_nonmember,
     "af-credential/judge.rs 撞名非成员（内容型判定不误伤：双绿 4 件）"),
    ("S12-marker-in-tests-ignored", sc_marker_in_tests_ignored,
     "tests/ 面出现锚字段（非生产源不扫：双绿 4 件）"),
]


def main() -> int:
    rows = []
    for name, setup, note in SCENARIOS:
        sb = build_sandbox(name)
        setup(sb)
        # 同一沙箱双跑（lint 只读；before 版放沙箱 ci/ 下保证 REPO_ROOT 一致）
        before_rc, before_msg = run_lint(
            sb, sb / "ci" / "k5-path-lint.py", f"{name}.before.log")
        # 换 after 版（同槽位，保证 __file__ 派生的 REPO_ROOT 逐字一致）
        shutil.copy2(AFTER, sb / "ci" / "k5-path-lint.py")
        after_rc, after_msg = run_lint(
            sb, sb / "ci" / "k5-path-lint.py", f"{name}.after.log")
        verdict = "一致" if (before_rc == after_rc) else "分叉(改后更严)"
        rows.append((name, before_rc, before_msg, after_rc, after_msg,
                     verdict, note))
        print(f"[{name}] before({before_rc}): {before_msg}")
        print(f"[{' ' * len(name)}] after ({after_rc}): {after_msg}")
        print(f"[{' ' * len(name)}] => {verdict}")
        print()
    out = HERE / "evidence" / "scenarios.tsv"
    with out.open("w", encoding="utf-8") as f:
        f.write("场景\t改前exit\t改前结论\t改后exit\t改后结论\t判定\t说明\n")
        for r in rows:
            f.write("\t".join(str(c) for c in r) + "\n")
    print(f"对照表落 {out.relative_to(REPO)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
