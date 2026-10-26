# ADR-005: 多 agent bench 选型——MARBLE 主 + TaskBench 备

- 状态：已裁决（2026-10-24，编排者）；**部分被 [ADR-011](011-redteam-w1-fixes.md)
  取代（2026-10-26）**：裁决 1「主选 MARBLE」作废（MARBLE 退出第二阶段，
  W2 裁决 + 多 agent 走 AFB 原生图）；裁决 3 的 bench→WorkOrder 转换协议
  冻结（转换器推迟至扩量期）。裁决 2「备选 TaskBench」相应转入冻结。
  本文件正文保留为裁决时点原貌（ADR README 纪律：不原地改写）。

## 依据

- 设计/benchmark调研/04-多智能体协同效率.json（15 条带 availability/recommendation）：MultiAgentBench/MARBLE 已被计划/40 §2.3 裁决"改造采用"；TaskBench 的 Tool-Graph DAG 与本厂宪法 #1/#2（只传工件引用、数据流图非对话拓扑）同构度最高；TheAgentCompany 最接近端到端接单但体量重。
- MAST 14 类失败分类学 → Evaluation/安灯标签（不在本 ADR 范围，随执行面建设接线）。

## 裁决

1. **主选 MARBLE/MultiAgentBench**：里程碑 KPI 判分；消息语义必须改造为工件投递（宪法 #1/#2，40 计划已列改造注意事项）。
2. **备选 TaskBench**：作 DSL 图语料与第二落点（Tool-Graph→工单图转换），若 MARBLE 改造受阻即切换。
3. 接入走 计划/40 §3.2 bench→WorkOrder 转换协议：转换器实现为工厂 Tool（instruction→需求工件、oracle→注入器考题、测试脚本→终检 Evaluation、时限→预算凭证、判定→三态）。
4. 纪律：availability 未核实的 bench 不得当可用（benchmark调研 README 纪律）；接入前先跑通下载/判分冒烟再改造。
