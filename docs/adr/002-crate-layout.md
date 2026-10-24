# ADR-002: 落仓与 crate 划分

- 状态：已裁决（2026-10-24，编排者）

## 裁决

第二阶段全部新代码落 **af 仓**，自 `guardrail-fix2`（94cd192）起开 `phase2-*` 分支线：

| 单元 | 职责 |
|---|---|
| `crates/af-dsl` | 前端：语法规范实现、YAML→AST 解析、CEL 谓词接入、L0 不变量静态判定 |
| `crates/af-compiler` | 四阶段编译管线：校验→IR→组件绑定→计划生成 |
| `crates/af-plan` | 可执行计划对象 + 落地契约类型（消费 落地契约-v0.md） |
| `crates/af-order` | 下单门面 CLI：submit/approve/list/close，直驱 af-engine 9-op |
| `crates/af-execline` | 统一执行面：吸收 af-bench 的 swev/tb 执行逻辑为库，挂订单生命周期 |
| `packages/` | 组件与领域配置包登记处（模型路由/推理循环/记忆/工具/上下文压缩组件 + 软件工程/研究报告配置包） |

af-bench 的 swev/tb 模块**迁移而非重写**（保留判分三件套契约：task_json/eval.log/agent.diff）；af-bench CLI 在切流前保持可用（影子对拍需要），退役时降级为薄兼容壳或移除。

## 理由

- 与 af-graph（图模板+校验器）、af-harness（AgentSpec 四元组）复用距离最短；
- gitea 远端与 GitHub 镜像统一覆盖，无新仓库管理面；
- 组件仓库（使命指令 COMPONENT_REPO）以 `packages/` 目录 + manifest 承担，不另建外部仓。

## 边界

- 不修改 af-workorder/af-engine 既有语义（订单模型是底座资产）；确需扩展 op 集走新 ADR。
- 每个新 crate 从第一天起可独立 `cargo test`（ADR-007）。
