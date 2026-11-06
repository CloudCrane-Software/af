# PoC-temporal：Temporal 语义验证（durable 执行引擎）

- id: `poc-temporal`，目录：`af/poc/poc-temporal/`
- 环境：Temporal CLI 1.9.1（dev server, Server 1.32.0, 无 Docker，文件库 `/tmp/temporal-dev.db`）
- SDK：Python temporalio 1.34.0（完整场景）+ Rust temporalio-sdk 1.1.0（smoke 全往返）

## ① 结论：**通过**

全部验收点在本机真实执行通过：

| 验收点 | 结果 |
|---|---|
| dev server 文件库启动/重启恢复 | ✅ |
| signal **和** Update 两种人工审批等待 | ✅（order-1 走 signal，order-crash 走 update） |
| activity 外部预算账本：预占 + 幂等扣减 | ✅（activity 执行 2 次，扣款恰好 1 次） |
| child workflow fan-out（5 个子任务） | ✅（父 workflow 等 5 个 child 全部 Completed） |
| 计数器到阈值 continue-as-new | ✅（counter=8≥6 触发，同 workflow id 换 run_id） |
| CAN 后原 workflow id 查询仍工作 + 状态续接 | ✅（见证据 4） |
| kill -9 server → 重启 → workflow 自动续跑完成 | ✅（同 run_id 存活，Update 审批后续跑至完成） |
| Rust SDK 可用性 | ✅（编译 1m42s + worker/starter 全往返 result=84） |

## ② 做了什么

1. `bin/temporal server start-dev --port 7233 --db-filename /tmp/temporal-dev.db` 后台起服务。
2. **Rust SDK smoke**（`rust-sdk-smoke/`，独立 cargo 项目、空 `[workspace]` 表避免并入根 workspace）：crates.io 实际包名是 `temporalio-sdk` 1.1.0（`temporalio` 不存在，404）。写 worker+starter 两个 bin（官方示例的推荐形态，worker future 非 Send 不能 spawn），工作流=activity×2+持久 timer，对 dev server 全往返。
3. **Python 完整订单工作流**（`poc_workflow.py` + `worker.py` + `client.py` + `ledger.py`）：
   - `ledger.py`：模拟外部预算账本，JSON 文件持久化（独立于 Temporal/worker），`reserve` 按 order_id 幂等；`commit` 首次调用**扣款成功后抛 RuntimeError**（模拟响应丢失），逼 Temporal 重试，第二次调用去重返回同一回执。
   - `OrderWorkflow`：`OrderState` dataclass 既是 input 又是 CAN 携带载体。审批门开在携带状态 `approved_by` 上（signal/update 都能开）；预占→fan-out 5 个 `FulfillItemWorkflow`（child id 确定性 `order_id-fulfill-{i}`）→计数器 `len(ops)>=6` 时 `continue_as_new(s)`→settle 门→幂等扣减→完成。
4. **崩溃恢复**：`order-crash` 启动并停在审批等待 → `kill -9` server → 同 db 重启 → worker 自动重连 → Update 审批 → 全流程跑完。

## ③ 证据（关键输出摘录）

**1) 幂等扣减（activity 执行 2 次、扣款 1 次）** — `/tmp/poc-budget-ledger.json` 最终状态：
```json
{"balance": 9580.0,
 "reservations": {"order-1":  {"commit_attempts": 2, "status": "committed"},
                  "order-crash":{"commit_attempts": 2, "status": "committed"}},
 "ops_log": ["commit:deduct order=order-1 amount=300.0 balance=9700.0",
             "commit:dedup order=order-1 attempts=2"]}
```
10000−300−120=9580，两单各扣一次。order-1 结果回执 `rcpt-2f3d27ccf564` 与账本一致。

**2) CAN + 原 id 查询**（approve signal 后 ~4s 查询 `order-1`）：
```
run_id: 01a1504f...(start) → status.workflow_run_id: 579b28c5-...(CAN后)
ops: [started, approved_by=signal:alice, reserved, child0..child4,
      continue_as_new(counter=8)]  runs_done=1  ← 审批/预占/5个child结果全部随身携带
```

**3) 旧 run 历史完整可查**：`temporal workflow show --workflow-id order-1 --run-id 01a15050...`
```
事件 1..58：WorkflowExecutionStarted, Signaled, ActivityTask*,
  StartChildWorkflowExecutionInitiated ×5, ChildWorkflowExecutionCompleted ×5,
  ... 事件54 WorkflowExecutionContinuedAsNew
```
新 run `HistoryLength=4`（历史重新开始）——**CAN 的"续接"是：workflow id 不变 + 状态携带 + 旧历史按 (id, run_id) 可追溯，而非同一份历史文件**。

**4) 崩溃恢复**：
```
kill -9 server → ps: 0 个 temporal 进程；/tmp/temporal-dev.db 仍在
重启（同 db 文件）→ temporal workflow describe --workflow-id order-crash
  RunId 01a1505d-3dc9-7a2b-a296-67535ddc1ecd   ← 与杀前完全相同
client.py approve-update → {"update_result": "approved-by-update:bob"}
最终 result: balance_after=9580.0, ops 含 settled/committed, runs_done=1
```
server 宕机期间 worker（Python 与 Rust 各一）持续重试 poll（`logs/worker.log`：`gRPC call poll_activity_task_queue retried 6 times`），重启后自动恢复消费。

