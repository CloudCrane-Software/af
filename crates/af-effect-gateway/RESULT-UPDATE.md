# RESULT-UPDATE — F2 af-effect-gateway 外呼抢占

## 结论

通过。`execute` 不再在入口 fence 一次之后就调外部闭包。同纪元两条连接并发执行同一意图时，`BEGIN IMMEDIATE` 里先把 `declared` 原子改成 `executing` 并记下纪元，只有抢到的一方外呼；外呼成功后，同一条纪元守卫把回执写成 `executed`。红测复现是双 `RanFresh`、外呼计数 2；修完 t9 为恰好一方 `RanFresh`、外呼计数 1。

崩溃留在 `executing` 且没有回执时不自动重发。`Gateway::resume` 只把纪元 +1，不把行放回 `declared`。人工用 `record_receipt` 补回执，或显式 `resume_execution` 放回 `declared` 后再执行。闭包返回 `Err` 仍把意图放回 `declared`，窗口关闭后可重试（t6 语义保留）。

`cargo test -p af-effect-gateway` 全绿：库内单测 2→3，集成测试 8→11。原 t1–t8 与 2 个时钟单测仍通过。clippy `--all-targets -D warnings` 无警告。未新增第三方 crate。

## 步骤

1. 先提交失败测试 `t9_same_epoch_concurrent_execute_calls_out_once`（`703175e`）。两条连接在同一文件上、同一纪元，`Barrier` 对齐后 `execute` 同一键。修复前外呼计数为 2，两个结果都是 `RanFresh`（失败信息里“其他结果”为空）。
2. `effect_intent.state` 增加 `executing`，增加 `claim_epoch`。新库直接建新表。已有 W3B 表（有 `declared_order_state`、没有 `claim_epoch`）在打开时的 `BEGIN IMMEDIATE` 里重建并拷贝旧行，`claim_epoch` 填 NULL。早于订单快照列的库不迁，和 W3B 留下的行为一样。
3. 每条连接设置 `busy_timeout` 5000 ms，写锁冲突时等待，而不是立刻 `SQLITE_BUSY`。抢占事务在调用闭包之前提交，外呼期间不持有写锁。
4. `execute` 的判定顺序不变：fencing、订单 id、withdrawn、已执行则重放、executing 则 `ExecutionInDoubt`、订单态、窗口，然后才抢占。抢到后闭包返回 `Err` 或载荷解析失败，且纪元仍匹配，则放回 `declared`。成功则 `UPDATE ... WHERE state='executing' AND claim_epoch=会话纪元 AND 当前纪元=会话纪元`。
5. `record_receipt` 的写入放进同一条 `BEGIN IMMEDIATE`，`WHERE` 带当前纪元。允许 `declared` 或 `executing` → `executed`；已执行保留原回执；已撤回拒绝。`resume_execution` 只把“executing 且无回执”放回 `declared`。
6. 补测试：t10 崩溃停在 executing 后重入不外呼，`record_receipt` 接管，`resume` 不自动放行，`resume_execution` 之后才再外呼一次。t11 外呼期间另一连接 `resume`，旧会话不能落回执，新会话重入不外呼。库内单测覆盖 W3B 旧表迁移后可抢占一次，第二次打开不再重建。t3 补“撤回后不能 record_receipt”，t4 补“窗口未关的分步回执只记一次、execute 只重放”。

## 证据

工作目录 `/data/af-factory/fx2`，分支 `task-F2`。红测提交 `703175e`，修复提交 `6c28b2f`。

修复前（只跑 t9）：

```text
source /data/af-factory/env.sh
cargo test -p af-effect-gateway --test gateway_tests t9_same_epoch_concurrent_execute_calls_out_once -- --test-threads=1
# assertion `left == right` failed
# F2: 同纪元双连接并发 execute 外呼计数必须为 1，其他结果 []
#   left: 2
#   right: 1
# “其他结果”为空，两个返回值都是 RanFresh
# exit 101
```

修复后：

```text
source /data/af-factory/env.sh
cargo fmt -p af-effect-gateway
cargo clippy --all-targets -p af-effect-gateway -- -D warnings
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.43s
# exit 0，无警告行

cargo test -p af-effect-gateway
# unittests src/lib.rs: ok. 3 passed; 0 failed
#   clock 2 条，schema_tests::w3b_table_migrates_and_declared_row_can_be_claimed ok
# tests/gateway_tests.rs: ok. 11 passed; 0 failed
#   t1 … t11 ok
# Doc-tests af_effect_gateway: ok. 0 passed; 0 failed
```

