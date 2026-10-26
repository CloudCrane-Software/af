#!/usr/bin/env bash
# =============================================================================
# step8 中立性门主脚本（N1 删包自洽 + N2 领域词扫描 + N3 golden 登记 + 门红语义）
# ——《G4-任务中立性机器检查方案.md》§1.2 结构；G4 施工批 Phase 3 落盘（裁决15/16/19）
#
# 判据（§1.4；任一 FAIL → exit 1 = 门红，对应 run.sh OVERALL=1）：
#   C1  对照臂（包原样）cargo test -p af-dsl -p af-compiler exit=0
#   I1  隔离臂失败集 ⊆ ci/neutrality-expected-fail.txt（22 项，红队 W2-C-53 勘误：原注 19）
#   I2  名单 ⊆ 隔离臂失败集（缺项 = fail-open，最严重）
#   I3  I1∧I2（集合精确相等；R3 漂移）
#   I4  g3_golden_corpus_byte_exact（17 语料，含 sample1-6）在隔离臂通过
#   P-A 无 --registry 时 sample1 快照链 exit=0 且与 golden 逐字节一致
#   P-B 空包根下 selector 稿（MetaGPT）exit≠0 且 stderr 含 E2001
#   P-C 空包根下非 selector 稿（ReAct）exit≠0 且 stderr 含 E9001
#   S1  ci/neutrality_scan.py 白名单外命中数 = 0（对正仓 src 扫描）
#   R7  testdata/golden/*.ir.json ⇔ ci/neutrality-golden-registry.tsv IR 行互为闭集
#   Z0  正仓 git status 前后零差（沙箱绝不写正仓）
#
# 挂门前置断言（裁决15）：step8 挂入 ci/run.sh 的时机=施工批收官；挂门时对照臂
#   必须实测 exit=0。2026-10-25 挂门时点实测：沙箱（cp 集含 rust-toolchain.toml，
#   裁决19）对照臂 cargo test --offline -p af-dsl -p af-compiler --no-fail-fast
#   CONTROL_EXIT=0 全绿——断言通过后本门随同 commit 挂入。此后 C1 判据即该断言
#   的常态化：每次门跑都实测对照臂（非一次性背书）。
#
# 环境开关与缺省（复核修正 2026-10-25：/tmp 每用户配额可使缺省冷建撞
# os error 122 → 对照臂 exit≠0 连锁七判据红——缺省改家目录外置大盘，
# 原样 `bash ci/run.sh` 即可复现绿；阶段0 另有配额探针 fail-fast）：
#   AF_NEUT_SANDBOX_DIR  沙箱目录（默认 $HOME/afb-neut-sbx；可改 /tmp 等
#                        任意可写位置——路径固定非随机，理由见下）
#   AF_NEUT_TARGET_DIR   沙箱 cargo target 目录（默认 $HOME/afbuild-neutrality
#                        外置共享：跨臂/跨跑复用、臂间零重建；多跑并发不共址）
# =============================================================================
set -u -o pipefail

SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CI_DIR="$SRC/ci"
OUT_DIR="$CI_DIR/out"
LOG="$OUT_DIR/step8-neutrality.log"
EXPECTED="$CI_DIR/neutrality-expected-fail.txt"
WORDLIST="$CI_DIR/neutrality-wordlist.txt"
REGISTRY_TSV="$CI_DIR/neutrality-golden-registry.tsv"
mkdir -p "$OUT_DIR"

PASS_COUNT=0; FAIL_COUNT=0
verdict() { # verdict <判据> <PASS|FAIL> [说明]——参数安全展开（${:-}），防 set -u
  # 崩溃在管道子壳引爆继承的 EXIT trap（实测 repro：沙箱会被误删），故全部默认化。
  local id="${1:-?}" st="${2:-FAIL}" note="${3:-}"
  if [ "$st" = "PASS" ]; then PASS_COUNT=$((PASS_COUNT+1)); else FAIL_COUNT=$((FAIL_COUNT+1)); fi
  printf '%s\n' "[$st] $id $note" | tee -a "$LOG"
}

