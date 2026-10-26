# Wave-1 证明批次：确定性组 9 条 + 构造语义组 Verus 条目——对应表与内核裁决

- 批次：theorems-v1 C 组（9 条）+ B 组中建议 Verus/可镜像条目
- 日期：2026-10-25（会话内实跑；套件终跑 2026-10-25T09:02:01+08:00）
- 源码基线：`/home/pan-ding/workspace/af-wt-phase2-dsl` @ `phase2-g2b`（commit e85bfdb）
- 产出目录：`计划/研究组/形式化/proofs-wave1/确定性组-构造语义-Verus/`
  - `kani-src/lib.rs`——17 个镜像穷尽 harness（Kani）
  - `kani-src/run-suite.sh`——套件运行器
  - `logs/kani-suite-final.log`——终跑逐 harness 内核输出（**pass=17 fail=0**）
  - `logs/kani-suite-first-pass.log`——首轮日志（8/17，含 4 类失败与修复依据）
  - `verus-prepared/b_loop_guard_e2002_e2004.rs`——Verus 版草稿（**未跑内核**，见 §三.5）

## 〇、一句话结论

**17/17 harness 内核 VERIFIED-SUCCESSFUL（Kani 0.68.0 / CBMC 6.11.0 终跑单次全绿），覆盖 11 条陈述**（C-01/C-02/C-03/C-07 + B-01/B-03/B-04/B-06 + B-09/B-11/B-13）；**3 条标记已完成**（B-14/B-15/C-04——FV-0.1 route::normalize 三性质 12 verified/0 errors，按指令不重证）；**C-05 部分证据**（镜像签名纯度+源码锚，全程序性质不可单 harness 全判——陈述工具注记原文即「混合证据形态」）；**C-06/C-08/C-09 受阻归因**（工具不可达/病态面，与 theorems-v1 审计修订注记一致）。

> **全库覆盖率（红队 W1-C42 补登，2026-10-26；W2-C42/C43 机算勘误后 2026-10-27 回填；计数经下方 §三.6 `coverage-check` 块机算）**：theorems-v1 共 **57 条义务**（A16/B23/C9/D9，`grep -c '^## [A-D]-'` 机算），四流合计 **40 条有内核裁决载体**（Kani INV 32 harness→A 全 16；本表 Kani 17 harness→11 条；FV-0.1→B-14/B-15/C-04；状态机组 Verus 29 verified→D 全 9；**B-16→FV-0.2 `mirror_normalize_all_gotos_exhaustive` 27-goto 分配全枚举 Kani harness 3 verified**，FV0-工具链PoC-20261024.md:79-80 + theorems-v1.md:362-368 工具栏在案——原口径误列无载体，红队 W2-C43 勘误补入；**证据工件 `proofs/kani/`（含 kani-run.log 3 verified）已于 2026-10-27 红队 W2-C-37 入仓，在仓可复核**）= **40/57（70.2%）**；其余 17 条按「部分证据（C-05）/仅陈述（B-20，Rocq T10 Admitted）/受阻（C-06/08/09）/无载体（12 条，B-12 有覆盖注记仍计无载体）」如实分类，**不以覆盖率数字冒充全量已证**——分类与清单见 §三.6。

## 一、内核裁决汇总（实跑输出，摘自 logs/kani-suite-final.log）

命令：`bash /dev/shm/wave1/run-suite.sh /dev/shm/wave1/suite-final.log`
终跑汇总行：`=== SUITE SUMMARY: pass=17 fail=0 total=17 2026-10-25T09:02:01+08:00 ===`
逐 harness（VERIFICATION:- SUCCESSFUL；耗时为内核 Verification Time）：

