#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""step12 计数复算门（红队复盘 C2:117-118/C3/PP4:292；开发计划 §2.1 表行 3）。

对 numbers.json 做三件事：
  ① 限定名键校验：每键必须 <域>.<名> 形态（裸键无法消歧——六门 vs 九门史的根治；
     域路径允许多段如 fv.kani_inv_harness.count——2026-10-28 合流批按总对账 §三.3
     终稿键名放宽 KEY_RE，裁决详见 numbers.json _note）；
  ② 逐键跑来源命令复算：登记值为 int 时 stdout 须整数可解析且相等；为 str 时
     （如 manifest sha256 十六进制串）stdout 逐字符比对（大小写不敏感）——漂移即红；
  ③ 文书扫描：
     a. {{numbers.<key>}} 占位符必须解析到在册键；
     b. watch 手写计数对值：正则命中已知度量名（含汉字数词映射）即与 numbers.json
        对值，不符即红；skip_pattern 命中处豁免（勘误/历史行——AGENT.md:27/87 实测：
        同一行既含现行「九门」又含勘误注记「六门」，须窗口级豁免而非行级）。
  ④ step14 子门名册绑定（红队终局轮 RT-E81 修复批，2026-11-02）：子门全集以单点
     名册 ci/subgates.list 为唯一正本（钉键 source 由名册生成）——断言
     「名册计数 == 正则计数 == 名册逐件在场数」三者一致，且名册集合 == ci/run.sh
     step14 槽实际接线集合（双向：名册外多件/名册内缺件均红），且名册每件的接线行
     不带弱化标志（--skip-compile/--text-mode）。修前钉键是手写正则枚举，「同名替换」
     攻击（14d 的 ci/strategies_check.py 调用行换成 ci/xcheck.py）复算值不变、全链照绿。
