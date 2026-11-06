#!/usr/bin/env bash
# poc-admit-seal-roles — admit_seal 补 --decision/--role 面的真跑驱动。
#
# 一次跑全量留证：
#   ⓪ 构建 workspace（bins + examples）
#   ① 真实 CLI submit 造源单（stub 单元，真引擎门控，全生命周期至 Closed）
#   ② 三条路径真签 + 真受理：reject×approver(seq7) / revise×reviser(seq8) /
#      reject×arbiter(seq9)，外加缺省回归（不给 --decision/--role = 旧行为
#      approve×approver，seq10）——四笔全部经 af-order admit 真引擎四层验签
#   ③ 工具面非法配对拒绝（approver×revise / reviser×reject）+ 取值面拒绝
#      （--decision arbitrate / --role reader）：exit 2、中文报错、零写出
#   ④ 独立对账（python3 -I）：admission.json 工件字段 vs 所选 role/decision、
#      回执 decision 与所选一致、admissions.jsonl 行数/seq 单调/decision 序列
#   ⑤ 证据 sha256 清单 + run-meta.json
#
# 复现：
#   export PATH="$HOME/.cargo/bin:$PATH" LANG=C.UTF-8
#   CARGO_TARGET_DIR=/data/bench/af-target bash poc/poc-admit-seal-roles/run_poc.sh
# （CARGO_TARGET_DIR 可换任意构建盘；家目录 18G 装不下 target，勿用默认。）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
POC="$ROOT/poc/poc-admit-seal-roles"
RUN="$POC/run"
TARGET="${CARGO_TARGET_DIR:-/data/bench/af-target}"
export PATH="$HOME/.cargo/bin:$PATH"
export LANG=C.UTF-8
export CARGO_TARGET_DIR="$TARGET"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-8}"

ORDER_ID="wo-asr-poc-1"
NOW="2026-11-04T00:00:00Z"        # admit 受理时刻（--now 钉死，可复现）
EXPIRES="2027-01-01T00:00:00Z"    # 决策有效期（晚于 NOW，避开 OR2108）

AF_ORDER="$TARGET/debug/af-order"
ADMIT_SEAL="$TARGET/debug/examples/admit_seal"

log() { printf '[run_poc] %s\n' "$*" >&2; }

rm -rf "$RUN"
mkdir -p "$RUN/logs" "$RUN/state" "$RUN/admissions" "$RUN/tmp"

# ── ⓪ 构建 ────────────────────────────────────────────────────
cargo build --workspace --examples >"$RUN/logs/build.log" 2>&1
log "0. build ok（target=$TARGET）"

# ── ① 源单：真实 CLI submit 全生命周期 ─────────────────────────
cat >"$RUN/spec.json" <<EOF
{"order_id":"$ORDER_ID","line":{"line_id":"line-poc","line_version":"lv-1"},"budget":100,"note":"poc-admit-seal-roles 源单"}
EOF
"$AF_ORDER" submit --spec "$RUN/spec.json" --state "$RUN/state" \
    --workspace "$ROOT" --target-dir "$TARGET" --unit stub \
    >"$RUN/logs/00-submit.out" 2>"$RUN/logs/00-submit.err"
"$AF_ORDER" show "$ORDER_ID" --state "$RUN/state" >"$RUN/logs/01-show.out" 2>"$RUN/logs/01-show.err"
log "1. 源单 $ORDER_ID 已 submit（见 logs/00-submit.out / 01-show.out）"

SEAL_COMMON=(--state "$RUN/state" --order-id "$ORDER_ID"
             --actor-id user-a --expires "$EXPIRES" --next-orders "$ORDER_ID-rw")
ADMIT_COMMON=(--state "$RUN/state" --workspace "$ROOT" --target-dir "$TARGET" --now "$NOW")

# ── ② 三条路径 + 缺省回归：真签 → 真受理 ────────────────────────
seal_and_admit() { # <name> <seq> <seed-byte-hex-char> <decision> <role>
    local name="$1" seq="$2" seed_char="$3" decision="$4" role="$5"
    local seed
    seed="$(printf "${seed_char}%.0s" $(seq 64))"
    "$ADMIT_SEAL" "${SEAL_COMMON[@]}" --seq "$seq" --seed-hex "$seed" \
        --decision "$decision" --role "$role" \
        --out "$RUN/admissions/$name.json" \
        >"$RUN/logs/seal-$name.out" 2>"$RUN/logs/seal-$name.err"
    "$AF_ORDER" admit --request "$RUN/admissions/$name.json" "${ADMIT_COMMON[@]}" \
        >"$RUN/logs/admit-$name.out" 2>"$RUN/logs/admit-$name.err"
    log "2. 路径 $name ok（--decision $decision --role $role --seq $seq → 受理回执见 logs/admit-$name.out）"
}

seal_and_admit reject-approver 7 1 reject  approver
seal_and_admit revise-reviser  8 2 revise  reviser
seal_and_admit reject-arbiter  9 3 reject  arbiter
seal_and_admit default-approve 10 4 approve approver   # 不给 --decision/--role 的缺省回归（旧行为）

