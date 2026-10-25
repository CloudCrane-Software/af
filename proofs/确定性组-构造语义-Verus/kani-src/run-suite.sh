#!/bin/bash
# Wave-1 Kani 镜像套件运行器——逐 harness 内核裁决 + 汇总日志。
# 用法: bash /dev/shm/wave1/run-suite.sh <日志输出路径>
set -u
export PATH=/dev/shm/fv0/kani-0.68.0/bin:$PATH
export TMPDIR=/dev/shm/wave1/tmp
export CARGO_TARGET_DIR=/dev/shm/wave1/kani-target
LOG="${1:-/dev/shm/wave1/suite-run.log}"
cd /dev/shm/wave1/kani-proj || exit 2

HARNESSSES=(
  b01_b06_e2002_e2004_sound_and_complete
  b04_carried_bounded_exhaustive_flags
  b01_counter_bounds_and_exhaustion
  b01_no_new_round_after_exhaustion
  b03_iteration_rebuild_empty
  b04_truncate_respects_declared_bound
  b09_each_element_exactly_one_instance
  b13_aggregation_order_is_instance_order
  b11_in_flight_bounded_by_declared_fanout
  c03_sort_output_sorted_and_multiset_preserving
  c03_sort_idempotent
  c03_sort_permutation_invariant
  c02_iteration_order_is_lexicographic
  c07_naming_injective_and_stateless
  c07_entry_is_first_node_or_empty_body_error
  c07_graph_version_stamps_parent
  c01_double_run_bitwise_identical
)

echo "=== wave1 suite run $(date -Is) ===" > "$LOG"
PASS=0; FAIL=0
for h in "${HARNESSSES[@]}"; do
  echo "--- harness: $h" >> "$LOG"
  timeout 600 cargo kani --harness "$h" >> "$LOG" 2>&1
  rc=$?
  verdict=$(grep -E "VERIFICATION:- (SUCCESSFUL|FAILED)" "$LOG" | tail -1)
  if [ $rc -eq 0 ] && echo "$verdict" | grep -q SUCCESSFUL; then
    PASS=$((PASS+1)); status="VERIFIED"
  else
    FAIL=$((FAIL+1)); status="FAILED(rc=$rc)"
  fi
  echo "RESULT $h $status $verdict" >> "$LOG"
  echo "RESULT $h $status" >&2
done
echo "=== SUITE SUMMARY: pass=$PASS fail=$FAIL total=$((PASS+FAIL)) $(date -Is) ===" >> "$LOG"
echo "SUITE-DONE pass=$PASS fail=$FAIL"