echo "== step8 neutrality 门开始 $(date '+%F %T')" | tee -a "$LOG"

# 正仓快照函数：ci/out/ 是门自身声明产物面（run.sh 各步同写此处），从 Z0 比对
# 面剔除；其余任何路径前后有差 = 沙箱/扫描误写正仓 = 门红。
snap_status() { git -C "$SRC" status --porcelain 2>/dev/null | grep -v -E '^.. ?"?ci/out/' || true; }

# ---- 阶段 0：沙箱构造（正仓只读；裁决19：cp 集含 rust-toolchain.toml） --------
if [ ! -f "$SRC/rust-toolchain.toml" ]; then
  echo "[FAIL] Z0-pre rust-toolchain.toml 缺席——裁决19 要求其随沙箱拷贝（工具链钉面）" | tee -a "$LOG"
  exit 1
fi
GIT_PRE="$(snap_status)"
# 沙箱固定路径（非 mktemp 随机）：cargo 新鲜度按内容不按路径判——测试二进制在
# 编译期烙印 CARGO_MANIFEST_DIR，若沙箱路径每次随机而 target 外置共享，会把
# 「指向旧沙箱路径」的陈旧二进制直接复用（2026-10-25 挂门批实测复现：对照臂
# 读到旧沙箱被清空的包根）。固定路径 + 每跑重建 = 烙印路径恒有效；换沙箱根
# 须清空配对 target（下方守卫自动兜底）。
SBX="${AF_NEUT_SANDBOX_DIR:-$HOME/afb-neut-sbx}"
rm -rf "$SBX"
mkdir -p "$SBX"
# EXIT trap 仅主壳执行（$$==BASHPID 判别）：管道/子壳继承 trap，其内若因 set -u
# 等崩溃会误删沙箱（2026-10-25 首跑实测复现）——判别后子壳退出不再引爆清理。
trap '[ "$$" -eq "$BASHPID" ] && rm -rf "$SBX"' EXIT
cp -a "$SRC/Cargo.toml" "$SRC/Cargo.lock" "$SRC/rust-toolchain.toml" "$SRC/crates" "$SRC/af" "$SBX/"
export CARGO_TARGET_DIR="${AF_NEUT_TARGET_DIR:-$HOME/afbuild-neutrality}"

# 配额/可写性探针（复核修正新增）：冷建两臂 target 需数百 MB 连续写——/tmp
# 每用户配额小的主机会在建到一半时 os error 122（Disk quota exceeded），
# 连锁 C1/I2/I3/I4/P-A/P-B/P-C 红。预写 128MB 探针文件提前拦截，fail-fast
# 点名开关（不把磁盘问题误报成中立性问题）。
mkdir -p "$CARGO_TARGET_DIR"
QUOTA_PROBE="$CARGO_TARGET_DIR/.afb-neut-quota-probe"
if ! dd if=/dev/zero of="$QUOTA_PROBE" bs=1M count=128 >/dev/null 2>"$SBX/quota-probe.err"; then
  echo "[FAIL] Z0-pre target 目录 $CARGO_TARGET_DIR 128MB 预写失败（磁盘/配额不足；$(tail -1 "$SBX/quota-probe.err" 2>/dev/null)）——请以 AF_NEUT_TARGET_DIR（及必要时 AF_NEUT_SANDBOX_DIR）指向可写大盘后重跑" | tee -a "$LOG"
  exit 1
fi
rm -f "$QUOTA_PROBE"

