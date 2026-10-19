# RESULT-UPDATE — W2E：af-trace critical 自有落盘轨

## ① 结论

**通过**。critical 从内存 Vec 换成调用方目录里的 JSONL：落盘线程把整行写入 `critical.jsonl` 并 `sync_data`，成功后才推进镜像。写盘失败走 fail-closed（`PersistFailed` 退还事件，后续 `write_critical` 不再入队）。normal 仍是异步追加 `normal.jsonl`，队列满或写盘失败都记缺口。有界 `try_send`、事件原样退还、gap span 合并都保持 RESULT 已验证的语义。

`trace.db` 用工作区已有的 rusqlite 0.32.1（bundled）：WAL、`busy_timeout=5000`、`synchronous=NORMAL`、`foreign_keys=ON`。`open` 返回 `Created` 或 `AlreadyDeclared`。schema 版本、行格式、critical 同步耐久性对不上，或库内缺口计数与 critical 文件的 `Σ(gap.count)` 不一致，启动失败。干净关闭后重开，critical 文件、缺口计数和 seq 原样可读。

`cargo test -p af-trace` 全绿：单元 13（原 4 + 新 9）+ 洪峰 1。洪峰仍是 10 万 normal 事件，并满足「落盘文件行数 = 事件数 + 缺口数」。`cargo clippy --all-targets -p af-trace -- -D warnings` 无警告。`cargo bench -p af-trace` 退出码 0，三条基准都跑完。

## ② 步骤

- 先读 `RESULT.md`、`RESULT-UPDATE.md`（上一轮基准）和 `benches/trace_paths.rs`。未改有界缓冲、合并缺口、`shutdown` 顺序。
- `src/clock.rs`：`Clock` / `SystemClock` / `ManualClock`。事件 `ts_ms` 只读注入钟。新测试用手动钟，不 `sleep`。
- `src/store.rs`：`dir/trace.db`。首启 `Created`（建 `declaration` + `gap_state`），重入 `AlreadyDeclared` 并核对 `schema_version=1`、`line_format=jsonl`、`critical_durability=sync`。不符即 `DeclarationMismatch`，不改已有行。
- `src/trace.rs`：`TraceConfig.dir` 必填。`spawn_consumer` 在写文件前拿镜像锁（`hold_consumers` 仍能把队列钉满）。critical 追加并 `sync_data` 后才 `push` 镜像；失败把事件放进 `unpersisted`，`shutdown` 返回 `PersistFailed`。normal 追加但不逐条 fsync，失败调用既有 `record_gap`。`shutdown` 成功时把 `gap_counter` / `gap_reported` 写成与 critical 文件一致的值。
- 重开从文件恢复 seq（最大 seq + 1）和缺口基数。只有 jsonl、没有 `trace.db` 时拒绝当成首启。坏 JSONL 行拒绝打开。
- `tests/flood.rs`：保留 10 万洪峰。补文件行数恒等，并 `drop` 后重开核对 `AlreadyDeclared` 与缺口基数。
- 基准每个迭代使用独立临时目录，计时窗口仍不含 `spawn` / `shutdown`。
- `Cargo.toml` 增加 `rusqlite = { workspace = true }`。`Cargo.lock` 只给 af-trace 加上已有的 `rusqlite` 边，没有新包。根 `Cargo.toml` 与其他 crate 未改。

## ③ 证据

命令都在 `source /data/af-factory/env.sh` 之后执行。

`cargo check -p af-trace --all-targets`：`Finished dev profile ... target(s)`，退出码 0。

`cargo fmt -p af-trace` 后 `cargo clippy --all-targets -p af-trace -- -D warnings`：`Finished dev profile ... in 0.82s`，无警告。

`cargo test -p af-trace -- --nocapture`：

```
running 13 tests
test clock::tests::manual_clock_set_and_advance_are_shared_by_clones ... ok
test trace::tests::append_line_on_dev_full_returns_error ... ok
test trace::tests::orphan_jsonl_and_bad_dir_fail_closed ... ok
test store::tests::pragmas_created_then_already_declared_keeps_gap ... ok
test trace::tests::critical_persist_failure_is_fail_closed ... ok
test trace::tests::restart_restores_critical_log_gap_counter_seq_and_clock ... ok
test trace::tests::declaration_gap_and_corrupt_log_fail_closed ... ok
test store::tests::declaration_mismatch_fails_closed_without_rewriting ... ok
test trace::tests::zero_loss_when_under_capacity_and_closed_channel_errors ... ok
test trace::tests::normal_persist_failure_counts_gap ... ok
test trace::tests::normal_overflow_drops_and_counts_gap ... ok
test trace::tests::gap_spans_coalesce_when_critical_is_also_full ... ok
test trace::tests::critical_full_returns_err_and_event_is_preserved ... ok
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.01s

running 1 test
==== 洪峰结果 ====
normal 发送: 100000, 落盘: 968, 丢弃: 99032 (99.03%)
gap_counter == gap_reported == 99032（gap span 条数: 11291）
critical 落盘: 11391 行（检查点 100 + gap span 11291），重试次数: 113（被拒 seq 数: 113）
落盘文件行数 12359 = 事件数 1068 + 缺口数 11291
test flood_100k_normal_events ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.08s

Doc-tests af_trace
test result: ok. 0 passed; 0 failed
```

