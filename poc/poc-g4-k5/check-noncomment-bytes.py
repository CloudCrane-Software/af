#!/usr/bin/env python3
# =============================================================================
# 判据面零弱化机械证明（G4 返修批）：剥离整行注释后，工作树
# ci/k5-path-lint.py 与基线 ref（返修前提交 f3cacdb 的同路径 blob）逐字节比对。
#
# 用法：python3 -I check-noncomment-bytes.py <基线导出件> <工作树件>
# 剥离口径：整行注释（lstrip 后以 # 起头，含 shebang）整行剔除；其余行
# （代码行、空行、字符串字面量）逐字节保留。本返修批对门件的改动全部为
# 整行注释（git diff 行级分类见 evidence/noncomment-bytecheck.log），故该
# 口径下比对相等 ⇔ 非注释行逐字节未变（判定逻辑/红条件/exit 语义零改动）。
# 比对不等 = exit 1（fail-loud）。
# =============================================================================
import hashlib
import pathlib
import sys


def strip_comment_lines(path: str) -> tuple:
    raw = pathlib.Path(path).read_bytes()
    lines = raw.splitlines(keepends=True)
    kept = [ln for ln in lines if not ln.lstrip().startswith(b"#")]
    return b"".join(kept), len(lines), len(lines) - len(kept)


def main() -> int:
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    base_kept, base_total, base_cmt = strip_comment_lines(sys.argv[1])
    cur_kept, cur_total, cur_cmt = strip_comment_lines(sys.argv[2])
    print(f"基线 ref（{sys.argv[1]}）: {base_total} 行，整行注释 {base_cmt} 行"
          f" → 剥离后 {len(base_kept.splitlines())} 行")
    print(f"工作树（{sys.argv[2]}）: {cur_total} 行，整行注释 {cur_cmt} 行"
          f" → 剥离后 {len(cur_kept.splitlines())} 行")
    print(f"剥离后 sha256（基线 ref）= {hashlib.sha256(base_kept).hexdigest()}")
    print(f"剥离后 sha256（工作树）  = {hashlib.sha256(cur_kept).hexdigest()}")
    if base_kept == cur_kept:
        print("非注释行逐字节比对: IDENTICAL（判据面零改动）")
        return 0
    print("非注释行逐字节比对: DIFFER（判据面被触碰——禁止收卷）")
    return 1


if __name__ == "__main__":
    sys.exit(main())