# 陈旧二进制守卫：若 target 缓存里的 af-dsl 测试二进制不含本沙箱路径（跨沙箱
# 复用 target 的陈旧烙印），清掉两 crate 工件强制重编（依赖缓存保留）。
stale_guard() {
  local hit=0
  for bin in "$CARGO_TARGET_DIR"/debug/deps/registry_test-*; do
    [ -f "$bin" ] || continue
    if LC_ALL=C grep -aqF "$SBX/crates/af-dsl" "$bin" 2>/dev/null; then hit=1; break; fi
  done
  if [ "$hit" -eq 0 ] && ls "$CARGO_TARGET_DIR"/debug/deps/registry_test-* >/dev/null 2>&1; then
    echo "  [守卫] target 缓存含跨沙箱陈旧测试二进制（烙印路径≠$SBX）——cargo clean -p 两 crate 强制重编" | tee -a "$LOG"
    (cd "$SBX" && cargo clean --offline -p af-dsl -p af-compiler) >>"$LOG" 2>&1 || true
  fi
}
stale_guard
verdict Z0-pre PASS "沙箱 $SBX（cp 集：Cargo.toml/Cargo.lock/rust-toolchain.toml/crates/af，$(du -sh "$SBX" | cut -f1)）；target=${CARGO_TARGET_DIR}"

# cargo test 日志解析：→ 键集 <crate>::<测试文件>::<测试名>（FAILED 或 ok）
parse_keys() { # parse_keys <crate> <log路径> <FAILED|ok> —— 日志路径经 argv 传入
  python3 - "$1" "$3" "$2" <<'PYEOF'
import re, sys
crate, want, log = sys.argv[1], sys.argv[2], sys.argv[3]
section = ""
with open(log, encoding="utf-8", errors="replace") as f:
    for line in f:
        m = re.match(r"\s*Running (?:unittests )?(?:src|tests)/(\S+)\.rs", line)
        if m:
            p = m.group(1)
            section = "lib" if p == "src/lib" else p.split("/")[-1]
        m = re.match(r"test (\S+) \.\.\. (ok|FAILED)$", line)
        if m and section and m.group(2) == want:
            print(f"{crate}::{section}::{m.group(1)}")
PYEOF
}

CRATES="af-dsl af-compiler"

# 臂日志持久化到 ci/out/（门声明产物面；沙箱 trap 清理后仍可追溯）
arm_log() { echo "$OUT_DIR/step8-arm-$1-$2.log"; }

# ---- 阶段 1：对照臂（control：包原样）—— C1 = 挂门前置断言常态化 ---------------
echo "-- 阶段1 对照臂（包原样）" | tee -a "$LOG"
C1_STATE=PASS
for c in $CRATES; do
  L="$(arm_log control "$c")"
  if (cd "$SBX" && cargo test --offline -p "$c" --no-fail-fast) > "$L" 2>&1; then
    echo "  对照臂 $c exit=0" | tee -a "$LOG"
  else
    rc=$?
    echo "  对照臂 $c exit=$rc（基线不绿，本门结论无效；日志 $L）" | tee -a "$LOG"
    C1_STATE=FAIL
  fi
done
verdict C1 "$C1_STATE" "对照臂 cargo test exit=0（R0：基线本身不绿则本门结论无效；裁决15 挂门前置断言常态化）"

# ---- 阶段 2：隔离臂（isolated：清空包、保留空目录——README:26-28 空根语义） -----
echo "-- 阶段2 隔离臂（清空保留空目录）" | tee -a "$LOG"
find "$SBX/af/packages" -mindepth 1 -delete
echo "  af/packages 已清空（保留空目录）：$(ls -A "$SBX/af/packages" | wc -l) 项残留" | tee -a "$LOG"
ISO_FAIL_SET="$SBX/iso-fail-set.txt"; ISO_PASS_SET="$SBX/iso-pass-set.txt"
: > "$ISO_FAIL_SET"; : > "$ISO_PASS_SET"
for c in $CRATES; do
  L="$(arm_log iso "$c")"
  (cd "$SBX" && cargo test --offline -p "$c" --no-fail-fast) > "$L" 2>&1
  parse_keys "$c" "$L" FAILED >> "$ISO_FAIL_SET"
  parse_keys "$c" "$L" ok >> "$ISO_PASS_SET"
  n=$(grep -c "^test .* \.\.\. FAILED$" "$L" || true)
  echo "  隔离臂 $c 失败 $n 项（exit 码不作判据，失败集才是）" | tee -a "$LOG"
