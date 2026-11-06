# RESULT-UPDATE.md — W2G af-station 分派路由 + 班次日历

## ① 结论

**通过**。`cargo test -p af-station` **56 全绿、0 失败**。`cargo clippy --all-targets -p af-station -- -D warnings` 无警告。

B5 的 22 个测试都还在。本波次多 34 个：开库声明 7、班次失败关闭 2、`tests/shift_window.rs` 25。认领 / 心跳 / 回收 / 完成的 SQL 守卫没有改。幂等键仍是 `ON CONFLICT(idempotency_key)`，主键冲突仍是裸 Sqlite 错误。

分派在角色精确匹配之外加了班次门。角色或工位绑定 `af-calendar::Calendar` 后，只有 `is_working_instant(now)` 为真才 `claim`。没绑定不设门。非班次返回 `Ok(None)`，任务保持 queued。SLA 截止是 `DateTime<Utc>`，落盘为 unix 毫秒。

## ② 步骤

1. 读了 `crates/af-station/RESULT.md`、B5 的 `RESULT-UPDATE.md`（`Clock` / `ManualClock`、完成不看租约、`lease_until == now` 的边界），以及 `crates/af-calendar/RESULT.md`（29 个常规测试里 SLA 18 + DST/夜班 11，另有 1 个 ignored 密扫）。没有改 `af-calendar`。
2. `b5e3dc0`：开库改为声明。空库建表并写入 `schema_version=1` 与指纹，返回 `OpenStatus::Created`。重入对照 `sqlite_master` 的建表 SQL 和声明行，一致才返回 `AlreadyDeclared`。版本、指纹、缺表、多列、预先存在的 `task` 表都 `DeclarationMismatch`，不补表、不收养。每次连接重设 WAL、`busy_timeout=5000`、`synchronous=NORMAL`、`foreign_keys=ON`。`Opened` 对 `Station` 做 `Deref`，原调用点不用改签名。
3. `40ef916`：`claim` 仍先用同一次 `now_ms` 回收过期租约，再问班次。角色日历与工位日历都要在班次内；只绑一边就只查那一边。心跳和完成不看班次。`enqueue` 接受 `Option<DateTime<Utc>>`，亚毫秒拒绝落盘。`sla_deadline_utc` 把日历解析结果折成 UTC。`Cargo.lock` 只给 `af-station` 加上已有包的依赖边：`af-calendar`（路径）、`chrono 0.4.45`、`serde_json`、dev 的 `chrono-tz 0.10.4`。没有新包，没有改版本。
4. 班次测试用 `ManualClock` 设绝对时刻。夜班沿用日历的两窗口（`[22:00, 24:00)` 用 `23:59:59.999999999` 逼近，加次日 `[00:00, 06:00)`）。DST 用 Europe/Berlin 2026-03-29 春跳和 2026-10-25 秋拨。每条持久化写入都有「写完 → drop → 重开 → 原样可读」，重开断言 `AlreadyDeclared`。

## ③ 证据

工具链：`source /data/af-factory/env.sh` 之后，`rustc 1.99.1`。命令都在仓库根。

```
$ cargo check -p af-station -j 4
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.32s
CHECK:0

$ cargo fmt -p af-station -- --check
FMT:0

$ cargo clippy --all-targets -p af-station -j 4 -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
CLIPPY:0

$ cargo test -p af-station -j 4
running 20 tests
test result: ok. 20 passed; 0 failed; finished in 0.04s

running 8 tests
test result: ok. 8 passed; 0 failed; finished in 0.06s

running 3 tests
test result: ok. 3 passed; 0 failed; finished in 0.63s

running 25 tests
test result: ok. 25 passed; 0 failed; finished in 0.03s

Doc-tests af_station
test result: ok. 0 passed; 0 failed
```

20 = 原 11 个单测 + 开库声明 7 + 坏日历 2。8 与 3 是 B5 的并发集成和 proptest，断言没改，只把 `open_with_clock` 的返回值 `into_station()`。25 全在 `tests/shift_window.rs`。合计 56。

`Cargo.lock` 的 diff 只有 `af-station` 的依赖列表多了四行，没有新的 `[[package]]`。

## ④ 反馈

1. **没绑定日历和绑定一张空日历不是一回事。** 前者保持 B5 的「只按 `required_role` 精确匹配」，现有并发和属性测试才不用改影子模型。后者 `is_working_instant` 恒为 false，永不派单。
2. **非班次的 `claim` 仍会回收过期租约，然后返回 None。** 任务回到 queued，但这次调用不把它派出去。已认领且租约未过期的行，下班后保持 claimed；心跳和完成都不看班次。完成仍不看租约：回收发生前，过期原主还能 `complete`。这是 RESULT.md / B5 的 SQL，这次没有改 WHERE。
3. **夜班继续拆成两个窗口。** `Shift` 在构造期拒绝 `start >= end`，本任务不能改 `af-calendar`。午夜接缝的 1ns 低于账本的毫秒钟，`23:59:59` 和次日 `00:00:00` 都会派单，测试里没有观察到空档。
4. **DST 沿用日历已验证的裁决，没有在账本里重写工时算术。** 窗口看本地钟面。春跳当天 `[02:00, 03:00)` 没有可注入的时刻，间隙两侧都不派；前一天 02:30 会派，用来证明窗口本身是开的。秋拨的两个 02:30（`earliest` / `latest`）钟面相同，两次都派。`is_working_instant` 不消歧，和日历 RESULT ⑥ 的坑 3 一致。
5. **deadline 不参与派单。** 逾期任务在班次内仍可认领，班次外仍保持 queued。判定式与 af-workorder `Sla::is_overdue` 相同：`deadline < now` 才算逾期，恰在截止不算。周五 16:00+08、承诺 4 工时，得到 `2026-10-26T03:00:00Z`。同一幂等键再入账不改已有 deadline。带亚毫秒的时刻直接拒绝，避免截断后读回变成另一个时刻。
6. **旧库不会被静默升级。** B5 之前只有 `task` 表、没有 `station_decl` 的文件，open 失败。声明行被改、或 `ALTER TABLE` 增删列，也失败。外键开关是连接态，每次 open 都打开；用临时父子表确认悬空引用会被拒绝，探针表在关闭前删掉，重开仍是 `AlreadyDeclared`。

## ⑤ 遗留

- `README.md` 仍写「8 测试」。改 README 不在本任务范围。
- `RESULT.md` 的行号和「8/8」计数已过时。四条 SQL 结论没有推翻，所以没有改那份账本。
- `claim` 的 `LIMIT 64`、done 之后的审计查询、优先级、取消/作废，仍是原遗留。
- 完成是否应在租约过期后失败，仍未改行为。见上一节第 2 条。
- 夜班的单窗口跨午夜、以及 `is_working_instant` 对歧义钟面不消歧，都在 `af-calendar`。本 crate 只消费它。
- 属性测试不绑定班次，影子模型因此没有班次状态。班次门由 `tests/shift_window.rs` 的定点用例覆盖，不是 proptest。
- 派单不按 deadline 排序。截止时刻只用于逾期判定。
- `task` 与 `shift_binding` 之间没有外键。`foreign_keys=ON` 已打开，但这两张表谁也不引用谁。

---

# 历史 · B5 af-station 并发竞争补强

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
