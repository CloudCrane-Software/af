(* =====================================================================
 * T10 · envelope 血缘包装——类型化交接不丢失（E2201 + 血缘三位）
 *
 * 义务编号：theorems-v1.md（定稿 v1, 2026-10-24）§B-20
 * 审计结论：通过（2026-10-24；E2201 关联为释义，码位 :1569 核对在案）
 *
 * 原文摘录（定稿，逐字）：
 *   「端口类型 envelope<T> 的边，运行时包装器填 from_node/artifact_type/
 *    step 三血缘位；类型在编译期已核（E2201 两端相等），包装忠实性在
 *    TCB——编译期只核类型不验证包装内容。」
 *
 * 锚定（转引自 theorems-v1.md §B-20，行号经审计复核）：
 *   · 【规范】执行语义规范-v0.md :60（逐字）
 *   · 【矩阵】保证边界矩阵.md :30（逐字）
 *   · 【提案B】DSL-语法提案-B.md :1569（E2201 码位）
 * 对应实现位置：编译期类型核对（E2201 两端相等）在 af-dsl/af-compiler
 *   校验面；运行时包装器（envelope 包装，填三血缘位）在执行面——
 *   FV-0 实测：envelope.rs:14 入参 serde_yaml::Value，CBMC 不收敛
 *   （>4min），故机器验证走具体输入拒绝测试 + 镜像断言（theorems-v1.md
 *   §0.3/§B-20 工具修订）。路径转引自 theorems-v1.md；撰写员未直接读取
 *   实现仓库。
 *
 * 建模说明：
 *   · 编译期面（可全判）：envelope 边两端端口类型不等 ⇒ 拒绝 + E2201
 *     （「类型在编译期已核（两端相等）」的 soundness 方向）。
 *   · 运行时面（TCB）：wrapper_fill 对交付载荷填三血缘位；「包装忠实性
 *     在 TCB——编译期只核类型不验证包装内容」按 theorems-v1.md §0.1
 *     纪律登记为显式假设 wrapper_faithful_tcb（填值 = 声明面真值），
 *     性质定理在其下成立——「类型化交接不丢失」= 交付信封三血缘位在场
 *     且为真值；信任不因此消除，只可对 TCB 成员实现建模。
 *   · 「编译期只核类型不验证包装内容」为范围陈述，以注释登记，不入命题。
 *
 * 地位：只译不证。Proof. Admitted. = 待证义务登记。
 * ===================================================================== *)

Section T10_B20.

(* —— 编译期类型核对面（E2201） —— *)

Variable Document : Type.
Variable EdgeId : Type.
Variable PortType : Type.

Variable edge_src_port_type : EdgeId -> PortType.
(* 边生产端端口类型。 *)

Variable edge_dst_port_type : EdgeId -> PortType.
(* 边消费端端口类型。 *)

Variable edge_in_document : EdgeId -> Document -> Prop.
(* 边属于该文档。 *)

Variable is_envelope : PortType -> Prop.
(* 端口类型为 envelope<T>。 *)

Variable CompileOutcome : Type.
Variable compile : Document -> CompileOutcome.
Variable rejected : CompileOutcome -> Prop.
Variable ErrorCode : Type.
Variable E2201 : ErrorCode.
(* 稳定码 E2201（码表【提案B】:1569；两端端口类型相等核对）。 *)
Variable report_contains : CompileOutcome -> ErrorCode -> Prop.

Theorem B20_envelope_type_checked_E2201 :
  forall (d : Document) (e : EdgeId),
    edge_in_document e d ->
    is_envelope (edge_src_port_type e) \/ is_envelope (edge_dst_port_type e) ->
    edge_src_port_type e <> edge_dst_port_type e ->
    rejected (compile d) /\ report_contains (compile d) E2201.
Proof. Admitted. (* 待证：envelope 边两端类型不等 ⇒ 编译拒绝且 Report 含
                    E2201——「类型在编译期已核（两端相等）」。 *)

(* —— 运行时血缘包装面（TCB） —— *)

Variable NodeName : Type.
Variable ArtifactType : Type.
Variable StepId : Type.

Variable Envelope : Type.
(* 运行时信封对象。 *)

Variable lineage_from_node : Envelope -> NodeName.
Variable lineage_artifact_type : Envelope -> ArtifactType.
Variable lineage_step : Envelope -> StepId.
(* 三血缘位：from_node / artifact_type / step（【规范】:60）。 *)

Variable EdgePayload : Type.
Variable wrapper_fill : EdgeId -> EdgePayload -> Envelope.
(* 运行时包装器：对 envelope<T> 边的载荷打血缘包装。 *)

Variable delivered : EdgeId -> EdgePayload -> Prop.
(* 该载荷经该边交付。 *)

Variable producer_of : EdgeId -> NodeName.
(* 生产端节点（声明面真值）。 *)

Variable declared_artifact_type_of : EdgeId -> ArtifactType.
(* 工件类型（声明面真值）。 *)

Variable current_step_of : EdgeId -> StepId.
(* 当前步骤（声明面真值）。 *)

Hypothesis wrapper_faithful_tcb :
  forall (e : EdgeId) (p : EdgePayload),
    delivered e p ->
    lineage_from_node (wrapper_fill e p) = producer_of e
    /\ lineage_artifact_type (wrapper_fill e p) = declared_artifact_type_of e
    /\ lineage_step (wrapper_fill e p) = current_step_of e.
(* 【TCB 显式假设】（【矩阵】:30）：包装忠实性在 TCB——包装器填值 =
 * 声明面真值。编译期只核类型不验证包装内容；按 theorems-v1.md §0.1，
 * TCB 内条目只可对 TCB 成员实现建模、不可消除信任本身。 *)

Theorem B20_envelope_handoff_lineage_complete :
  forall (e : EdgeId) (p : EdgePayload),
    delivered e p ->
    lineage_from_node (wrapper_fill e p) = producer_of e
    /\ lineage_artifact_type (wrapper_fill e p) = declared_artifact_type_of e
    /\ lineage_step (wrapper_fill e p) = current_step_of e.
Proof. Admitted. (* 待证（TCB 成员实现符合前提）：类型化交接不丢失——
                    每次交付的信封三血缘位在场且为声明面真值。机器验证
                    面见文件头 FV-0 工具修订记录。 *)

End T10_B20.
