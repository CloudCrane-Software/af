#!/usr/bin/env bash
# =============================================================================
# poc/poc-g3-manifest/coverage-gate/run_probes.sh — G3 任务包组包覆盖面门·破窗自证（可复现）
#
# 在一次性干净克隆面（git archive origin/product/main + 本批新件覆盖）上逐个
# 制造破窗 → 跑门 → 落读数 → 回收探针 → 终读复绿。读数落
# poc/poc-g3-manifest/coverage-gate/readings/（覆盖写入）。探针只落在克隆面——真仓零接触。
#
# 用法：bash poc/poc-g3-manifest/coverage-gate/run_probes.sh [克隆目录] [读数目录]
#   克隆目录缺省 /tmp/g3-probe-clone；读数目录缺省 poc/poc-g3-manifest/coverage-gate/readings
#   CARGO_TARGET_DIR 缺省 /data/bench/targets/g3-probe（可外置复用暖缓存）
# =============================================================================
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
CLONE="${1:-/tmp/g3-probe-clone}"
READ="${2:-$HERE/readings}"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-8}"
# 探针构建 target：强制独占（不继承外层 CARGO_TARGET_DIR——会话管理器可能已为
# 本工作树预种；探针克隆与真树共用 target 会被 provenance ensure 判脏即清，
# 互掀构建产物——2026-11-04 实证：并发 cargo test 被 ensure 清掉二进制 exit 101）。
# 覆写口：PROBE_CARGO_TARGET_DIR。
export CARGO_TARGET_DIR="${PROBE_CARGO_TARGET_DIR:-/data/bench/targets/g3-probe}"
export LANG=C.UTF-8
GATE_REL=ci/task_package_manifest_check.py
REG_REL=ci/baselines/task-package-writepoints.json

mkdir -p "$READ"

# —— 建面：干净克隆 + 本批新件覆盖（模拟合流后盘面） ——
rm -rf "$CLONE"; mkdir -p "$CLONE"
git -C "$REPO" archive HEAD | tar -x -C "$CLONE" || exit 2
cp "$REPO/ci/task_package_manifest_check.py" "$REPO/ci/test_task_package_manifest_check.py" "$CLONE/ci/"
cp "$REPO/ci/baselines/task-package-writepoints.json" "$CLONE/ci/baselines/"
cp "$REPO/ci/run.sh" "$CLONE/ci/"

run_gate() { # run_gate <名> <模式...>
  local name="$1"; shift
  python3 -I "$CLONE/$GATE_REL" "$@" --root "$CLONE" > "$READ/$name.out" 2>&1
  local rc=$?
  echo "exit=$rc" >> "$READ/$name.out"
  echo "[$name] exit=$rc（读数 readings/$name.out）"
  return 0
}

pin_in_clone() { # 克隆面注册表重钉（探针用；真仓钉在脚本常量里）
  python3 - "$CLONE" <<'PY'
import hashlib, re, sys
import sys
from pathlib import Path
clone = Path(sys.argv[1])
reg = clone / "ci/baselines/task-package-writepoints.json"
sha = hashlib.sha256(reg.read_bytes()).hexdigest()
script = clone / "ci/task_package_manifest_check.py"
s = script.read_text(encoding="utf-8")
s2 = re.sub(r'REGISTRY_SHA256 = "[0-9a-f]{64}"', f'REGISTRY_SHA256 = "{sha}"', s)
assert s2 != s, "钉常量替换未命中"
script.write_text(s2, encoding="utf-8")
PY
}

echo "== 0) 正常盘面（克隆面冷跑全量：静态三段 + 真实组包回环）"
run_gate normal check

echo "== P1) 有生产写入点未覆盖（核心腿：未造册写入点）"
cat >> "$CLONE/crates/af-station/src/lib.rs" <<'EOF'

/// G3 破窗探针 P1：未造册的新生产写入点（写 API 面：create_dir_all）。
pub fn g3_probe_unregistered_writer(dir: &std::path::Path) {
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(dir.join("probe-out.json"), "{}");
}
EOF
run_gate p1-uncovered-site check
python3 - "$CLONE" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1]) / "crates/af-station/src/lib.rs"
t = p.read_text(encoding="utf-8")
m = "\n/// G3 破窗探针 P1"
assert m in t
p.write_text(t[:t.index(m)] + "\n", encoding="utf-8")
PY