done
sort -u -o "$ISO_FAIL_SET" "$ISO_FAIL_SET"; sort -u -o "$ISO_PASS_SET" "$ISO_PASS_SET"
EXPECTED_KEYS="$SBX/expected-keys.txt"
grep -v '^\#' "$EXPECTED" | grep -v '^\s*$' | sort -u > "$EXPECTED_KEYS"

ONLY_IN_REAL="$(comm -23 "$ISO_FAIL_SET" "$EXPECTED_KEYS")"   # 意外失败（隐藏依赖）
ONLY_IN_LIST="$(comm -13 "$ISO_FAIL_SET" "$EXPECTED_KEYS")"   # 名单项变绿（fail-open）
if [ -z "$ONLY_IN_REAL" ]; then verdict I1 PASS "隔离臂失败集 ⊆ 预期红名单（$(wc -l < "$EXPECTED_KEYS") 项）——核心测试无一意外失败"
else verdict I1 FAIL "隐藏依赖（R1）：$(echo "$ONLY_IN_REAL" | tr '\n' ' ')"; fi
if [ -z "$ONLY_IN_LIST" ]; then verdict I2 PASS "预期红名单 ⊆ 隔离臂失败集——fail-closed 有牙齿"
else verdict I2 FAIL "fail-open（R2，最严重）：名单项在隔离臂变绿：$(echo "$ONLY_IN_LIST" | tr '\n' ' ')"; fi
if [ -z "$ONLY_IN_REAL" ] && [ -z "$ONLY_IN_LIST" ]; then
  verdict I3 PASS "集合精确相等（$(wc -l < "$ISO_FAIL_SET") 项，无漂移）"
else verdict I3 FAIL "名单漂移（R3，双向）"; fi
if grep -q "^af-compiler::golden_test::g3_golden_corpus_byte_exact$" "$ISO_PASS_SET"; then
  verdict I4 PASS "g3_golden_corpus_byte_exact（17 语料含 sample1-6）隔离臂通过——非领域 golden 照常（L3 口径，裁决14）"
else verdict I4 FAIL "非领域 golden 在隔离臂失败（P-A 关联）"; fi

# ---- 阶段 3：fail-closed 三探针（隔离臂同沙箱） ---------------------------------
echo "-- 阶段3 探针 P-A/P-B/P-C" | tee -a "$LOG"
if (cd "$SBX" && cargo build --offline -p af-compiler --bins) > "$SBX/probe-build.log" 2>&1; then
  BIN="$CARGO_TARGET_DIR/debug"
  TD="crates/af-compiler/testdata"
  # P-A：非领域链自洽（sample1 快照，不带 registry）
  (cd "$SBX" && "$BIN/af-compiler-snapshot" "$TD/src/sample1.yaml") > "$SBX/pa.out" 2>"$SBX/pa.err"
  PA=$?
  if [ $PA -eq 0 ] && cmp -s "$SBX/pa.out" "$SBX/$TD/golden/sample1.ir.json"; then
    verdict P-A PASS "sample1 快照链 exit=0 且与 golden 逐字节一致（cmp）"
  else verdict P-A FAIL "exit=$PA 或字节漂移（R4）：$(head -2 "$SBX/pa.err")"; fi
  # P-B：图清单 fail-closed（selector 稿 → E2001）
  (cd "$SBX" && "$BIN/af-compiler-plan" --registry "$SBX/af/packages" "$TD/src/MetaGPT.yaml") > "$SBX/pb.out" 2>"$SBX/pb.err"
  PB=$?
  if [ $PB -ne 0 ] && grep -q "E2001" "$SBX/pb.err"; then
    verdict P-B PASS "空包根下 MetaGPT exit=$PB + E2001（selector 候选图未声明，×$(grep -c E2001 "$SBX/pb.err"))"
  else verdict P-B FAIL "删包后静默放行（R5）：exit=$PB"; fi
  # P-C：组件清单 fail-closed（非 selector 稿 → E9001）
  (cd "$SBX" && "$BIN/af-compiler-plan" --registry "$SBX/af/packages" "$TD/src/ReAct.yaml") > "$SBX/pc.out" 2>"$SBX/pc.err"
  PC=$?
  if [ $PC -ne 0 ] && grep -q "E9001" "$SBX/pc.err"; then
    verdict P-C PASS "空包根下 ReAct exit=$PC + E9001（uses.* 未注册，注册表共 0 条）"
  else verdict P-C FAIL "删包后静默放行（R5）：exit=$PC"; fi
