(* =====================================================================
 * T06 · INV-2 completeness + 性质引理——单写者 ⇒ 无并发写冲突
 *
 * 义务编号：theorems-v1.md（定稿 v1, 2026-10-24）§A-10
 * 审计结论：通过（2026-10-24；TCB 前提处理与【矩阵】:22/:49/:50 三行
 *   核对相符）。工具栏钦定：Rocq（抽象写事件模型引理）+ Kani（静态面）
 *   ——本文件即 Rocq 侧条目。
 *
 * 原文摘录（定稿，逐字）：
 *   「①（completeness）写者闭包核对通过 ⇒ 每个 store 的写者集合恰为声明
 *    writer 单点，编译不因 E2311/E2312 拒。②（性质引理）静态单写者成立
 *    且运行时执行面按 writer 路由写请求的条件下，任意 store 无并发写冲突
 *    ——性质（无并发写冲突）由机制（单写者）在 v0.1 兑现。」
 *
 * 锚定（转引自 theorems-v1.md §A-10，行号经审计复核）：
 *   · 【矩阵】保证边界矩阵.md :22（两段引文逐字）、:50、:49（执行面 TCB
 *     行：按 writer 路由/幂等键生成等机制强制在执行面）
 *   · 【提案B】DSL-语法提案-B.md :1385
 * 对应实现位置：INV-2 静态核对（writer 闭包核对，见 T05）+ 执行面写路由
 *   机制（TCB）；撰写员未直接读取实现仓库。
 *
 * 建模说明：
 *   · ①以 writer_closure_pass 为前提（静态核对通过），结论 = 每 store
 *     写者集合恰为声明 writer 单点 且 不因 E2311/E2312 拒。「每个 store」
 *     按 A-07 字面口径（基数恰 1）；豁免细则若有，证明时对照实装核实。
 *   · ②为抽象写事件模型：WriteEvent 带 store/writer 分量，happens_before
 *     为执行面并发序；「并发」= 双向均无先后。
 *   · declared_writer_of 将「静态单写者」前提全函数化（每 store 恰一个
 *     声明 writer——①结论的静态化）。
 *   · 【TCB 显式假设】execution_routes_writes_to_declared_writer：运行时
 *     执行面按 writer 路由写请求（【矩阵】:49 执行面 TCB 行）。按
 *     theorems-v1.md §0.1 纪律：TCB 内条目只可对 TCB 成员实现建模、不可
 *     消除信任本身——此假设即显式登记，不作为待消除命题。
 *
 * 地位：只译不证。Proof. Admitted. = 待证义务登记。
 * ===================================================================== *)

Section T06_A10.

(* —— ① completeness —— *)

Variable NodeName : Type.
Variable StoreName : Type.
Variable Document : Type.

Variable NodeSet : Type.
Variable set_card : NodeSet -> nat.
Variable node_in : NodeName -> NodeSet -> Prop.

Variable writer_set : Document -> StoreName -> NodeSet.
(* 每 store 的写者集合（writes[] 解析，见 T05）。 *)

Variable declared_writer : Document -> StoreName -> option NodeName.
(* store.writer 声明。 *)

Variable CompileOutcome : Type.
Variable compile : Document -> CompileOutcome.
Variable rejected : CompileOutcome -> Prop.
Variable ErrorCode : Type.
Variable E2311 : ErrorCode.
Variable E2312 : ErrorCode.
(* E2311：写者集合违例；E2312：map 体禁写体外 store（【矩阵】:22 同族）。 *)
Variable report_contains : CompileOutcome -> ErrorCode -> Prop.

Variable writer_closure_pass : Document -> Prop.
(* 写者闭包核对通过（INV-2 静态核对：E2311 恰一 + E2312 map 体边界）。 *)

Theorem A10_writer_closure_completeness :
  forall d : Document,
    writer_closure_pass d ->
    (forall s : StoreName,
        0 < set_card (writer_set d s) ->       (* W2-C44：零写者前提 *)
        set_card (writer_set d s) = 1
        /\ exists w : NodeName,
             node_in w (writer_set d s) /\ declared_writer d s = Some w)
    /\ ~ report_contains (compile d) E2311
    /\ ~ report_contains (compile d) E2312.
Proof. Admitted. (* 待证（completeness）：闭包核对通过 ⇒ 写者集合恰为
                    声明 writer 单点，且编译不因 E2311/E2312 拒。
                    零写者 store 不在「恰一」结论取域（CONFLICTS #2/#2b
                    机器反例：闭包可通过而 card=0）——与 Kani 侧
                    a10_completeness_closure 的「每 store ≥1 写者」前提
                    同口径，RUNLOG §3 在案；红队 W2-C44 补入。 *)

(* —— ② 性质引理：抽象写事件模型 —— *)

Variable WriteEvent : Type.
(* 运行时写事件。 *)

Variable event_store : WriteEvent -> StoreName.
(* 写事件目标 store。 *)

Variable event_writer : WriteEvent -> NodeName.
(* 写事件实际执行写者（执行面路由后的主体）。 *)

Variable happens_before : WriteEvent -> WriteEvent -> Prop.
(* 执行面并发序。 *)
Hypothesis hb_transitive :
  forall e1 e2 e3 : WriteEvent,
    happens_before e1 e2 -> happens_before e2 e3 -> happens_before e1 e3.

Variable declared_writer_of : StoreName -> NodeName.
(* 静态单写者前提：每 store 恰有一个声明 writer（①结论的静态化全函数）。 *)

Variable executed : WriteEvent -> Prop.
(* 该写事件被运行时执行。 *)

Hypothesis execution_routes_writes_to_declared_writer :
  forall e : WriteEvent,
    executed e -> event_writer e = declared_writer_of (event_store e).
(* 【TCB 显式假设】（【矩阵】:49）：运行时执行面按 writer 路由写请求——
 * 写事件执行者 = 目标 store 的（唯一）声明 writer。 *)

Definition concurrent (e1 e2 : WriteEvent) : Prop :=
  ~ happens_before e1 e2 /\ ~ happens_before e2 e1.
(* 并发 = 双向均无先后。 *)

Definition concurrent_write_conflict (e1 e2 : WriteEvent) : Prop :=
  event_store e1 = event_store e2
  /\ concurrent e1 e2
  /\ event_writer e1 <> event_writer e2.
(* 并发写冲突：同 store、并发、写者不同。 *)

Theorem A10_no_concurrent_write_conflict :
  forall e1 e2 : WriteEvent,
    executed e1 -> executed e2 -> ~ concurrent_write_conflict e1 e2.
Proof. Admitted. (* 待证（性质引理）：同 store 的执行写事件均路由到唯一
                    声明 writer ⇒ 写者相同 ⇒ 冲突第三支不成立 ⇒ 任意
                    store 无并发写冲突——性质由机制在 v0.1 兑现。 *)

End T06_A10.
