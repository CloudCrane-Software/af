//! 累计金额账本。
//!
//! 词与 af-credential 对齐，类型都是 [`Cents`]（i64 美分）：
//! - `budget_cents`：账本总额，可原样取凭证事实 `budget_cents`。
//! - `spend_cents`：一次消耗，与验证方注入凭证的 `spend_cents` 是同一个整数。
//! - `spend_cap_cents`：本次 per-call 上限，与衰减参数 `spend_cap_cents` 是同一个整数。
//!
//! 凭证上的 check 是 per-call 的：`spend_cents <= spend_cap_cents`，根块还有
//! `spend_cents <= budget_cents`。累计是否超出账本由 [`BudgetLedger::try_spend`] 拒绝。
//! 比较用的是同一个美分整数，调用方不做单位换算。
//!
//! 多个消费者各自记自己的账，事后用 [`BudgetLedger::merge_consumers`] 把已花相加。
//! 这不是 [`crate::Meter`] 的并发访问：计量树仍是单消费者状态机。

use serde::{Deserialize, Serialize};

use crate::Cents;

/// 账本拒绝原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BudgetError {
    /// `spend_cents > spend_cap_cents`。这是凭证 check `spend_cents <= spend_cap_cents` 的否定。
    #[error("spend_cents {spend_cents} exceeds spend_cap_cents {spend_cap_cents}")]
    OverCap {
        spend_cents: Cents,
        spend_cap_cents: Cents,
    },
    /// 累计后将超过 `budget_cents`。凭证不做这项检查。
    #[error("spend_cents {spend_cents} exceeds remaining_cents {remaining_cents}")]
    OverBudget {
        spend_cents: Cents,
        remaining_cents: Cents,
    },
    /// 消耗必须是非负美分。退款不是一次 spend。
    #[error("spend_cents {spend_cents} is negative")]
    NegativeSpend { spend_cents: Cents },
    /// 合并的账本总额不一致。`low` / `high` 是其中最小与最大的总额，与切片顺序无关。
    #[error("budget_cents mismatch: low {low} vs high {high}")]
    BudgetMismatch { low: Cents, high: Cents },
}

/// 累计预算真相源。凭证只携带 per-call 上限，不管累计消耗。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetLedger {
    /// 账本总额（美分）。
    pub budget_cents: Cents,
    /// 已累计消耗（美分）。凭证里没有这个数。
    pub spent_cents: Cents,
}

impl BudgetLedger {
    /// `budget_cents` 与凭证字段同型，直接传入签发值即可。
    pub fn new(budget_cents: Cents) -> Self {
        Self {
            budget_cents,
            spent_cents: 0,
        }
    }

    /// 尚未花掉的美分。`budget_cents - spent_cents`，下溢时停在 `i64::MIN`。
    pub fn remaining_cents(&self) -> Cents {
        self.budget_cents.saturating_sub(self.spent_cents)
    }

    /// 记一笔消耗。
    ///
    /// 拒绝条件：
    /// - `spend_cents < 0`
    /// - `spend_cents > spend_cap_cents`（与凭证 per-call check 同一比较）
    /// - `spent_cents + spend_cents > budget_cents`（累计，凭证不检查）
    ///
    /// 拒绝时账本不变。
    pub fn try_spend(
        &mut self,
        spend_cents: Cents,
        spend_cap_cents: Cents,
    ) -> Result<(), BudgetError> {
        if spend_cents < 0 {
            return Err(BudgetError::NegativeSpend { spend_cents });
        }
        if spend_cents > spend_cap_cents {
            return Err(BudgetError::OverCap {
                spend_cents,
                spend_cap_cents,
            });
        }
        let Some(next) = self.spent_cents.checked_add(spend_cents) else {
            return Err(BudgetError::OverBudget {
                spend_cents,
                remaining_cents: self.remaining_cents(),
            });
        };
        if next > self.budget_cents {
            return Err(BudgetError::OverBudget {
                spend_cents,
                remaining_cents: self.remaining_cents(),
            });
        }
        self.spent_cents = next;
        Ok(())
    }

