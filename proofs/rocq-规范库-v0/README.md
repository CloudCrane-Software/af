# rocq-规范库 v0（T01–T10 · 陈述化草案）

- 生成：2026-10-24，规范库撰写员（动态工作流子代理）
- 上游：`../theorems-v1.md`（定理陈述库定稿 v1：57 条义务 = 41 通过 + 16 修订后通过 + 0 拒绝；地位 = **只译不证**）
- 纪律：
  - 每份 `.v` 只写陈述不写证明——`Proof. Admitted.` 即待证义务登记；
  - 基本对象与前提用 `Variable`/`Hypothesis`（Section 内）声明，注释标明对应实现面概念；
  - **只依赖 Coq 标准库 prelude，零 `Require`/`Import`**（`Permutation` 在 T02 本地重述，不引 List 模块）；
  - 陈述逐字忠实于 theorems-v1.md 的定稿文本（16 条修订条目以修订后表述为准，含 B-15 补互斥前提、B-01/B-06 的 E1002/E2002/E2004 分码前置句）；
  - 文件头注释 = 义务编号 + 原文摘录（定稿逐字）+ 锚定与对应实现位置（转引自 theorems-v1.md，其行号经 2026-10-24 审计复核；撰写员未直接读取 af-dsl/af-compiler/af-workorder 实现仓库）。

## 一、选条理由

任务包指定 5 个优先面 → 在 theorems-v1.md 文件内标注中的落点：

| 优先面（任务包） | 文件内落点 | 入选条目 |
|---|---|---|
| 确定性/双跑逐位一致及其分解 | C-01（核心）+ C-02/C-03/C-04/C-05（标注「C-01 分解」）+ C-08 | **C-01、C-03、C-04**（3 条） |
| INV-2 单写者性质 | A-07（soundness 恰一）、A-09（carried 覆盖）、A-10（completeness + 并发引理，工具栏**钦定 Rocq**） | **A-07、A-10、A-09**（3 条） |
| loop 终止守卫 E2002/E2004 逻辑 | B-01（=0 → E2002 + 计数器不变量）、B-06（为空 → E2004 + 安全点必经） | **B-01、B-06**（2 条） |
| envelope 类型化交接不丢失 | B-20（E2201 两端相等 + 血缘三位，忠实性 TCB） | **B-20**（1 条） |
| 绑定完备性 E900x | **theorems-v1.md 全文无 E900x 码位**（57 条用码在 E1002–E2403） | 按任务包「以文件内标注为准」：取 **A-09**（carried 对体外声明、体内写入 store 的覆盖绑定，原文自带双向 soundness/completeness）为绑定完备性代位条目 |

另：B-14「route 互斥性（由构造成立）」在文件内标注为**「最高优先验证对象」**（全语言唯一条件边产生点的正确性论证，【提案B】:245），且 B-15 修订版陈述自含「两两不相交（B-14）」前提——故 T04 将 B-14 定理化（构造性归纳）与 B-15 定理化（恰一命中）一并覆盖 route 纯逻辑面。

**未入选（次批候选）及理由**：

- **C-02**（BTree 遍历序）：文件工具修订自注「类型级断言 = 工程审计（非 Kani 判定）」「容器结构性质本身信任标准库」——陈述化后内容大部分退化为 TCB 假设组合，Coq 收益低于 C-03/C-04；
- **C-05**（编译期无隐藏输入）：全程序性质，文件自注「单 harness 不可全判」，混合证据形态，纯陈述化保真度低；
- **C-08**（Report 确定性）：弱陈述 + 工具受阻记录（validate 链 ICE、format! 串病态）；
- **B-05**（E2211 ⇒ 运行时无未登记体外写，传递引理）：以编译器正确性为 TCB 前提，宜与 A-09 证明同战役处理；
- **B-08**（goto:exit 裁决语义）/ **B-14 已并入 T04** / **B-18**（幂等键单射性）：route-裁决与执行面建模条目，次批；
- **D-01～D-09**（状态机组）：有限域穷举对象（文件口径：Kani 理想面、Verus 表断言），Rocq 陈述化价值次之；
- **A 组其余 / B-02~B-04/B-07/B-09~B-13/B-16~B-17/B-19/B-21~B-23 / C-06/C-07**：非任务包优先面或以 Kani/Verus 为建议主工具。

## 二、文件↔义务对照表

