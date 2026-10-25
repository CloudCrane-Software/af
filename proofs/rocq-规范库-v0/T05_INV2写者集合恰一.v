(* =====================================================================
 * T05 · INV-2 soundness——写者集合恰一（E2311）
 *
 * 义务编号：theorems-v1.md（定稿 v1, 2026-10-24）§A-07
 * 审计结论：通过（2026-10-24）
 *
 * 原文摘录（定稿，逐字）：
 *   「任何文档若存在某 store 的写者集合（全部节点 writes[] 点号路径解析
 *    后，与 store.writer 声明交叉核对）基数 ≠ 1，或 ≠ 声明的 writer，则
 *    编译被拒绝且 Report 含 E2311。」
 *
 * 锚定（转引自 theorems-v1.md §A-07，行号经审计复核）：
 *   · 【提案B】DSL-语法提案-B.md :1385（逐字）、:1387、:1570（E2311 码位）
 *   · 【矩阵】保证边界矩阵.md :22
 *   · 【G1裁决】G1-放行裁决-20261024.md :12（19 自构样例实证）
 * 对应实现位置：INV-2 判定函数（inv2 判定 harness；FV-0 已备 pending.rs
 *   模板，启用以 theorems-v1.md §0.3 工具链前置为前提——INV-2 链在
 *   Kani 0.68.0 不可跑）。路径与工具状态转引自 theorems-v1.md；撰写员
 *   未直接读取实现仓库。
 *
 * 建模说明：
 *   · writer_set：全部节点 writes[] 点号路径解析后的写者集合（抽象有限集，
 *     基数 card + 成员谓词 node_in）。
 *   · declared_writer：store.writer 声明，取 option——None = 未声明 writer
 *     （此时任何写者均 ≠ 声明）。
 *   · 违例谓词 = 原文两个析取支的字面翻译：基数 ≠ 1，或（单点情形下）
 *     该写者 ≠ 声明 writer。
 *   · 「每个 store」的取域按 A-07 字面口径（基数恰 1）；若有「无写者
 *     store 豁免」等细则，证明时须对照实装判定函数核实（登记于 README）。
 *
 * 地位：只译不证。Proof. Admitted. = 待证义务登记。
 * ===================================================================== *)

Section T05_A07.

Variable NodeName : Type.
Variable StoreName : Type.
Variable Document : Type.

Variable NodeSet : Type.
(* 有限节点集合抽象。 *)

Variable set_card : NodeSet -> nat.
(* 集合基数。 *)

Variable node_in : NodeName -> NodeSet -> Prop.
(* 集合成员。 *)

Variable writer_set : Document -> StoreName -> NodeSet.
(* 某文档某 store 的写者集合：全部节点 writes[] 点号路径解析后、与
 * store.writer 声明交叉核对的产物。 *)

Variable declared_writer : Document -> StoreName -> option NodeName.
(* store.writer 声明；None = 未声明 writer。 *)

Variable CompileOutcome : Type.
Variable compile : Document -> CompileOutcome.
(* 编译入口与结果。 *)

Variable rejected : CompileOutcome -> Prop.
(* 编译被拒绝。 *)

Variable ErrorCode : Type.
Variable E2311 : ErrorCode.
(* 稳定码 E2311（码表【提案B】:1570）。 *)

Variable report_contains : CompileOutcome -> ErrorCode -> Prop.
(* Report 含某稳定码。 *)

Definition inv2_writer_set_violation (d : Document) (s : StoreName) : Prop :=
  set_card (writer_set d s) <> 1
  \/ exists n : NodeName,
       node_in n (writer_set d s) /\ declared_writer d s <> Some n.
(* 原文两析取支：基数 ≠ 1，或 ≠ 声明的 writer。 *)

Theorem A07_inv2_writer_set_exactly_one_soundness :
  forall (d : Document) (s : StoreName),
    inv2_writer_set_violation d s ->
    rejected (compile d) /\ report_contains (compile d) E2311.
Proof. Admitted. (* 待证：写者集合违例 ⇒ 编译拒绝且 Report 含 E2311
                    （soundness：违例必拒）。 *)

End T05_A07.
