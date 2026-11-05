# P3-A1 三态清单（61 条人读表）

全集口径：`strategies/index.yaml`（61 条）。分布：compilable 44 / needs_syntax 0 / not_applicable 17。

| # | rank | id | tier | topic | 三态 | 判定依据 | 59簇 |
|---|---|---|---|---|---|---|---|
| 1 | 1 | `st-C008` | A | 多agent协同 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 137 行 | Y |
| 2 | 2 | `st-C007` | A | 评测方法论 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 150 行 | Y |
| 3 | 3 | `st-C015` | A | 上下文管理 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 121 行 | Y |
| 4 | 4 | `st-C019` | A | 工具与接口 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 88 行 | Y |
| 5 | 5 | `st-C006` | A | 多agent协同 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 115 行 | Y |
| 6 | 6 | `st-C005` | A | 记忆 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 89 行 | Y |
| 7 | 7 | `st-C016` | A | 多agent协同 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 152 行 | Y |
| 8 | 8 | `st-C018` | A | 反思与自修正 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 95 行 | Y |
| 9 | 9 | `st-C010` | A | 多agent协同 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 98 行 | Y |
| 10 | 10 | `st-C009` | A | 规划与分解 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 164 行 | Y |
| 11 | 11 | `st-C012` | A | 多agent协同 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 150 行 | Y |
| 12 | 12 | `st-b-C058` | B | 工具与接口 | not_applicable | na_reason：离线组件选型规程：策略内容是「构造内部评测集→A/B 不同检索器与查询改写→报 nDCG@10 并追踪下游任务成功率」的实验方法（S056 body.测试设计）… | Y |
| 13 | 13 | `st-b-C060` | B | 测试时扩展 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 81 行 | Y |
| 14 | 14 | `st-b-C097` | B | 验证与测试 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 98 行 | Y |
| 15 | 15 | `st-b-C121` | B | 记忆 | not_applicable | na_reason：受控评测协议：策略内容是「配对 A/B + 控制模型/预算/工具集 + 同时比较 pass rate、token 消耗与回归率」的实验设计（S160 body.… | Y |
| 16 | 16 | `st-b-C108` | B | harness工程 | not_applicable | na_reason：harness 仓库自身的发布纪律：被评测与防护对象是 harness 工程过程（版本回归集、发布阻断阈值），不是任何一次 agent 运行图；AFB 是单次运… | Y |
| 17 | 17 | `st-b-C111` | B | 多agent协同 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 73 行 | Y |
| 18 | 18 | `st-b-C037` | B | 强弱模型协同 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 59 行 | Y |
| 19 | 19 | `st-b-C063` | B | 技能与插件 | not_applicable | na_reason：技能资产的内容级治理规程：策略约束的是「技能文本怎么写才可自动检查、怎么命名命令/路径、触发率与加载成本如何同测」（S177 body.测试设计），属技能资产准… | Y |
| 20 | 20 | `st-b-C098` | B | 验证与测试 | not_applicable | na_reason：能力标定基准：策略内容是「先用 CRBugBench 量化 review agent 抓真 bug 的上界，再决定是否进无人值守」的准入标定协议（S106 bo… | Y |
| 21 | 21 | `st-b-C077` | B | 反思与自修正 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 70 行 | Y |
| 22 | 22 | `st-b-C080` | B | 评测方法论 | not_applicable | na_reason：评测有效性方法论：策略内容是「基准污染自查 + 首错自查能力 + 多 judge 不可依赖」的评测纪律（S122 body.测试设计），约束的是实验解释而非运行… | Y |
| 23 | 23 | `st-b-C125` | B | 多agent协同 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 116 行 | Y |
| 24 | 24 | `st-b-C071` | B | 工具与接口 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 85 行 | Y |
| 25 | 25 | `st-b-C073` | B | 评测方法论 | not_applicable | na_reason：离线轨迹归因规程：策略内容是「对历史轨迹跑失败模式判官、把工具使用错误/语法错单独拆类、用聚类分布排工具改进优先级」的分析方法（S067 body.测试设计），… | Y |
| 26 | 26 | `st-b-C028` | B | 上下文管理 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 81 行 | Y |
| 27 | 27 | `st-b-C042` | B | 记忆 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 48 行 | Y |
| 28 | 28 | `st-b-C054` | B | 评测方法论 | not_applicable | na_reason：实验场地选型方法：策略内容是「A/B 主战场选哪个基准、任务时长/token 量级多大、如何多基准交叉」的评测规划（S037 body.测试设计）；AFB 描述… | Y |
| 29 | 29 | `st-b-C102` | B | 多agent协同 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 97 行 | Y |
| 30 | 30 | `st-b-C118` | B | 评测方法论 | not_applicable | na_reason：评测指标定义：策略内容是「把 $/resolved、tokens/call、昂贵失败比设为一等指标并建成本基线」的度量口径（S170 body.测试设计）；AF… | Y |
| 31 | 31 | `st-b-C035` | B | 仓库级理解与检索 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 81 行 | Y |
| 32 | 32 | `st-b-C069` | B | 工具与接口 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 95 行 | Y |
| 33 | 33 | `st-b-C087` | B | 反思与自修正 | not_applicable | na_reason：实验设计纪律：策略内容是「带反馈重试的 A/B 必须配同预算盲重采样安慰剂臂」的对照设计要求（S086 body.测试设计）；AFB 表达单次运行图，不表达多臂… | Y |
| 34 | 34 | `st-b-C114` | B | 评测方法论 | not_applicable | na_reason：基准组合策略：策略内容是「公开饱和集做回归、活水集做外推」的评测集分层方法（S147 body.测试设计），属实验规划面；AFB 无基准集合管理语义。… | Y |
| 35 | 35 | `st-b-C126` | B | 工具与接口 | not_applicable | na_reason：接口选型方法：策略内容是「按失败成本与实际接口服从（而非 token 数）选 CLI/MCP」的选型判据（S176 body.测试设计），是工具面决策规程；AF… | Y |
| 36 | 36 | `st-b-C128` | B | 技能与插件 | not_applicable | na_reason：工程资源配置建议：策略结论是「上下文文件无可测收益，预算从写文件挪到调优文件/验证器」的投入导向（S178 消融设计），属资产管理决策；AFB 的 templa… | Y |
| 37 | 37 | `st-b-C046` | B | 技能与插件 | not_applicable | na_reason：技能资产组织与注入治理：策略中的 DAG 是「技能/计划资产之间」的依赖组织（资产图），不是 agent 运行图；AFB 的图节点是运行时执行体，无技能资产实体… | Y |
| 38 | 38 | `st-b-C064` | B | 技能与插件 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 65 行 | Y |
| 39 | 39 | `st-b-C078` | B | 验证与测试 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 81 行 | Y |
| 40 | 40 | `st-b-C095` | B | 测试时扩展 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 75 行 | Y |
| 41 | 41 | `st-b-C096` | B | 验证与测试 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 86 行 | Y |
| 42 | 42 | `st-b-C109` | B | 强弱模型协同 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 77 行 | Y |
| 43 | 43 | `st-b-C003` | B | 评测方法论 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 90 行 | Y |
| 44 | 44 | `st-b-C038` | B | 上下文管理 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 79 行 | Y |
| 45 | 45 | `st-b-C048` | B | 验证与测试 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 82 行 | Y |
| 46 | 46 | `st-b-C055` | B | 多agent协同 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 95 行 | Y |
| 47 | 47 | `st-b-C075` | B | harness工程 | not_applicable | na_reason：训练期方法：策略内容是「RL 训练中联合演化 harness 工具面与 hooks」的训练流程（S069 body.测试设计），被演进对象是训练过程中的工具/h… | Y |
| 48 | 48 | `st-b-C092` | B | 测试时扩展 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 87 行 | Y |
| 49 | 49 | `st-b-C036` | B | 仓库级理解与检索 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 73 行 | Y |
| 50 | 50 | `st-b-C001` | B | 测试时扩展 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 118 行 | Y |
| 51 | 51 | `st-b-C029` | B | 上下文管理 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 111 行 | Y |
| 52 | 52 | `st-b-C062` | B | 技能与插件 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 88 行 | Y |
| 53 | 53 | `st-b-C086` | B | 上下文管理 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 82 行 | Y |
| 54 | 54 | `st-b-C105` | B | 上下文管理 | not_applicable | na_reason：消融实验方法：策略内容是「逐组件替换做可拆解消融、按模型/窗口分档读差距」的实验设计（S129 body.测试设计）；AFB 单图无法表达「同图多组件配置矩阵」… | Y |
| 55 | 55 | `st-b-C124` | B | 成本与推理效率 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 39 行 | Y |
| 56 | 56 | `st-b-C022` | B | 记忆 | not_applicable | na_reason：判分校准方法学：策略内容是「无标准答案场景的评审门禁设计 + 评委校准 + 判分污染检测」的评审质量方法（OB 卡目标句），约束的是判分器本身的可信度；AFB … | Y |
| 57 | 57 | `st-b-C057` | B | 工具与接口 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 70 行 | Y |
| 58 | 58 | `st-b-C056` | B | 规划与分解 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 88 行 | Y |
| 59 | 59 | `st-b-C079` | B | 执行与沙箱 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 72 行 | Y |
| 60 | 70 | `st-C122` | C | 记忆 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 62 行 | N(C档回写) |
| 61 | 81 | `st-C103` | C | 多agent协同 | compilable | 编译门实测 COMPILE-OK（load_str+validate 零违例）；dsl_snippet 118 行 | N(C档回写) |

超出 59 簇口径的两条：['st-C122', 'st-C103']——超出两条均为 tier C 实验承载档：循环产线按实验终裁自动回写建条（st-C122←EXP-1 终裁 inconclusive；st-C103←EXP-2 同型），strategies_check.py v1.2 契约修订（tier 闭集扩 A|B|C|D）收编。