| 文件 | 义务（theorems-v1.md） | 审计结论 | 陈述核心 | 关键登记假设/前提 |
|---|---|---|---|---|
| `T01_双跑逐位一致.v` | §C-01 | 修订后通过 | ∀ 文档 ∀ 隐藏状态：IR 与计划对象序列化逐字节相同（双跑 = 任意两次运行） | lower 建模为可读 HiddenState 的函数；逐字节 = 序列化逐位置相等 |
| `T02_Vec稳定排序.v` | §C-03 | 通过 | Permutation + 键唯一 ⇒ 排序输出 = 输入多重集的规范函数（与构建序无关） | 键唯一（nodup_keys）显式入前提；排序契约（置换 + 有序）与字典序性质为 Hypothesis |
| `T03_route归一化互定.v` | §C-04 | 通过 | 纯函数性（不依赖隐藏状态）+ 注入性，两半合取 =「一一互定」 | 相等取归一化产物结构相等；注入性依赖归一化不商掉信息 |
| `T04_route守卫恰一命中.v` | §B-14 + §B-15 | B-14/B-15 均修订后通过 | B-14：归一化守卫（when_i ∧ ¬前缀析取）两两不交，由构造归纳；B-15：互斥 ∧ 恒真 ⇒ 恰一命中 | 守卫语义代数（sat 与 ¬/∨/∧/⊥ 对应）= CEL 求值通道契约 |
| `T05_INV2写者集合恰一.v` | §A-07 | 通过 | 写者集合基数 ≠ 1 ∨ ≠ 声明 writer ⇒ 编译拒绝 + Report 含 E2311 | writer_set = writes[] 解析集合抽象；declared_writer 取 option |
| `T06_单写者无并发写冲突.v` | §A-10 | 通过 | ①闭包通过 ⇒ 恰单点且不因 E2311/E2312 拒；②执行写事件无并发写冲突（性质由机制兑现） | ②「按 writer 路由」为显式 TCB 假设（【矩阵】:49） |
| `T07_carried覆盖E2211.v` | §A-09 | 通过 | 覆盖缺口 ⇒ 拒 + E2211；覆盖通过 ⇒ 不因 E2211 拒（双向） | 三集合（体内写/体外声明/carried）抽象；绑定完备性代位（E900x 缺席） |
| `T08_loop迭代边界E2002.v` | §B-01 | 修订后通过 | 守卫面：缺失→E1002、=0→E2002；运行时面：iter ∈ [1, max_iterations] 恒成立、耗尽且 exit_when 未命中 ⇒ 不开新轮 | 驱动器单步契约（单调/只按完成轮 +1/预算门/max 恒定且 >0）为 Hypothesis |
| `T09_安全点必经E2004.v` | §B-06 | 修订后通过 | 守卫面：缺失→E1002、空→E2004；运行时面：计入完成 ⇒ 经安全点判定（及其逆否） | 「判定输出」按 :28 原词抽象（UNCERTAIN 照草稿口径）；E2322 属 A-13 未入本批 |
| `T10_envelope血缘包装.v` | §B-20 | 通过 | E2201：envelope 边两端类型不等 ⇒ 拒；TCB 前提下交付信封三血缘位 = 声明面真值（不丢失） | 「包装忠实性」为显式 TCB 假设（【矩阵】:30）；编译期只核类型不验证内容 |

## 三、Admitted 即待证义务清单（21 条定理，FV-0.4+ 证明战役排队）

| 文件 | 定理 | 待证内容 |
|---|---|---|
| T01 | `C01_double_run_bit_identical` | 管线产物与 HiddenState 无关 ⇒ 双跑 IR/计划对象逐字节相同 |
| T02 | `C03_output_canonical_function_of_multiset` | 同多重集 + 键唯一 ⇒ 排序输出唯一确定 |
| T03 | `C04_normalize_pure_rewrite` | 归一化输出不依赖隐藏状态（无搜索/图分析/枚举） |
| T03 | `C04_normalize_mutually_determined` | 归一化输出唯一决定（规则列表，文档序）——注入性 |
| T04 | `B14_normalized_guards_pairwise_disjoint` | ¬ 前缀展开构造的守卫两两不交（对规则表归纳，无需图分析） |
| T04 | `B15_route_exactly_one_hit` | 互斥 ∧ 恒真 ⇒ 每条出边恰被一个守卫命中 |
| T05 | `A07_inv2_writer_set_exactly_one_soundness` | 写者集合违例 ⇒ 拒绝 + E2311 |
| T06 | `A10_writer_closure_completeness` | 闭包通过 ⇒ 恰单点 + 不因 E2311/E2312 拒 |
| T06 | `A10_no_concurrent_write_conflict` | 路由 TCB 前提下任意 store 无并发写冲突 |
| T07 | `A09_carried_coverage_soundness` | 覆盖缺口 ⇒ 拒绝 + E2211 |
| T07 | `A09_carried_coverage_completeness` | 覆盖通过 ⇒ 不因 E2211 拒 |
| T08 | `B01_loop_guard_missing_rejected` | max_iterations 缺失 ⇒ 拒绝 + E1002（形状层域） |
| T08 | `B01_loop_guard_zero_rejected_E2002` | max_iterations = 0 ⇒ 拒绝 + E2002（loop_guard.rs 口径） |
| T08 | `B01_iter_counter_invariant` | 可达态恒有 iter ∈ [1, max_iterations]（契约归纳） |
| T08 | `B01_no_new_round_on_exhaustion` | 耗尽且 exit_when 未命中 ⇒ 不再开新轮 |
| T09 | `B06_safety_guard_missing_rejected` | safety_points 缺失 ⇒ 拒绝 + E1002 |
| T09 | `B06_safety_guard_empty_rejected_E2004` | safety_points 为空 ⇒ 拒绝 + E2004 |
| T09 | `B06_safety_point_mandatory` / `B06_round_without_safety_not_counted` | 计入完成 ⇒ 经安全点判定（互为逆否，照原文两分句） |
| T10 | `B20_envelope_type_checked_E2201` | envelope 边两端类型不等 ⇒ 拒绝 + E2201 |
| T10 | `B20_envelope_handoff_lineage_complete` | TCB 前提下交付信封三血缘位 = 声明面真值 |

