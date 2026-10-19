# RESULT-UPDATE — F3 · fuzz/ 锁冲突与旧 API

## 结论

通过。`fuzz/Cargo.lock` 里的 47 段冲突标记已删掉，由 cargo 重新生成，cargo 可以解析。`af_effect_gateway_idempotency` 与 `af_workorder_order_parse` 按当前公共 API 调用：`declare_intent` / `execute` 递交 `WorkOrder`，声明成功时快照的 `declared_order_state` 等于声明当时的订单态，之后订单再迁移不改写。两个 target 各 2000 次、`--sanitizer none`、退出码 0、无 panic，`fuzz/artifacts` 下 0 个崩溃样本。

`cargo check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 全绿。fuzz 包测试 6 → 14，多出来的 8 个是本次回归。未改任何产品 crate，未新增第三方 crate。根 `Cargo.lock` 无 diff。

## 步骤

1. 先读 `crates/af-effect-gateway/RESULT.md` 与 `RESULT-UPDATE.md`、`crates/af-workorder/RESULT.md` 与 `RESULT-UPDATE.md`，以及图 / 抽样的 RESULT。不改已验证语义：声明只允许 Released / InProgress，执行发送只允许 InProgress，已执行只重放；`stable_id` 创建时生成，缺字段走拒绝式信封，不补默认身份；`continue_as_new` 只换 `run_id`；不调用 `Transition::from_state`（Cancel 在那里是 `unreachable!`）。
2. 复现锁文件。`fuzz/Cargo.lock` 从第 6 行起是 `<<<<<<< HEAD`，`^<<<<<<<` 共 47 行。`cd fuzz && cargo check` 在解析锁时失败，退出码 101。
3. 删掉 `fuzz/Cargo.lock`，让 cargo 按独立工作区重新生成。清单补上源码已经在用、但合并时丢掉的路径依赖 `af-graph`、`af-sampling`，并登记已有的 `af_sampling_invariant_input` bin。没有新的 crates.io 依赖。`libfuzzer-sys` 仍解析为 0.4.13。
4. 锁能解析之后，复现旧签名。`af_effect_gateway_idempotency` 上 7 处 `declare_intent` 和 1 处 `execute` 报 E0061，缺 `&WorkOrder`。这是修之前的红。
5. 两个 harness 的逻辑收进 `fuzz/src/gateway.rs` 与 `fuzz/src/workorder.rs`，bin 只转发。网关按字节选订单态，声明时把这张 `WorkOrder` 递进去；同载荷两把键的 `payload_sha256` 必须相同，快照订单态必须等于声明当时的 `order.state()`，重复声明、开工、执行、完成之后都不改写。工单侧走 `decode_work_order`，缺 `stable_id` 只得到拒绝式信封；`new` 的身份等于 `StableId::generate(id)`；门控和 `continue_as_new` 不改 `stable_id` / `graph_version`。时刻用 `ManualClock` 和钳过的 `DateTime<Utc>`，没有 `sleep`。
6. `fuzz/.gitignore` 里另有两段冲突标记，其中一行会把 `corpus` 忽略掉。按 C3 已验证的口径改回：忽略 `target` / `artifacts` / `coverage`，种子可以入库。树上原来没有 corpus。补回现有测试点名的三份种子（`min_legal.json`、`orphan.json`、`min_recipe_one_event`），否则那 6 个旧测试在干净检出里会读文件失败。冒烟长出来的 corpus 没有入库。

提交：`874024f`。

## 证据

工具链：`source /data/af-factory/env.sh` 之后 `rustc 1.99.1 (58cf8f9c3 2026-10-13)`、`cargo 1.99.1`。fuzz 的 check / clippy / test / run 使用 `CARGO_TARGET_DIR=/tmp/f3-fuzz-target`，避免和共享 `target` 抢锁。工作目录是 `/data/af-factory/fx3`。

红（修之前）：

```text
cd fuzz && cargo check --offline -j 4
error: failed to parse lock file at: .../fuzz/Cargo.lock
TOML parse error at line 6, column 9
  6 | <<<<<<< HEAD
    |         ^
  key with no value, expected `=`
EXIT:101
```

`grep -c '^<<<<<<<' fuzz/Cargo.lock` = 47。

锁重新生成、target 尚未改时：

```text
error[E0061]: this method takes 4 arguments but 3 arguments were supplied
 --> fuzz_targets/af_effect_gateway_idempotency.rs:39:30
  (同样的缺参还有 :40 :42 :56 :59 :66 :72 的 declare_intent，以及 :61 的 execute)
  argument #4 of type `&af_workorder::order::WorkOrder` is missing
