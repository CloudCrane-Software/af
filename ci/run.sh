#!/usr/bin/env bash
# =============================================================================
# AF CI 骨架（ci-skeleton 定案管道）— AF/ci/run.sh
#
# 六步验收管道，全部真实执行、逐步计时、产物落 AF/ci/out/：
#   1 fmt     cargo fmt --check                          硬门：非零即红
#   2 clippy  cargo clippy + 基线门（ci/clippy_gate.py）   硬门：只拦新增警告
#   3 test    cargo test -j4 --workspace                 硬门：非零即红
#   4 cue     poc-cue recipe vet + export + 结构断言      硬门：正例过/反例拒/断言全中
#   5 scrub   ci/parse_scrub.py 对 7 份真实日志判定       硬门：期望判定逐一核对
#   6 cosign  断网 netns verify-blob --trusted-root    硬门：正例过/篡改例拒
#             （sudo unshare -n 优先；no_new_privs 沙箱回退 unshare -Urn，隔离语义等价）
#   7 deny    cargo deny --offline check               硬门：licenses/bans/sources/advisories
#   8 neutrality 删包自洽+领域词扫描+golden 登记        硬门（ci/neutrality_gate.sh）
#   9 fv-advisory 形式验证早警报（Rocq coqc + Kani INV 镜像）advisory：FAIL 不改
#             退出码（非阻塞；工具缺席=ADVISORY-WARN、跑了且判据红=ADVISORY-FAIL
#             落 ci/out/step9-failures.json 供 step10 消费——G0-B2 三态化；
#             预算 <2min；Verus 本体不装——失败史在案）
#
# 外部依赖: cargo+rustup(rustfmt/clippy 组件)、python3、unshare（sudo 可选）、
#           AF/bin/cosign、AF/poc/poc-cue/bin/cue、AF/poc/poc-cas 真实测试资产；
#           step9 可选依赖 coqc（PATH）、Kani 0.68.0（tmpfs /dev/shm/fv0，缺席 WARN）。
# 可重复性: 全部路径相对本脚本所在仓库根；产物写 ci/out/（可整目录删除重跑）。
# step0 预检: 工具/组件/workspace manifest 之外含 /tmp 配额护栏——TMPDIR 落在
#             /tmp 树且可用 <2GB 时自动 export 磁盘 TMPDIR=$HOME/af-ci-tmp 并
#             登记 ci/out/step0.log（史实：/tmp usrquota 默认 TMPDIR 曾致
#             os error 122 连锁门红，改磁盘 TMPDIR 后九门全绿）。
# 退出码: 0=全绿；1=有硬门失败（所有步骤仍会跑完，一次拿全量诊断）；
#         step9 advisory 失败不影响退出码（SUMMARY 标 ADVISORY-WARN/ADVISORY-FAIL，
#         ADVISORY-FAIL 另落 ci/out/step9-failures.json 由 step10 对账门消费）。
# =============================================================================
set -u -o pipefail

AF_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CI_DIR="$AF_ROOT/ci"
OUT="$CI_DIR/out"
mkdir -p "$OUT"
export AF_ROOT OUT   # 步骤函数会在子 bash 里执行，必须导出

# ---- 计时与登记 -------------------------------------------------------------
declare -a SUMMARY=()
OVERALL=0

now_ns() { date +%s%N; }

step() { # step <编号> <名称> -- <命令...>   执行、计时、记录
  local id="$1" name="$2"; shift 2; [ "${1:-}" = "--" ] && shift
  local t0=$(now_ns) rc
  "$@" > "$OUT/step${id}.log" 2>&1
  rc=$?
  local t1=$(now_ns)
  local ms=$(( (t1 - t0) / 1000000 ))
  if [ $rc -eq 0 ]; then
    SUMMARY+=("step$id  PASS  ${ms}ms  $name")
    printf '  [%s] PASS %6dms  %s\n' "$id" "$ms" "$name"
  else
    SUMMARY+=("step$id  FAIL  ${ms}ms  $name  (exit=$rc, 日志: ci/out/step${id}.log)")
    printf '  [%s] FAIL %6dms  %s  (exit=%s)\n' "$id" "$ms" "$name" "$rc"
    tail -15 "$OUT/step${id}.log" | sed 's/^/      | /'
    OVERALL=1
  fi
  return 0   # 步骤失败不中断管道，跑完全部步骤收集完整诊断
}

