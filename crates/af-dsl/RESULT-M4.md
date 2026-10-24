# af-dsl M4 施工结果（P2-L05 · 收口模块）

- 分支：`phase2-dsl-m4`（基 M3 完成点 `fd6b3b8`）。模块边界即审计边界（ADR-007）。
- 范围：①CEL 保守子集静态判定（`src/cel.rs`，用 af-graph 同源 cel-interpreter
  0.10.0 + cel-parser 0.10.1——与提案 §4.7 白名单实测版本一致；畸形串
  catch_unwind 兜底承 af-graph `predicate.rs:11-14` 纪律）；②route 构造校验 +
  归一化（`src/route.rs` 纯函数，§1.5 确定性重写：互斥/覆盖由构造成立）；
  ③topology_selector 编译期解析语义（裁决 §二-1）；④全部 CEL 位位点位
  （route/exit_when/assert/stop/selector）接静态判定——`failures[]` 按 §1.2
  无 CEL 字段（见偏差 5）。
- 权威输入：《DSL-语法提案-B》§1.3/§1.4/§1.5/§3.1/§4.7/§6 + 择优裁决
  2026-10-24 §二-1（topology_selector）/§四-4（模块切分）。
- 至此 M1-M4 全部落成：af-dsl 前端 39 码位、205 例测试、六 golden 样例
  全量（M1-M4）零违例。G1 放行检查以此为对象。

## 验收命令与结果（实跑）

| 检查 | 命令 | 结果 |
|---|---|---|
| 全量测试 | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo test -p af-dsl` | **205 通过 / 0 失败**（envelope 11 + parse 37 + refs 37 + inv1 36 + inv2 9 + inv3 10 + inv4 12 + **cel 10 + route 18 + selector 10 + m4 15**） |
| 静态检查 | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo clippy -p af-dsl --all-targets` | **0 警告** |
| 红绿序列 | `git log --oneline fd6b3b8..HEAD` | RED `340f74f`（53 例：38 红/15 绿，红均因 M4 判定/归一化缺席——`left=[]`）→ GREEN（205 全绿），严格先红后绿 |
| 环境注记 | /tmp 用户配额告警（doctest 临时文件 `QuotaExceeded`） | 以 `TMPDIR=~/afbuild-tmp` 重跑上述两命令，全绿零警告；非代码问题 |

正例门槛：提案 B §4 六个 golden 样例（`testdata/sample1-6.yaml`）全量
（M1-M4）校验零违例（`six_golden_samples_pass_full_m4_validation`）——特别地，
样例 2/3/5/6 的 route 规则 when、loop exit_when、gate assert 全部过三项 CEL
判定（位置词表按作用域链 + schema 建模），样例 3 的 `vote.stats.top_count * 2
> vote.stats.answers` 经由 tally_result schema 的整型合成值真实求值为布尔。

反例覆盖（码集精确断言 + 违例信息逐串比对）：

- **CEL 三项判定 ×8 反例**：方法调用（`'FINISH'.contains(...)`）/ `size`
  函数 / 三元 `_?_:_` / 一元负号 `-_` → E1052（§4.7 反向对照 `size` 同面）；
  未知根（`unknown_root`/`ghost`，单位点与文档级双面）→ E2203；非布尔
  （`steps.turns + 1` → int、`last_tool.first_line` → string、裸 `inputs`
  Map 值）→ E1053；解析失败 + cel-parser 内部 panic（`1 +`/空串/尾部比较
  符）→ E1051（catch_unwind 归一，不击穿）。判定序（解析→算子→根→布尔）
  有专项钉界（同时踩两码只报序前者）。
- **16 条合法夹具**（§4.7 实跑清单逐条）：全过三项判定且 `CelCheck.roots`
  逐条断言；白名单常量逐字锁 15 算子（含 `in` → `@in` 实测形）；15 算子
  逐一构造表达式放行。
- **route 归一化 ×7 反例 + 4 正例逐字**：normalize 纯函数的 guard 公式
  逐字断言（guard_0=`(w)`、guard_i=`(wi) && !((w0) || …)`、guard_d=
  `!(…)`/无规则 `true`）；E2101 出边未覆盖 · E2102 goto 悬空（规则/default
  两形态）· E2103 同目标重复（规则-规则 + 规则-default）各有纯函数与文档
  级恰码恰信息双面；**互斥性/覆盖性性质测试**——布尔赋值网格（a×b 四格）
  上逐格验证全部分支 guard（规则+default+exit）两两不相交且析取永真
  （§1.5「由构造成立」的机器验证）。
