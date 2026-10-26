# af-dsl M1 施工结果（P2-L02）

- 分支：`phase2-dsl`（基 `eac1831`）。模块边界即审计边界（ADR-007）。
- 范围：①拒绝式版本信封（E0001-E0004，version 非 1 整份拒绝、不解析 body）；
  ②七段 AST serde 解析（全文 `deny_unknown_fields`；loop/map/human/route/escape/ops
  六构造字段解析为结构体，语义校验留待 M2-M4）；③引用完整性（类型/节点/store/端口）；
  ④INV-2 写入串行化编译期判定（store 唯一写者 + writes[] 交叉核对 + map 体禁写共享槽
  + carried 覆盖，含 loop/map 体作用域点号路径解析）；⑤语义层一次收齐（Report 范式，
  承 af-graph `validate.rs`）。
- 权威输入：《DSL-语法提案-B》（正本）§1/§1.4/§3.1/§5/§6；
  择优裁决 2026-10-24 §二（topology_selector 编译期解析语义、escape.kind 闭环词表 7 类）。
- CEL 说明：§1.4 词汇表 M1 只解析不判定（`when`/`exit_when`/`assert`/route 规则/
  stop 条件均为不透明字符串；静态判定属 M4）。

## 验收命令与结果（实跑）

| 检查 | 命令 | 结果 |
|---|---|---|
| 全量测试 | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo test -p af-dsl` | **94 通过 / 0 失败**（envelope_test 11 + parse_test 37 + refs_test 37 + inv2_test 9） |
| 静态检查 | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo clippy -p af-dsl --all-targets` | **0 警告** |
| 红绿序列 | `git log --oneline eac1831..HEAD` | 8 commits：4×test(RED) 与 4×feat(GREEN) 严格交替 |

负例覆盖（ask 要求各≥1，均精确断言码集）：信封拒绝（E0001 缺/根非映射/语法错、
E0002 字符串/浮点/布尔、E0003 非 1 含负数、E0004 单/多未知键）；未知引用拒绝
（E2001/E2003/E2104/E2105/E2106/E2107/E2201/E2202/E2204/E2205/E2207/E2208 各≥1）；
INV-2 违规拒绝（E2311×2、E2312 含提案 §5 golden 夹具、E2211×2、三码一次收齐）。
正例门槛：提案 B §4 六个 golden 样例（react_mono/metagpt_sop/debate_kv/aflow_search/
langgraph_subset/voyager_curriculum）解析 + 全量 M1 校验零违例（夹具
`testdata/sample1-6.yaml`，由正本 §4 围栏块机器提取，提取脚本核对六段名一致）。

## 结构体清单（`src/model.rs`，均 `deny_unknown_fields`）

顶层：`Document{version,meta,types,stores,nodes,edges,policies}` ·
`Meta{name,graph_version,entry,input,output,intent?,topology_selector?}` ·
`OutputSpec{ref:PortType,from}` · `TopologySelectorDecl{rules,fallback}` ·
`TopologyRule{when,use→use_graph}`（裁决 §二-1，M1 只解析结构）·
`TypeDecl{name,schema:serde_json::Value}` · `EdgeDecl{from,output,to,input,checkpoint?}` ·
`Policies{budgets,stop,failures}`。

stores：`StoreDecl{name,type:StoreKind,item?,value?,writer,lifecycle:Lifecycle,
retention:Retention,eval_input?}` · `RententionPolicy`（仅 rolling_window）。

nodes：`NodeDecl{id,kind:String(开放),uses?,writes?,reads?,consumes?,emits?,context?,
ops?,assert?,route?,loop_spec?,map?,human?,escape?}` · `UsesDecl{model?,tools?,tool?,
unit?,memory?}` · `EmitSpec{FromBody{from_body}|Type(PortType)}`（自定义反序列化）·
`PortType{Named(TypeRef)|Inline(InlinePrimitive)}`（自定义反序列化；`TypeExpr`：
`Signal|Named|List|Envelope`，一层泛型）· `ContextDecl`/`SectionDecl`（平铺联合体）·
`OpDecl`（平铺联合体）· `RouteDecl{default,rules}`/`RouteRule{when,goto}` ·
`LoopDecl{body,max_iterations,carried?,safety_points,args?,exit_when?,on_exhausted?}` ·
`Body{nodes,edges?}` · `CarriedDecl` · `SafetyPointDecl{node,kind:Check|Gate|Human}` ·
`OnExhausted` · `MapDecl{over,body,max_fanout?}` · `HumanDecl{station,role,quorum?,
resume{schema}}` · `EscapeDecl{kind?,impl_ref,reason,owner,share_budget_pct}`（裁决
§二-2：`EscapeKind` 闭环 7 类）。

