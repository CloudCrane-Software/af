#!/usr/bin/env python3
# =============================================================================
# K5 判分链路径 lint（总计划 §2 裁决7：「K5 lint 并入编译阶段 1（黑名单=
# 判分链三件+obs-checker+obs.rs）」的 af 仓落地形态，红队 W1-C03 补齐）。
#
# 黑名单 = ci/scorechain-manifest.txt 登记的判分链路径：逐文件 sha256 对
# 表锚核对，任何漂移/缺席 = exit 1（fail-loud，不静默放行）。有意变更
# 判分链必须走裁决并更新登记表（同 neutrality-wordlist 冻结纪律）。
# RSI 演练原设计的 obs-checker.py / obs.rs 载体属 MES 5221 判分链（本仓
# 无对应文件），af 仓黑名单 = swev/report 两判分臂的判分核 + 判分门本体。
#
# 判分链成员下界对账（红队终局轮 RT-E6/E51 修复，2026-11-02）：
#   本 lint 只校验「在册行逐位对锚」——登记表被收缩（如删 judge.rs 登记行，
#   4 件→3 件）时在册行仍逐位对锚、lint 照绿，而被摘掉登记行的
#   crates/af-execline/src/judge.rs（判分门本体）即脱离哈希锚保护面、
#   任意改写而 step1 硬门不红（RT-E6/E51 复现：收缩后 lint PASS「3 件」、
#   judge.rs 追加一行注释后仍 PASS）。故此处加「必需成员下界」：下界清单
#   缺一即红（fail-closed）。有意移除判分链成员须走裁决并同批改本下界清单
#   （代码 diff 即评审面——与登记表同款冻结纪律，两处分离互为跨锚）。
#
# 下界成员来源（G4 门自派生，2026-11-04）：下界不再手工维护成员清单——
# 手写清单 ⇒ 仓面加新成员不会自动进清单、删成员清单也不会自动收敛，
# 正是 RT-E6/E51 记的「只钉 N 件」同型漏钉（同类教训：跨树收口门的门族
# 面已从手抄清单改为基线 ref 自派生，ci/fix_verify.py 机制③）。下界改为
# 从仓面按规则现算全集（可复算、可解释，规则如下）：
#   臂1 report 判分臂 = af/packages/reportkit/ 包内全部 *.py。判分包内的
#       python 源即 report 判分脚本面：ReportJudgeLocator 装载/锚定的
#       citation/rubric/render 三件全住此包（report_unit.rs production 的
#       pkg_root 锚），登记表头亦钉「report 判分 = reportkit 三件」。包内
#       新增 .py 自动入下界（不登记即红，fail-closed）。不稳定成分显式
#       排除（按文件种类，同 fix_verify 排除面先例）：__pycache__/ 目录与
#       *.bak（字节码产物/历史快照非正本——__pycache__ 运行侧另有 W2-C52
#       在场性门独立守）。判分包目录缺席或该臂派生为空 = 红（保护面无法
#       派生，不静默缩面）。
#   臂2 swev 判分臂 = 生产源（crates/*/src/ 下 **.rs）中声明判分脚本
#       sha256 锚（script_sha256 字段——judge.rs「判分脚本 sha256 锚」，
#       即对仓外 swev-judge.py 的冻结锚）的文件 = 判分门本体（swev 判分核
#       swev-judge.py 不在仓内，仓内保护面 = 冻结锚所在的门本体，登记表
#       头规则）。内容型判定、非文件名判定：crates/af-credential/src/
#       judge.rs 系权限交集判定器（Cedar∩biscuit），与判分链同名不同物，
#       不因撞名入面；tests/ 下的 script_sha256 是测试构造 locator，非门
#       本体，不在生产源扫描面。该臂派生为空 = 红。
#   互为跨锚（强度不弱于手写清单）：派生下界只能通过「改判分链本体字节」
#   来收缩——在册成员字节被登记表 sha256 锚死（腿1 逐位对锚），任何收缩
#   动作先红腿1、走裁决（代码 diff 即评审面，冻结纪律不变）。
# =============================================================================
import hashlib
import pathlib
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = REPO_ROOT / "ci" / "scorechain-manifest.txt"

# ----------------------------------------------------------------------
# 判分链必需成员下界——派生规则常量（规则全文与证据见文件头；G4，2026-11-04）：
# ----------------------------------------------------------------------
# 臂1 report 判分臂载体包（判分包内 *.py 全集 = report 判分脚本面）
REPORT_ARM_PKG = pathlib.Path("af") / "packages" / "reportkit"
# 臂2 swev 判分臂生产源面（crates/*/src/ 下 **.rs；tests/ 不在面内）
GATE_SRC_PATTERN = "*/src"
# 判分门本体的内容型标记：判分脚本 sha256 锚字段（judge.rs 声明，tests 只是
# 构造非门本体；af-credential/judge.rs 无此标记不因撞名入面）
GATE_BODY_MARKER = "script_sha256"
# 派生排除面（按文件种类排除不稳定成分，同 ci/fix_verify.py 排除面先例）
EXCLUDE_DIRNAMES = ("__pycache__",)  # 字节码产物目录（W2-C52 运行侧另有在场性门）
EXCLUDE_SUFFIXES = (".bak",)  # 历史快照（任何位置的 .bak 都不是正本）


