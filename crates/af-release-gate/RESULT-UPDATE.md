# RESULT-UPDATE — F1 af-release-gate 分数权重重开被判损坏

## ① 结论

**通过**。发现编号 F1 成立，已修复。

修法选第二条：在制队列是 WIP 的唯一真相，WIP 每次按队列顺序从 `+0.0` 左折叠重算，不再做 `+=` / `-=`。账本写入的 `wip_bits` 就是这个折叠。重开时仍逐位核对，对不上照旧 `Corrupt`。

不选毫分整数。理由：

- RESULT 把 WIP 定义成在制权重之和。坏的是增量累加器偏离了这个和，不是数域本身。整分钟（既有 u16 属性、日模拟 15–30）在 f64 里精确，`to_bits` 与「精确等号可入」保持原样。
- 毫分会把任意 f64 量化到 0.001，并改掉 schema 1 里 `weight_bits` / `wip_bits` 的含义（它们存的是 f64 位型）。声明核对和「把 `wip_bits` 改成 0 必须失败关闭」都按这个位型锁着。schema 仍是 1，不迁移。

原复现路径已绿：admit 0.1、admit 0.2、complete 掉 0.1、重开为 `AlreadyDeclared`，剩下那件的权重与 WIP 都是 0.2 的位型。

`cargo test -p af-release-gate` **45 全绿**（原 43 + 新增 2；0 失败）。`cargo clippy --all-targets -p af-release-gate -- -D warnings` 无警告。

测试数 43 → 45：日模拟 1、单元 14、工单 7 不变。持久化 17 → 18（`fractional_wip_survives_reopen_after_partial_complete`）。属性 4 → 5（`fractional_milli_weights_survive_restart`）。原断言没改。

本文件替换前的 W3C 报告在基线 `aa4689b` 的 `crates/af-release-gate/RESULT-UPDATE.md`。

## ② 步骤

1. 读完 `RESULT.md` 与当时的 `RESULT-UPDATE.md`（W3C）。不改准入顺序、夜间信用、渐放、schema、声明核对。
2. 先写失败测试再跑红。`admit 0.1` + `admit 0.2` + `complete 0.1` 后重开：`Corrupt { detail: "wip bits 3fc999999999999b != item sum 3fc999999999999a" }`，差 1 ULP。提交 `f246b4e`（该提交测试是红的，故意只留下复现）。
3. `ReleaseGate` 去掉 wip 累加器。`wip_minutes`、快照、上限判断、下限边沿都读队列左折叠。`seed_wip`、`try_admit`、`complete`、`rollback_admit` 只改队列。`store::load_runtime` 与 `restore_runtime` 用同一个 `sum_weights`。空队列必须是 `+0.0`：`Iterator::sum` 对 f64 从 `-0.0` 加起，空和的位型是 `0x8000_0000_0000_0000`，会把空闸重开判坏。提交 `9bd12c2`。
4. 属性测试补 0.001 级权重。既有 `Op` 仍只出 u16。新生成器出 1..=2000 毫分（`milli as f64 * 0.001`），序列里含准入、完成和一次重开，断言内存闸与账本的 WIP 位型等于在制条目左折叠，且不超过上限。提交 `d6b0e74`。

## ③ 证据

工具链：`source /data/af-factory/env.sh` 之后为 `rustc 1.99.1 (58cf8f9c3 2026-10-13)`、`cargo 1.99.1 (5f94df478 2026-08-27)`。命令都带 `--offline`。时刻由 `ManualClock` 或用例传入的 `MinuteOfDay` 注入，没有 sleep。没有新依赖。

红（`f246b4e`，修之前）：

```
$ cargo test -p af-release-gate --offline --test persist fractional_wip_survives_reopen_after_partial_complete -- --nocapture

thread 'fractional_wip_survives_reopen_after_partial_complete' panicked at crates/af-release-gate/tests/persist.rs:686:77:
called `Result::unwrap()` on an `Err` value: Corrupt { detail: "wip bits 3fc999999999999b != item sum 3fc999999999999a" }

test result: FAILED. 0 passed; 1 failed; 17 filtered out
```

绿（修完属性测试之后）：

```
$ cargo clippy --all-targets -p af-release-gate --offline -- -D warnings
    Checking af-release-gate v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.62s
```

```
$ cargo test -p af-release-gate --offline

running 0 tests
test result: ok. 0 passed

running 1 test
test day_traffic_respects_caps_and_never_starves ... ok
test result: ok. 1 passed

running 14 tests
test result: ok. 14 passed

running 7 tests
test result: ok. 7 passed

running 18 tests
test fractional_wip_survives_reopen_after_partial_complete ... ok
test corrupt_rows_fail_closed ... ok
test result: ok. 18 passed

running 5 tests
test canary_batch_is_monotonic_and_never_reverts ... ok
test conwip_capacity_never_over_releases ... ok
test conwip_does_not_over_release_after_restart ... ok
test fractional_milli_weights_survive_restart ... ok
test rejection_does_not_partially_commit ... ok
test result: ok. 5 passed; 0 failed; finished in 0.77s

Doc-tests af_release_gate
test result: ok. 0 passed
```

合计 45 passed / 0 failed。`corrupt_rows_fail_closed` 仍在：手工把 `wip_bits` 改成 0，重开还是 `Corrupt`。

提交：

- `f246b4e` af-release-gate: 复现 F1 分数权重重开 wip 位不符（红）
- `9bd12c2` af-release-gate: F1 在制队列为 WIP 真相，按条目重算
- `d6b0e74` af-release-gate: F1 属性测试覆盖 0.001 级分数权重

## ④ 反馈

1. 位比较留下来，是核对「写入的折叠」和「读出的折叠」是不是同一个数。F1 的假损坏来自写入侧用了另一套加法。放宽比较会把 `wip_bits = 0` 这种篡改也放过去。
2. 入队只追加在队尾，所以新折叠等于旧折叠再加这一件，上限式 `wip + weight > upper + 1e-9` 与入队后的和一致。从队列中间完成一件时，新 WIP 是剩余条目的折叠，不再是 `旧值 - 权重`。整分钟两者位型相同；分数权重只认折叠。
3. `1e-9` 容差没动。这个量级上 1 ULP 远小于它。整分钟的精确等号测试仍覆盖「等于上限可入，再加 1 拒绝」。
4. 空和的位型是个坑：必须从 `+0.0` 加起。用 `Iterator::sum` 会让空闸的 WIP 变成 `-0.0`，重开与 `assert_books(..., 0.0, 0)` 都会挂。

## ⑤ 遗留

- 本修复之前若已用增量累加写下分数权重账本，重开仍会因位差被拒。schema 仍是 1，不迁移。这个 PoC 没有那种存量库。
- 夜间信用仍是 f64 累加器，不在 F1 里。现有生成器只用八分之一的二进分数，位型精确。重开只原样带回信用位，不拿它和另一条重求和做比较。
- RESULT.md 的遗留仍在：策略顺序硬编码、`admit_blocking` 用固定 `now` 重试、夜间暂存归还顺序、模拟未建模审阅人异质性、评估报告是累计口径。
- W3C 的接缝仍在：`WorkOrder::release()` 可以绕过闸；`DurableGate` 没有 `admit`；在制快照不存工单字符串 id。
- 根 `README.md` 的测试计数不在本任务改动范围。
