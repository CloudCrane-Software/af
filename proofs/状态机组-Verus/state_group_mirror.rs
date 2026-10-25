//! D 组 · 状态机组（D-01～D-09）Verus 镜像证明。
//! 正本：agent黑灯工厂/计划/研究组/形式化/theorems-v1.md §D 组（9 条，2026-10-24 审计定稿）。
//! 锚定：af-wt-phase2-dsl/crates/af-workorder/src/state.rs（127 行全文）、
//!       order.rs（门控 release_missing/close_missing/transition :401-458、new :196-257）、
//!       error.rs（GateError/Missing :10-52）。
//!
//! **镜像声明（沿用 FV-0 先例 proofs/verus/route_norm.rs 的模式，如实登记）**：
//! Verus 工具链（自带 rustc fork）不能编译 af-workorder crate 本体
//! （serde/chrono/thiserror 依赖树 + String/format! 不在其支持面），故对状态机
//! 与门控的**判定核心**做保结构镜像后证明。每个镜像函数逐行锚定源码行号；
//! 抽象层仅三处，均为语义保距：
//!   ① voucher_id: String → u64（凭证身份是不透明标识；缺失清单的变体与顺序
//!      与其字面值无关——D-09 关心变体与顺序与确定性）；
//!   ② artifacts: Vec<WorkArtifact> → Vec<ArtM{is_final, qualified}>（close 门控
//!      只读这两个字段——order.rs:422，其余字段不在判定可达面）；
//!   ③ line: Option<LineBinding> → bool（release 门控只读"是否已绑定"——order.rs:407）。
//! 纯度判定（任务令要求先行核定的边界）：
//!   - state.rs 全部五个函数（from_state/to_state/allowed_transitions/can_apply/
//!     is_terminal）为纯函数，无 IO；
//!   - order.rs 的 release_missing/close_missing/transition 亦为纯内存计算，无
//!     时钟/网络/打印——**本组 9 条不存在"含 IO 只能证半边"的条目**；
//!   - 真实边界另有三处，见文末「边界登记」注释块与 RUN-SUMMARY。

use vstd::prelude::*;

verus! {

// ═══════════════════ 镜像类型 ═══════════════════

/// OrderState 镜像（state.rs:10-18，六态逐字对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderState {
    Created,
    Released,
    InProgress,
    Completed,
    Closed,
    Cancelled,
}

/// Transition 镜像（state.rs:21-33，五跃迁逐字对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    Release,
    Start,
    Complete,
    Close,
    Cancel,
}

/// WorkArtifact 判定面镜像（order.rs:49-56；只取 close 门控可达的两字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtM {
    pub is_final: bool,
    pub qualified: bool,
}

/// HistoryRecord 镜像（order.rs:59-63）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryRecordM {
    pub from: OrderState,
    pub to: OrderState,
}

/// Missing 镜像（error.rs:11-21；voucher_id: String → u64 抽象①）。
/// （int 无 Debug，故此镜像类型不派生 Debug。）
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MissingM {
    LineVersionNotBound,
    BudgetVoucherNotIssued,
    BudgetVoucherNotSigned {
        voucher_id: u64,
    },
    NoQualifiedFinalArtifact,
}

use OrderState::*;
use Transition::*;
use MissingM::*;

/// GateError 判定面镜像（error.rs:37-52；transition 只产前两变体——
/// MutationForbidden 走 attach 路径，不在 D 组判定可达面）。
#[derive(Clone, PartialEq, Eq)]
pub enum GateErrM {
    /// error.rs:39-44
    IllegalTransition {
        from: OrderState,
        to: OrderState,
        allowed: Vec<Transition>,
    },
    /// error.rs:45-49
    PreconditionsNotMet {
        transition: Transition,
        missing: Vec<MissingM>,
    },
}

// ═══════════════════ state.rs 判定核心镜像（spec 层） ═══════════════════

/// 合法出边表镜像（state.rs:58-67 逐状态逐序——push 链保序）。
pub open spec fn allowed_spec(s: OrderState) -> Seq<Transition> {
    match s {
        OrderState::Created => Seq::empty().push(Transition::Release).push(Transition::Cancel),        // :61
        OrderState::Released => Seq::empty().push(Transition::Start).push(Transition::Cancel),         // :62
        OrderState::InProgress => Seq::empty().push(Transition::Complete).push(Transition::Cancel),    // :63
        OrderState::Completed => Seq::empty().push(Transition::Close),                                 // :64
        OrderState::Closed | OrderState::Cancelled => Seq::empty(),                                    // :65
    }
}

/// t ∈ 表（Seq 成员判定；状态机层"边存在性"的唯一形态）。
/// 本状态机的表长恒 ≤ 2（allowed_spec 逐臂定义），前三分支即完全判定；
/// 末支为一般回退（本镜像可达面内不可达）。
pub open spec fn in_seq(tbl: Seq<Transition>, t: Transition) -> bool {
    if tbl.len() == 0 {
        false
    } else if tbl.len() == 1 {
        tbl[0] == t
    } else if tbl.len() == 2 {
        tbl[0] == t || tbl[1] == t
    } else {
        exists |i: int| 0 <= i < tbl.len() && tbl[i] == t
    }
}