else
  verdict P-A FAIL "af-compiler bins 构建失败（见 $SBX/probe-build.log）"
  verdict P-B FAIL "同上（构建失败）"; verdict P-C FAIL "同上（构建失败）"
fi

# ---- 阶段 4：N2 扫描 + N3 登记校验（对正仓 SRC） -------------------------------
echo "-- 阶段4 N2 扫描 + N3 登记闭集" | tee -a "$LOG"
if (cd "$SRC" && python3 "$CI_DIR/neutrality_scan.py") > "$SBX/scan.log" 2>&1; then
  verdict S1 PASS "领域词扫描白名单外命中 = 0（$(grep -o '命中 [0-9]* 处' "$SBX/scan.log" | head -1)）"
else verdict S1 FAIL "白名单外命中 ≥1（R6）—— $(tail -2 "$SBX/scan.log" | tr '\n' ' ')"; fi

R7CHECK="$(python3 - "$SRC" <<'PYEOF'
import pathlib, sys
src = pathlib.Path(sys.argv[1])
golden = sorted(p.name for p in (src / "crates/af-compiler/testdata/golden").glob("*.ir.json"))
rows = []
for line in (src / "ci/neutrality-golden-registry.tsv").read_text(encoding="utf-8").splitlines():
    if line.startswith("#") or not line.strip():
        continue
    cols = line.split("\t")
    if len(cols) >= 2 and cols[1] == "IR":
        rows.append(pathlib.Path(cols[0]).name)
if sorted(set(rows)) != sorted(rows):
    print(f"DUP 登记行重复"); sys.exit(1)
if set(golden) != set(rows):
    print(f"闭集破坏：golden-only={sorted(set(golden)-set(rows))} registry-only={sorted(set(rows)-set(golden))}")
    sys.exit(1)
print(f"闭集守卫 OK：{len(golden)} 份 IR golden 与登记互为闭集")
PYEOF
)"
if [ $? -eq 0 ]; then verdict R7 PASS "$R7CHECK"; else verdict R7 FAIL "$R7CHECK"; fi

# 词表冻结（§2.1 规则3：SHA-256 随门日志落盘，diff 即审计面）
echo "词表 SHA-256：$(sha256sum "$WORDLIST" | cut -d' ' -f1)" | tee -a "$LOG"

# ---- 正仓零差自检（防误写；ci/out/ 声明产物面除外——见 snap_status 注） ----------
GIT_POST="$(snap_status)"
if [ "$GIT_PRE" = "$GIT_POST" ]; then
  verdict Z0 PASS "正仓 git status 前后零差（沙箱只读正仓）"
else
  verdict Z0 FAIL "正仓被写（R0 前置破坏）：$(echo "$GIT_POST" | diff <(echo "$GIT_PRE") <(echo "$GIT_POST") | head -5)"
fi

# ---- 汇总 -----------------------------------------------------------------------
echo "== step8 汇总：PASS=$PASS_COUNT FAIL=$FAIL_COUNT（判据 C1/I1/I2/I3/I4/P-A/P-B/P-C/S1/R7/Z0）" | tee -a "$LOG"
if [ "$FAIL_COUNT" -eq 0 ]; then
  echo "== step8 neutrality 门全绿" | tee -a "$LOG"
  exit 0
fi
echo "== step8 neutrality 门红（见 $LOG）" | tee -a "$LOG"
exit 1
