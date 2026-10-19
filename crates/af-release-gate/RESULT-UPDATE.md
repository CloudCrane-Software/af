# RESULT-UPDATE — W3C af-release-gate 挂工单状态机（波次 3）

## ① 结论

**通过**。`ReleaseGate::admit` 把工单 Created→Released 包进投放闸。判定顺序是：工单状态机与 release 门控 → 审阅分钟权重 → 渐放 → 夜间窗口 → CONWIP 上限。前两步失败时在制、夜间信用、渐放游标都不变。后三步仍走原来的 `try_admit`，顺序没改（canary → 夜间 → WIP，`wip + weight > upper` 才拒绝，精确等号可入）。放行才把调用方工单迁到 Released，并返回 `Permit`，上面的 `RampPhase` 标明 Ungated / Probe / Open，供 trace 使用。拒绝是类型化的 `Reject::{Order, AtCapacity, RampPhaseNotOpen, NightThrottled, BadWeight}`。

已验证语义没有改。持久化层（`store.rs`、`persist.rs`、schema、运行态快照）没有改。

`cargo test -p af-release-gate` **43 全绿**（原 36 + 新增 7；0 失败）。`cargo clippy --all-targets -p af-release-gate -- -D warnings` 无警告。

测试数 36 → 43：原 1 个日模拟、14 个单元、17 个持久化、4 个 proptest 仍在，断言没改。新增 `tests/order_release.rs` 7 个。

## ② 步骤

1. 读完 `RESULT.md` 与当时的 `RESULT-UPDATE.md`（W2C 持久化）。本克隆基线 `b7d3a9b` 已经包含 W2C，不是合流前的 main。按任务做成纯增量：不改账本、不改重启测试、不改 `try_admit` / `complete` 的策略顺序。
2. `Cargo.toml` 增加 `af-workorder = { path = "../af-workorder" }`。`Cargo.lock` 只在本 crate 的依赖列表加了 `af-workorder` 一行。没有新的第三方 crate。提交 `a9c5978`。
3. `ReleaseGate::admit(&mut WorkOrder, review_minutes, now)`：
   - 在副本上调用工单 `release()`。状态机边不存在或前置缺失则返回 `Reject::Order(工单 GateError)`，闸一字不改。
   - 权重必须是有限正数，否则 `BadWeight`，闸仍不改。缺前置优先于非法权重。
   - 通过后按 `graph_version` 构造 `Item`，走 `try_admit`。`CanaryProbeCap` → `RampPhaseNotOpen`，`NightThrottled` 同名保留，`WipCap` → `AtCapacity`。
   - 闸放行后才对正本调用 `release()`。若这次失败，用 `rollback_admit` 把刚入队的一件摘掉，不走 `complete`，避免记成探针完成或触发晋升。以当前工单实现，副本已通过时正本不会失败。
   - `Permit.ramp`：该版本没有渐放为 `Ungated`；gated 探针为 `Probe { index, max_probes }`；已晋升为 `Open { probes_released }`。
4. `tests/order_release.rs` 7 个用例（见下）。测试里的 dev-dependency 仍是同一个 `af-workorder` 路径，锁文件没有再变。提交 `79df27e`。

## ③ 证据

工具链：`source /data/af-factory/env.sh` 之后为 `rustc 1.99.1 (58cf8f9c3 2026-10-13)`、`cargo 1.99.1 (5f94df478 2026-08-27)`。命令都带 `--offline`。时刻由测试传入 `MinuteOfDay`，没有 sleep。

```
$ cargo clippy --all-targets -p af-release-gate --offline -- -D warnings
    Checking af-release-gate v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.21s
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
test admit_rejects_when_release_edge_is_absent ... ok
test full_gate_rejects_release_until_capacity_returns ... ok
test missing_preconditions_do_not_spend_night_credit ... ok
test missing_preconditions_win_over_closed_ramp ... ok
test missing_preconditions_win_over_full_gate ... ok
test non_positive_weight_does_not_enter_the_gate ... ok
test ramp_phase_blocks_until_promotion_then_admits ... ok
test result: ok. 7 passed

running 17 tests
test result: ok. 17 passed

running 4 tests
test canary_batch_is_monotonic_and_never_reverts ... ok
test conwip_capacity_never_over_releases ... ok
test conwip_does_not_over_release_after_restart ... ok
test rejection_does_not_partially_commit ... ok
test result: ok. 4 passed; 0 failed; finished in 0.64s

Doc-tests af_release_gate
test result: ok. 0 passed
```

