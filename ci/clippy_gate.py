#!/usr/bin/env python3
"""clippy_gate.py — clippy 警告基线门。

策略（ci-skeleton 定案）: 现有代码不要求零警告；以 (lint 名, 文件路径, 出现次数) 为签名
记录基线；本次运行出现"基线中没有的警告"（同签名次数超基线，或全新签名）才 fail。
行号漂移不影响判定；文件删除/警告修好后基线偏松（只松不紧），可用 --update 收紧。

用法:
    cargo clippy --workspace --all-targets -j4 --message-format=json > out/clippy.jsonl
    python3 ci/clippy_gate.py out/clippy.jsonl              # 门禁: 新增警告 → exit 1
    python3 ci/clippy_gate.py out/clippy.jsonl --update     # 重新固化基线（人工操作）

基线文件格式（ci/clippy-baseline.txt，每行一条，tab 分隔）:
    <count>\t<lint>\t<相对仓库根的文件路径>

基线钉（fail-closed）: 判据正本须与外部哈希钉 ci/baselines/clippy-baseline.sha256 对账
（格式 "<sha256>  <数据行数>  clippy-baseline.txt"，同 mutations-registry.sha256 家族约定）。
钉文件缺席、格式坏、哈希或行数不匹配 → exit 2 红，绝不拿未钉/被篡改的基线判 PASS。
--update 重写基线时同步刷新钉（人工操作，钉文件 diff 随批评审）。
"""
import argparse
import collections
import hashlib
import json
import sys
from pathlib import Path

DEFAULT_BASELINE = Path(__file__).resolve().parent / "clippy-baseline.txt"
DEFAULT_PIN = Path(__file__).resolve().parent / "baselines" / "clippy-baseline.sha256"


def baseline_data_lines(path: Path) -> int:
    """基线数据行数（非空、非 # 注释）。"""
    return sum(1 for l in path.read_text(encoding="utf-8").splitlines()
               if l.strip() and not l.startswith("#"))


def pin_error(baseline: Path, pin: Path):
    """判据正本 vs 外部哈希钉 fail-closed 对账；返回 None=通过，否则返回红字消息。"""
    if not pin.exists():
        return f"基线钉文件缺席（{pin}）——判据未钉，fail-closed 红"
    parts = pin.read_text(encoding="utf-8").splitlines()[0].split()
    if len(parts) != 3:
        return (f"基线钉文件格式坏（{pin}）——须为 "
                f"'<sha256>  <数据行数>  clippy-baseline.txt'")
    want_sha, want_count = parts[0], parts[1]
    got_sha = hashlib.sha256(baseline.read_bytes()).hexdigest()
    got_count = baseline_data_lines(baseline)
    if got_sha != want_sha or str(got_count) != want_count:
        return (f"基线正本漂移：期望 {want_count} 数据行/哈希 {want_sha[:12]}…，"
                f"实得 {got_count} 数据行/哈希 {got_sha[:12]}…——判据被改须走 --update 同批评审")
    return None


def collect(jsonl_path: str, repo_root: str):
    """从 cargo --message-format=json 输出提取 (lint, 相对路径) → 次数。"""
    counts = collections.Counter()
    n_total = 0
    root = str(repo_root)
    with open(jsonl_path, encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue  # 非 JSON 行（cargo 进度输出）
            if obj.get("reason") != "compiler-message":
                continue
            msg = obj.get("message") or {}
            if msg.get("level") != "warning":
                continue
            n_total += 1
            lint = (msg.get("code") or {}).get("code") or "<no-lint>"
            primary = next((s for s in msg.get("spans", []) if s.get("is_primary")),
                           (msg.get("spans") or [None])[0])
            path = primary["file_name"] if primary else "<virtual>"
            if path.startswith(root):
                path = path[len(root):].lstrip("/")
            counts[(lint, path)] += 1
    return counts, n_total


def load_baseline(path: Path):
    base = collections.Counter()
    if not path.exists():
        return base
    for raw in path.read_text(encoding="utf-8").splitlines():
        raw = raw.strip()
        if not raw or raw.startswith("#"):
            continue
        cnt, lint, fpath = raw.split("\t", 2)
        base[(lint, fpath)] += int(cnt)
    return base


def main() -> int:
    ap = argparse.ArgumentParser(description="clippy 警告基线门（新增警告才 fail）")
    ap.add_argument("jsonl", help="cargo clippy --message-format=json 的输出文件")
    ap.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    ap.add_argument("--update", action="store_true", help="用本次结果重写基线后 exit 0")
    ap.add_argument("--root", default=str(Path(__file__).resolve().parent.parent),
                    help="仓库根（用于路径归一化）")
    args = ap.parse_args()

    cur, n_total = collect(args.jsonl, args.root)
    if n_total == 0 and not cur:
        # 没有任何 warning 诊断：可能是输入拿错了（如普通文本日志），直接拒绝当基线输入
        head = open(args.jsonl, encoding="utf-8", errors="replace").readline().strip()
        if not head.startswith("{"):
            print(f"clippy-gate: 输入不像 --message-format=json（首行: {head[:60]!r}）", file=sys.stderr)
            return 2

    if args.update:
        lines = [f"{cnt}\t{lint}\t{path}" for (lint, path), cnt in sorted(cur.items())]
        args.baseline.write_text(
            "# clippy 警告基线 — 由 `python3 ci/clippy_gate.py <jsonl> --update` 生成\n"
            "# 策略: 只拦新增警告；修好现有警告后请用 --update 收紧基线\n"
            + "".join(l + "\n" for l in lines), encoding="utf-8")
        DEFAULT_PIN.parent.mkdir(parents=True, exist_ok=True)
        DEFAULT_PIN.write_text(
            f"{hashlib.sha256(args.baseline.read_bytes()).hexdigest()}"
            f"  {len(lines)}  clippy-baseline.txt\n", encoding="utf-8")
        print(f"clippy-gate: 基线已更新 → {args.baseline}（{len(lines)} 签名 / {n_total} 警告）"
              f"，钉已刷新 → {DEFAULT_PIN}")
        return 0

    if not args.baseline.exists():
        print(f"clippy-gate: 基线缺失 {args.baseline} —— 先用 --update 生成", file=sys.stderr)
        return 2
    pin_err = pin_error(args.baseline, DEFAULT_PIN)
    if pin_err:
        print(f"clippy-gate: {pin_err}", file=sys.stderr)
        return 2
    base = load_baseline(args.baseline)

    new = {sig: cnt for sig, cnt in cur.items() if cnt > base.get(sig, 0)}
    fixed = {sig: base[sig] - cnt for sig, cnt in cur.items() if cnt < base[sig]}
    gone = [sig for sig in base if sig not in cur]

    if new:
        print(f"CLIPPY GATE: FAIL —— {len(new)} 个签名出现新增警告（新增 {sum(new.values())} 条）:")
        for (lint, path), cnt in sorted(new.items()):
            print(f"  +{cnt - base.get((lint, path), 0):d} {lint} {path}（现 {cnt} / 基线 {base.get((lint, path), 0)}）")
        return 1
    print(f"CLIPPY GATE: PASS —— 现有 {n_total} 条警告 / {len(cur)} 签名，均在基线内"
          f"（基线 {len(base)} 签名）")
    if fixed or gone:
        print(f"  提示: {sum(fixed.values())} 条减少、{len(gone)} 签名消失，"
              f"可用 --update 收紧基线")
    return 0


if __name__ == "__main__":
    sys.exit(main())
