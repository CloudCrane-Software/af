# RESULT-UPDATE — B2 · cargo fuzz 骨架（af-workorder + af-effect-gateway）

## 结论

通过。仓库根 `fuzz/` 是独立工作区（空 `[workspace]`），不在根 `members` 里。两个 target 在 stable 上以 `--sanitizer none` 各跑 2000 次，退出码 0，无 panic，`fuzz/artifacts` 下没有崩溃样本。`cargo test --workspace --offline` 全绿：非 doc 测试 215 passed、1 ignored（原有日历慢扫描），doctest 1 passed（af-stop）。合起来就是基线的 216+1，数字没有变。两个 crate 的 `Cargo.toml` 没改。

## 步骤

1. 在仓库根执行 `cargo fuzz init -t af_workorder_order_parse`。虚拟工作区没有根 package，`cargo metadata` 的第一个包是 `af-calendar`，生成的依赖是 `path = ".."`。已改成 `../crates/af-workorder` 与 `../crates/af-effect-gateway`，包名改为 `af-fuzz`。
2. 按接入清单补上空 `[workspace]`。接着 `cargo fuzz add af_effect_gateway_idempotency`。
3. 两个 harness 从字节拼最小合法输入，只调公共 API。
   - `af_workorder_order_parse`：能解析就 `serde_json` 反序列化 `WorkOrder`；始终再构造一张工单，走 `transition` 和具名门控（release/start/complete/close/cancel）。SLA 用字节拼出的 `DateTime<Utc>` 调用 `is_overdue`。
   - `af_effect_gateway_idempotency`：`ManualClock` 注入单调毫秒和墙钟，`open_in_memory_with_clock` 后 `declare_intent`。指纹没有单独的公共函数，观测面是 `snapshot_intent` 的 `payload_sha256`。同载荷两把键的指纹必须相同。随后推进注入的钟，再走撤回、执行、`record_receipt` 和 `resume` 之后的旧纪元声明。
4. 不调用 `Transition::from_state`：`Cancel` 在该函数里是 `unreachable!`。
5. init 模板的 `.gitignore` 含 `corpus`。初始样本要入库，所以只忽略 `artifacts`、`coverage`、`target`。
6. 冒烟时 libFuzzer 往 corpus 里又写了 337 个样本，已从工作区删掉。入库的只有初始样本。

提交：`a21d7bf`（工作区与两个 target）、`8c74d4f`（初始 corpus）。

新依赖只在 `fuzz/Cargo.lock`，不进主工作区：

- `libfuzzer-sys` 0.4.13（cargo-fuzz 0.13.2 的 init 模板指定 0.4）
- 传递：`arbitrary` 1.5.0、`cc` 1.8.0
- `serde_json` 1.0.154、`chrono` 0.4.45，与主工作区锁一致，用来构造 JSON 载荷和注入时刻

## 证据

工具链：直接调用 stable 目录里的 cargo/rustc，版本 `rustc 1.99.1 (58cf8f9c3 2026-10-13)`、`cargo 1.99.1`。命令前都 `source /data/af-factory/env.sh`。fuzz 的 check/clippy/run 使用 `CARGO_TARGET_DIR=/tmp/b2-fuzz-target`，避免和共享 `target` 抢锁。workspace 测试用 env.sh 里的共享 target。

```text
cargo fmt --manifest-path fuzz/Cargo.toml
cargo clippy --manifest-path fuzz/Cargo.toml --all-targets --offline -j 4 -- -D warnings
# Finished `dev` profile ... af-fuzz, exit 0
cargo check --manifest-path fuzz/Cargo.toml --offline --all-targets -j 4
# Finished `dev` profile ... in 16.19s, exit 0
cargo test --manifest-path fuzz/Cargo.toml --offline -j 4
# Finished `test` profile ... in 0.08s, exit 0（bin 设了 test=false，没有测试可跑）
```

根工作区成员只有 `crates/*` 下 11 个包。`fuzz` 的 workspace member 只有 `af-fuzz`。

```text
cargo fuzz run --sanitizer none af_workorder_order_parse -- -runs=2000
# Finished `release` profile ... in 1m 19s
# #2000  DONE   cov: 1148 ft: 2474 corp: 263/27Kb
# Done 2000 runs in 0 second(s)
# WORKORDER_EXIT:0

cargo fuzz run --sanitizer none af_effect_gateway_idempotency -- -runs=2000
# #2000  DONE   cov: 758 ft: 1043 corp: 80/1129b
# Done 2000 runs in 1 second(s)
# GATEWAY_EXIT:0
```

两次运行都打印了三行 `WARNING: Failed to find function "__sanitizer_*"`（没有 ASan 运行时）。没有 `ERROR`、没有 panic。`fuzz/artifacts` 下没有文件。

```text
cargo test --workspace --offline -j 4
# TEST_EXIT:0
# 非 doc：215 passed，0 failed，1 ignored
# doctest：1 passed（af-stop）
# 输出里没有 af-fuzz
```

忽略的那条是 `af-calendar` 的 `dense_hourly_scan_3y_berlin_and_new_york`，原来就是 `#[ignore]`。

## 反馈

1. 父目录已是虚拟工作区时，`cargo fuzz init` 默认不写 `[workspace]`，接着在 `fuzz/` 里跑 cargo 会报「current package believes it's in a workspace when it's not」。cargo 的修法就是补一张空 `[workspace]`。`--fuzzing-workspace=true` 会写成 `members = ["."]`；cargo 1.99 上两种写法 metadata 都能过，这次按接入清单留空表。
2. 在虚拟工作区根上 init，依赖名和 `path = ".."` 对不上任何 crate。要改成 `../crates/<crate>`。
3. `--sanitizer none` 在本机 stable 上可以完成插桩并链上 libFuzzer。那三行 `__sanitizer_*` WARNING 是没链 ASan 的正常现象，2000 次仍然跑完。
4. 网关的载荷指纹函数是私有的。fuzz 入口是 `declare_intent`，读回 `IntentSnapshot.payload_sha256`。
5. `cargo-fuzz` 0.13.2 启动时打印 `GLIBC_2.39` weak version not found（本机 glibc 2.38）。init、add、run 都实际执行成功了。

## 遗留

- 真实长 fuzz 留 L3。这次只冒烟 2000 次。
- 冒烟长出来的 corpus 没有入库，仓库里只有初始样本（每个 target：空输入、非 JSON 前缀、可解析 JSON；订单另有一张 Completed JSON，网关另有键序对调的两个对象）。
- 没有改 `af-workorder` / `af-effect-gateway` 的源码。`Transition::from_state(Cancel)` 仍然会 panic，harness 避开了它。
- 名为 `1.99.1` 的 rustup toolchain 目录缺 `rustc`。rustup 一恢复就因 `bin/cargo` 冲突回滚，还会在恢复过程里卸组件。本次没有重装这份共享工具链，改用同版本的 stable 目录二进制。