error: could not compile `af-fuzz` (bin "af_effect_gateway_idempotency") due to 8 previous errors
EXIT:101
```

绿：

```text
cargo fmt --manifest-path fuzz/Cargo.toml
cargo clippy --manifest-path fuzz/Cargo.toml --all-targets --offline -j 4 -- -D warnings
# Finished `dev` profile ... af-fuzz in 0.22s
# CLIPPY:0，无警告行

cargo check --manifest-path fuzz/Cargo.toml --offline -j 4
# Finished `dev` profile ... in 0.22s
# CHECK:0

cargo test --manifest-path fuzz/Cargo.toml --offline -j 4
# running 14 tests
# graph 4、sampling 2、lock 2、gateway 3、workorder 3，全部 ok
# test result: ok. 14 passed; 0 failed; 0 ignored
# Doc-tests af_fuzz: 0 passed
# TEST:0
```

计数：修之前能编过的 lib 测试是 C3 留下的 6 个（图 4、抽样 2）。本次新增 8 个，所以是 14。旧 6 个断言没改。

```text
cargo fuzz run --sanitizer none af_workorder_order_parse -- -runs=2000
# #2000 DONE   cov: 1209 ft: 2182 corp: 166/505b lim: 4 exec/s: 0 rss: 35Mb
# Done 2000 runs in 0 second(s)
# WORKORDER:0

cargo fuzz run --sanitizer none af_effect_gateway_idempotency -- -runs=2000
# #2000 DONE   cov: 910 ft: 1296 corp: 85/252b lim: 4 exec/s: 2000 rss: 35Mb
# Done 2000 runs in 1 second(s)
# GATEWAY:0

find fuzz/artifacts -type f | wc -l
# 0
```

两次都有三行 `WARNING: Failed to find function "__sanitizer_*"`（`--sanitizer none` 不链接 ASan）。`cargo-fuzz` 0.13.2 启动时打印 `GLIBC_2.39` weak version not found。进程都以 0 退出，没有 panic。

`git diff -- Cargo.lock Cargo.toml`（仓库根）无输出。`grep` 冲突标记：`fuzz/` 下 0 处。

## 反馈

1. 这两处缺陷是编译期的，不是运行期断言失败。锁文件解析失败时 `cargo test` 起不来；签名不对时 bin 直接 E0061。红证据是这两次 `cargo check`。随后把同一条不变量写成库内测试，修完是绿的。
2. `fuzz/` 是空 `[workspace]` 的独立工作区。删锁再 `cargo check` 只重写 `fuzz/Cargo.lock`。路径依赖上的 `edition.workspace = true` 仍由根工作区继承，这次能编过。
3. 合并把 `src/lib.rs` 里的 `graph` / `sampling` 留在树上，却从清单里拿掉了 `af-graph` 和 `af-sampling`。不补这两条路径依赖，锁文件修好之后 `cargo check` 仍会在 `use af_graph` / `use af_sampling` 处失败。补的是本仓库已有 crate，不是新的第三方 crate。传递依赖（如 `cel-interpreter`、`rusqlite`）跟着进了 fuzz 锁，版本是 cargo 按清单解析的结果，`libfuzzer-sys` 仍是 0.4.13，`serde_json` 仍是 1.0.154，`chrono` 仍是 0.4.45。
4. 网关不保存工单。harness 每次调用递交同一张已经迁移的 `WorkOrder`。快照是声明那一次的 `order.state()`。已执行的意图在订单离开 InProgress 之后仍只重放，这是 W3B 已验证的行为，harness 不把重放当成失败。
5. 工单 JSON 缺 `stable_id` 现在解析失败，fuzzer 把失败当成跳过，不生成替补身份。自己 `WorkOrder::new` 出来的单会带上身份，往返必须保住 `stable_id` 和 `run_id`。

## 遗留

- 冒烟只覆盖任务点名的两个 target，各 2000 次，无 sanitizer。`af_graph_schema_parse` 和 `af_sampling_invariant_input` 能编过，这次没有再跑 2000 次。
- C3 记录的 118 个冒烟样本不在这个克隆里。入库的只有测试点名的 3 个种子。这次 libFuzzer 新写的样本已删，没有入库。
- `fuzz/.gitignore` 的冲突标记不在红队那句证据里，但不清掉的话 `corpus` 会被忽略，上面 3 个种子进不了 git，旧测试在干净检出里失败。一并清了。
- 产品 crate 的源码与根锁文件没有改。`Transition::from_state(Cancel)` 仍然会 panic，harness 继续避开。
- 网关仍信任调用方递交的 `WorkOrder`，不自己重跑工单状态机。这是 W3B 已记录的边界，本次没有改网关。
