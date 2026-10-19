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
