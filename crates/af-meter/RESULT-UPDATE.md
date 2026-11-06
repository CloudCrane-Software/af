# RESULT-UPDATE — W2A：af-meter 分库 + outbox 账本

## ① 结论

**通过**。`af-meter` 增加一份声明对应一个 SQLite 文件的计数账本。`events` 幂等键 UNIQUE，`windows` 起止按钟面日期解析，`outbox` 与扣减在同一个 `BEGIN IMMEDIATE` 事务里。`user_version = 1`，重入时版本或声明对不上就启动失败。`recover` 重发未标记的补偿记录，接收方用幂等键去重。

`cargo test -p af-meter` **42 全绿**（原 25：15 单元 + 4 proptest + 5 路由 + 1 词表；新增 17 个账本集成测试；文档测试仍是 0）。`cargo clippy --all-targets -p af-meter -- -D warnings` 无本 crate 警告。内存 `Meter` / `BudgetLedger` 的已验证语义没有改。

## ② 步骤

1. 读完 `RESULT.md`、`budget.rs`、`tests/vocab.rs`、`lib.rs`。计数保持 `Count = u64`，金额保持 `Cents = i64`。`Meter` 仍是单消费者状态机，窗口仍按注入的整数秒滚动。这些结论没有改。持久化账本记的是计数，不是美分。
2. `Cargo.toml` 增加 `rusqlite = { workspace = true }`（0.32.1，bundled）、`chrono = "=0.4.45"`、`chrono-tz = "=0.10.4"`。后两个已由 af-calendar / af-workorder 锁定。`Cargo.lock` 只在 af-meter 的依赖列表里多了这三行，没有新包。根 `Cargo.toml` 未改。
3. `src/ledger/`：打开时设 WAL、`busy_timeout=5000ms`、`synchronous=NORMAL`、`foreign_keys=ON`。空库走 `Opened::Created`（建表、写入声明、`PRAGMA user_version = 1`）。`user_version == 1` 且声明一致走 `Opened::AlreadyDeclared`。版本不是 1、声明字段不一致、多了表、或 `user_version` 仍是 0 但表已经在，都返回错误，不改存量。
4. 窗口起止：注入的 Unix 毫秒先映到时区里的日历日，再取当地 00:00 与下一个日历边界的当地 00:00。周窗从周一开始。不用「现在 + 86400000 毫秒」。
5. `consume`：`BEGIN IMMEDIATE` 内幂等扣减。同键同数量不二次扣；同键不同数量拒绝且不改账。剩余不足则整笔回滚，不留空窗口。跨过钟面终点的下一次扣减才插入新窗口，旧窗口的已消耗留下。返回的 `Reading.recovers_at` 是窗口终点的 Unix 毫秒。
6. 扣减、事件、outbox JSON 同一事务提交。`recover` 按序把 `sent_at_ms IS NULL` 的行交给回调，回调成功后才把 `sent_at_ms` 写成注入时刻。回调失败则停在该条。标记之前崩溃会再送同一幂等键。
7. 测试不睡眠。并发用例用 `std::thread` 和 `Barrier`，每线程自己的连接。

## ③ 证据

命令均在 `source /data/af-factory/env.sh` 之后执行。

`cargo check -p af-meter -j 4`：`Finished dev profile ... in 5.70s`（首次编译 chrono-tz）。

`cargo clippy --all-targets -p af-meter -j 4 -- -D warnings`：`Finished dev profile ... in 0.89s`，退出码 0。cargo 另打印既有的 future-incompat：`proc-macro-error2 v2.0.1`，不是本 crate 的警告。

`cargo test -p af-meter -j 4`：

