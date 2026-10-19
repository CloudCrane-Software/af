# RESULT-UPDATE — B4 af-release-gate proptest 补强（波次 1 ⑦/T9-7）

## ① 结论

**通过**。未改 CONWIP、夜间信用制、渐放晋升的已验证语义。`proptest =1.12.0` 作为 dev-dependency 引入（只开 `std`）。新增 3 组属性测试。`cargo test -p af-release-gate` **18 全绿**（原 14 单元 + 1 日模拟 + 3 个 proptest；0 失败）。`cargo clippy --all-targets -p af-release-gate -- -D warnings` 无警告。

测试数 15 → 18：只新增测试。原 15 个用例的断言没有改。为让 `-D warnings` 能通过，顺手改了 4 处既有写法（下限比较、两处队列合并、两处供给循环、日模拟偶数拍），行为与改前一致，日模拟与 14 个单元测试仍绿。

## ② 步骤

1. 读完 `crates/af-release-gate/RESULT.md`、`src/gate.rs`、`src/sim.rs`、`tests/gate_unit.rs`、`tests/day_sim.rs`。已验证结论保持为：WIP 判定是 `wip + weight > upper`（精确等号可入）；策略顺序 canary → 夜间 → WIP；夜间初始信用 1.0，WipCap 发生在信用已扣之后；渐放 `open` 只在谓词通过时从 gated 到全量，`released` 在晋升后冻结；`seed_wip` 不衡上限。
2. `crates/af-release-gate/Cargo.toml` 加入 `proptest = { version = "=1.12.0", default-features = false, features = ["std"] }`。版本与 af-calendar / af-sampling / af-stop 对齐。关掉默认特性，避免 `bit-set` 把 `borsh` derive 特性统一进工作区。提交 `66bb98b`。`Cargo.lock` 只给本 crate 挂上锁里已有的 proptest 1.12.0，没有新增包。
3. clippy 1.99 对既有代码报 5 条 `-D warnings`（外加测试里 1 条 `manual_is_multiple_of`）。按原语义改写：`partial_cmp != Some(Less)`（NaN 仍拒绝）、`mem::take` 合并泊位/暂存、`while let` 供给循环、`k.is_multiple_of(2)`。提交 `9782b6a`。
4. 新增 `tests/properties.rs`，三组各 256 例，失败持久化关闭：
   - `conwip_capacity_never_over_releases`：精确等号可入、超重件拒绝后 WIP 不动；任意 admit / complete / begin_canary 之后，WIP 等于在制权重和且不超过上限。
   - `canary_batch_is_monotonic_and_never_reverts`：同一批次 `released` 单步至多 +1 且不超过 `max_probes`；`open` 只从 false 到 true；晋升后不再 `CanaryProbeCap`；gated 期间 `begin_canary` 失败且状态不动。缺陷累计不晋升的路径单独钉住。
   - `rejection_does_not_partially_commit`：拒绝不改 WIP、在制件数、渐放计数、下限标志。夜间计数按已验证顺序变化：CanaryProbeCap 不烧信用，NightThrottled 只加 requests，窗内 WipCap 在扣信用之后拒绝。未知 `complete` 与重复 `begin_canary` 整状态不动。
5. `seed_wip` 不在操作序列里。时刻全部由用例注入，属性测试没有 `sleep`。提交 `6a30cbf`。

## ③ 证据

工具链：`rust-toolchain.toml` 的 `1.99.1` 目录被 rustup 标成半安装，直接 `cargo` 会走恢复并在 `bin/cargo` 冲突处回滚，该目录没有 `rustc`。下面的命令在 `source /data/af-factory/env.sh` 之后加了 `RUSTUP_TOOLCHAIN=stable`。`stable` 就是 `rustc 1.99.1 (58cf8f9c3 2026-10-13)`、`cargo 1.99.1 (5f94df478 2026-08-27)`，与 BOOTSTRAP 一致。

```
$ cargo clippy --all-targets -p af-release-gate --offline -- -D warnings
    Checking af-release-gate v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.48s
```

```
$ cargo test -j 4 -p af-release-gate --offline

running 0 tests
test result: ok. 0 passed

running 1 test
test day_traffic_respects_caps_and_never_starves ... ok
test result: ok. 1 passed

running 14 tests
test result: ok. 14 passed

running 3 tests
test conwip_capacity_never_over_releases ... ok
test canary_batch_is_monotonic_and_never_reverts ... ok
test rejection_does_not_partially_commit ... ok
test result: ok. 3 passed; 0 failed; finished in 0.09s

Doc-tests af_release_gate
test result: ok. 0 passed
```

合计 18 passed / 0 failed。属性测试约 0.09s（每组 256 例，序列最长 28 步，无 sleep）。没有写出 `proptest-regressions/`。

提交：

- `66bb98b` af-release-gate: 引入 proptest =1.12.0 作为 dev-dependency
- `9782b6a` af-release-gate: 清掉阻断 clippy -D warnings 的既有告警
- `6a30cbf` af-release-gate: 补三组 proptest 属性测试

## ④ 反馈

1. 公开接口够用。`wip_minutes`、`in_process_count`、`canary_status`、`night_stats`、`drain_events`、`refill_needed` 能把三组不变量钉死，不必为测试改闸门。
2. 窗内 WipCap 会留下已扣的夜间信用。这是 RESULT 反馈 4 的已验证行为。属性测试把准入提交（WIP、队列、渐放、下限边沿）钉成拒绝时不变，同时要求信用消耗与 canary → 夜间 → WIP 的顺序一致。把顺序改成先查 WIP 再扣信用，这组测试会直接红。
3. 新批次只能在上一批已经 `open`（或还没有批次）时 `begin_canary`。gated 中途换配置会被 `CanaryAlreadyActive` 拒绝，计数不重置。晋升后的 `released` 冻结，再开一批才从 0 重新 gated。这是“批次单调、晋升不回头”，不是全局 `released` 永不归零。
4. 谓词看到的是累计 `defects`。首件带缺陷后再来干净件，零缺陷谓词仍不晋升。属性测试把这条和“晋升之后缺陷不能把 open 打回去”分开钉住。
5. `prop_assert!` 单参数形式会把条件 `stringify!` 后送进 `format!`。条件里的 `{` 会变成非法格式串，要另写格式参数，或先把布尔结果放进局部变量。

## ⑤ 遗留

- RESULT.md 原有遗留仍在：策略顺序硬编码、`admit_blocking` 用固定 `now` 重试、夜间暂存归还顺序、模拟未建模审阅人异质性、评估报告是累计口径。本任务不改这些。
- `seed_wip` 仍不衡上限。属性测试故意不把它放进“不超发”序列。
- 256 例是抽样，不是穷尽证明。反例会打印缩到最小的 `Case`（配置名只有 `v1`/`v2`，权重与分钟都是小整数）。默认不写回归文件。
- `proptest` 只写在本 crate 的 dev-dependencies。根 `Cargo.toml` 不在改动范围，没有收进 `[workspace.dependencies]`。
- 命名工具链目录 `1.99.1-x86_64-unknown-linux-gnu` 缺 `rustc`，rustup 恢复会因 `bin/cargo` 冲突失败。本次用同版本的 `stable` 工具链编译。修这个目录不在本任务范围。