原测试 5 个（`trace.rs` 4 + `flood.rs` 1）都还在。新增 9 个单元测试，所以是 13 + 1。洪峰里 1068 = 检查点 100 + normal 落盘 968，11291 是 gap span 行数；12359 = 1068 + 11291。丢弃条数仍由 `Σ(gap.count) == 99032` 对账，不把合并后的行数当成丢弃条数。

`cargo bench -p af-trace`：`Finished bench profile [optimized] in 1m 28s`，退出码 0。libtest 把 13 个单元测试标为 ignored（bench 剖面不跑 `#[test]`）。

```
steady_enqueue/write_normal_under_capacity
                        time:   [634.83 µs 704.37 µs 765.35 µs]
                        thrpt:  [1.3380 Melem/s 1.4538 Melem/s 1.6130 Melem/s]
（Collecting 10 samples in estimated 3.0212 s (715 iterations)；每轮 gap_counter == 0）

flood_backpressure/write_normal_100k_full_queue
                        time:   [45.089 ms 45.923 ms 46.766 ms]
                        thrpt:  [2.1383 Melem/s 2.1775 Melem/s 2.2178 Melem/s]
（Collecting 10 samples in estimated 5.1930 s (55 iterations)；每轮 shutdown 后 reported == counter）
Found 1 outliers among 10 measurements (10.00%)
  1 (10.00%) high severe

gap_count_scan/sum_gap_span_counts
                        time:   [351.95 µs 352.99 µs 354.28 µs]
                        thrpt:  [282.26 Melem/s 283.29 Melem/s 284.13 Melem/s]
```

环境没有 gnuplot，criterion 改用 plotters，基准不算失败。

## ④ 反馈

- `Ok(seq)` 仍表示事件已进入有界缓冲，不是调用线程已经 fsync。确认发生在落盘线程：`write` + `sync_data` 成功后才进入镜像；`shutdown` 返回 `Ok` 时，该 seq 已在 `critical.jsonl`。若让 `write_critical` 自己等 fsync，缓冲会边等边被排空，RESULT 里「容量满立即 `CriticalFull`」的测试不再成立。写盘失败时事件在 `PersistFailed` 里，不在文件里。
- 内存 `Vec` 还在，只做本次进程的镜像和 `hold_consumers` 的锁。持久事实是目录里的 JSONL 与 `trace.db`。重开后的历史用 `read_trace_log` 读文件，不在新镜像里。
- 洪峰恒等式里的「缺口数」是 gap span 的行数。已验证的合并会把多次丢弃合成一行，丢弃条数仍看 `count` 之和。本轮洪峰是 11291 行 span、99032 次丢弃。
- 慢消费者测试里原有的 `sleep` 留着，那是 RESULT 用来打满队列的手段。本轮新测试的时刻用 `ManualClock`，背压用 `hold_consumers`，不用 `sleep` 推断时间。
- 没有新增第三方 crate。rusqlite 0.32.1 是工作区依赖，af-station / af-effect-gateway 已在用。

## ⑤ 遗留

- 进程在 critical 行已经 `sync_data`、`shutdown` 还没把缺口计数写进 `trace.db` 时被杀死，重开会因计数与文件不一致而失败，不会自动选边修复。未刷出的 pending gap 仍随进程消失，与 RESULT ⑤ 相同。
- 没有日志轮转，也没有多进程同时写同一目录。
- critical 是每条 `sync_data`。洪峰计时窗口外的 shutdown 会把已入队的 gap span 逐条同步。
- 基准的丢弃路径从约 26.6 ms / 10 万次变成约 45.9 ms。计时窗口仍是 `hold_consumers` 钉满队列后的 `write_normal`，文件同步在窗口外。变慢来自每条缺口都要经注入钟构造 span，不是另一套丢弃语义。`gap_count_scan` 仍是基准文件里的本地求和，约 353 µs，不读盘。
