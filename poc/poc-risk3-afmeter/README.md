# PoC 留证 · 风险#3：af-meter Concurrency 数量路径 no-op

> 分支 `feat/risk3-afmeter-concurrency`（**未 commit / 未 push**）。
> 基线 = 开工时 `origin/product/main` = `bc1ef7d9eb2e1f3466536f8c9e58af75249429a3`。
> 工作树 `/home/pan-ding/af-dev/af/wt/risk3-afmeter-concurrency`。
> 上轮 grok 会话修完 `crates/af-meter/src/lib.rs`（+183/−24）后被截断；
> 本目录是续完会话的自足留证：侦查、缺陷单在位性、根因、先红后绿、四门。

## 一、侦查结论（缺陷单在位，口径与盘面对得上）

欠账正本（任务书引用 `30-工件/剩余欠账总表-20261104/剩余欠账总表.md` §1）：

> v0.3-风险#3 | af-meter Concurrency no-op 真 bug 修复进缺陷单（首版配方绕开）
> | af 内核面 | 未核 | 方案:37,124 | 下一步：**查缺陷单在位性后派修复**

| 载体 | 坐标 | 写的什么 |
|---|---|---|
| 缺陷单 | pandev `30-工件/janitor-20261103/pocg-meter-bug.md`（2026-11-03） | Concurrency 嵌套 `deduct` 是 no-op；`try_consume` 先查 available 再 `deduct`；Concurrency 分支只有注释；Min 会递归进空分支；`acquire` 不在这条路上。数量扣减碰到 Concurrency 必须拒绝（不许静默成功），或把 deduct 与 acquire/release 打通。先加 Min/Max/route 回归，再抬配方限制。 |
| 方案 | pandev `计划/产品化工作方案-v0.3-20261103.md:37`、`:124`（`:105` 配方规避） | Meter Concurrency 在 deduct 是 no-op（嵌套 Min 记账失效）；首版配方用 Stock 模拟并发上限；修复进缺陷单。 |
| 欠账表 | `30-工件/剩余欠账总表-20261104/剩余欠账总表.md` §1 | 见上。 |

本 anolis 工作树无 `30-工件/`（未入库）。缺陷单全文由上轮本工作树 grok 会话连 pandev 读出；本续完会话按该坐标与盘面交叉核对，**口径仍对得上**：

- HEAD `crates/af-meter/src/lib.rs`：`try_consume` :372-373 available 够就 `self.deduct(need); Ok(())`；`deduct` :410-412 `Meter::Concurrency { .. } => { /* 注释 */ }`；Min :413-416 递归进空分支；`route` :468-469 available 够就 `deduct` 并 `Use`。摘录见 `before/head-deduct-noop.txt`。
- 配方规避仍在仓内：`poc/poc-supply/validate_machines.py:193`（`kind != "Stock"` 即拒，文案 "never nested Concurrency"）、同目录 README:55、`tests/test_registry_validation.py` 的 `concurrency_meter_forbidden`。

选定缺陷单第一方案：**数量扣减命中 Concurrency 拒绝，不许静默成功**（`MeterError::Unsupported`）。未把 deduct 与 acquire/release 打通，也未抬 PoC-G 配方限制（缺陷单写明测试齐后再抬）。

## 二、根因（修前 HEAD `bc1ef7d`）

文件：`crates/af-meter/src/lib.rs`

1. **`try_consume` :372-373**：available 够就 `deduct` 再 `Ok(())`，Concurrency 走同一条数量路径。
2. **`deduct` :410-412**：`Meter::Concurrency { .. } => { /* 并发占用走 acquire/release，不走数量扣减。 */ }`，不占槽、不报错。
3. **`deduct` Min :413-416**：递归进上述空分支；兄弟 Stock 仍被扣。
4. **`route` :468-469**：`available >= need` 即 `deduct` 并返回 `Use`。Concurrency 的 available 是空槽数，于是 `route` 把并发供给当成已扣减成功（先红实测 `Use { supply: "slots", remaining_after: 2 }`）。

修后（工作区）：`quantity_consume_unsupported`（`:403`）在任何子计量改写前走树；Min 任一叶子命中即整笔拒绝；Max 只看被选中的孩子。`deduct` 改为 `Result`（`:441`），Concurrency 路径返回 `Unsupported("try_consume 不适用于 Concurrency 节点")`。`try_consume` 直接返回该 Result（`:372`）。`route`（`:499`）`deduct` 失败则跳过该供给，继续找下一个 / `Defer`。

## 三、失败测试：先红后绿

钉面 6 条新测试（既有 12 条零删，lib.rs `#[test]` 12→18），全部在 `crates/af-meter/src/lib.rs` `mod tests`：

| # | 测试 | 钉什么 |
|---|---|---|
| 1 | `try_consume_on_concurrency_is_unsupported_and_does_not_occupy` | 叶子 Concurrency 数量路径 fail-closed，槽位不动 |
| 2 | `min_try_consume_rejects_nested_concurrency_and_leaves_stock` | Min 遇 Concurrency 整笔拒绝，兄弟 Stock 不被扣 |
| 3 | `max_try_consume_rejects_when_concurrency_is_chosen` | Max 选中 Concurrency 时拒绝 |
| 4 | `max_try_consume_still_deducts_stock_when_stock_is_chosen` | Max 选中 Stock 仍扣减（回归；修前已绿） |
| 5 | `route_does_not_use_concurrency_via_quantity_deduct` | route 不得经 deduct 把 Concurrency 当成 Use |
| 6 | `route_skips_nested_concurrency_and_uses_stock_fallback` | 嵌套 Concurrency 供给被跳过，落到后备 Stock |

