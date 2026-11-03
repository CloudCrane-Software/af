# ADR-004: MES 记账统一收敛到 af-mes crate

- 状态：已裁决（2026-10-24，编排者）

## 背景

现状双写入路径并存：af-mes crate（outbox 攒批 + RowBinary 写入）为设计态；在役消费全部是 Python 旁路（factory-sched/mcode-dispatch/keepalive/bench-mes 直 INSERT `af_mes.mes_event`，work_order_id 用 `factory-<node_id>` 约定，与 Rust 侧 StableId 体系无关）。

## 裁决

1. 目标态：MES 记账唯一路径 = af-mes crate（outbox）；新执行面（af-execline/af-order）从第一天起只走 af-mes。
2. 切流观察期内，旧 Python 旁路允许**只读**（对照对账），不再新增记账点。
3. 旧旁路随 ADR-008 退役一并废除；`work_order_id` 收敛到 af-workorder StableId 体系。

## 后果

- af-mes 需在 G2 前补齐：Python 侧调用桥（CLI 或 FFI 级薄壳）供执行面调用——若成本高，允许过渡期由 af-execline 直接以 Rust 调用（本就是 crate）。
- 对账口径：mes_event 中 work_order_id 前缀 `factory-` 的历史行=旧体系遗留，不迁移、不重写（历史账保留原样）。
