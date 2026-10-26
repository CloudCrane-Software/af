# af/packages —— 包图清单（P2-L09）

> 失败分类账 #1-4（ADAS / AutoGen-Team-JSON / GPTSwarm / MetaGPT 被 E2001 拒）
> 终裁为**缺组件**：topology_selector 候选图的引用形合法，缺的是包图清单 +
> 装载注入面。本目录即该清单的正本；装载器为 `af-dsl::registry`。

## manifest 规范（首版：声明集）

- 每包一个目录：`af/packages/<包>/manifest.json`（JSON——serde_json 已在
  af-dsl 依赖面内，可 serde 静态校验）。
- 字段（全文 `deny_unknown_fields`，未知字段即拒）：

| 字段 | 层级 | 约束 |
|---|---|---|
| `package` | 顶层 | 非空、不含 `'@'` |
| `graphs` | 顶层 | 至少一项 |
| `graphs[].name` | 图 | 非空、不含 `'@'`；可含点号分层（如 `team.round_robin`） |
| `graphs[].version` | 图 | 非空、不含 `'@'`；自由串（同 `meta.graph_version`，如 `"1"`、`"2.1"`） |
| `graphs[].source` | 图 | 非空；图本体（AFB 文件）定位串，**首版只登记不打开** |

- 组合规则：`pkg.{package}.{name}@{version}` —— 恰为文档中
  `topology_selector` 的候选图引用形（`af-dsl` `validate::m4` 按
  `ValidateOptions.declared_graphs` 精确串匹配）。
  例外登记（红队 W1-C69，2026-10-26）：**reportkit 包的组件名段为
  `pkg.report.*`（非包目录名 `reportkit`）**——六件
  web_search/web_fetch/pdf_extract/citation_unit/rubric_unit/mem.notes
  落 `pkg.report.*` 段，而同包 template_ref 用 `pkg.reportkit.*`（G4 施工
  裁决 10/12 后的模板面），同包两段并存。G4「组件名段匹配包目录」约定对
  reportkit 不成立，登记为已知例外（更名牵动注册表/计划快照/golden 联动，
  收益不成比例；消费者按 components.json 实况解析，不按目录名推导）。
- 唯一性：同清单内、跨包间组合引用均不得重复（装载即拒，首声者路径在案）。
- 扫描规则：**目录即包候选**——`af/packages/` 下每个非隐藏目录必须有
  `manifest.json`；非目录条目（README.md、`.gitkeep` 等）不是包，跳过；
  空根合法 = 空集 = fail-closed（一切候选引用仍拒，与不注入同效，
  无隐式放行）。

## 首批四包（图清单 = 4 份对齐草稿 topology_selector 引用闭包）

| 包 | 服务草稿 | 图数 |
|---|---|---|
| `adaskit` | ADAS.yaml:12-14 | 3 |
| `autogen` | AutoGen-Team-JSON.yaml:14-19 | 5 |
| `swarmkit` | GPTSwarm.yaml:11-13 | 3 |
| `sweng` | MetaGPT.yaml:23-25 | 3 |

图本体（AFB 文件）可后续提供；装载期只登记引用（`source` 指向预留路径）。

## components.json —— 组件声明清单（P2-L11，af-compiler stage 3 装载）

`af-dsl::registry` 只读 `manifest.json`（包图清单）；本目录自 P2-L11 起另有
**组件声明清单** `components.json`，装载器为 `af-compiler::components`（DSL
侧对此文件零感知——`manifest.json` 规范与四包闭包不变）。

- 位置两种：**包级** `af/packages/<包>/components.json`（该包自有的
  tool/unit/memory 组件）与**根级** `af/packages/components.json`（跨包共享
  组件——本首版收 model_route 模型路由档案）。根级文件是非目录条目，
  `af-dsl::registry` 的目录扫描天然跳过。
- 每条声明：`class`（model_route | tool | unit | memory）+ `name`（DSL
  `uses.*` 引用串去 `@版本` 段）+ `version`；tool 类必填 `effect_class`
  （pure | persist | external_once——ADR-010 effect_class 回填源）；model_route
  类必填 `endpoint`（af-harness `EndpointSpec` 字段级同形，`key_ref` 只存
  引用名）；可选 `impl_ref`（实现定位串）。
- 组合引用：`{name}@{version}`。同名同版本跨文件重复 → 装载即拒（首声者
  路径在案）。目录按路径序处理，确定性装载；全量声明的规范序列化取
  SHA-256 即 `registry_digest`（计划对象携带，执行面核对——E9004 面）。

- `impl_ref` **双形态合法**（G4 裁决13，2026-10-25 入契约）：
  ①包路径形态 `packages/<pkg>@<ver>/...`（预留路径，装载期只登记）；
  ②执行面 builtin 形态 `executor:builtin/<组件名>@<crate>`（无版本段——
  版本面住组件声明本身；`<crate>` = 实现宿主 crate 名）。装订器按前缀
  `executor:builtin/` 判别；既有 15 稿计划全为包路径形态，golden 零影响
  （impl_ref 不入计划投影，`plan.rs` `PlanComponents` 只携带
  reference+effect_class）。首批 builtin 锚 = sweng 域 15 件（根级
  `pkg.sweagent.*` 十件 + sweng 包级 `pkg.sweng.*` 五件，G4 Phase 1
  锚改写；实现住 `af-execline/src/pkg/sweng.rs` 进程内注册表）。