```
running 15 tests
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 17 tests
test bad_declaration_is_rejected_before_create ... ok
test fall_back_day_ends_at_next_civil_midnight ... ok
test ordinary_day_boundary_is_the_next_civil_midnight ... ok
test same_instant_falls_on_different_civil_days ... ok
test week_crossing_spring_forward_uses_civil_mondays ... ok
test spring_forward_day_ends_at_next_civil_midnight ... ok
test consume_is_idempotent_and_conflict_does_not_charge ... ok
test lazy_reset_opens_the_next_civil_window ... ok
test open_created_then_reopen_checks_recipe_and_declaration ... ok
test extra_table_fails_closed ... ok
test insufficient_rolls_back_and_restart_keeps_the_committed_charge ... ok
test user_version_mismatch_and_torn_init_fail_closed ... ok
test recover_redelivers_the_same_idempotency_key ... ok
test recover_persists_the_sent_mark_across_reopen ... ok
test declaration_mismatch_fails_closed_and_keeps_committed_rows ... ok
test concurrent_same_key_is_charged_once ... ok
test concurrent_consumers_conserve_the_window ... ok
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 4 tests
test result: ok. 4 passed; 0 failed
running 5 tests
test result: ok. 5 passed; 0 failed
running 1 test
test result: ok. 1 passed; 0 failed
Doc-tests af_meter
test result: ok. 0 passed; 0 failed
```

`Cargo.lock` 的 diff 只有 af-meter 依赖列表中的 `chrono`、`chrono-tz`、`rusqlite`。

提交：`f176cf3` af-meter: 落地分库账本、钟面窗口与 outbox。

## ④ 反馈

1. 账本读数复用 `Reading`，但 `recovers_at` 在这条路径上是窗口终点的 Unix 毫秒。内存 `Meter` 的 `Time` 仍是调用方注入的整数秒。两套数字不能混在同一次比较里。
2. `synchronous`、`foreign_keys`、`busy_timeout` 只活在当前连接上，每次 `open` 都要重设。`journal_mode=WAL` 和 `user_version` 落在文件头。bundled SQLite 编译带了 `SQLITE_DEFAULT_FOREIGN_KEYS=1`，原始连接默认也是开的；账本仍然显式执行这条 pragma。
3. 恰好一次是「至少送一次 + 幂等键」。回调成功和 `sent_at_ms` 落库之间如果进程停掉，下次 `recover` 会再送同一键。接收方必须用这个键去重。
4. 春跳日（柏林 2026-03-29）的钟面窗口短于 86400000 毫秒，秋拨日（2026-10-25）长于 86400000 毫秒，终点都是次日当地 00:00。这是「不用绝对时间加法」的检查。
5. 多消费者不共享一条 `Connection`。每个线程 `open` 同一个文件，写事务用 `BEGIN IMMEDIATE` 串行。4 线程 × 30 笔、容量 50，成功笔数等于 50，事件合计等于窗口已消耗。8 线程抢同一幂等键只入账一次。

## ⑤ 遗留

- 内存 `Meter` 的固定窗口仍从创建时刻按秒滚动。这是 `RESULT.md` 已验证的行为，本任务没有把它改成钟面窗口。两套窗口算法并存。
- `BudgetLedger` 的美分累计仍在内存里。本账本只记 `Count`。
- `RESULT.md` 里其余遗留仍在：固定窗口双倍突发、Max 的 `recovers_at` 是下界、`route` 不拆分任务。
- outbox 回调是进程内函数，不是网络发送。没有单独压测「回调已返回、标记尚未提交」的进程崩溃，测试用的是把 `sent_at_ms` 清回 NULL 再扫描。
- 两个连接同时第一次 `open` 空文件没有单独压测。初始化事务是 `BEGIN IMMEDIATE`，后到的一方会看到已经写好的 `user_version`。
- SQLite 仍是单写者。`busy_timeout` 为 5000 毫秒。更高的写入并发会在这把锁上等待。

---

# RESULT-UPDATE — C4：af-meter criterion 基准（波次 1 ⑦/T9-6 后半）

## ① 结论

**通过**。`af-meter` 增加 criterion 0.8.2 基准两条：窗口内逐事件 `try_consume`（记账扣减），以及一次 `given` 跨过大量错过窗口（滚转）。`cargo test -p af-meter` 全绿，21 个测试（15 单元 + 5 路由集成 + 1 词表集成），数量与本任务开始前相同。`cargo bench -p af-meter --bench meter_paths` 退出码 0。`cargo clippy --all-targets -p af-meter -- -D warnings` 无警告。

