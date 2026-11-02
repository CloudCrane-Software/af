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
# =============================================================================
import hashlib
import pathlib
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = REPO_ROOT / "ci" / "scorechain-manifest.txt"

# 判分链必需成员下界（判分核 + 判分门本体；缺一即红——登记表收缩=逃逸通道）
REQUIRED_MEMBERS = (
    "crates/af-execline/src/judge.rs",
    "af/packages/reportkit/units/citation_unit.py",
    "af/packages/reportkit/units/rubric_unit.py",
    "af/packages/reportkit/tools/render.py",
)


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
    registered = {relpath for relpath, _anchor, _purpose in entries}
    missing = [relpath for relpath in REQUIRED_MEMBERS if relpath not in registered]
    if missing:
        sys.exit(
            "k5-path-lint: 判分链登记表被收缩（必需成员缺席："
            + "、".join(missing)
            + "）——判分链成员脱离哈希锚保护面即红"
            "（有意移除须走裁决并同批改 REQUIRED_MEMBERS 下界清单）"
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
