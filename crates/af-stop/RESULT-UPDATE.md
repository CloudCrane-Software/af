# RESULT-UPDATE.md — R2 af-stop proptest 属性测试

## ① 结论

**通过**。`crates/af-stop` 增加 dev-dependency `proptest 1.12.0`（只开 `std`），并补上三组属性测试。引擎实现未改，RESULT.md 里的止损语义没有被推翻。

测试从 12 个（11 单测 + 1 doctest）增到 18 个。多出来的 6 个都是新的属性测试。`cargo test -p af-stop` 全绿，`cargo clippy --all-targets -p af-stop -- -D warnings` 无警告。

## ② 步骤

1. 读完 `crates/af-stop/RESULT.md` 和 `src/lib.rs`。优先级维持为：check 首败 > 指纹连击 > 普通错误阈值。`streak_limit = 0` 是白名单。普通错误计数在递增后用 `>= M` 判断。引擎不锁存停机态。连击被成功、不同指纹、check、普通错误打断。
2. 在 `crates/af-stop/Cargo.toml` 加入 `proptest = { version = "1.12.0", default-features = false, features = ["std"] }`。默认特性里的 `bit-set` 会经 `bit-vec 0.10` 把 `borsh` 的 derive 特性统一进整个工作区，所以关掉。`fork` / `timeout` 本任务不用。
3. `cargo check` 更新了工作区 `Cargo.lock`：新增 proptest 及其 std 依赖，并把已有的同名包依赖写成带版本号的形式（如 `rand_core 0.6.4` 与 `rand_core 0.10.1`）。没有改动其他 crate 的源码，也没有改它们的版本号。
4. 新增 `crates/af-stop/tests/properties.rs`，三组共 6 个测试：
   - 指纹相同则处置确定且一致：`same_fingerprint_disposition_is_deterministic_and_consistent`、`equivalent_fingerprints_share_the_same_disposition`
   - 规则优先级不随注入顺序改变：`rule_priority_invariant_under_store_injection_order`、`check_priority_invariant_under_event_injection_order`、`fingerprint_priority_invariant_under_event_injection_order`
   - 止损计数永不回退：`stop_counter_never_regresses`（普通错误计数只增不减且等于工具错误 + 普通错误个数；连击长度只 +1，或在打断后落到空 / 1，不做部分回退）
5. 每组生成器只用 `search` / `browser` / `deploy` / `db` 和 `E_TIMEOUT` / `E_LOCK` / `E_5XX` / `E_NAV`，阈值在 `0..=8`。失败持久化关掉，避免写出 `proptest-regressions/`。
6. 提交前跑过 `cargo fmt -p af-stop`、`cargo clippy --all-targets -p af-stop -j 4 -- -D warnings`、`cargo test -p af-stop -j 4`。所有 cargo 命令前都 `source /data/af-factory/env.sh`。

## ③ 证据

```
$ source /data/af-factory/env.sh && cargo check -p af-stop -j 4
     Locking 10 packages to highest compatible versions
      Adding proptest v1.12.0
      Adding rand v0.10.3
      ...
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.97s
```

```
$ cargo clippy --all-targets -p af-stop -j 4 -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.16s
```

```
$ cargo test -p af-stop -j 4

running 11 tests
test tests::rule1_fingerprint_streak_stops_at_nth ... ok
test tests::rule1_different_error_code_is_different_fingerprint ... ok
test tests::rule1_success_breaks_the_streak ... ok
test tests::rule1_alternating_tools_never_triggers ... ok
test tests::rule2_check_first_failure_stops_immediately ... ok
test tests::rule3_generic_errors_escalate_at_threshold ... ok
test tests::rule3_tool_errors_also_count_toward_threshold ... ok
test tests::fingerprint_stop_wins_over_threshold_escalate ... ok
test tests::store_override_whitelisted_fingerprint_never_stops ... ok
test tests::store_override_lower_limit_and_escalate_action ... ok
test tests::mixed_stream_end_to_end ... ok
test result: ok. 11 passed; 0 failed

running 6 tests
test same_fingerprint_disposition_is_deterministic_and_consistent ... ok
test equivalent_fingerprints_share_the_same_disposition ... ok
test rule_priority_invariant_under_store_injection_order ... ok
test fingerprint_priority_invariant_under_event_injection_order ... ok
test check_priority_invariant_under_event_injection_order ... ok
test stop_counter_never_regresses ... ok
test result: ok. 6 passed; 0 failed; finished in 0.13s

Doc-tests af_stop
test result: ok. 1 passed; 0 failed
```

反例可读性用一次故意写错的断言确认过（`count > previous`，随后已改回 `>=`，上表是改回后的全绿结果）。缩小结果是：

```
止损计数回退 step=0: 0 -> 0 event=ToolOk { tool: "search" } decision=Continue registry=[]
minimal failing input: streak_limit = 0, threshold = 0, registry = [], events = [
    ToolOk { tool: "search" },
]
```

这次失败没有留下 `proptest-regressions/`。

提交：

- `07958d8` af-stop: 引入 proptest 1.12.0 作为 dev-dependency
- `bc4cf88` af-stop: 补三组 proptest 属性测试

## ④ 反馈

1. 公开接口够用。`feed`、`current_streak`、`normal_error_count` 和可克隆的 `StopPolicy` 就能把三组不变量钉死，不必为测试改引擎。
2. proptest 在本工作区不宜开默认特性。`bit-set` 会把 `borsh` derive 特性统一到所有使用 `borsh 1.8.1` 的 crate。只开 `std` 后，锁文件只新增 proptest 这条依赖链。
3. `prop_assert!` / `prop_assert_eq!` 的格式串不能用隐式捕获，消息要写成 `"step={}", step`。生成器保持小字母表之后，缩小反例可以直接读。
4. 「check 首败」在当前实现里是每一次 `CheckFailed` 都返回 `Stop(CheckFirstFail)`，不是只停第一次。这和引擎文档一致，属性测试按这个语义钉住了。最终方案若要改成真正的一次锁存，需要同时改文档和这组测试。

## ⑤ 遗留

- RESULT.md 里原有的遗留仍在：停机不锁存、错误码只做精确匹配、没有 serde、没有时间窗。本任务不改这些。
- `proptest` 只写在 `af-stop` 的 dev-dependencies 里。根 `Cargo.toml` 不在本任务改动范围内，没有收进 `[workspace.dependencies]`。
- 指纹库注入顺序这组用了 4 个固定槽位。任意工具名 / 错误码字符串的纯连击由第一组覆盖。
