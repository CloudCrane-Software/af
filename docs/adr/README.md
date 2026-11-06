# ADR 索引（第二阶段）

> 架构决策记录。裁决者：第二阶段编排者（Leader Agent）。冲突裁决序：宪法 > 任务中立 > 向后兼容 > 功能丰富度 > 开发速度。
> 引用格式：`ADR-00N`。修改已有 ADR = 新增 ADR 并标注取代关系，不原地改写。

| 编号 | 标题 | 状态 |
|---|---|---|
| [ADR-001](001-dsl-form.md) | DSL 形态：YAML 主干 + CEL 谓词，Python 仅限 L4 逃逸舱（占比 >30% **警戒线**的被测总体由 [ADR-014](014-escape-threshold-policy.md) 指定 2026-10-27：锚定生产图口径，对齐稿口径分列作观察项——不修改本 ADR，仅补其未指定的被测总体） | 已裁决 2026-10-24；警戒线被测总体由 ADR-014 指定 2026-10-27 |
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
| [ADR-014](014-escape-threshold-policy.md) | 逃逸舱占比治理阈值口径：锚定生产图口径（1/16=6.2%，未触 ADR-001 警戒线），对齐稿口径分列作观察项（97/247=39.3%，12 条语法缺位转 v0.2 候选），30% 数值不变、恢复「警戒」语义（计划侧工作稿「ADR-012」称呼与仓内 ADR-012 无关，本 ADR 取下一个空闲号 014——013 已被 af-plan 折入占用） | 已批准 2026-10-27 编排者 |
| [ADR-015](015-trigger-inbox-resident-contract.md) | trigger-inbox 常驻运营面契约：生产落点（af 仓 `tools/trigger-inbox.py` 版本受控正本 ＋ 工作区级 `tools/trigger-inbox.py` 在役运行位，两处逐字同源 sha256 对账）／暴露面收窄做成代码级 fail-loud 闸（通配 `0.0.0.0`/`::` 一律拒起 exit 3；非回环须 `AF_TRIGGER_BIND_ACK` 显式确认）／源断供安灯分码（形态A 既有码 `trigger-inbox-down` ＋ 形态B 新码 `trigger-inbox-source-silent`，不新建第四个守护）／存储形态＝收件侧 per-delivery 文件 ＋ 读侧 JSONL 游标（补登索引行——文件 2026-11-04 已落地在档、索引行此前漏登；bind 取值已由主代理裁为**序列 B**：`AF_TRIGGER_BIND=<内网地址>` ＋ `AF_TRIGGER_BIND_ACK=<内网地址>`，序列 A 不采用） | 已裁决 2026-11-04；bind 取值＝序列 B（主代理裁决）；拉起闸＝主代理（2026-11-04 23:57:55 已按序列 B 起在役） |
