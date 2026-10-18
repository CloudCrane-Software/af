# RESULT — poc-effect-gateway 副作用网关与幂等

## ① 结论

**通过**。四条验收用例全部真实执行并通过，另加两条补充用例（窗口门控、外呼失败重试），6/6 绿，连续 10 轮重跑无抖动。

## ② 做了什么

`crates/af-effect-gateway`（约 400 行 + 集成测试 340 行），rusqlite(bundled) 持久化，零外部服务：

- **意图表 `effect_intent`**：主键 `(workorder_id, step_path, seq)` 即幂等键；存载荷原文、`payload_sha256`（sha256(规范 JSON)，serde_json Map 默认按键排序故序列化确定）、状态机 `declared→executed / withdrawn`、撤回窗口 deadline、回执。另有 `gateway_meta` 存持久化纪元。
- **四步流程**：`declare_intent`（同键同载荷幂等 `AlreadyDeclared`，异载荷报 `PayloadConflict` 并点名"需人工升级"，不覆盖存量指纹）→ 可配置撤回窗口（测试用 20–300ms）→ `execute`（判定序：withdrawn 拒发 / 已 executed 直接**重放已存回执不调闭包** / 窗口未关报 `WindowStillOpen` 等待 / 否则调外部闭包并立即落回执）→ `record_receipt`（外部调用发生在闸门外的分步流程用，幂等保留原回执）。
- **Fencing 纪元**：`resume()` 一条 `UPDATE…RETURNING` 原子地纪元 +1 并持久化；调用方经 `attach()` 拿 `Session{epoch}`，四个操作全部先过 `fence`：请求纪元 ≠ 当前纪元 → `StaleEpoch` 拒绝。**旧纪元声明的意图本身仍有效**，新 session 可继续操作（崩溃恢复重放正是走这条路）。
- **测试**（`tests/gateway_tests.rs`，真实跑 DB、真实 sleep 过窗口、用 `Arc<AtomicUsize>` 计数器证明"外呼恰好一次"）：
  1. 崩溃恢复重放：declare→execute（外呼 1 次）→drop 模拟崩溃→重开库→`resume` 纪元 0→1→同键同载荷重放→返回 `txn T-1`（崩溃前结果），计数器仍为 1；
  2. 同键异载荷：`PayloadConflict`，错误信息含"升级"，存量指纹未被覆盖；
  3. 窗口内撤回：`Withdrawn` 后 execute 被拒、计数器 0；对照组窗口关闭后撤回报 `WithdrawWindowClosed`；
  4. 旧纪元：declare/withdraw/execute/record_receipt 四种旧 session 请求全拒，resume 两次后上一 session 也失效；新 session 对旧意图操作正常。

## ③ 证据

命令：`cd af && source ~/.cargo/env && cargo test -p af-effect-gateway -j 4`

```
running 6 tests
test t2_same_key_different_payload_rejected_requires_escalation ... ok
test t6_external_failure_keeps_declared_and_retry_succeeds ... ok
test t4_stale_epoch_requests_rejected ... ok
test t1_crash_recovery_same_key_same_payload_replays_stored_result ... ok
test t5_execute_before_window_close_is_refused_then_allowed ... ok
test t3_withdraw_within_window_means_never_sent ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; finished in 0.33s
```

稳定性：同一命令连跑 10 轮，10/10 全绿（含真实 sleep 的时序断言无抖动）。`cargo build -p af-effect-gateway -j 4` 0 warning。

## ④ 对最终方案的反馈

1. **接口**：fencing 用"网关签发 `Session{epoch}`、请求必须携带"比让调用方自己管理纪元数字更防呆；建议正式版保留。回执放 `execute` 内部落库是安全默认（外呼与落回执之间的崩溃窗口最小化），分步 `record_receipt` 留给外呼在别处发生的场景。
2. **坑 1——撤回窗口是"发送闸"不是"宽限期"**：`execute` 在窗口未关时必须**拒绝而非放行**，否则"窗口内撤回"与"窗口关闭即执行"会竞争；本 PoC 用 `WindowStillOpen` 错误把等待责任交还调用方。正式版建议网关内建等待（带 deadline 的 sleep）以免每个调用方重写。
3. **坑 2——SQLite RETURNING 的类型**：`UPDATE…RETURNING value` 返回的是存储类型（TEXT），rusqlite 按 i64 取会 `InvalidColumnType`；纪元这类计数器如果存 TEXT 列，读取后须 parse。
4. **坑 3——载荷指纹的确定性**：依赖 serde_json Map 默认按 key 排序（未开 `preserve_order`）。正式版应把这个前提写进文档或加 CI 断言，否则有人改 feature 后同载荷会得出不同指纹、误报冲突。
5. **建议**：生产多进程/多线程需每 session 一条连接（或 `Mutex<Gateway>`）；`resume()` 跨进程并发靠单条原子 UPDATE 保护，若两进程同时 resume 会拿到连续纪元，属可接受但要知道。外部调用失败时意图保持 declared 可重试——这依赖外部操作在**上游幂等键**下可安全重试，网关防的是"已成功的重复发送"，不是"未成功的重试"。

## ⑤ 遗留问题

- 未做并发压测（同键并发 declare/execute 的 SQLite 锁竞争路径未验证；PoC 按单线程语义实现，`Connection` 非 Sync）。
- 时间用 `SystemTime`（可回拨）；正式版应换单调钟 + 持久化 deadline 比较。
- 整 workspace `cargo build` 当前因 **af-graph**（`crates/af-graph/src/validate.rs:358` 缺生命周期，E0106，他人 crate，未触碰）失败；本 crate 单独 build/test 均通过。
- 回执无签名/加密；若外部回执要作为审计凭据需另加。
- 依赖新增 `sha2 = "0.10"`（纪律清单外的最小依赖，用于载荷指纹；如需收敛可改 BLAKE3 或自实现，未做）。