/// can_apply 镜像（state.rs:69-75：Cancel 三起点特判 :71-73，其余查表 :74）。
pub open spec fn can_apply_spec(s: OrderState, t: Transition) -> bool {
    if t is Cancel {
        s is Created || s is Released || s is InProgress
    } else {
        in_seq(allowed_spec(s), t)
    }
}

/// to_state 镜像（state.rs:46-54）。
pub open spec fn to_state_spec(t: Transition) -> OrderState {
    match t {
        Transition::Release => OrderState::Released,     // :48
        Transition::Start => OrderState::InProgress,     // :49
        Transition::Complete => OrderState::Completed,   // :50
        Transition::Close => OrderState::Closed,         // :51
        Transition::Cancel => OrderState::Cancelled,     // :52
    }
}

/// from_state 镜像（state.rs:36-44）。
/// 边界：Cancel 分支在源码是 `unreachable!`（:42）——Cancel 无单一起点。
/// 按 theorems-v1 D-05 工具注记「unreachable 分支以 ensures 前置排除」：
/// exec 镜像以 requires t != Cancel 排除；spec 层该分支取值无语义（recommends）。
pub open spec fn from_state_spec(t: Transition) -> OrderState
    recommends t != Transition::Cancel,
{
    match t {
        Transition::Release => OrderState::Created,      // :38
        Transition::Start => OrderState::Released,       // :39
        Transition::Complete => OrderState::InProgress,  // :40
        Transition::Close => OrderState::Completed,      // :41
        Transition::Cancel => OrderState::Created,       // :42 unreachable!——被 requires 排除，取值无义
    }
}

/// is_terminal 镜像（state.rs:77-79）。
pub open spec fn is_terminal_spec(s: OrderState) -> bool {
    s is Closed || s is Cancelled
}

/// D-01 的"声明生命周期"判定面：定理陈述逐字所列的合法边集。
pub open spec fn declared_edge(s: OrderState, t: Transition) -> bool {
    match s {
        OrderState::Created => t is Release || t is Cancel,
        OrderState::Released => t is Start || t is Cancel,
        OrderState::InProgress => t is Complete || t is Cancel,
        OrderState::Completed => t is Close,
        _ => false,
    }
}

// ═══════════════════ state.rs 判定核心镜像（exec 层，与 spec 焊接） ═══════════════════

/// allowed_transitions 镜像（state.rs:59-67；源返回 &'static [Transition]，
/// transition 在 order.rs:438 做 .to_vec()——镜像以 Vec 承接同一序）。
pub fn allowed_transitions_exec(s: OrderState) -> (v: Vec<Transition>)
    ensures v@ == allowed_spec(s),
{
    let mut v: Vec<Transition> = Vec::new();
    match s {
        OrderState::Created => {
            v.push(Transition::Release);
            v.push(Transition::Cancel);
        },
        OrderState::Released => {
            v.push(Transition::Start);
            v.push(Transition::Cancel);
        },
        OrderState::InProgress => {
            v.push(Transition::Complete);
            v.push(Transition::Cancel);
        },
        OrderState::Completed => {
            v.push(Transition::Close);
        },
        OrderState::Closed | OrderState::Cancelled => {},
    }
    v
}

/// can_apply 镜像（state.rs:69-75）。
pub fn can_apply_exec(s: OrderState, t: Transition) -> (r: bool)
    ensures r == can_apply_spec(s, t),
{
    match t {
        Transition::Cancel => {
            // :70-73 三起点特判
            match s {
                OrderState::Created | OrderState::Released | OrderState::InProgress => true,
                _ => false,
            }
        },
        _ => {
            // :74 allowed_transitions().contains(&t)
            in_tbl_exec(s, t)
        },
    }
}

/// Vec 版 contains 镜像（:74 `.contains(&t)`）：
/// 逐状态结构匹配 = 对 allowed_transitions()（:59-67）成员性的直接复合。
fn in_tbl_exec(s: OrderState, t: Transition) -> (r: bool)
    ensures r == in_seq(allowed_spec(s), t),
{
    match s {
        OrderState::Created => {
            match t {
                Transition::Release | Transition::Cancel => true,
                _ => false,
            }
        },
        OrderState::Released => {
            match t {
                Transition::Start | Transition::Cancel => true,
                _ => false,
            }
        },
        OrderState::InProgress => {
            match t {
                Transition::Complete | Transition::Cancel => true,
                _ => false,
            }
        },
        OrderState::Completed => {
            match t {
                Transition::Close => true,
                _ => false,
            }
        },
        OrderState::Closed | OrderState::Cancelled => false,
    }
}

/// to_state 镜像（state.rs:46-54）。
pub fn to_state_exec(t: Transition) -> (s: OrderState)
    ensures s == to_state_spec(t),
{
    match t {
        Transition::Release => OrderState::Released,
        Transition::Start => OrderState::InProgress,
        Transition::Complete => OrderState::Completed,
        Transition::Close => OrderState::Closed,
        Transition::Cancel => OrderState::Cancelled,
    }
}