> 说明：各文件用 `Section … End …` 包裹，`Admitted` 定理在 `End` 后被 Section 变量/假设泛化——编译产物中的定理类型 = 陈述 + 全部登记前提，属预期形态。

## 四、显式登记的 TCB / 建模假设（诚实边界汇总）

按 theorems-v1.md §0.1：**TCB 内条目只可对 TCB 成员实现建模、不可消除信任本身**——以下 Hypothesis 即此口径的显式登记：

1. **T06 `execution_routes_writes_to_declared_writer`**：运行时执行面按 writer 路由写请求（【矩阵】:49 执行面 TCB 行；A-10 档位栏明文「显式假设」）；
2. **T10 `wrapper_faithful_tcb`**：包装器填值 = 声明面真值（【矩阵】:30「包装忠实性在 TCB——编译期只核类型不验证包装内容」）；
3. **T02 键唯一前提**（`nodup_keys` 入定理前提）：稳定排序在重复键处不满足多重集规范函数性；nodes/edges/stores/policies id、carried store 名在 DSL 内唯一为实现面事实，建议在规范或证明战役中显式化；
4. **T02/T04 语义契约**：字典序性质、稳定排序契约（置换 + 有序）、守卫代数与 sat 的对应（CEL 求值通道）；
5. **T08/T09 驱动器与守卫局部契约**：Rocq 侧待证内容 = 「契约 ⇒ 全局不变量」的归纳；契约本身的机器核验属 Verus/Kani 面（文件工具栏口径）；
6. **T03 注入性**：依赖归一化不商掉信息（如不删除恒假守卫边）——若实装存在此类归一化，义务须按实装修订。

## 五、类型检查命令

**本库撰写时未运行 coqc（按任务包指令，类型检查由统一脚本执行）；以下命令供脚本使用：**

```bash
cd 计划/研究组/形式化/rocq-规范库-v0
for f in T*.v; do echo "== $f"; coqc "$f" || exit 1; done
```

- 单文件：`coqc T01_双跑逐位一致.v`（各文件相互独立、零 Require，可任意顺序/并行编译）；
- 依赖：仅 Coq 标准库 prelude；本机实测 **Coq 8.20.1**；`Admitted` 定理登记为公理（`Print Assumptions` 可见），属预期（待证登记），不是错误；
- 语法稳健性说明（2026-10-25 门禁修复）：list 记法「::」依赖 `list_scope`，本机 coqc 8.20.1 的 prelude 未默认开启（最小复现 `Check (1 :: 2 :: nil).` → `Error: Unknown interpretation for notation "_ :: _".`）；T02/T04 已全部改用显式 `cons` 构造子/构造子模式（`cons` 即 `::`，语义等价，不弱化义务），修复后在交付目录就地验证 `for f in T*.v; do coqc "$f"; done` → 10× exit=0；
- **文件名格式说明（探测记录，2026-10-24）**：任务包模板为 `T01-<主题>.v`（连字符）。实测 coqc 8.20.1 **拒绝连字符**：
  ```
  $ printf 'Parameter x : nat.\n' > T01-test-dash.v && coqc T01-test-dash.v
  Error: Invalid character '-' in identifier "T01-test-dash".
  ```
  故本库采用 `T01_<主题>.v`（连字符 → 下划线，主题保留中文）。中文主题实测可编译（`T01_双跑.v` exit=0）。若统一脚本按 `T01-*` 通配，请改按 `T01_*` 或逐文件列举。

## 六、边界与未尽事项