- 包内提示词资产（G4 裁决10）：`<包>/prompts/<名>.md` +
  `MANIFEST.sha256` 清单（头行 `# manifest_version: <版本>`；数据行
  `<sha256>  prompts/<名>.md`，与 `sha256sum` 输出同形）。DSL 图的
  `context.sections[].template_ref`（`pkg.<包>.<名>[@<版本>]`，版本段
  可选）由执行面装载期解析到该文件并逐件 sha256 核对，不符拒绝启动
  （EX4001-4003；版本段给出时与清单版本核对）。首包 = sweng
  （swev_system/swev_instance_protocol/aci_system + G4 收尾批 SOP 面
  planner_system/engineer_system/reviewer_system——sop_full/sop_no_qa/
  sop_minimal 三图的模板引用）；reportkit 同机制（reader_system/
  writer_system）。

- 图源文件：manifest `graphs[].source` 定位串 ↔ `<包>/graphs/<名>@<版本>.afb`
  落盘正本。sweng 四图（sop_full/sop_no_qa/sop_minimal/fix_swe_task）与
  reportkit paper_report 均已在盘（G4 收尾批物化 selector 时代「声明无源」
  的三份 SOP 图源；装载面仍只登记引用——registry.rs 首版语义不变）。

首批声明 = golden 源 sample2 / sample3 / GPTSwarm 三稿 `uses.*` 引用闭包：
`route.main@1`、`route.cheap@1`（根级）；`pkg.sweng.locate/view/edit`、
`pkg.sweng.test_unit@1`（sweng）；`pkg.swarmkit.tools@1`（swarmkit）。

## P2-L12 扩容（G2G3 放行裁决 §三.1 缺组件处置，2026-10-24）

G3 全链闸门 L1-L4 首轮 13 稿败于 L4 绑定（E9001 ×57，去重 50 名——闸门
§⑤ 覆盖清单）。本扩容**声明式注册**这 50 名：条目语义 = 该组件名已被
注册表承认、绑定层可解析；**非实现**——`impl_ref` 指向预留包路径（承
sweng 首批先例：装载期只登记引用，实现本体归 G4/执行面）。

- 扩容量：model_route 5（route.plan/manager/alt/judge/reflect，根级，
  与 route.main/cheap 同端点面——plan/manager/judge/reflect 主力档
  GLM-5.3、alt 备档 GLM-5.3-Flash）+ tool 28 + unit 13 + memory 4
  = 50；总计 7 + 50 = **57 条声明**。
  （红队 W1-C54 勘误，2026-10-26：G4 reportkit 建包 +6（web_search/
  web_fetch/pdf_extract/citation_unit/rubric_unit/mem.notes），终态总量
  **63 条**——`registry_decl_test.rs` 断言 component_count()==63、
  `bind_test.rs` 断言 63；本行保留 P2-L12 时点数，正本口径以 63 为准。）
- 落位：**已有包**补包级清单——adaskit（task_loader/verifier）、
  autogen（tool.search/calc、team.stall/term_unit、mem.list）、sweng
  （publish_task_finished）；**未建包**（aflowkit/agensh/camelkit/
  crewkit/debatekit/reactkit/reflexkit/sweagent/langgraph/openworld 十
  包，尚无图清单）的暂驻**根级** components.json 的 `pkg.*` 命名空间段
  （红队 W1-C65 勘误，2026-10-26：十包实为 **37 件**——aflowkit 2 /
  agensh 8 / camelkit 1 / crewkit 7 / debatekit 2 / reactkit 2 /
  reflexkit 3 / sweagent 10 / langgraph 1 / openworld 1，aff4f1a 落地当天
  即 37；历史文书中的「35 件」均误抄 2 件）。不新建空包目录：`af-dsl::registry` 规则「包目录必须有
  manifest.json 且 graphs 至少一项」（registry.rs:230-234/:152）——十包
  无图可声明（selector 闭包 14 图属 adaskit/autogen/swarmkit/sweng 四包），
  建 dir 即造空清单或伪图，两皆不可。G4 建包时将暂驻件迁包级。
- 版本：版本化引用对齐稿内 `@1`；未版本化引用的 7 名
  （pkg.agensh.inspect/local_edit/local_run/publish_update、
  pkg.reactkit.hopqa_api/lookup、pkg.sweng.publish_task_finished）恰好
  单版本声明 `"1"`（bind 未版本化解析须唯一，否则 E9002 歧义拒）。
- effect_class 判例（tool 28）：执行面/环境/网络类 external_once
  （task_loader×2、local_run、web_search/fetch、debatekit.search、
  hopqa_api/lookup、env_exec/search、aci.run_test、sweagent.env、
  openworld.env）；落盘类 persist（local_edit、publish_update、
  board_append、workspace_commit、aci.edit/create/submit、
  publish_task_finished）；只读/计算类 pure（inspect、kickoff_inputs、
  doc_render、tool.calc、aci.search/open）。承闸门 §⑤ 效果面建议
  （env_exec/env/run_test/submit → persist 或 external_once；
  web_search/fetch → external_once）。
- 测试：`crates/af-compiler/tests/registry_decl_test.rs`（50 名可解析/
  版本+类逐名、57 总量、7 名单版本消歧、真实声明 E9003 类不匹配拒）。

