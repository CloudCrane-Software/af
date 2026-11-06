(* =====================================================================
 * T01 · 确定性编译——双跑逐位一致（C 组核心定理）
 *
 * 义务编号：theorems-v1.md（定稿 v1, 2026-10-24）§C-01
 * 审计结论：修订后通过（2026-10-24）
 *
 * 原文摘录（定稿，逐字）：
 *   「同一份 YAML 文档经 af-dsl→af-compiler 管线两次编译，产出的 IR 与计划
 *    对象逐字节相同——lower 是输入文档的纯函数（无隐藏状态影响输出）。」
 *
 * 锚定（转引自 theorems-v1.md §C-01，行号经该次独立审计复核）：
 *   · 【提案B】计划/研究组/DSL-语法提案-B.md :367（句末）
 *   · 【矩阵】crates/af-compiler/docs/保证边界矩阵.md :41
 *   · 【G1裁决】计划/研究组/G1-放行裁决-20261024.md :31（G2 放行判据：
 *     双跑逐位一致）
 *
 * 对应实现位置：af-dsl→af-compiler 管线 lower 入口（crates/af-compiler）。
 *   序确定性的分解面：C-02（model.rs:32 subgraphs BTreeMap）、C-03（序列化
 *   前 Vec 排序）、C-04（route.rs 纯文本重写）——见本库 T02/T03。
 *   以上路径与行号均转引自 theorems-v1.md §0 别名表与 §C-01/02/03/04 锚定；
 *   撰写员未直接读取实现仓库。
 *
 * 建模说明：
 *   · HiddenState：实现面一切潜在隐藏输入（时钟、随机源、HashMap 迭代序、
 *     全局可变量）。「无隐藏状态影响输出」译为对 HiddenState 的全称量化。
 *   · lower 建模为可能读取隐藏状态的函数；若实装已是纯函数，本定理仍为
 *     待证义务（证明战役 FV-0.4+ 对象），不因建模而弱化。
 *   · ByteSeq := nat -> Byte：序列化产物的逐位编码；bytes_equal 为逐位置
 *     相等，即「逐字节相同」（G1裁决 :31「逐位一致」口径）。
 *
 * 地位：只译不证。Proof. Admitted. = 待证义务登记。
 * ===================================================================== *)

Section T01_C01.

Variable Byte : Type.
(* 序列化产物的字节域（实现面 u8 的抽象）。 *)

Definition ByteSeq := nat -> Byte.
(* 逐字节产物：位置到字节的函数。 *)

Definition bytes_equal (a b : ByteSeq) : Prop := forall i : nat, a i = b i.
(* 逐字节相同 = 逐位置字节相等。 *)

Variable Document : Type.
(* 输入 YAML 文档（反序列化后的声明面对象）。 *)

Variable IR : Type.
(* 编译产物一：中间表示（af-compiler IR）。 *)

Variable PlanObject : Type.
(* 编译产物二：计划对象（af-compiler plan）。 *)

Variable HiddenState : Type.
(* 隐藏状态：管线可能接触的一切非文档输入。 *)

Variable lower : Document -> HiddenState -> (IR * PlanObject).
(* af-dsl→af-compiler 编译管线入口（lower）。第二参数为可能被读取的隐藏
 * 状态；义务内容即证明产物与之无关。 *)

Variable serialize_ir : IR -> ByteSeq.
(* IR 到序列化产物（逐字节）的编码。 *)

Variable serialize_plan : PlanObject -> ByteSeq.
(* 计划对象到序列化产物（逐字节）的编码。 *)

Theorem C01_double_run_bit_identical :
  forall (d : Document) (s1 s2 : HiddenState),
    bytes_equal (serialize_ir (fst (lower d s1))) (serialize_ir (fst (lower d s2)))
    /\ bytes_equal (serialize_plan (snd (lower d s1))) (serialize_plan (snd (lower d s2))).
Proof. Admitted. (* 待证：管线产物与 HiddenState 无关 ⇒ 双跑逐字节相同；
                    等价形式：lower 在字节编码意义下是输入文档的纯函数。 *)

End T01_C01.