/// from_state 镜像（state.rs:36-44；:42 unreachable! 分支按 D-05 以前置排除）。
pub fn from_state_exec(t: Transition) -> (s: OrderState)
    requires t != Transition::Cancel,
    ensures s == from_state_spec(t),
{
    match t {
        Transition::Release => OrderState::Created,
        Transition::Start => OrderState::Released,
        Transition::Complete => OrderState::InProgress,
        Transition::Close => OrderState::Completed,
        // requires 已排除；匹配完整性所需的兜底臂不可达
        Transition::Cancel => OrderState::Created,
    }
}

/// is_terminal 镜像（state.rs:77-79）。
pub fn is_terminal_exec(s: OrderState) -> (r: bool)
    ensures r == is_terminal_spec(s),
{
    match s {
        OrderState::Closed | OrderState::Cancelled => true,
        _ => false,
    }
}

// ═══════════════════ order.rs 门控镜像（spec 层） ═══════════════════

/// close 门控的存在判定镜像（order.rs:422 `a.is_final && a.qualified` 的 any）。
pub open spec fn artifacts_qf(arts: Seq<ArtM>) -> bool {
    exists |i: int| 0 <= i < arts.len() && arts[i].is_final && arts[i].qualified
}

/// close_missing 镜像（order.rs:420-427）：qf ⇔ 清单空。
pub open spec fn close_missing_spec(qf: bool) -> Seq<MissingM> {
    if qf {
        Seq::empty()
    } else {
        Seq::empty().push(MissingM::NoQualifiedFinalArtifact)
    }
}

/// release_missing 镜像（order.rs:405-418 逐分支）：产线在前、凭证在后。
pub open spec fn release_missing_spec(lb: bool, vp: bool, vs: bool, vid: u64) -> Seq<MissingM> {
    if !lb && !vp {
        // :407-409 + :410-411
        Seq::empty().push(MissingM::LineVersionNotBound).push(MissingM::BudgetVoucherNotIssued)
    } else if !lb && vp && !vs {
        // :407-409 + :412-414
        Seq::empty().push(MissingM::LineVersionNotBound)
            .push(MissingM::BudgetVoucherNotSigned { voucher_id: vid })
    } else if !lb && vp && vs {
        // :407-409 + :415（Some(_)=>{}）
        Seq::empty().push(MissingM::LineVersionNotBound)
    } else if lb && !vp {
        // :410-411
        Seq::empty().push(MissingM::BudgetVoucherNotIssued)
    } else if lb && vp && !vs {
        // :412-414
        Seq::empty().push(MissingM::BudgetVoucherNotSigned { voucher_id: vid })
    } else {
        // lb && vp && vs
        Seq::empty()
    }
}

/// transition 的门控汇总镜像（order.rs:441-445 match t）。
pub open spec fn gate_missing_spec(
    t: Transition,
    lb: bool,
    vp: bool,
    vs: bool,
    vid: u64,
    arts: Seq<ArtM>,
) -> Seq<MissingM> {
    match t {
        Transition::Release => release_missing_spec(lb, vp, vs, vid),
        Transition::Close => close_missing_spec(artifacts_qf(arts)),
        _ => Seq::empty(),
    }
}

/// 履历不变量（D-07：链相接 + 末条 to = 当前状态）。
pub open spec fn hist_inv(h: Seq<HistoryRecordM>, s: OrderState) -> bool {
    (forall |i: int| 0 <= i < h.len() - 1 ==> h[i].to == h[i + 1].from)
        && (h.len() > 0 ==> h[h.len() - 1].to == s)
}

// ═══════════════════ order.rs 门控镜像（exec 层） ═══════════════════

/// release_missing 镜像（order.rs:405-418）。
pub fn release_missing_exec(lb: bool, vp: bool, vs: bool, vid: u64) -> (m: Vec<MissingM>)
    ensures m@ == release_missing_spec(lb, vp, vs, vid),
{
    let mut m: Vec<MissingM> = Vec::new();
    if !lb {
        m.push(MissingM::LineVersionNotBound); // :407-409 产线在前
    }
    if !vp {
        m.push(MissingM::BudgetVoucherNotIssued); // :410-411
    } else if !vs {
        m.push(MissingM::BudgetVoucherNotSigned { voucher_id: vid }); // :412-414
    }
    m
}

/// close_missing 镜像（order.rs:420-427；:422 any 的显式扫描循环）。
pub fn close_missing_exec(artifacts: &Vec<ArtM>) -> (m: Vec<MissingM>)
    ensures m@ == close_missing_spec(artifacts_qf(artifacts@)),
{
    let mut i: usize = 0;
    while i < artifacts.len()
        invariant
            i <= artifacts@.len(),
            forall |k: int| 0 <= k < i as int ==> !(artifacts@[k].is_final && artifacts@[k].qualified),
        decreases artifacts@.len() - (i as int),
    {
        let a = artifacts[i];
        if a.is_final && a.qualified {
            // 命中：order.rs:422-423 返回空清单
            assert(artifacts@[i as int].is_final && artifacts@[i as int].qualified);
            assert(artifacts_qf(artifacts@));
            let m: Vec<MissingM> = Vec::new();
            assert(m@ == close_missing_spec(true));
            return m;
        }
        i = i + 1;
    }
    // 扫描毕无命中：order.rs:424-426 返回 [NoQualifiedFinalArtifact]
    assert(i == artifacts@.len());
    assert(!artifacts_qf(artifacts@));
    let mut m: Vec<MissingM> = Vec::new();
    m.push(MissingM::NoQualifiedFinalArtifact);
    assert(m@ == close_missing_spec(false));
    m
}