    /// 合并多个消费者各自累计的账本。
    ///
    /// 每个消费者在自己的账本上调用 [`Self::try_spend`]。合并只把 `spent_cents` 相加，
    /// 不重放消耗顺序，因此结果与切片顺序无关：
    /// - 空切片 → `budget_cents = 0`、`spent_cents = 0`；
    /// - 总额不一致 → [`BudgetError::BudgetMismatch`]（`low` / `high` 取最小与最大）；
    /// - 总额一致但有负数 `spent_cents` → [`BudgetError::NegativeSpend`]（取最小的负数）；
    /// - 已花之和超过 `i64`，或和大于总额且和不为 0 → [`BudgetError::OverBudget`]。
    ///   `spend_cents` 是这个和（超出 `i64` 时钳在 `i64::MAX`），`remaining_cents` 是共同总额；
    /// - 否则得到同一总额，`spent_cents` 为和。
    ///
    /// 和为 0 时允许总额为负：没有任何已花可合并，负数余额来自总额本身
    /// （[`Self::remaining_cents`] 的 `saturating_sub`），不是扣出来的透支。
    /// 入参不修改。
    pub fn merge_consumers(ledgers: &[Self]) -> Result<Self, BudgetError> {
        let Some(first) = ledgers.first() else {
            return Ok(Self::new(0));
        };

        let mut low = first.budget_cents;
        let mut high = first.budget_cents;
        let mut min_spent = first.spent_cents;
        let mut sum: i128 = 0;
        for ledger in ledgers {
            low = low.min(ledger.budget_cents);
            high = high.max(ledger.budget_cents);
            min_spent = min_spent.min(ledger.spent_cents);
            sum += i128::from(ledger.spent_cents);
        }
        if low != high {
            return Err(BudgetError::BudgetMismatch { low, high });
        }
        if min_spent < 0 {
            return Err(BudgetError::NegativeSpend {
                spend_cents: min_spent,
            });
        }

        let budget = low;
        // 已花都非负，和也非负。和为 0 时没有新的消耗，负总额保持原样。
        // 和超出 i64 时一定大于任何总额，同一条 OverBudget 就能接住。
        if sum != 0 && sum > i128::from(budget) {
            return Err(BudgetError::OverBudget {
                spend_cents: i64::try_from(sum).unwrap_or(i64::MAX),
                remaining_cents: budget,
            });
        }
        let Ok(spent_cents) = i64::try_from(sum) else {
            return Err(BudgetError::OverBudget {
                spend_cents: i64::MAX,
                remaining_cents: budget,
            });
        };
        Ok(Self {
            budget_cents: budget,
            spent_cents,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spends_exact_cap_and_exact_remaining() {
        let mut ledger = BudgetLedger::new(1_000);
        ledger.try_spend(500, 500).unwrap();
        assert_eq!(ledger.spent_cents, 500);
        assert_eq!(ledger.remaining_cents(), 500);
        ledger.try_spend(500, 500).unwrap();
        assert_eq!(ledger.spent_cents, 1_000);
        assert_eq!(ledger.remaining_cents(), 0);
    }

    #[test]
    fn over_cap_does_not_mutate() {
        let mut ledger = BudgetLedger::new(1_000);
        let err = ledger.try_spend(800, 500).unwrap_err();
        assert_eq!(
            err,
            BudgetError::OverCap {
                spend_cents: 800,
                spend_cap_cents: 500,
            }
        );
        assert_eq!(ledger.spent_cents, 0);
        assert_eq!(ledger.remaining_cents(), 1_000);
    }

    #[test]
    fn over_budget_does_not_mutate() {
        let mut ledger = BudgetLedger::new(100);
        let err = ledger.try_spend(300, 500).unwrap_err();
        assert_eq!(
            err,
            BudgetError::OverBudget {
                spend_cents: 300,
                remaining_cents: 100,
            }
        );
        assert_eq!(ledger.spent_cents, 0);
    }

    #[test]
    fn negative_spend_does_not_mutate() {
        let mut ledger = BudgetLedger::new(1_000);
        let err = ledger.try_spend(-1, 500).unwrap_err();
        assert_eq!(err, BudgetError::NegativeSpend { spend_cents: -1 });
        assert_eq!(ledger.spent_cents, 0);
    }
}
