#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""step11 自述-实现对账门（红队复盘 C1:104/C10:245/PP3:286；开发计划 §2.1 表行 2）。

对 doc-claims.txt 逐条 claim 做三件事：
  ① 引文核位：quote 必须真实出现在 source_file 的 source_line 行（防编造 claim）；
  ② 证据核验，三种形态：
     cmd:<命令>       —— CI 每批复跑（cwd=--repo），stdout.strip() 必须等于 expect；
     test:<测试名>    —— 测试名必须在仓测试面在场（git grep crates + ci/run.sh；通过与否由 step3 承载）；
     exempt:history / exempt:manual —— 显式豁免：history 需 evidence_path（+可选 sha256），
                        manual 需 note 含 date:YYYY-MM-DD 且年龄 ≤ 新鲜度档位（7d/1d）；
  ③ 新鲜度档位：7d=默认（证据年龄 ≤7 天）；1d=跨天/速变判据；exempt=显式豁免。
退出码：0=全过；1=有 claim 红；2=基础设施缺件。
"""
import argparse
import datetime
import hashlib
import os
import re
import subprocess
import sys

COLUMNS = ['claim_id', 'freshness', 'owner', 'source_file', 'source_line',
           'quote', 'evidence', 'expect', 'evidence_path', 'evidence_sha256', 'note']


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 16), b''):
            h.update(chunk)
    return h.hexdigest()


def norm(s):
    return re.sub(r'\s+', ' ', s).strip()


def resolve(f, roots):
    for r in roots:
        p = os.path.join(r, f)
        if os.path.isfile(p):
            return p
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--file', required=True, help='doc-claims.txt 路径')
    ap.add_argument('--repo', required=True, help='af 仓根（cmd/test 证据执行根）')
    ap.add_argument('--docs-root', default=None,
                    help='文书根（source_file 解析根；缺省=--repo）')
    args = ap.parse_args()
    docs_root = args.docs_root or args.repo
    roots = [docs_root, args.repo]

    if not os.path.isfile(args.file):
        print(f"step11 FAIL(infra): doc-claims.txt 缺席：{args.file}", file=sys.stderr)
        return 2
    lines = open(args.file, encoding='utf-8').read().splitlines()
    header = None
    claims = []
    for ln, raw in enumerate(lines, 1):
        if not raw.strip() or raw.startswith('#'):
            continue
        if header is None:
            header = raw.split('\t')
            if header != COLUMNS:
                print(f"step11 FAIL(infra): 表头不符：{header}（期望 {COLUMNS}）",
                      file=sys.stderr)
                return 2
            continue
        fields = raw.split('\t')
        if len(fields) != len(COLUMNS):
            print(f"step11 FAIL(infra): 第 {ln} 行字段数 {len(fields)} ≠ {len(COLUMNS)}",
                  file=sys.stderr)
            return 2
        claims.append(dict(zip(COLUMNS, fields)))
    if header is None or not claims:
        print("step11 FAIL(infra): 无有效表头/数据行", file=sys.stderr)
        return 2

    red = []
    tiers = {'7d': 7, '1d': 1}

    def bad(cid, msg):
        red.append(cid)
        print(f"RED [{cid}] {msg}")

    for c in claims:
        cid = c['claim_id']
        # ① 引文核位
        src = resolve(c['source_file'], roots)
        if not src:
            bad(cid, f"source_file 缺席：{c['source_file']}")
            continue
        try:
            n = int(c['source_line'])
        except ValueError:
            bad(cid, f"source_line 非整数：{c['source_line']!r}")
            continue
        doc_lines = open(src, encoding='utf-8', errors='replace').read().splitlines()
        if not (1 <= n <= len(doc_lines)):
            bad(cid, f"source_line {n} 越界（{c['source_file']} 共 {len(doc_lines)} 行）")
            continue
        if norm(c['quote']) not in norm(doc_lines[n - 1]):
            bad(cid, f"引文与正本不符：{c['source_file']}:{n} 不含 quote"
                      f"（编造/漂移的 claim——自述提取完整性破绽）")
            continue
        # ② 证据核验
        ev = c['evidence']
        if ev.startswith('cmd:'):
            cmd = ev[4:]
            r = subprocess.run(cmd, shell=True, cwd=args.repo,
                               capture_output=True, text=True)
            out = r.stdout.strip()
            if r.returncode != 0:
                bad(cid, f"cmd 证据非零退出 rc={r.returncode}: {cmd}")
            elif out != c['expect']:
                bad(cid, f"cmd 复算值漂移：登记 expect={c['expect']!r} 实得={out!r}"
                          f"（{cmd}）——自述句已与实现脱钩")
            else:
                print(f"OK  [{cid}] 引文@{c['source_file']}:{n} + cmd 复跑={out}"
                      f"（档位 {c['freshness']}，复跑即新鲜）")
        elif ev.startswith('test:'):
            name = ev[5:]
            r = subprocess.run(f"git grep -l -F {name!r} -- crates ci/run.sh",
                               shell=True, cwd=args.repo, capture_output=True, text=True)
            if r.returncode != 0:
                bad(cid, f"test 证据不在测试面：{name}")
            else:
                print(f"OK  [{cid}] 引文@{c['source_file']}:{n} + 测试在场 {name}"
                      f"（通过与否由 step3 承载；档位 {c['freshness']}）")
        elif ev == 'exempt:history':
            ep = c['evidence_path']
            p = resolve(ep, roots) if ep else None
            if not p:
                bad(cid, f"exempt:history 需在场 evidence_path，缺席：{ep!r}")
            else:
                oksha = True
                if c['evidence_sha256'] and c['evidence_sha256'] != '-':
                    oksha = sha256_file(p) == c['evidence_sha256']
                if not oksha:
                    bad(cid, "exempt:history 的 evidence_sha256 不符")
                else:
                    print(f"OK  [{cid}] 引文@{c['source_file']}:{n} + 历史豁免"
                          f"（不可复跑事实，evidence={ep} 在场且哈希一致，不设年龄衰减）")
        elif ev == 'exempt:manual':
            m = re.search(r'date:(\d{4}-\d{2}-\d{2})', c['note'])
            if not m:
                bad(cid, "exempt:manual 需 note 含 date:YYYY-MM-DD（属主+日期+最长年龄）")
                continue
            if c['freshness'] not in tiers:
                bad(cid, f"manual 豁免的新鲜度档位非法：{c['freshness']}")
                continue
            d = datetime.date.fromisoformat(m.group(1))
            age = (datetime.date.today() - d).days
            if age > tiers[c['freshness']]:
                bad(cid, f"manual 证据过期：{age}d > {c['freshness']}（W2-C-01 的 ✅ 过期即此类）")
            else:
                print(f"OK  [{cid}] 引文@{c['source_file']}:{n} + manual 豁免 {age}d"
                      f" ≤ {c['freshness']}（owner={c['owner']}）")
        else:
            bad(cid, f"证据形态非法：{ev!r}（cmd:|test:|exempt:history|exempt:manual）")

    print(f"step11 自述-实现对账: claims={len(claims)} 红={len(red)}")
    if red:
        print(f"step11 FAIL: 以下 claim 无有效测试/豁免支撑：{', '.join(red)}")
        return 1
    print("step11 PASS: 全部 claim 有测试名或显式豁免在册")
    return 0


if __name__ == '__main__':
    sys.exit(main())
