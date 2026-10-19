# RESULT-UPDATE.md — B5 af-station 并发竞争补强

## ① 结论

**通过**。未改认领 / 心跳 / 回收 / 完成 / 幂等键的 SQL 语义（RESULT.md 已验证的四条保持）。`cargo test -p af-station` **22 全绿、0 失败**（原 8 个单测仍在，另加 14 个）。`cargo clippy --all-targets -p af-station -- -D warnings` 无警告。

新增测试：

- 单测 +3：手动钟克隆共享、系统钟读 Unix 毫秒、主键冲突不是 Duplicate。
- 集成 +8：八线程抢同一任务恰好一次认领并完成、32 行并发 complete 不丢行、非主人 complete 风暴不改账、4 线程认领/完成流水线分区、租约内断线重入、过期后续点回到 queued 再认领、回收前后两条完成顺序、过期原主 complete 与新主 claim 竞争只留一行。
- 属性 +3（proptest 1.12.0，dev-dependency）：操作序列对照影子模型并在中点重开连接、并发认领/完成是划分、并发 complete 不丢行。

租约时刻改为注入。[`Station::open`] / [`Station::open_in_memory`] 仍读本机墙钟。测试用 [`ManualClock`]，仓库内不再有 `sleep`。

## ② 步骤

1. 读完 `crates/af-station/RESULT.md` 与 `src/lib.rs`。认领仍是 `WHERE claimant IS NULL` 的守卫 UPDATE；心跳仍要求 `lease_until > now` 并以该 now 重置截止点；回收仍是 `lease_until < now` 打回 queued；完成仍只看现任 claimant 且 `state = 'claimed'`，不删行、不看租约。这些结论没有改 SQL 去推翻。
2. `src/clock.rs` 增加 `Clock` / `SystemClock` / `ManualClock`。`claim` / `heartbeat` / `reclaim_expired` 每次调用只读一次时刻，回收与新租约、心跳守卫与新截止点共用这一读数。两个原睡眠用例（心跳续期、租约过期接管）改成推进 `ManualClock`。顺手消掉本 crate 原有的 3 条 clippy（`doc_lazy_continuation`、`needless_question_mark`、`map_flatten`），否则 `-D warnings` 过不了。
3. `tests/concurrency.rs`：`Barrier` 对齐多连接。覆盖「同一任务只能被一个工位认领」「并发 complete 不丢行」「中断后 queued / claimed 续点可重入」。过期与接管的时刻全部用 `ManualClock::set` / `advance`。竞争用例接受两种合法交错（原主先完成，或回收后新主认领），两种都断言只剩一行。
4. `Cargo.toml` 增加 dev-dependency `proptest = { version = "=1.12.0", default-features = false, features = ["std"] }`。关掉默认特性，避免 `bit-set` 把 `borsh` derive 统一进工作区。`tests/properties.rs` 用操作序列生成器对照影子模型；序列中点丢掉连接再打开；结尾把时刻推过全部租约，未完成的行回到 queued 后再次认领并完成，id 集合不变。失败持久化关掉，不写 `proptest-regressions/`。`Cargo.lock` 只给 `af-station` 加上已有的 `proptest` 依赖边，没有新包、没有改版本。
5. 提交：`0aaf24f` 时刻注入，`36de9c9` 并发集成测试，`e6c3b5b` proptest。命令前均 `source /data/af-factory/env.sh`。

## ③ 证据

工具链：`rust-toolchain.toml` 的 `1.99.1` 与 `stable` 都是 `rustc 1.99.1 (58cf8f9c3 2026-10-13)`。会话前半段命名工具链目录缺 `rustc`（另一工作区正在安装），当时用 `cargo +stable`（同一 rustc）。下面三条是工具链补齐后的验收命令。

```
$ source /data/af-factory/env.sh && cargo fmt -p af-station -- --check
FMT:0

$ cargo clippy --all-targets -p af-station -j 4 -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.33s
CLIPPY:0

$ cargo test -p af-station -j 4
running 11 tests
test clock::tests::manual_clock_set_and_advance_are_shared_by_clones ... ok
test clock::tests::system_clock_reads_unix_millis ... ok
test tests::primary_key_conflict_is_not_reported_as_duplicate ... ok
test tests::enqueue_and_get_roundtrip ... ok
test tests::role_routes_claim_to_matching_worker_only ... ok
test tests::idempotency_key_prevents_duplicate_enqueue ... ok
test tests::heartbeat_renews_lease_and_only_claimant_can_use_it ... ok
test tests::expired_lease_is_reclaimed_and_takeover_works ... ok
test tests::end_to_end_lifecycle ... ok
test tests::two_threads_race_on_same_task_exactly_one_wins ... ok
test tests::stress_four_threads_drain_forty_tasks_without_double_claim ... ok
test result: ok. 11 passed; 0 failed; finished in 0.02s

running 8 tests
test expired_complete_before_reclaim_and_reclaim_before_complete ... ok
test resume_while_lease_holds_is_reentrant ... ok
test expired_owner_complete_races_takeover_keeps_one_row ... ok
test expired_interrupt_reenters_queued_resume_point ... ok
test eight_threads_claim_and_complete_one_task_exactly_once ... ok
test foreign_complete_storm_leaves_every_claimed_row ... ok
test interleaved_claim_and_complete_partition_without_losing_rows ... ok
test concurrent_complete_does_not_drop_ledger_rows ... ok
test result: ok. 8 passed; 0 failed; finished in 0.07s

running 3 tests
test concurrent_claim_complete_is_a_partition ... ok
test concurrent_complete_keeps_every_row ... ok
test op_sequence_matches_model_across_reopen ... ok
test result: ok. 3 passed; 0 failed; finished in 0.55s

Doc-tests af_station
test result: ok. 0 passed; 0 failed
```

开发期间把「过期竞争 / 八线程单任务 / 32 行并发 complete」连跑 20 轮，每轮通过。属性测试 96 条序列 + 各 24 条并发用例，一次全绿。

## ④ 反馈

1. 时刻注入是租约测试的正确入口。生产 `open` 仍用墙钟，多连接共享同一 `ManualClock` 才能在不睡眠的情况下对齐过期点。心跳与认领每次只取一个 now，避免守卫和写入的截止点跨过一次时钟跳动。
2. **完成不看租约**是现有 SQL，不是这次新行为。回收发生前，过期原主仍能 `complete`；回收之后原主失败、别人认领同一行。两条路都只留一行。若最终方案希望「过期即不可完成」，要在 `complete` 的 WHERE 里加上 `lease_until > now`，并和心跳守卫一起改。本次按 RESULT.md 未改这条。
3. `lease_until == now` 既不算过期（回收条件是 `<`），也不能心跳（守卫是 `>`）。差 1 毫秒才进入 queued 续点。测试把这个边界钉住了。
4. 幂等键坑再次对上 RESULT.md ④.3：同一 id 且同一 key 返回 `Duplicate`；同一 id、不同 key（或 key 为 NULL）是主键上的裸 Sqlite 错误，不是 `Duplicate`。

## ⑤ 遗留

- `README.md` 仍写「8 测试」。改 README 不在本任务范围。
- `RESULT.md` 的行号已随时钟注入漂移，结论未改，故没有重写那份账本。
- `claim` 的 `LIMIT 64` 候选窗口仍未做万级压测（原遗留）。
- done 之后的审计查询、优先级、取消/作废状态仍未做（原遗留，超出本次范围）。
- 完成是否应在过期后失败，见上一节第 2 条，本次未改行为。
- 属性测试用例数是 96 / 24，不是穷举。反例不落盘。
