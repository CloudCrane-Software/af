# RESULT-UPDATE — W5D：af-trace critical 事件锚定到 rekor-tiles

## ① 结论

**通过**。已落盘的 critical 事件按 N 条或注入钟上的 T 毫秒封成批次，计算 RFC 6962 Merkle 根，`POST {endpoint}/api/v2/log/entries`。回执（log index、检查点 HMAC 签名、纳入路径）追加到 `critical.anchors.jsonl`。`verify_anchor` 重算批次根，并按回执把 `SHA-256(0x00 || 根)` 折回 rekor 树根，再核对检查点签名。

rekor 断连、拒连或非 2xx 时，批次写入 `anchor-queue.jsonl` 待重试。`anchor_gap` 是还没锚定成功的 critical 事件数，`anchor_alert` 与 `anchor-status.json` 的 `alert` 是告警字段。这不增加 `gap_counter`，也不让 `write_critical` 等待 HTTP。`Σ(gap span.count) == gap_counter` 保持 W4E 的语义：只计磁盘丢弃和在线 OTel 丢弃。

端点为空时锚关闭，不写侧车。`trace.db` 的 schema 仍是版本 1。没有新的第三方 crate。真实 rekor 不在 `cargo test` 里；测试用本 crate 的 stub HTTP。

`cargo test -p af-trace` 与 `--all-features` 全绿。clippy 两条（默认与 `--all-features`）均 `-D warnings` 通过。

## ② 步骤

- 先读 `crates/af-trace/RESULT.md` 与 W4E 的 `RESULT-UPDATE.md`。critical 有界 `try_send`、fail-closed、`sync_data`、缺口合并、OTel 非关键 lane 未改语义。
- `src/sha256.rs`：FIPS 180-4 SHA-256、RFC 2104 HMAC-SHA256、十六进制与标准 base64。不引入 `sha2`。
- `src/merkle.rs`：RFC 6962。叶子 `SHA-256(0x00 || data)`，内部节点 `SHA-256(0x01 || left || right)`，按严格小于 n 的最大 2 的幂拆分。
- `src/http_util.rs`：只支持 `http://` 的 HTTP/1.1 JSON POST。连接与读写有超时。
- `src/rekor.rs`：`hashedRekordRequestV002.digest` 是批次 Merkle 根的原始 32 字节（标准 base64）。日志叶子是 `SHA-256(0x00 || 根)`。回执签名是检查点信封的 HMAC-SHA256，密钥由 `AnchorSettings::hmac_key` 注入。`verify_anchor(batch, receipt, key, origin)` 做复核。`stub` 只在测试里编译，可按次断连。
- `src/anchor.rs`：落盘线程在 `sync_data` 成功后把事件交给锚线程（无界通道，不在落盘线程里做 HTTP）。N 与 T 同时为 0 时按 1 条封口。只设 T 时不按条数封口。T 只和注入的 `Clock` 比较。观察点是事件进入锚线程、`Trace::poll_anchor`、shutdown。锚线程不睡眠。
- 侧车 `critical.anchors.jsonl`，失败队列 `anchor-queue.jsonl`，状态 `anchor-status.json`（`anchor_gap`、`alert`、`alert_reason`）。重开时先重试队列，再按 critical 日志补上还没有回执的事件。坏的侧车或队列行让 `open` 失败，不跳过。
- `ShutdownReport` 增加 `anchor_gap`、`anchor_alert`、`anchor_alert_reason`。`TraceConfig` 增加 `anchor: AnchorSettings`。`tests/flood.rs` 与基准只补了这个字段的默认值。

## ③ 证据

命令都在 `source /data/af-factory/env.sh` 之后执行。工作目录 `/data/af-factory/w5d`。

`cargo check -p af-trace`：`Finished dev profile [unoptimized + debuginfo] target(s) in 0.54s`，退出码 0。

`cargo check -p af-trace --all-features --all-targets`：`Finished dev profile [unoptimized + debuginfo] target(s) in 0.86s`，退出码 0。