- **default 必填**：route/selector 缺 default → serde E1002（§1.3「serde
  层即拒，不进语义 pass」——任务书「default 必填 E2101 系」按正本纠正，
  见偏差 12）。
- **selector ×7 反例**：默认空集任何候选拒（rules+fallback 双报）、已声明
  放行、fallback 独立核对、非版本化形拒（形与集两面：声明集含未版本化名
  不豁免）、when CEL 非法（E1052/E2203）、meta 层节点 id 根不在词表。
- **位点接线 ×8 反例**：stop[].when（E1053/E2203/节点根 E2203）· exit_when
  （attempt/作用域链正例 + E1053/E2203 恰信息）· assert（inputs 建模正例、
  裸 inputs 值 E1053、E2203）· resume 同域 human 正例 · map 体 item 根正例
  · CEL 与引用违例一次收齐（`[E1052, E2102]`，判定序在案）。

## 规则清单（`src/cel.rs` + `src/route.rs` + `src/validate/m4.rs`）

1. **CEL 三项判定**（`cel::check(source, env)`，判定序固定，单错出口）：
   ① `Program::compile` 可解析（Err→E1051；catch_unwind 归一 panic→E1051）；
   ② `references().functions() ⊆ OPERATOR_WHITELIST`（15 算子，`in` 登记
   为 `@in`；函数/方法调用一律拒→E1052；HashSet 迭代序不定——排序保首个
   违例确定性）；③ `references().variables() ⊆ env.roots()`（→E2203）且
   合成绑定上求值为 Bool（Ok(非 Bool)→E1053；**求值错误不判**，偏差 2）。
2. **位置词表（`validate/m4.rs` 环境构造）**：全局五根（steps 四整型字段/
   budget 八维 × spent+cap 十六整型/last_tool 三字段/last_message 单字段/
   input=meta.input schema 展开）恒可见；节点 id 根按作用域链内层优先解析
   （含节点自身；同名校内层胜）；`route.rules[].when` + 验证面裸 `passed`；
   `loop.exit_when` + attempt（体内验证面 passed+emits）+ item（链上 map 体
   元素）；`check/gate assert` + inputs（入边端口集按生产者 emit 类型建模）；
   resume（链上首个 human 构造节点的 resume.schema）。schema→cel 值合成：
   object 递归/原始四型/ array 单元素 list/无 type→Null。词汇表名在可见位
   点为保留字（后写覆盖同名节点 id，偏差 7）。
3. **route 归一化**（`route::normalize`，纯函数）：§1.5 算法逐行——优先级
   前缀 guard、first-hit 边守卫、exit 分支不产边（联合守卫按文档序析取）。
   构造校验一次收齐：E2102（goto ∉ 出边目标 ∪ {exit}，规则序→default）→
   E2103（同一出边目标多分支命中；exit 重复良性）→ E2101（出边未覆盖；
   E2102/E2103 在场时整体抑制）。全部通过才产 `RouteNorm{edges, exit_guard}`
   ——G2（af-compiler）的 `Edge.guard` 落点直接消费。
4. **topology_selector**（编译期解析语义）：rules[].when 过全局词表 CEL
   判定；rules[].use 与 fallback 均须**版本化 name@version 形**且 ∈
   `ValidateOptions.declared_graphs`（包产物注入；默认空集=全拒）→ E2001。

抑制规则（承 RESULT-M1 偏差⑨纪律）：route E2101 在 E2102/E2103 在场时
整体抑制；CEL 每表达式单错出口（判定序首错）；CEL 位点的根集合成失败
（input/item 等 Null）不产 E2203 级联——悬空已由 E2205/E2104/E2105 定罪。

## 错误码分配表（M4 实装 7 码；`src/error.rs` 全 39 码位）

| 码 | name | 触发点 | 层 |
|---|---|---|---|
| E1051 | cel_parse | CEL 解析失败（含 cel-parser panic 兜底归一） | 形状·表达式 |
| E1052 | cel_non_subset | 非算子函数/方法调用（白名单 15 算子外） | 形状·表达式 |
| E1053 | cel_not_boolean | 谓词求值非布尔（合成绑定 Ok(非 Bool)） | 形状·表达式 |
| E2101 | route_edge_uncovered | route 出边目标未被任何规则/default 覆盖 | 语义·拓扑 |
| E2102 | route_goto_dangling | goto 既非出边目标也非 exit（规则/default） | 语义·拓扑 |
| E2103 | route_rule_unreachable | 同一出边目标被多条分支命中（后中者不可达） | 语义·拓扑 |
| E2203 | cel_unknown_root | CEL 根绑定 ∉ 位置词表 | 语义·契约 |

