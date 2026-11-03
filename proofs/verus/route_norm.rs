//! FV-0.1 Verus PoC：`af-dsl/src/route.rs::normalize` 三性质的形式证明。
//!
//! **镜像声明（重要）**：Verus 工具链（自带 rustc fork）不能编译 af-dsl crate
//! 本体（serde/serde_yaml 依赖树 + `String`/`format!` 不在 Verus 支持面）。本
//! 文件按任务包「proofs/ 独立目录」纪律，对 normalize 的**算法核心**做保结构
//! 镜像并证明；镜像漂移风险与对账手段见报告 §FV-0.1。
//!
//! 抽象层（语义保距的化简，非功能删减）：
//! - `when_i`（CEL 串）→ 赋值 `val[i]`：互斥/覆盖是「对任意 when 真值赋值」
//!   成立的命题逻辑事实，与 CEL 语法无关，故抽象不失真；
//! - `goto`（目标名）→ `Target`（usize；EXIT 用 usize::MAX 哨兵）；
//! - guard 的 CEL **串**重构 → 结构等价谓词（串重构是该谓词的确定性打印）。
//!
//! 三性质：
//! 1. 互斥（T1）：任意赋值下，任意两条规则分支 guard 不同时成立；
//! 2. default 覆盖（T2）：任意赋值下，规则分支与 default **恰一个**成立；
//! 3. 输出确定序（T3）：边序列严格保持 out_targets 的升序、长度相等、逐位
//!    相同（BTreeSet 迭代序 = 升序的镜像）。

use vstd::prelude::*;