合计 43 passed / 0 failed。

新增用例对应关系：

- `full_gate_rejects_release_until_capacity_returns`：6+4 精确等于上限 10 可入，再加 1 返回 `AtCapacity`，工单停在 Created；释容 4 后同一张单以权重 4 放行。
- `ramp_phase_blocks_until_promotion_then_admits`：`graph_version = g-2`、`max_probes = 1` 时第二张单 `RampPhaseNotOpen`；探针 0 缺陷完成并晋升后，该单以 `RampPhase::Open { probes_released: 1 }` 放行。别的图谱版本在封顶期间仍是 `Ungated`。
- `missing_preconditions_win_over_full_gate`：闸已满且产线、凭证都缺，返回 `PreconditionsNotMet`（顺序仍是产线在前、凭证在后），不是 `AtCapacity`。WIP 与在制件数不变。
- `missing_preconditions_win_over_closed_ramp`：探针名额为 0 且凭证未签名。返回 `BudgetVoucherNotSigned`，不是 `RampPhaseNotOpen`。`prev_in_window` 仍是 `None`，说明 `try_admit` 没被调用（它的第一步就会记下窗口观察）。
- `missing_preconditions_do_not_spend_night_credit`：缺前置不扣夜间信用。随后一张门控齐全的单在 ratio 0.25 的窗口里被放行，`night_stats = (1, 1)`。若前一次已经扣过初始信用，这一次会因信用不足被夜间拒绝。
- `admit_rejects_when_release_edge_is_absent`：已 Released 的单（包括绕过闸直接 `release()` 的）返回 `IllegalTransition`，不入队。
- `non_positive_weight_does_not_enter_the_gate`：0、负数、无穷、NaN 都是 `BadWeight`。缺前置优先于 NaN。权重合法后才看到 `RampPhaseNotOpen`。

提交：

- `a9c5978` af-release-gate: 订单 release 经 admit 过 CONWIP 与渐放
- `79df27e` af-release-gate: 补工单 release 过闸测试

## ④ 反馈

1. 接缝在投放闸这一侧。`af-workorder` 不在改动范围，`WorkOrder::release()` 仍然可以直接把 Created 迁到 Released，从而绕过 CONWIP 和渐放。编排层要走 `ReleaseGate::admit`，不能两套入口混用。
2. 工单上没有标准审阅分钟。`review_minutes` 和 `now` 都由调用方注入。同一张单若两次传入不同权重，闸按当次权重计，不会回头改已入队的票据。
3. 门控用「副本上调用 `release()`」而不是重写缺失清单。`release_missing` 是私有的，复制一份会和工单 crate 漂移。今天 `release()` 只改状态和履历，副本上的成功不会碰到正本。以后若 `release()` 有副本之外的副作用，这个探针要重看。
4. 夜间信用的原行为还在：请求过了渐放、进了窗口，再因 WIP 被拒时，信用已经扣掉。门控失败不走这条路径，所以不扣信用。这是「先门控后闸」和 RESULT 反馈 4 叠在一起的结果，不是把扣信用的时机改了。
5. 释容仍是 `ReleaseGate::complete(permit.ticket.item_id)`。工单自己的 `complete`（InProgress→Completed）不会把 WIP 还回去。`ticket.config` 就是 `graph_version`，trace 记相位时用 `Permit.ramp`，不要再从事件缓冲里反推。
6. `admit` 只加在内存闸上。`DurableGate` 没有这个入口。在制队列的快照格式也不认识工单字符串 id，只认识分配出来的 `u64` item id。`Permit.order_id` 必须由调用方自己留着。

## ⑤ 遗留

- RESULT.md 里的遗留仍在：策略顺序硬编码、`admit_blocking` 用固定 `now` 重试、夜间暂存归还顺序、模拟未建模审阅人异质性、评估报告是累计口径。本任务不改这些。
- `SharedGate` 没有 `admit`。阻塞等待仍只覆盖 `try_admit` 的 `Item` 路径。
- 订单准入没有落库。重启后的在制恢复仍是 W2C 那条 `Item` 路径。本文件替换前的 W2C 报告在基线提交 `b7d3a9b` 的 `RESULT-UPDATE.md`。
- `rollback_admit` 只防「副本通过、正本 release 失败」。当前工单实现下这条走不到，没有单独的测试。
- 根 `README.md` 的测试计数不在本任务改动范围。
