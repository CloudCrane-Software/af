#!/usr/bin/env python3
"""parse_scrub.py — zot scrub 表格输出解析门。

用法:
    parse_scrub.py [LOGFILE ...]        # 有参数: 逐文件解析; 无参数: 读 stdin
选项:
    --json        机器可读输出（每输入一个 JSON 对象数组）

背景（poc-cas/VERIFY.md ②-B/④-2）: `zot scrub <config>` 检出损坏时退出码恒为 0，
自动化必须解析表格输出。表格形状（真实样例见 poc/poc-cas/verify-logs/scrub-*.log）:

    REPOSITORY TAG STATUS   AFFECTED BLOB  ... ERROR
    scrub/repo v1  affected 61bc01...      blob not found: ...
    gclong/img c0   ok

zot 可能在表格前后混入 JSON debug 日志行——表头（首 token 为 REPOSITORY 且含 STATUS）
之前的行一律忽略；表头之后的非空行必须是数据行（第 3 个字段为 ok 或 affected），
无法识别则按解析错误处理，宁可误红不可漏报。

退出码: 0 = 全部 ok; 1 = 出现 STATUS=affected（或损坏判定）; 2 = 解析失败（无表头/坏行）。
"""
import argparse
import json
import sys

HEADER_TOKENS = ("REPOSITORY", "TAG", "STATUS")
KNOWN_STATUS = {"ok", "affected"}


def parse_stream(text: str, name: str):
    """解析一段 scrub 输出，返回 (rows, errors)。
    rows: [{"repo","tag","status","affected_blob","error"}]；errors: [str]。"""
    rows, errors = [], []
    saw_header = False
    for lineno, raw in enumerate(text.splitlines(), 1):
        line = raw.replace("\t", "  ").strip()
        if not line:
            continue
        fields = line.split()
        if not saw_header:
            # 表头行: 首三 token 匹配 REPOSITORY TAG STATUS
            if fields[:3] == list(HEADER_TOKENS):
                saw_header = True
            # 表头前的内容（zot debug 日志等）一律忽略
            continue
        # 表头之后: 非空行必须是数据行
        if len(fields) < 3:
            errors.append(f"{name}:{lineno}: 数据行字段不足: {line[:80]!r}")
            continue
        status = fields[2]
        if status not in KNOWN_STATUS:
            errors.append(f"{name}:{lineno}: 未知 STATUS {status!r}: {line[:80]!r}")
            continue
        rows.append({
            "repo": fields[0],
            "tag": fields[1],
            "status": status,
            "affected_blob": fields[3] if len(fields) > 3 else "",
            "error": " ".join(fields[4:]) if len(fields) > 4 else "",
        })
    if not saw_header:
        errors.append(f"{name}: 未找到 scrub 表头（REPOSITORY TAG STATUS ...）")
    return rows, errors


def main() -> int:
    ap = argparse.ArgumentParser(description="zot scrub 表格输出解析门（affected → 非零退出）")
    ap.add_argument("files", nargs="*", help="scrub 输出日志；缺省读 stdin")
    ap.add_argument("--json", action="store_true", help="输出机器可读 JSON")
    args = ap.parse_args()

    streams = []
    if args.files:
        for path in args.files:
            try:
                with open(path, encoding="utf-8", errors="replace") as f:
                    streams.append((path, f.read()))
            except OSError as e:
                print(f"parse_scrub: 无法读取 {path}: {e}", file=sys.stderr)
                return 2
    else:
        streams.append(("<stdin>", sys.stdin.read()))

    all_rows, all_errors = [], []
    for name, text in streams:
        rows, errors = parse_stream(text, name)
        all_rows.extend(rows)
        all_errors.extend(errors)

    if args.json:
        print(json.dumps({"rows": all_rows, "errors": all_errors,
                          "verdict": "affected" if any(r["status"] == "affected" for r in all_rows)
                          else ("parse-error" if all_errors else "ok")}, ensure_ascii=False, indent=2))
    if all_errors:
        for e in all_errors:
            print(f"PARSE-ERROR {e}", file=sys.stderr)
        print(f"SCRUB GATE: FAIL（解析失败，{len(all_errors)} 处）—— 视为损坏处理", file=sys.stderr)
        return 2
    affected = [r for r in all_rows if r["status"] == "affected"]
    for r in affected:
        print(f"AFFECTED {r['repo']} {r['tag']} blob={r['affected_blob']} err={r['error'][:100]}")
    if affected:
        print(f"SCRUB GATE: FAIL —— {len(affected)} 个 tag STATUS=affected"
              f"（共 {len(all_rows)} 行）", file=sys.stderr)
        return 1
    print(f"SCRUB GATE: PASS —— {len(all_rows)} 行全部 ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