// ═══════════════════ order.rs transition 统一入口镜像（:432-458） ═══════════════════

/// transition 镜像。源码以 &mut self 改 self.state / self.history；镜像以
/// 值传入当前状态 s0、&mut 传 history，返回（迁移后状态, 判定结果）——
/// 调用方按 out.0 续链（见 main 的多步场景）。
///
/// 本 ensures 即 D-02 / D-06（②错误不混淆）/ D-07 / D-09（门控失败半边）的
/// 机器化陈述，由内核对**任意**输入验证（6 状态 × 5 跃迁 = 30 例由数据域
/// 有限性覆盖）：
///   - Ok ⇔ 边存在 ∧ 清单空；Ok ⇒ 落 to_state ∧ 恰追加一条 {from,to} 履历；
///   - !can_apply ⇒ Err(IllegalTransition{from,to,allowed}) 且状态/履历不动；
///   - can_apply ∧ 清单非空 ⇒ Err(PreconditionsNotMet{t,missing}) 且状态/履历不动。
pub fn transition_exec(
    s0: OrderState,
    t: Transition,
    lb: bool,
    vp: bool,
    vs: bool,
    vid: u64,
    artifacts: &Vec<ArtM>,
    history: &mut Vec<HistoryRecordM>,
) -> (out: (OrderState, Result<(), GateErrM>))
    ensures
        hist_inv(old(history)@, s0) ==> hist_inv(final(history)@, out.0),
        match out.1 {
            Result::Ok(_) =>
                out.0 == to_state_spec(t)
                    && can_apply_spec(s0, t)
                    && gate_missing_spec(t, lb, vp, vs, vid, artifacts@).len() == 0
                    && final(history)@ == old(history)@.push(HistoryRecordM { from: s0, to: out.0 }),
            Result::Err(GateErrM::IllegalTransition { from, to, allowed }) =>
                out.0 == s0
                    && final(history)@ == old(history)@
                    && !can_apply_spec(s0, t)
                    && from == s0
                    && to == to_state_spec(t)
                    && allowed@ == allowed_spec(s0),
            Result::Err(GateErrM::PreconditionsNotMet { transition: tt, missing }) =>
                out.0 == s0
                    && final(history)@ == old(history)@
                    && can_apply_spec(s0, t)
                    && tt == t
                    && missing@ == gate_missing_spec(t, lb, vp, vs, vid, artifacts@)
                    && gate_missing_spec(t, lb, vp, vs, vid, artifacts@).len() > 0,
        },
{
    let from = s0; // order.rs:433
    if !can_apply_exec(from, t) {
        // order.rs:434-440：先于一切状态变更早退
        let allowed = allowed_transitions_exec(from); // :438 .to_vec()
        return (from, Result::Err(GateErrM::IllegalTransition {
            from,
            to: to_state_exec(t),
            allowed,
        }));
    }
    let missing: Vec<MissingM> = match t {
        // order.rs:441-445
        Transition::Release => release_missing_exec(lb, vp, vs, vid),
        Transition::Close => close_missing_exec(artifacts),
        _ => Vec::new(),
    };
    if missing.len() > 0 {
        // order.rs:446-451：门控拒绝，先于落状态返回
        return (from, Result::Err(GateErrM::PreconditionsNotMet { transition: t, missing }));
    }
    // order.rs:452-456：两条都过才落状态并记履历
    let to = to_state_exec(t);
    history.push(HistoryRecordM { from, to });
    proof {
        assert(missing@ == gate_missing_spec(t, lb, vp, vs, vid, artifacts@));
        assert(missing.len() == 0);
        assert(gate_missing_spec(t, lb, vp, vs, vid, artifacts@).len() == 0);
        // hist_inv 保持：前缀不动 + 末条衔接
        let h0 = old(history)@;
        let n0: int = h0.len() as int;
        assert(final(history)@ == h0.push(HistoryRecordM { from: s0, to }));
        assert(final(history)@[n0].from == s0 && final(history)@[n0].to == to);
        // ensures 是条件保持：前件 hist_inv(old, s0) 成立时链相接延续
        if hist_inv(h0, s0) {
            if n0 > 0 {
                assert(h0[n0 - 1].to == s0);
                assert(final(history)@[n0 - 1].to == s0);
            }
        }
    }
    (to, Result::Ok(()))
}

// ═══════════════════ D 组引理（proof 层，内核直接裁决） ═══════════════════

