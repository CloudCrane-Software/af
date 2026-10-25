(* =====================================================================
 * T04 · route 互斥性（由构造成立）与覆盖性（恰一命中）
 *
 * 义务编号：theorems-v1.md（定稿 v1, 2026-10-24）§B-14 + §B-15
 * 审计结论：B-14 修订后通过（工具建议修订：Verus 优先、Rocq 可选）；
 *   B-15 修订后通过（修订要点：补「两两不相交（B-14）」前提——恒真只给
 *   ≥1，恰一需互斥）。本文件以 B-15 为主条目，B-14 定理化为其前提。
 *   B-14 在文件内标注为「最高优先验证对象」（全语言唯一条件边产生点的
 *   正确性论证）。
 *
 * 修订记录 v0.1（2026-10-25 · 保真审计后修订）：
 *   修订依据：保真审计条目【T04_route守卫恰一命中.v · weakened】——B-14
 *     陈述只含 normalized_rule_guards 两两不交，丢了义务明文「含 default
 *     守卫」（theorems-v1.md §B-14，:346：「对规则 i≠j（含 default 守卫
 *     guard_d = ¬(when_1 ∨ … ∨ when_n)），guard_i ∧ guard_j ≡ false」）
 *     与 B-15 交叉引用「全部守卫（含 default）两两不相交」（:355）所要
 *     求的规则↔default 守卫对——default_guard_of 已建模却未入定理。
 *   修订处：①B-14 定理改在 all_guards_with_default（归一化规则守卫 +
 *     default 守卫）上陈述两两不交，规则↔default 对补进互斥陈述；②B-15
 *     的 Admitted 注记与修订后陈述对齐（B-14 修订后确提供含 default 的
 *     互斥前提）；B-15 定理陈述本体不动（其前提本就是对任意守卫表的
 *     全称式，代入即实例化）。
 *
 * 原文摘录（定稿，逐字）：
 *   B-14：「归一化后节点任意两条不同出边的守卫逻辑不相交：对规则 i≠j
 *    （含 default 守卫 guard_d = ¬(when_1 ∨ … ∨ when_n)），guard_i ∧
 *    guard_j ≡ false——由构造成立（第 i 条显式排除前 i-1 条），无需任何
 *    图分析。」
 *   B-15：「归一化后全部守卫（含 default）两两不相交（B-14）且析取恒真：
 *    guard_1 ∨ … ∨ guard_n ∨ guard_d ≡ true ⇒ 路由节点每条出边恰被一个
 *    守卫命中——不存在零投递或双投递；执行面不得再做去重/仲裁逻辑（守
 *    卫已互斥，再仲裁是语义重复）。」
 *
 * 锚定（转引自 theorems-v1.md §B-14/§B-15，行号经审计复核）：
 *   · 【提案B】DSL-语法提案-B.md :235/:236/:245（B-14）、:237/:245（B-15）
 *   · 【规范】执行语义规范-v0.md :48
 *   · 【矩阵】保证边界矩阵.md :34
 *   · 【G1裁决】G1-放行裁决-20261024.md :18（goto:exit 语义裁决读法）
 * 对应实现位置：route 归一化的守卫生成（route.rs ¬ 前缀展开公式，见
 *   T03 文件头所引 route.rs:1-12）；撰写员未直接读取实现仓库。
 *
 * 建模说明：
 *   · Guard 为 CEL 谓词抽象，sat g v = 守卫 g 在求值取值 v 下成立；
 *     ¬/∨/∧ 组合子与 sat 的对应按求值通道语义契约登记为 Hypothesis。
 *   · 归一化守卫由构造建模：with_prefix 逐条生成 when_i ∧ ¬(前缀析取)
 *     （第 i 条显式排除前 i-1 条）；default 守卫 = ¬(when_1 ∨ … ∨ when_n)。
 *   · B-14 定理由该构造归纳证明（互斥不需要任何图分析——原文口径），
 *     陈述域 = all_guards_with_default（规则守卫 + default 守卫——修订
 *     v0.1：义务「对规则 i≠j（含 default 守卫 guard_d …）」的守卫全域）。
 *   · B-15 按修订版陈述结构：以「两两不相交（B-14）」+「析取恒真」为
 *     前提 ⇒ 恰一命中（全称量化的一般形式，代入归一化守卫+default 即
 *     实例化）。
 *   · 「执行面不得再做去重/仲裁逻辑」为实现义务（规范句），以注释登记，
 *     不入命题。
 *   · list 一律用显式 cons 构造子/构造子模式：记法「::」依赖 list_scope
 *     （本机 coqc 8.20.1 的 prelude 未默认开启该 scope），显式 cons 语义
 *     等价、不依赖 scope 状态。
 *
 * 地位：只译不证。Proof. Admitted. = 待证义务登记。
 * ===================================================================== *)

Section T04_B15.

Variable Valuation : Type.
(* 守卫求值取值（CEL 运行期输入上下文）。 *)

Variable Guard : Type.
(* 守卫（CEL 谓词抽象）。 *)

Variable sat : Guard -> Valuation -> Prop.
(* 守卫语义：在取值下成立。 *)

Variable gfalse : Guard.
Variable gnot : Guard -> Guard.
Variable gor : Guard -> Guard -> Guard.
Variable gand : Guard -> Guard -> Guard.
(* 守卫代数：恒假、¬、∨、∧。 *)

