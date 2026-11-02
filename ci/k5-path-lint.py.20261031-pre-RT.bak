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
# =============================================================================
import hashlib
import pathlib
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = REPO_ROOT / "ci" / "scorechain-manifest.txt"


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