/// D-01 · 跃迁表恰为声明生命周期。
/// 上半：逐状态表内容与定理陈述逐字一致（push 链保序，term 级恒等）；
/// 下半：表即全部合法边——can_apply(s,t) ⇔ declared_edge(s,t)，
/// 30 例（6 状态 × 5 跃迁）由内核对有限数据域裁决。
pub proof fn d01_table_is_declared_lifecycle()
    ensures
        // 逐状态恰为（theorems-v1 D-01 陈述原样，序亦逐字）：
        allowed_spec(OrderState::Created)
            == Seq::empty().push(Transition::Release).push(Transition::Cancel),
        allowed_spec(OrderState::Released)
            == Seq::empty().push(Transition::Start).push(Transition::Cancel),
        allowed_spec(OrderState::InProgress)
            == Seq::empty().push(Transition::Complete).push(Transition::Cancel),
        allowed_spec(OrderState::Completed) == Seq::empty().push(Transition::Close),
        allowed_spec(OrderState::Closed) == Seq::<Transition>::empty(),
        allowed_spec(OrderState::Cancelled) == Seq::<Transition>::empty(),
        // 不在表内的 (from, to) 组合一律拒绝（表完备性）：
        forall |s: OrderState, t: Transition| can_apply_spec(s, t) == declared_edge(s, t),
{
    assert(in_seq(allowed_spec(OrderState::Created), Transition::Release));
    assert(in_seq(allowed_spec(OrderState::Completed), Transition::Close));
    assert(!in_seq(allowed_spec(OrderState::Closed), Transition::Close));
}

/// D-03 · 终态吸收。
pub proof fn d03_terminal_absorb()
    ensures
        // Closed/Cancelled 出边空表：
        forall |t: Transition| !in_seq(allowed_spec(OrderState::Closed), t),
        forall |t: Transition| !in_seq(allowed_spec(OrderState::Cancelled), t),
        // is_terminal 恰等于 {Closed, Cancelled}：
        forall |s: OrderState| is_terminal_spec(s) == (s is Closed
            || s is Cancelled),
        // 终态上任何跃迁必被 D-02 拒（吸收态）：
        forall |s: OrderState, t: Transition|
            is_terminal_spec(s) ==> !can_apply_spec(s, t),
{
    assert forall |s: OrderState, t: Transition|
        is_terminal_spec(s) implies !can_apply_spec(s, t) by {
        if is_terminal_spec(s) {
            match s {
                OrderState::Closed => {},
                OrderState::Cancelled => {},
                _ => {},
            }
        }
    }
}

/// D-04 · Cancel 窗口（完成即不可取消）。
pub proof fn d04_cancel_window()
    ensures
        forall |s: OrderState|
            can_apply_spec(s, Transition::Cancel)
                == (s is Created || s is Released
                    || s is InProgress),
{
}

/// D-05 · from_state 全函数性（Cancel 除外）+ (from,to) ∈ 合法边集。
pub proof fn d05_from_state_totality()
    ensures
        forall |t: Transition|
            t != Transition::Cancel ==>
                can_apply_spec(from_state_spec(t), t)
                && declared_edge(from_state_spec(t), t)
                && to_state_spec(t) != from_state_spec(t),
    // Cancel 无单一起点：from_state 在源码为 unreachable!（state.rs:42）——
    // exec 镜像以 requires t != Cancel 排除（结构性债务登记，非正确性缺口；
    // 补偿机制 = can_apply 对 Cancel 的三起点特判 state.rs:71-73，见 d04）。
{
    assert(can_apply_spec(from_state_spec(Transition::Release), Transition::Release));
    assert(can_apply_spec(from_state_spec(Transition::Start), Transition::Start));
    assert(can_apply_spec(from_state_spec(Transition::Complete), Transition::Complete));
    assert(can_apply_spec(from_state_spec(Transition::Close), Transition::Close));
}

/// D-06 · 状态机/门控双轨分离。
/// ① 状态机层只回答边存在性：can_apply 的判定面只有 (s, t)——定义上不触及
///    门控字段（镜像签名即证；order.rs:434 的边查先于 :441 的门控查）；
/// ② 错误不混淆：IllegalTransition ⇔ 边不存在；PreconditionsNotMet ⇔ 边存在
///    ∧ 清单非空（两变体互斥由 transition_exec ensures 对任意输入成立）；
/// ③ Release 门控 = 产线绑定 + 凭证存在且 signed；Close 门控 = 存在合格终态工件；
/// ④ 缺失清单顺序固定：产线在前、凭证在后。
pub proof fn d06_dual_track_separation()
    ensures
        // ③ Release 门控恰为（lb ∧ vp ∧ vs）：
        forall |lb: bool, vp: bool, vs: bool, vid: u64|
            (release_missing_spec(lb, vp, vs, vid).len() == 0) <==> (lb && vp && vs),
        // ③ Close 门控恰为「存在 is_final ∧ qualified 工件」：
        forall |arts: Seq<ArtM>|
            (close_missing_spec(artifacts_qf(arts)).len() == 0) <==> artifacts_qf(arts),
        // ④ 清单顺序固定：含产线缺失则必居首；清单长 2 时次位必为凭证侧缺失：
        forall |lb: bool, vp: bool, vs: bool, vid: u64|
            release_missing_spec(lb, vp, vs, vid).len() <= 2
                && (in_seq_missing(release_missing_spec(lb, vp, vs, vid),
                        MissingM::LineVersionNotBound)
                    ==> release_missing_spec(lb, vp, vs, vid)[0]
                        == MissingM::LineVersionNotBound)
                && (release_missing_spec(lb, vp, vs, vid).len() == 2
                    ==> is_voucher_err(release_missing_spec(lb, vp, vs, vid)[1])),
{
}

