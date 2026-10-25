# MAPPING · 镜像 ↔ 真实实现一致性说明（INV 组 16 条）

- 真实源：`/home/pan-ding/workspace/af-wt-phase2-dsl`（分支 phase2-g2b，只读）`crates/af-dsl/src/validate/inv{1,2,3,4}.rs`；码位表 `crates/af-dsl/src/error.rs:160-179`；类型 `crates/af-dsl/src/model.rs`。
- 镜像源：本目录 `harness/src/mirror_inv{1,2,3,4}.rs` + `lib.rs` + `conflict_demo.rs`（沙盒 `~/.cache/kani-mirror-inv/` 的落盘副本，同一内容）。
- 抽象层（语义保距声明）：String 词面→小整数域/成员布尔位；`Option<String>` 形状→present 位；u64 quota→{0,>0} 位；点号路径→节点索引相等；作用域链→可见形参成员位。所有「跨层抑制」（refs 层 E2001/E2107/E1004 已定罪处本层跳过）以域值/前提/哨兵显式化并注明——见 CONFLICTS.md 与各「域注记」。
- 码位镜像：`lib.rs` C_E23xx 常量逐一锚定 `error.rs:160-179` as_str 返回值（E2301@:168、E2302@:169、E2303@:170、E2304@:171、E2402@:173、E2311@:175、E2312@:176、E2211@:167、E2321@:177、E2322@:178、E2331@:179）。

## INV-1（mirror_inv1.rs → inv1.rs）

| 镜像实体（harness/src/mirror_inv1.rs） | 复刻的真实代码（validate/inv1.rs） | 对应关系 |
|---|---|---|
| `check`（遍历结构，节点级独立） | `check` :36-40 逐节点 `check_node` | 镜像以「单节点视角」复刻（check 逐节点独立，无跨节点状态） |
| `NodeMirror.llm_bound` | :50 `let llm_bound = node.uses.as_ref().is_some_and(\|u\| u.model.is_some())` | uses.model 在场位 |
| `NodeMirror.has_port` | :53-61 入边端口集（`e.to == node.id` 过滤 `.map(e.input)` 收集） | 2 槽（P0/P1）成员位；「1-2 槽」短域 |
| `port_refs` 计数 | :63 初始化 + :301-303 `port_refs.get_mut(port).(*c += 1)` | 键=真实入边端口（镜像以 has_port 门复刻 :63 只收真实端口） |
| `sec_source_resolvable` FROM_TEMPLATE 支 | :193-203（`template_ref` `is_none_or(is_empty)` ⇒ E2301） | None 与空串合并为 tref_nonempty=false |
| 同 FROM_INPUT 支 | :220-247（fields None ⇒ E2301；Some ⇒ 逐字段查 schema；:231-232 `input_has_properties` 抑制） | fields_present + 单字段 field_ok + schema_has_props 抑制位（1 字段短域；逐字段循环=独立同构判定） |
| 同 FROM_ARG 支 | :250-273（name None ⇒ E2301@:250-259；Some ∧ ∉formals ⇒ E2402@:260-273） | **分码口径镜像**：缺 name 归 E2301、不归 E2402（sec_arg_in_formals 返回 true） |
| 同 FROM_EDGE 支 | :276-306（port None ⇒ E2301；∉in_ports ⇒ E2301@:286-300；合法则计数@:301-303） | port_missing + port_idx ∈{0,1} |
| 同 FROM_STORE 支 | :308-329（store None ⇒ E2301；∉ix.stores ⇒ E2301@:318-329） | store_present + store_declared |
| 词表外 from | :333 `_ => {}` 整段跳过（E1004 已报） | from 域值 5、6 → 跳过支 |
| `sec_quota_ok` / E2302 | :336-346（`quota_tokens == 0 && !(from=="store" && policy==Some("none"))` ⇒ E2302） | 判定在 match 外逐段必判——镜像同构；例外只认 store+none |
| E2303 | :107-133（`llm_bound && reads 非空` → 过滤已声明 → 非空才报） | reads_nonempty ∧ reads_decl_hit 双位；抑制=:104-106 注记+过滤（CONFLICTS #1） |
| E2304 | :135-164（`:142 if llm_bound` 门 + `:143-163` refs==0 / refs>1 同码） | llm 门 + 双向判定（0 与 >1 都触发，后者=实装口径，A-04 UNCERTAIN 审计补证 :153-161） |
| A-05 段级核对照常 | :65 `if let Some(ctx) = &node.context`（无 llm 门） | E2301/E2302 对非 LLM 节点照常——a05 两 harness 钉住 |
| 总量上限路径 | :78-100（opts.context_quota_cap，默认关） | **不在镜像域**（默认不核对——inv1.rs:12 注记「§5 无总量规则」），域注记见镜像文件头 |

## INV-2（mirror_inv2.rs → inv2.rs）

