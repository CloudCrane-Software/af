# ADR-003: 订单持久化走 af-engine + ledger + MES，Temporal 常驻化暂缓

- 状态：已裁决（2026-10-24，编排者）

## 背景

- af/orchestration（Temporal Python 壳）是波次 3 验收演示件：`forge_artifact` 为 mock、temporal dev server（localhost:7240）非常驻、无看护。
- Temporal **dev server** 官方定位非生产件（SQLite 单进程）；本厂无 Temporal 生产集群。
- poc-temporal 结论：Temporal 的价值在长流程 durable replay + 审批回执 + 事件级审计——这些语义在 af-workorder（状态机+决策工件）+ af-engine（门控）+ MES（事件账本）中已有承载。

## 裁决

1. 订单语义与持久化：**af-engine 9-op 门控 + 订单 ledger + MES 事件账本**，无常驻编排引擎。
2. Temporal 常驻化暂缓；af/orchestration 不接入产线，保留为历史演示件（退役归档随 ADR-008 执行）。
3. 可执行计划的设计必须满足"计划自含"（落地契约 v0 §终节七条）：审批走决策工件不依赖常驻等待、9-op 幂等跃迁、continue-as-new+supersede 续接原语、at-least-once + 幂等键。
4. 若未来需要 durable replay 级编排（如小时级多阶段工作流），重开 ADR 评估 Temporal 生产集群或 DBOS，不复活 dev server。

## 后果

- 下单门面（af-order）是纯 CLI + 状态文件 + af-engine 子进程调用，无需守护进程即可受理订单；
- 执行面的常驻性由 factory-keepalive 看护清单承接（切流后改看护 af 执行面，ADR-008）。

## RT-E138 语义层 span 生产者（2026-11-05 增补）

订单 `drive` 收尾把四层语义 span（`invoke_workflow` / `invoke_agent` / `chat` / `execute_tool`）投影到状态面 JSON。生产者只写数据，不判绿。路径由环境变量 `AF_OTEL_PRODUCTION_STATE` 显式指定；未设置则静默跳过，既有 ledger / MES / 测试语义不变。schema 名 `af.otel.production.v1`；写面为 tmp+rename 原子替换；订单级键取自 `ledger.jsonl` 的 `order_id`（无真实行不落盘）。落点：`crates/af-execline/src/span_produce.rs`，挂钩 `driver.rs` 的 `drive`。

本轮接通消费面（2026-11-05 返修）：`AF_OTEL_EMIT_CMD` 指向 `poc/poc-rt-e138-span-producer/raw/emit-semantic-otlp.py` 时，同一挂钩把投影器产出经既有 OTLP collector（`AF_OTEL_ENDPOINT`，缺省 `http://<内网地址>:4318`）送入 ClickHouse `otel.otel_traces`。断言器 `--production-check` 消费的是 `--production` 判 CH 后落盘的 status/checks 面，不是本地 `af.otel.production.v1`。

键推广（2026-11-05 A3 续建，v2 勘正）：`SpanRow` 订单级键集由单键（`order_id`）推广为双键（`order_id` + `work_order_id`）。**取键两侧独立，容许漂移**：Rust 取 ledger 全部行 `stable_id` 的 `BTreeSet` 最小值（`tracetree.rs`）；投影器 `af-otel-projector.py` 取**首行** `stable_id`（`:90`）写入 `af.work_order_id`（`:408` / `:437`）。P2 判据输入面是 CH，以投影器侧为准。前批「同一真源同一值 / 状态面与 CH 键集 1:1」断言已撤回——漂移/首行缺键时两侧值可以不等，投影器侧会落空串半键行。本状态面 JSON **无消费者入 CH**；CH 订单域行来源=投影器。Rust 侧 `order_domain` 键进入 CH 的效果当前不可直接观测，依赖投影器侧。漂移探针：两侧不等即 WARN（`attrs.ledger_key_drift`，`count_key_drift_warns`），不改 P2 红绿；工厂 `otlp-assert.py` 本批零改（判据面零弱化）。

缺 `stable_id` 路径（v2 勘正）：状态面**新行**两键齐全——`project_order` 在 `report.stable_id` 为空时 fail-closed 不产出行（不落半键）。tracetree 对字段缺席/空串**不报 finding**（仅多值漂移报），前批「核对器另报 finding」为假，已改写。旧状态面行缺 `work_order_id` 字段仍按空串兼容读入（`serde(default)`）；`count_fully_keyed_rows` 如实不计入。发射条件与零行判据不变（只增不减）。
