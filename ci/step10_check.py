#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""step10 承诺-交付对账门（红队复盘 C1；开发计划 §2.1 表行 1）。

对 deliverables.yaml 做四件事（复盘:101）：
  ① 承诺正本条目数 == 清单条目数（sources[].count_command 机数复算）；
  ② 声明了 check 的条目逐项跑 check（八类 kind，见 SPEC §①）；
  ③ status=done 项：evidence_path 存在 + sha256==登记值 + 发布窗口
     （window_kind=git：最近提交 ≤ window_days 天；=mtime：文件年龄 ≤ window_days 天）；
  ④ status=descoped 项：descope_adr 必须解析到在册 ADR 文件。
open 项：登记完整性（owner/due/source）+ due 未过期即容忍；--closeout 时任何 open 即红。
退出码：0=全过；1=对账红；2=基础设施缺件（文件缺席/解析失败/子进程异常）。
零第三方依赖：deliverables.yaml 为严格子集 YAML（见 SPEC §①），内置解析器。
"""
import argparse
import datetime
import hashlib
import os
import re
import subprocess
import sys

# ---- 严格子集 YAML 解析（2 空格缩进 / key: value / '- ' 列表项 / 行内 [a, b]） ----

def _scalar(s):
    s = s.strip()
    if s.startswith('[') and s.endswith(']'):
        inner = s[1:-1].strip()
        return [x.strip().strip("'\"") for x in inner.split(',')] if inner else []
    if len(s) >= 2 and s[0] == s[-1] and s[0] in ('"', "'"):
        return s[1:-1]
    try:
        return int(s)
    except ValueError:
        return s


def parse_simple_yaml(path):
    """解析 deliverables.yaml 严格子集；越界语法一律抛错（fail-closed）。"""
    items = []
    with open(path, encoding='utf-8') as f:
        for raw in f:
            if not raw.strip() or raw.strip().startswith('#'):
                continue
            indent = len(raw) - len(raw.lstrip(' '))
            items.append((indent, raw.strip()))
    pos = [0]

    def block(indent):
        if pos[0] >= len(items):
            return {}
        if items[pos[0]][1].startswith('- '):
            out = []
            while (pos[0] < len(items) and items[pos[0]][0] == indent
                   and items[pos[0]][1].startswith('- ')):
                first = items[pos[0]][1][2:]
                pos[0] += 1
                m = {}
                if ':' in first:
                    k, _, v = first.partition(':')
                    if v.strip():
                        m[k.strip()] = _scalar(v)
                    elif pos[0] < len(items) and items[pos[0]][0] > indent:
                        m[k.strip()] = block(items[pos[0]][0])
                    else:
                        m[k.strip()] = None
                while (pos[0] < len(items) and items[pos[0]][0] > indent
                       and not items[pos[0]][1].startswith('- ')):
                    sub = block(items[pos[0]][0])
                    if isinstance(sub, dict):
                        m.update(sub)
                out.append(m)
            return out
        m = {}
        while (pos[0] < len(items) and items[pos[0]][0] == indent
               and not items[pos[0]][1].startswith('- ')):
            content = items[pos[0]][1]
            pos[0] += 1
            k, _, v = content.partition(':')
            if v.strip():
                m[k.strip()] = _scalar(v)
            elif pos[0] < len(items) and items[pos[0]][0] > indent:
                m[k.strip()] = block(items[pos[0]][0])
            else:
                m[k.strip()] = None
        return m

    doc = block(items[0][0])
    if pos[0] != len(items):
        raise ValueError(f"无法归位的行: {items[pos[0]][1][:60]!r}（子集语法越界）")
    return doc


# ---- 工具 ----

def sha256_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 16), b''):
            h.update(chunk)
    return h.hexdigest()


def run(cmd, cwd):
    r = subprocess.run(cmd, shell=True, cwd=cwd,
                       capture_output=True, text=True)
    return r.returncode, r.stdout.strip(), r.stderr.strip()


def age_days(ts):
    return (datetime.datetime.now().timestamp() - ts) / 86400.0


# ---- check 八类 ----

def do_check(chk, root, log):
    kind = chk.get('kind')
    try:
        if kind == 'file_exists':
            p = os.path.join(root, chk['path'])
            ok = os.path.isfile(p)
            log.append(f"file_exists {chk['path']} -> {'在' if ok else '缺'}")
            return ok
        if kind in ('grep_present', 'grep_zero'):
            p = os.path.join(root, chk['path'])
            if not os.path.isfile(p):
                log.append(f"{kind}: 文件缺席 {chk['path']}")
                return False
            text = open(p, encoding='utf-8', errors='replace').read()
            hit = re.search(chk['pattern'], text, re.M) is not None
            ok = hit if kind == 'grep_present' else not hit
            log.append(f"{kind} /{chk['pattern']}/ @{chk['path']} -> "
                       f"{'命中' if hit else '零命中'}（期望{'命中' if kind == 'grep_present' else '零命中'}）")
            return ok
        if kind == 'crate_enum':
            crates_dir = os.path.join(root, 'crates')
            if not os.path.isdir(crates_dir):
                log.append("crate_enum: crates/ 目录缺席")
                return False
            have = set(os.listdir(crates_dir))
            miss = [c for c in chk.get('present', []) if c not in have]
            leak = [c for c in chk.get('absent', []) if c in have]
            log.append(f"crate_enum present={chk.get('present', [])} absent={chk.get('absent', [])}"
                       f" -> 缺{miss} 漏在{leak}" if (miss or leak) else
                       f"crate_enum OK（present 全在、absent 全无）")
            return not miss and not leak
        if kind == 'adapter_enum':
            p = os.path.join(root, chk['path'])
            text = open(p, encoding='utf-8', errors='replace').read()
            got = sorted(set(re.findall(chk['arms_pattern'], text)))
            want = sorted(chk['expect'])
            log.append(f"adapter_enum @{chk['path']} 实得={got} 期望={want}")
            return got == want
        if kind == 'cli_enum':
            rc, out, err = run(chk['command'], root)
            got = sorted(l.strip() for l in out.splitlines() if l.strip())
            want = sorted(chk['expect_lines'])
            log.append(f"cli_enum rc={rc} 实得={got} 期望={want}")
            return rc == 0 and got == want
        if kind == 'test_name':
            name = chk['name']
            rc, _, _ = run(f"git grep -l -F {name!r} -- crates ci/run.sh", root)
            if rc != 0:  # 非 git 根回退全量 grep
                rc2, _, _ = run(f"grep -rIl -F {name!r} {os.path.join(root, 'crates')}"
                                f" {os.path.join(root, 'ci', 'run.sh')}", '/')
                rc = 0 if rc2 == 0 else 1
            log.append(f"test_name {name} -> {'在测试面' if rc == 0 else '不在测试面'}")
            return rc == 0
        if kind == 'adr_pointer':
            num = re.sub(r'[^0-9]', '', chk['adr'])
            hits = [f for f in os.listdir(os.path.join(root, 'docs', 'adr'))
                    if f.startswith(num + '-')]
            log.append(f"adr_pointer {chk['adr']} -> {hits or '无在册文件'}")
            return bool(hits)
        log.append(f"未知 check.kind: {kind!r}")
        return False
    except (KeyError, OSError, re.error) as e:
        log.append(f"check 执行异常: {e}")
        return False


# ---- 主流程 ----

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--file', required=True, help='deliverables.yaml 路径')
    ap.add_argument('--root', required=True, help='路径解析根（af 仓根或工作区根）')
    ap.add_argument('--window-days', type=int, default=None,
                    help='覆盖文件内 window_days')
    ap.add_argument('--closeout', action='store_true',
                    help='收口模式：任何 open 即红（常规模式容忍未过期 open）')
    args = ap.parse_args()

    if not os.path.isfile(args.file):
        print(f"step10 FAIL(infra): deliverables.yaml 缺席：{args.file}", file=sys.stderr)
        return 2
    try:
        doc = parse_simple_yaml(args.file)
    except (ValueError, OSError) as e:
        print(f"step10 FAIL(infra): 解析失败：{e}", file=sys.stderr)
        return 2
    if not isinstance(doc, dict) or 'items' not in doc or 'sources' not in doc:
        print("step10 FAIL(infra): 顶层必须含 sources/items", file=sys.stderr)
        return 2

    window_days = args.window_days or doc.get('window_days', 7)
    sources = {s.get('id'): s for s in doc['sources']}
    items = doc['items']
    red = []

    def bad(msg):
        red.append(msg)
        print(f"RED {msg}")

    # ① 承诺正本计数对账
    for sid, s in sources.items():
        n_items = sum(1 for it in items if it.get('source') == sid)
        spath = os.path.join(args.root, s.get('path', ''))
        if not os.path.isfile(spath):
            bad(f"[{sid}] 承诺正本未归档：{s.get('path')}"
                f"（复盘:101 ①——原文未归档则对账无正本可依）")
            continue
        if s.get('count_pending'):
            print(f"WARN [{sid}] count_pending：正本已归档但计数规则未定稿"
                  f"（expect_count={s.get('expect_count')}，登记 {n_items} 条）——"
                  f"统稿定稿前不红（裁决点，见 SPEC §⑧）")
            continue
        rc, out, err = run(s['count_command'], s.get('cwd', args.root))
        try:
            n_src = int(out.strip().splitlines()[-1])
        except (ValueError, IndexError):
            bad(f"[{sid}] count_command 不可机数（rc={rc}, out={out!r}, err={err!r}）")
            continue
        if n_src != s.get('expect_count') or n_src != n_items:
            bad(f"[{sid}] 计数不对账：正本={n_src} 登记期望={s.get('expect_count')}"
                f" 清单条目={n_items}——散文枚举与机读清单漂移")
        else:
            print(f"OK  [{sid}] 正本计数 {n_src} == 清单条目 {n_items}")

    seen_ids = set()
    stat = {'open': 0, 'done': 0, 'descoped': 0}
    for it in items:
        iid = it.get('id', '?')
        if iid in seen_ids:
            bad(f"[{iid}] id 重复")
        seen_ids.add(iid)
        status = it.get('status')
        if status not in stat:
            bad(f"[{iid}] 非法 status={status!r}（open|descoped|done）")
            continue
        stat[status] += 1
        if it.get('source') not in sources:
            bad(f"[{iid}] source 未登记：{it.get('source')!r}")
        if not it.get('owner'):
            bad(f"[{iid}] 缺 owner（反空过：无属主的承诺不可追踪）")

        if status == 'open':
            due = it.get('due')
            if args.closeout:
                bad(f"[{iid}] closeout 模式：open 不得在场（title={it.get('title')!r}）")
            elif not due:
                bad(f"[{iid}] open 项缺 due（无到期日的 open=永久绿灯缺口）")
            else:
                try:
                    d = datetime.date.fromisoformat(str(due))
                except ValueError:
                    bad(f"[{iid}] due 非法：{due!r}")
                else:
                    if datetime.date.today() > d:
                        bad(f"[{iid}] open 已过期（due={due}）——承诺未交付且未 descope")
                    else:
                        print(f"OK  [{iid}] open 登记完整（due={due} 未过期）")
        elif status == 'descoped':
            adr = it.get('descope_adr')
            if not adr:
                bad(f"[{iid}] descoped 缺 descope_adr（无 descope 记录的缺口直接红，复盘:102）")
            else:
                num = re.sub(r'[^0-9]', '', str(adr))
                hits = [f for f in os.listdir(os.path.join(args.root, 'docs', 'adr'))
                        if f.startswith(num + '-')] if os.path.isdir(
                            os.path.join(args.root, 'docs', 'adr')) else []
                if hits:
                    print(f"OK  [{iid}] descoped -> {adr} 解析到 {hits[0]}")
                else:
                    bad(f"[{iid}] descope_adr {adr} 无法解析到在册 ADR")
        elif status == 'done':
            ev = it.get('evidence_path')
            if not ev:
                bad(f"[{iid}] done 缺 evidence_path")
                continue
            p = os.path.join(args.root, ev)
            if not os.path.isfile(p):
                bad(f"[{iid}] evidence 缺席：{ev}")
                continue
            real = sha256_file(p)
            if real != it.get('evidence_sha256'):
                bad(f"[{iid}] sha256 不符：登记={str(it.get('evidence_sha256'))[:16]}…"
                    f" 实算={real[:16]}…")
            wk = it.get('window_kind', 'mtime')
            if wk == 'git':
                rc, out, _ = run(f"git log -1 --format=%ct -- {ev!r}", args.root)
                if rc != 0 or not out.strip():
                    bad(f"[{iid}] git 窗口：{ev} 无提交记录（未提交即登记 done）")
                else:
                    a = age_days(int(out.strip()))
                    if a > window_days:
                        bad(f"[{iid}] git 窗口过期：最近提交 {a:.1f}d > {window_days}d")
                    else:
                        print(f"OK  [{iid}] done evidence/sha256/git 窗口 {a:.1f}d ≤ {window_days}d")
            else:
                a = age_days(os.path.getmtime(p))
                if a > window_days:
                    bad(f"[{iid}] mtime 窗口过期：{a:.1f}d > {window_days}d")
                else:
                    print(f"OK  [{iid}] done evidence/sha256/mtime 窗口 {a:.1f}d ≤ {window_days}d")
        # ② check（声明即跑；open 项通常不声明）
        chk = it.get('check')
        if chk:
            log = []
            if do_check(chk, args.root, log):
                print(f"OK  [{iid}] check {chk.get('kind')} 通过")
            else:
                bad(f"[{iid}] check 失败：{'; '.join(log)}")

    print(f"step10 承诺-交付对账: items={len(items)} "
          f"done={stat['done']} open={stat['open']} descoped={stat['descoped']} "
          f"红={len(red)}")
    if red:
        print("step10 FAIL: 承诺-交付不对账（逐条 RED 见上）")
        return 1
    print("step10 PASS: 承诺-交付对账闭合")
    return 0


if __name__ == '__main__':
    sys.exit(main())
