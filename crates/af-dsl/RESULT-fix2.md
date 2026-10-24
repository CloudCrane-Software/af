# RESULT-fix2 — af-dsl 受控循环三守卫 + 执行语义规范勘误（P2-L08）

- 任务包：P2-L08（G2a 独立验证三缺口修复，报告：`计划/研究组/af-compiler-G2a-独立验证-20261024.md` 观察项 2/3/4）
- 工作树：`af-wt-phase2-dsl` 分支 `phase2-dsl-fix2`（基线 `82b8d5a`：af-dsl 全量 + fix1 + af-compiler + golden 基线）
- 资源纪律：开工 `df -h /` 248G 空闲；构建 `CARGO_TARGET_DIR=~/afbuild-phase2d`；只动本工作树；未触 docker/网络/工厂守护进程
- 纪律：TDD 先红（12 红/6 绿）后绿再重构；golden 快照与草稿零改动（`git status` 佐证：testdata 全目录无变更）

## 1. 三缺口与实装（正本：提案 B §1.3；码表 §6 + M 系列 RESULT 总表）

| # | 缺口（验证者实测反例） | 新码 | 码位来源 | 实现落点 |
|---|---|---|---|---|
| ① | `loop.max_iterations = 0` 编译通过（u32 必填但无非正检查） | **E2002** `value_out_of_range` | 提案 §1.3「迭代上限 >0（E2002）」——af-graph 图域共享码（§6「由 B 的 DSL 面共享触发」；RESULT-M1:93 原注「M1 不触发」即留给本面） | `validate/loop_guard.rs` |
| ②a | `safety_points: []` 编译通过（只强制键存在） | **E2004** `empty_collection` | 提案 §1.3「缺安全点=拒绝编译，承 E2004」——同上共享码；af-graph `validate.rs:427-433` 同语义 | `validate/loop_guard.rs` |
| ②b | 安全点可指向非验证面节点（无 E2322 判定） | **E2322** `safety_point_not_verifier` | 提案 §6 E23xx 段已分配码位（RESULT-M3:78-89 原注「已分配未实装，实装需同步改 M1 夹具断言为 [E2312, E2322]」）；§5 inv2 夹具信息格式 `safety point 'debater' kind 'task' not in {check, gate, human}` | `validate/inv3.rs`（INV-3 族，`is_verifier` 所在） |
| ③ | `carried` 条目可两上界皆缺（`CarriedDecl` 两字段皆 Optional 无 is_bounded） | **E2002**（同①码） | 提案 §1.3「至少一个上界（E2002）」——与 E2211（carried 覆盖性）**互补不重复**：值域 vs 覆盖义务 | `validate/loop_guard.rs` |

**码位核验**：error.rs 原 39 码（E0001-E0004/E1001-E1004/E1051-E1053/E2001/E2003/E2101-E2107/E2201-E2205/E2207/E2208/E2211/E2301-E2304/E2311/E2312/E2321/E2331/E2401-E2403），新增 E2002/E2004/E2322 三码后 **42 码，无冲突无重号**（E2002/E2004 跨 crate 与 af-graph 共享属 §6 明文「码随域走」的共享族，非重号）。

**语义口径（从严面，均承 af-graph 原样）**：
- E2322 三值严格匹配：check→`kind=check` 节点、gate→`kind=gate` 节点、human→human 标记（af-graph `validate.rs:454-456` kind_ok 判定原样）。比提案 §5 信息文面的宽读法（目标∈验证面即可）更严——交叉错配（声明 `check` 指向 gate 节点）亦拒；golden 17 份全过严格口径（见 §4）。
- E2002 上界口径承 af-graph `CarriedState::is_bounded`（`model.rs:90-95`）：至少其一，`Some(0)` 视为已声明上界。
- 分域不重复定罪：安全点路径不可解析只报 E2107（refs 域，`refs.rs:253-264`），E2322 跳过不可解析目标；重复 id 节点（E2003 已报，首声明者胜）的 loop 不再守卫定罪（与 refs 同口径）。
- 错误信息含节点路径定位（如 `loop l1.inner 的 max_iterations = 0…`、`…安全点 "w"（kind=gate）指向节点 l1.w（kind="task"…）`）；scope = loop 属主所在体作用域，报告序确定（管线：refs → **loop_guard** → inv1 → inv2 → **inv3(含 E2322)** → inv4 → m4）。

## 2. 测试（TDD）

**RED**（提交 `test(af-dsl): RED fix2 …`）：`tests/loop_guard_test.rs` 18 例，实跑 12 红/6 绿（绿 = 正例与 E2107 互补例，本应绿）。反例逐码精确断言（恰一码，防无关码噪声）：

- E2002 ①：`zero_max_iterations_is_e2002`、嵌套定位 `zero_max_iterations_message_carries_loop_path`（信息须含 `l1.inner`）
- E2004 ②a：`empty_safety_points_is_e2004` + 定位断言
- E2322 ②b：`safety_point_on_task_node_is_e2322`（§5 inv2 场景）、`safety_point_kind_mismatch_is_e2322`（check→gate 严格口径）、`safety_point_human_on_plain_node_is_e2322`、定位断言
- E2002 ③：`unbounded_carried_is_e2002`（信息含 store 名）+ 定位断言
- 一次收齐：`three_guards_collected_in_one_report`（码序确定 `[E2002, E2004, E2002]`）、`two_loops_both_reported`、`unbounded_carried_and_uncovered_are_distinct_codes`（`[E2001, E2002, E2211]`——E2002/E2211 互补性实证）
- 正例零误伤：gate/check/human 三类合法安全点、`carried_bytes_only_bound_is_clean`、`dangling_safety_point_stays_e2107_only`（互补不越界）、`fully_guarded_loop_is_clean`