# ---- 0. 预检 ---------------------------------------------------------------
echo "== AF CI 管道开始 $(date '+%F %T') root=$AF_ROOT"
if [ -f "$HOME/.cargo/env" ]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi
rustup component add rustfmt clippy >>"$OUT/step0.log" 2>&1
# ---- 0.x /tmp 配额护栏（2026-10-27 合流发布批固化） ---------------------------
# 史实：CI 曾因 /tmp usrquota（tmpfs 每用户配额）下默认 TMPDIR 建到一半报
# os error 122（Disk quota exceeded）连锁多门红；把 TMPDIR 改到磁盘后九门全绿。
# 护栏（step0 预检，最小实现——机器强制优于 runbook 文档化）：
#   条件一 TMPDIR 指向 /tmp 树（未设时默认即 /tmp）；
#   条件二 该文件系统可用空间 <2GB（配额紧张）；
#   两条件同时命中 ⇒ 自动 export 磁盘 TMPDIR=$HOME/af-ci-tmp 并登记 step0.log；
#   候选盘并不更空裕（同文件系统）时保持原样并 WARN——改挂同紧盘无意义。
# 不紧张则零扰动放行（不写日志、不改环境）。
tmp_quota_guard() {
  local d="${TMPDIR:-/tmp}"
  [ -d "$d" ] || d=/tmp
  local real; real="$(cd "$d" 2>/dev/null && pwd -P)" || return 0
  case "$real" in /tmp|/tmp/*) ;; *) return 0 ;; esac   # 条件一：/tmp 树
  local avail_kb; avail_kb="$(df -P "$real" 2>/dev/null | awk 'NR==2{print $4}')"
  local min_kb=$((2*1024*1024))                          # 2GB 紧张阈值
  [ "${avail_kb:-0}" -lt "$min_kb" ] || return 0         # 条件二：不紧张即放行
  local disk="$HOME/af-ci-tmp"
  mkdir -p "$disk" || return 0
  local disk_avail; disk_avail="$(df -P "$disk" 2>/dev/null | awk 'NR==2{print $4}')"
  if [ "${disk_avail:-0}" -gt "${avail_kb:-0}" ]; then
    export TMPDIR="$disk"
    printf '[tmp-guard] %s TMPDIR 原指向 %s（/tmp 树，可用 %sKB < %sKB 阈值）——已自动改挂磁盘 TMPDIR=%s（可用 %sKB；史实：/tmp usrquota 默认 TMPDIR 曾致 os error 122 连锁门红，改磁盘 TMPDIR 后九门全绿）\n' \
      "$(date '+%F %T')" "$real" "$avail_kb" "$min_kb" "$disk" "$disk_avail" >> "$OUT/step0.log"
    printf '  [0] /tmp 配额护栏命中：TMPDIR 改挂 %s（/tmp 可用 %sKB < 2GB；磁盘可用 %sKB）\n' \
      "$disk" "$avail_kb" "$disk_avail"
  else
    printf '[tmp-guard] %s WARN：TMPDIR=%s 可用 %sKB < %sKB 阈值，但候选磁盘 %s 可用 %sKB 并不更空裕——保持原 TMPDIR（改挂同紧盘无意义；如门红请手工指定更空裕的 TMPDIR）\n' \
      "$(date '+%F %T')" "$real" "$avail_kb" "$min_kb" "$disk" "$disk_avail" >> "$OUT/step0.log"
    printf '  [0] /tmp 配额护栏 WARN：无更空裕磁盘候选，保持 TMPDIR=%s\n' "$real"
  fi
}
tmp_quota_guard
PREFLIGHT=()
for tool in cargo python3 sudo unshare "$AF_ROOT/bin/cosign" "$AF_ROOT/poc/poc-cue/bin/cue"; do
  if ! command -v "$tool" >/dev/null 2>&1 && [ ! -x "$tool" ]; then
    PREFLIGHT+=("缺依赖: $tool")
  fi
done
# workspace manifest 完整性（共享仓库里并发改动可能弄坏 members）
if ! (cd "$AF_ROOT" && cargo metadata --no-deps --format-version 1 >/dev/null 2>>"$OUT/step0.log"); then
  PREFLIGHT+=("cargo metadata 失败：workspace manifest 已损坏（查看 ci/out/step0.log）")
fi
# ---- 0.x mutations 元探针完整性（G0-B3；复盘 §三 C3-② :136 / PP1） -------------
# 探针清单（数量+每探针期望值）哈希钉 ci/baselines/mutations-registry.sha256：
#   删/改任一探针即「探针清单漂移」红（自指防护——防护被关掉本身就是红，
#   基线缺席同样 fail-closed）。有意变更须 mutation_runner.sh
#   update-registry-baseline 与注册表改动同批评审提交（防悄悄改基线=逃逸）。
#   不占新 step 编号（门数账纪律）；全量演练（should-run 判 full）挂合流入口。
if ! bash "$CI_DIR/mutation_runner.sh" check-registry --root "$AF_ROOT" >>"$OUT/step0.log" 2>&1; then
  echo "step0 FAIL: mutations 探针清单漂移（删/改探针未走 update-registry-baseline 同批评审）"
  exit 1
fi
if [ ${#PREFLIGHT[@]} -gt 0 ]; then
  printf '预检失败:\n' ; printf '  %s\n' "${PREFLIGHT[@]}"
  exit 2
fi
echo "  [0] 预检 OK（工具/组件/workspace manifest）"

# ---- 1. cargo fmt --check ---------------------------------------------------
echo "== step1 cargo fmt --check + K5 判分链路径 lint（总计划 §2 裁决7：K5 并入编译阶段 1）"
step 1 "cargo fmt --check + K5 判分链路径 lint" -- \
  bash -c "cd '$AF_ROOT' && cargo fmt --check --all && python3 ci/k5-path-lint.py"

# ---- 2. clippy + 警告基线门 --------------------------------------------------
echo "== step2 clippy（基线门：新增警告才 fail）"
step 2 "cargo clippy + clippy_gate" -- \
  bash -c "cd '$AF_ROOT' && cargo clippy --workspace --all-targets -j4 --message-format=json > ci/out/step2-clippy.jsonl 2>/dev/null; rc=\$?; [ \$rc -ne 0 ] && exit \$rc; python3 ci/clippy_gate.py ci/out/step2-clippy.jsonl"

# ---- 3. cargo test ----------------------------------------------------------
# 【G0-B1】step3 pre-step provenance 门（红队 W3-C-25 / 复盘 §三 C8 / §五 5.1）：
#   共享 CARGO_TARGET_DIR 跨工作树时，cargo 对 workspace 成员的 fingerprint
#   local 表为空，freshness 退化为「输出 mtime vs 源 mtime」比较，不校验内容
#   与路径——从本树跑 cargo test 会原样执行另一棵工作树构建的测试二进制并报
#   exit 0（假绿）。W3-C-25 实证：共享 target 内 exec_contract-*.d 的
#   CARGO_MANIFEST_DIR env-dep 全部指向另一棵工作树。
#   语义：ensure 判脏即清（外来污染且清不动/拒清 → step3 直接红不跑测试）；
#   cargo test 跑完无论成败都 record 三要素落 ci/out/step3-provenance.json
#   （缺件或异值的「绿」不得作为结案证据，由 step10 消费）。step 编号不变
#   （AGENT.md 硬纪律 5「九门」门数账口径）。详见 ci/provenance_check.sh 头注。
echo "== step3 pre-step provenance 门（W3-C-25：共享 target 跨工作树假绿防护——判脏即清，绿记录带三要素）"
echo "== step3 cargo test"
step 3 "provenance 门 + cargo test -j4 --workspace" -- \
  bash -c "cd '$AF_ROOT' && bash ci/provenance_check.sh ensure --root '$AF_ROOT'; pvc=\$?; \
             [ \$pvc -ne 0 ] && exit \$pvc; \
             cargo test -j4 --workspace; rc=\$?; \
             bash ci/provenance_check.sh record --root '$AF_ROOT' \
               --out '$OUT/step3-provenance.json' --test-log '$OUT/step3.log' \
               >> '$OUT/step3-provenance-record.log' 2>&1 || true; \
             exit \$rc"

# ---- 4. CUE 配方预编译 --------------------------------------------------------
echo "== step4 cue vet/export + 结构断言"
cue_step() {
  local CUE="$AF_ROOT/poc/poc-cue/bin/cue"
  local R="$AF_ROOT/poc/poc-cue/recipe"
  # 4a. L3+L4 合法合并：vet 必须过
  "$CUE" vet "$R/recipe-default.cue" "$R/operator-override.cue" || return 1
  # 4b. export 落 JSON（部署期预编译产物）
  "$CUE" export "$R/recipe-default.cue" "$R/operator-override.cue" \
    > "$OUT/step4-recipe-merged.json" || return 1
  # 4c. 结构断言：L4 覆盖生效 + L3 默认保留 + 键集闭合
  python3 - "$OUT/step4-recipe-merged.json" <<'PYEOF' || return 1
import json, sys
d = json.load(open(sys.argv[1]))
expected = {
    "scheduling": {"batchIntervalSec": 15, "maxConcurrentJobs": 4,
                   "retry": {"maxAttempts": 3, "backoffSec": 5}},
    "thresholds": {"cycleTimeWarnSec": 120, "defectRatioWarn": 0.05,
                   "temperatureCritC": 85},
}
assert set(d) == {"scheduling", "thresholds"}, f"顶层键集漂移: {sorted(d)}"
assert d == expected, f"合并结果漂移:\n got={d}\n want={expected}"
print("recipe-merged.json 结构断言: 9 值全中，键集闭合")
PYEOF
  # 4d. 反例：L4 越界覆盖必须被 vet 拒绝（非零退出才算过）
  if "$CUE" vet "$R/recipe-default.cue" "$R/operator-override-bad.cue" \
      > "$OUT/step4-bad-override.log" 2>&1; then
    echo "反例 operator-override-bad.cue 未被拒绝" >&2; return 1
  fi
}
step 4 "cue recipe vet+export+断言" -- bash -c "$(declare -f cue_step); cue_step"

# ---- 5. zot scrub 输出解析门（真实日志测试集） ---------------------------------
echo "== step5 parse_scrub 真实日志测试集"
scrub_step() {
  local P="$AF_ROOT/ci/parse_scrub.py"
  local L="$AF_ROOT/poc/poc-cas/verify-logs"
  local verdict_file="$OUT/step5-scrub-verdicts.txt"; : > "$verdict_file"
  local rc_all=0
  # 应通过（STATUS 全 ok）
  for f in scrub-baseline.log scrub-after-gc.log; do
    python3 "$P" "$L/$f" >/dev/null 2>&1; rc=$?
    echo "PASS-EXPECTED  exit=$rc  $f" >> "$verdict_file"
    [ $rc -ne 0 ] && rc_all=1
  done
  # 应失败（STATUS=affected）
  for f in "$L"/scrub-corrupt1.log "$L"/scrub-corrupt2.log "$L"/scrub-corrupt3a.log \
           "$L"/scrub-corrupt3b-*.log; do
    python3 "$P" "$f" >/dev/null 2>&1; rc=$?
    echo "FAIL-EXPECTED  exit=$rc  $(basename "$f")" >> "$verdict_file"
    [ $rc -ne 1 ] && rc_all=1
  done
  cat "$verdict_file"
  return $rc_all
}
step 5 "parse_scrub × 7 真实日志" -- bash -c "$(declare -f scrub_step); scrub_step"

# ---- 6. cosign 离线验签（断网命名空间） ---------------------------------------
echo "== step6 cosign --trusted-root 断网验签"
cosign_step() {
  local COSIGN="$AF_ROOT/bin/cosign"
  local T="$AF_ROOT/poc/poc-cas/verify-tr"
  cd "$T" || return 2
  # 断网手段择一即可：sudo unshare -n（真 root netns）优先；sudo 不可用（如
  # no_new_privs 沙箱）时回退 unshare -Urn——非特权 user+netns，loopback down、
  # 无外联，断网隔离语义等价，门禁强度不变（6b 篡改例仍必须被拒）。
  local NETNS=()
  if command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
    NETNS=(sudo unshare -n)
  elif unshare -Urn true 2>/dev/null; then
    NETNS=(unshare -Urn)
  else
    echo "无可用断网手段：sudo(unshare -n) 与 unshare -Urn 均失败" >&2
    return 2
  fi
  # 6a. 正例：无网络命名空间内，bundle+trusted-root 离线验签必须过
  "${NETNS[@]}" "$COSIGN" verify-blob \
    --key tr-key.pub --bundle tr-bundle.json --trusted-root trusted-root.json \
    artifact-tr.txt > "$OUT/step6-verify.log" 2>&1 || { return 1; }
  # 6b. 负例：篡改 1 字节的工件必须被拒（证明门真的在验）
  if "${NETNS[@]}" "$COSIGN" verify-blob \
      --key tr-key.pub --bundle tr-bundle.json --trusted-root trusted-root.json \
      artifact-tampered.txt >> "$OUT/step6-verify.log" 2>&1; then
    echo "篡改工件未被拒绝" >&2; return 1
  fi
  echo "正例: $(head -1 "$OUT/step6-verify.log")；篡改例: 已拒绝（exit 1）；netns: ${NETNS[*]}"
}
step 6 "cosign 断网 verify-blob" -- bash -c "$(declare -f cosign_step); cosign_step"

# ---- 7. cargo deny 供应链门（licenses/bans/sources/advisories，离线本地 db） ----
echo "== step7 cargo deny（四类检查，advisory-db 本地化离线）"
step 7 "cargo deny --offline check" -- bash -c "cd '$AF_ROOT' && cargo deny --offline check"

# ---- 8. 中立性门（N1 删包自洽 + N2 领域词扫描 + N3 golden 登记；G4 施工批
#         Phase 3 挂入，裁决15/16/19）------------------------------------------------
# 挂门前置断言（裁决15）：挂门时对照臂（沙箱 cp 集含 rust-toolchain.toml——
# 裁决19）cargo test -p af-dsl -p af-compiler 实测 exit=0。2026-10-25 挂门时点
# 实测 CONTROL_EXIT=0（全绿）；此后该断言由门内 C1 判据常态化复测。
# 环境开关：AF_NEUT_SANDBOX_DIR / AF_NEUT_TARGET_DIR（复核修正 2026-10-25：
# 缺省=家目录外置 $HOME/afb-neut-sbx + $HOME/afbuild-neutrality——/tmp 每用户
# 配额会使内建 target 冷建撞 os error 122 连锁判据红；门内另有 128MB 配额
# 探针 fail-fast）。详见 ci/neutrality_gate.sh 头注。
echo "== step8 neutrality 删包自洽+领域词扫描+golden 登记"
step 8 "neutrality 删包自洽+领域词扫描+golden 登记" -- \
  bash "$CI_DIR/neutrality_gate.sh"

# ---- 9. 形式验证 advisory 门（FV-1 收编批 2026-10-25 挂入；非阻塞） -------------
# 语义：advisory = 形式验证面的漂移/回退早警报，**不是放行门**——step9 任何失败
#   只进 SUMMARY 的 ADVISORY-WARN 行并全文打印，绝不改 OVERALL（exit code 不受
#   影响）；工具缺席同判 WARN（环境面，非内容失败）。
# 内容：9a Rocq 陈述库 coqc 类型检查（proofs/rocq-规范库-v0 十文件，期望 10/10
#   ——Admitted 即待证义务登记，类型过=陈述自洽）；
#   9b Kani 快检子集 = INV 镜像套件全量 32 harness（proofs/Kani镜像员-INV组，
#   实测 ~28s 预算内）；确定性组 17 harness 不入快检——c02 单条实测 143.4s，超
#   本门 <2min 预算（即「Kani 全量太慢」分支，全量复现命令见 proofs/README）。
# 工具前提（tmpfs/PATH 缺席只 WARN）：coqc 走 PATH（本机 8.20.1 实测 10 文件
#   ~0.5s）；Kani 0.68.0 为 tmpfs 保留件 /dev/shm/fv0/kani-0.68.0（重启即失），
#   配套 RUSTUP_HOME=/dev/shm/fv0/rustup（工具链 nightly-2026-08-21；
#   RUSTUP_TOOLCHAIN 显式覆盖仓内 rust-toolchain.toml 的 1.99.1 钉住——Kani
#   harness 需跑在其自带工具链上）。
# Verus 本体不装（安装失败史在案：41 次代理下载 curl exit 35，FV-1 裁决书 §一；
#   状态机组 29 verified 以 proofs/状态机组-Verus/verus-state-group.log 入档
#   内核输出为准，CI 不重跑）。
echo "== step9 FV advisory（非阻塞：Rocq coqc 10 文件 + Kani INV 镜像 32 harness）"
fv_advisory() {
  local rc=0
  # —— 9a. Rocq 陈述库 coqc 类型检查（临时副本跑，.vo/.glob/.aux 不落工作树）——
  if command -v coqc >/dev/null 2>&1; then
    local RP="$OUT/step9-rocq" ; rm -rf "$RP" ; mkdir -p "$RP"
    cp "$AF_ROOT"/proofs/rocq-规范库-v0/T*.v "$RP"/
    local rp=0 rf=0 f
    for f in "$RP"/T*.v; do
      if (cd "$RP" && coqc "$(basename "$f")" >/dev/null 2>&1); then
        rp=$((rp+1))
      else
        rf=$((rf+1)) ; echo "coqc FAIL: $(basename "$f")"
      fi
    done
    echo "9a Rocq 陈述库 coqc 类型检查: ${rp}/10 通过${rf:+，$rf 失败}"
    rm -rf "$RP"
    [ "$rp" -eq 10 ] || rc=1
    # —— 9a-2 定理清单/陈述对账（红队 W3-C29）：coqc 只查陈述良构，删除定理
    # 或改前提/结论（弱化）可能照过（全库 Admitted 形态 21 条）——按文件计
    # 数拦不住证明载体缩水。对账 theorems-manifest.txt：①定理名集合逐位；
    # ②陈述块（^Theorem 起、^Proof. 前止）规范串 SHA-256。有意变更定理面须
    # 同步更新清单（承 9b harness 计数断言 32/0/32 同一登记纪律）；仅改证明
    # 体（Admitted→Qed）不触发。未纳入 step9 的证明集（确定性组 17 harness、
    # Verus 29、proofs/kani 镜像）继续明确标为覆盖外（见本文件头注）。
    local MAN="$AF_ROOT/proofs/rocq-规范库-v0/theorems-manifest.txt"
    local RPDIR="$AF_ROOT/proofs/rocq-规范库-v0"
    if [ ! -f "$MAN" ]; then
      echo "9a FAIL: 定理清单对账正本缺席（$MAN）——陈述库漂移防护失效"
      rc=1
    else
      local man_names now_names man_hash now_hash
      man_names=$(grep -v '^#' "$MAN" | awk -F'\t' '$1 != "statements-sha256" && NF >= 2 {print $2}' | sort)
      now_names=$(grep -h "^Theorem " "$RPDIR"/T*.v | sed 's/^Theorem \([^ :]*\).*/\1/' | sort)
      man_hash=$(grep '^statements-sha256' "$MAN" | awk '{print $2}')
      now_hash=$(
        (cd "$RPDIR" && for f in T*.v; do
          sed -n '/^Theorem /,/^Proof\./p' "$f" | sed '/^Proof\./d'
        done) | sha256sum | cut -d' ' -f1
      )
      if [ "$man_names" != "$now_names" ]; then
        echo "9a FAIL: 定理名集合漂移（期望 $(echo "$man_names" | wc -l) 条 ↔ 实得 $(echo "$now_names" | wc -l) 条；差异：$(comm -3 <(echo "$man_names") <(echo "$now_names") | tr '\n' ' '))——定理被删/改名或有增，须更新 theorems-manifest.txt"
        rc=1
      fi
      if [ "$man_hash" != "$now_hash" ]; then
        echo "9a FAIL: 定理陈述漂移（SHA-256 期望 ${man_hash:0:12}… ↔ 实得 ${now_hash:0:12}…）——陈述/前提被弱化或改写，须更新 theorems-manifest.txt 并附裁决依据"
        rc=1
      fi
      if [ "$man_names" = "$now_names" ] && [ "$man_hash" = "$now_hash" ]; then
        echo "9a-2 定理清单对账: $(echo "$man_names" | wc -l) 条定理名逐位一致 + 陈述块 SHA-256 一致（${man_hash:0:12}…）"
      fi
    fi
  else
    echo "9a WARN: coqc 不在 PATH——Rocq 面 advisory 未跑（工具缺席，非内容失败）"
    rc=1
  fi
  # —— 9b. Kani INV 镜像快检（临时副本置 $HOME——仓库树外，cargo 不会把它误认
  #    workspace 成员；Cargo.lock/target 均不落工作树，跑完即删）——
  local KDIR=/dev/shm/fv0/kani-0.68.0
  if [ -x "$KDIR/bin/cargo-kani" ] && [ -d /dev/shm/fv0/rustup ]; then
    local KH="$HOME/afb-fv-advisory-harness" ; rm -rf "$KH" ; mkdir -p "$KH"
    cp -r "$AF_ROOT/proofs/Kani镜像员-INV组/harness/." "$KH"/
    if (cd "$KH" && env RUSTUP_HOME=/dev/shm/fv0/rustup \
        RUSTUP_TOOLCHAIN=nightly-2026-08-21-x86_64-unknown-linux-gnu \
        PATH="$KDIR/bin:$PATH" \
        CARGO_TARGET_DIR="$HOME/afbuild-phase2fv-kani" \
        timeout 100 cargo kani > "$OUT/step9-kani.log" 2>&1); then
      # 9b 判定不再只看退出码（红队 W1-C44）：cargo kani 对「harness 被删减
      #     甚至全删」仍 exit 0（No proof harnesses found 也是 0）——解析
      #     Complete 汇总行并断言 harness 计数恰 32（INV 镜像面契约）且
      #     0 failures；缺行/计数漂移均 FAIL（证明集缩水此前零防护）。
      #     【G0-B2】期望值不再手写常数 32：由
      #     ci/baselines/kani-inv-harness.manifest（baseline_calc gen-kani 从
      #     proofs 源机算）的 count 行读入；正本缺席/漂移即 9b 红（先显式判
      #     退出码再消费，禁裸管道吞退出码——复盘 §三 C3-③ :137）。手写常数
      #     = 判据基准可改（C3 rootCause :131）。
      #     有意变更 harness 数须同批再生成正本并登记依据。
      local complete verified_k failures_k total_k exp_k
      if ! grep -q '^Complete - ' "$OUT/step9-kani.log"; then
        echo "9b FAIL: 日志缺 Complete 汇总行（见 ci/out/step9-kani.log）"
        rc=1
      else
        complete=$(grep -E '^Complete - ' "$OUT/step9-kani.log" | tail -1)
        # 9b 基线对账（C3-③：manifest 机算替手写 32；正本缺席/漂移即红）
        # 路径参数化到本工作树（root 钉死纪律）：fv_advisory 在调用方 cwd 下跑，
        # 不用相对路径 ci/baseline_calc.sh。
        if ! bash "$CI_DIR/baseline_calc.sh" verify-kani --root "$AF_ROOT" >>"$OUT/step9-kani.log" 2>&1; then
          echo "9b FAIL: 基线对账不过（kani-inv-harness.manifest 缺席或漂移）——先 gen-kani 再生成正本并登记依据（见 ci/out/step9-kani.log）"
          rc=1
        fi
        exp_k=$(awk -F'\t' '$1=="count"{print $2}' "$AF_ROOT/ci/baselines/kani-inv-harness.manifest")
        verified_k=$(echo "$complete" | grep -oE '[0-9]+ successfully verified harnesses' | grep -oE '^[0-9]+')
        failures_k=$(echo "$complete" | grep -oE '[0-9]+ failures' | grep -oE '^[0-9]+')
        total_k=$(echo "$complete" | grep -oE '[0-9]+ total' | grep -oE '^[0-9]+')
        echo "9b Kani INV 镜像快检: exit=0 | $complete"
        if [ "${verified_k:-0}" -ne "${exp_k:-x}" ] || [ "${failures_k:-1}" -ne 0 ] || [ "${total_k:-0}" -ne "${exp_k:-x}" ]; then
          echo "9b FAIL: harness 计数断言不过（基线机算期望 ${exp_k:-缺} verified / 0 failures / ${exp_k:-缺} total，实得 ${verified_k:-缺}/${failures_k:-缺}/${total_k:-缺}——证明集被删减或有失败）"
          rc=1
        fi
      fi
    else
      echo "9b WARN: Kani 快检非零退出（超时 100s 或验证失败——详见 ci/out/step9-kani.log）"
      tail -5 "$OUT/step9-kani.log" 2>/dev/null | sed 's/^/      | /'
      rc=1
    fi
    rm -rf "$KH"
  else
    echo "9b WARN: Kani 0.68.0 tmpfs 安装缺席（$KDIR 或 /dev/shm/fv0/rustup 不在）——Kani 面 advisory 未跑（tmpfs 重启即失，非内容失败）"
    rc=1
  fi
  return $rc
}
T9_0=$(now_ns)
# 【G0-B2】a_verdict 三态化（开发计划 §2.2：消除两态死代码；复盘 C3 rootCause
#   :131）：ADVISORY-PASS=跑了且判据全绿 / ADVISORY-WARN=工具缺席（coqc/Kani
#   不在，维持现状语义）/ ADVISORY-FAIL=**跑了且 9a/9b 判据红**——现状这类也落
#   PASS/WARN 属死代码（绿灯不承载守护承诺）。退出码仍非阻塞（step9 advisory
#   定位与「九门」门数账口径不变），但 ADVISORY-FAIL 必须落结构化产物
#   ci/out/step9-failures.json（fail 项+日志指针），由 step10 对账门消费：
#   存在未消化的 ADVISORY-FAIL 不得结案——把「不改退出码」从死代码变成有
#   消费者的显式契约。
if fv_advisory > "$OUT/step9.log" 2>&1; then
  a_verdict="ADVISORY-PASS"          # 非阻塞：退出码不受影响（九门口径不变）