`RESULT.md` 记录的是更早的 10 个单元测试 + 5 个集成测试。当前树上另有 `budget.rs` 的 4 个单元测试、`lib.rs` 里后来补上的令牌桶取整测试，以及 `tests/vocab.rs` 的 1 个集成测试。这些在本任务开始前就在，本任务没有增删测试。

## ② 步骤

- `Cargo.toml`：dev-dependency `criterion = "0.8.2"`，与 `af-trace` 锁定的同一版本。`[[bench]] name = "meter_paths"`，`harness = false`，没有 `test = true`。
- 工作区 `Cargo.lock`：在 `af-meter` 的依赖列表里加上已有的 `criterion`。没有新的第三方版本。根 `Cargo.toml` 未改。
- `benches/meter_paths.rs` 两条基准。时钟是注入的整数秒。计时使用 `Instant`，夹具构造在窗外。每组 10 个样本，预热 1 秒，测量 3 秒。
  - `per_event_accounting/try_consume_in_open_window`：`Min(周窗 65536, 日窗 8192)`，注入时刻 `T0 = 1_700_000_000`。每轮 4096 次 `try_consume(1, T0)`。批大小小于两个窗口的容量，不跨 `reset_at`。窗外断言日窗、周窗各扣 4096，可用量等于日窗剩余，`recovers_at = T0 + 86400`。
  - `window_roll/given_across_missed_windows`：同一棵订阅树创建于时刻 0，先扣空当前日窗（窗外）。计时一次 `given(100_000 * 86_400)`。`now` 经 `black_box` 传入，滚转循环按注入的窗口数执行。窗外断言剩余量回到容量（错过的窗口不累积），日窗 `reset_at` 落到 `now` 之后的下一个日界，周窗落到下一个周界。
- 未改 `src/`。`Min` 各扣一份、窗口重置不累积，都沿用 `RESULT.md` 已验证语义。

## ③ 证据

命令均在 `source /data/af-factory/env.sh` 之后执行。

`cargo clippy --all-targets -p af-meter -- -D warnings`：`Finished dev profile [unoptimized + debuginfo] target(s) in 0.24s`，退出码 0，无本 crate 警告。

`cargo test -p af-meter`（输出里没有 `Running benches/`）：

```
running 15 tests
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 5 tests
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 1 test
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
Doc-tests af_meter
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

`cargo bench -p af-meter --bench meter_paths`：`Finished bench profile [optimized] target(s) in 59.54s`，退出码 0。`Gnuplot not found, using plotters backend`。

控制台 `time` 三元组是 criterion 的 typical（有斜率时用斜率）95% 区间。均值和标准差取 `target/criterion/<id>/new/estimates.json` 的 `mean` / `std_dev` 点估计。

```
per_event_accounting/try_consume_in_open_window
                        time:   [247.73 µs 258.13 µs 273.75 µs]
                        thrpt:  [14.963 Melem/s 15.868 Melem/s 16.534 Melem/s]
均值 257.38 µs，标准差 23.27 µs（95% 均值区间 [245.33 µs, 272.45 µs]）
10 个样本里 2 个 high severe 离群点
Collecting 10 samples in estimated 3.0076 s (12k iterations)

window_roll/given_across_missed_windows
                        time:   [70.825 µs 71.416 µs 72.186 µs]
                        thrpt:  [1.3853 Gelem/s 1.4002 Gelem/s 1.4119 Gelem/s]