上轮已把测试写进 lib.rs，修复已使它们绿。本续完按任务书补「先红」读数：

1. 抽出 6 条钉面，拼进 `git show HEAD:crates/af-meter/src/lib.rs`（生产仍 no-op）；
2. `cargo test --offline -p af-meter --lib` → **红**；
3. 恢复工作树修复版 → **绿**。

| 阶段 | 读数 | 日志 |
|---|---|---|
| **红**（测试已插入、生产仍 no-op） | **32 passed / 5 failed / 0 ignored**，exit 101。红的 5 条即 1–3、5–6。冒烟：`route` 对 Concurrency 返回 `Use { supply: "slots", remaining_after: 2 }`；Min 嵌套被当成 `Use { supply: "nested" }`。第 4 条在 HEAD 上已绿。 | `before/lib-tests-red.txt`、`before/probe-before.txt`、`before/pin-tests-red-filtered.txt` |
| **绿**（fail-closed 之后） | `--lib` **37 passed / 0 failed / 0 ignored**；crate 全目标 lib 37 + bifrost 3 + ledger 18 + properties 4 + routing 5 + vocab 1 + doctest 0。 | `after/lib-tests-green.txt`、`after/crate-tests-green.txt` |

## 四、改动面

| 文件 | 行数 | 说明 |
|---|---|---|
| `crates/af-meter/src/lib.rs` | +183 / −24 | 数量路径 fail-closed：`quantity_consume_unsupported` + `chosen_max_child`；`deduct`→`Result`；`try_consume`/`route` 接住拒绝；6 条钉面测试。 |

未动：既有 12 条测试函数、既有 assert 行、`ci/`、`Cargo.toml`/`Cargo.lock`、`ci/clippy-baseline.txt`、crates 集合、PoC-G 配方限制。

## 五、四门（本续完会话实测）

详见 `after/gates.txt`。摘要：

| 门 | 命令 | exit | 读数 |
|---|---|---|---|
| 1 fmt | `cargo fmt --all -- --check` | 0 | 无 diff |
| 2 test | `cargo test --offline --workspace --no-fail-fast` | 0 | **1592 passed / 0 failed / 9 ignored**（159 套件） |
| 3 clippy | `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 零告警 |
| 3 json+gate | touch 全源强制重检 → `--message-format=json` → `python3 -I ci/clippy_gate.py` | 0 | compiler-message errors=0 warnings=0；**CLIPPY GATE: PASS**（0 条警告 / 0 签名，基线 0 签名未动） |
| 4 gate_inputs | `bash ci/gate_inputs_check.sh check` | 0 | 14 件钉对账一致 |

归因（相对任务书钉 1578/0/9）：failed=0、ignored=9 未变；passed +14 = **本施工 +6** + **HEAD 已含 risk5 A1–A8 +8**。

`ci/clippy-baseline.txt` sha256 `a48639e75b5203fef7e1ffc7026b8f0d9de20f3ce862fa3988a1ec6b49326ba4` 未变。

## 六、零弱化核

- 既有 12 条测试函数全部保留，**0 条删除**。
- diff：−24 / +183；**既有 assert 行删除 = 0**；新增 assert 只在 6 条新测试里。
- 只动 `crates/af-meter/src/lib.rs`。`ci/`、`Cargo.toml`/`Cargo.lock`、基线钉未动。
- 未抬 PoC-G 配方限制。

## 七、边界（如实登记）

1. **语义**：数量路径 fail-closed；Concurrency 占用仍走 `acquire`/`release`。未把两条操作打通（缺陷单第一方案）。
2. **Max 选中 Stock** 时照常扣减（测试 4 钉住）。
3. **Min** 在改任何孩子之前整笔拒绝，兄弟 Stock 不被扣。
4. **`route`** 对 Unsupported 供给跳过，可落到后续 Stock。
5. **PoC-G 配方**仍强制并发上限用 Stock（`validate_machines.py:193`）。缺陷单写明测试齐后再抬，本批不动配方。
6. 多消费者并发安全、持久化，沿用 crate 既有边界（`RESULT.md` ⑤），本批未扩。
7. 工作区另有未跟踪 `.offload/`、`out/clippy.jsonl`（上轮门禁输出），不在交付面。
8. **未 commit、未 push、未 sudo**。判据件零改动。

## 八、复现

```bash
export PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR=/tmp/risk3-afmeter-concurrency-target
export CARGO_BUILD_JOBS=8 LANG=C.UTF-8
unset -f which; unset 'BASH_FUNC_which%%'

# 修后绿
cargo test --offline -p af-meter --lib

# 先红协议（临时还原生产码、保留 6 条钉面）：
#   见 before/probe-before.txt；测完必须恢复 crates/af-meter/src/lib.rs

cargo fmt --all -- --check
cargo test --offline --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --message-format=json \
  > poc/poc-risk3-afmeter/raw/gate3-clippy.jsonl
python3 -I ci/clippy_gate.py poc/poc-risk3-afmeter/raw/gate3-clippy.jsonl
bash ci/gate_inputs_check.sh check
```
