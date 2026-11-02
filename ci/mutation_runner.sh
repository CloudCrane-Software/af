#!/usr/bin/env bash
# =============================================================================
# ci/mutation_runner.sh — mutations/ 元探针 runner（G0-CI机制批 子代理B 草案）
#
# 依据（复盘 §三 C3-②「升级修订」/ PP1 升级/ §五 5.1）：
#   mutation 套件落 mutations/ 目录：每探针一个用例、预登记期望 exit code +
#   期望 stderr 子串，通过=全探针实跑匹配、机械判定无裁量；元探针两件套
#   （删门脚本本身、删任一探针各一用例，CI 必须红）；探针清单（数量+期望值）
#   哈希钉基线，探针被删即计数漂移红；触发规则=diff 触及 ci//proofs//门输入
#   全量跑、否则 nightly（kani 探针单跑 timeout 100s，每次 CI 全跑不可支付）。
#
# 注册表 mutations/registry.yaml 采用 JSON 子集（合法 YAML），python3 标准库
# 可解析（零第三方依赖；PyYAML 在场则优先用）。
#
# 契约：
#   mutation_runner.sh check-registry  [--root DIR]
#       注册表完整性：sha256+探针数 vs ci/baselines/mutations-registry.sha256。
#       基线缺席=红（fail-closed：防护关掉本身就是红）。
#   mutation_runner.sh update-registry-baseline [--root DIR]
#       有意变更注册表后同批重钉基线（防悄悄改基线=逃逸；走评审 diff）。
#   mutation_runner.sh run [--root DIR] [--probe ID]... [--mode full|nightly]
#                         [--scratch-dir DIR] [--keep]
#       逐探针演练：整仓复制到 scratch（排除 .git/target/ci/out，硬链接优先），
#       施加变异（删文件/删注册表条目/按正则删 run.sh 执行调用行/按正则替换 run.sh 执行
#       调用行），跑探针声明的
#       门命令（mirror_of stepN 时先对账 run.sh 命令未漂移），比对期望 exit/stderr，
#       删 scratch。绝不动真树。
#   mutation_runner.sh should-run [--root DIR] [--base REF]
#       触发规则判定：diff（--base，缺省 HEAD~1）触及 ci//proofs//gate_inputs
#       glob → 输出 full；否则 nightly-only。非 git 环境 → full（fail-safe）。
#   mutation_runner.sh list [--root DIR]
# 退出码: 0 全过/该跑 full/列毕；1 探针红或注册表漂移；2 用法/环境错误。
# =============================================================================
set -euo pipefail

PROG=mutation_runner
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() { sed -n '2,27p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }

CMD=${1:-}
case "$CMD" in
  check-registry|update-registry-baseline|run|should-run|list) shift ;;
  *) usage ;;
esac

ROOT='' MODE=full BASE='' SCRATCH_PARENT='' KEEP=0
declare -a ONLY_PROBES=()
while [ $# -gt 0 ]; do
  case "$1" in
    --root) ROOT=${2:?}; shift 2 ;;
    --mode) MODE=${2:?}; shift 2 ;;
    --base) BASE=${2:?}; shift 2 ;;
    --probe) ONLY_PROBES+=("${2:?}"); shift 2 ;;
    --scratch-dir) SCRATCH_PARENT=${2:?}; shift 2 ;;
    --keep) KEEP=1; shift ;;
    *) echo "$PROG: 未知参数: $1" >&2; usage ;;
  esac
done
[ -n "$ROOT" ] || ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REG="$ROOT/mutations/registry.yaml"
BASELINE="$ROOT/ci/baselines/mutations-registry.sha256"
case "$MODE" in full|nightly) ;; *) echo "$PROG: --mode 须为 full|nightly" >&2; exit 2 ;; esac

# ---------- 注册表解析（JSON 子集 YAML；PyYAML 可用则用） ----------

reg_json() { # 整表 → JSON
  python3 -c '
import json, sys
try:
    import yaml
    data = yaml.safe_load(open(sys.argv[1], encoding="utf-8"))
except ImportError:
    data = json.load(open(sys.argv[1], encoding="utf-8"))
json.dump(data, sys.stdout, ensure_ascii=False)
' "$REG"
}

