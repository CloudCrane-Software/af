# RESULT — poc-trace：分级背压与缺口计数（crates/af-trace）

## ① 结论

**通过**。双通道语义按规格实现并全部验证：critical 同步写 fail-closed（缓冲满返回 Err、事件退还、零静默丢失）；normal 异步写有损（队列满丢弃并递增 gap_counter）；每次丢弃以 critical gap span 报告（critical 也满时合并不丢计数，shutdown 补发）。10 万事件洪峰 + 慢消费者下：critical 全部落盘零丢失、`gap_counter == gap_reported == 实际丢弃数` 精确一致。测试 5 连跑全绿。

## ② 做了什么

- `src/event.rs`：`TraceEvent`（全局单调 `seq` 跨双通道共用，供精确对账；serde 序列化为 JSON line）、`Level`、`GapInfo`（total_dropped / count / 丢弃 seq 闭区间）。
- `src/error.rs`：`TraceError::CriticalFull(TraceEvent)` / `Shutdown(TraceEvent)`——被拒事件**原样退还**调用方（fail-closed 的载体）。
- `src/trace.rs`：核心 `Trace`。
  - critical：`std::sync::mpsc::sync_channel(critical_capacity)` + 落盘线程（内存 JSON-line sink），`write_critical` 用 `try_send`，满则立即 `Err`；`Ok(seq)` 即保证落盘。
  - normal：`sync_channel(normal_capacity)` + 后台消费线程，`write_normal` `try_send` 满则丢弃并 `gap_counter += 1`。
  - 缺口可验证：丢弃时尝试发 `kind="gap"` 的 critical span；critical 也满时在 `pending_gap` 中**合并**（count 累加），`shutdown()` 强制补发。不变量：`Σ(gap span.count) == gap_counter == 实际丢弃数`。
  - `shutdown()` 顺序：关 normal → 等 normal 消费线程清空 → 补发 pending gap → 关 critical → 等 critical 清空。
- 单元测试 4 个（`src/trace.rs`）：critical 满返回 Err 且事件原样退还+重试成功零丢失；normal 溢出精确丢弃计数；critical 也满时 gap span 合并（count>1）且计数不丢；容量充足时零丢弃零丢失 + shutdown 后写入返回 Err。
- 集成测试 `tests/flood.rs`（PoC 指定检查）：4 线程 × 2.5 万 = **10 万 normal 事件**灌入（队列 512、慢消费者 200µs/条），穿插 100 条 critical 检查点（fail-closed + 退避重试模式），断言三大不变量。

## ③ 证据

命令：`cargo build -j 4 -p af-trace`（无警告）→ `cargo test -j 4 -p af-trace -- --nocapture`，5 连跑全部通过：

```
running 4 tests
test trace::tests::zero_loss_when_under_capacity_and_closed_channel_errors ... ok
test trace::tests::normal_overflow_drops_and_counts_gap ... ok
test trace::tests::gap_spans_coalesce_when_critical_is_also_full ... ok
test trace::tests::critical_full_returns_err_and_event_is_preserved ... ok
test result: ok. 4 passed; 0 failed

==== 洪峰结果 ====
normal 发送: 100000, 落盘: 901, 丢弃: 99099 (99.10%)
gap_counter == gap_reported == 99099（gap span 条数: 17957）
critical 落盘: 18057 行（检查点 100 + gap span 17957），重试次数: 86（被拒 seq 数: 86）
test flood_100k_normal_events ... ok
test result: ok. 1 passed; 0 failed
```

断言覆盖（`tests/flood.rs:87-127`）：①所有 `Ok(seq)` 的 critical 事件都能在 critical sink 中按 seq 找到，且 sink 内无重复——**零丢失**；②`gap_counter == 发送数 − 实际落盘数`，`gap_reported == gap_counter`——**计数一致**；③`Σ(gap span.count) == gap_counter`——**"缺了多少"成为落盘里的可验证事实**。洪峰中 critical 通道被 gap span 挤压出 86 次 fail-closed 拒绝，全部经重试落盘，恰好实战演练了该路径。

## ④ 对最终方案的反馈

- **接口建议**：`write_critical(kind, payload) -> Result<u64, TraceError>` 的形态够用；`Ok(seq)` 让调用方可对账，`Err(TraceError::CriticalFull(原事件))` 让重试零成本。最终方案建议保留"事件退还 + 全局 seq 对账"这两点。
- **坑 1：gap span 会挤占 critical 容量**。洪峰里 99,087 次丢弃的 gap 报告与真实 critical 事件共抢一个缓冲，导致 86 次真实 critical 写被拒。最终方案应给 gap span 预留子容量（如 critical 容量的 10% 专用于 gap），或独立 lane，避免"报告缺口反而挤掉正事"。
- **坑 2：逐条 gap span 放大写入量**。合并机制把 99,087 次丢弃压成约 1.8 万条 span（coalesce 自然生效），但 1.8 万条仍是噪声。最终方案建议按时间窗/计数窗批量报告（如每 100ms 或每 1024 次丢弃一条），span 内带区间即可。
- **坑 3：`shutdown` 的调用契约**。必须所有生产者 join 之后调用，否则 straggler 的丢弃可能晚于补发时机。生产实现建议用世代/epoch 或在 shutdown 时循环检查 `pending` 清空后再关 critical。
- **语义澄清**：critical 的"同步写"指移交同步（try_send 立即成功或 Err），落盘仍由后台线程做——这是既有界又不阻塞的唯一组合，验证侧靠 `Ok(seq) ⇒ 必在 sink` 的不变量闭环。
- 根 `Cargo.toml` 未动；新增依赖只写在本 crate（serde/serde_json/thiserror，Cargo.lock 由 cargo 自动追加传递依赖）。

## ⑤ 遗留问题

- 落盘是内存 JSON-line sink（按任务要求"消费到内存 sink"）；换真实文件只改 `spawn_consumer` 一处（接口不变），未做 fsync/轮转。
- 三个时序敏感单元测试用"消费者先 sleep 再 recv + 大 delay 窗口（200/500ms）"保证确定性，极端调度抖动（>500ms 停顿）理论上仍可能误报，已在共享机器上 5 连跑验证稳定。
- `record_gap` 内 `pending_gap` 与 `critical_tx` 两把锁按固定顺序获取，无死锁路径，但未做 formal 验证；`flush_pending_gap` 的重试上限（200 万次自旋）是防御性的，理论不可达。
- 未做多进程/多写者崩溃恢复（进程死掉时 pending 中未报告的缺口会随进程消失——真实落盘版需把 gap 计数周期性 checkpoint）。