/// D-06/D-09 辅助：Missing 清单成员判定。
pub open spec fn in_seq_missing(m: Seq<MissingM>, x: MissingM) -> bool {
    exists |i: int| 0 <= i < m.len() && m[i] == x
}

/// D-06/D-09 辅助：凭证侧缺失变体判定。
pub open spec fn is_voucher_err(x: MissingM) -> bool {
    x is BudgetVoucherNotIssued || x is BudgetVoucherNotSigned
}

/// D-07 · 迁移履历完备性。
/// 保持性载体 = transition_exec ensures（hist_inv 前置 ⇒ 后置，内核验证）；
/// 本引理钉住初态（WorkOrder::new：order.rs:249 state Created / :256 history 空）；
/// 多步链相接的执行面演示见 main。
pub proof fn d07_history_initial()
    ensures
        hist_inv(Seq::<HistoryRecordM>::empty(), OrderState::Created),
{
}

/// D-08① · 状态机无自环 ⇒ 重复施加刚成功的跃迁必拒。
pub proof fn d08_no_self_loop()
    ensures
        // 跃迁表中无自环：任何合法边 (s, t) 的终点异于起点（30 例穷举）：
        forall |s: OrderState, t: Transition|
            can_apply_spec(s, t) ==> to_state_spec(t) != s,
        // 推论：对全部 5 跃迁，can_apply(to_state(t), t) == false（审计同款核验）：
        forall |t: Transition| !can_apply_spec(to_state_spec(t), t),
{
    assert(!can_apply_spec(OrderState::Released, Transition::Release));
    assert(!can_apply_spec(OrderState::InProgress, Transition::Start));
    assert(!can_apply_spec(OrderState::Completed, Transition::Complete));
    assert(!can_apply_spec(OrderState::Closed, Transition::Close));
    assert(!can_apply_spec(OrderState::Cancelled, Transition::Cancel));
}

/// D-09 · release/close 门控内容的确定性。
/// 确定性：release_missing_spec 是 (lb, vp, vs, vid) 的函数（spec 定义即函性）；
/// 实质内容 = 8 组合清单逐一钉死 + 顺序固定（D-06④）+ close 只查存在合格
/// 终态工件（D-06③）+ 门控失败时状态/履历不变（transition_exec ensures 的
/// PreconditionsNotMet 臂，执行面演示见 main）。
pub proof fn d09_gate_determinism()
    ensures
        // (lb, vp, vs) 全组合清单逐一钉死（vp=false 时 vs/vid 不影响）：
        release_missing_spec(false, false, false, 0u64)
            == Seq::empty().push(MissingM::LineVersionNotBound)
                .push(MissingM::BudgetVoucherNotIssued),
        release_missing_spec(false, false, true, 0u64)
            == Seq::empty().push(MissingM::LineVersionNotBound)
                .push(MissingM::BudgetVoucherNotIssued),
        release_missing_spec(false, true, false, 7u64)
            == Seq::empty().push(MissingM::LineVersionNotBound)
                .push(MissingM::BudgetVoucherNotSigned { voucher_id: 7u64 }),
        release_missing_spec(false, true, true, 7u64)
            == Seq::empty().push(MissingM::LineVersionNotBound),
        release_missing_spec(true, false, false, 0u64)
            == Seq::empty().push(MissingM::BudgetVoucherNotIssued),
        release_missing_spec(true, true, false, 7u64)
            == Seq::empty().push(MissingM::BudgetVoucherNotSigned { voucher_id: 7u64 }),
        release_missing_spec(true, true, true, 7u64) == Seq::<MissingM>::empty(),
        // 凭证缺席时签名位/凭证号不影响清单（确定性与相关输入面一致）：
        forall |lb: bool, vs1: bool, vs2: bool, v1: u64, v2: u64|
            release_missing_spec(lb, false, vs1, v1) == release_missing_spec(lb, false, vs2, v2),
{
}

// ═══════════════════ 执行面场景（内核验证的 exec 演示） ═══════════════════

// ═══════════════════ 执行面场景（内核验证的 exec 演示，按条拆分上下文） ═══════════════════