| # | harness | 内核裁决 | 耗时 |
|---|---|---|---|
| 1 | b01_b06_e2002_e2004_sound_and_complete | VERIFICATION:- SUCCESSFUL | 0.33s |
| 2 | b04_carried_bounded_exhaustive_flags | VERIFICATION:- SUCCESSFUL | 0.28s |
| 3 | b01_counter_bounds_and_exhaustion | VERIFICATION:- SUCCESSFUL | 0.70s |
| 4 | b01_no_new_round_after_exhaustion | VERIFICATION:- SUCCESSFUL | 0.05s |
| 5 | b03_iteration_rebuild_empty | VERIFICATION:- SUCCESSFUL | 0.04s |
| 6 | b04_truncate_respects_declared_bound | VERIFICATION:- SUCCESSFUL | 0.27s |
| 7 | b09_each_element_exactly_one_instance | VERIFICATION:- SUCCESSFUL | 0.06s |
| 8 | b13_aggregation_order_is_instance_order | VERIFICATION:- SUCCESSFUL | 0.17s |
| 9 | b11_in_flight_bounded_by_declared_fanout | VERIFICATION:- SUCCESSFUL | 0.10s |
| 10 | c03_sort_output_sorted_and_multiset_preserving | VERIFICATION:- SUCCESSFUL | 4.35s |
| 11 | c03_sort_idempotent | VERIFICATION:- SUCCESSFUL | 0.78s |
| 12 | c03_sort_permutation_invariant | VERIFICATION:- SUCCESSFUL | 17.4s |
| 13 | c02_iteration_order_is_lexicographic | VERIFICATION:- SUCCESSFUL | 143.4s |
| 14 | c07_naming_injective_and_stateless | VERIFICATION:- SUCCESSFUL | 0.05s |
| 15 | c07_entry_is_first_node_or_empty_body_error | VERIFICATION:- SUCCESSFUL | 0.02s |
| 16 | c07_graph_version_stamps_parent | VERIFICATION:- SUCCESSFUL | 0.03s |
| 17 | c01_double_run_bitwise_identical | VERIFICATION:- SUCCESSFUL | 10.2s |

## 二、条目对应表（陈述编号 → 证明物 → 锚定 → 内核裁决）

| 陈述 | 对应 harness | 实现锚定（实读源码行，2026-10-24 核对） | 内核裁决 |
|---|---|---|---|
| **C-01** 双跑逐位一致 | `c01_double_run_bitwise_identical` | lower.rs:66（`lower` 纯函数签名）、:1-4（模块自注：纯函数/无 I/O/无时钟随机网络）、:135-148、:225、:123-125 | ✅ SUCCESSFUL（ideal 对象镜像：同文档双跑三表逐位全等 + 边序对换不变 + 节点序升序 + 命名纯派生） |
| **C-02** BTree 遍历序=内容规范函数 | `c02_iteration_order_is_lexicographic` | ir.rs:54-64/:89（types/stores/node_exec/escapes/subgraphs 全 BTreeMap）、ir.rs:117（capability_refs BTreeSet）、bind.rs:147-164、route.rs:108/:140-141 | ✅ SUCCESSFUL（3 键 × 6 插入序全枚举：遍历键严格升序 + 与「键升序独立重排」逐位相等——插入序自由度消去） |
| **C-03** Vec 稳定排序规范函数性 | `c03_sort_output_sorted_and_multiset_preserving` / `c03_sort_idempotent` / `c03_sort_permutation_invariant` | lower.rs:146-148（稳定键=(from,output,to,input)）、:225（节点按 id）、:325（carried 按 store 名）、:331（safety_points 按路径）、:424（budgets 按 id） | ✅ SUCCESSFUL ×3（输出升序+多重集保持；幂等 sort∘sort=sort；对换不变 ⇒ 与构建序无关） |
| **C-05** 编译期无隐藏输入 | 部分——`c01` 镜像函数签名纯度（无 &mut 外部态）+ lower.rs:1-4 源码锚；cel_ref 摘要面（lower.rs:369-389，BTreeSet roots + SHA-256 规范 JSON）只读引用未单独 harness | 同左 | ⚠️ 部分（全程序性质，单 harness 不可全判——陈述工具注记原文「混合证据形态如实登记」） |
| **C-07** 降维命名确定性 | `c07_naming_injective_and_stateless` / `c07_entry_is_first_node_or_empty_body_error` / `c07_graph_version_stamps_parent` | lower.rs:123-125（名=图名/路径机械拼接）、:300-305、:346-351（entry=首元素，空体→EmptyBody）、:307-311（version stamp 父图） | ✅ SUCCESSFUL ×3（命名单射+双跑一致；entry=body[0]/空体 Err 不 panic；stamp 后版本=父图版本） |
| **B-01** loop 迭代边界 | `b01_b06_e2002_e2004_sound_and_complete`（编译侧 E2002）+ `b01_counter_bounds_and_exhaustion` + `b01_no_new_round_after_exhaustion`（运行侧） | loop_guard.rs:26-32（E2002：==0 → ValueOutOfRange）、【规范】:20 | ✅ SUCCESSFUL ×3（max_iterations==0 ⇔ E2002 sound+complete；iter 恒 ∈[1,max]；耗尽不开新轮、on_exhausted 收束） |
| **B-03** carried iteration 重建 | `b03_iteration_rebuild_empty` | 【规范】:24-25 | ✅ SUCCESSFUL（iteration ⇒ 轮始为空；run/persistent ⇒ 逐位保留） |
| **B-04** carried 上界判定/截断 | `b04_carried_bounded_exhaustive_flags`（编译侧）+ `b04_truncate_respects_declared_bound`（运行侧） | loop_guard.rs:40-51、af-graph model.rs:92-94（is_bounded，Some(0) 视为已声明）、lower.rs:312-325（上界声明面透传+排序）、【规范】:24 | ✅ SUCCESSFUL ×2（2³ 旗标全枚举：无界 ⇔ E2002(slot2,idx) 逐项双向往返，Some(0) 不产码；截断后 len ≤ cap 且 = min(n,c)；None 不截） |
| **B-06** 安全点必经 | `b01_b06_e2002_e2004_sound_and_complete`（编译侧 E2004）+ `b01_counter_bounds_and_exhaustion` 后件 (b)（运行侧必经） | loop_guard.rs:33-39（E2004）、【规范】:28 | ✅ SUCCESSFUL（sp_len==0 ⇔ E2004；计入完成数 = 已开跑轮次中经判定数——必经性的可判定形态） |
| **B-09** map 定基数与空扇出 | `b09_each_element_exactly_one_instance` | lower.rs:344-358（map_spec 透传）、【规范】:37-38 | ✅ SUCCESSFUL（实例数=N；实例 i=元素 i 下标双射；N=0 ⇒ 空实例表） |
| **B-11** map 并发上限 | `b11_in_flight_bounded_by_declared_fanout` | ir.rs:231-238（max_fanout: Option<u32>）、【规范】:39（已声明方向） | ✅ SUCCESSFUL（逐波 assert 在飞 ≤ max_fanout + 峰值复核；发射总数=n） |
| **B-13** map 聚合序确定性 | `b13_aggregation_order_is_instance_order` | 【规范】:40（实例序非完成序） | ✅ SUCCESSFUL（4 实例 24 排列全枚举：聚合[i]=实例 i 产出，与到达/完成序无关） |
| **C-04** route 归一化确定性 | **已完成（FV-0.1 T3 确定序）**——按批次指令不重证 | FV0-工具链PoC-20261024.md §1.2 | FV-0.1：12 verified / 0 errors |
| **B-14** route 互斥 | **已完成（FV-0.1 T1）** | 同上 | FV-0.1：12 verified / 0 errors |
| **B-15** route 覆盖性恰一 | **已完成（FV-0.1 T2）** | 同上 | FV-0.1：12 verified / 0 errors |

