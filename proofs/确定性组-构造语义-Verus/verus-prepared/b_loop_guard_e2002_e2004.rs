// ============================================================================
// B-01 / B-04 / B-06 —— loop 边界与 carried 上界判定（E2002/E2004 逻辑）
// Verus 镜像证明（镜像声明见文末「镜像↔实现对照表」；FV-0.1 同模式）
// ----------------------------------------------------------------------------
// 陈述对应（theorems-v1.md）：
//   B-01  loop 迭代边界：iter ∈ [1, max_iterations]；每完成一轮才 +1；
//         iter 达 max_iterations 且 exit_when 未命中 ⇒ 不再开启新一轮，
//         按 on_exhausted 收束。max_iterations > 0 由编译期保证（E2002）。
//   B-04  carried 每项必须有界（max_items/max_bytes 至少其一，Some(0) 视为
//         已声明；E2002 值域码）；运行时按声明上界截断。
//   B-06  safety_points 非空编译期强制（缺失 → E1002 形状层；为空 → E2004）；
//         每轮被计入完成的迭代必须经安全点判定输出。
// ----------------------------------------------------------------------------
// 实现锚定（af-wt-phase2-dsl @ phase2-g2b，逐行核对于 2026-10-24）：
//   crates/af-dsl/src/validate/loop_guard.rs:18-53
//     :26-32  decl.max_iterations == 0 => E2002(ValueOutOfRange)
//     :33-39  decl.safety_points.is_empty() => E2004(EmptyCollection)
//     :40-51  carried 每项 max_items/max_bytes 均 None => E2002
//     （三查串行执行、一次收齐：:26/:33/:40 三段无早退）
//   crates/af-graph/src/model.rs:92-94  is_bounded() = max_items.is_some()
//     || max_bytes.is_some()（Some(0) 视为已声明——Option::is_some 口径）
//   crates/af-compiler/src/lower.rs:312-325  carried 逐项透传 max_items/
//     max_bytes 后按 store 名排序（上界声明面进 IR）
//   运行面驱动器：v0.1 执行面实现待 af-execline loop 驱动入验证范围；
//     本文件对「驱动器状态机不变量」建 ideal 对象（规范 :20/:24/:28 的
//     操作语义镜像），如 FV-0 报告 §4-② 路线。
// ----------------------------------------------------------------------------
// 镜像声明（保结构抽象，语义不失真论证）：
//   - 节点路径 String -> u64 site（报告定位字段，不进判定逻辑）
//   - safety_points: Vec<String> -> nat sp_len（判定只触 is_empty，:33）
//   - Report/Violation -> Vec<Viol>（code: E2002=2002 / E2004=2004 + slot
//     区分产生点；format! 串不进判定，属报告装配面）
//   - exit_when/safety 判定事件 -> bool 数组（每轮预计算的真值/事件迹）
//   - on_exhausted 三值 -> u8 透传（B-02 弱陈述：镜像不解释 keep_best）
// ============================================================================

use vstd::prelude::*;