probe_tsv() { # 探针行（ASCII 单元分隔符 \x1f 连接——tab 属 IFS 空白类会吞空字段）:
  # id \x1f trigger \x1f action \x1f target \x1f remove_id \x1f run \x1f timeout \x1f exit \x1f stderr_contains \x1f mirror_of \x1f args_json
  reg_json | python3 -c '
import json, sys
reg = json.load(sys.stdin)
for p in reg.get("probes", []):
    e = p.get("expect", {})
    cols = [p.get("id",""), p.get("trigger","on-diff"), p.get("action",""),
            p.get("target",""), (p.get("args") or {}).get("remove_id",""),
            p.get("run",""), str(p.get("timeout_sec",120)),
            str(e.get("exit","")), e.get("stderr_contains",""), p.get("mirror_of",""),
            json.dumps(p.get("args") or {}, ensure_ascii=False)]
    print("\x1f".join(c.replace("\x1f"," ") for c in cols))
'
}

gate_input_globs() { reg_json | python3 -c 'import json,sys; [print(g) for g in json.load(sys.stdin).get("gate_inputs",[])]'; }

# ---------- check-registry / update-registry-baseline ----------

reg_sha() { sha256sum "$REG" | cut -d" " -f1; }
reg_count() { probe_tsv | wc -l | tr -d " "; }

do_check_registry() {
  if [ ! -f "$REG" ]; then
    echo "$PROG: 探针清单漂移：注册表缺席（$REG）" >&2; return 1
  fi
  if [ ! -f "$BASELINE" ]; then
    echo "$PROG: 探针清单漂移：注册表基线缺席（$BASELINE）——防护未钉，fail-closed 红" >&2; return 1
  fi
  local want_sha want_count got_sha got_count
  want_sha=$(awk 'NR==1{print $1}' "$BASELINE")
  want_count=$(awk 'NR==1{print $2}' "$BASELINE")
  got_sha=$(reg_sha); got_count=$(reg_count)
  if [ "$got_sha" != "$want_sha" ] || [ "$got_count" != "$want_count" ]; then
    echo "$PROG: 探针清单漂移：期望 ${want_count} 条/哈希 ${want_sha:0:12}…，实得 ${got_count} 条/哈希 ${got_sha:0:12}…——探针被删/改须走 update-registry-baseline 同批评审" >&2
    return 1
  fi
  echo "$PROG: 注册表完整: ${got_count} 探针, sha256=${got_sha:0:12}…"
}

do_update_baseline() {
  mkdir -p "$(dirname "$BASELINE")"
  printf '%s  %s  mutations/registry.yaml\n' "$(reg_sha)" "$(reg_count)" > "$BASELINE"
  echo "$PROG: 基线已重钉: $(cat "$BASELINE")（须与注册表改动同批提交）"
}

# ---------- run ----------

make_scratch() { # $1 = 目标目录
  mkdir -p "$1"
  if ! cp -al "$ROOT/." "$1/repo" 2>/dev/null; then   # 同设备硬链接（删=安全 unlink）
    tar -C "$ROOT" --exclude=.git --exclude=target --exclude=ci/out -cf - . |
      tar -C "$1/repo" -xf -
  else
    rm -rf "$1/repo/.git" "$1/repo/target" "$1/repo/ci/out" 2>/dev/null || true
  fi
}

apply_mutation() { # $1=scratch/repo $2..=探针字段（末位=args_json，edit-drop-lines 用）
  local repo=$1 action=$2 target=$3 remove_id=$4 args_json=${10}
  case "$action" in
    delete)
      rm -f "$repo/$target" || { echo "$PROG: 变异失败: rm $target" >&2; return 2; }
      echo "$PROG: 已施加变异: delete $target" >&2 ;;
    edit-drop-lines)
      # 按正则逐行删除目标文件中的执行调用行（红队 2026-10-31「摘门保牌」回归面）：
      # fail-closed——drop_patterns 任一条零命中即报错（探针与正本漂移必须显式红，
      # 不静默放过；同 xcheck_probe.sh 的 sed 自检纪律）。新 inode 落盘，不惊动硬链接正本。
      python3 - "$repo/$target" "$args_json" <<'PYEOF' \
        || { echo "$PROG: 变异施加失败: edit-drop-lines $target" >&2; return 2; }