## 三、未完成/受阻条目与归因（如实）

1. **C-06（CEL 编译一次不折叠、运行时同源求值）**：predicate 通道含 cel-interpreter，即 FV-0 实测 kani-compiler 0.68.0 ICE 源（FV0 报告 §2.3 阻断一）；本批次无新版 Kani（代理关闭全程，0.66 对照 FV-0 亦未跑成）。直证受阻——与陈述工具修订注记一致，非新信息。
2. **C-08（校验序固定 ⇒ Report 确定）**：陈述工具修订已判双跑 harness 受阻（validate 链 ICE + format! 串 CBMC 病态面）；串域 Report 镜像失真度高，本批次未镜像——如实不做。
3. **C-09（拒绝式信封/unknown_kinds）**：envelope.rs:14 入参 serde_yaml::Value，FV-0 实测 >4 分钟不收敛（阻断二）；未做。
4. **C-05**：部分证据（见 §二），未列全 verified——如实。
5. **Verus 路线（陈述对 B-01/B-04/B-06/C-02/C-03 的首选工具）**：安装未成。40+1 次代理下载全败（curl exit 35，SSL wrong version number；FV-0 §2.3 预记的「SSL 窗口性失败」本批次全程未开窗）。`verus-prepared/b_loop_guard_e2002_e2004.rs` 为已写未跑的 Verus 镜像草稿（B-01/B-04/B-06 编译+运行侧，含无界不变式形态），**未获任何内核裁决**，不计入 verified。本轮内核证据全部为 Kani 有界域穷尽（域声明见 lib.rs 头注），无界归纳版留给 Verus 窗口开启后的后续批次——分层如实。
6. **B 组未证条目（红队 W1-C42 勘误 2026-10-26——原写「不在义务清单」系错误措辞：B-02/B-05/B-07/B-08/B-10/B-12/B-16..B-23 共 14 条全部是 theorems-v1 §B 组正式义务（`grep -c '^## B-'` = 23），应为「**在义务清单、本批无证明载体**」**）：B-12 的聚合完整面已由 B-09 发射总数=n + B-13 双射前提覆盖注记；B-20 已有 Rocq T10_envelope血缘包装.v 陈述（Admitted，「只译不证」，非证明）；其余 12 条（B-02/B-05/B-07/B-08/B-10/B-12/B-17/B-18/B-19/B-21/B-22/B-23）四流均无证明载体——**B-16 已于 2026-10-27 红队 W2-C42/C43 机算勘误中移入有载体**（FV-0.2 27-goto 分配全枚举 Kani harness 3 verified，证据链接见 §〇；B-12 的覆盖注记不构成内核裁决，仍计无载体）。全库 57 条义务的覆盖率与分类如下（清单由脚本对 theorems-v1 条目头机算闭合校验：40+17=57——有载体 40（含 B-16）+部分 1+仅陈述 1+受阻 3+无载体 12、类间无交、并集=全集；2026-10-27 复算通过）：

