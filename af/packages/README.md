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

首批声明 = golden 源 sample2 / sample3 / GPTSwarm 三稿 `uses.*` 引用闭包：
`route.main@1`、`route.cheap@1`（根级）；`pkg.sweng.locate/view/edit`、
`pkg.sweng.test_unit@1`（sweng）；`pkg.swarmkit.tools@1`（swarmkit）。
