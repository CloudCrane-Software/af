# RESULT-G3L4 —— P2-L12：50 缺组件声明式注册 + L1-L4 闸门重跑（2026-10-24）

分支 `phase2-g3-l4`（基 `phase2-g2b @ e85bfdb`）。执行 G2G3 放行裁决 §三.1
缺组件处置：**声明式注册（非实现）**，合规 R2（不改草稿/不改 golden/不写
变更日志）。闸门报告正本：`计划/研究组/G3-全链闸门-L1L4-v2-20261024.md`。

## 1. 注册清单（50 名，声明条目）

对照面来源：G3 闸门 §⑤ 覆盖清单（15 稿 uses.* 103 处 → 缺 57 → 去重 50）。
类归属经 15 份草稿 uses 面逐名复核（`uses.tool`/`uses.tools` 同归 tool），
零同名双面冲突；版本对齐稿内引用串（@1 或未版本化单版本 `"1"`）。

| 类 | 数 | 落位 |
|---|---|---|
| model_route | 5 | 根级：route.plan/manager/judge/reflect（GLM-5.3）、route.alt（GLM-5.3-Flash），与 route.main/cheap 同端点面 |
| tool | 28 | sweng 补 1（publish_task_finished）；adaskit 1（task_loader）；autogen 2（tool.search/calc）；余 24 根级 |
| unit | 13 | adaskit 1（verifier）；autogen 2（team.stall/term_unit）；余 10 根级 |
| memory | 4 | autogen 1（mem.list）；余 3 根级（crewkit.mem.longterm/entity、debatekit.belief） |

- 文件：根级 `af/packages/components.json`（7→44 条）、新建
  `af/packages/adaskit/components.json`（2）、`af/packages/autogen/components.json`
  （5）、`af/packages/sweng/components.json` 追加 1。合计 7+50=**57 条声明**。
- **不新建空包目录**：`af-dsl::registry` 规则「包目录必须 manifest.json 且
  graphs 至少一项」（`crates/af-dsl/src/registry.rs:230-234`/:152）——十包
  （aflowkit/agensh/camelkit/crewkit/debatekit/reactkit/reflexkit/sweagent/
  langgraph/openworld）无图可声明（selector 闭包 14 图属既有四包），建目录
  即伪图。其 35 件暂驻根级 `pkg.*` 命名空间段，G4 建包时迁包级（README 已记）。
- **schema 零扩展**：四类（model_route/tool/unit/memory）全在 P2-L11 既有
  schema 内，装载器/绑定器零改动——`crates/af-compiler/src/` 本任务零触碰。
- effect_class 判例（28 tool）：执行面/环境/网络 external_once（12）、落盘
  persist（8）、只读/计算 pure（8），承闸门 §⑤ 建议；impl_ref 一律指向
  预留包路径（`packages/<pkg>@1.0.0/...`，承 sweng 先例）——**声明非实现**。

## 2. 矩阵 v2（15 × 4，目标 ≥12/15）

**15/15 全链通过（L1 解析 / L2 校验 / L3 IR / L4 绑定+计划全 ✅）**，超目标。
v1 为 2/15（GPTSwarm、MemGPT）——本轮 13 份 E9001 败稿全部转绿，零新败因、
零新错误码（E9002/E9003 零出现）。L3 通道：15/15 IR 与编译器 golden 集逐位
一致（`af-compiler-snapshot` ×15 cmp 全等）；双跑确定性 15/15 逐位一致。
15 份 plan.json 已落 `计划/研究组/对齐计划快照/`（13 新增 + 2 覆盖）。
全 15 稿同一 registry_digest
`d5f3bd3aa0f7ea65eb66f6534b22c255ea2f0860c1602e8adad290fd52d57977`
（57 条声明规范序列化 SHA-256）；plan_id 与 v1 逐字相同（dsl 未动）。

## 3. 测试与验收

- TDD 先红后绿：新增 `crates/af-compiler/tests/registry_decl_test.rs` 5 例
  （50 名可解析+版本+类逐名 / 57 总量 / 7 未版本化名单版本消歧 / 真实声明
  E9003 类不匹配拒 / 表形与闸门 §⑤ 聚合一致性）；`bind_test.rs` 既有正本
  计数断言 7→57 同步。RED 实录：4 例红（名未注册/E9001 代位）+ 计数红；
  GREEN：`CARGO_TARGET_DIR=~/afbuild-phase2d cargo test -p af-compiler`
  **58/58 全绿**（既有 53 + 新 5；golden 全绿零回归）。clippy 零新警告
  （仅 golden_test.rs:256 既有 1 条）。
- 复现命令：`~/afbuild-phase2d/release/af-compiler-plan --registry
  <工作树>/af/packages <draft>.yaml`（harness `~/.cache/afb-l14gate/l14gate`
  重建于 `CARGO_TARGET_DIR=~/afbuild-phase2d`，bin 仍为上游 CLI 逐字副本，
  双 `cmp` 全等；原始输出 `~/.cache/afb-l14gate/v2/{runs,runs2,ir}/`）。

## 4. 遗留

1. 十包 35 件驻根级——G4 建包（有图可声明或 registry 规则演进）时迁包级。
2. 50 件皆声明非实现：impl_ref 预留路径，实现本体归 G4/执行面；effect_class
   为注册面判例，执行面实测可调（声明版本化可承）。
3. route 5 条端点为档案面（角色档位 plan/manager/judge/reflect=主力、alt=备），
   角色差异化参数归 G4。
4. CrewAI context retrieve 组件 `pkg.crewkit.mem.similarity@1` 不在 uses.*
   面、不进绑定对照（v0.1 范围）——context 组件注册面是后续议题。
5. registry_digest 变更使 v1 两份快照更新（diff 恰一行：digest），已覆盖落盘。
