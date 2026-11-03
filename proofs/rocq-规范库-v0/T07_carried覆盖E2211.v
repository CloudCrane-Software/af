(* =====================================================================
 * T07 · INV-2 soundness + 反向 completeness——carried 覆盖（E2211）
 *        （绑定完备性代位条目，见下方说明）
 *
 * 义务编号：theorems-v1.md（定稿 v1, 2026-10-24）§A-09
 * 审计结论：通过（2026-10-24）
 *
 * 修订记录 v0.1（2026-10-25 · 保真审计后修订）：
 *   修订依据：保真审计条目【T07_carried覆盖E2211.v · wrong · 必修】——
 *     A-09 completeness 半边前提被误域收窄为单 loop。义务原文
 *     （theorems-v1.md §A-09，:153）：「……反向：carried 覆盖核对通过
 *     ⇒ 不因 E2211 拒。」——「覆盖核对通过」是文档级核对（全部 loop）；
 *     修订前定理只对单个 loop l 取覆盖前提却下文档级结论（¬report
 *     E2211），在多 loop 文档（l1 未覆盖、l2 覆盖）上与 soundness 半边
 *     （l1 缺口 ⇒ report E2211）联合矛盾，义务不可满足（结论过强）。
 *   修订处：completeness 半边前提改为文档级谓词 carried_coverage_check_
 *     pass（∀ loop 的覆盖全称式）；soundness 半边
 *     （A09_carried_coverage_soundness：覆盖缺口 ⇒ 拒 + E2211）语义不动
 *     （审计判「忠实」）。
 *
 * 原文摘录（定稿，逐字）：
 *   「任何文档若 loop 的 carried[] 未覆盖「体内（含嵌套体）写入的、体外
 *    声明的全部 store」，则编译被拒绝且 Report 含 E2211；反向：carried
 *    覆盖核对通过 ⇒ 不因 E2211 拒。」
 *
 * 锚定（转引自 theorems-v1.md §A-09，行号经审计复核）：
 *   · 【提案B】DSL-语法提案-B.md :1385（句末）、:1569（E2211 码位）
 *   · 【规范】执行语义规范-v0.md :27（逐字）
 *   · 【矩阵】保证边界矩阵.md :32（carried 上界行）
 * 对应实现位置：INV-2 carried 覆盖核对（编译期引用闭包面；E2211 与
 *   E2208/E2002 为不同声明面——【提案B】:1569 与 B-04 码位澄清）。
 *   路径转引自 theorems-v1.md；撰写员未直接读取实现仓库。
 *
 * 代位说明（绑定完备性）：任务包优先面提到「绑定完备性 E900x」，但
 *   theorems-v1.md 全文（57 条）不含任何 E9xx 码位（用码范围 E1002–
 *   E2403）。按「以文件内标注为准」，本条取 A-09 为绑定完备性代表——
 *   carried[] 对体外声明、体内写入 store 的覆盖绑定，且原文自带双向
 *   （soundness + completeness），是最贴合「完备性」义务形态的条目。
 *
 * 建模说明：
 *   · 三集合抽象：loop_body_writes（体内含嵌套体写入）、outer_declared
 *     （体外声明）、carried（carried[] 声明）；成员谓词 store_in。
 *   · 「未覆盖」= 存在体内写入 ∧ 体外声明 ∧ 不在 carried 的 store
 *     （覆盖性结论的全称式的存在性反演）。
 *   · 反向定理前提 = 覆盖核对通过（文档级全称式：全部 loop × 全部
 *     store，修订 v0.1 由单 loop 收窄修复为文档级），结论 = 不因 E2211 拒。
 *
 * 地位：只译不证。Proof. Admitted. = 待证义务登记。
 * ===================================================================== *)

Section T07_A09.

Variable StoreName : Type.
Variable LoopId : Type.
Variable Document : Type.

Variable StoreSet : Type.
Variable store_in : StoreName -> StoreSet -> Prop.

Variable loop_body_writes : Document -> LoopId -> StoreSet.
(* loop 体内（含嵌套体）写入的 store 集合（点号路径解析后）。 *)

Variable outer_declared : Document -> LoopId -> StoreSet.
(* 体外声明的 store 集合。 *)

Variable carried : Document -> LoopId -> StoreSet.
(* loop 的 carried[] 声明集合。 *)

Variable CompileOutcome : Type.
Variable compile : Document -> CompileOutcome.
Variable rejected : CompileOutcome -> Prop.
Variable ErrorCode : Type.
Variable E2211 : ErrorCode.
(* 稳定码 E2211（码表【提案B】:1569）。 *)
Variable report_contains : CompileOutcome -> ErrorCode -> Prop.

Definition carried_uncovered (d : Document) (l : LoopId) : Prop :=
  exists s : StoreName,
    store_in s (loop_body_writes d l)
    /\ store_in s (outer_declared d l)
    /\ ~ store_in s (carried d l).
(* carried[] 未覆盖：存在体内写入的、体外声明的、未列入 carried 的 store。 *)

Theorem A09_carried_coverage_soundness :
  forall (d : Document) (l : LoopId),
    carried_uncovered d l ->
    rejected (compile d) /\ report_contains (compile d) E2211.
Proof. Admitted. (* 待证（soundness）：覆盖缺口 ⇒ 编译拒绝且 Report 含
                    E2211。 *)

Definition carried_coverage_check_pass (d : Document) : Prop :=
  forall l : LoopId,
    forall s : StoreName,
      store_in s (loop_body_writes d l) ->
      store_in s (outer_declared d l) ->
      store_in s (carried d l).
(* carried 覆盖核对通过（文档级：文档内全部 loop 的 carried[] 均覆盖其
   体内（含嵌套体）写入的、体外声明的全部 store）——修订 v0.1 新增，
   对应义务「反向：carried 覆盖核对通过 ⇒ 不因 E2211 拒」的文档级口径。 *)

Theorem A09_carried_coverage_completeness :
  forall d : Document,
    carried_coverage_check_pass d ->
    ~ report_contains (compile d) E2211.
Proof. Admitted. (* 待证（反向 completeness）：carried 覆盖核对通过
                    （文档级全 loop）⇒ 不因 E2211 拒。 *)

End T07_A09.
