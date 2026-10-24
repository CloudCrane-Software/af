# af-dsl M2 施工结果（P2-L03）

- 分支：`phase2-dsl-m2`（基 M1 完成点 `049572a`）。模块边界即审计边界（ADR-007）。
- 范围：①`context.sections` 语义校验（来源存在性 / 视图策略域 / 配额约束）；
  ②INV-1 上下文完整性判定（上下文断流类违规）；③错误码按提案 §6 的 E23xx/E24xx
  段分配（七码，全部避开 M1 已用码）；④`ValidateOptions` 可配置项（context 配额
  总量上限，默认关）。
- 权威输入：《DSL-语法提案-B》§1.2『context.sections』小节（from 五类 + policy
  词表 + quota_tokens 必填 + message_log 交错合并）+ §5 INV-1 判定规则 + §6 码表；
  择优裁决 2026-10-24 §四模块切分（M2 = context.sections + INV-1）。
- 不变量边界：INV-3/INV-4/route 归一化/CEL 静态判定/topology_selector 未动工（M3/M4）；
  既有 crate 与 M1 已交付测试零改动（M1 的 94 例断言原样全绿）。

## 验收命令与结果（实跑）

| 检查 | 命令 | 结果 |
|---|---|---|
| 全量测试 | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo test -p af-dsl` | **130 通过 / 0 失败**（envelope 11 + parse 37 + refs 37 + inv2 9 + inv1 36） |
| 静态检查 | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo clippy -p af-dsl --all-targets` | **0 警告** |
| 红绿序列 | `git log --oneline 049572a..HEAD` | test(RED 34 例，28 反例红/6 正例绿) → feat(GREEN 130 全绿)，严格先红后绿 |

正例门槛：提案 B §4 六个 golden 样例（`testdata/sample1-6.yaml`）全量（M1+M2）
校验零违例——含 `validate_with` 宽松总量上限（100000）路径；六样例单节点配额
总量最大 7200（sample5 agent）。反例覆盖（≥4 类 INV-1 违规，码集与信息逐字断言）：
E2301×5（含 §5 inv1 golden 夹具 `testdata/reject/inv1_e2301_e2303.yaml`——由提案
§5 围栏块逐字提取，触发恰 [E2301, E2303]、两条信息逐字对齐 §5 预期错误风格）；
E2302×4（零配额 / policy:none 记 0 例外正反两面 / 可配置总量上限含恰等边界）；
E2303×4（直读、未知 store 抑制、混合、非 LLM 对照）；E2304×4（未覆盖、无 context
的实际消费、双引用、非 LLM 对照）；E2401×9；E2402×3（含作用域链向外收集正例与
item 根图不可见）；E2403×4；四节点七码一次收齐 ×1。

## 规则清单（`src/validate/inv1.rs`，判定序 = 段序 → E2303 → E2304）

1. **E2301 来源不可解析**：`input.fields` ∉ meta.input schema（逐字段）；`edge.port`
   ∉ 本节点入边端口集（所在体作用域内 `to == 本节点` 的边的 `input` 去重集）；
   `store` 名不在 stores 表；各 from 缺必配来源字段（template.template_ref /
   input.fields / edge.port / store.store / arg.name——平铺联合体的 per-from 必配
   项 serde 无法表达，归入「来源不可解析」的退化情形）。
2. **E2401 store 视图策略域**：store 段必填 `full|latest_k|retrieve|summarize|none`；
   参数闭合——latest_k/retrieve 须 `args.k` 正整数，retrieve/summarize 须
   `component`（非空）；input/arg/edge 段策略缺省合法、出现须恰为 `project`；
   template 段不得带 policy。
3. **E2302 配额非正**：`quota_tokens = 0`（u64 无负值）；例外：store 段
   `policy: none` 记 0 合法（提案 §7-5 词法变通）。可配置总量上限
   （`ValidateOptions.context_quota_cap`，默认 None 不核对——§5 INV-1 无总量
   规则）：超限同码 E2302、信息注明「总量」（见偏差 3）。
4. **E2402 arg ∉ 所在体形参表**：形参可见集沿作用域链向外收集——loop 体贡献
   其 `args` 键、map 体贡献隐式 `item`、根图无体形参（`input` 经 from:input 段
   进入）。map 体嵌 loop 体时两层并集可见（正例在案）。
5. **E2403 template vars 引用悬空**：vars 值为字面量（无 `input.`/`arg.` 前缀）
   不判；`input.字段` ∈ meta.input schema；`arg.名` ∈ 可见形参；前缀后空名
   （`input.`/`arg.`）报悬空。
6. **E2303 agent 直读 store**：LLM-bound 节点（判定 = `uses.model` 在场，与 kind
   开放词表正交）声明非空 `reads[]` → 一条违例列出其已声明的 store 名。
7. **E2304 入边端口闭合**：判定对象 = 带 context 的节点 ∪ LLM-bound 节点；
   入边端口零引用（实际消费未声明——含 LLM-bound 有入边而无 context 的情形）
   与多引用（违「恰被一个」）各报一条。非 LLM 节点读侧是 `reads[]`，不要求
   section 覆盖。

抑制规则（承 RESULT-M1 偏差⑨，避免噪声级联）：from 不在五词表（M1 E1004 已报）
整段跳过；meta.input schema 无 properties（E2205 域）跳过字段核对；reads 全为
未声明 store（refs 层 E2001 已定罪）不重复报 E2303、混合时 E2303 只列已声明项。