均值 71.12 µs，标准差 0.653 µs（95% 均值区间 [70.82 µs, 71.55 µs]）
10 个样本里 1 个 high severe 离群点
Collecting 10 samples in estimated 3.0039 s (40k iterations)
```

吞吐量的“元素”：记账是 4096 次 `try_consume`；滚转是 100_000 个被跨过的日窗口（周窗在同一次 `given` 里按周界滚）。滚转每元素约 0.71 ns，是整数循环；记账每元素约 63 ns，每次 `try_consume` 都会走 `Min::read`。

## ④ 反馈

- 记账路径和滚转路径都在订阅形态 `Min(周窗, 日窗)` 上测。这是 `RESULT.md` 里路由场景的那棵树。`BudgetLedger::try_spend` 是另一条直线美分校验，没有窗口，这次没有单独开基准。
- `Min::read` 每次分配一个 `Vec<Reading>`。逐事件记账的时间里包含这次分配。这是现有实现，本次不改。
- 滚转循环次数随错过的窗口数线性增长，错过的额度不累积。基准把 `now` 藏进 `black_box`，避免优化器把循环折成一次赋值。断言要求扣空的日窗被重置回容量，用来确认测到的是滚转。
- 环境没有 gnuplot。criterion 改用 plotters，统计结果仍然打印，基准退出码为 0。

## ⑤ 遗留

- 每组 10 个样本，只说明路径可跑、数量级稳定。语义仍以 `cargo test` 为准。
- 记账基准固定在一个打开的窗口内。跨界那一下落在滚转基准里，两条路径没有合成同一次调用。
- 滚转吞吐量按日窗口计数，读的时候要和记账的“每事件”分开看。
- 未做多消费者、持久化、滑动窗口。与 `RESULT.md` ⑤ 相同，本次范围不含。

---

# RESULT-UPDATE.md — C2 af-meter proptest 补强（波次 1 ⑦/T9-7）

## ① 结论

**通过**。未改 Meter 树、`try_spend`、路由的已验证语义。`proptest =1.12.0` 作为 dev-dependency 引入。新增 `BudgetLedger::merge_consumers`（多消费者已花相加，与顺序无关）和 4 个属性测试。`cargo test -p af-meter` **25 全绿**（原 21：15 单元 + 5 路由 + 1 词表；新增 4 个 proptest；文档测试仍是 0）。`cargo clippy --all-targets -p af-meter -- -D warnings` 无新警告。

## ② 步骤

1. 读完 `crates/af-meter/RESULT.md`、`src/budget.rs`、`tests/vocab.rs`、`src/lib.rs`。计数保持 `Count = u64`，金额保持 `Cents = i64`。`try_spend` 拒绝负数消耗；`remaining_cents` 用 `saturating_sub`。Meter 仍是单消费者 `&mut self` 状态机。这些结论没有改。
2. `Cargo.toml` 增加 `proptest = "=1.12.0"`，只放 dev-dependencies，与 af-sampling、af-calendar 的版本钉死方式一致。`cargo check -p af-meter` 后 `Cargo.lock` 只在 af-meter 的依赖列表里多了一行已有的 `proptest`，没有新包，也没有改其他 crate。
3. `BudgetLedger::merge_consumers`：把各消费者的 `spent_cents` 相加，不重放消耗顺序。空切片得到 0/0；总额不一致报 `BudgetMismatch`（low/high 取最小与最大）；负数已花报 `NegativeSpend`（取最小的负数）；和超过总额且和不为 0，或和不进 `i64`，报 `OverBudget`。和为 0 时允许总额为负，负数余额来自总额本身。入参不修改。
4. `tests/properties.rs` 四组属性，每组 256 例，失败持久化关掉：
   - `window_quota_conserved_over_any_event_sequence`：任意注入时刻的观察/消耗序列下，已授予 + 剩余 + 已丢弃 = 容量 × 开窗数。跨窗用闭式计数，不复制实现里的 while。错过多个窗口只回到容量。`reset_at` 只前进。`window == 0` 不重置。
   - `min_windows_conserve_and_grant_the_same_amount`：Min 的两个窗口各自守恒；成功时两边都扣同一份 `need`，失败时两边都不扣。
   - `budget_spend_never_overdrafts`：拒绝不改账本；成功消耗 `>= 0` 且不超过 cap；已花等于成功消耗之和且不超过非负总额。总额为负时没有任何成功扣减，`remaining_cents == budget_cents`。
   - `consumer_ledger_merge_is_commutative`：同一组账本的反转、旋转、按已花排序，合并结果相同；成功时已花等于各消费者之和，且不透支。
5. 提交前跑过 `cargo fmt -p af-meter`、`cargo clippy --all-targets -p af-meter -j 4 -- -D warnings`、`cargo test -p af-meter -j 4`。cargo 命令前都 `source /data/af-factory/env.sh`。

## ③ 证据

```
$ source /data/af-factory/env.sh && cargo check -p af-meter -j 4
    Checking af-meter v0.1.0 (/data/af-factory/c2/crates/af-meter)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.28s