import json, os, re, sys
path = sys.argv[1]
pats = [re.compile(p) for p in json.loads(sys.argv[2]).get("drop_patterns", [])]
if not pats:
    print("edit-drop-lines: drop_patterns 缺席", file=sys.stderr); sys.exit(2)
with open(path, encoding="utf-8") as f:
    lines = f.readlines()
kept, hits = [], [0] * len(pats)
for ln in lines:
    hit = any(p.search(ln) for p in pats)
    for i, p in enumerate(pats):
        if p.search(ln):
            hits[i] += 1
    if not hit:
        kept.append(ln)
dropped = len(lines) - len(kept)
if min(hits) < 1:
    miss = [p.pattern for p, h in zip(pats, hits) if h == 0]
    print(f"edit-drop-lines: drop_patterns 零命中 {miss}（正本漂移，探针源失效）", file=sys.stderr)
    sys.exit(2)
tmp = path + ".tmp"
with open(tmp, "w", encoding="utf-8") as f:
    f.writelines(kept)
os.replace(tmp, path)
print(f"edit-drop-lines: 删除 {dropped} 行（每条 pattern 命中 {hits}）", file=sys.stderr)
PYEOF
      echo "$PROG: 已施加变异: edit-drop-lines $target" >&2 ;;
    edit-replace-lines)
      # 按正则逐行**替换**目标文件中的执行调用行（红队终局轮 RT-E81 修复批 2026-11-02
      # 「同名替换」回归面）：把 14d 的 ci/strategies_check.py 调用行换成 ci/xcheck.py
      # ——子门数不变、钉键手写正则枚举仍数到 4，摘门静默（修前 step12/xcheck 全绿）。
      # fail-closed——replace_patterns 任一条零命中即报错（探针与正本漂移必须显式红，
      # 不静默放过；承 edit-drop-lines 同款纪律）。新 inode 落盘，不惊动硬链接正本。
      python3 - "$repo/$target" "$args_json" <<'PYEOF' \
        || { echo "$PROG: 变异施加失败: edit-replace-lines $target" >&2; return 2; }
import json, os, re, sys
path = sys.argv[1]
spec = json.loads(sys.argv[2]).get("replace_patterns", [])
if not spec:
    print("edit-replace-lines: replace_patterns 缺席", file=sys.stderr); sys.exit(2)
rx = [(re.compile(s["pattern"]), s["replace"]) for s in spec]
with open(path, encoding="utf-8") as f:
    lines = f.readlines()
hits = [0] * len(rx)
out = []
for ln in lines:
    for i, (r, repl) in enumerate(rx):
        if r.search(ln):
            hits[i] += 1
            ln = r.sub(lambda _m, _r=repl: _r, ln)
    out.append(ln)
if min(hits) < 1:
    miss = [s["pattern"] for s, h in zip(spec, hits) if h == 0]
    print(f"edit-replace-lines: replace_patterns 零命中 {miss}（正本漂移，探针源失效）", file=sys.stderr)
    sys.exit(2)
tmp = path + ".tmp"
with open(tmp, "w", encoding="utf-8") as f:
    f.writelines(out)
os.replace(tmp, path)
print(f"edit-replace-lines: 替换 {sum(hits)} 行（每条 pattern 命中 {hits}）", file=sys.stderr)
PYEOF
      echo "$PROG: 已施加变异: edit-replace-lines $target" >&2 ;;
    registry-remove-probe)
      # 极小修正（2026-10-27 补完代理）：原版 python3 失败（如 remove_id 不在注册表
      # 的 AssertionError）被随后的 echo 吞掉退出码、误报「已施加变异」——afmut-bPuyoN
      # 彩排 mutate.log 亲证。显式接力：python3 非零 → return 2（run_probe 判环境错）。
      python3 - "$repo/mutations/registry.yaml" "$remove_id" <<'PYEOF' \
        || { echo "$PROG: 变异施加失败: registry-remove-probe $remove_id" >&2; return 2; }
