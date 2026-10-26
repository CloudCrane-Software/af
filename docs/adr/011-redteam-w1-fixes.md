# ADR-011: 红队修复批（W1）——执行面范围裁定 + 判分链/预算面/装订面收口

- 状态：已裁决（2026-10-26，红队修复批；依据：红队候选报告 53 条 real-defect 裁定，裁判亲验）
- 取代：ADR-005 裁决 1/3（MARBLE 主选与 bench→WorkOrder 转换协议——裁决 4/5）
- 关联：总计划 §0 三适配器承诺（裁决 1/2）、§2 裁决 7（裁决 8）、SYNTAX.md 四层契约（裁决 6）

## 背景

红队第一轮（W1）对第二阶段交付投喂 54 条缺陷候选，裁判裁定全部 real-defect
（前波清单 53 条 + 续核增补 W1-C55：G5 健康度首报「轴二·成功」行计数三数互斥，
completed×6 误计 verdict=false 的 F5b——处置在工作区文书，无 ADR 决策项；
W1-C14 的遗留表总数 3,043,489→3,042,489 手工加总勘误与决策定时器落地同属
工作区 ops 面）。本 ADR 登记其中需要**决策记录**（非纯代码修复）的部分；
逐条修复与测试见 commit 23d9d7a / ae98366 / ecb5d99 / 71b7cef / 777672b
（九门+advisory 全绿，修复员 2026-10-26 亲跑复核）。

## 裁决

1. **mcode headless 适配器不在第二阶段范围**。总计划 §0:14-17 承诺的三适配器
   （af-native / swe-v+terminal-bench / mcode headless「经门面派发」）中，
   mcode headless 对照线（m3.1f 等）未建、亦无取消记录——执行面承诺被单方
   缩减且无裁决痕迹（W1-C01）。本 ADR 补录：mcode headless 推迟出第二阶段，
   是否/何时建线随后续阶段另裁；总计划 §0/§6 的三适配器表述由本 ADR 取代。
2. **terminal-bench 以库形态吸收，不设门面路由**。TbExecuteUnit 仅库在册
   （`af-execline::suite::tb_unit`），`af-order --unit` 枚举保持
   swev|report|graph|stub（裁决 7 原口径）；tb 对拍需求经执行面直调（p5 测试
   面）满足，不增设门面单元。
3. **bench→WorkOrder 转换器推迟至扩量期**（W1-C02）。W2 裁决（2026-10-24）
   已定「保留、目标改 TaskBench、非首单前提」；本 ADR 正式登记：转换器不在
   第二阶段交付面，随外部口径对照需求再立任务。ADR-005 裁决 3 的接入协议
   条目相应冻结。
4. **MARBLE 退出第二阶段**（W1-C05，补 ADR-005 修订承诺）。多 agent 语义走
   AFB 原生图（sweng sop_full / reportkit paper_report），不经外部 bench 框架；
   ADR-005 裁决 1「主选 MARBLE」作废，裁决 2「备选 TaskBench」转入裁决 3 的
   冻结状态。
5. **af-order 门面命令面 = submit / list / show**（W1-C04 勘误）。ADR-002 与
   总计划 §0 的 submit/approve/list/close 四命令枚举按实交付修订：submit 一
   命令走通全生命周期（create→bind→voucher→release→start→execute→complete→
   close 由 `OrderLifecycleDriver` 内部驱动），approve/close 不设门面子命令
   （引擎 9-op 生命周期在役；`approve_decision` op 归编排侧）。
6. **af-dsl 语义收紧五项**（W1-C22..C-26/C-28，SYNTAX.md 契约面演进）：
   E2202 判定面接入内建端口表（loop/map done、check/gate passed——与
   port_exists/emit_type 同源）且匹配端口须恰一（同型双端口=终态工件歧义）；
   CEL 词法面跳过 `//` 行注释（与 cel-parser 文法同口径，块注释不跳——文法
   不收且跳过即给深括号绕 E1054 前置闸开门）；route `default:exit` 产出
   ¬(when₁∨…) 并入 exit_guard（非 loop 文档不再静默丢失默认分支守卫）；
   新增 E2209 construct_conflict（loop+map 构造并存，解析面/类型面自相矛盾
   故 fail-closed 拒绝）；E2311 文案按违例实况分叉。golden 7 份随批再生
   （CrewAI/GPTSwarm/LangGraph/MetaGPT/Reflexion/sample2/sample5，diff 均
   route_exit_guard 增位）。
7. **执行面/门面修复清单**（详 commit ecb5d99）：strict declare 键集=全部
   artifact_contract 端口（C-31 死锁收口）；graph_version 一次性绑定真核对
   +门面按计划图版本取值（C-32）；报告环共享预算账本（C-33——cents/turns/tokens_total 三维白名单强制，plan turns/token 帽不得静默忽略，强制集外条目拒收；完整化见 commit 53c2504）；预算面强制集
   外组合 fail-loud（C-34）；MES 写失败跃迁重放补发（C-35）；图行 af.plan.
   record.v1 契约化（C-36，evidence.run_dir 同步闭合 trace L1→L2）；边级重影
   与路径安全核（C-37/C-38）；判据面 digest 钉死 + tests_collected=0 不当成功
   （C-59）；渲染器入双指纹门（C-61）；MES 同 event_id 异 line 拒并（C-63）；
   stub 冒烟档 MES line_id 加 `stub:` 前缀（C-64，产线聚合口径按前缀排除）；
   凭证面 line 身份钉死（C-41）。
8. **CI 面修复**（详 commit 71b7cef）：step9 9b 解析 Complete 汇总行并断言
   harness 计数恰 32/0 failures（证明集删减此前零防护，W1-C44）；中立性门
   词边界改 ASCII 口径（中文注释邻接面，W1-C60）+ 词表登记 4 处新见豁免 +
   `--self-test` 对抗自检；K5 判分链路径 lint 落地并入 step1（W1-C03，
   `ci/k5-path-lint.py` + `ci/scorechain-manifest.txt` 冻结锚——总计划 §2
   裁决 7 的执行）。

## 理由

- 执行面承诺与交付的偏差必须留裁决痕迹（无记录的缩减 = 单方变更）；
- 判分真值源（任务包判据/渲染器基准）与预算面是可作弊面，收口方向一律
  fail-closed；
- 语法演进按 SYNTAX.md C10 契约走 ADR + 全量 golden 再生。
