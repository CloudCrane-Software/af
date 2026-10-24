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
