# RESULT-UPDATE — W4E：af-trace 双轨，OTel 非关键 lane

## ① 结论

**通过**。非关键 lane 已接上：feature `otel` 默认关闭。normal 事件写入 `normal.jsonl` 成功之后，才按 `TraceConfig` 里的端点与批大小导出 OTLP span。导出失败或该 lane 队列满只走既有 `record_gap`（`gap_counter += 1` 并尝试发 gap span），不阻塞 `write_normal`。critical 事件（含 gap span）不进 OTel。未开 feature、端点空白、exporter 建不起来时 `Trace::open` 仍成功，lane 离线，只把落盘成功的 normal 计入 `otel_counted`。

critical 落盘轨语义未改：有界 `try_send`、fail-closed、`critical.jsonl` + `sync_data`、`PersistFailed` 退还事件、时刻注入。离线时（默认，以及全部既有测试）`gap_counter` 仍等于磁盘丢弃数。在线时 `gap_counter` = 磁盘丢弃 + OTel 丢弃，`Σ(gap.count) == gap_counter` 仍成立。

`cargo test -p af-trace --all-features` 全绿。clippy 两条（默认与 `--all-features`）均 `-D warnings` 通过。

## ② 步骤

- 先读 `crates/af-trace/RESULT.md` 与 W2E 落盘改造。critical 写盘、缺口合并、`shutdown` 对账未改语义。
- `Cargo.toml` 增加可选依赖，版本钉死：`opentelemetry =0.33.1`、`opentelemetry-otlp =0.33.1`，以及 `opentelemetry_sdk =0.33.1`（`SpanExporter` / `SpanData` 在该 crate）。feature `otel` 打开 `http-proto` + `reqwest-blocking-client`，不启用 `grpc-tonic`。根 `Cargo.toml` 未改；`Cargo.lock` 锁住这批传递依赖。
- `src/otel.rs`：自管队列与批导出，直接调 `SpanExporter::export`，用线程 park 把 future 跑完。导出 `Err` 对批次里每个 seq 调 `record_gap`。SDK 的 `BatchSpanProcessor` 会吞掉导出错误，这条 lane 不用它。
- `TraceConfig` 增加 `otel_endpoint`（完整 URL，原样交给 `with_endpoint`）和 `otel_batch_size`（`0` 当作 512；在线时也是该 lane 的 `sync_channel` 容量）。配方层值进 config，feature 关闭时字段仍在，只是不导出。
- 只把落盘成功的 normal 送进 lane。离线只计数。在线 `try_send`，满或断连记缺口。`shutdown` 先等 normal 落盘线程，再关 OTel 发送端并等 worker，然后才 `flush_pending_gap`，最后关 critical。
- 单测用 `MockExporter`（实现 `SpanExporter`）。真实 collector 不进 `cargo test`。
- 映射测试第一次把 4 条 normal 一次性写入容量为 2 的队列，worker 尚未排空时有 2 条被记成缺口（`gap_counter` 左 2 右 0）。改为先握住 mock 的门：第一条进入 `export` 后批次已封口，再等后两条 `try_send` 成功，然后放行。批次稳定为 `[1, 2, 1]`。

## ③ 证据

命令都在 `source /data/af-factory/env.sh` 之后执行。工作目录 `/data/af-factory/w4e`。

`cargo check -p af-trace --all-features --all-targets` 首轮失败：`Status` 不在 `opentelemetry_sdk::trace`（在 `opentelemetry::trace`）；`SpanExporter::export` 未引入 trait。改完后 `cargo check -p af-trace --all-targets` 与 `--all-features --all-targets` 均为 `Finished`，退出码 0。

`cargo fmt -p af-trace` 之后：

`cargo clippy --all-targets -p af-trace -- -D warnings`：`Finished dev profile [unoptimized + debuginfo] target(s) in 0.37s`，退出码 0。

`cargo clippy --all-targets --all-features -p af-trace -- -D warnings`：`Finished dev profile [unoptimized + debuginfo] target(s) in 0.44s`，退出码 0。

`cargo test -p af-trace`：

- lib：`ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.01s`
- `tests/flood.rs`：`ok. 1 passed`（`flood_100k_normal_events`，1.14s）
- doc-tests：`0 passed`

相对 W2E 的 lib 13 + 洪峰 1：默认构建新增 3 个单测（`zero_batch_size_uses_the_bsp_default`、`offline_lane_counts_persisted_normals_and_skips_critical`、`configured_endpoint_without_feature_stays_offline`）。洪峰仍是 1。

`cargo test -p af-trace --all-features`：

- lib：`ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.01s`
- `tests/flood.rs`：`ok. 1 passed`（1.19s）
- doc-tests：`0 passed`

20 = 默认 16 减去仅在 feature 关闭时编译的 `configured_endpoint_without_feature_stays_offline`，再加上 5 个 feature 测试：`normal_events_map_to_spans_and_critical_never_enters`、`export_failure_only_increments_gap_and_keeps_disk`、`wedged_export_does_not_block_writes_and_overflow_counts_gap`、`missing_collector_does_not_fail_startup`、`unbuildable_exporter_degrades_to_pure_count`。

实现提交：`6542b85`。

## ④ 反馈

- `otel_endpoint` 原样交给 `opentelemetry_otlp` 的 `with_endpoint`。程序配置不会自动补 `/v1/traces`。配方应写完整 URL，例如 `http://127.0.0.1:4318/v1/traces`。
- 在线 lane 的队列容量等于 `otel_batch_size`。批大小小于突发条数时，worker 还没排空就会记缺口。这是非关键 lane 的有损语义，不是落盘失败。
- `opentelemetry_sdk` 是任务卡点名的两个 crate 之外的直接依赖，理由与版本写在 `6542b85` 的提交说明里。
- 本机 `cargo-deny` 二进制要求 `GLIBC_2.39`，当前动态链接器加载失败，许可证检查没有跑成。新增 crate 的声明许可证是 Apache-2.0；传递依赖未在本机用 deny 复核。

## ⑤ 遗留

- 真实 collector 的连通与导出不在 `cargo test` 里，留给 orchestration 手册。本次改动范围是 `crates/af-trace/**`，没有新增手册。
- collector 不在、但 exporter 已经建成时，若有 normal 事件进入在线 lane，OTLP 客户端会在 `af-trace-otel` 线程里按自己的重试策略等待。`write_normal` 仍是 `try_send`，调用线程不等待。`missing_collector_does_not_fail_startup` 只断言 `open` 成功且不写事件，不覆盖这条等待。`http://` 在 0.33.1 上建 exporter 失败，所以 `unbuildable_exporter_degrades_to_pure_count` 走离线纯计数，也不会打出重试。
- 钉住的 feature 是 `http-proto` + `reqwest-blocking-client`，没有 rustls / native-tls。https 端点没有进 `cargo test`。
- W2E 已记录的日志轮转与多进程未做。critical 落盘语义保持原样。