## M1-M4 全模块错误码总表（本 crate 产出面；39 码）

| 模块 | 码位 | 明细 |
|---|---|---|
| M1 信封 | E0001-E0004 | E0001 missing_version · E0002 version_not_integer · E0003 version_not_1 · E0004 envelope_unknown_key |
| M1 形状（serde 派生） | E1001-E1004 | E1001 unknown_field · E1002 missing_field · E1003 type_mismatch · E1004 invalid_enum_value |
| M4 表达式 | E1051-E1053 | E1051 cel_parse · E1052 cel_non_subset · E1053 cel_not_boolean |
| M1 引用闭包（图域共享） | E2001/E2003 | E2001 dangling_reference（与 af-graph 共码位；M4 扩展至 selector 候选）· E2003 duplicate_id |
| M4 拓扑 | E2101-E2103 | E2101 route_edge_uncovered · E2102 route_goto_dangling · E2103 route_rule_unreachable |
| M1 拓扑 | E2104-E2107 | E2104 map_over_not_list · E2105 arg_over_ref_dangling · E2106 from_body_dangling · E2107 safety_point_dangling |
| M1 契约 | E2201/E2202/E2204/E2205/E2207/E2208/E2211 | E2201 edge_port_type_mismatch · E2202 output_not_emitted · E2204 failure_route_dangling · E2205 unknown_type_name · E2207 budget_cap_or_scope_invalid · E2208 store_retention_unbounded · E2211 carried_not_covering |
| M4 契约 | E2203 | E2203 cel_unknown_root |
| M2 上下文装配 | E2301-E2304/E2401-E2403 | E2301 context_source_unresolvable · E2302 quota_not_positive · E2303 agent_direct_store_read · E2304 input_port_unreferenced · E2401 store_view_policy_unknown · E2402 arg_not_in_params · E2403 template_vars_dangling |
| M1 INV-2 | E2311/E2312 | E2311 store_multi_writer · E2312 map_shared_store_write |
| M3 INV-3/INV-4 | E2321/E2331 | E2321 self_verification · E2331 eval_input_visible |
| （未实装，码位在 §6 表但不属任何已交付模块） | E1005-E1007/E2209/E2210/E2322 | 构造形状三码 · ops 参数闭合 · escape 记账 · safety_point 非验证面——M1/M3 遗留，未变（见「遗留」） |

## 与提案/任务书的偏差（逐条在案）

1. **CEL 判定位次**：§3.1 草图序「CEL 编译 → 引用闭包」，实装在 M4 pass
   （refs/inv1-4 之后）——三项判定需按位置词表（作用域链+schema）建合成
   绑定，依赖引用索引。语义层一次收齐不受影响（违例集合与序确定）。
2. **求值错误不判**：E1053 只在求值 `Ok(非 Bool)` 时触发；NoSuchKey/未声明
   引用/类型不符等求值错误放行。理由：合成绑定只承担**布尔性**判据（§4.7
   判据 ③ 的文面），字段级词汇核对（`steps.不存在字段`）不在 §1.4 判定面；
   且 schema 缺 type 时合成 Null，若按错误拒绝将产 schema 质量级联。代价：
   文档级正例不区分「真求值」与「求值错误放行」——单位点 16 夹具（手工
   建模真值）+ `steps.turns + 1` 文档级 E1053 共同保证判别力。
3. **E2103 扩展**：§1.5 文面「同一目标被多条**规则**命中」；实装含规则与
   default 同目标（first-hit 语义下 default 的路由贡献丢失——同一「后中者
   不可达」缺陷，信息注明 default 分支）。多条 **exit** 规则良性（exit 分支
   不产边、无丢失；联合守卫按文档序析取）。
4. **E2101 抑制**：E2102/E2103 在场时整体抑制（goto 悬空/重复已定罪，覆盖
   面不可信，不重复定罪）——normalize 内实现，纯函数层可测。
