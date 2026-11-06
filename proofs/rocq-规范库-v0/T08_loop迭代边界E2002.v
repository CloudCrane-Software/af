(* =====================================================================
 * T08 · loop 迭代边界（计数器不变量）与 E2002 编译期守卫逻辑
 *
 * 义务编号：theorems-v1.md（定稿 v1, 2026-10-24）§B-01
 * 审计结论：修订后通过（2026-10-24；前置句替换为 :20 逐字引文，含
 *   E1002/E2002 分码；审计亲核 loop_guard.rs:26-32 实装 E2002（==0，u64））
 *
 * 修订记录 v0.1（2026-10-25 · 保真审计后修订）：
 *   修订依据：保真审计条目【T08_loop迭代边界E2002.v · weakened】——运行
 *     时契约只登记了「+1 ⇒ 完成轮」单向（step_increments_only_on_
 *     round_completion），缺「计数器每步至多 +1、否则不变」对侧：可构
 *     造跳步驱动器（iter 1→3、max=2）满足修订前全部 Hypothesis 而违反
 *     iter ∈ [1, max_iterations]（跳步下「+1 ⇒ 完成轮」「预算门」两契约
 *     前件均为假、空虚满足），B01_iter_counter_invariant 在修订前前提
 *     集下不可证。
 *   修订处：补登记对侧契约 step_at_most_one（iter ≤ iter' ≤ S iter）；
 *     义务原文（theorems-v1.md §B-01，:229）「每完成一轮迭代（子图
 *     entry 可达的全部节点终态化）计数器才 +1」口径不变——两契约合成
 *     该句完整运行时口径（增量 ⇒ 完成轮 ∧ 每步增量 ∈ {0, +1}）。
 *
 * 原文摘录（定稿，逐字）：
 *   主句：「运行时循环驱动器维护单调递增迭代计数器 iter，恒有 iter ∈
 *    [1, max_iterations]（max_iterations > 0 由编译期保证）；每完成一轮
 *    迭代（子图 entry 可达的全部节点终态化）计数器才 +1；iter 达
 *    max_iterations 且 exit_when 未命中 ⇒ 不再开启新一轮，按 on_exhausted
 *    收束。」
 *   前置句（:20 修订逐字，节录）：「max_iterations > 0 编译期已强制（缺
 *    失 → E1002 形状层拒绝；= 0 → E2002 值域拒绝——fix2/P2-L08 起实装，
 *    af-dsl validate/loop_guard.rs；……）」
 *
 * 锚定（转引自 theorems-v1.md §B-01，行号经审计复核）：
 *   · 【规范】执行语义规范-v0.md :20（主句 + 前置句）、
 *   · 【矩阵】保证边界矩阵.md :32
 * 对应实现位置：编译期守卫 = af-dsl validate/loop_guard.rs:26-32（E2002
 *   ==0，u64；缺失归 E1002 形状层域）；运行时面 = 循环驱动器（执行面）。
 *   路径转引自 theorems-v1.md §B-01 锚定与审计标记；撰写员未直接读取
 *   实现仓库。
 *
 * 建模说明：
 *   · 编译期守卫面：max_iterations_declared 取 option nat（u64 建模为
 *     nat）；缺失 → E1002、= 0 → E2002 两支分别定理化（原文分码口径）。
 *   · 运行时面：驱动器为单步转移系统 driver_step（None = 不再开启新一轮）；
 *     驱动器局部契约（单调、只按完成轮 +1、每步至多 +1 否则不变、预算
 *     门、max 恒定、max > 0）登记为 Hypothesis——义务 = 由契约归纳出全
 *     局不变量与耗尽收束。
 *   · 「按 on_exhausted 收束」的收束分支在模型中以 driver_step = None
 *     （不再开启新一轮）承载；on_exhausted 三值语义属 B-02（未入本批）。
 *   · 工具口径：theorems-v1.md 建议 Verus（驱动器状态机不变量）；本文件
 *     为 Rocq 陈述化（第四档可选引擎）。
 *
 * 地位：只译不证。Proof. Admitted. = 待证义务登记。
 * ===================================================================== *)

Section T08_B01.

(* —— 编译期守卫面（E2002 逻辑） —— *)

Variable Document : Type.
Variable LoopId : Type.

Variable max_iterations_declared : Document -> LoopId -> option nat.
(* max_iterations 声明；None = 缺失（E1002 形状层域）；u64 建模为 nat。 *)

Variable CompileOutcome : Type.
Variable compile : Document -> CompileOutcome.
Variable rejected : CompileOutcome -> Prop.
Variable ErrorCode : Type.
Variable E1002 : ErrorCode.
Variable E2002 : ErrorCode.
(* E1002：形状层必填缺失；E2002：值域拒绝（=0）。 *)
Variable report_contains : CompileOutcome -> ErrorCode -> Prop.

