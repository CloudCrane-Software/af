(* =====================================================================
 * T03 · route 归一化无搜索、确定性重写（C-01 分解）
 *
 * 义务编号：theorems-v1.md（定稿 v1, 2026-10-24）§C-04
 * 审计结论：通过（2026-10-24；审计补证：route.rs:21-22 模块自注
 *   「纯函数（不触索引/文档）」、:1-12 确定性重构公式）
 *
 * 原文摘录（定稿，逐字）：
 *   「route 归一化 = 纯文本重写（规则文档序 + ¬ 前缀展开），不涉及集合枚
 *    举或排序策略选择，无搜索、无图分析 ⇒ 归一化输出与（节点 route 规则
 *    列表，文档序）一一互定。」
 *
 * 锚定（转引自 theorems-v1.md §C-04，行号经审计复核）：
 *   · 【提案B】计划/研究组/DSL-语法提案-B.md :368、:230
 *   · 【规范】crates/af-compiler/docs/执行语义规范-v0.md :45
 *   · 【矩阵】crates/af-compiler/docs/保证边界矩阵.md :34
 * 对应实现位置：crates/af-compiler route 归一化（route.rs:21-22、:1-12，
 *   theorems-v1.md §C-04 审计标记所引）；撰写员未直接读取实现仓库。
 *
 * 建模说明：
 *   · normalize 建模为可能读取 HiddenState 的函数：若归一化存在集合枚举、
 *     排序策略选择、搜索或图分析等非输入依赖，即抽象为对 HiddenState 的
 *     读取；义务 = 证明输出与之无关（C04_normalize_pure_rewrite）。
 *   · 「一一互定」拆两半：输出由（规则列表，文档序）唯一决定（纯函数性）
 *     + 规则列表由输出唯一决定（注入性）。两定理合取即原文结论。
 *   · 相等取归一化产物的结构相等（逐字段）；若实装存在商掉信息的归一化
 *     细节（如删除恒假守卫边），注入性方向须在证明时显式处理或按实装
 *     修订义务——留待证明战役核实。
 *
 * 地位：只译不证。Proof. Admitted. = 待证义务登记。
 * ===================================================================== *)

Section T03_C04.

Variable RouteRule : Type.
(* route 规则（when 守卫 + goto 目标），文档序条目。 *)

Variable NormalizedRoute : Type.
(* 归一化产物：各规则出边（带 ¬ 前缀展开后的守卫）+ route_exit_guard
 * 联合守卫（G1裁决 :18 读法）。 *)

Variable HiddenState : Type.
(* 一切潜在非输入信息源（全局量/时钟/迭代序/随机）。 *)

Variable normalize : list RouteRule -> HiddenState -> NormalizedRoute.
(* route 归一化：纯文本重写（规则文档序 + ¬ 前缀展开）。 *)

Theorem C04_normalize_pure_rewrite :
  forall (r : list RouteRule) (s1 s2 : HiddenState),
    normalize r s1 = normalize r s2.
Proof. Admitted. (* 待证（一半）：无搜索、无图分析、不涉及集合枚举或排序
                    策略选择 ⇒ 归一化输出不依赖任何隐藏状态。 *)

Theorem C04_normalize_mutually_determined :
  forall r1 r2 : list RouteRule,
    normalize r1 = normalize r2 -> r1 = r2.
Proof. Admitted. (* 待证（另一半）：「一一互定」反向——归一化输出唯一
                    决定（规则列表，文档序）——注入性。 *)

(* 两定理合取 = 「归一化输出与（节点 route 规则列表，文档序）一一互定」。 *)

End T03_C04.