```coverage-check
分母: theorems-v1.md ^## 条目头 = 57 (A16/B23/C9/D9)
covered-kani-inv: A-01..A-16
covered-kani-17: B-01 B-03 B-04 B-06 B-09 B-11 B-13 C-01 C-02 C-03 C-07
covered-fv01: B-14 B-15 C-04
covered-verus-d: D-01..D-09
covered-fv02-goto-enum: B-16
partial: C-05
statement-admitted: B-20
blocked: C-06 C-08 C-09
no-carrier: B-02 B-05 B-07 B-08 B-10 B-12 B-17 B-18 B-19 B-21 B-22 B-23
```

（复算命令：`grep -c '^## A-'|^## B-'|^## C-'|^## D-' theorems-v1.md` 得 16/23/9/9；四流内核裁决源 = Kani镜像员-INV组/RUNLOG.md:27「32 successfully verified harnesses」、本表 §一「pass=17 fail=0」、FV0-工具链PoC-20261024.md §1.2（FV-0.1 12 verified）、状态机组-Verus/RUN-SUMMARY.md §1「29 verified, 0 errors」。）

## 四、镜像↔实现对账（诚实边界）

- **零堆镜像**：全 harness 固定数组+显式长度。实测依据：堆 Vec + 符号 push 探针 9.0s/369 checks（含分配噪声），本库首版 Vec harness 单个 >4 分钟不收敛——与 FV-0 §2.3「瓶颈为数据结构/串机器，非 Kani 本身」一致，故取 FV-0 已验证的整数/数组对照组形态。
- **首轮失败与修复**（logs/kani-suite-first-pass.log，内核反例如实记录）：①数组/元组/串 `==` 编译为 memcmp 耗尽 unwind（6 例）→ 手工逐位比较/离散错误码；②B-06 必经断言形态错误（把计数轮误当前缀轮，内核反例 j[0]=false,completed=1）→ 改「计数=判定求和」等式；③B-13 非双射到达未前置排除（内核反例漏落位）→ 加两两相异前提；④C-07 镜像哨兵槽撞 name=0（内核反例）→ 加非空名域假设（与 lower.rs:123-125 拼接名恒非空一致）。四类均为**镜像/harness 缺陷**，无一构成对源码行为或陈述的否定；无陈述-现实冲突需上报。
- **对账残余**：镜像逐函数行号锚已写入 lib.rs 注释；FV-0 §1.2 建议的「镜像↔实现」对账单测（真实 route.rs/loop_guard.rs 关键分支 diff）仍未建，为后续批次义务。

## 五、复现

```sh
# 前提：Kani 0.68.0 于 /dev/shm/fv0/kani-0.68.0（tmpfs，重启即失；重装按 FV-0 §2.1）
bash <产出目录>/kani-src/run-suite.sh /tmp/suite.log   # → SUITE-DONE pass=17 fail=0
# 单 harness：
cd <kani-proj> && PATH=/dev/shm/fv0/kani-0.68.0/bin:$PATH cargo kani --harness <名>
```

磁盘/沙盒纪律：工具与构建全在 tmpfs（/dev/shm/wave1），af-dsl/af-compiler 工作树零写入（仅读），产物唯一落 proofs-wave1/ 本目录。