policies：`BudgetDecl{id,dimension:8 值,scope,cap}` · `StopDecl{when,outcome:3 值}` ·
`FailureDecl{class:9 值,retry?,route?,feedback?,fallback_route?,terminal?,outcome?}`。

校验侧：`error.rs`（`ErrorCode` 23 码位/`Violation{code,scope,message}`/`Report`/
`LoadError{Envelope|Shape|Semantic}`）· `envelope.rs` · `shape.rs`（serde 错误分类 +
`check_vocab` 手写词表）· `validate/mod.rs`（`Index` 作用域索引 + `NodeRef`）·
`validate/refs.rs` · `validate/inv2.rs`。

## 错误码分配表（M1 实装 23 码位；全部登记于 `src/error.rs`）

| 码 | name | 触发点 | 层 |
|---|---|---|---|
| E0001 | missing_version | 缺 version / 根非映射 / YAML 语法错（归并，见偏差③） | 信封 |
| E0002 | version_not_integer | version 为字符串/浮点/布尔 | 信封 |
| E0003 | version_not_1 | 整数 ≠1（含负数）；短路整份拒绝、不扫其余键 | 信封 |
| E0004 | envelope_unknown_key | 顶层未知键（version 合法后逐键收齐） | 信封 |
| E1001 | unknown_field | serde `unknown field`（全文 deny_unknown_fields） | 形状 |
| E1002 | missing_field | serde `missing field`（含 route.default、escape 记账四字段、body.nodes） | 形状 |
| E1003 | type_mismatch | serde `invalid type`/untagged 失配/类型表达式语法错/端口内联对象 schema/嵌套泛型 | 形状 |
| E1004 | invalid_enum_value | serde `unknown variant`（store 型/lifecycle/安全点三值/escape 7 类/预算维度/stop 终态/失败类/retention policy）+ 枚举形 `invalid value`（如 string 值不在词表）+ 手写词表（ops 8 算子、section 5 来源） | 形状 |
| E2002 | value_out_of_range | serde `invalid value: integer <n>, expected <数值域>`——负数/越界入无符号或正数字段（max_iterations/quota_tokens/cap/max_items/share_budget_pct/backoff_ms/max_fanout 等）；红队 W3-C17 勘误：原与 unknown variant 一并归 E1004，负数入无符号被误报「枚举值非法」，按 serde 实际语义二分后归值域（fix2 起实装） | 形状 |
| E2001 | dangling_reference | meta.entry/output.from/边端点/writer 路径/writes/reads/carried.store | 语义 |
| E2003 | duplicate_id | 同作用域节点 id/类型名/store 名（首个声明者胜） | 语义 |
| E2104 | map_over_not_list | over 解析到非 list（端口类型或 schema 字段 type≠array 且可判定） | 语义 |
| E2105 | arg_over_ref_dangling | map.over 与 loop.args 的 `节点.端口`/`input.字段` 悬空（字段尾部不判） | 语义 |
| E2106 | from_body_dangling | emits.from_body 非体内 `节点.端口`/目标或端口不存在/无构造体 | 语义 |
| E2107 | safety_point_dangling | 安全点路径从体根逐层解析失败（可穿嵌套体） | 语义 |
| E2201 | edge_port_type_mismatch | 出边端口不存在（含内建 done/passed）/入边端口 ∉ consumes/两端类型不等 | 语义 |
| E2202 | output_not_emitted | meta.output.from 不发射 ref 类型（类型不可信时抑制） | 语义 |
| E2204 | failure_route_dangling | failures.route/fallback_route 非关键词(same_node/next_iteration)亦非节点 | 语义 |
| E2205 | unknown_type_name | 类型名/list·envelope 载荷/store item·value/meta.input 不在 types 表 | 语义 |
| E2207 | budget_cap_or_scope_invalid | cap=0 / 作用域非法或悬空（loop:·map: 须构造在场） | 语义 |
| E2208 | store_retention_unbounded | max_items/max_bytes 均缺 | 语义 |
| E2211 | carried_not_covering | 体内（含嵌套体）写入的体外 store 未登记 carried | 语义·INV-2 |
| E2311 | store_multi_writer | 实际写者 ≥2，或唯一实际写者 ≠ 声明值 | 语义·INV-2 |
| E2312 | map_shared_store_write | map 体节点写体外声明的 store | 语义·INV-2 |

未占用（属后续模块，未分配未触发）：E1005-E1007（构造×kind 形状）、E1051-E1053（CEL）、
E2101-E2103（route 归一化）、E2209（ops 参数闭包）、E2210（escape 记账语义）、
E2206/E2002/E2004-E2007（af-graph 图域共享码，M1 不触发）、E2301-E2304/E2310 段
（INV-1/上下文）、E2321/E2322（INV-3）、E2331（INV-4）、E2401-E2403（上下文）、E9xxx（部署期）。