## 错误码分配表（M2 实装 7 码；`src/error.rs` 全 30 码位）

| 码 | name | 触发点 | 层 |
|---|---|---|---|
| E2301 | context_source_unresolvable | input 字段/edge 端口/store 名/per-from 必配字段不可解析 | 语义·INV-1 |
| E2302 | quota_not_positive | 段 quota=0（例外 store+none）；可配置总量超限（同码注明「总量」） | 语义·INV-1 |
| E2303 | agent_direct_store_read | LLM-bound 节点声明 reads[]（已声明项逐名列出） | 语义·INV-1 |
| E2304 | input_port_unreferenced | 入边端口零引用 / 多引用（须恰被一个） | 语义·INV-1 |
| E2401 | store_view_policy_unknown | store 段缺策略/越词表/参数不闭合；非 store 段策略不适用 | 语义·上下文 |
| E2402 | arg_not_in_params | arg 名不在作用域链形参可见集 | 语义·上下文 |
| E2403 | template_vars_dangling | vars 的 input./arg. 引用悬空或空名 | 语义·上下文 |

§6 E23xx/E24xx 段内未占用余量（E2305-E2310、E2313-E2320、E2323-E2330、E2332+、
E2404+）未私占；§6 预算「每新增一条须在提案登记」，本模块未新增码位。

## 与提案 B 的偏差（逐条在案）

1. **E2301 覆盖「缺必配来源字段」**：提案 §5 只列引用不可解析；平铺联合体（M1
   结构决策，保 deny_unknown_fields）的 per-from 必配字段缺失在 serde 层不可见，
   语义层无处安放则整段无来源可判——按「来源不可解析」的退化情形归 E2301。
   候选纠正位：E1002/E1005 域，若评审归形状层可迁移（触发面不变）。
2. **input/arg/edge 段 policy 缺省容许**：提案 §1.2 表列 policy 列无 `?` 注记
   （字面读法为必填 project）；但 §6 E2401 码名义限「store 视图策略」，缺省视为
   缺省即 project 的宽容读法，从严会私扩码义。store 段 policy 必填从严（词表
   语义自足）。六样例全部显式携带 project，从严读法不影响 golden。
3. **可配置总量上限复用 E2302**：任务书要求「总量上限可配置」，提案 §5/§6 无
   总量规则与独立码位（§6 预算禁止私占空号）。默认 None 不核对（§5 语义原样，
   六样例不受影响）；配置后超限报 E2302、信息注明「总量超上限」与计入口径
   （none 段按 0）。若评审要独立码位，余量在 E24xx（建议 E2404），迁移点：
   `inv1.rs` 总量分支一处。
4. **LLM-bound 判定 = `uses.model` 在场**：提案用词「LLM-bound task 节点」，kind
   是开放词表（M1 决策）不可作判据；model 在场是唯一静态可判定面。`uses.tools`
   仅（无 model）不算 LLM-bound（样例 5 tools 节点即此类，其读侧是 writes 落账
   而非 reads）。
5. **E2304 判定对象 = 带 context ∪ LLM-bound**：提案 §5 主语是 LLM-bound task
   节点；context 仅 LLM-bound 可用属 E1005（构造×kind，归属待裁决，M1 遗留），
   在 E1005 落地前，非 LLM 节点带 context 的端口闭合同样核对（其 context 已
   声明即应自洽），E1005 落地后可按需收窄。
6. **E2303 抑制未知 store 项**：reads 引用未声明 store 时 refs 层 E2001 已定罪，
   E2303 只列已声明项（混合时同报两码、E2303 列已声明子集）。提案 §5 无此样例；
   此为保 M1 已交付测试（`reads_unknown_store_is_e2001` 断言恰 [E2001]）不动
   的裁决，承 M1 偏差⑨「不重复定罪」纪律。文档拒绝性不受影响（E2001 已拒）。

## 遗留（移交后续模块，均未动工）

- M3：INV-3（E2321/E2322）与 INV-4（E2331——`reads`/`context.sections` 引用
  eval_input store 的读者集合核对，M2 已建立 sections 的 store 引用面，M3 可
  复用 `Index`+`visible_formals`）。
- M4：route 归一化（E2101-E2103）、CEL 保守子集静态判定（E1051-E1053）、
  topology_selector 解析。
- 构造形状三码（E1005/E1006/E1007，含「context 仅 LLM-bound task 可用」的
  E1005 面——见偏差 5）与 E2209/E2210 归属待裁决（M1 遗留，未变）。
- edge/arg 段的 `fields` 深核对（字段 ∈ 生产者端口类型 schema / 形参元素
  schema）需类型内省，提案 §5 INV-1 未列，未实现。
- `template_ref` 仅核非空；包内模板存在性属部署期组件核对（E9001 域）。
- message_log 多段按 step 交错合并是零字段内建运行期规则（提案 §1.2），无静态
  判定面，未实现校验（样例 5 注释行即其用例）。
- `latest_k` 的 `redact?` 可选参数与 full/none 段的多余 args 未设拒绝规则
  （提案 §6 码名义不含「多余参数」）。

## 复现

```bash
cd /home/pan-ding/workspace/af-wt-phase2-dsl-m2
CARGO_TARGET_DIR=~/afbuild-phase2d cargo test -p af-dsl        # 130 passed
CARGO_TARGET_DIR=~/afbuild-phase2d cargo clippy -p af-dsl --all-targets  # 0 warnings
git log --oneline 049572a..HEAD                                # RED → GREEN
```
