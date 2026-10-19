# RESULT-UPDATE — R3：af-trace criterion 基准（波次 1 ⑦/T9-6 部分）

## ① 结论

**通过**。`af-trace` 增加 criterion 0.8.2 基准，覆盖常载入队、10 万事件洪峰背压、缺口计数扫描。`cargo test -p af-trace` 全绿（4 单元 + 1 集成，数量与 `RESULT.md` 一致，未增未减）。`cargo bench -p af-trace` 退出码 0，三条基准都跑完。`cargo clippy --all-targets -p af-trace -- -D warnings` 无新增警告。

任务卡写「现有 14 测试」。本 crate 的 `#[test]` 实际是 5 个（`src/trace.rs` 4 个 + `tests/flood.rs` 1 个），与已验证的 `RESULT.md` 相同。没有为凑数加测试，也没有改这 5 个测试的断言。

## ② 步骤

- `Cargo.toml`：dev-dependency `criterion = "0.8.2"`（引入时 crates.io 上的最新稳定版）。`[[bench]] name = "trace_paths"`，`harness = false`。bench target 默认不带 `test = true`，`cargo test` 不会执行它。
- 工作区 `Cargo.lock`：只追加 criterion 0.8.2 及其传递依赖。根 `Cargo.toml` 未改。其他 crate 未改。
- `Trace::hold_consumers` / `ConsumerHold`：挡住两侧落盘线程，使有界队列保持满。不改变 `write_critical` / `write_normal` 的语义。锁顺序固定为 critical → normal。`shutdown` 前必须 drop，否则补发缺口时 critical 缓冲一直满。
- `benches/trace_paths.rs` 三条基准，计时窗口不含 `spawn` / `shutdown`：
  - `steady_enqueue/write_normal_under_capacity`：容量 8192，每轮 1024 次 `write_normal`。批次小于容量，断言 `gap_counter == 0`（入队路径）。
  - `flood_backpressure/write_normal_100k_full_queue`：与洪峰集成测试相同容量（normal 512 / critical 8192），消费延迟为 0。先 `hold_consumers` 再打满队列，然后计时 10 万次 `write_normal`。断言本轮丢弃数 ≥ 100_000 − 2，且 shutdown 后 `gap_reported == gap_counter`。不用 sleep 制造背压。
  - `gap_count_scan/sum_gap_span_counts`：10 万条注入 `ts_ms` 的事件，按集成测试同一公式对 `gap.count` 求和（不是数 span 条数）。开跑前用夹具期望值自检。
- 采样收成每组 10 个样本，避免默认 100 样本把 10 万事件路径重复过多次。

## ③ 证据

命令均在 `source /data/af-factory/env.sh` 之后执行。

`cargo check -p af-trace --all-targets`：`Finished dev profile ... in 18.65s`（含 bench 目标）。

`cargo fmt -p af-trace` 后 `cargo clippy --all-targets -p af-trace -- -D warnings`：`Finished dev profile ... in 0.16s`，无警告。

`cargo test -p af-trace`（未出现 `Running benches/`）：

```
running 4 tests
test trace::tests::zero_loss_when_under_capacity_and_closed_channel_errors ... ok
test trace::tests::normal_overflow_drops_and_counts_gap ... ok
test trace::tests::gap_spans_coalesce_when_critical_is_also_full ... ok
test trace::tests::critical_full_returns_err_and_event_is_preserved ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.00s

running 1 test
test flood_100k_normal_events ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.50s

Doc-tests af_trace
test result: ok. 0 passed; 0 failed
```

`cargo bench -p af-trace`：`Finished bench profile [optimized] in 16.89s`，退出码 0。libtest 把 4 个单元测试标为 ignored（bench 剖面只跑基准，不跑 `#[test]`）；语义回归以上面的 `cargo test` 为准。

```
Gnuplot not found, using plotters backend
steady_enqueue/write_normal_under_capacity
                        time:   [1.2664 ms 1.3363 ms 1.4473 ms]
                        thrpt:  [707.53 Kelem/s 766.27 Kelem/s 808.58 Kelem/s]
（Collecting 10 samples in estimated 3.0737 s (1705 iterations)；每轮 gap_counter == 0，否则 panic）

flood_backpressure/write_normal_100k_full_queue
                        time:   [26.442 ms 26.593 ms 26.803 ms]
                        thrpt:  [3.7310 Melem/s 3.7604 Melem/s 3.7818 Melem/s]
（Collecting 10 samples in estimated 5.4176 s (165 iterations)；每轮丢弃数 ≥ 99998 且 shutdown 后 reported == counter）

gap_count_scan/sum_gap_span_counts
                        time:   [291.60 µs 293.58 µs 295.56 µs]
                        thrpt:  [338.34 Melem/s 340.62 Melem/s 342.94 Melem/s]
Found 1 outliers among 10 measurements (10.00%)
  1 (10.00%) high mild
（10 万事件求和；Collecting 10 samples in estimated 2.0115 s (6875 iterations)）
```

洪峰单次约 26.6 ms / 10 万次，高于常载的约 1.3 ms / 1024 次（折合约 3.8 Melem/s 对 0.77 Melem/s）。背压路径在 critical 缓冲打满后走 `try_send` 失败并合并 pending，不再每条都成功入队，所以单条更短。这是现有 `record_gap` 的实测，不是另一套语义。

## ④ 反馈

- `Ok(seq)` / 事件原样退还 / `Σ(gap.count) == gap_counter` 都没改。基准只在这些不变量上加了断言，用来保证测到的是入队或丢弃，而不是跑偏的路径。
- 洪峰集成测试仍用 `normal_consume_delay_us = 200` 证明慢消费者下的丢弃比例。基准把延迟设为 0，用 `hold_consumers` 把队列钉满，量的是 CPU 上的丢弃与缺口记账，不是 sleep 被算进时间。
- `hold_consumers` 是为基准暴露的公开守卫。生产调用方若在 `shutdown` 前一直持有它，`flush_pending_gap` 会撞上既有的自旋上限。类型文档已写明必须先 drop。
- 环境没有 gnuplot。criterion 自动改用 plotters，统计结果仍然打印，基准不算失败。

## ⑤ 遗留

- 基准样本数为 10，只说明路径可跑、数量级稳定。不替代 `tests/flood.rs` 的不变量证明。
- `record_gap` 在 critical 已满时仍会先构造再丢掉一条 gap 事件。这是已验证实现，本次不改。
- `flush_pending_gap` 里原有的 `sleep(100µs)` 重试还在，位于 shutdown，不在计时窗口内。
- 未做多进程崩溃恢复，也未把内存 sink 换成真实文件。与 `RESULT.md` ⑤ 相同，本次范围不含。