## 与提案 B §3.1 草图的偏差（逐条在案）

1. **`Document.types` 是 `Vec<TypeDecl>` 而非草图 `BTreeMap<String, Schema>`**：YAML 面
   是 `- {name, schema}` 列表（正本 §1.2 样例即列表）；词典化（TypeTable）是 Stage 2
   IR 的事（草图自己也在 §3.2 把 TypeTable 划给 IR）。校验期即时建 `BTreeSet` 查表。
2. **`Document.version: u32` 直存而非信封枚举**：信封检查（E0001-E0003）在 serde 之前
   独立完成（草图的阶段 1a/1b 分层照做），`Document` 只承载已验过的值。
3. **YAML 语法错误归 E0001**（「缺 version」的退化情形）：正本 §6 未给语法错误独立码位；
   poc-cue 先例将装载入口失败全部归 E0 层。若评审要独立码位，余量在 E0005-E0099。
4. **`types/stores/edges/policies` 四段可缺省（`#[serde(default)]`）**：正本 §5 的四条
   INV 违规样例（golden 拒绝夹具来源）均省略其中若干段，故缺省性以正本样例为准；
   草图七段全在。`meta`/`nodes` 仍必填。
5. **形状层单违例（serde fail-fast）而非一次收齐**：serde 天性首错即止；「一次收齐」
   按草图顺序说明落在语义层（Report 全收）。正本 §6 称 E1001-E1004「serde 派生免费」
   即此义。
6. **`ops`/`context.sections` 用平铺联合体而非内部标签枚举**：serde 内部标签枚举不支持
   `deny_unknown_fields`（实测确认）；平铺保住「全文 deny_unknown_fields」硬边界，
   词表闭合改由 `check_vocab`（E1004）。
7. **`EscapeDecl.kind` 收为可选字段**：裁决 §二-2 定的 7 类闭环词表已实装（值域外 →
   E1004），但正本 §4 六样例成文先于裁决、均未带 `kind`——按 golden 样例不能解体的
   纪律收为 `Option`。正本 v0.1 修订把 `kind` 升必填时改一行即可。
8. **E2311 对「零写者」不报**：码名是「多写者」；声明写者不写且无他人写 = 非串行化
   违例（无并发写风险），M1 容忍。正本 §5 规则句「写者集合势=1」的字面全覆盖留待
   后续模块决定是否收严（候选码位 E2311 或新码）。
9. **类型不可信时抑制契约核对**：E2205 已报的文档上，E2201/E2202 的类型相等比较跳过
   （比较对象未登记，重复定罪只产噪声）；E2106 不可解析同理抑制 E2202。

## 遗留（移交 M2-M4，均未动工）

- M2：`context.sections` 语义校验 + INV-1（E2301/E2302/E2303/E2304/E2401/E2402/E2403）。
  `testdata/reject/` 已备正本 §5 inv1 样例来源可提取（本模块未提取，避免夹具孤儿）。
- M3：INV-3（E2321/E2322——提案 §5 inv2 夹具中的 `E2322 safety_point_not_verifier`
  在 M1 有意不触发）与 INV-4（E2331）。
- M4：route 归一化（E2101-E2103）、CEL 保守子集静态判定（E1051-E1053，含 cel-parser
  panic 兜底）、topology_selector 候选图解析。
- 构造形状三码（E1005 构造×kind / E1006 控制节点污染 / E1007 内联体为空）与
  E2209（ops 参数闭包）、E2210（escape 记账语义闭环）未列入 M1-M4 切分，归属待裁决；
  本模块未实现。
- serde_yaml 0.9.34 依赖由 **ADR-001（YAML 主干裁决）授权引入**（任务书 P2-L02 明示），
   已登记 Cargo.lock；上游仓库已归档（0.9.34+deprecated），钉在 0.9 线。
- `max_iterations>0` 等值域（af-graph E2002 域）未在 M1 复检：正本将 E2001-E2007 留给
  af-graph 图域，DSL 面何时共享触发待 M2-M4 编排决定。

## 复现

```bash
cd /home/pan-ding/workspace/af-wt-phase2-dsl
CARGO_TARGET_DIR=~/afbuild-phase2d cargo test -p af-dsl        # 94 passed
CARGO_TARGET_DIR=~/afbuild-phase2d cargo clippy -p af-dsl --all-targets  # 0 warnings
git log --oneline eac1831..HEAD                                # 4×RED → 4×GREEN
```