退出码：0=全过；1=计数红；2=基础设施缺件。
"""
import argparse
import datetime
import json
import os
import re
import subprocess
import sys

KEY_RE = re.compile(r'^[a-z0-9_]+(\.[a-z0-9_]+)+$')
PLACEHOLDER_RE = re.compile(r'\{\{numbers\.([A-Za-z0-9_.]+)\}\}')

# ---- ④ step14 子门名册绑定（RT-E81 修复批）常量 ----
# 名册正本（相对仓根）：每行一个子检查脚本名（不含 .py），# 起始为注释行。
ROSTER_REL = os.path.join('ci', 'subgates.list')
# 名册条目字面口径（与 numbers.json gates.ci_step14_subgates_roster 的 source
# 「grep -cE '^[A-Za-z0-9_]+[[:space:]]*$'」逐字一致——两边口径不同即红）。
ROSTER_ENTRY_RE = re.compile(r'^[A-Za-z0-9_]+$')
# step14 槽接线抽取：`(cd "$AF_ROOT" && python3 [-B] ci/<name>.py …)` 形态。
# 该形态在 ci/run.sh 中**仅**出现在 step14 machine_batch 块（step10/11/12 走
# `step N "…" -- bash -c "cd '…' && python3 …"` 续行形态，不命中本式）——故本式
# 即「step14 子门实际接线集合」的全量动态枚举（不依赖手写子门名单）。
SUBGATE_WIRE_RE = re.compile(
    r'^[ \t]*\(?cd "\$AF_ROOT" && python3(?:\s+-\S+)*\s+ci/(?P<name>[A-Za-z0-9_]+)\.py')
# 接线行弱化标志（出现即红）：--skip-compile=关掉 strategies_check 的 R4 af-dsl
# 编译断言面；--text-mode=门判据降级为文本模式（yaml 库缺席时的宽松路径）。
WEAKEN_FLAGS = ('--skip-compile', '--text-mode')


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
    recomputed = {}          # 键 -> 本次复算值（int 键；第④节名册三臂对账消费）

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
        want = ent['value']
        if isinstance(want, str):
            # 字符串登记值（G0-B 补合流放宽面：fv.kani_inv_harness.manifest_sha256
            # 等十六进制串）——stdout 逐字符比对，大小写不敏感（十六进制大小写等价，
            # 位数与数字仍逐位核）。放宽只增可表达面，「复算==登记」纪律不变。
            lines = out.splitlines()
            got = lines[-1] if lines else ''
            same = got.lower() == want.lower()
        else:
            try:
                got = int(out.splitlines()[-1])
            except (ValueError, IndexError):
                bad(f"[{key}] 来源命令不可机数（rc={r.returncode}, out={out!r}）")
                continue
            recomputed[key] = got
            same = got == want
        if not same:
            bad(f"[{key}] 复算值漂移：登记={want} 复算={got}（{cmd}）"
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

    # ④ step14 子门名册绑定（红队终局轮 RT-E81 修复批，2026-11-02）
    #   钉键三臂：gates.ci_step14_subgates_roster（名册计数，source 由名册生成）/
    #   gates.ci_step14_subgates（名册逐件在场数，source 由名册生成）/
    #   gates.ci_step14_subgates_regex（正则计数，手写枚举对照臂）——三者必须一致；
    #   且名册集合 == ci/run.sh step14 槽实际接线集合（双向）；且接线行不带弱化标志。
    #   fail-closed：名册/接线对照面缺席一律红（防护的对照面缺席=防护失效，不得当绿）。
    roster_path = os.path.join(args.repo, ROSTER_REL)
    run_sh_path = os.path.join(args.repo, 'ci', 'run.sh')
    roster = []
    roster_err = []
    if not os.path.isfile(roster_path):
        roster_err.append(f"名册缺席：{ROSTER_REL}——子门全集无正本可比对，fail-closed 红")
    else:
        for ln, raw in enumerate(
                open(roster_path, encoding='utf-8', errors='replace'), 1):
            s = raw.strip()
            if not s or s.startswith('#'):
                continue
            if not ROSTER_ENTRY_RE.match(s):
                roster_err.append(f"{ROSTER_REL}:{ln} 名册条目非法：{s!r}"
                                  f"（只允许 [A-Za-z0-9_]+ 子门名）")
                continue
            roster.append(s)
        if not roster and not roster_err:
            roster_err.append(f"名册为空：{ROSTER_REL}——子门全集无正本可比对，fail-closed 红")
        dup = sorted({n for n in roster if roster.count(n) > 1})
        if dup:
            roster_err.append(f"名册重复条目：{dup}——重名会稀释逐件在场判定")
    for m in roster_err:
        bad(f"[ci/subgates.list] {m}")

    # 名册逐件在场：名册每件在 ci/run.sh 有非注释接线行（评论行不算接线——修前 run.sh
    # 头注里残留 `ci/strategies_check.py` 字样，naive grep 会把「只剩注释」误判为在场）。
    wired = {}          # 子门名 -> [(行号, 行文本)]
    if not os.path.isfile(run_sh_path):
        bad(f"[ci/subgates.list] ci/run.sh 缺席：{run_sh_path}——名册逐件在场无对照面，"
            f"fail-closed 红")
    else:
        for ln, line in enumerate(
                open(run_sh_path, encoding='utf-8', errors='replace'), 1):
            if line.lstrip().startswith('#'):
                continue
            m = SUBGATE_WIRE_RE.match(line)
            if m:
                wired.setdefault(m.group('name'), []).append((ln, line))
        if not wired:
            bad("[ci/subgates.list] ci/run.sh 中零条 step14 子门接线行——接线抽取口径失效，"
                "fail-closed 红（对照面缺席=防护失效）")
        for name in roster:
            if name not in wired:
                bad(f"[ci/subgates.list] 名册件缺席：{name}（ci/run.sh 无其非注释接线行）"
                    f"——摘门/改名形态（策略库 14d 门同型：调用行被替换或删除即红）")
            for ln, line in wired.get(name, []):
                for flag in WEAKEN_FLAGS:
                    if flag in line:
                        bad(f"[ci/subgates.list] 名册件 {name} 的接线行带弱化标志 {flag}"
                            f"（ci/run.sh:{ln}）——弱化即静默关门，禁（"
                            f"--skip-compile 关 R4 af-dsl 编译断言面；--text-mode 判据降级）")
        extra = sorted(set(wired) - set(roster))
        if extra:
            bad(f"[ci/subgates.list] 名册外多件：{extra}（ci/run.sh step14 槽接线了名册未登记的"
                f"子检查）——子门全集漏造册，须同批补名册+钉键+钉文件")

    # 三者一致：名册计数 == 正则计数 == 名册逐件在场数（复算值口径，非登记值口径——
    # 登记值一律由上面逐键复算段核对过，此处对账的是三臂彼此相等）。
    arms = (('gates.ci_step14_subgates_roster', '名册计数'),
            ('gates.ci_step14_subgates_regex', '正则计数'),
            ('gates.ci_step14_subgates', '名册逐件在场数'))
    arm_vals = []
    for key, label in arms:
        if key not in numbers:
            bad(f"[ci/subgates.list] 钉键缺席：{key}（{label}）——三臂对账缺臂，fail-closed 红")
            arm_vals.append(None)
            continue
        v = recomputed.get(key)
        if v is None:
            bad(f"[ci/subgates.list] 钉键 {key}（{label}）本轮无可用复算值——三臂对账缺臂，"
                f"fail-closed 红")
            arm_vals.append(None)
            continue
        arm_vals.append(v)
    if all(v is not None for v in arm_vals):
        if len(set(arm_vals)) != 1:
            bad(f"[ci/subgates.list] 三者不一致：名册计数={arm_vals[0]} 正则计数={arm_vals[1]} "
                f"名册逐件在场数={arm_vals[2]}——名册/正则枚举/逐件在场三者任一不等即红")
        else:
            print(f"OK  [ci/subgates.list] 三者一致：名册计数==正则计数==名册逐件在场数"
                  f"=={arm_vals[0]}")
    if roster and set(wired) == set(roster):
        print(f"OK  [ci/subgates.list] 名册集合 == ci/run.sh step14 槽接线集合："
              f"{sorted(roster)}")
    elif roster:
        bad(f"[ci/subgates.list] 名册集合 != ci/run.sh step14 槽接线集合："
            f"名册={sorted(set(roster))} 接线={sorted(set(wired))}——双向差集即红")

    print(f"step12 计数复算: keys={len(numbers)} watch={len(doc.get('watch', []))} "
          f"docs={len(args.docs)} 红={len(red)}")
    if red:
        print("step12 FAIL: 计数口径漂移（逐条 RED 见上）")
        return 1
    print("step12 PASS: 全部限定名键复算一致、占位符可解析、手写计数对值、"
          "step14 子门名册绑定三者一致")
    return 0


if __name__ == '__main__':
    sys.exit(main())
