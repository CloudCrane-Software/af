# ADR 索引（第二阶段）

> 架构决策记录。裁决者：第二阶段编排者（Leader Agent）。冲突裁决序：宪法 > 任务中立 > 向后兼容 > 功能丰富度 > 开发速度。
> 引用格式：`ADR-00N`。修改已有 ADR = 新增 ADR 并标注取代关系，不原地改写。

| 编号 | 标题 | 状态 |
|---|---|---|
| [ADR-001](001-dsl-form.md) | DSL 形态：YAML 主干 + CEL 谓词，Python 仅限 L4 逃逸舱 | 已裁决 2026-10-24 |
| [ADR-002](002-crate-layout.md) | 落仓与 crate 划分（af-dsl/compiler/plan/order/execline + packages）（**部分被 [ADR-013](013-af-plan-fold-in.md) 取代 2026-10-27**：`crates/af-plan` 规划行作废，折入 af-compiler 追认） | 已裁决 2026-10-24；部分取代 2026-10-27 |
| [ADR-003](003-order-persistence.md) | 订单持久化走 af-engine+ledger+MES，Temporal 常驻化暂缓 | 已裁决 2026-10-24 |
| [ADR-004](004-mes-unification.md) | MES 记账统一收敛到 af-mes crate | 已裁决 2026-10-24 |
| [ADR-005](005-multi-agent-bench.md) | 多 agent bench 选型：MARBLE 主 + TaskBench 备（**部分被 [ADR-011](011-redteam-w1-fixes.md) 取代 2026-10-26**：裁决 1 MARBLE 主选作废、裁决 3 bench→WorkOrder 转换协议冻结） | 已裁决 2026-10-24；部分取代 2026-10-26 |
| [ADR-006](006-second-package.md) | 第二配置包 = 研究报告包（任务中立性证明件） | 已裁决 2026-10-24 |
| [ADR-007](007-tdd-discipline.md) | TDD 开发规约：红-绿-重构、小模块自包含可审计 | 已裁决 2026-10-24 |
| [ADR-008](008-bench-retirement.md) | 旧 bench 基础设施退役纪律（单一入口终态） | 已裁决 2026-10-24 |
| [ADR-009](009-resource-discipline.md) | 资源并行纪律（与 AB 测试会话共存） | 已裁决 2026-10-24 |
| [ADR-010](010-expert-feedback.md) | 专家高维意见的讨论与吸纳裁决（补登索引，红队 W1 批） | 已裁决 2026-10-24 |
| [ADR-011](011-redteam-w1-fixes.md) | 红队修复批（W1）：执行面范围裁定（mcode headless 推迟/tb 库形态/转换器冻结/MARBLE 退出）+ 判分链/预算面/装订面收口 | 已裁决 2026-10-26 |
| [ADR-012](012-redteam-w2-fixes.md) | 红队修复批（W2）：判分/预算/身份面收口 + 口径勘误（补登索引行——文件 2026-10-26 已裁决在档，索引行此前漏登） | 已裁决 2026-10-26 |
| [ADR-013](013-af-plan-fold-in.md) | af-plan 折入 af-compiler 的正式裁决（补登）——ADR-002 crate 划分修订：规划行作废、折入追认、独立建位留 v0.2 开放议程（红队 W2-C-03） | 已裁决 2026-10-27 |
