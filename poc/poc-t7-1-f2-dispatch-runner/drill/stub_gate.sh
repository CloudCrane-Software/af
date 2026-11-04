#!/usr/bin/env bash
# =============================================================================
# drill_stub_gate.sh — F2 执行面**演练桩门**（DRILL STUB GATE）
#
# ★★ 这不是 ci/run.sh 实跑 ★★
#   本件只产出一份**形态与 ci/run.sh 完全一致**的门摘要（ci/out/SUMMARY.txt），
#   用来演练「派单 → 领单 → 跑门 → 落 SUMMARY → 结论入账 → integrator 复核」
#   这条**管道**。它**不执行** fmt/clippy/test/cosign/deny 任何真门件，
#   **不产生任何真实门结论**。真实 F2 全量门仍须在供给齐备的独立 runner 宿主上
#   跑 `bash ci/run.sh`（见 docs/F2-DISPATCH-RUNNER.md §三）。
#
# 摘要格式（逐字对齐 ci/run.sh 的 SUMMARY 行 ⇒ integrator 复核器认得）：
#   step<槽位>  <状态>  <毫秒>ms  <名称>
#   状态词表与 ci/integrator_bind_check.py 一致：
#     绿：PASS / PASS-WITH-WARN / ADVISORY-PASS / ADVISORY-WARN
#     红：FAIL / ADVISORY-FAIL / 任何未知词（含 SKIP-* ⇒ 不在白名单 ⇒ 红）
#
# 模式（环境变量 AF_DRILL_MODE）：
#   green（默认）：14 槽全 PASS
#   red：          step7 置 FAIL（模拟门红）
#   skip：         step6 置 SKIP-UNMET-HOST-SUPPLY（模拟宿主缺件 ⇒ SKIP 不得计绿）
#   partial：      只出 step1..step5（模拟门未跑完 ⇒ 槽位不齐）
#
# 摘要落点：$AF_DRILL_SUMMARY（缺省 ./ci/out/SUMMARY.txt）
# =============================================================================
set -u
OUT_SUMMARY="${AF_DRILL_SUMMARY:-ci/out/SUMMARY.txt}"
MODE="${AF_DRILL_MODE:-green}"
mkdir -p "$(dirname "$OUT_SUMMARY")"

{
  echo "F2-DISPATCH-E2E-DRILL banner: gate=stub（非 ci/run.sh 实跑；本摘要只演练管道，不构成门结论）"
  echo "F2-DISPATCH-E2E-DRILL mode=$MODE generated_at=$(date '+%F %T')"
} > "$OUT_SUMMARY"

add() { # add <槽位> <状态> <毫秒> <名称>
  printf 'step%s  %s  %sms  %s\n' "$1" "$2" "$3" "$4" >> "$OUT_SUMMARY"
}

add 1 PASS 11 "cargo fmt --check + K5 判分链路径 lint"
add 2 PASS 23 "cargo clippy + clippy_gate"
add 3 PASS 913 "provenance 门 + cargo test -j4 --workspace"
add 4 PASS 7 "cue recipe vet+export+断言"
add 5 PASS 4 "parse_scrub × 7 真实日志"
if [ "$MODE" = "skip" ]; then
  add 6 SKIP-UNMET-HOST-SUPPLY 0 "cosign 断网 verify-blob（宿主未供给 bin/cosign ⇒ 跳过档）"
else
  add 6 PASS 31 "cosign 断网 verify-blob"
fi
if [ "$MODE" = "red" ]; then
  add 7 FAIL 88 "cargo deny --offline check"
else
  add 7 PASS 52 "cargo deny --offline check"
fi
add 8 PASS 18 "neutrality 删包自洽+领域词扫描+golden 登记"
add 9 ADVISORY-PASS 64 "FV advisory: Rocq coqc + Kani INV 镜像（非阻塞）"
add 10 PASS 6 "承诺-交付对账 deliverables.yaml"
add 11 PASS 3 "自述-实现对账 doc-claims.txt"
add 12 PASS 5 "计数复算 numbers.json + 占位符"
add 13a PASS 9 "fix-scan 同族横切扫描"
add 13b PASS 7 "arm-wiring 三臂防线逐格断言"
add 13c PASS 3 "budget-cast-lint 预算 cast 门"
if [ "$MODE" = "partial" ]; then
  exit 0   # 故意不出 step14 ⇒ 槽位不齐
fi
add 14 PASS 12 "机读一致性批（branches+xcheck+criteria+strategies）"

exit 0