# ── ③ 拒绝面：非法配对 + 非法取值 ──────────────────────────────
expect_reject() { # <name> <期望报错关键词> <额外 admit_seal 参数...>
    local name="$1" needle="$2"
    shift 2
    local rc=0 out="$RUN/tmp/SHOULD-NOT-EXIST-$name.json"
    "$ADMIT_SEAL" "${SEAL_COMMON[@]}" --seq 99 --seed-hex "$(printf 'ee%.0s' $(seq 64))" \
        --decision approve --role approver "$@" --out "$out" \
        >"$RUN/logs/neg-$name.out" 2>"$RUN/logs/neg-$name.err" || rc=$?
    printf '%s\n' "$rc" >"$RUN/logs/neg-$name.rc"
    [[ "$rc" -eq 2 ]] || { log "FAIL: $name 期望 exit 2，实得 $rc"; exit 1; }
    [[ ! -e "$out" ]] || { log "FAIL: $name 不应写出工件（$out 存在）"; exit 1; }
    grep -qF "$needle" "$RUN/logs/neg-$name.err" \
        || { log "FAIL: $name 报错应含「$needle」"; cat "$RUN/logs/neg-$name.err" >&2; exit 1; }
    log "3. 拒绝面 $name ok（exit 2、零写出；见 logs/neg-$name.err）"
}

# 非法配对（根本法角色阶梯 × Role::allowed_permissions 既有映射）：
#   approver 无 Revise 权；reviser 无 Approve 权（Reject 与 Approve 同权）。
expect_reject approver-revise "非法配对" --decision revise
expect_reject reviser-reject  "非法配对" --decision reject --role reviser
# 非法取值：--decision 只收 approve|revise|reject（arbitrate 归裁决环生产面）；
#           --role 只收 approver|reviser|arbiter（reader 只读，不开放）。
expect_reject decision-arbitrate "只接受 approve|revise|reject" --decision arbitrate
expect_reject role-reader "只接受 approver|reviser|arbiter" --role reader

# ── ④ 独立对账 ────────────────────────────────────────────────
python3 -I - "$RUN" "$ORDER_ID" <<'PYEOF' >"$RUN/verify.out" 2>"$RUN/verify.err"
import json
import sys
from pathlib import Path

run = Path(sys.argv[1])
order_id = sys.argv[2]

def load(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))

# 工件面（签出的 role/decision/permissions 与所选一致）+ 受理面（回执 decision
# 与所选一致、11 项 checks、真引擎版本）。
expect = {
    "reject-approver": ("Reject", "Approver", ["Read", "Approve"]),
    "revise-reviser": ("Revise", "Reviser", ["Read", "Revise"]),
    "reject-arbiter": ("Reject", "Arbiter", ["Read", "Approve", "Revise", "Arbitrate"]),
    "default-approve": ("Approve", "Approver", ["Read", "Approve"]),
}
for name, (decision, role, perms) in expect.items():
    adm = load(run / "admissions" / f"{name}.json")
    art = adm["decision"]
    assert art["decision"] == decision, (name, "artifact.decision", art["decision"])
    assert art["role"] == role, (name, "artifact.role", art["role"])
    assert art["permissions"] == perms, (name, "artifact.permissions", art["permissions"])
    reg_role = adm["actor_registry"]["actors"][0]["role"]
    assert reg_role == role, (name, "registry.role", reg_role)
    receipt = load(run / "logs" / f"admit-{name}.out")
    assert receipt["decision"] == decision, (name, "receipt.decision", receipt["decision"])
    assert receipt["replayed"] is False, (name, "replayed")
    assert len(receipt["checks"]) == 11, (name, receipt["checks"])
    assert receipt["checks"][-1] == "credential_intersection", receipt["checks"]
    print(f"PASS {name}: 工件 role={role} decision={decision} permissions={perms}"
          f"；注册表 role={reg_role}；回执 decision={receipt['decision']}"
          f"，{len(receipt['checks'])} checks，engine={receipt['engine_version']}")

# 受理账本面：4 笔、seq 单调、decision 序列与路径一致。
lines = [json.loads(l) for l in
         (run / "state" / "orders" / order_id / "admissions.jsonl").read_text().splitlines()
         if l.strip()]
seqs = [l["decision_seq"] for l in lines]
decisions = [l["decision"] for l in lines]
assert seqs == [7, 8, 9, 10], ("seq 单调", seqs)
assert decisions == ["Reject", "Revise", "Reject", "Approve"], ("decision 序列", decisions)
assert all(l["order_id"] == order_id for l in lines), lines
print(f"PASS admissions.jsonl：{len(lines)} 笔，seq={seqs} 单调，decision={decisions}")

print("VERIFY-ALL-PASS")
PYEOF
grep -q "VERIFY-ALL-PASS" "$RUN/verify.out"
log "4. 独立对账全过（见 verify.out）"

# ── ⑤ 证据清单 + 元数据 ───────────────────────────────────────
git -C "$ROOT" status --porcelain -- crates/ poc/poc-admit-seal-roles >"$RUN/logs/git-status-poc.txt" 2>&1 || true
# 凭证签名种子（voucher-key.seed）是密钥材料：不入清单、不入库（.gitignore
# 同排除；与 poc-router-adjudicate 同纪律）。
( cd "$POC" && find run -type f ! -name 'evidence-sha256.txt' \
      ! -name 'voucher-key.seed' | LC_ALL=C sort \
    | xargs sha256sum > run/evidence-sha256.txt )
cat >"$RUN/run-meta.json" <<EOF
{
  "poc": "poc-admit-seal-roles",
  "host": "$(hostname)",
  "date_utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_head": "$(git -C "$ROOT" rev-parse HEAD)",
  "git_branch": "$(git -C "$ROOT" rev-parse --abbrev-ref HEAD)",
  "rustc": "$("$HOME/.cargo/bin/rustc" --version)",
  "af_order_bin": "$AF_ORDER",
  "admit_seal_bin": "$ADMIT_SEAL",
  "order_id": "$ORDER_ID",
  "now": "$NOW",
  "expires": "$EXPIRES"
}
EOF
log "5. 证据收口：run/evidence-sha256.txt + run/run-meta.json"
log "ALL OK"