verus! {

/// 目标哨兵：usize::MAX 表示 `exit`（route.rs:28 的 EXIT）。
pub const EXIT: usize = usize::MAX;

spec fn is_exit(t: usize) -> bool {
    t == EXIT
}

// ───────────────── 分支 guard 谓词（§1.5 公式的谓词镜像）─────────────────

/// guard_i = when_i ∧ ¬(when_0 ∨ … ∨ when_{i-1})（route.rs:158 `rule_guard`）。
spec fn guard_holds(val: Seq<bool>, i: int) -> bool {
    0 <= i < val.len() && val[i] && !(exists |k: int| 0 <= k < i && val[k])
}

/// guard_d = ¬(when_0 ∨ … ∨ when_{n-1})；无规则时恒 true（route.rs:165）。
spec fn default_holds(val: Seq<bool>) -> bool {
    !(exists |k: int| 0 <= k < val.len() && val[k])
}

// ───────────────── T1：分支互斥 ─────────────────

proof fn prefix_true_blocks_later(val: Seq<bool>, j: int)
    requires 0 <= j < val.len()
    ensures val[j] ==> forall |i: int| j < i < val.len() ==> !guard_holds(val, i)
{
    if !val[j] {
        return;
    }
    assert forall |i: int| (j < i < val.len()) implies !guard_holds(val, i) by {
            if guard_holds(val, i) {
                // guard_i 要求无前序真值；但 val[j] 真且 j < i —— 矛盾。
                assert(val[j] && 0 <= j < i);
                assert(exists |k: int| 0 <= k < i && val[k]);
                assert(!(exists |k: int| 0 <= k < i && val[k]));
                assert(false);
            }
        };
}

/// T1：i < j 时 guard_i ∧ guard_j 不可满足（互斥，由前缀排除构造成立）。
proof fn lemma_mutual_exclusion(val: Seq<bool>, i: int, j: int)
    requires
        0 <= i < val.len(),
        0 <= j < val.len(),
        i < j,
    ensures !(guard_holds(val, i) && guard_holds(val, j))
{
    if guard_holds(val, i) {
        assert(val[i]);
        prefix_true_blocks_later(val, i);
    }
}

// ───────────────── T2：default 覆盖（恰一个分支）─────────────────

/// 存在性半边（对前缀长 n 归纳；proof fn 禁 while，用结构递归）：
/// P(n)：存在 i<n 使 guard 成立，或 [0,n) 全假。
proof fn lemma_prefix(val: Seq<bool>, n: int)
    requires 0 <= n <= val.len()
    ensures
        (exists |i: int| 0 <= i < n && guard_holds(val, i))
        || (forall |k: int| 0 <= k < n ==> !val[k]),
    decreases n
{
    if n == 0 {
        assert(forall |k: int| 0 <= k < 0 ==> !val[k]);
    } else {
        lemma_prefix(val, n - 1);
        if exists |i: int| 0 <= i < n - 1 && guard_holds(val, i) {
            assert(exists |i: int| 0 <= i < n && guard_holds(val, i));
        } else {
            assert(forall |k: int| 0 <= k < n - 1 ==> !val[k]);
            if val[n - 1] {
                assert(!(exists |k: int| 0 <= k < n - 1 && val[k]));
                assert(guard_holds(val, n - 1));
                assert(val[n - 1] && 0 <= n - 1 < val.len());
                assert(exists |i: int| 0 <= i < n && guard_holds(val, i));
            } else {
                assert(forall |k: int| 0 <= k < n ==> !val[k]);
            }
        }
    }
}

/// 存在性半边：或存在真值下标（其 guard 成立），或全假（default 成立）。
proof fn lemma_first_true_or_none(val: Seq<bool>)
    ensures (exists |i: int| 0 <= i < val.len() && guard_holds(val, i)) || default_holds(val)
{
    let n = val.len() as int;
    lemma_prefix(val, n);
    if n == 0 {
        assert(!(exists |k: int| 0 <= k < 0 && val[k]));
        assert(default_holds(val));
        return;
    }
    // 两个析取支的换算：全假 ⇔ 无真值存在。
    assert((forall |k: int| 0 <= k < n ==> !val[k]) == !(exists |k: int| 0 <= k < n && val[k]));
}

/// T2 完整形态：任意赋值下**恰一个**分支成立——
/// ① 存在性（first_true_or_none）；② 规则分支两两互斥（T1）；
/// ③ 规则分支与 default 互斥（guard_i 蕴含 val[i] 真，default 要求全假）。
proof fn lemma_exactly_one_branch(val: Seq<bool>)
    ensures
        (exists |i: int| 0 <= i < val.len() && guard_holds(val, i)) || default_holds(val),
        forall |i: int, j: int|
            0 <= i < val.len() && 0 <= j < val.len() && i < j
                ==> !(guard_holds(val, i) && guard_holds(val, j)),
        (exists |i: int| 0 <= i < val.len() && guard_holds(val, i)) ==> !default_holds(val),
{
    lemma_first_true_or_none(val);
    assert forall |i: int, j: int| (0 <= i < val.len() && 0 <= j < val.len() && i < j) implies !(guard_holds(val, i) && guard_holds(val, j)) by {
            lemma_mutual_exclusion(val, i, j);
        };
    if exists |i: int| 0 <= i < val.len() && guard_holds(val, i) {
        // 逆否：default 成立 ⇒ 全假 ⇒ 无 guard 成立，与前提矛盾。
        assert(default_holds(val) ==> !(exists |i: int| 0 <= i < val.len() && guard_holds(val, i)));
        assert(!default_holds(val));
    }
}

// ───────────────── T3：输出确定序（可执行镜像 + 后条件证明）─────────────────

/// 边镜像：目标 + 命中它的分支（None = default 分支）。guard 串由分支编号
/// 确定性重构（`(w_i) && !(前序析取)` / `!(全序析取)`），串层详见报告。
pub struct EdgeMirror {
    pub to: usize,
    pub first_rule: Option<usize>,
}

/// targets 严格升序（BTreeSet::iter 的镜像不变式）。
spec fn strictly_ascending(s: Seq<usize>) -> bool {
    forall |a: int, b: int| 0 <= a < b < s.len() ==> s[a] < s[b]
}

/// 实例化提示：升序 ⇒ ∀p<q: s[p] < s[q]（独立小问题，Z3 直接可解；
/// 供循环体内 E-matching 使用）。
proof fn ascending_hint(s: Seq<usize>, q: int)
    requires 0 <= q < s.len() && strictly_ascending(s)
    ensures forall |p: int| 0 <= p < q ==> s[p] < s[q]
{
}

/// 首命中查找（route.rs:174-179 `position` 的镜像）。
fn find_first_rule(rules: &Vec<usize>, t: usize) -> (r: Option<usize>)
    ensures
        match r {
            None => forall |k: int| 0 <= k < rules@.len() ==> rules[k] != t,
            Some(ix) => 0 <= ix < rules@.len() && rules[ix as int] == t
                && forall |k: int| 0 <= k < ix ==> rules[k] != t,
        }
{
    let mut k: usize = 0;
    while k < rules.len()
        invariant
            0 <= k <= rules@.len(),
            forall |q: int| 0 <= q < k as int ==> rules[q] != t,
        decreases rules@.len() - k
    {
        if rules[k] == t {
            return Some(k);
        }
        k = k + 1;
    }
    None
}

/// normalize 边构造循环的镜像（route.rs:172-184）。
fn norm_edges(rules: &Vec<usize>, targets: &Vec<usize>) -> (edges: Vec<EdgeMirror>)
    requires
        strictly_ascending(targets@),
        forall |k: int| 0 <= k < rules@.len() ==> is_exit(rules[k])
            || ((rules[k] as int) < targets@.len()),
    ensures
        edges@.len() == targets@.len(),
        forall |q: int| 0 <= q < edges@.len() ==> edges[q].to == targets[q],
        forall |a: int, b: int| 0 <= a < b < edges@.len() ==> edges[a].to < edges[b].to,
	{
    let mut edges: Vec<EdgeMirror> = Vec::new();
    let mut q: usize = 0;
    while q < targets.len()
        invariant
            q <= targets@.len(),
            strictly_ascending(targets@),
            edges@.len() == q as int,
            forall |p: int| 0 <= p < q as int ==> edges[p].to == targets[p],
            // 升序在 targets 层维护（出口处与上一条不变式链出边序后条件）。
            forall |a: int, b: int| 0 <= a < b < q as int ==> targets[a] < targets[b],
        decreases targets@.len() - q
    {
        let tq = targets[q];
        proof {
            ascending_hint(targets@, q as int);
        }
        let first = find_first_rule(rules, tq);
        edges.push(EdgeMirror { to: tq, first_rule: first });
        q = q + 1;
    }
    // 出口推导：位置相等（不变式4）× targets 升序（不变式5/前置）⇒ 边升序。
    assert forall |a: int, b: int| (0 <= a < b < edges@.len()) implies edges[a].to < edges[b].to by {
        assert(edges[a].to == targets[a]);
        assert(targets[a] < targets[b]);
    };
    edges
}

// ───────────────── 顶层断言（验证入口；T1/T2 为 proof fn，由内核直接接受）─────────────────

fn main() {
    // T3 执行确认：3 目标 × 3 规则（含一条 exit 规则）。
    let mut r2: Vec<usize> = Vec::new();
    r2.push(0); // goto t0
    r2.push(EXIT); // goto exit（不产边）
    r2.push(2); // goto t2
    let mut ts: Vec<usize> = Vec::new();
    ts.push(0);
    ts.push(1);
    ts.push(2);
    let edges = norm_edges(&r2, &ts);
    assert(edges.len() == 3);
    // 分步断言：先钉住 targets 内容，再实例化 ensures 的逐位相等。
    assert(ts[0] == 0 && ts[1] == 1 && ts[2] == 2);
    assert(r2[0] == 0 && r2[1] == EXIT && r2[2] == 2);
    assert(edges[0].to == ts[0] && edges[1].to == ts[1] && edges[2].to == ts[2]);
    assert(edges[0].to == 0 && edges[1].to == 1 && edges[2].to == 2);
}

}