5. **「failures 路由条件」不存在**：任务书 ④ 要求 failures 路由条件的 CEL
   过判定，但 §1.2/模型的 failures[] 无 CEL 字段（`route` 是目标串或
   same_node/next_iteration 关键词，E2204 已在 M1 判）。按正本取「无此判定
   对象」；④的其余文面（stop[].when）+ 全位点（route/exit_when/assert/
   selector——§4.7 十六夹具的实跑清单所要求）全部接线。
6. **stop[].when / selector when 词表 = 全局五根**（无节点 id 根）：§1.4
   节点 id 绑定的可见范围是「从当前作用域向外逐层解析」，文档级 policies/
   meta 无作用域链——窄解释（写 `n1.answer.ok` → E2203 有反例钉界）。需要
   节点状态做停止条件的写法是 route/exit_when。
7. **词汇表保留字**：节点 id 与词表名同名（如名为 `steps` 的节点）时，词
   汇表绑定覆盖节点根（词汇表在可见位点语义固定）。同名节点从 CEL 不可
   引用——v0.1 样例无此形态，不设专码。
8. **resume 同域多 human**：取 BTree id 序首个（索引作用域表为 BTreeMap，
   文档序不可得；确定序优先）。§1.4「第一个带 human 构造的节点」无同域
   二义样例。
9. **inv1_test 三处 `..Default::default()`**：`ValidateOptions` 新增
   `declared_graphs` 字段，M2 已交付的三处结构体字面量机械补默认——断言
   与测试语义零改动（「不改 M1-M3 已交付测试语义」的字面边界内，特此登记）。
10. **selector 候选集注入面**：候选图是包产物（裁决文面），v0.1 单文档不
    自带声明面——经 `ValidateOptions.declared_graphs` 注入，默认空集=任何
    候选引用拒（G1 文档不用 selector 则零影响）。码位用 E2001 共享
    （「候选图引用存在且版本化」即悬空引用语义；§6 码表无专用码，禁止私占
    空号）。版本化形 = `@` 两侧非空（版本字符集不收紧，与 graph_version
    "1.0" 形一致）。
11. **exit_guard 联合形**：多 exit 分支的联合守卫为各分支 guard 的析取
    （无外层括号，依赖 CEL `&&` 高于 `||` 的标准优先级）——与边守卫同构，
    性质测试在其上直接求值。
12. **「default 必填 E2101 系」按正本纠正**：任务书文面提示 default 缺失
    属 E2101 系；正本 §1.3 明定 default 必填 → serde E1002（「serde 层即拒，
    不进语义 pass」）。取正本，测试 `route_without_default_is_rejected_as_e1002`。
13. **RED 期夹具笔误修正 ×3**（GREEN 前修，断言意图不变）：三元算子登记名
    `_?_:_`（测试误写 `_?_:`）；多 exit 联合守卫期望改为分支守卫析取形
    （`(a) || (b) && !((a))`，与边守卫同构而非化简形）；E2101 文档级夹具的
    default 与规则同目标（会先踩 E2103）改为 default: exit。

## 遗留（移交后续，均未动工）

- **G2（af-compiler）**：`RouteNorm.edges/exit_guard` → `Edge.guard` 的 IR
  落地、内联体降维、`CelRef{source, roots, digest}`——本 crate 已产出可直接
  消费的归一化产物与判定出口。
- **E2322**（safety_point 非验证面）：M3 遗留，实装需协调 M1 夹具断言
  （RESULT-M3 偏差 1），未变。
- **构造形状三码 E1005-E1007 / E2209 / E2210**：M1 遗留归属待裁决，未变。
- **字段级 CEL 词汇核对**（`steps.不存在字段` 之类）：§1.4 判定面外；若
  评审要求，需把求值错误按位点细分（偏差 2 的对称扩展）。
- **E2005 裸环 / E2006 孤立**：af-graph 图域码，本 crate 不产出（M1 决策
  未变；route 归一化后裸环判定属 G2 拓扑检查）。

## 复现

```bash
cd /home/pan-ding/workspace/af-wt-phase2-dsl-m2
CARGO_TARGET_DIR=~/afbuild-phase2d cargo test -p af-dsl          # 205 通过 / 0 失败
CARGO_TARGET_DIR=~/afbuild-phase2d cargo clippy -p af-dsl --all-targets   # 0 警告
git log --oneline fd6b3b8..HEAD                                  # RED → GREEN
# /tmp 配额告警时：TMPDIR=~/afbuild-tmp 前缀重跑（本机实测全绿）
```