计数：库内单测 2→3，多出来的是旧表迁移。集成测试 8→11，多出来的是 t9、t10、t11。t1–t8 仍在。源码和测试里没有 `sleep`；t9 的胜出方在闭包里有界 `yield_now`，用来把“已提交抢占、尚未落回执”的窗口留住，对端在这期间返回且不外呼。

## 反馈

1. 抢占在闭包之前提交。进程死在外呼和回执之间，行停在 `executing`。下一次 `execute` 返回 `ExecutionInDoubt`，不调用闭包。
2. `Gateway::resume` 不是这条路的自动重发开关。它只让旧会话变成 `StaleEpoch`。要再试一次必须再调 `resume_execution`。
3. 并发里没抢到的一方，回执还没落时得到 `ExecutionInDoubt`，回执已经落时得到 `Replayed`。两种都不会外呼。t9 接受这两种。
4. `record_receipt` 补 `executing` 时只要求请求纪元等于当前纪元，不要求 `claim_epoch` 仍是抢占时的纪元。新纪元可以接管已经发生的回执。旧纪元在 `resume` 之后写不进去。
5. 闭包返回 `Err` 表示调用方确认这次没发出去，行放回 `declared`。这和“结果不明不许重发”不是一条路径。t6 仍然是失败后保持 `declared`、重试成功、再执行只重放。
6. 连接上的 `busy_timeout` 是 5000 ms。抢占事务不包住外呼，等待的是状态更新那一下，不是外部调用本身。
7. 已经是 `executing` 的行不能撤回。`withdraw_intent` 只改 `declared`，否则仍是 `WithdrawWindowClosed`，状态文本是 `executing`。

## 遗留

- 早于 `declared_order_state` 列的库仍然不迁移。打开后读写意图会失败。这是 W3B 已经记下的行为，这次没有扩成完整历史迁移。
- `fuzz/fuzz_targets/af_effect_gateway_idempotency.rs` 仍按旧签名调用 `declare_intent` / `execute`（不递交 `WorkOrder`）。该文件不在本任务改动范围，没有编译。这次没有改这两个方法的参数个数。
- 库内不 `catch_unwind`。闭包恐慌或进程消失都把行留在 `executing`。测试用 `catch_unwind` 模拟崩溃，生产路径没有把恐慌收成 `Err`。
- 已知失败后若放回 `declared` 的更新没写上、而纪元仍匹配，调用方可能看到 `ExternalCall`，行却还在 `executing`。下一次 `execute` 是 `ExecutionInDoubt`，不会重发。正常失败路径会写上，t6 覆盖的是这条。
- 没有重跑整个 workspace。其他 crate 的源码没改。

---

# 历史 · W3B af-effect-gateway 挂工单状态机

## 结论

通过。副作用意图已绑定工单状态。`declare_intent` 要求幂等键的 `workorder_id` 等于递交的 `WorkOrder` id，且声明当时的状态是 `Released` 或 `InProgress`；否则 `WorkOrderMismatch` 或 `OrderStateRejected`（带当前状态和允许状态，不是字符串清单）。`execute` 只在当前状态为 `InProgress` 时真正发送。意图行写入 `order_id` 与 `declared_order_state`，重复声明不改写这份快照。

幂等、fencing、撤回窗口没有回退。Clock 注入保持：窗口仍只看单调逻辑毫秒，墙钟只写审计字段。`cargo test -p af-effect-gateway` 全绿：2 个单测 + 8 个集成测试（原 7 条保持通过，新增订单态生命周期 1 条）。clippy `--all-targets -D warnings` 无警告。未新增第三方 crate，只加了 `af-workorder` 路径依赖。

## 步骤

1. `Cargo.toml` 增加 `af-workorder = { path = "../af-workorder" }`。根 `Cargo.lock` 只在 `af-effect-gateway` 的依赖列表里加一条 `af-workorder`，没有新的第三方 crate，也没有改版本。
2. `declare_intent` / `execute` 增加 `&WorkOrder`。判定在 fencing 之后：先对订单 id，再查状态。声明允许 `Released | InProgress`，执行发送只允许 `InProgress`。拒绝变体是 `OrderStateRejected { op, order_id, state, allowed }` 与 `WorkOrderMismatch { intent_workorder_id, presented_order_id }`。
3. `effect_intent` 增加 `order_id`、`declared_order_state`。状态以枚举名落库（`Released`、`InProgress` 等）。`ON CONFLICT DO NOTHING` 不改写快照。`IntentSnapshot` 读出这两列。
4. 执行判定顺序：订单 id 不一致拒绝；已撤回仍 `Withdrawn`；已执行仍重放已存回执，不再看当前订单态；尚未发送且不是 `InProgress` 则 `OrderStateRejected`（先于撤回窗口）；窗口未关仍 `WindowStillOpen`；否则外呼并落回执。`record_receipt` 仍不看订单态，也不看窗口。
5. 既有 7 条集成测试改为递交一张 `InProgress` 工单，断言未改。新增 `t8_order_state_transitions_drive_intent_lifecycle`：`Created` 拒绝声明且不落库；`Released` 可声明、窗口开或关都不能执行、外呼计数为 0；`InProgress` 可执行且快照仍是声明时的 `Released`；`Completed` 后新声明拒绝，未发送的意图也不执行，已执行的只重放。时序用 `ManualClock`，没有 `sleep`。