```

`Cargo.lock` 的 diff 只有 af-meter 依赖列表中的 `+ "proptest"`。

```
$ cargo clippy --all-targets -p af-meter -j 4 -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.40s
```

exit 0。cargo 另打印了一条既有的 future-incompat：`proc-macro-error2 v2.0.1`，不是本 crate 的 clippy 警告。

```
$ cargo test -p af-meter -j 4

running 15 tests
...
test result: ok. 15 passed; 0 failed

running 4 tests
test window_quota_conserved_over_any_event_sequence ... ok
test min_windows_conserve_and_grant_the_same_amount ... ok
test consumer_ledger_merge_is_commutative ... ok
test budget_spend_never_overdrafts ... ok
test result: ok. 4 passed; 0 failed; finished in 0.04s

running 5 tests
test result: ok. 5 passed; 0 failed

running 1 test
test credential_limit_consumed_without_unit_conversion ... ok
test result: ok. 1 passed; 0 failed

Doc-tests af_meter
test result: ok. 0 passed; 0 failed
```

反例可读性用一次故意写错的断言确认过（`total_granted == 0`，随后已删，上表是删掉后的全绿结果）。缩小结果是：

```
故意失败以观察反例 granted=1 at crates/af-meter/tests/properties.rs:294.
minimal failing input: case = WindowCase {
    window: 0,
    capacity: 6,
    origin: 0,
    events: [
        Consume {
            now: 0,
            need: 1,
        },
    ],
}
```

没有留下 `proptest-regressions/`。

提交：

- `a10215f` af-meter: 引入 proptest 1.12.0 作为 dev-dependency
- `5073454` af-meter: 补三组 proptest，并加上可交换的多消费者账本合并

## ④ 反馈

1. 多消费者若按流水重放，超额时谁先谁后会改变最终已花，合并不可交换。`merge_consumers` 因此只加总已经入账的 `spent_cents`，超额就整体拒绝。这和「记账合并可交换」一致，也没有把 Meter 改成并发状态机。
2. 负数美分只出现在两处已有语义里：`spend_cents < 0` 被拒绝；`budget_cents < 0` 时 `remaining_cents` 就是这个负总额（`saturating_sub`）。成功扣减不会把已花或非负总额的余额打成负数。
3. `window == 0` 时 `refresh` 直接返回，但 `read` 在剩余小于容量时仍给出 `recovers_at = Some(reset_at)`。属性测试按这个已有行为钉住，没有改成「真的会恢复」。
4. 生成器以小整数为主，极端值只有 `0` / `-1` / `i64::MIN` / `i64::MAX`。256 例是抽样，不是穷尽证明。

## ⑤ 遗留

- RESULT.md 里 Meter 的遗留仍在：单消费者、无持久化、固定窗口双倍突发、Max 的 `recovers_at` 是下界、`route` 不拆分任务。本任务不改这些。
- `merge_consumers` 不是锁，也不是并发扣减。两个消费者要先各自 `try_spend`，再把账本交过来合并。
- `proptest` 只写在 af-meter 的 dev-dependencies。根 `Cargo.toml` 不在本任务改动范围内，没有收进 `[workspace.dependencies]`。

---

# RESULT-UPDATE — W4D：af-meter 的 Bifrost 双层接线（weight + header）

## ① 结论

**通过**。`af-meter` 新增 `bifrost`：纯函数 `render(keys, now)` 把供给水位（`Meter::given(now).available`）或预算余额（`BudgetLedger::remaining_cents`）写成 Bifrost 配置片段。`keys[].weight` 是 0–100 的百分整；`governance.routing_rules` 用请求头 `x-af-pin-key` 钉住 key。主 key 量化后为 0 且没有正权重的备 key 时拒绝生成。负余额拒绝生成。调用方的计量树不被刷新。

内存 `Meter`、`BudgetLedger`、SQLite 账本的已验证语义没有改。

`cargo test -p af-meter` **60 全绿**（原 42：15 单元 + 17 账本 + 4 proptest + 5 路由 + 1 词表；新增 15 个 bifrost 单元测试和 3 个金标集成测试；文档测试仍是 0）。`cargo clippy --all-targets -p af-meter -- -D warnings` 无本 crate 警告。

## ② 步骤

1. 读完 `RESULT.md`、`RESULT-UPDATE.md`、`budget.rs`、`lib.rs`。计数仍是 `Count = u64`，金额仍是 `Cents = i64`。`remaining_cents` 是普通减法，只有 `i64` 下溢才停在 `i64::MIN`（`10 - 20 = -10`，不是 0）。`Meter::given` 仍会刷新调用方手里的树。这些结论没有改。Bifrost 研究原文不在本副本；权重公式对照的是 core `keyselectors/weightedrandom.go`（`totalWeight += int(key.Weight * 100)`，和为 0 时均匀随机，和为负时 `rand.Intn` panic），路由目标的 `weight` 对照的是 governance routing rule（一条规则内的命中概率，必须加总为 1）。
2. `src/bifrost.rs`：一组 key 必须有唯一主 key、至少一把备 key、同一个 provider。余额按合计归一，`quantize_percent` 做一半向上的四舍五入，各自落在 0–100，不把和再摊回 100。保底校验发生在量化之后：主 key 为 0 且每把备 key 也是 0（含 201 份相等余额被舍成全 0）就返回 `ZeroWeights`，不产出 JSON。`targets[].weight` 恒为 `1.0`，不抄 key 的百分数。密钥只接受 `env.VAR`。`now` 只传给供给的克隆；账本不看时刻，但仍把 `now` 写入 `generated_at`。
3. 金标三份，insta 1.49.0（锁里已有，只加到本 crate 的 dev-dependencies，`default-features = false`，feature `json`）。`Cargo.lock` 只在 af-meter 的依赖列表加了一行 `insta`，没有新包，没有改其他 crate 的源码。快照直接入库，不走 `cargo insta review`。
4. 提交前跑过 `cargo fmt -p af-meter`、`cargo clippy --all-targets -p af-meter -j 4 -- -D warnings`、`cargo test -p af-meter -j 4`。cargo 命令前都 `source /data/af-factory/env.sh`。

## ③ 证据

命令均在 `source /data/af-factory/env.sh` 之后执行。

`cargo check -p af-meter -j 4`（模块落地后）：`Finished dev profile ... in 0.51s`。

`cargo clippy --all-targets -p af-meter -j 4 -- -D warnings`（快照入库后）：`Finished dev profile ... in 2.36s`，退出码 0。cargo 另打印既有的 future-incompat：`proc-macro-error2 v2.0.1`，不是本 crate 的警告。

`cargo test -p af-meter -j 4`（未设 `INSTA_UPDATE`，快照只比对）：

```
running 30 tests
test bifrost::tests::all_zero_weights_are_refused ... ok
test bifrost::tests::budget_ratio_uses_remaining_cents ... ok
test bifrost::tests::exact_exhaustion_is_zero_weight_not_a_negative ... ok
test bifrost::tests::header_pin_weight_is_not_the_key_percent ... ok
test bifrost::tests::injected_now_refreshes_a_clone_and_leaves_the_supply ... ok
test bifrost::tests::rounding_collapse_to_all_zero_is_refused ... ok
test result: ok. 30 passed; 0 failed

