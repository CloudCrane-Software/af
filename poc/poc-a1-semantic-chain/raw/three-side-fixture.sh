#!/usr/bin/env bash
# three-side-fixture.sh — A1 语义层名册正本三面消费一致对账 + 篡改正本夹具
#
# 绿路：三面（crate 生产者 / 投影器 / 断言器）消费同一正本字节，全绿。
# 红路：篡改正本（L1 改名 + order_domain_keys 缩编）⇒ 三面全红（各自机制）：
#   crate 面  = include_str 编进二进制；契约测试 roster_contract_pins_* 红
#               （改正本需在工作树内换文件复编译；跑完 git checkout 复原）
#   投影器面 = AF_SEMANTIC_ROSTER 指向篡改版；selftest D 腿 / project 对账红
#   断言器面 = AF_SEMANTIC_ROSTER 指向篡改版；一致性 FATAL exit 1
#
# 运行位：工厂机（投影器/断言器面本机；crate 面 ssh anolis）。
# 用法：bash poc/poc-a1-semantic-chain/raw/three-side-fixture.sh
set -u
WS="${WS:-$HOME/workspace/agent黑灯工厂}"
ANOLIS="${ANOLIS:-anolis}"
WT_REMOTE='~/af-dev/af/wt/a1-semantic-chain'
TD="$(mktemp -d /tmp/a1-three-side.XXXXXX)"
trap 'rm -rf "$TD" /tmp/a1-tampered-roster.json' EXIT
fail=0
note() { printf '%s\n' "$*"; }

# 正本真字节（anolis 工作树=提交源；工厂侧 af/ 检出位合流前可能缺席）
ssh "$ANOLIS" "cat $WT_REMOTE/otel/semantic-roster.json" > "$TD/roster.json"
python3 - "$TD/roster.json" "$TD/tampered.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
d["semantic_layers"]["bogus_l1"] = d["semantic_layers"].pop("invoke_workflow")
d["semantic_layers"]["bogus_l1"]["layer"] = "L1"
d["order_domain_keys"] = ["af.order_id"]
json.dump(d, open(sys.argv[2], "w"), ensure_ascii=False)
PY

note "== 绿路：三面消费同一正本字节 =="
ssh "$ANOLIS" "cd $WT_REMOTE && PATH=\$HOME/.cargo/bin:\$PATH \
  CARGO_TARGET_DIR=/data/bench/targets/a1-semantic-chain \
  cargo test -p af-execline --test span_produce \
  roster_contract_pins_layers_and_order_domain_keys -- --exact" \
  > "$TD/crate-green.txt" 2>&1
rc=$?; note "crate-green    rc=$rc"; [ "$rc" -eq 0 ] || fail=1

AF_SEMANTIC_ROSTER="$TD/roster.json" python3 -I \
  "$WS/tools/otel/af-otel-projector.py" selftest > "$TD/projector-green.txt" 2>&1
rc=$?; note "projector-green rc=$rc"; [ "$rc" -eq 0 ] || fail=1

AF_SEMANTIC_ROSTER="$TD/roster.json" python3 -I \
  "$WS/tools/otel/otlp-assert.py" --selftest > "$TD/assertor-green.txt" 2>&1
rc=$?; note "assertor-green rc=$rc"; [ "$rc" -eq 0 ] || fail=1

note "== 红路：篡改正本 ⇒ 三面全红 =="
scp -q "$TD/roster.json" "$ANOLIS:/tmp/a1-roster-save.json"
scp -q "$TD/tampered.json" "$ANOLIS:/tmp/a1-tampered-roster.json"
ssh "$ANOLIS" "cd $WT_REMOTE && cp /tmp/a1-tampered-roster.json \
    otel/semantic-roster.json && PATH=\$HOME/.cargo/bin:\$PATH \
  CARGO_TARGET_DIR=/data/bench/targets/a1-semantic-chain \
  cargo test -p af-execline --test span_produce \
  roster_contract_pins_layers_and_order_domain_keys -- --exact" \
  > "$TD/crate-red.txt" 2>&1
rc=$?; note "crate-red      rc=$rc（期望非0）"; [ "$rc" -ne 0 ] || fail=1
# 复原：cp 回真字节 + 摘要比对（正本未 commit 时 git checkout 不适用；commit 后
# 该行同样成立——cp 复原是字节级复原，与版本面无关）。
ssh "$ANOLIS" "cd $WT_REMOTE && cp /tmp/a1-roster-save.json \
    otel/semantic-roster.json \
  && sha256sum otel/semantic-roster.json /tmp/a1-roster-save.json \
  && rm -f /tmp/a1-tampered-roster.json /tmp/a1-roster-save.json"

AF_SEMANTIC_ROSTER="$TD/tampered.json" python3 -I \
  "$WS/tools/otel/af-otel-projector.py" selftest > "$TD/projector-red.txt" 2>&1
rc=$?; note "projector-red  rc=$rc（期望非0）"; [ "$rc" -ne 0 ] || fail=1

AF_SEMANTIC_ROSTER="$TD/tampered.json" python3 -I \
  "$WS/tools/otel/otlp-assert.py" --selftest > "$TD/assertor-red.txt" 2>&1
rc=$?; note "assertor-red   rc=$rc（期望非0）"; [ "$rc" -ne 0 ] || fail=1

note "== 读数摘录 =="
grep -E 'test result:' "$TD/crate-green.txt" "$TD/crate-red.txt" || true
grep -hE 'selftest:' "$TD/projector-green.txt" "$TD/projector-red.txt" || true
grep -hE 'FATAL|名册键集|order_domain_keys' "$TD/assertor-red.txt" | head -4 || true
note "FIXTURE_RESULT: $([ "$fail" -eq 0 ] && echo PASS || echo FAIL)"
exit "$fail"