Theorem B01_loop_guard_missing_rejected :
  forall (d : Document) (l : LoopId),
    max_iterations_declared d l = None ->
    rejected (compile d) /\ report_contains (compile d) E1002.
Proof. Admitted. (* 待证：max_iterations 缺失 → 形状层拒绝，Report 含
                    E1002（E1002 归形状层域，口径见 :20 修订前置句）。 *)

Theorem B01_loop_guard_zero_rejected_E2002 :
  forall (d : Document) (l : LoopId),
    max_iterations_declared d l = Some 0 ->
    rejected (compile d) /\ report_contains (compile d) E2002.
Proof. Admitted. (* 待证：max_iterations = 0 → E2002 值域拒绝
                    （loop_guard.rs:26-32 实装口径 ==0，u64）。 *)

(* —— 运行时计数器不变量面（:20 主句） —— *)

Variable DriverState : Type.
(* 循环驱动器运行态。 *)

Variable iter_of : DriverState -> nat.
(* 迭代计数器 iter（单调递增）。 *)

Variable max_iterations_of : DriverState -> nat.
(* 该循环的 max_iterations 上界。 *)

Variable round_completed : DriverState -> Prop.
(* 子图 entry 可达的全部节点终态化 = 一轮迭代完成。 *)

Variable exit_when_hit : DriverState -> Prop.
(* exit_when 命中（求值为真）。 *)

Variable driver_step : DriverState -> option DriverState.
(* 驱动器单步转移；None = 不再开启新一轮（循环收束）。 *)

Variable initial_state : DriverState.
Variable reachable : DriverState -> Prop.
(* 初态与可达性。 *)

Hypothesis initial_reachable : reachable initial_state.
Hypothesis initial_iter : iter_of initial_state = 1.
Hypothesis reachable_cases :
  forall r : DriverState,
    reachable r ->
    r = initial_state \/ exists p : DriverState, reachable p /\ driver_step p = Some r.
(* 驱动器状态域归纳基础。 *)

Hypothesis step_iter_monotone :
  forall r r' : DriverState,
    driver_step r = Some r' -> iter_of r <= iter_of r'.
(* 计数器单调（不回退）。 *)

Hypothesis step_increments_only_on_round_completion :
  forall r r' : DriverState,
    driver_step r = Some r' -> iter_of r' = S (iter_of r) -> round_completed r.
(* 每完成一轮迭代计数器才 +1（增量必以完成轮为前提）。 *)

Hypothesis step_at_most_one :
  forall r r' : DriverState,
    driver_step r = Some r' ->
    iter_of r <= iter_of r' /\ iter_of r' <= S (iter_of r).
(* 对侧契约：计数器每步至多 +1、否则不变（iter ≤ iter' ≤ S iter）——
   与 step_increments_only_on_round_completion 合成「每完成一轮迭代……
   计数器才 +1」的完整运行时口径。修订 v0.1 补登记：无此对侧则跳步驱
   动器（iter 1→3）满足其余全部契约而违反 iter ∈ [1, max_iterations]。 *)

Hypothesis step_new_round_within_budget :
  forall r r' : DriverState,
    driver_step r = Some r' -> iter_of r' = S (iter_of r) ->
    iter_of r < max_iterations_of r.
(* 预算门：仅当 iter < max 才开启新一轮（计数器 +1）。 *)

Hypothesis max_iterations_constant :
  forall r r' : DriverState,
    driver_step r = Some r' -> max_iterations_of r' = max_iterations_of r.
(* 上界沿驱动器运行恒定。 *)

Hypothesis max_iterations_positive : forall r : DriverState, 0 < max_iterations_of r.
(* max_iterations > 0 由编译期保证（E1002/E2002 守卫兑现后的运行时前提，
 * 见上文守卫面两定理）。 *)

Theorem B01_iter_counter_invariant :
  forall r : DriverState,
    reachable r -> 1 <= iter_of r /\ iter_of r <= max_iterations_of r.
Proof. Admitted. (* 待证：恒有 iter ∈ [1, max_iterations]——初态 iter=1，
                    沿可达链归纳：步增量 ∈ {0, +1}（step_at_most_one），
                    +1 分支被预算门（step_new_round_within_budget）夹住上
                    界，max 沿链恒定（max_iterations_constant）。 *)

Theorem B01_no_new_round_on_exhaustion :
  forall r : DriverState,
    reachable r ->
    iter_of r = max_iterations_of r ->
    ~ exit_when_hit r ->
    ~ (exists r' : DriverState, driver_step r = Some r' /\ iter_of r' = S (iter_of r)).
Proof. Admitted. (* 待证：iter 达 max 且 exit_when 未命中 ⇒ 不再开启新
                    一轮（预算门逆否）；按 on_exhausted 收束由 driver_step
                    停开新轮承载（三值收束语义属 B-02）。 *)

End T08_B01.