#[verifier::rlimit(100)]
fn scenario_lifecycle_and_history() {
    // 成功链 Created→Released→InProgress→Completed→Closed + D-07 履历 + D-08① 重复施加必拒
    let empty_arts: Vec<ArtM> = Vec::new();
    let mut history: Vec<HistoryRecordM> = Vec::new();

    // WorkOrder::new 镜像（order.rs:249/:256）：state = Created，history 空。
    let mut s = OrderState::Created;
    proof {
        d07_history_initial();
    }

    let (s1, r1) = transition_exec(s, Transition::Release, true, true, true, 0u64, &empty_arts, &mut history);
    match r1 {
        Result::Ok(_) => {},
        Result::Err(_) => assert(false),
    }
    s = s1;
    assert(s == OrderState::Released);
    assert(history.len() == 1);
    assert(history[0].from == OrderState::Created && history[0].to == OrderState::Released);

    let (s2, r2) = transition_exec(s, Transition::Start, false, false, false, 0u64, &empty_arts, &mut history);
    match r2 {
        Result::Ok(_) => {},
        Result::Err(_) => assert(false),
    }
    s = s2;
    assert(s == OrderState::InProgress);

    let (s3, r3) = transition_exec(s, Transition::Complete, false, false, false, 0u64, &empty_arts, &mut history);
    match r3 {
        Result::Ok(_) => {},
        Result::Err(_) => assert(false),
    }
    s = s3;
    assert(s == OrderState::Completed);
    assert(history.len() == 3);

    // ── D-08①：对 Completed 重复施加刚成功的 Complete 必拒 ──
    let (s_dup, r_dup) = transition_exec(s, Transition::Complete, false, false, false, 0u64, &empty_arts, &mut history);
    match r_dup {
        Result::Err(GateErrM::IllegalTransition { from, to, allowed }) => {
            assert(from == OrderState::Completed); // from == to：自环被拒
            assert(to == OrderState::Completed);
            assert(allowed@ == allowed_spec(OrderState::Completed));
            assert(allowed.len() == 1);
        },
        _ => assert(false),
    }
    assert(s_dup == OrderState::Completed); // 状态不落地（D-02 后件）
    assert(history.len() == 3); // 履历不追加（D-07 拒绝半边）
    s = s_dup;

    // ── 挂合格终态工件 → Close 成功（D-06 close 门控存在侧）──
    let mut qf_arts: Vec<ArtM> = Vec::new();
    qf_arts.push(ArtM { is_final: true, qualified: true });
    proof {
        assert(qf_arts@[0].is_final && qf_arts@[0].qualified); // ∃ witness = 0
        assert(artifacts_qf(qf_arts@));
    }
    let (s4, r4) = transition_exec(s, Transition::Close, false, false, false, 0u64, &qf_arts, &mut history);
    match r4 {
        Result::Ok(_) => {},
        Result::Err(_) => assert(false),
    }
    s = s4;
    assert(s == OrderState::Closed);
    assert(history.len() == 4);

    // ── D-07：链相接 + 长度 = 成功迁移次数（4）──
    assert(history[0].from == OrderState::Created && history[0].to == OrderState::Released);
    assert(history[1].from == OrderState::Released && history[1].to == OrderState::InProgress);
    assert(history[2].from == OrderState::InProgress && history[2].to == OrderState::Completed);
    assert(history[3].from == OrderState::Completed && history[3].to == OrderState::Closed);
    assert(history[0].to == history[1].from && history[1].to == history[2].from
        && history[2].to == history[3].from);
    assert(hist_inv(history@, s));
}

fn scenario_terminal_absorb() {
    // D-03：终态吸收 + D-04：Cancel 窗口
    let empty_arts: Vec<ArtM> = Vec::new();
    let mut history: Vec<HistoryRecordM> = Vec::new();
    let s = OrderState::Closed;
    let term_closed = is_terminal_exec(OrderState::Closed);
    assert(term_closed);
    let (s_t1, r_t1) = transition_exec(s, Transition::Start, false, false, false, 0u64, &empty_arts, &mut history);
    match r_t1 {
        Result::Err(GateErrM::IllegalTransition { from, to, allowed }) => {
            assert(from == OrderState::Closed);
            assert(to == OrderState::InProgress);
            assert(allowed.len() == 0); // 空表
        },
        _ => assert(false),
    }
    assert(s_t1 == OrderState::Closed && history.len() == 0);

    let ca_c = can_apply_exec(OrderState::Created, Transition::Cancel);
    let ca_r = can_apply_exec(OrderState::Released, Transition::Cancel);
    let ca_i = can_apply_exec(OrderState::InProgress, Transition::Cancel);
    let ca_p = can_apply_exec(OrderState::Completed, Transition::Cancel);
    let ca_x = can_apply_exec(OrderState::Closed, Transition::Cancel);
    let ca_n = can_apply_exec(OrderState::Cancelled, Transition::Cancel);
    assert(ca_c && ca_r && ca_i);
    assert(!ca_p && !ca_x && !ca_n); // 完成即不可取消
}

