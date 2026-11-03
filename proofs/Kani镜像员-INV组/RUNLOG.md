# RUNLOG · INV 组 16 条 Kani 镜像穷尽 harness（wave1）

- 执行者：Kani镜像员-INV组（dynamic-workflow 子代理）
- 日期：2026-10-24 深夜 → 2026-10-25（跨零点，验证运行在 25 日 08:4x-08:5x）
- 义务清单：`计划/研究组/形式化/theorems-v1.md` A 组 16 条（A-01..A-16，只读正本）
- 模式：FV-0.2 已验证镜像法（`计划/研究组/FV0-工具链PoC-20261024.md` §2）——纯逻辑层镜像 + 穷尽 harness，**不** harness af-dsl validate 全链（FV-0 阻断一 ICE 规避）

## 1. 环境（实况）

| 项 | 值 | 取证命令 |
|---|---|---|
| Kani | 0.68.0 standalone（tmpfs 保留件） | `kani --version` → `Kani Rust Verifier 0.68.0 (standalone)` |
| CBMC | 6.11.0（CaDiCaL 3.0.0） | 运行日志首行 |
| 工具链 | nightly-2026-08-21（/dev/shm/fv0/rustup + bundle toolchain/） | `cat /dev/shm/fv0/kani-0.68.0/rust-toolchain-version` |
| 目标码 | af-wt-phase2-dsl @ phase2-g2b（只读引用，0 行改动） | `git branch --show-current` → `phase2-g2b` |
| 沙盒 | `~/.cache/kani-mirror-inv/`（铁律③） | — |
| 构建产物 | `~/afbuild-kani-inv/`（磁盘，编排者指示；曾用 /dev/shm/kani-inv-target 已弃用） | — |
| 磁盘开工读数 | `/` 242G 可用（46% 用）；/dev/shm 4.9G 可用 | `df -h`（开工首查，铁律④） |

## 2. 命令（实跑，逐条）

```sh
# 全量默认集（32 harness，一次编译全验证）
export PATH=/dev/shm/fv0/kani-0.68.0/bin:$PATH
export CARGO_TARGET_DIR=~/afbuild-kani-inv
cd ~/.cache/kani-mirror-inv && cargo kani
# → exit=0；Complete - 32 successfully verified harnesses, 0 failures, 32 total.
#   总墙钟 28.6s（编译 ~11s + 验证合计 17.2s）；逐 harness 日志见 logs/kani-full-run.log.gz
# （交付前复认终跑：同命令同结果 32/32、exit=0，日志 /dev/shm/kani-final.log——结果摘要与上run逐字节同判）

# 冲突实证（字面陈述反例，预期 FAILURE）
cargo kani --features conflict-demo --harness a03_literal_statement_conflict
# → exit=1；Complete - 0 successfully verified harnesses, 1 failures, 1 total.
#   FAILURE：conflict_demo.rs:29 「A-03 字面陈述: llm 声明 reads 必含 E2303」（logs/a03_…log）
cargo kani --features conflict-demo --harness a07_literal_statement_conflict
# → exit=1；Complete - 0 successfully verified harnesses, 1 failures, 1 total.
#   FAILURE：conflict_demo.rs:49 「A-07 字面陈述: 写者基数 0（≠1）必含 E2311」（logs/a07_…log）
```

## 3. 逐 harness 裁决（Kani 内核输出，唯一裁判——铁律②）

32/32 VERIFICATION:- SUCCESSFUL。逐条时间（`logs/kani-full-run-results.txt` 全文）：

| harness | 定理·方向 | 裁决 | 验证用时 |
|---|---|---|---|
| a01_soundness | A-01 soundness | SUCCESS | 0.339s |
| a01_completeness | A-01 completeness | SUCCESS | 0.247s |
| a02_soundness | A-02 soundness（含 policy:none 例外半边） | SUCCESS | 0.092s |
| a02_completeness | A-02 completeness | SUCCESS | 0.081s |
| a03_soundness | A-03 soundness（前提：reads 含已声明 store，见 CONFLICTS #1） | SUCCESS | 0.045s |
| a03_completeness | A-03 completeness | SUCCESS | 0.066s |
| a04_soundness | A-04 soundness（零引用方向 + 实装多引同码） | SUCCESS | 0.678s |
| a04_completeness | A-04 completeness | SUCCESS | 0.677s |
| a05_range_soundness | A-05 范围 soundness | SUCCESS | 0.399s |
| a05_existence_non_llm | A-05 存在性（含对照组两侧钉住） | SUCCESS | 0.099s |
| a06_completeness | A-06 completeness | SUCCESS | 1.028s |
| a06_soundness_counterpart | A-06 配对（逐子句违例必报） | SUCCESS | 0.969s |
| a07_soundness | A-07 soundness（前提：cnt≥1，见 CONFLICTS #2） | SUCCESS | 0.446s |
| a07_completeness | A-07 completeness | SUCCESS | 0.410s |
| a08_soundness | A-08 soundness（含两码独立性 sanity） | SUCCESS | 0.598s |
| a08_completeness | A-08 completeness | SUCCESS | 0.368s |
| a09_soundness | A-09 soundness（含前缀过滤 sanity） | SUCCESS | 0.322s |
| a09_completeness | A-09 反向 completeness | SUCCESS | 0.171s |
| a10_completeness_closure | A-10① completeness（前提：每 store ≥1 写者，见 CONFLICTS #2 同根） | SUCCESS | 0.338s |
| a10_no_conflict_lemma | A-10② 性质引理（TCB 前提显式） | SUCCESS | 0.096s |
| a11_soundness | A-11 soundness | SUCCESS | 1.409s |
| a11_completeness | A-11 补方向（自环仅在非验证面 ⇒ 无 E2321） | SUCCESS | 1.266s |
| a12_range_soundness | A-12 范围 soundness | SUCCESS | 0.925s |
| a12_existence_indirect_ok | A-12 存在性（间接回环通过 + 对照） | SUCCESS | 0.063s |
| a13_completeness_e2321 | A-13① completeness | SUCCESS | 0.947s |
| a13_soundness_e2322 | A-13② 对应性双向 | SUCCESS | 0.935s |
| a14_soundness | A-14 soundness | SUCCESS | 1.082s |
| a14_completeness | A-14 completeness | SUCCESS | 0.883s |
| a15_completeness | A-15 completeness | SUCCESS | 0.626s |
| a15_whitelist_members_never_flagged | A-15 白名单逐点 | SUCCESS | 0.326s |
| a16_decl_surface_basis | A-16① 声明面基据 | SUCCESS | 0.643s |
| a16_oob_independence | A-16② 视野外通道独立性 | SUCCESS | 0.657s |