def _excluded(relpath: pathlib.Path) -> bool:
    """派生排除面：按文件种类判定（目录名/后缀），不按具体路径点名。"""
    return (
        any(part in EXCLUDE_DIRNAMES for part in relpath.parts)
        or relpath.suffix in EXCLUDE_SUFFIXES
    )


def derive_report_arm() -> list:
    """臂1：report 判分臂下界 = 判分包（af/packages/reportkit/）内全部 *.py。

    包目录缺席/派生为空即红——保护面无法派生时不静默缩面（fail-closed）。"""
    pkg = REPO_ROOT / REPORT_ARM_PKG
    if not pkg.is_dir():
        sys.exit(
            "k5-path-lint: report 判分臂载体包缺席（"
            + REPORT_ARM_PKG.as_posix()
            + "/）——下界无法派生即红（fail-closed，不静默缩面）"
        )
    members = [
        p.relative_to(REPO_ROOT).as_posix()
        for p in sorted(pkg.rglob("*.py"))
        if not _excluded(p.relative_to(REPO_ROOT))
    ]
    if not members:
        sys.exit(
            "k5-path-lint: report 判分臂派生为空（判分包内无 .py 判分脚本）"
            "——下界无法派生即红（fail-closed，不静默缩面）"
        )
    return members


def derive_swev_arm() -> list:
    """臂2：swev 判分臂下界 = 生产源中声明判分脚本 sha256 锚的门本体文件。

    内容型判定（GATE_BODY_MARKER 在文件字节内），非文件名判定；派生为空
    即红（fail-closed）。"""
    members = []
    crates_dir = REPO_ROOT / "crates"
    for gate_src in sorted(crates_dir.glob(GATE_SRC_PATTERN)):
        for src in sorted(gate_src.rglob("*.rs")):
            rel = src.relative_to(REPO_ROOT)
            if _excluded(rel):
                continue
            if GATE_BODY_MARKER in src.read_text(encoding="utf-8"):
                members.append(rel.as_posix())
    if not members:
        sys.exit(
            "k5-path-lint: swev 判分臂派生为空（生产源无声明判分脚本 sha256 "
            "锚（" + GATE_BODY_MARKER + "）的判分门本体）——下界无法派生即红"
            "（fail-closed，不静默缩面）"
        )
    return members


def derive_required_members() -> tuple:
    """判分链必需成员下界（判分核 + 判分门本体；缺一即红）——仓面派生全集。

    两臂并集，排序稳定（可复算：同仓面 ⇒ 同集合）。"""
    return tuple(sorted(derive_report_arm() + derive_swev_arm()))


def main() -> int:
    entries = []
    for raw in MANIFEST.read_text(encoding="utf-8").splitlines():
        line = raw.rstrip("\n")
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        cols = [c.strip() for c in line.split("\t")]
        if len(cols) < 3 or not cols[0] or not cols[1]:
            sys.exit(f"k5-path-lint: 登记表行格式损坏：{raw!r}")
        entries.append((cols[0], cols[1], cols[2]))
    if not entries:
        sys.exit("k5-path-lint: 登记表为空（判分链保护面缺失）")

    # 判分链成员下界对账：登记表被收缩（必需成员缺席）即红——否则被摘掉登记
    # 行的判分门本体脱离哈希锚保护面、任意改写而本门照绿（RT-E6/E51）。
    # 下界成员自仓面派生（G4，2026-11-04；派生规则见文件头与
    # derive_required_members）——判定逻辑与强度和手写清单版逐字同源。
    registered = {relpath for relpath, _anchor, _purpose in entries}
    missing = [
        relpath
        for relpath in derive_required_members()
        if relpath not in registered
    ]
    if missing:
        sys.exit(
            "k5-path-lint: 判分链登记表被收缩（必需成员缺席："
            + "、".join(missing)
            + "）——判分链成员脱离哈希锚保护面即红"
            "（有意移除须走裁决并同批改本文件下界派生规则）"
        )

    drift = []
    for relpath, anchor, purpose in entries:
        path = REPO_ROOT / relpath
        if not path.is_file():
            drift.append(f"  {relpath}: 文件缺席（判分链成员被删/被移）——{purpose}")
            continue
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if digest != anchor:
            drift.append(
                f"  {relpath}: sha256 漂移（表锚 {anchor[:12]}…，实得 {digest[:12]}…）"
                f"——{purpose}"
            )
    if drift:
        print("K5 判分链路径 lint: FAIL（判分链被触碰——有意变更须走裁决并在")
        print("ci/scorechain-manifest.txt 登记新锚 + 裁决出处）：")
        print("\n".join(drift))
        return 1
    print(f"K5 判分链路径 lint: PASS（{len(entries)} 件判分链成员逐位对锚）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