`cargo fmt -p af-trace` 之后：

`cargo clippy --all-targets --all-features -p af-trace -- -D warnings`：`Finished dev profile [unoptimized + debuginfo] target(s) in 0.57s`，退出码 0。

`cargo clippy --all-targets -p af-trace -- -D warnings`：`Finished dev profile [unoptimized + debuginfo] target(s) in 0.43s`，退出码 0。

`cargo test -p af-trace`：

- lib：`ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.02s`
- `tests/flood.rs`：`ok. 1 passed`（`flood_100k_normal_events`，1.10s）
- doc-tests：`0 passed`

相对 W4E 默认构建的 lib 16 + 洪峰 1：本次 lib 增加 21 个测试，洪峰仍是 1。

21 = 13 + 8。13 个在 `0808730`：sha256 3（NIST 空串/`abc`/长向量、RFC 4231 HMAC）、merkle 4、http_util 2、rekor 4（内存日志篡改、第二片叶子的证明、stub 提交与断连、`127.0.0.1:1` 拒连）。8 个在 `46dd364`：`disabled_anchor_does_not_write_sidecar_or_gap`、`batch_n_submits_and_shutdown_flushes_the_remainder`、`batch_window_uses_the_injected_clock`、`unreachable_queues_for_retry_without_blocking_critical`、`wedged_rekor_does_not_block_the_next_critical_write`、`restart_retries_the_persisted_queue`、`refused_endpoint_does_not_fail_open_and_still_persists_critical`、`default_batch_seals_each_event`。

`cargo test -p af-trace --all-features`：

- lib：`ok. 41 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.01s`
- `tests/flood.rs`：`ok. 1 passed`（1.09s）
- doc-tests：`0 passed`

41 = 默认 37 减去只在 feature 关闭时编译的 `configured_endpoint_without_feature_stays_offline`，加上 W4E 已有的 5 个 OTel 测试。锚定测试两种构建都在。

实现提交：`0808730`、`46dd364`。

## ④ 反馈

- `anchor_gap` 故意不并进 `gap_counter`。后者的不变量是丢弃条数。锚定缺口是另一件事，塞进 gap span 会挤占 critical 容量，也和「不阻塞 critical 轨」冲突。
- 检查点签名是 HMAC-SHA256，不是官方 rekor-tiles 的 Ed25519 note。没有引入 `ed25519-dalek`。私有端点要能接受 `hashedRekordRequestV002.digest` = 批次 Merkle 根，并按同一把 HMAC 密钥签 C2SP 形状的检查点。对接官方日志需要适配器。
- `integrated_time_ms` 是提交时的注入钟，不在检查点签名覆盖范围内。
- 时间窗在事件进入锚线程、`Trace::poll_anchor` 和 shutdown 时判定。没有新 critical 事件时，要到点封口就调用 `poll_anchor`。这个调用会等本次检查结束，包含一次 rekor 往返，不要放在 `write_critical` 的调用线程上。
- `AnchorSettings` 的 `batch_n` 与 `batch_t_ms` 都是 0 时按 1 条封口，避免端点已填但两个参数都没写时永远不提交。
- 回执先 `sync_data` 再从队列删掉。若提交已经进日志但响应丢失，重试会再追加一片叶子；验锚用新回执。

## ⑤ 遗留

- 真实 rekor-tiles 的连通、Ed25519 检查点和不经适配器的叶子预映像不在 `cargo test` 里。本次改动范围是 `crates/af-trace/**`，没有改 orchestration 手册。
- 客户端只发 `http://`。没有 rustls / native-tls，https 端点会在 `RekorClient::new` 失败并使 `Trace::open` 返回 `Open`。
- 锚线程不自带定时器。进程停在没有新 critical 事件、也没有人调用 `poll_anchor` 或 `shutdown` 时，未满 N 的批次保持开放。事件已经在 `critical.jsonl` 里，重开后按日志补进开放批次。
- W2E 已记录的日志轮转与多进程未做。critical 落盘语义保持原样。
