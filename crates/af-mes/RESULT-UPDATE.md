# RESULT-UPDATE — W5E：af-mes ClickHouse 建库与写入器

## ① 结论

**通过（crate 内测试）。** 新建 `crates/af-mes`。事件表 DDL 是 MergeTree，`PARTITION BY toYYYYMM(occurred_at)`，`ORDER BY (work_order_id, occurred_at)`。产线/小时聚合是物化视图 `mes_line_hour_mv`，目标表 `mes_line_hour` 用 SummingMergeTree。

批写入默认 5 万行或 `max_delay`，先到先发。时刻来自注入的 `Clock`，crate 内不睡眠。端点由 `WriterConfig` 注入。发送用官方 `clickhouse` 0.15.3：显式 `Compression::Lz4`，`with_validation(false)`，所以插入是 HTTP + `FORMAT RowBinary` + `decompress=1`。不启用 `test-util`（该 feature 会把默认压缩改成 None），也不用官方 `Inserter`（它用真实时间切批）。

写入失败的整批 `fsync` 后改名进 outbox 文件，再按序号重放。outbox 达到上限时 `push` 返回 `MesError::Backpressure`，这一行不接收。outbox 目录本身写失败时返回 `MesError::OutboxPersist`，批留在内存。

读 `mes_event` 只生成 `SELECT … FROM mes_event FINAL`。`guard_read_sql` / `select_final` 拒绝 `OPTIMIZE` 和 `rand64`。DDL 常量里也没有这两处。

真实 ClickHouse **没有**跑。`cargo test` 只用本 crate 的 mock HTTP。这是任务要求，不是漏测后的补记。

## ② 步骤

- 本目录是新 crate，没有既有 `RESULT.md`。裁决按任务卡：MergeTree + 物化视图；批 ≥5 万或延迟；`clickhouse` 0.15.3 = HTTP + RowBinary + lz4；`OPTIMIZE FINAL` 不跨分区；`rand64` 非纯函数。
- `schema.rs`：三句 DDL 常量（事件表、聚合表、物化视图）。排序键只有工单 + 时刻，`event_id` 不进排序键。
- `final_query.rs`：`select_final` 把 `FINAL` 贴在表名后。读路径不发 `OPTIMIZE`。
- `writer.rs`：`push` / `poll` 用注入时钟比较 `max_delay`。到行数或到延迟就 `Client::insert`。内存上限不得小于 `max_rows`，避免批在阈值前被截断。
- `outbox.rs`：一批一个文件。`*.partial` → `sync_all` → 改名 `*.batch` → 目录 `fsync`。重开后还能读。未完成的 `*.partial` 在 open 时丢掉（那一批当时还没耐久）。
- 更早的 outbox 文件没送成功时，新批只追加在后面，不插队。
- `reader.rs`：`SELECT ?fields FROM mes_event FINAL`，过滤值走客户端绑定。
- `mock.rs`：`127.0.0.1` 上的 HTTP/1.1。插入解 lz4 帧（magic `0x82`）再解 RowBinary。checksum 是 CityHash，mock 不重算，只对 magic、长度和行内容。
- 依赖：`clickhouse =0.15.3`（任务卡指定，`default-features = false`，`features = ["lz4"]`）。`tokio =1.53.3`（锁里已有，`rt-multi-thread`/`net`/`time`，同步 API 内部 `block_on`）。`serde` / `serde_json` / `thiserror` 走 workspace。根 `Cargo.toml` 未改。`Cargo.lock` 只追加了 clickhouse 这一支。

## ③ 证据

命令都在 `source /data/af-factory/env.sh` 之后，工作目录 `/data/af-factory/w5e`，`-j 4`。

`cargo fmt -p af-mes` 无输出。

`cargo clippy --all-targets -p af-mes -- -D warnings`：

```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.24s
```

退出码 0。

`cargo test -p af-mes -j 4`：

```
running 12 tests
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.29s

Doc-tests af_mes
test result: ok. 0 passed; 0 failed
```

新 crate，基线 0。现在 lib 12 + `tests/http.rs` 10 + doctest 0，共 22。HTTP 里包含：3 行与 5 万行各一批且 raw RowBinary 与手写 LEB128 布局一致（5 万行核对首尾行、帧 magic、`decompress=1`、`FORMAT RowBinary` 且不是 `RowBinaryWithNamesAndTypes`）；注入时钟在 499ms 不发、500ms 发；失败批落盘后重开再重试；新批排在失败文件后面；outbox 满背压且被拒行不在文件里；outbox 目录被换成普通文件时批留在内存；DDL 原文送出；`SELECT … FINAL` 把插入时的 RowBinary 原样读回（含引号转义）。

真实 ClickHouse 集成：**未执行**。本环境 `cargo test` 没有 ClickHouse 进程。

## ④ 反馈

- 延迟阈值必须由调用方 `poll`（或下一次 `push`）触发。写入器不起线程、不 `sleep`，否则注入时钟推不动。
- 官方 `Inserter::with_period` 用 quanta 读真实时间，和「时刻注入、禁止 sleep」冲突，所以批缓冲是自己写的。
- `with_validation(false)` 是为了钉 RowBinary。代价是客户端不再 `DESCRIBE` 对列。列顺序以 `MesEvent` 和 DDL 为准，改一边必须改另一边。
- 事件表按裁决是 MergeTree，没有版本列。`SELECT FINAL` 对普通 MergeTree 不去重。包装器是为了读 ReplacingMergeTree 的跨分区残留，避免有人改回 `OPTIMIZE TABLE … FINAL`。
- 聚合目标用 SummingMergeTree，这样同一产线同一小时的 `count`/`sum` 在合并时相加。源表仍是 MergeTree。
- 投递是至少一次。ClickHouse 已提交但删 outbox 文件之前崩溃，重放会在 MergeTree 里多一行。`FINAL` 消不掉这种重复。

## ⑤ 遗留

- 没有对真实 ClickHouse 执行建表、插入、物化视图和 `SELECT FINAL`。mock 只覆盖客户端发出的 HTTP 与 RowBinary，不覆盖服务端分区合并。
- mock 不校验 lz4 帧的 CityHash checksum。坏 checksum 在真实服务端会被拒绝，在 mock 里只要 magic 和长度对就能解开。
- 至少一次，不是正好一次。要按 `event_id` 去重需要另一张 ReplacingMergeTree；那会改掉「事件表是 MergeTree」的裁决，这里没做。
- `Drop` 不刷内存批。调用方要 `flush`，否则进程退出时只留在内存里的行会丢。已经改名进 outbox 的不会丢。
- 一个 outbox 目录假定一个写入器。没有文件锁。
- 只接受 `http://` 端点。没有开 TLS feature，`https://` 在 `open` 时直接拒绝。