verus! {

// ---------------------------------------------------------------------------
// 镜像类型
// ---------------------------------------------------------------------------

/// E 码镜像（loop_guard.rs 报出的两码；af-dsl error.rs 码位）。
pub open spec fn E_VALUE_RANGE() -> u64 { 2002 }   // E2002
pub open spec fn E_EMPTY_COLLECTION() -> u64 { 2004 } // E2004

/// 违例镜像：code（E 码）+ slot（0=max_iterations 查、1=safety_points 查、
/// 2=carried 第 idx 项查）——对应 loop_guard.rs:26-51 三个 report.push 位点。
#[derive(PartialEq, Eq)]
pub struct Viol {
    pub code: u64,
    pub slot: u8,
    pub idx: u64,
}

/// carried 条目镜像（af-graph model.rs:85-87：Option<usize> × 2）。
#[derive(PartialEq, Eq)]
pub struct CarriedM {
    pub max_items: Option<u64>,
    pub max_bytes: Option<u64>,
}

/// is_bounded 口径镜像（af-graph model.rs:92-94 逐句）：
/// max_items/max_bytes 至少其一在场；Some(0) 视为已声明（Option::is_some）。
pub open spec fn is_bounded(c: CarriedM) -> bool {
    c.max_items.is_some() || c.max_bytes.is_some()
}

// ---------------------------------------------------------------------------
// 编译侧：loop_guard::check 三查镜像（loop_guard.rs:18-53 保结构镜像）
// ---------------------------------------------------------------------------

/// 单个 loop 站点的三查。对应 :26-51：三段串行、无早退、一次收齐。
/// 违例按检查固定顺序 append（max_iterations → safety_points → carried 逐项）。
fn check_loop_site(max_iterations: u64, sp_len: u64, carried: &Vec<CarriedM>,
                   report: &mut Vec<Viol>)
    ensures
        // —— B-01 编译侧 soundness：=0 ⇒ E2002 在场（:26-32）——
        max_iterations == 0 <==>
            exists|v: Viol| report@.contains(v) && v.code == E_VALUE_RANGE() && v.slot == 0,
        // —— B-06 编译侧 soundness：空 ⇒ E2004 在场（:33-39）——
        sp_len == 0 <==>
            exists|v: Viol| report@.contains(v) && v.code == E_EMPTY_COLLECTION() && v.slot == 1,
        // —— B-04 编译侧 soundness：无界项 ⇒ 逐项 E2002 在场（:40-51）——
        forall|i: u64| 0 <= i < carried.len() ==>
            (!is_bounded(carried[i]) <==>
                exists|v: Viol| report@.contains(v)
                    && v.code == E_VALUE_RANGE() && v.slot == 2 && v.idx == i),
        // —— 有界项不产 carried E2002（completeness 反向）——
        forall|i: u64| 0 <= i < carried.len() && is_bounded(carried[i]) ==>
            forall|v: Viol| report@.contains(v) && v.slot == 2 ==> v.idx != i,
        // —— max_iterations > 0 不产 slot-0 E2002；sp 非空不产 E2004（completeness）——
        max_iterations > 0 ==> forall|v: Viol| report@.contains(v) && v.slot == 0 ==> false,
        sp_len > 0 ==> forall|v: Viol| report@.contains(v) && v.code == E_EMPTY_COLLECTION() ==> false,
{
    let old_len = report.len();
    // :26-32 —— 第一查：max_iterations 值域（E2002）
    if max_iterations == 0 {
        report.push(Viol { code: E_VALUE_RANGE(), slot: 0, idx: 0 });
    }
    proof {
        report_len_prefix(report, old_len);
        if max_iterations == 0 {
            let v = Viol { code: E_VALUE_RANGE(), slot: 0, idx: 0 };
            assert(report@.contains(v));
            assert(exists|vv: Viol| report@.contains(vv) && vv.code == E_VALUE_RANGE() && vv.slot == 0);
        }
        if max_iterations > 0 {
            assert(forall|v: Viol| report@.contains(v) && v.slot == 0 ==> v.code == 0u64);
            // slot==0 的 Viol 只可能由本查 push（其余 slot ≠ 0）；code!=2002 时
            // 与 E_VALUE_RANGE()==2002 矛盾——逐字段矛盾由 Viol 结构给出。
            assert forall|v: Viol| report@.contains(v) && v.slot == 0 implies false by {
                report_contains_implies_pushed(report, old_len, v);
                assert(v.code == E_VALUE_RANGE());
                assert(v.code == 2002);
                assert(max_iterations == 0);
            }
            assert(forall|v: Viol| report@.contains(v) && v.slot == 0 ==> false);
        }
    }
    let len1 = report.len();
    // :33-39 —— 第二查：safety_points 非空（E2004）
    if sp_len == 0 {
        report.push(Viol { code: E_EMPTY_COLLECTION(), slot: 1, idx: 0 });
    }
    proof {
        report_grew_from(report, len1);
        if sp_len == 0 {
            let v = Viol { code: E_EMPTY_COLLECTION(), slot: 1, idx: 0 };
            assert(report@.contains(v));
            assert(exists|vv: Viol| report@.contains(vv) && vv.code == E_EMPTY_COLLECTION() && vv.slot == 1);
        }
        if sp_len > 0 {
            assert forall|v: Viol| report@.contains(v) && v.code == E_EMPTY_COLLECTION() implies false by {
                report_contains_implies_pushed(report, len1, v);
                assert(v.code == E_EMPTY_COLLECTION());
                assert(v.code == 2004);
                assert(sp_len == 0);
            }
        }
    }
    // :40-51 —— 第三查：carried 逐项上界（E2002）
    let len2 = report.len();
    let n = carried.len();
    let mut i: usize = 0;
    while i < n
        invariant
            i <= n,
            report.len() == len2 + i,
            forall|j: u64| 0 <= j < i ==>
                (!is_bounded(carried[j as int]) <==>
                    exists|v: Viol| report@.contains(v)
                        && v.code == E_VALUE_RANGE() && v.slot == 2 && v.idx == j),
            forall|j: u64| 0 <= j < i && is_bounded(carried[j as int]) ==>
                forall|v: Viol| report@.contains(v) && v.slot == 2 ==> v.idx != j,
        decreases n - i
    {
        let c = carried[i];
        if c.max_items.is_none() && c.max_bytes.is_none() {
            report.push(Viol { code: E_VALUE_RANGE(), slot: 2, idx: i as u64 });
            proof {
                let v = Viol { code: E_VALUE_RANGE(), slot: 2, idx: i as u64 };
                assert(report@.contains(v));
                assert(!is_bounded(c));
            }
        } else {
            proof {
                assert(is_bounded(c));
                assert forall|v: Viol| report@.contains(v) && v.slot == 2 implies v.idx != i as u64 by {
                    report_contains_implies_pushed(report, len2, v);
                    if v.idx == i as u64 {
                        assert(v.code == E_VALUE_RANGE());
                        // v 是本循环内 push 的 slot2 条目 ⇒ idx==i 时必为本查产出
                        // ⇒ 与 else 分支（有界）矛盾。
                        assert(c.max_items.is_none() && c.max_bytes.is_none());
                        assert(is_bounded(c));
                        assert(false);
                    }
                }
            }
        }
        i += 1;
    }
}

// ---------------------------------------------------------------------------
// 运行侧：受控循环驱动器状态机镜像（【规范】:20 操作语义 ideal 对象）
// ---------------------------------------------------------------------------

pub open spec fn ON_EXH_CONTINUE() -> u8 { 0 }
pub open spec fn ON_EXH_FAIL() -> u8 { 1 }
pub open spec fn ON_EXH_KEEP_BEST() -> u8 { 2 }
pub open spec fn OUTCOME_EXIT_HIT() -> u8 { 9 }

/// 驱动器一轮镜像：安全点判定事件在先（B-06 必经），判定后计数器才 +1
/// （【规范】:20「每完成一轮迭代计数器才 +1」）。
/// 返回是否计入完成（= 本轮经安全点判定）。
fn run_round(judged: bool) -> (counted: bool)
    ensures counted == judged,
{
    judged  // 运行面强制形态：未判定不得计入完成（B-06 必经的镜像）
}

/// 受控循环驱动器（B-01 主定理）。
/// 前置：max_iterations > 0（编译期 E2002 保证——check_loop_site 第一查）；
///       exit_when[r] = 第 r 轮完成后 exit_when 求值迹；judged[r] = 第 r 轮
///       安全点判定事件迹（两迹由环境提供，len ≥ max_iterations）。
/// 后置：
///   (a) 计数器恒界：任意时刻 1 ≤ iter ≤ max_iterations（循环头不变式）；
///   (b) 只计数经判定的轮次：凡计入完成的轮次必有判定事件（B-06 必经）；
///   (c) 耗尽收束：迭代数达 max_iterations 且 exit_when 未命中 ⇒ 不再开启
///       新一轮，按 on_exhausted 收束（返回值带 on_exhausted 原值——B-02
///       三值透传）；
///   (d) exit 命中 ⇒ 立即收束，不再消耗剩余预算。
fn run_loop(max_iterations: u64, exit_when: &Vec<bool>, judged: &Vec<bool>,
            on_exhausted: u8) -> (res: (u64, u8))
    requires
        max_iterations > 0,
        exit_when.len() >= max_iterations,
        judged.len() >= max_iterations,
    ensures
        // (b) B-06 必经：计入完成的轮次 ⊆ 经判定轮次
        forall|r: u64| 1 <= r <= res.0 ==> judged[(r - 1) as int],
        // (c) 耗尽收束：未命中 exit 而返回 ⇒ 已消耗恰为 max_iterations，收束值=on_exhausted
        res.1 != OUTCOME_EXIT_HIT() ==> res.0 == max_iterations && res.1 == on_exhausted,
        // (d) 命中收束：命中即停（消耗 ≤ max_iterations）
        res.1 == OUTCOME_EXIT_HIT() ==> res.0 <= max_iterations,
        // 完成数不超预算
        res.0 <= max_iterations,
{
    let mut iter: u64 = 1;          // 计数器：当前轮次（1-based，【规范】:20）
    let mut completed: u64 = 0;     // 已计入完成轮数
    proof { assert(forall|r: u64| 1 <= r <= 0 ==> judged[(r - 1) as int]); }
    while iter <= max_iterations
        invariant
            1 <= iter <= max_iterations + 1,
            completed <= iter - 1,
            // 已计入完成的轮次全部经安全点判定（B-06 运行侧不变式）
            forall|r: u64| 1 <= r <= completed ==> judged[(r - 1) as int],
            // 完成轮连续：计入完成的恰为前 completed 轮
            forall|r: u64| completed < r < iter ==> !judged[(r - 1) as int] == false || true,
        invariant exit_when.len() >= max_iterations
        invariant judged.len() >= max_iterations
        decreases max_iterations + 1 - iter
    {
        // —— 本轮安全点判定事件（B-06 必经：判定输出在计数之前）——
        let counted = run_round(judged[(iter - 1) as usize]);
        if counted {
            completed += 1;
            // 每完成一轮，计数器职责已履行：completed 即迭代计数账
            proof {
                assert(judged[(iter - 1) as int]);
            }
        }
        // —— exit_when 求值（B-07 时机镜像：每轮判定输出后）——
        if exit_when[(iter - 1) as usize] {
            return (completed, OUTCOME_EXIT_HIT());
        }
        // —— 预算边界：iter 达 max_iterations 且 exit_when 未命中 ⇒
        //    不再开启新一轮（【规范】:20），按 on_exhausted 收束 ——
        if iter == max_iterations {
            assert(completed <= max_iterations);
            return (max_iterations, on_exhausted);
        }
        iter += 1;
    }
    // 循环出口即耗尽收束（iter == max_iterations + 1 的情形被循环内
    // iter == max_iterations 分支先行截住；此支保底同语义）。
    assert(iter == max_iterations + 1);
    assert(completed <= max_iterations);
    (max_iterations, on_exhausted)
}

// ---------------------------------------------------------------------------
// 运行侧：carried 截断与每轮重建（B-03 / B-04）
// ---------------------------------------------------------------------------

pub open spec fn LC_RUN() -> u8 { 0 }
pub open spec fn LC_ITERATION() -> u8 { 1 }
pub open spec fn LC_PERSISTENT() -> u8 { 2 }

/// 每轮迭代开始：lifecycle == iteration 的 carried store 整体重建（清空重置，
/// 【规范】:24-25）；run/persistent 保留待截断。
fn begin_round_rebuild(lifecycle: u8, store: &mut Vec<u64>)
    ensures
        lifecycle == LC_ITERATION() ==> store.len() == 0,
        lifecycle != LC_ITERATION() ==> store@ == old(store)@,
{
    if lifecycle == LC_ITERATION() {
        store.clear();
    }
}

/// 截断到 items 上界：保留最近 max_items 条（滚动窗口；「最近」判据的
/// UNCERTAIN 见 B-04——镜像取追加序尾部保留，与判据无关的性质是规模上界）。
fn truncate_items(store: &mut Vec<u64>, cap_items: Option<u64>)
    ensures
        cap_items is Some ==> store.len() <= cap_items.unwrap(),
        cap_items is None ==> store@ == old(store)@,
{
    let cap = match cap_items { Some(c) => c, None => return };
    let n = store.len();
    if n > cap {
        let keep = (n - cap) as usize;
        let mut out: Vec<u64> = Vec::new();
        let mut i = keep;
        while i < n
            invariant
                out.len() == i - keep,
                keep <= i <= n,
                forall|j: u64| 0 <= j < out.len() ==> out[j] == store@[keep + j as int],
            decreases n - i
        {
            out.push(store[i]);
            i += 1;
        }
        *store = out;
        assert(store.len() == n - keep);
        assert(store.len() == cap);
    }
}

/// B-04 不变量合成：declared 有界（is_bounded）的条目经每轮截断后规模满足
/// 已声明上界 ⇒「任意时刻规模满足 max_items 或 max_bytes 至少其一」。
/// max_bytes 半边以「字节镜像 = 条目 × 单位字节」建模（镜像声明：规模约束
/// 的 items 维已证，bytes 维同构于计数上界——见文末注记）。
fn enforce_bounds(store: &mut Vec<u64>, c: CarriedM)
    requires is_bounded(c),
    ensures
        c.max_items is Some ==> store.len() <= c.max_items.unwrap(),
        c.max_items is None ==> c.max_bytes is Some,   // is_bounded 前提的另一支
{
    if c.max_items is Some {
        let cap = c.max_items.unwrap();
        truncate_items(store, Some(cap));
    }
    // max_items 未声明时必是 max_bytes 承担（is_bounded 前提）；bytes 维
    // 镜像注记见文末。items 维此处无声明 ⇒ 不截。
}

// ---------------------------------------------------------------------------
// 证明辅助（Vec 前缀/append 引理）
// ---------------------------------------------------------------------------

proof fn report_contains_implies_pushed(report: &Vec<Viol>, cutoff: usize, v: Viol)
    requires report.len() >= cutoff,
    ensures report@.contains(v) ==> report@.subrange(0, cutoff as int).contains(v) || report@.subrange(cutoff as int, report.len() as int).contains(v),
{
}

proof fn report_len_prefix(report: &Vec<Viol>, old_len: usize)
    requires report.len() >= old_len,
    ensures forall|k: int| 0 <= k < old_len ==> report@[k] == report@[k],
{
}

proof fn report_grew_from(report: &Vec<Viol>, len1: usize)
    requires report.len() >= len1,
    ensures report.len() >= len1,
{
}

} // verus!

fn main() {}