running 3 tests
test budget_balances_80_20 ... ok
test supply_exhausted_primary_weight_zero ... ok
test supply_recovers_on_injected_day_boundary ... ok
test result: ok. 3 passed; 0 failed

running 17 tests
test result: ok. 17 passed; 0 failed

running 4 tests
test result: ok. 4 passed; 0 failed

running 5 tests
test result: ok. 5 passed; 0 failed

running 1 test
test result: ok. 1 passed; 0 failed

Doc-tests af_meter
test result: ok. 0 passed; 0 failed
```

60 = 原 42 + 15 个单元 + 3 个金标。单元里的 15 个旧测试（账本 4 + 计量树 11）数量没变。

金标要点：预算剩余 800 与 200 → `keys[].weight` 80 和 20，`generated_at` 1700000000。日窗耗尽、`now = 86399` → 订阅权重 0、API 权重 100，钉住规则的 `targets[].weight` 仍是 `1.0`。`now = 86400` 日界重置后水位回到日窗容量 200（周窗剩余 800 不单独进权重）→ 2 和 98。

`Cargo.lock` 的 diff 只有 af-meter 依赖列表中的 `+ "insta"`。

提交：

- `f8c8920` af-meter: 用余额比例生成 Bifrost 的 weight 与 header 钉住
- `58c9852` af-meter: 补 Bifrost 配置金标快照

## ④ 反馈

1. `keys[].weight` 写成百分整，不是文档示例里的 0.7。V1.6 选择器还会再做 `int(w*100)`，每把 key 同乘 100，80:20 仍是 80:20。一半向上和 Go 的截断不一样：正好 0.5% 这里是 1，截断会收成 0，进而掉进全零均匀随机。
2. `targets[].weight`、`keys[].weight`、虚拟 key 的 `provider_configs[].weight` 是三个字段。本片段只写前两个。钉住规则单目标恒为 1，表示这条规则的命中概率，不加总到 100，也不做 `int(w*100)`。
3. 保底是拒绝，不是把备 key 抬到 1。主 key 为 0、备 key 有正的量化权重时允许下发（0 与 100），全零不下发。201 份相等的正余额会各自舍成 0，同样拒绝。
4. 主 key 权重为 0 时，它的钉住规则还在，`targets[].weight` 仍是 1.0。请求带 `x-af-pin-key` 可以盖过 weight 层，选中这把耗尽的 key。
5. 直接拼出来的账本如果已花大于总额，`remaining_cents` 是负数（`saturating_sub` 只防 `i64` 下溢）。这种余额会让 V1.6 panic，生成器拒绝。`try_spend` 本身仍然不会把账本记成这种状态。
6. `generated_at` 不是 Bifrost 的字段。Go 的 `encoding/json` 默认丢掉未知字段，所以片段仍能喂进去；它用来钉住注入时刻。预算比例不随 `now` 变，供给水位会变。
7. 权重在整组 key 上归一，并且要求同一个 provider。Bifrost 只在一个 provider 的 key 之间比较 `keys[].weight`。跨 provider 的比例是另一层 weight，这里直接拒绝，避免写出看起来按比例、实际各算各的配置。
8. 独立四舍五入的和可以是 99 或 101（余额 1 与 199 → 权重 1 与 100）。选择器按相对权重抽，不要求和为 100。

## ⑤ 遗留

- `RESULT.md` 里 Meter 的遗留仍在：单消费者、内存树无持久化、固定窗口双倍突发、Max 的 `recovers_at` 是下界、`route` 不拆分任务。本任务不改这些。
- SQLite 计数账本没有接进这个片段。生成器只读内存里的 `Meter` 和 `BudgetLedger`。
- 没有对 Bifrost 的 Go 进程做加载测试。JSON 形状按 `providers` 与 `governance.routing_rules` 对齐；`generated_at` 依赖解码器忽略未知字段，没有拿发布的 JSON Schema 再验一次。
- 钉住规则不改写 model。没写的 model 沿用请求里的模型。
- 多 provider 的预算拆分不生成。虚拟 key 上的 `provider_configs[].weight` 也不生成。
- 量化后和不为 100 时，没有用最大余数法把百分数摊平。
- header 覆盖是声明式规则，本 crate 不跑 CEL。
- 很多把极小余额被舍成全 0 时，整份配置拒绝，不会把最大的那把抬到 1。
- `Meter::given` 跨过大量窗口仍是按窗循环。生成器继承这个开销，没有另写闭式。调用方不要对很细的窗口传入过大的 `now`。