else
  if grep -qE '^(9a|9b) FAIL' "$OUT/step9.log"; then
    a_verdict="ADVISORY-FAIL"
    grep -E '^(9a|9b) FAIL' "$OUT/step9.log" | python3 -c 'import json,sys; print(json.dumps({"verdict":"ADVISORY-FAIL","step9_failures":sys.stdin.read().splitlines(),"log":"ci/out/step9.log"}, ensure_ascii=False))' > "$OUT/step9-failures.json"
  else
    a_verdict="ADVISORY-WARN"
  fi
fi
T9_1=$(now_ns); T9_MS=$(( (T9_1 - T9_0) / 1000000 ))
SUMMARY+=("step9  $a_verdict  ${T9_MS}ms  FV advisory: Rocq coqc + Kani INV 镜像（非阻塞）")
printf '  [9] %s %6dms  FV advisory（Rocq coqc + Kani INV 镜像 32，非阻塞）\n' "$a_verdict" "$T9_MS"
cat "$OUT/step9.log"

# ---- 汇总 -------------------------------------------------------------------
echo "== 汇总"
printf '%s\n' "${SUMMARY[@]}" | tee "$OUT/SUMMARY.txt"
if [ $OVERALL -eq 0 ]; then
  echo "== 管道全绿"
else
  echo "== 管道存在红项（见上），exit 1"
fi
exit $OVERALL