**GREEN**（提交 `feat(af-dsl): GREEN fix2 …`）：error.rs 三变体 + `validate/loop_guard.rs` 新模块 + `inv3.rs` E2322 + `model.rs` `SafetyPointKind::as_str`。

**既有夹具同步**（13 处，全部为「把夹具中的 loop 补成合法受控循环」——体内加最小 gate 节点 + 合法安全点，各测试原有断言不变；非弱化校验）：

- `refs_test.rs` 6 处：same_id_in_different_scopes_is_ok / loop_node_builtin_done_port_feeds_edge / carried_unknown_store_is_e2001 / from_body_node_dangling_is_e2106 / from_body_port_dangling_is_e2106 / loop_args_ref_dangling_is_e2105
- `inv1_test.rs` 1 处：arg_resolution_walks_scope_chain_outward（嵌套 map>loop）
- `inv2_test.rs` 4 处夹具 + 1 处断言：**spec_inv2 夹具断言按 RESULT-M3:89 预案由恰 `[E2312]` 改为 `[E2312, E2322]`**（该夹具 = 提案 §5 inv2 片段原样，§5 明文「E2312 + E2322」两码均触发；夹具文件本身零改动）
- `inv3_test.rs` 1 处：loop_body_check_self_loop_is_e2321（安全点指向 verifier check 节点，断言仍恰 `[E2321]`）

## 3. 文档勘误（G2a 观察项 2/3/4 → 已强制+实码）

| 文档 | 位置 | 勘误前（高报/不实） | 勘误后 |
|---|---|---|---|
| 执行语义规范-v0.md | §1.1 | 「`max_iterations > 0` 编译期已保证（缺/非正拒绝编译）」 | 已强制：缺失 → E1002；`= 0` → **E2002**（fix2 起实装；注记 G2a 观察项 2 就此闭口） |
| 执行语义规范-v0.md | §1.2 | 「必须有界（…编译期已判）」 | 已强制：**E2002** 每项至少一上界（承 af-graph is_bounded 口径；与 E2211 互补） |
| 执行语义规范-v0.md | §1.3 | 「`safety_points` 非空编译期已保证」 | 已强制：缺失 → E1002；为空 → **E2004**；指向验证面 → **E2322**（三值原样；悬空仍归 E2107） |
| 执行语义规范-v0.md | 头部版本行 | （无勘误记号） | 增补 fix2/P2-L08 勘误记号（三处高报的来龙去脉） |
| 保证边界矩阵.md | §2 L1 loop 行 | 「必填>0」「至少一上界」「缺失拒绝」中仅必填/缺失为真（观察项 3） | 全项落码 E1002/E2002/E2004/E2322 + 注记原高报与 fix2 实装 |
| 保证边界矩阵.md | §2 逃逸舱行 | 「E2210 四字段缺一拒绝」——码位不实（实际装载层 E1002 强制，G2a 实证；E2210 不在 af-dsl 码表） | 改引 **E1002**（serde 派生）+ 注明 E2210 为 §6 保留码位、语义留待后续模块（af-dsl `model.rs` escape 段自注同述） |

## 4. golden 影响（如实：零拒绝）

- **预扫**（Python/PyYAML，本会话实跑）：17 份 golden 源（11 草稿 + 6 样例）共 19 个 loop，三守卫全过——含 E2322 严格三值口径（各安全点声明 kind 与目标节点验证面逐一匹配）。
- **实跑**：`cargo test -p af-compiler` 全绿 26 例，其中 `g3_golden_corpus_byte_exact`（17 份语料重编译与 golden 快照逐字节相等）、`six_golden_samples_pass_full_validation`（六样例过含新守卫的全量校验）、`g3_golden_tamper_detection` 全过——**无任何 golden 源被新校验拒绝，快照零改动**。

## 5. 验收命令与结果（本会话实跑）

| 检查 | 命令 | 结果 |
|---|---|---|
| af-dsl 全量 | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo test -p af-dsl` | **全绿 224 通过/0 失败**（cel 10 + envelope 11 + inv1 37 + inv2 9 + inv3 10 + inv4 12 + **loop_guard 18（新增）** + m4 15 + parse 37 + refs 37 + route 18 + selector 10） |
| golden 回归 | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo test -p af-compiler` | **全绿 26 通过/0 失败**（golden 8 + inline_body 4 + ir_schema 8 + lower_error 2 + route_guard 4） |
| clippy | `CARGO_TARGET_DIR=~/afbuild-phase2d cargo clippy -p af-dsl -p af-compiler --all-targets` | **零新警告**——仅存量 1 条（`af-compiler/tests/golden_test.rs:256` doc 缩进，本任务未触碰该文件；stash 对照基线 `82b8d5a` 同为 1 条，证非新增） |
| fmt | `rustfmt --check` 逐改动文件 | 新文件（loop_guard.rs / loop_guard_test.rs）clean；其余文件的 fmt diff 均在未触碰的存量行（仓库存量债，G2a 验证观察项 1 已在案，本任务未新增） |

## 6. 遗留与边界

- E2322 严格三值口径（声明 kind 须与节点验证面原样对应）为承 af-graph 的从严面；若后续裁决取宽读法（目标∈验证面即可），改 `inv3.rs` 一处 match 即可，测试 `safety_point_kind_mismatch_is_e2322` 需同步翻转。
- E1007（内联体为空）、E1005/E1006、E2209/E2210 仍为 §6 已分配未实装码位（M1/M4 遗留，归属待裁决，本任务未动）。
- 空 loop 体（`nodes: []`）经 E2004 事实上不可再配出合法安全点（无节点可指），但 E1007 本身仍未实装——空体文档现为「无安全点即拒」，若需显式空体错误码仍待 E1007。
