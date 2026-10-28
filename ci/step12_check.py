#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""step12 计数复算门（红队复盘 C2:117-118/C3/PP4:292；开发计划 §2.1 表行 3）。

对 numbers.json 做三件事：
  ① 限定名键校验：每键必须 <域>.<名> 形态（裸键无法消歧——六门 vs 九门史的根治）；
  ② 逐键跑来源命令复算，stdout 必须解析为整数且 == 登记值（漂移即红）；
  ③ 文书扫描：
     a. {{numbers.<key>}} 占位符必须解析到在册键；
     b. watch 手写计数对值：正则命中已知度量名（含汉字数词映射）即与 numbers.json
        对值，不符即红；skip_pattern 命中处豁免（勘误/历史行——AGENT.md:27/87 实测：
        同一行既含现行「九门」又含勘误注记「六门」，须窗口级豁免而非行级）。
退出码：0=全过；1=计数红；2=基础设施缺件。
"""
import argparse
import datetime
import json
import os
import re
import subprocess
import sys

KEY_RE = re.compile(r'^[a-z0-9_]+\.[a-z0-9_]+$')
PLACEHOLDER_RE = re.compile(r'\{\{numbers\.([A-Za-z0-9_.]+)\}\}')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--file', required=True, help='numbers.json 路径')
    ap.add_argument('--repo', required=True, help='来源命令执行根（af 仓根）')
    ap.add_argument('--docs-root', default=None, help='--docs 相对解析根（缺省=--repo）')
    ap.add_argument('--docs', action='append', default=[],
                    help='待扫文书（相对 --docs-root，可多次）')
    args = ap.parse_args()
    docs_root = args.docs_root or args.repo

    if not os.path.isfile(args.file):
        print(f"step12 FAIL(infra): numbers.json 缺席：{args.file}", file=sys.stderr)
        return 2
    try:
        doc = json.load(open(args.file, encoding='utf-8'))
    except (OSError, json.JSONDecodeError) as e:
        print(f"step12 FAIL(infra): 解析失败：{e}", file=sys.stderr)
        return 2
    numbers = doc.get('numbers')
    if not isinstance(numbers, dict) or not numbers:
        print("step12 FAIL(infra): 顶层缺 numbers 对象", file=sys.stderr)
        return 2

    red = []

    def bad(msg):
        red.append(msg)
        print(f"RED {msg}")

    # ① 限定名键 + ② 逐键复算
    for key, ent in numbers.items():
        if not KEY_RE.match(key):
            bad(f"键 {key!r} 非限定名（<域>.<名>）——裸键无法消歧（复盘:117）")
            continue
        if not isinstance(ent, dict) or 'value' not in ent or 'source' not in ent:
            bad(f"[{key}] 条目缺 value/source（分子分母单源机算的最小完备集）")
            continue
        cmd = ent['source']
        cwd = os.path.join(args.repo, ent['cwd']) if ent.get('cwd') else args.repo
        r = subprocess.run(cmd, shell=True, cwd=cwd,
                           capture_output=True, text=True)
        out = r.stdout.strip()
        try:
            got = int(out.splitlines()[-1])
        except (ValueError, IndexError):
            bad(f"[{key}] 来源命令不可机数（rc={r.returncode}, out={out!r}）")
            continue
        if got != ent['value']:
            bad(f"[{key}] 复算值漂移：登记={ent['value']} 复算={got}（{cmd}）"
                f"——数字跨文书漂移活现（35vs37/六门九门同模式）")
        else:
            print(f"OK  [{key}] 复算 {got} == 登记（{cmd.split()[0]}…，"
                  f"last_recomputed={ent.get('last_recomputed')}）")

    # ③a 占位符解析
    for d in args.docs:
        p = os.path.join(docs_root, d)
        if not os.path.isfile(p):
            bad(f"待扫文书缺席：{d}")
            continue
        text = open(p, encoding='utf-8', errors='replace').read()
        for m in PLACEHOLDER_RE.finditer(text):
            k = m.group(1)
            if k not in numbers:
                bad(f"{d}: 占位符 {{{{numbers.{k}}}}} 未解析到在册键")
            else:
                print(f"OK  {d}: {{{{numbers.{k}}}}} -> {numbers[k]['value']}")

    # ③b 手写计数对值
    for w in doc.get('watch', []):
        key, pat = w['key'], w['pattern']
        if key not in numbers:
            bad(f"watch 引用未登记键：{key}")
            continue
        want = numbers[key]['value']
        skip = re.compile(w['skip_pattern']) if w.get('skip_pattern') else None
        cn = w.get('cn_map', {})
        for d in args.docs:
            p = os.path.join(docs_root, d)
            if not os.path.isfile(p):
                continue
            for ln, line in enumerate(
                    open(p, encoding='utf-8', errors='replace'), 1):
                for m in re.finditer(pat, line):
                    lo = max(0, m.start() - 40)
                    hi = min(len(line), m.end() + 40)
                    ctx = line[lo:hi]
                    if skip and skip.search(ctx):
                        print(f"SKIP {d}:{ln} 「{m.group(0)}」命中勘误/历史豁免语境")
                        continue
                    tok = m.group(w.get('group', 1))
                    try:
                        got = cn[tok] if cn and tok in cn else int(tok)
                    except (KeyError, ValueError):
                        bad(f"{d}:{ln} 「{m.group(0)}」数值不可解析（token={tok!r}）")
                        continue
                    if got != want:
                        bad(f"{d}:{ln} 手写计数「{m.group(0)}」={got} ≠ numbers.{key}"
                            f"={want}——登记值与文书漂移")
                    else:
                        print(f"OK  {d}:{ln} 「{m.group(0)}」== numbers.{key}={want}")

    print(f"step12 计数复算: keys={len(numbers)} watch={len(doc.get('watch', []))} "
          f"docs={len(args.docs)} 红={len(red)}")
    if red:
        print("step12 FAIL: 计数口径漂移（逐条 RED 见上）")
        return 1
    print("step12 PASS: 全部限定名键复算一致、占位符可解析、手写计数对值")
    return 0


if __name__ == '__main__':
    sys.exit(main())
