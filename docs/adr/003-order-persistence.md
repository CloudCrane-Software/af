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
