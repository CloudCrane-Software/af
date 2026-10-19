# RESULT-UPDATE.md — W2D af-stop 持久化 + serde 边界

## ① 结论

**通过**。指纹账本落在单个 SQLite 文件里，一行是「指纹 → 计数 / 处置 / 时间窗」。`open` 对空库返回 `Created`，对已有库返回 `AlreadyDeclared`。schema 版本或声明（连击阈值、普通错误阈值、时间窗、指纹库）对不上就启动失败，不改已有行。对外 JSON 带 `v`，只接受 1；缺版本、读不出整数、或不是 1，都直接拒绝。

已验证的止损语义没有改：check 首败 > 指纹连击 > 普通错误阈值；`streak_limit = 0` 仍是白名单；普通错误在递增后用 `>= M` 判断；引擎不锁存停机。时间窗到期只滚动窗口，不把计数清零，也不改裁决。

`cargo test -p af-stop` 全绿：单元 29、重启集成 12、属性 7、doctest 1，合计 49。相对本任务开始前的 18（单元 11 + 属性 6 + doctest 1）新增 31。`cargo clippy --all-targets -p af-stop -- -D warnings` 无警告。

## ② 步骤

1. 读完 `RESULT.md` 与 `RESULT-UPDATE.md`（R2）。优先级、白名单、阈值和「计数不回退」按已验证语义保留。`StopEngine::feed` 的三条规则没有改判断顺序。
2. 时钟注入：`Clock::monotonic_ms` 管时间窗，`wall_ms` 只写入 `observed_wall_ms`。逻辑毫秒是库里的高水位加上本次打开之后的流逝。关闭期间的钟面推进和墙钟回拨都不推动过期。
3. serde 边界：领域类型 `derive(Serialize, Deserialize)`，作为信封的 `body`。`Versioned`、`LedgerDeclaration`、`FingerprintRecord`、`EngineSnapshot` 自带 `v`。`encode_wire` / `decode_wire` / `decode_doc` 先看 `v`，不是 1 就不解析 body。写出时 `v` 不是 1 也拒绝。
4. 账本：`rusqlite` bundled，WAL、`busy_timeout=5000ms`、`synchronous=NORMAL`、`foreign_keys=ON`，`user_version=1`。表是 `declaration`、`runtime`（外键指向声明）、`fingerprint`。没有用 `CREATE TABLE IF NOT EXISTS`：`user_version` 与表是否存在对不上，按损坏拒绝，不补建。
5. `feed` 仍走原来的引擎，再把运行状态和工具错误的指纹行写进同一事务。指纹 `count` 只做 +1；SQL 的 `WHERE excluded.count > fingerprint.count` 写不进去就失败。普通错误计数和逻辑毫秒同样拒绝变小。连击仍可被成功、别的指纹、check、普通错误打断，这是原规则，不是账本回退。
6. 重启恢复测试覆盖：声明重入、版本不符不初始化、四类声明不符且不改行、指纹库登记顺序、指纹行原样读回、引擎计数原样读回、check 优先级跨重启、时间窗滚动后计数不降、墙钟回拨、关闭期间推进注入钟、白名单跨重启、与纯引擎逐步对齐。
7. 属性测试新增 `fingerprint_count_does_not_regress_across_restart`（64 个用例，失败持久化关闭）。中途 drop 再打开，行与重开前相等；最终计数等于该指纹的工具错误次数。
8. 依赖只用工作区已有包：`rusqlite 0.32.1`、`serde 1.0.229`、`serde_json 1.0.154`、`thiserror 2.0.21`。没有新的第三方 crate。`Cargo.lock` 只给 `af-stop` 挂上这四条已有依赖。

提交：

- `bf9cde1` af-stop: 指纹账本落 SQLite，对外 JSON 版本固定为 v=1
- `81d4cd5` af-stop: 为重启后指纹计数不回退补一条 proptest

## ③ 证据

命令都在 `source /data/af-factory/env.sh` 之后执行。

```
$ cargo check -p af-stop -j 4
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.70s
```

```
$ cargo clippy --all-targets -p af-stop -j 4 -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.24s
```

退出码 0，无警告。

```
$ cargo test -p af-stop -j 4

running 29 tests
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 7 tests
test fingerprint_count_does_not_regress_across_restart ... ok
test stop_counter_never_regresses ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 12 tests
test declaration_mismatch_is_fail_closed_and_preserves_rows ... ok
test fingerprint_row_roundtrips_across_restart ... ok
test engine_counters_roundtrip_across_restart ... ok
test fingerprint_stop_still_beats_threshold_after_restart ... ok
test window_roll_keeps_count_and_survives_restart ... ok
test wall_clock_does_not_drive_expiry_or_drop_the_count ... ok
test advance_while_closed_does_not_expire_the_window_or_drop_the_count ... ok
test whitelisted_fingerprint_stays_continue_after_restart ... ok
test ledger_feed_matches_the_pure_engine_across_restart ... ok
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

Doc-tests af_stop
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

数字：开始前 18（单元 11 + 属性 6 + doctest 1）。现在 49（单元 29 + 重启集成 12 + 属性 7 + doctest 1）。多出来的 31 个都是本任务新写的。原有 11 个规则单测和 6 个属性测试仍在，doctest 仍是 1。

## ④ 反馈

1. 对外版本放在信封 `{"v":1,"body":...}` 上，声明、指纹行和引擎快照则把 `v` 放在文档自身。读路径是 `decode_wire` / `decode_doc` / `from_json`。领域枚举单独 `serde_json::from_str` 没有版本字段，那不是账本的读入口。
2. 时间窗是逻辑毫秒上的半开区间 `[start, end)`。到期只换窗口，累计计数继续 +1。若把「窗内次数」做成到期归零，会和「计数不回退」冲突，所以没有这么做。三条规则仍只看严格连续和普通错误总量。
3. `user_version = 0` 且表已存在、或 `user_version = 1` 但表缺了，都当损坏拒绝。`CREATE TABLE IF NOT EXISTS` 会把这种半截库补成首启，这里不用。
4. 关闭期间不把注入钟的绝对读数加进逻辑毫秒。`Instant` 不能跨进程比；墙钟回拨也不能把窗口重新打开。测试要看到过期，得在账本打开时推进 `ManualClock`。
5. 指纹库登记顺序不参与相等判断。重入用另一份插入顺序、同一组覆盖项，得到 `AlreadyDeclared`。

## ⑤ 遗留

- `RESULT.md` 里的停机不锁存、错误码精确匹配，这次都没动。最终方案若要锁存，要同时改引擎和现有测试。
- 账本是一条连接的持有者。配方里的 `busy_timeout` 留着，但没有做多连接并发写测试。
- 重启属性测试是 64 个用例，不是原来那组的 256。失败不写 `proptest-regressions/`。
- 直接反序列化不带信封的 `Event` / `Decision` 不会检查版本。跨边界要走 `encode_wire` / `decode_wire`。