**5) workflow list 终态**：order-1 / order-crash 各显示 `ContinuedAsNew`(旧 run) + `Completed`(新 run)，每单 5 个 `-fulfill-{0..4}` child 全部 Completed；`rust-smoke-wf-1` Completed。

**6) Rust 全往返**：
```
[worker] connected / up on rust-smoke-queue
[starter] started, run_id=Some("01a1505d-03b0-...")
[starter] workflow result = 84 (expect 84)
```
编译：纯编译 1m42s（`cargo build -j 4`；含两次失败的试探总耗时 ~2m43s，远低于 8 分钟预算）。

## ④ 对最终方案的反馈

**SDK 成熟度**
- Python SDK 成熟顺手：装饰器齐全（signal/update/query/wait_condition/CAN），update 支持同步返回值，适合做引擎/平台层与快速验证。
- Rust SDK 1.1.0（crates.io 包名 `temporalio-sdk`，注意 `temporalio` 是空名 404）：宏 API（`#[workflow]`/`#[workflow_methods]`/`#[activities]`）干净，全往返真实跑通。坑：**构建硬依赖系统 `protoc`**（prost-wkt-types、temporalio-protos 都要），缺它两次 build 失败；宏展开直接引用 `futures`/`temporalio_common`/`temporalio_workflow`，使用方必须显式声明这仨直接依赖；worker future 非 Send，官方形态就是 worker/starter 分进程。
- 若底座选 Rust 做 worker 宿主可行，但迭代成本明显高于 Python；建议底座核心用 Rust SDK 前先做同规模的 signal/update/CAN 验证（本次只做了基础往返）。

**CAN 在"订单身份不变"下的表现**：语义正好满足订单场景——workflow_id 即订单号，CAN 后客户端用同一 id 查询/发信号全部路由到最新 run；已完成阶段靠携带状态跳过（审批不重审、预占不重占）；旧 run 事件历史可审计。**最大的坑：CAN 参数类型必须与 run 方法签名一致**——我曾传内部 `OrderState` 给 `run(OrderInput)`，解码按目标 dataclass 字段裁剪，**静默丢失**审批/预占等字段，新 run 从头再来。另外跨 run 的门必须开在携带状态上，开在 run 内成员变量上等于每次 CAN 重新关锁（第 5 个 bug）。

**Replay 纪律是真实成本（对 DBOS 对比最关键的证据）**：本次踩的 5 个坑里 4 个是"代码 vs 事件历史"问题——① activity 返回类型注解 `Dict[str, object]` 无法反序列化（`Any` 才行）；② schedule 时**类型提示会进历史**，换代码后 replay 按历史里的旧提示解码，在飞 run 直接中毒（WorkflowTask failed 反复重试），热修代码救不了，只能 terminate 重来；③ 改了控制流后 replay 命令不匹配 = 非确定性失败。Temporal 的确定性 = 换来了完整的可追溯/可恢复，但要求"工作流代码是受版本管的资产"（生产要 `patched()`/versioning）。相比之下 DBOS 把状态放 Postgres、函数就是普通函数，没有 replay 纪律，但也没有等价的事件级审计与多语言 worker 形态。**裁决证据：需要长流程编排+人工审批+外部系统幂等+审计，Temporal 语义收益 > 纪律成本；仅是"函数级 crash恢复"则 DBOS 更省心。**

**接口建议**：审批门一律"signal 进、update 出回执"二选一需明确——update 有返回值适合同步 UI，signal 更适合异步事件源；订单状态机所有跨 run 字段收敛到一个显式 State dataclass 并与 run 签名共用同一类型；外部账本类 activity 必须带业务幂等键（order_id），Temporal 的 at-least-once + 业务幂等 = exactly-once 效果，这一点被账本 attempts=2/扣款=1 实证。

## ⑤ 遗留问题

1. Rust SDK 只验证了 workflow/activity/timer 往返，signal/update/query/CAN 未在 Rust 侧验证。
2. 崩溃恢复只杀了 server；worker 进程本身被杀后的任务重投递未测（dev server 单机，生产 SQL 持久化未测）。
3. 工作流代码版本化（`workflow.patched()` / Worker Versioning）未实测——本次靠 terminate 重来，生产需要正式的版本策略。
4. Python 侧 `get_current_history_length` 在 1.34.0 不是模块级 API（按 run 历史长度触发 CAN 的做法未验证），本 PoC 用业务计数器触发。
5. CAN 阈值语义（业务操作计数）是自定义的；"按历史字节数/事件数滚动"需用 CLI 侧统计或后续验证 API。

— 产物：`poc_workflow.py`、`worker.py`、`client.py`、`ledger.py`、`rust-sdk-smoke/`（`src/lib.rs`、`src/bin/smoke_worker.rs`、`src/bin/smoke_starter.rs`）、`logs/`。
