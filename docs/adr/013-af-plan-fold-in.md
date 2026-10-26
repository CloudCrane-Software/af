# ADR-013: af-plan 折入 af-compiler 的正式裁决（补登）——ADR-002 crate 划分修订

- 状态：已裁决（2026-10-27，红队修复批；依据：红队 W2-C-03 裁定——`af-plan`
  折入事实散见多份文书且均称「ADR-002 修订」，但该修订的正式裁决载体此前
  不存在，违反 ADR README 纪律「修改已有 ADR = 新增 ADR 并标注取代关系，
  不原地改写」。裁判亲验：主线 438fa6d 与 phase2-redteam-fixes 两树的
  ADR-002 原文在列 `crates/af-plan` 行、索引无取代行）
- 取代：ADR-002 规划表中 `crates/af-plan` 行（**部分取代**——落仓决定、
  其余 crate 划分、迁移纪律与边界照旧有效）
- 关联：docs/VERSIONS.md §一/§五、软件工程包设计 §7-2、af README crate 表、
  计划/00-总纲.md、状态/组件建设状态.md

## 背景

ADR-002（2026-10-24）规划六单元，其中 `crates/af-plan`（可执行计划对象 +
落地契约类型，消费 落地契约-v0.md）实际从未建位：crate 清单 22 个无
af-plan；`ExecutablePlan` 与落地契约类型落在 `crates/af-compiler/src/plan.rs`
（`PLAN_VERSION = 1`，经 af-compiler lib 导出），随编译管线与 golden 21 份
逐字节回归同 crate 演进。折入事实自建设期起散见 af README、
docs/VERSIONS.md §一注、计划/00-总纲.md、状态/组件建设状态.md，多处表述为
「ADR-002 修订」——而 ADR-002 正文与 ADR 索引均无该修订的裁决行，正式载体
缺位（红队 W2-C-03，裁判亲验）。

## 裁决

1. **折入追认（正式裁决）**：`af-plan` 不再作为独立 crate 建位；其规划
   职责——可执行计划对象（`ExecutablePlan`、`PLAN_VERSION`）与落地契约
   类型——由 `crates/af-compiler` 承担（`src/plan.rs`）。ADR-002 规划表
   `crates/af-plan` 行由本裁决取代；ADR-002 其余内容继续有效。
2. **「独立建位」议题不注销，留在 v0.2 开放议程**：折入是阶段内裁定而非
   永久封案——「类型搬迁去重」按 docs/VERSIONS.md §五「开放议程（在档，
   非承诺）」继续登记（软件工程包设计 §7-2）。若未来独立建位重启，走新
   ADR 并标注对本 ADR 的取代关系。
3. **表述纪律**：此后文书引用本折入一律点名「ADR-013（追认折入）」，不再
   写「ADR-002 修订」——该表述曾指向不存在的载体，正是本条缺陷的根源。
   既有引用面（af README crate 表、docs/VERSIONS.md、计划/00-总纲.md、
   状态/组件建设状态.md）随本裁决同步改为点名 ADR-013。

## 理由

- 折入是既成事实且已经 22 crate 清单、golden 21 逐字节回归、九门全绿在案
  验证；补登裁决恢复「文书—事实—裁决」三面对齐，不改变任何代码。
- `ExecutablePlan` 与编译管线共用类型演化节奏（`PLAN_VERSION` 随计划面
  变更同步进位），阶段内独立 crate 无消费者隔离收益；去重搬迁的收益留待
  v0.2 议程按需评估（裁决 2 不注销议题，正是为该评估留门）。
- ADR 纪律的价值在裁决载体可追：散见表述若无正式行，后续审计无法区分
  「已裁决的折入」与「未裁决的漂移」。

## 边界

- 本 ADR 不移动任何代码、不改任何类型路径与公开契约
  （`ExecutablePlan` schema、`PLAN_VERSION = 1` 不变）；纯文书裁决。
- af-compiler 的 crate 版本仍 0.1.0（阶段内修复不进位，对齐
  docs/VERSIONS.md 注记纪律）。