Hypothesis sat_gfalse : forall v : Valuation, ~ sat gfalse v.
Hypothesis sat_not : forall (g : Guard) (v : Valuation), sat (gnot g) v <-> ~ sat g v.
Hypothesis sat_or :
  forall (g1 g2 : Guard) (v : Valuation), sat (gor g1 g2) v <-> sat g1 v \/ sat g2 v.
Hypothesis sat_and :
  forall (g1 g2 : Guard) (v : Valuation), sat (gand g1 g2) v <-> sat g1 v /\ sat g2 v.
(* 求值通道语义契约（CEL eval 的组合子对应；TCB 登记面）。 *)

Definition disjoint (g1 g2 : Guard) : Prop :=
  forall v : Valuation, ~ (sat g1 v /\ sat g2 v).
(* 守卫逻辑不相交（guard_i ∧ guard_j ≡ false）。 *)

Fixpoint member_guard (g : Guard) (gs : list Guard) : Prop :=
  match gs with
  | nil => False
  | cons g' gs' => g = g' \/ member_guard g gs'
  end.

Variable RuleWhen : Type.
(* route 规则的 when 守卫（文档序条目）。 *)

Variable guard_of_when : RuleWhen -> Guard.

Fixpoint all_when_disjunction (ws : list RuleWhen) : Guard :=
  match ws with
  | nil => gfalse
  | cons w ws' => gor (guard_of_when w) (all_when_disjunction ws')
  end.
(* when_1 ∨ … ∨ when_n。 *)

Fixpoint with_prefix (p : Guard) (ws : list RuleWhen) : list Guard :=
  match ws with
  | nil => nil
  | cons w ws' =>
      cons (gand (guard_of_when w) (gnot p))
           (with_prefix (gor p (guard_of_when w)) ws')
  end.
(* 归一化构造：第 i 条显式排除前 i-1 条——逐条生成 when_i ∧ ¬(前缀析取)。 *)

Definition normalized_rule_guards (ws : list RuleWhen) : list Guard :=
  with_prefix gfalse ws.
(* 归一化后各规则出边的守卫列表（文档序）。 *)

Definition default_guard_of (ws : list RuleWhen) : Guard :=
  gnot (all_when_disjunction ws).
(* default 守卫 guard_d = ¬(when_1 ∨ … ∨ when_n)（【提案B】:245）。 *)

Definition all_guards_with_default (ws : list RuleWhen) : list Guard :=
  app (normalized_rule_guards ws) (cons (default_guard_of ws) nil).
(* 全部守卫（含 default）= 归一化规则守卫 + default 守卫 guard_d——
   B-14 义务「对规则 i≠j（含 default 守卫 guard_d …）」与 B-15「全部守卫
   （含 default）」的守卫全域（修订 v0.1 新增）。 *)

(* —— B-14：归一化守卫（含 default）两两不交（由构造成立） —— *)

Theorem B14_normalized_guards_pairwise_disjoint :
  forall (ws : list RuleWhen) (g1 g2 : Guard),
    member_guard g1 (all_guards_with_default ws) ->
    member_guard g2 (all_guards_with_default ws) ->
    g1 <> g2 ->
    disjoint g1 g2.
Proof. Admitted. (* 待证：对 ws 归纳（前缀一般化）——规则↔规则对：首条
                    含 ¬p 与 when，后续每条含 ¬(p ∨ when)，二者 conjunct
                    下矛盾；规则↔default 对：规则 i 含 when_i ∧ ¬(前缀析
                    取)，default = ¬(when_1 ∨ … ∨ when_n)，when_i 与 ¬全
                    析取 conjunct 下矛盾 ⇒ 互斥；无需任何图分析（原文口
                    径「由构造成立」）。 *)

(* —— B-15：两两不交 ∧ 析取恒真 ⇒ 恰一命中 —— *)

Definition exactly_one_sat (gs : list Guard) (v : Valuation) : Prop :=
  exists g : Guard,
    member_guard g gs /\ sat g v
    /\ forall g' : Guard, member_guard g' gs -> sat g' v -> g' = g.
(* 恰一守卫命中（零投递/双投递均不存在）。 *)

Theorem B15_route_exactly_one_hit :
  forall (gs : list Guard) (v : Valuation),
    (forall g1 g2 : Guard,
        member_guard g1 gs -> member_guard g2 gs -> g1 <> g2 -> disjoint g1 g2) ->
    (exists g : Guard, member_guard g gs /\ sat g v) ->
    exactly_one_sat gs v.
Proof. Admitted. (* 待证：互斥前提封死双投递、恒真前提封死零投递 ⇒ 恰一。
                    gs 取 all_guards_with_default ws（归一化规则守卫 +
                    default 守卫）即得路由节点实例；B-14（修订后：全部守
                    卫含 default 两两不交）提供互斥前提——注记与修订后
                    B-14 陈述一致（修订 v0.1 对齐）。 *)

(* 执行面不得再做去重/仲裁逻辑（守卫已互斥，再仲裁是语义重复）——
 * 规范句为实现义务，登记于此：执行面投递决策必须直接采用守卫求值结果，
 * 不加第二层仲裁。 *)

End T04_B15.
