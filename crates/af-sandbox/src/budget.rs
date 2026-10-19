//! 沙箱并发维的预算决策。
//!
//! 扣不扣由调用方在 `exec` 之前决定（通常是 af-meter 的 `Concurrency::acquire`）。
//! 本 crate 只要求看到一份类型化决策，不调用 meter，也不把拒绝折成布尔值后再丢掉原因。
//!
//! 映射（调用方自己做，这里不执行）：
//! - [`BudgetDimension::SandboxConcurrency`] ↔ `af_meter::Meter::Concurrency`
//! - `slots` ↔ 占用的槽位数（`acquire` 一次占一个槽时就是 1）
//! - `decided_at` / `release_at` ↔ 调用方注入的时刻。单位由调用方与 meter 约定。
//!   它们不是本 crate 的超时毫秒，本 crate 不在这两个数之间睡眠，也不替调用方 `release`。
//! - [`BudgetDecision::Refused`] ↔ `MeterError::Insufficient` 的 available / recovers_at

use crate::error::{Result, SandboxError};

/// 预算维度。exec 只接受 [`BudgetDimension::SandboxConcurrency`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetDimension {
    /// 同时存活的沙箱执行槽。
    SandboxConcurrency,
    /// 调用方交来的其它维。原样拒绝，不改写成并发维。
    Other(&'static str),
}

/// 调用方声明已经扣下的并发槽。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcurrencyCharge {
    pub dimension: BudgetDimension,
    /// 已占用槽位。exec 要求至少 1。0 不是一次扣费。
    pub slots: u64,
    /// 做决定时的注入时刻。
    pub decided_at: u64,
    /// 调用方计划归还槽位的时刻。本 crate 到点不归还。
    pub release_at: u64,
}

/// 调用方决定不扣或扣不下来。字段与 meter 的不足读数对齐，本 crate 不解释恢复策略。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetRefusal {
    pub dimension: BudgetDimension,
    pub need: u64,
    pub available: u64,
    pub recovers_at: Option<u64>,
}

/// exec 之前的预算结论。两个变体都要能原样回到错误里。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetDecision {
    Charged(ConcurrencyCharge),
    Refused(BudgetRefusal),
}

/// `decide_concurrency` 交给钩子的请求。钩子可以忽略它并返回别的决策，exec 会拒绝那份决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConcurrencyRequest {
    pub dimension: BudgetDimension,
    pub slots: u64,
    pub now: u64,
    pub release_at: u64,
}

/// 调用方的扣费钩子。实现里才接触 meter。
///
/// [`decide_concurrency`] 调用它。[`crate::Session::exec`] 不再调用它，避免同一次 exec 扣两次，
/// 也避免把扣费藏进执行路径。
pub trait ConcurrencyBudget {
    fn decide(&mut self, request: &ConcurrencyRequest) -> BudgetDecision;
}

/// 向钩子要一份沙箱并发维决策。不碰沙箱后端，不读时钟，不睡眠。
///
/// `now` 与 `release_at` 原样写进请求，由钩子决定是否写进 [`ConcurrencyCharge`]。
///
/// # Examples
/// ```
/// use af_sandbox::{
///     decide_concurrency, BudgetDecision, BudgetRefusal, ConcurrencyBudget, ConcurrencyRequest,
/// };
///
/// struct Deny;
/// impl ConcurrencyBudget for Deny {
///     fn decide(&mut self, request: &ConcurrencyRequest) -> BudgetDecision {
///         BudgetDecision::Refused(BudgetRefusal {
///             dimension: request.dimension,
///             need: request.slots,
///             available: 0,
///             recovers_at: None,
///         })
///     }
/// }
///
/// assert!(matches!(
///     decide_concurrency(&mut Deny, 0, 1),
///     BudgetDecision::Refused(_)
/// ));
/// ```
pub fn decide_concurrency(
    budget: &mut dyn ConcurrencyBudget,
    now: u64,
    release_at: u64,
) -> BudgetDecision {
    budget.decide(&ConcurrencyRequest {
        dimension: BudgetDimension::SandboxConcurrency,
        slots: 1,
        now,
        release_at,
    })
}

/// 只有“沙箱并发维且至少一槽”的 [`BudgetDecision::Charged`] 能进入 exec。
///
/// 其它决策（含拒绝、零槽、其它维）原样放进 [`SandboxError::BudgetRejected`]。
pub fn accept_charge(decision: &BudgetDecision) -> Result<ConcurrencyCharge> {
    match decision {
        BudgetDecision::Charged(charge)
            if charge.dimension == BudgetDimension::SandboxConcurrency && charge.slots >= 1 =>
        {
            Ok(charge.clone())
        }
        _ => Err(SandboxError::BudgetRejected {
            decision: decision.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn charge(slots: u64, dimension: BudgetDimension) -> BudgetDecision {
        BudgetDecision::Charged(ConcurrencyCharge {
            dimension,
            slots,
            decided_at: 1,
            release_at: 2,
        })
    }

    #[test]
    fn accepts_sandbox_concurrency_slot() {
        let decision = charge(2, BudgetDimension::SandboxConcurrency);
        let got = accept_charge(&decision).unwrap();
        assert_eq!(got.slots, 2);
        assert_eq!(got.decided_at, 1);
        assert_eq!(got.release_at, 2);
    }

    #[test]
    fn refusal_is_returned_unchanged() {
        let decision = BudgetDecision::Refused(BudgetRefusal {
            dimension: BudgetDimension::SandboxConcurrency,
            need: 1,
            available: 0,
            recovers_at: Some(9),
        });
        let err = accept_charge(&decision).unwrap_err();
        match err {
            SandboxError::BudgetRejected { decision: echoed } => assert_eq!(echoed, decision),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn wrong_dimension_and_zero_slots_are_not_rewritten() {
        for decision in [
            charge(1, BudgetDimension::Other("calls")),
            charge(0, BudgetDimension::SandboxConcurrency),
        ] {
            let err = accept_charge(&decision).unwrap_err();
            match err {
                SandboxError::BudgetRejected { decision: echoed } => assert_eq!(echoed, decision),
                other => panic!("unexpected {other:?}"),
            }
        }
    }
}