fn scenario_rejections_and_gates() {
    // D-02 代表例 + D-06/D-09 门控拒绝臂（错误不混淆 + 状态/履历不动）
    let empty_arts: Vec<ArtM> = Vec::new();
    let mut history: Vec<HistoryRecordM> = Vec::new();

    // Created 上 Start 拒；allowed 表逐字
    let (s_i1, r_i1) = transition_exec(OrderState::Created, Transition::Start, false, false, false, 0u64, &empty_arts, &mut history);
    match r_i1 {
        Result::Err(GateErrM::IllegalTransition { from, to, allowed }) => {
            assert(from == OrderState::Created);
            assert(to == OrderState::InProgress);
            assert(allowed.len() == 2);
        },
        _ => assert(false),
    }
    assert(s_i1 == OrderState::Created);

    // 边存在但门控不满足 ⇒ PreconditionsNotMet（不是 IllegalTransition）
    let mut history2: Vec<HistoryRecordM> = Vec::new();
    let (s_g1, r_g1) = transition_exec(OrderState::Created, Transition::Release, false, false, false, 5u64, &empty_arts, &mut history2);
    match r_g1 {
        Result::Err(GateErrM::PreconditionsNotMet { transition: tt, missing }) => {
            assert(tt == Transition::Release);
            assert(missing.len() == 2); // 产线在前、凭证在后
            assert(missing[0] == MissingM::LineVersionNotBound);
            assert(missing[1] == MissingM::BudgetVoucherNotIssued);
        },
        _ => assert(false),
    }
    assert(s_g1 == OrderState::Created && history2.len() == 0);

    // 未签名凭证 ⇒ BudgetVoucherNotSigned{voucher_id}（order.rs:412-414）
    let (s_g2, r_g2) = transition_exec(OrderState::Created, Transition::Release, true, true, false, 42u64, &empty_arts, &mut history2);
    match r_g2 {
        Result::Err(GateErrM::PreconditionsNotMet { missing, .. }) => {
            assert(missing.len() == 1);
            assert(missing[0] == MissingM::BudgetVoucherNotSigned { voucher_id: 42u64 });
        },
        _ => assert(false),
    }

    // close 门控：无合格终态工件 ⇒ PreconditionsNotMet（order.rs:420-427）
    let mut bad_arts: Vec<ArtM> = Vec::new();
    bad_arts.push(ArtM { is_final: true, qualified: false });
    let mut qfhist: Vec<HistoryRecordM> = Vec::new();
    let (s_g3, r_g3) = transition_exec(OrderState::Completed, Transition::Close, false, false, false, 0u64, &bad_arts, &mut qfhist);
    match r_g3 {
        Result::Err(GateErrM::PreconditionsNotMet { missing, .. }) => {
            assert(missing.len() == 1);
            assert(missing[0] == MissingM::NoQualifiedFinalArtifact);
        },
        _ => assert(false),
    }
    assert(s_g3 == OrderState::Completed && qfhist.len() == 0);
}

fn scenario_exec_slices() {
    // D-05 from_state + D-01 出边表 + D-09 清单确定性（执行面抽查）
    let fs_r = from_state_exec(Transition::Release);
    let fs_s = from_state_exec(Transition::Start);
    let fs_c = from_state_exec(Transition::Complete);
    let fs_l = from_state_exec(Transition::Close);
    assert(fs_r == OrderState::Created);
    assert(fs_s == OrderState::Released);
    assert(fs_c == OrderState::InProgress);
    assert(fs_l == OrderState::Completed);

    let at_c = allowed_transitions_exec(OrderState::Created);
    let at_p = allowed_transitions_exec(OrderState::Completed);
    let at_x = allowed_transitions_exec(OrderState::Closed);
    let at_n = allowed_transitions_exec(OrderState::Cancelled);
    assert(at_c.len() == 2);
    assert(at_p.len() == 1);
    assert(at_x.len() == 0);
    assert(at_n.len() == 0);

    let m1 = release_missing_exec(false, false, false, 0u64);
    assert(m1.len() == 2);
    let m2 = release_missing_exec(true, true, true, 0u64);
    assert(m2.len() == 0);
    let m3 = release_missing_exec(false, true, true, 0u64);
    assert(m3.len() == 1 && m3[0] == MissingM::LineVersionNotBound);

    let mut qf_arts: Vec<ArtM> = Vec::new();
    qf_arts.push(ArtM { is_final: true, qualified: true });
    let mut bad_arts: Vec<ArtM> = Vec::new();
    bad_arts.push(ArtM { is_final: true, qualified: false });
    proof {
        assert(qf_arts@[0].is_final && qf_arts@[0].qualified); // ∃ witness = 0
        assert(artifacts_qf(qf_arts@));
        assert(bad_arts@[0].is_final && !bad_arts@[0].qualified);
        assert(!artifacts_qf(bad_arts@));
    }
    let cm1 = close_missing_exec(&qf_arts);
    assert(cm1.len() == 0);
    let cm2 = close_missing_exec(&bad_arts);
    assert(cm2.len() == 1 && cm2[0] == MissingM::NoQualifiedFinalArtifact);
}

fn main() {
    // 全部 D 组引理（proof 层全域陈述，内核裁决）
    proof {
        d01_table_is_declared_lifecycle();
        d03_terminal_absorb();
        d04_cancel_window();
        d05_from_state_totality();
        d06_dual_track_separation();
        d07_history_initial();
        d08_no_self_loop();
        d09_gate_determinism();
    }
    scenario_lifecycle_and_history();
    scenario_terminal_absorb();
    scenario_rejections_and_gates();
    scenario_exec_slices();
}

}

