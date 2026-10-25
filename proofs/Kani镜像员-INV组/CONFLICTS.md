# CONFLICTS · 陈述与代码现实分歧清单（INV 组批次）

- 纪律：theorems-v1 定稿陈述**原样照证**（铁律①）——发现陈述与实现现实冲突时：不改陈述、不改源码、冲突实证入册。本清单即该产出。
- 实证形态：conflict-demo harness 以陈述【字面】含义为断言，Kani/CBMC 判 FAILURE 并给出具体反例——机器证据，非人工评断。日志：`logs/a03_literal_statement_conflict.log`、`logs/a07_literal_statement_conflict.log`。
- 性质声明：以下均为**角落分歧**（陈述字面全域 vs 实装跨层抑制角落），不是陈述主体被证伪；主体方向（soundness 核心）在实装一致域内 32/32 VERIFIED。是否修订陈述属审计员/正本维护者职权，本清单只供其裁决。

## #1 · A-03：「LLM 节点声明 reads ⇒ Report 含 E2303」在「reads 全部未声明」角落与实装不符

- 陈述（theorems-v1 A-03 定稿，=草稿原文）：「任何文档若存在 LLM-bound 节点声明 `reads`，则编译被拒绝且 Report 含 E2303」。
- 实现：`crates/af-dsl/src/validate/inv1.rs:104-115`——E2303 只对 reads 中【已声明】store 判；引用未声明 store 的项由 refs 层 E2001 定罪，本层不产 E2303（模块注记 inv1.rs:21-24「不重复定罪」纪律）。
- 机器实证：conflict_demo.rs:29 断言（字面陈述）FAILURE——反例 = llm_bound ∧ reads 非空 ∧ 引用 store 全部未声明 ⇒ 实装无 E2303（`Complete - 0 successfully verified harnesses, 1 failures`）。
- 影响面：文档在该角落**仍被拒绝**（E2001），「编译被拒绝」半边成立；「Report 含 E2303」半边不成立。
- 处置：A-03 soundness harness 前提 = 「reads 含已声明 store」（实装自述前提，非本批臆造）→ VERIFIED；字面全域反例由 conflict-demo 机器钉住。
- 附注：提案B:112 reads 行词面「LLM 节点声明即 E2303」同样强于实装角落；审计栏未登记该角落（审计补证聚焦 A-04 多引方向）。v0.2 勘误可考虑在码表/规则句注记抑制口径。

## #2 · A-07：「写者集合基数 ≠ 1 ⇒ Report 含 E2311」在零写者角落与实装不符

- 陈述（theorems-v1 A-07 定稿，=草稿原文）：「任何文档若存在某 store 的写者集合……基数 ≠ 1，或 ≠ 声明的 writer，则编译被拒绝且 Report 含 E2311」。基数 0 ≠ 1 在字面域内。
- 实现：`crates/af-dsl/src/validate/inv2.rs:5-6` 模块注记明文「**零写者不报**：非串行化违例——声明值与 writes 的一致性归 E2xxx 后续模块」+ :42-52 判定条件（`len > 1 || (len == 1 && paths[0] != declared)`）确实不含 len==0。
- 机器实证：conflict_demo.rs:49 断言（字面陈述）FAILURE——反例 = 无任何节点写该 store（基数 0）∧ 声明 writer 已解析 ⇒ 实装无 E2311。
- 影响面：零写者文档是否被其他码拒绝属「后续模块」（:6 注记），镜像不判；确定的是 **E2311 不在 Report**。
- 处置：A-07 soundness/completeness harness 前提 = cnt ≥ 1 → VERIFIED；字面全域反例机器钉住。

## #2b · A-10① 同根复现

- A-10①（写者闭包核对通过 ⇒ 每 store 写者集合恰为声明 writer 单点）：零写者 store 上「核对通过」（无 E2311/E2312，按实装口径）但写者集合 = ∅ ≠ 声明单点——与 #2 同根经 A-10① 前提取自 A-07 结论域而复现。
- 处置：a10_completeness_closure 前提显式加「每 store ≥1 实际写者」（镜像文件内注记指向本清单 #2）→ VERIFIED。

## 域注记（非冲突，跨层抑制的显式域限制，与实现一一对应）

1. **A-08/A-07/A-14 writer 悬空**：inv2.rs:19-22、inv4.rs:57-59 对 writer 路径悬空的 store 整体跳过（refs 层 E2001 已定罪）。镜像以 DANGLING 哨兵把跳过支纳入域内并按实装断言（悬空 ⇒ 本层无码）；A-08/A-14 陈述主体预设声明 writer 可指认，取已解析子域不构成陈述分歧。
2. **A-09 carried 只收已声明 store**：inv2.rs:63 过滤；镜像域内两 store 恒已声明（域注记在镜像文件头）。
3. **A-01 词表外 from**：inv1.rs:333 跳过支（E1004 域）——已纳入镜像域（from 值 5、6），非限制。
4. **A-02 字段缺失**：陈述修订已把「缺失」划归形状层（E1002 域），镜像域不含缺字段位——与定稿陈述一致，非分歧。
5. **A-16 运行时半边**：陈述自认「运行时半边 v0 不可立」（执行面读权限白名单层占位，【矩阵】:57）——无 harness 义务，非分歧。
6. **INV-1 总量上限**：inv1.rs:78-100 可配置 cap 路径默认关（:12 注记），不在镜像域——A-02 陈述不含总量面，非分歧。

## 验收口径回收

「≥10 条拿到 VERIFIED-SUCCESS 或如实 pending 归因」：16/16 条全部有 VERIFIED-SUCCESS harness（各含 soundness/completeness 双向；A-05/A-12/A-16 为范围/存在性/独立性形态的对应双向）——无运行时 pending。上述 #1/#2 为陈述字面角落分歧的如实登记，已按「冲突清单是合法产出」条款处理，不影响 VERIFIED 判定的前提如实性（前提全部来自实装自述并逐条锚定行号）。
