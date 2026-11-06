(* =====================================================================
 * T02 · Vec 输出稳定排序——输出序列是输入多重集的规范函数（C-01 分解）
 *
 * 义务编号：theorems-v1.md（定稿 v1, 2026-10-24）§C-03
 * 审计结论：通过（2026-10-24）
 *
 * 原文摘录（定稿，逐字）：
 *   「序列化前所有 Vec 输出按稳定键排序：nodes/edges/stores/policies.* 按
 *    id 排序；safety_points 按路径排序；carried 按 store 名排序 ⇒ 输出序
 *    列是输入多重集的规范函数（与构建过程顺序无关）。」
 *
 * 锚定：【提案B】计划/研究组/DSL-语法提案-B.md :367（句首）——转引自
 *   theorems-v1.md §C-03（行号经审计复核）。
 * 对应实现位置：af-compiler 序列化前的 Vec 排序面（nodes/edges/stores/
 *   policies/safety_points/carried 各输出集合）；键唯一性为 DSL 声明面
 *   事实（节点/store id 在文档内唯一）。路径转引自 theorems-v1.md §0
 *   别名表；撰写员未直接读取实现仓库。
 *
 * 建模说明：
 *   · key_of：排序键抽取（id / 路径 / store 名统一抽象）；key_lt：字典序
 *     （BTree 键序，标准库信任面——序性质按 TCB 登记为 Hypothesis）。
 *   · Permutation：多重集等价。标准库 List.Permutation 的本地重述——本库
 *     只依赖 prelude，不 Require List 模块；list 记法「::」依赖 list_scope
 *     （本机 coqc 8.20.1 的 prelude 未默认开启该 scope），故全文件一律用
 *     显式 cons 构造子/构造子模式，语义等价、不依赖 scope 状态。
 *   · sort_by_key：序列化前的排序步；其「置换输入 + 输出有序」实现契约
 *     登记为 Hypothesis（稳定排序语义面），义务 = 由此推出规范函数性。
 *   · 【登记前提】nodup_keys（键唯一）：稳定排序在重复键处的并列次序依赖
 *     构建序，故「多重集规范函数」在键唯一前提下才成立。nodes/edges/
 *     stores/policies 的 id 与 carried 的 store 名在 DSL 内唯一，该前提
 *     为实现面事实的显式登记，非对结论的弱化（结论逐字照原文）。
 *
 * 地位：只译不证。Proof. Admitted. = 待证义务登记。
 * ===================================================================== *)

Section T02_C03.

Variable Key : Type.
(* 排序键域。 *)

Variable Entry : Type.
(* 序列化输出条目。 *)

Variable key_of : Entry -> Key.
(* 排序键：节点/边/store/policy 的 id；safety_points 的路径；carried 的
 * store 名——各集合各取其键，统一抽象。 *)

Variable key_lt : Key -> Key -> Prop.
(* 键上的字典序（严格全序）。 *)
Hypothesis key_lt_transitive :
  forall a b c : Key, key_lt a b -> key_lt b c -> key_lt a c.
Hypothesis key_lt_irreflexive : forall a : Key, ~ key_lt a a.
Hypothesis key_lt_total :
  forall a b : Key, a <> b -> key_lt a b \/ key_lt b a.

Fixpoint member_entry (x : Entry) (l : list Entry) : Prop :=
  match l with
  | nil => False
  | cons y ys => x = y \/ member_entry x ys
  end.

Fixpoint sorted_by_key (l : list Entry) : Prop :=
  match l with
  | nil => True
  | cons x xs =>
      match xs with
      | nil => True
      | cons y _ => key_lt (key_of x) (key_of y)
      end /\ sorted_by_key xs
  end.
(* 按键升序（相邻严格递增）——字典序输出。 *)

(* 多重集等价：List.Permutation 的本地重述（只依赖 prelude）。 *)
Inductive Permutation : list Entry -> list Entry -> Prop :=
  | perm_nil : Permutation (@nil Entry) (@nil Entry)
  | perm_skip : forall (x : Entry) (l l' : list Entry),
      Permutation l l' -> Permutation (cons x l) (cons x l')
  | perm_swap : forall (x y : Entry) (l : list Entry),
      Permutation (cons y (cons x l)) (cons x (cons y l))
  | perm_trans : forall (l l' l'' : list Entry),
      Permutation l l' -> Permutation l' l'' -> Permutation l l''.

Variable sort_by_key : list Entry -> list Entry.
(* 序列化前的稳定键排序步。 *)

Hypothesis sort_permutes : forall l : list Entry, Permutation l (sort_by_key l).
(* 排序不增删条目（多重集保持）。 *)
Hypothesis sort_sorted : forall l : list Entry, sorted_by_key (sort_by_key l).
(* 输出按键升序。 *)

Definition nodup_keys (l : list Entry) : Prop :=
  forall x y : Entry,
    member_entry x l -> member_entry y l -> key_of x = key_of y -> x = y.
(* 键唯一：同键即同条目（DSL 声明面事实的登记，见文件头）。 *)

Theorem C03_output_canonical_function_of_multiset :
  forall l1 l2 : list Entry,
    Permutation l1 l2 ->
    nodup_keys l1 ->
    nodup_keys l2 ->
    sort_by_key l1 = sort_by_key l2.
Proof. Admitted. (* 待证：同多重集（Permutation）+ 键唯一 ⇒ 排序输出唯一
                    确定，与构建过程顺序无关——「输出序列是输入多重集的
                    规范函数」。 *)

End T02_C03.
