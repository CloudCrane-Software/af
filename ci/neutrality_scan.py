#!/usr/bin/env python3
# =============================================================================
# N2 核心领域词扫描器（仓内化正本）——《G4-任务中立性机器检查方案.md》§2.2
#
# 范围：crates/af-dsl/src/**/*.rs + crates/af-compiler/src/**/*.rs（含 src/bin/
#       ——CLI 入口属核心；不含 tests/、testdata/、docs/——测试语料以对齐稿为
#       名是设计使然；af-execline 等执行面 crate 不在范围——方案 §2.2 边界）。
# 词表：ci/neutrality-wordlist.txt（黑名单+白名单合册正本；本脚本只消费不定义）。
# 算法：\b(词1|词2|…)\\b IGNORECASE，长词在前防前缀吞并；词边界保证
#       ErrorCode/BindReport/reactive/searchkit 不误伤。
# 豁免：白名单条目级（文件:行 + 同词出现次数），不做文件级豁免（开放问题 6）。
# 输出：逐命中 相对路径:行号: [词] 行文前 100 字符（豁免项标 [白名单]）+ 末行计数。
# 退出码：0=白名单外命中 0（S1 绿）；1=白名单外命中 ≥1（R6 门红——铁律是零，
#         无「容忍 N 条」软阈值）；2=词表/配置损坏。
# =============================================================================
import pathlib
import re
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
WORDLIST = REPO_ROOT / "ci" / "neutrality-wordlist.txt"
SCAN_ROOTS = ["crates/af-dsl/src", "crates/af-compiler/src"]


def load_wordlist(path):
    """解析黑名单+白名单合册。返回 (blacklist_words, whitelist{(relpath,line,word): count})。"""
    blacklist, whitelist = [], {}
    in_whitelist_section = False
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.rstrip("\n")
        if "====" in line:  # 段标记（可带 # 前缀）：黑名单段/白名单段切换
            in_whitelist_section = "白名单" in line
            continue
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        cols = [c.strip() for c in line.split("\t")]
        if not in_whitelist_section:
            # 词 <TAB> 来源 <TAB> 处置 <TAB> 理由
            if len(cols) < 3 or not cols[0]:
                sys.exit(f"neutrality_scan: 黑名单段行格式损坏：{raw!r}")
            if not cols[2].startswith("黑名单"):
                sys.exit(f"neutrality_scan: 黑名单段处置列非法：{raw!r}")
            blacklist.append(cols[0].lower())
        else:
            # 词 <TAB> 处置 <TAB> 命中位置 <TAB> 次数 <TAB> 出处 <TAB> 理由 <TAB> 日期
            if len(cols) < 4 or cols[1] != "白名单":
                sys.exit(f"neutrality_scan: 白名单段行格式损坏：{raw!r}")
            loc, count = cols[2], cols[3]
            m = re.fullmatch(r"(.+):(\d+)", loc)
            if not m:
                sys.exit(f"neutrality_scan: 白名单命中位置非法（须 文件:行）：{raw!r}")
            key = (m.group(1), int(m.group(2)), cols[0].lower())
            whitelist[key] = whitelist.get(key, 0) + int(count)
    if not blacklist:
        sys.exit("neutrality_scan: 词表黑名单段为空")
    dup = {w for w in blacklist if blacklist.count(w) > 1}
    if dup:
        sys.exit(f"neutrality_scan: 黑名单重复词：{sorted(dup)}")
    return blacklist, whitelist


def main():
    blacklist, whitelist = load_wordlist(WORDLIST)
    # 长词在前防前缀吞并（词边界下双保险）
    ordered = sorted(blacklist, key=len, reverse=True)
    pattern = re.compile(
        r"\b(" + "|".join(re.escape(w) for w in ordered) + r")\b", re.IGNORECASE
    )

    hits = []  # (relpath, lineno, word, linetext)
    for root in SCAN_ROOTS:
        base = REPO_ROOT / root
        if not base.is_dir():
            sys.exit(f"neutrality_scan: 扫描根不存在：{root}")
        for path in sorted(base.rglob("*.rs")):
            rel = str(path.relative_to(REPO_ROOT))
            for lineno, text in enumerate(
                path.read_text(encoding="utf-8").splitlines(), 1
            ):
                for m in pattern.finditer(text):
                    hits.append((rel, lineno, m.group(1).lower(), text.strip()[:100]))

    # 计数对账：白名单按 (文件,行,词) 容纳次数；超出部分即白名单外命中
    grouped = {}
    for rel, lineno, word, text in hits:
        grouped.setdefault((rel, lineno, word), []).append(text)
    allowed = dict(whitelist)
    violations, exempt_used, exempt_stale = [], 0, []
    for key in sorted(grouped):
        texts = grouped[key]
        cap = allowed.get(key, 0)
        for i, text in enumerate(texts):
            if i < cap:
                print(f"{key[0]}:{key[1]}: [白名单] [{key[2]}] {text}")
                exempt_used += 1
            else:
                violations.append((key[0], key[1], key[2], text))
    for key in sorted(set(allowed) - set(grouped)):
        exempt_stale.append(key)

    for rel, lineno, word, text in violations:
        print(f"{rel}:{lineno}: [词] {word} {text}")
    print(
        f"\n扫描词表：黑名单 {len(blacklist)} 词；命中 {len(hits)} 处"
        f"（白名单豁免 {exempt_used} / 白名单外 {len(violations)}）；"
        f"失效白名单条目 {len(exempt_stale)}"
    )
    if exempt_stale:
        for rel, lineno, word in exempt_stale:
            print(f"  白名单条目已无对应命中（漂移，须更新词表）：{rel}:{lineno} [{word}]")
    if violations:
        print("S1=FAIL：白名单外命中 ≥1（R6 门红——处置：改代码去领域词（首选）或白名单登记附理由）")
        sys.exit(1)
    if exempt_stale:
        print("S1=FAIL：白名单条目漂移（命中消失未除名）——更新词表并登记理由")
        sys.exit(1)
    print("S1=PASS：白名单外命中数 = 0")
    sys.exit(0)


if __name__ == "__main__":
    main()
