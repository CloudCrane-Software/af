(* =====================================================================
 * T09 · 安全点必经与 E2004 编译期守卫逻辑
 *
 * 义务编号：theorems-v1.md（定稿 v1, 2026-10-24）§B-06
 * 审计结论：修订后通过（2026-10-24；前置句替换为 :28 逐字引文，含
 *   E1002/E2004 分码；UNCERTAIN「判定输出」端口形态按 :28 原词陈述）
 *
 * 原文摘录（定稿，逐字）：
 *   主句：「每轮被计入完成的迭代必须经过至少一个安全点节点（check/gate/
 *    human）的判定输出；未经任何安全点节点判定输出的轮次不得计入完成
 *    ——「循环受控」的运行时兑现。」
 *   前置句（:28 修订逐字，节录）：「safety_points 非空编译期已强制（缺
 *    失 → E1002 形状层拒绝；为空 → E2004 空集合拒绝；且每项必须指向验
 *    证面节点——E2322……）」
 *
 * 锚定（转引自 theorems-v1.md §B-06，行号经审计复核）：
 *   · 【规范】执行语义规范-v0.md :28（主句 + 前置句）
 * 对应实现位置：编译期守卫 = af-dsl validate 的 safety_points 非空强制
 *   （E1002/E2004 分码，:28 修订前置句；每项指向验证面节点为 E2322，
 *   属 A-13 对应性判定，见 theorems-v1.md §A-13——未入本批）；运行时面
 *   = 循环驱动器完成计入逻辑（执行面）。路径转引自 theorems-v1.md；
 *   撰写员未直接读取实现仓库。
 *
 * 建模说明：
 *   · 编译期守卫面：safety_points_declared 取 option (list SafetyPoint)；
 *     缺失 → E1002、为空 → E2004 两支分别定理化。E2322（每项指向验证面
 *     节点）属 A-13 义务，此处仅注释、不入命题。
 *   · 运行时面：round_counted_completed（被计入完成）⇒
 *     passed_safety_judgment（经过至少一个安全点节点判定输出）；原文两
 *     个分句（必须经过 / 不得计入）互为逆否，一并定理化登记。
 *   · 「判定输出」端口形态 :28 未细化（:29 对 exit_when 明说 passed/
 *     emits 端口）——按 :28 原词以抽象谓词陈述（UNCERTAIN 照草稿口径）。
 *
 * 地位：只译不证。Proof. Admitted. = 待证义务登记。
 * ===================================================================== *)

Section T09_B06.

(* —— 编译期守卫面（E2004 逻辑） —— *)

Variable Document : Type.
Variable LoopId : Type.

Variable SafetyPoint : Type.
(* safety_points 条目（点号路径）。 *)

Variable safety_points_declared : Document -> LoopId -> option (list SafetyPoint).
(* safety_points 声明；None = 缺失（E1002 形状层域）；Some [] = 为空。 *)

Variable CompileOutcome : Type.
Variable compile : Document -> CompileOutcome.
Variable rejected : CompileOutcome -> Prop.
Variable ErrorCode : Type.
Variable E1002 : ErrorCode.
Variable E2004 : ErrorCode.
(* E1002：形状层必填缺失；E2004：空集合拒绝。 *)
Variable report_contains : CompileOutcome -> ErrorCode -> Prop.

Theorem B06_safety_guard_missing_rejected :
  forall (d : Document) (l : LoopId),
    safety_points_declared d l = None ->
    rejected (compile d) /\ report_contains (compile d) E1002.
Proof. Admitted. (* 待证：safety_points 缺失 → 形状层拒绝，Report 含
                    E1002。 *)

Theorem B06_safety_guard_empty_rejected_E2004 :
  forall (d : Document) (l : LoopId),
    safety_points_declared d l = Some (@nil SafetyPoint) ->
    rejected (compile d) /\ report_contains (compile d) E2004.
Proof. Admitted. (* 待证：safety_points 为空 → E2004 空集合拒绝。 *)

(* —— 运行时必经面（:28 主句） —— *)

Variable Round : Type.
(* 一轮迭代。 *)

Variable round_counted_completed : Round -> Prop.
(* 该轮被计入完成。 *)

Variable passed_safety_judgment : Round -> Prop.
(* 本轮经过至少一个安全点节点（check/gate/human）的判定输出。 *)

Theorem B06_safety_point_mandatory :
  forall r : Round, round_counted_completed r -> passed_safety_judgment r.
Proof. Admitted. (* 待证：每轮被计入完成的迭代必须经过至少一个安全点
                    节点的判定输出（「循环受控」的运行时兑现）。 *)

Theorem B06_round_without_safety_not_counted :
  forall r : Round, ~ passed_safety_judgment r -> ~ round_counted_completed r.
Proof. Admitted. (* 待证：未经任何安全点节点判定输出的轮次不得计入完成
                    （前定理的逆否，原文两个分句照登）。 *)

End T09_B06.