| 镜像实体 | 复刻的真实代码 | 对应关系 |
|---|---|---|
| `actual_writers` | :23-27 实际写者收集（`n.node.writes.iter().flatten().any(\|w\| w == name)`） | 写目标 {无,S0,S1} 成员判定 |
| ② E2312 | :28-40（actual 内每 in_map 节点一条） | in_map 位；与 ① 独立（单码触发，镜像 a08 sanity 钉住） |
| ① E2311 | :42-52（`paths.len() > 1 \|\| (paths.len() == 1 && paths[0] != declared.path)`） | cnt>1 ∨ (cnt==1 ∧ uniq≠declared)；**cnt==0 不报**（:5-6 注记——CONFLICTS #2） |
| writer 悬空跳过 | :19-22 `let Some(declared) = ix.resolve_root_path(...) else { continue }` | DANGLING=3 哨兵跳过支（域内显式值） |
| `written_inside` / `carried_check` | :54-87（:57-59 路径前缀过滤、:62-66 体内写收集（:63 只收已声明）、:68-74 carried 集、:75-86 未覆盖 ⇒ E2211） | in_body 位（前缀过滤语义投影，嵌套体天然含于前缀——inv2.rs:10 注记）；镜像域内两 store 恒已声明（:63 过滤的域注记） |

## INV-3（mirror_inv3.rs → inv3.rs）

| 镜像实体 | 复刻的真实代码 | 对应关系 |
|---|---|---|
| `is_verifier` | :23-25 `matches!(kind, "check"\|"gate") \|\| human.is_some()` | K_CHECK/K_GATE ∨ human 位；开放词表→4 抽象值 |
| E2321 直接自环 | :32-52（:34-36 验证面过滤 + :39-50 同作用域 `e.to == e.from == 本节点`） | 3×3 邻接阵对角位 `g[i][i]`；单作用域域（跨体同本地 id 干扰属 E2003 域，域外注记） |
| 间接自验证不查 | :5-8 模块注记（正本 §8-4 开放问题；正例 indirect_self_verification_not_flagged_v01） | a12 存在性 harness 的 3 节点回环 N0→N1→N2→N0 = 该正例的镜像形态 |
| E2322 三值对应 | :57-89（:64-66 未解析跳过；:67-71 Check→kind=="check"、Gate→kind=="gate"、Human→human.is_some()；:72-88 ⇒ E2322） | SpMirror 三值 match 逐一对应；resolved=false 复刻 :64-66 跳过支；a13 sanity 钉住「check→gate 交叉必拒」（A-13 审计修订核心案例） |

## INV-4（mirror_inv4.rs → inv4.rs）

| 镜像实体 | 复刻的真实代码 | 对应关系 |
|---|---|---|
| `is_evaluator` | :26-28 `kind == "check" && uses.unit.is_some()`（严格双条件——:8-10 注记） | I4_CHECK ∧ unit；gate 不豁免、check 缺 unit 不豁免（a14 sanity 钉住） |
| `reads_store` | :31-42（reads[] 位 ∨ context.sections from:store 段位） | reads[n][s] ∨ sec_ref[n][s]；**oob 位不在函数视野**（A-16 声明面范围） |
| 评估器集 | :45-51 全节点评估器收集（「evaluator nodes」复数口径） | evaluator[n] 数组（对全部 eval_input store 通用；绑定关系 E9003 域，:9-11 注记） |
| E2331 | :44-77（:53-55 只判 eval_input==Some(true)；:57-59 writer 悬空跳过；:61-64 读者 ∧ ≠writer ∧ ¬评估器 ⇒ E2331） | 逐支对应；DANGLING 哨兵跳过支（refs 层 E2001 域，:17-18 注记——域内显式值） |
| A-16 声明面 | :2-7 模块注记「读者 = 经 reads[] 或 from:store 段引用」+【矩阵】:24/:57 | a16①（fired ⇒ 声明面基据）+ a16②（oob 双拷贝相等性）钉住「只覆盖声明面读法」 |

## 镜像实修记录（本批次内发现并修复的镜像 bug，非正本/源码改动）

1. `check_node_mirror` E2304 支初版漏写 `llm_bound` 门（注释有、代码无）——首跑 a05×2/a06 失败暴露，对照 inv1.rs:142 补门后全绿。此条如实记录：镜像首版曾偏离实装，Kani 反例逮住（方法有效性的旁证）。
2. 迭代子组合器（`.find/.filter/.any`）致 CBMC 病态展开——改显式循环（工程纪律，非语义变更）。
3. 两处 completeness 断言极性笔误（`legal ||` → `!legal ||`）——harness 笔误，非镜像语义。

## 与真实测试套的对账现状（如实）

af-dsl 自带 `tests/inv1_test.rs`（:337 多引同码断言）、`inv2_test.rs`、`inv3_test.rs`、`inv4_test.rs` 存在且未被本批次触碰（源码 0 行改动）。本批次的镜像↔实现一致性证据 = 上表逐分支行号锚定 + 抑制角落机器实证 + A-05/A-13 等审计亲核注记（theorems-v1 各条审计栏）。逐例行为级对账单测（FV-0 §1.2 建议形态）未做，残余义务登记于 RUNLOG §6。