import json, os, sys
path, rid = sys.argv[1], sys.argv[2]
try:
    import yaml; data = yaml.safe_load(open(path, encoding="utf-8"))
except ImportError:
    data = json.load(open(path, encoding="utf-8"))
before = len(data.get("probes", []))
data["probes"] = [p for p in data.get("probes", []) if p.get("id") != rid]
assert len(data["probes"]) == before - 1, "remove_id 不在注册表: " + rid
tmp = path + ".tmp"
with open(tmp, "w", encoding="utf-8") as f:
    json.dump(data, f, ensure_ascii=False, indent=1); f.write("\n")
os.replace(tmp, path)   # 新 inode，不惊动硬链接正本
PYEOF
      echo "$PROG: 已施加变异: registry-remove-probe $remove_id" >&2 ;;
    *) echo "$PROG: 未知动作: $action" >&2; return 2 ;;
  esac
}

run_probe() { # 参数 11 个字段（末位 args_json，仅 edit-drop-lines 消费）；0=过 1=红 2=环境错
  local id=$1 trigger=$2 action=$3 target=$4 remove_id=$5 run=$6 timeout=$7 \
        xexit=$8 xstderr=$9 mirror_of=${10} args_json=${11}
  # 触发过滤：full 模式跳过 nightly-only；nightly 模式全跑
  if [ "$MODE" = full ] && [ "$trigger" = nightly-only ]; then
    echo "  [probe $id] SKIP（nightly-only，本次 full 模式）"; return 0
  fi
  if [ ${#ONLY_PROBES[@]} -gt 0 ]; then
    local hit=0 p; for p in "${ONLY_PROBES[@]}"; do [ "$p" = "$id" ] && hit=1; done
    [ $hit -eq 1 ] || { echo "  [probe $id] SKIP（未点名）"; return 0; }
  fi
  # mirror 对账：探针命令须仍是 run.sh 对应 step 的门命令子串（防探针与门漂移）
  if [ -n "$mirror_of" ] && [ -f "$ROOT/ci/run.sh" ]; then
    if ! grep -qF "$run" "$ROOT/ci/run.sh"; then
      echo "  [probe $id] FAIL: 探针命令与 run.sh $mirror_of 漂移（run.sh 内不再包含: $run）——先修探针镜像" >&2
      return 1
    fi
  fi
  # 极小修正（2026-10-27 主代理补完）：scratch 绝对化——mktemp 得到的相对路径在
  # 下方 `cd $scratch/repo` 后重定向 $scratch/stdout.log 会解析失败（复现证据=
  # .rehearsal/r2-firstfail.log：带本 bug 的副本跑同注册表，两探针被误判红
  # 「没有那个文件或目录」；首轮现场 scratch 已被 runner 自清，故用可复现副本
  # 留证）。absolutize 后复跑绿（.rehearsal/r2-run.log）。
  local scratch; scratch=$(mktemp -d "${SCRATCH_PARENT:-${TMPDIR:-/tmp}}/afmut-XXXXXX")
  scratch=$(cd "$scratch" && pwd)
  local rc=0 got_rc=0
  make_scratch "$scratch" || { rm -rf "$scratch"; echo "  [probe $id] FAIL: scratch 复制失败" >&2; return 2; }
  if ! apply_mutation "$scratch/repo" "$action" "$target" "$remove_id" "" "" "" "" "" "$args_json" 2>>"$scratch/mutate.log"; then
    rm -rf "$scratch"; echo "  [probe $id] FAIL: 变异施加失败（见 $scratch）" >&2; return 2
  fi
  if ( cd "$scratch/repo" && timeout "${timeout:-120}" bash -c "$run" \
       > "$scratch/stdout.log" 2> "$scratch/stderr.log" ); then
    got_rc=0
  else
    got_rc=$?
  fi
  echo "  [probe $id] run exit=$got_rc（期望 ${xexit}）"
  if [ "$xexit" = nonzero ]; then
    if [ "$got_rc" -eq 0 ]; then
      echo "  [probe $id] FAIL: 期望非零退出，实得 0——门未红" >&2; rc=1
    fi
  elif [ -n "$xexit" ] && [ "$got_rc" != "$xexit" ]; then
    echo "  [probe $id] FAIL: 期望 exit=$xexit，实得 $got_rc" >&2; rc=1
  fi
  if [ -n "$xstderr" ]; then
    if grep -qF -- "$xstderr" "$scratch/stderr.log"; then
      echo "  [probe $id] stderr 子串命中: $xstderr"
    else
      echo "  [probe $id] FAIL: stderr 缺期望子串「$xstderr」；实得 stderr 尾:" >&2
      tail -5 "$scratch/stderr.log" 2>/dev/null | sed 's/^/      | /' >&2 || true
      rc=1
    fi
  fi
  if [ "$KEEP" = 1 ]; then
    echo "  [probe $id] scratch 保留: $scratch" >&2
  else
    rm -rf "$scratch"
  fi
  [ $rc -eq 0 ] && echo "  [probe $id] PASS"
  return $rc
}

do_run() {
  do_check_registry || return 1
  local overall=0
  while IFS=$'\037' read -r id trigger action target remove_id run timeout xexit xstderr mirror_of args_json; do
    [ -n "$id" ] || continue
    run_probe "$id" "$trigger" "$action" "$target" "$remove_id" "$run" "$timeout" "$xexit" "$xstderr" "$mirror_of" "$args_json" || overall=1
  done < <(probe_tsv)
  if [ $overall -eq 0 ]; then
    echo "$PROG: 全探针 PASS（模式=$MODE）"
  else
    echo "$PROG: 存在红探针——被测门未失败于删除/弱化=门失效或探针漂移（复盘 C3：mutation 不过不得合流）" >&2
  fi
  return $overall
}

# ---------- should-run ----------

do_should_run() {
  local base=${BASE:-HEAD~1}
  local globs_json='["ci/**","proofs/**"]'
  if [ -f "$REG" ]; then
    globs_json=$(reg_json | python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin).get("gate_inputs") or ["ci/**","proofs/**"]))')
  fi
  if ! git -C "$ROOT" rev-parse --git-dir >/dev/null 2>&1; then
    echo "full"; echo "$PROG: 非 git 环境 → fail-safe 全量跑" >&2; return 0
  fi
  local decision
  decision=$( { git -C "$ROOT" diff --name-only "$base" 2>/dev/null || true
                # 新增（未跟踪）文件不进 diff——「新增门脚本」恰是最该触发的 diff
                git -C "$ROOT" ls-files --others --exclude-standard 2>/dev/null || true; } | \
    GLOBS="$globs_json" python3 -c '
import fnmatch, json, os, sys
globs = json.loads(os.environ["GLOBS"])
hits = []
for line in sys.stdin:
    p = line.strip()
    if not p: continue
    if p.startswith("ci/") or p.startswith("proofs/") or any(fnmatch.fnmatch(p, g) for g in globs):
        hits.append(p)
print("full" if hits else "nightly-only")
if hits: print("触发路径:", " ".join(hits[:10]), file=sys.stderr)
' )
  [ -n "$decision" ] || decision=full
  echo "$decision"
}

do_list() {
  echo "注册表: $REG"
  probe_tsv | awk -F'\037' '{printf "  %-8s trigger=%-11s action=%-22s expect exit=%-8s stderr~\"%s\"\n", $1, $2, $3, $8, substr($9,1,30)}'
}

case "$CMD" in
  check-registry)           do_check_registry ;;
  update-registry-baseline) do_update_baseline ;;
  run)                      do_run ;;
  should-run)               do_should_run ;;
  list)                     do_list ;;
esac