## 证据

工作目录 `/data/af-factory/w3b`，分支 `task-W3B`，提交 `085c5ff`。

```text
source /data/af-factory/env.sh
cargo check -p af-effect-gateway
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.84s
# exit 0

cargo fmt -p af-effect-gateway -- --check
# exit 0，无差异。用 -p 避免格式化其他 crate。

cargo clippy --all-targets -p af-effect-gateway -- -D warnings
# Checking af-effect-gateway
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.47s
# exit 0，无警告行

cargo test -p af-effect-gateway
# unittests src/lib.rs: ok. 2 passed; 0 failed
# tests/gateway_tests.rs: ok. 8 passed; 0 failed
#   t1 … t7 ok，t8_order_state_transitions_drive_intent_lifecycle ok
# Doc-tests af_effect_gateway: ok. 0 passed; 0 failed
```

计数：库内单测仍是 2。集成测试 7→8，多出来的是 t8。t1–t7 的幂等、冲突升级、撤回、旧纪元、窗口闸、外呼失败重试、墙钟回拨断言都还在。源码与测试里没有 `sleep`。`SystemTime::now` 仍只在 `SystemClock::wall_ms`。

## 反馈

1. 网关不保存工单本体，每次调用读调用方递交的 `WorkOrder`。声明快照是那一刻的 `order.state()`。订单随后 `start` / `complete`，调用方要递交同一张已经迁移的工单，执行门控才看得到新状态。
2. 已执行意图的重放不看当前订单态。发送已经发生，订单完成后再 `execute` 仍返回已存回执，不第二次外呼。尚未发送的意图才会被当前状态挡住。这样幂等重放在 `Completed` 之后还在。
3. `execute` 的订单态检查在撤回窗口之前。`Released` 得到 `OrderStateRejected`，不是 `WindowStillOpen`。等到截止点也不会发送。
4. fencing 仍在订单态之前。旧纪元请求还是 `StaleEpoch`，即使订单状态本身也不允许。
5. `record_receipt` 仍不看订单态，和“外呼已经发生，回执必须落库”同一条理由。想靠它绕过 `InProgress` 门控是做得到的；本次没有改这条分步路径。
6. `declared_order_state` 存的是 `OrderState` 枚举名。`withdraw_deadline_ms` 仍是单调逻辑毫秒，不是墙钟。

## 遗留

- `fuzz/fuzz_targets/af_effect_gateway_idempotency.rs` 仍按旧签名调用 `declare_intent` / `execute`（不递交 `WorkOrder`）。该文件不在本任务改动范围里，没有改，本任务也没有编译它。按源码，这两处调用少了 `WorkOrder` 参数，和现在的公开签名对不上。`cargo test -p af-effect-gateway` 和 `cargo test --workspace` 都不编 fuzz 工作区（fuzz 自带空 `[workspace]`）。
- 这次之前建的库没有 `order_id` / `declared_order_state`。`CREATE TABLE IF NOT EXISTS` 不会给旧表加列，重新打开这种文件会在读写意图时报错。本任务的测试都是新库，没有做列迁移。
- 网关信任调用方递交的 `WorkOrder`，不自己保存订单、也不重跑工单状态机。调用方如果递交一张 id 相同但状态与真实订单不符的对象，门控按递交的那张判。
- 没有重跑整个 workspace 的测试。其他 crate 的源码没改。`Cargo.lock` 只有上面那一条依赖边。
- T7 的停机不计时仍然在：进程死亡到下次 `open` 之间，逻辑钟停在上次写回的单调高水位。这次没有改时钟。

---

# 历史 · T7 af-effect-gateway 计时换单调钟

## 结论

通过。撤回窗口的开关已改为单调逻辑毫秒，执行与过期判定不再读 `SystemTime`。墙钟只写入 `declared_at_ms` / `executed_at_ms` 两个审计字段。`cargo test -p af-effect-gateway` 全绿：2 个单测 + 7 个集成测试（原 6 条保持通过，新增墙钟回拨语义测试 1 条）。clippy `--all-targets -D warnings` 无警告。未新增第三方 crate。

