# RESULT-UPDATE — T7 af-effect-gateway 计时换单调钟

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