echo "== P2) manifest 有条目但盘面没有产物（注册表多造 meta.json + 同批重钉）"
python3 - "$CLONE" <<'PY'
import json
import sys
from pathlib import Path
reg = Path(sys.argv[1]) / "ci/baselines/task-package-writepoints.json"
doc = json.loads(reg.read_text(encoding="utf-8"))
for e in doc["entries"]:
    if e["fn"] == "assemble_construction_package":
        e["artifacts"] = ["plan.json", "task.json", "meta.json"]
reg.write_text(json.dumps(doc, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY
pin_in_clone "$CLONE"
run_gate p2-phantom-entry check
cp "$REPO/$REG_REL" "$CLONE/$REG_REL"; cp "$REPO/$GATE_REL" "$CLONE/$GATE_REL"

echo "== P3) 盘面有产物但 manifest 里没有（组包接线真多写 meta.json）"
python3 - "$CLONE" <<'PY'
import sys
from pathlib import Path
c = Path(sys.argv[1]) / "crates/af-compiler/src/construction.rs"
t = c.read_text(encoding="utf-8")
anchor = "    let task = task_json(spec, &plan)?;"
probe = "    let _ = std::fs::write(task_dir.join(\"meta.json\"), \"{}\\n\"); // G3 破窗探针 P3\n"
assert anchor in t
c.write_text(t.replace(anchor, probe + anchor, 1), encoding="utf-8")
PY
run_gate p3-extra-artifact check
python3 - "$CLONE" <<'PY'
import sys
from pathlib import Path
c = Path(sys.argv[1]) / "crates/af-compiler/src/construction.rs"
t = c.read_text(encoding="utf-8")
probe = "    let _ = std::fs::write(task_dir.join(\"meta.json\"), \"{}\\n\"); // G3 破窗探针 P3\n"
assert probe in t
c.write_text(t.replace(probe, "", 1), encoding="utf-8")
PY

echo "== P4) 产物内容与 manifest 身份不符（真组包 → 篡改 → verify-dir）"
rm -rf "$CLONE/.g3-p4"; mkdir -p "$CLONE/.g3-p4"
(cd "$CLONE" && cargo run --quiet -p af-compiler --bin af-compiler-construct -- \
  --registry af/packages --task-id G3-P4-PROBE \
  --statement "P4 身份不符探针" --acceptance "verify-dir 红" \
  "$CLONE/.g3-p4/task") > "$READ/p4-assemble.out" 2>&1
echo "exit=$?" >> "$READ/p4-assemble.out"
run_gate p4-verify-green verify-dir "$CLONE/.g3-p4/task"
printf '{"task_id": "G3-P4-PROBE", "tampered": true}\n' > "$CLONE/.g3-p4/task/task.json"
run_gate p4-identity-mismatch verify-dir "$CLONE/.g3-p4/task"
rm -rf "$CLONE/.g3-p4"

echo "== P5) 注册表被改而未重钉（改判据即改门）"
python3 - "$CLONE" <<'PY'
import json
import sys
from pathlib import Path
r = Path(sys.argv[1]) / "ci/baselines/task-package-writepoints.json"
doc = json.loads(r.read_text(encoding="utf-8"))
doc["entries"] = [e for e in doc["entries"] if e["fn"] != "issue_or_load"]
r.write_text(json.dumps(doc, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY
run_gate p5-registry-tamper check
cp "$REPO/$REG_REL" "$CLONE/$REG_REL"

echo "== 终读) 全探针回收后复绿"
run_gate final-green check

echo "== 读数判定（0=绿 1=红）："
for f in normal p1-uncovered-site p2-phantom-entry p3-extra-artifact \
         p4-verify-green p4-identity-mismatch p5-registry-tamper final-green; do
  printf '  %-24s %s\n' "$f" "$(tail -1 "$READ/$f.out")"
done