冲突实证 harness（不在默认集，feature `conflict-demo` 门控）：

| harness | 断言（陈述字面） | 裁决 | 归属 |
|---|---|---|---|
| a03_literal_statement_conflict | llm 声明 reads ⇒ 含 E2303 | **FAILURE（反例实证）** | CONFLICTS.md #1 |
| a07_literal_statement_conflict | 写者基数 0（≠1）⇒ 含 E2311 | **FAILURE（反例实证）** | CONFLICTS.md #2 |

## 4. 域尺寸与运行时（如实记录，铁律「域尺寸与运行时如实」）

穷尽方式：kani::any() 符号位 + assume 有界 + assert —— CBMC 对声明的有界域全域判定（SAT 全域裁决，非抽样）；每 harness 另含手构具体角点 sanity assert（非空洞性守护，防前提不可满足导致空真）。

| 镜像组 | 域（符号位） | 域基数（判定有效投影） | 最重 harness 用时 |
|---|---|---|---|
| INV-1 节点（A-01..A-06） | llm1 + has_port2 + reads2 + n_secs(0..=2) + 每段 from(0..7)×12 位 ×2 段 | 单段 7×2^12=28672 描述符；2 段节点 ≈ 2^33 位域 | 1.03s |
| INV-2 写者闭包（A-07/08/10） | 每节点写目标 3 值 ×3 节点 + in_map 3 位 + 每 store 声明 4 值 ×2 | 3^3 × 2^3 × 4^2 = 27×8×16 = 3456 | 0.60s |
| INV-2 carried（A-09） | in_body 3 + 写目标 3^3 + carried 2 | 2^3×3^3×2^2 = 864 | 0.32s |
| INV-3 图（A-11..13①） | kind 4 值×3 + human 3 + 邻接 9 位 | 4^3×2^3×2^9 = 262144 | 1.41s |
| INV-3 安全点（A-13②） | sp 条目 (kind 3 × target 3 × resolved 2)×2 + 节点描述 | ≤ (3×3×2)^2 × (4·2)^3 | 0.94s |
| INV-4（A-14..A-16） | reads/sec_ref 各 6 位 + oob 6 位 + kind 2×3 + unit 3 + eval 2 + writer 2×2 | 2^26 位域 | 1.08s |

小域口径核对：节点/图对象 3 个、store/端口槽 2 个、段数 ≤2、CEL 域以真值位抽象（承 FV-0「when→赋值」抽象，语义保距）。**单 harness 最大用时 1.41s ≪ 10min 降域阈值**——无降域、无 pending（运行时维度）。

## 5. 事件记录（如实）

1. **首跑前 CBMC 病态修复**：初版镜像/harness 用了 `.find()/.filter()/.any()` 迭代子组合器，CBMC 符号执行下病态展开（日志见 `Unwinding loop … iteration 989`）——按 FV-0 阻断二同款教训全部改显式 for 循环 + 布尔累加器后，全量 28.6s 收敛。镜像工程纪律由此固化为注释（各 mirror 文件头）。
2. **首跑 26/32**：6 失败归因三类——①镜像保真度 bug（check_node_mirror 的 E2304 支漏写 `llm_bound` 门，实装 inv1.rs:142 有门；已修，映射表 M-07 注记该次实修）；②两处 completeness harness 断言极性写反（`legal ||` 应为 `!legal ||`；已修）；③A-10① 零写者角落复现 CONFLICTS #2 同根分歧（按冲突处理加前提，见 CONFLICTS.md）。修后 32/32。
3. **会话 shell 失效一次**（FV-0 §0 同款第二例）：全量首跑期间后台 cargo kani 求解早段 RAM+swap 压力尖峰致 shell fork/exec 失败约 10 分钟；编排者核实（内存 available 9.7Gi、残留进程已自然终结、/dev/shm 2.4G/7.5G）后恢复。构建目录随即从 tmpfs 迁至磁盘 `~/afbuild-kani-inv`（编排者硬纪律）。已按编排者要求登记，变更日志归编排者。
4. **求解全程串行**（铁律④）：全部 cargo kani 调用逐条顺序执行，无并行 solver。

## 6. 残余义务（如实，不属本批次）

- **镜像↔实现对账单测**（FV-0 §1.2 即建议的通用残余）：本批次一致性证据 = 逐分支行号锚定（MAPPING.md）+ 真实实现自身的测试套（inv1..4_test.rs 在 af-dsl tests/ 未动）+ 抑制角落的机器实证；「镜像与实现行为逐例 diff」未做，列 FV-1。
- A-16 运行时半边：陈述自认不可立（执行面读权限白名单层占位，【矩阵】:57），无 harness 义务——未做，非失败。
- A-10② 抽象引理的 Rocq 主证：本批次只交 Kani 静态面镜像（TCB 前提显式化）；Rocq 引理不在本批工具面。