## 步骤

1. `src/clock.rs` 增加 `Clock`：`monotonic_ms` 做间隔判定，`wall_ms` 只供落库审计。`SystemClock` 的单调读数来自 `Instant`；`ManualClock` 用两个原子计数器，`advance` 两钟同进，`shift_wall_ms` 只动墙钟（负值即回拨）。本机 rustc 1.99 已把 `AtomicI64::fetch_update` 弃用为 `try_update`，手动钟用后者。新增 2 个单测。
2. `Gateway` 持有 `Arc<dyn Clock>`。`open` / `open_in_memory` 仍默认 `SystemClock`；另增 `open_with_clock` / `open_in_memory_with_clock`。打开时读 `gateway_meta.mono_ms`（TEXT，先取字符串再 parse，沿用纪元那条坑），进程内逻辑毫秒 = 该高水位 + 注入钟相对打开时刻的流逝，并且只增不减。`declare` / `withdraw` / `execute` 用这个逻辑毫秒比较 `withdraw_deadline_ms`。每次判定把更高的逻辑毫秒写回 `mono_ms`。fencing 纪元仍是计数器，未改。
3. 原 6 条集成测试改为注入 `ManualClock`，去掉 `sleep`。新增 `t7_wall_clock_rollback_does_not_affect_idempotency_window`：墙钟前跳 1 小时时窗口仍开（不发送、可撤回）；单调钟走到截止点且墙钟回拨到声明之前时窗口已关；进程重启后新单调钟从 0、墙钟在 Unix 纪元，已提交高水位仍让窗口保持关闭并只外呼一次，再回拨墙钟也只重放回执。

## 证据

命令（工作目录 `/data/af-factory/i1`）：

```
source /data/af-factory/env.sh && cargo fmt -p af-effect-gateway -- --check && cargo clippy -j 4 --all-targets -p af-effect-gateway -- -D warnings && cargo test -p af-effect-gateway -j 4
```

fmt 无差异。clippy 结束且无警告行（`-D warnings` 下退出码 0）。测试摘要：

```
running 2 tests
test clock::tests::system_clock_monotonic_does_not_go_backwards ... ok
test clock::tests::wall_rewind_does_not_move_monotonic_counter ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 7 tests
test t6_external_failure_keeps_declared_and_retry_succeeds ... ok
test t3_withdraw_within_window_means_never_sent ... ok
test t5_execute_before_window_close_is_refused_then_allowed ... ok
test t4_stale_epoch_requests_rejected ... ok
test t1_crash_recovery_same_key_same_payload_replays_stored_result ... ok
test t2_same_key_different_payload_rejected_requires_escalation ... ok
test t7_wall_clock_rollback_does_not_affect_idempotency_window ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

Doc-tests af_effect_gateway
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

计数变化：集成测试 6→7（新增 t7）；库内单测 0→2（时钟单调性）。原 6 条语义仍在，只是时间改由手动钟注入。crate 源码与测试中已无 `sleep`。`SystemTime::now` 只剩 `SystemClock::wall_ms`。

## 反馈

1. 对外暴露的 `withdraw_deadline_ms` 以及 `WindowStillOpen` 的 `deadline_ms` / `now_ms` 现在是单调逻辑毫秒，不是 Unix 墙钟。剩余窗口用二者相减。拿它们和 `SystemTime` 比较会错。
2. `executed_at_ms`（重放结果里的 `original_executed_at_ms`）和库内 `declared_at_ms` 仍是墙钟审计值，可以被 NTP 回拨改写。闸门不读它们。审计若也要防回拨，要另定口径。
3. 判定边界与原先一致：`withdraw` 要求 `deadline > now`，`execute` 在 `now < deadline` 时拒绝。正好走到截止点时窗口算关，允许发送、拒绝撤回。发送闸语义未改。
4. `gateway_meta.value` 仍是 TEXT。`mono_ms` 与纪元一样，读取后 parse，不按 i64 取列。

## 遗留

- 进程死亡到下次 `open` 之间，逻辑钟停在上次 API 写回的高水位，停机时长不累加。未再调用 API 就崩溃时，上次写入之后的进程内流逝也会丢掉，窗口可能比真实流逝更长。这是故意不把墙钟用于过期判定的结果。要在停机后按真实流逝关窗，需要跨进程、且不被 NTP 回拨的单调源；本 crate 没有。
- 未做同键并发压测（原 PoC 即单连接、非 Sync，本次未扩大范围）。
- 未重跑 workspace 全量构建，其他 crate 未改。