- **E900x**：theorems-v1.md 全文（57 条）不含 E9xx 码位；「绑定完备性」以 A-09 代位（见 §一）。若 E900x 出自其他正本，需补选条目。
- **实现仓库未读**：本库全部锚定与实现位置转引自 theorems-v1.md（其行号经该次独立审计逐字核对）；撰写员未直接 Read af-dsl/af-compiler/af-workorder 源码，实现面描述以 theorems-v1.md 审计标记为准。
- **类型检查未由撰写员执行**（任务包指令：由统一脚本统一跑，撰写员跑会重复）；撰写员仅以 /tmp 临时文件做了文件名兼容性探测（§五），未触及本库任何 `.v` 文件。
- 次批候选（见 §一未入选清单）；B-14 已并入 T04，独立成卷时可按 T04 的 B-14 定理拆出。

## 七、修订记录 v0.1（2026-10-25 · 保真审计后修订）

保真审计（2026-10-25）判定 10 文件中 3 处失真（1 wrong + 2 weakened），逐处修订；**其余 7 文件一字未动**。每处修订依据（审计条目 + 义务原文）已同步记入对应 `.v` 文件头注释的「修订记录 v0.1」块。

### 1. 三处修订依据（义务原文以 `../theorems-v1.md` 为准）

| 文件 | 审计判定 | 失真点 | 修订处 |
|---|---|---|---|
| `T07_carried覆盖E2211.v` | **wrong · 必修** | A-09 completeness 半边前提被误域收窄为单 loop：义务（theorems-v1.md:153）「反向：carried 覆盖核对通过 ⇒ 不因 E2211 拒」是**文档级全 loop** 口径；修订前定理只对单个 loop l 取覆盖前提却下文档级结论（¬report E2211）——多 loop 文档（l1 未覆盖、l2 覆盖）上与 soundness 半边联合矛盾，义务不可满足（结论过强） | completeness 半边前提改为文档级谓词 `carried_coverage_check_pass`（= ∀ loop 覆盖全称式，透明 Definition）；soundness 半边（`A09_carried_coverage_soundness`）语义不动（审计判「忠实」） |
| `T04_route守卫恰一命中.v` | **weakened** | B-14 陈述只含 `normalized_rule_guards` 两两不交，丢义务明文「**含 default 守卫**」（theorems-v1.md:346：「对规则 i≠j（含 default 守卫 guard_d = ¬(when_1 ∨ … ∨ when_n)），guard_i ∧ guard_j ≡ false」）与 B-15 交叉引用「全部守卫（含 default）两两不相交」（:355）所要求的**规则↔default 守卫对**——`default_guard_of` 已建模却未入定理 | ①B-14 定理改在 `all_guards_with_default`（归一化规则守卫 + default 守卫）上陈述两两不交；②B-15 的 Admitted 注记与修订后陈述对齐（B-14 修订后确提供含 default 的互斥前提）；B-15 定理陈述本体不动 |
| `T08_loop迭代边界E2002.v` | **weakened** | 运行时契约只登记「+1 ⇒ 完成轮」单向（`step_increments_only_on_round_completion`），缺「计数器**每步至多 +1、否则不变**」对侧——跳步驱动器（iter 1→3、max=2）满足修订前全部 Hypothesis 而违反 iter ∈ [1, max_iterations]（跳步使「+1」两契约前件为假、空虚满足），`B01_iter_counter_invariant` 在修订前前提集下不可证 | 补登记对侧契约 `step_at_most_one`（iter ≤ iter' ≤ S iter）；义务原文（:229）「每完成一轮迭代（子图 entry 可达的全部节点终态化）计数器才 +1」口径不变——两契约合成该句完整运行时口径 |

修订后三文件已经修订员 coqc 语法自查（2026-10-25，本机 coqc 8.20.1，命令 `for f in T04*.v T07*.v T08*.v; do coqc "$f"; done` → 3× exit=0），产物（.vo/.glob/.aux 及本机另生成的 .vok/.vos）已全部清理。

### 2. 两点说明

1. **首轮类型检查门 0/10**：系编排者门构造 bug（coqc `-o` 逻辑名不一致），与库内容无关；编排者复跑 **10/10 全过**。
2. **「绑定完备性 E900x」代位**：该优先面在 theorems-v1.md 全文（57 条）无码位（本库用码范围 E1002–E2403），已按 **A-09**（carried 覆盖 E2211，soundness + completeness 双向）代位（见 §一）；**正本补码位后需回看**。

## 修订记录补注（2026-10-25，编排者终验）

- 修订批报告的「类型门 7/10」系**假红**：修订员返回的 file 字段含目录前缀，编排者门脚本再前置 DIR_ABS 造成双拼路径，coqc 对不存在文件报非零。编排者以正确单一路径复跑全量：**10/10 通过**（T04/T07/T08 修订版含内）。
- 本库终态：v0.1，10 义务陈述、10/10 coqc 通过、10/10 保真 faithful（首轮 7 + 修订 3 全部 faithful）。证明本体待后续证明战役。
