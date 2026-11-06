//! 按轮扣预算。规格只带维度引用；账本由调用方注入。
//!
//! `cents` 走 [`af_meter::BudgetLedger::try_spend`]。
//! `count` 走 [`af_meter::Meter::try_consume`]。时刻是注入墙钟的整秒。
//! 拒绝时账本不变，错误是 [`BudgetFault`]，字段与 af-meter 的类型化拒绝一一对应。

use crate::error::{Error, Result};
use crate::spec::{BudgetUnit, SPEC_VERSION};
use af_meter::{BudgetError, BudgetLedger, Meter, MeterError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 调用方注入的预算。维度名必须与规格里的 `budget_ref.dimension` 相同。
#[derive(Debug, Clone)]
pub enum BudgetBinding {
    Cents {
        dimension: String,
        ledger: BudgetLedger,
        spend_cap_cents: i64,
    },
    Count {
        dimension: String,
        meter: Meter,
    },
}

impl BudgetBinding {
    pub fn dimension(&self) -> &str {
        match self {
            BudgetBinding::Cents { dimension, .. } | BudgetBinding::Count { dimension, .. } => {
                dimension
            }
        }
    }

    pub fn unit(&self) -> BudgetUnit {
        match self {
            BudgetBinding::Cents { .. } => BudgetUnit::Cents,
            BudgetBinding::Count { .. } => BudgetUnit::Count,
        }
    }
}

/// 落库的预算：声明（首启时的账本）和当前值分开。
///
/// 续跑时调用方再递交的绑定必须等于声明，当前余额以库里的为准。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BudgetRecord {
    pub v: u32,
    pub dimension: String,
    pub unit: BudgetUnit,
    pub spend_cap_cents: i64,
    pub declared: Value,
    pub current: Value,
}

impl BudgetRecord {
    pub fn fresh(binding: &BudgetBinding) -> Result<Self> {
        let (spend_cap_cents, declared) = match binding {
            BudgetBinding::Cents {
                ledger,
                spend_cap_cents,
                ..
            } => (*spend_cap_cents, serde_json::to_value(ledger)?),
            BudgetBinding::Count { meter, .. } => (0, serde_json::to_value(meter)?),
        };
        Ok(Self {
            v: SPEC_VERSION,
            dimension: binding.dimension().to_string(),
            unit: binding.unit(),
            spend_cap_cents,
            current: declared.clone(),
            declared,
        })
    }

    pub fn matches_launch(&self, binding: &BudgetBinding) -> Result<()> {
        if self.v != SPEC_VERSION {
            return Err(Error::DeclarationMismatch(format!(
                "预算记录版本 {} 不是 {SPEC_VERSION}",
                self.v
            )));
        }
        if binding.dimension() != self.dimension || binding.unit() != self.unit {
            return Err(Error::DeclarationMismatch(format!(
                "预算维度 {}/{:?} 与已声明的 {}/{:?} 不符",
                binding.dimension(),
                binding.unit(),
                self.dimension,
                self.unit
            )));
        }
        let (spend_cap, declared) = match binding {
            BudgetBinding::Cents {
                ledger,
                spend_cap_cents,
                ..
            } => (*spend_cap_cents, serde_json::to_value(ledger)?),
            BudgetBinding::Count { meter, .. } => (0, serde_json::to_value(meter)?),
        };
        if spend_cap != self.spend_cap_cents || declared != self.declared {
            return Err(Error::DeclarationMismatch(
                "预算声明与首启时的账本不一致".into(),
            ));
        }
        Ok(())
    }

    pub fn state(&self) -> Result<BudgetState> {
        match self.unit {
            BudgetUnit::Cents => {
                let ledger = serde_json::from_value(self.current.clone())?;
                Ok(BudgetState {
                    kind: Kind::Cents {
                        ledger,
                        spend_cap_cents: self.spend_cap_cents,
                    },
                })
            }
            BudgetUnit::Count => {
                let meter = serde_json::from_value(self.current.clone())?;
                Ok(BudgetState {
                    kind: Kind::Count { meter },
                })
            }
        }
    }

    pub fn write_current(&mut self, state: &BudgetState) -> Result<()> {
        self.current = state.snapshot()?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
enum Kind {
    Cents {
        ledger: BudgetLedger,
        spend_cap_cents: i64,
    },
    Count {
        meter: Meter,
    },
}

/// 本轮可改的账本。
#[derive(Debug, Clone)]
pub struct BudgetState {
    kind: Kind,
}

impl BudgetState {
    /// 不改当前账本。够扣才返回 `Ok`。
    pub fn probe(&self, per_turn: u64, now_ms: u64) -> std::result::Result<(), BudgetFault> {
        self.clone().charge(per_turn, now_ms)
    }

    pub fn charge(&mut self, per_turn: u64, now_ms: u64) -> std::result::Result<(), BudgetFault> {
        match &mut self.kind {
            Kind::Cents {
                ledger,
                spend_cap_cents,
            } => {
                let spend = i64::try_from(per_turn).map_err(|_| {
                    BudgetFault::Unsupported("per_turn 超出 i64，不能当成美分".into())
                })?;
                ledger
                    .try_spend(spend, *spend_cap_cents)
                    .map_err(BudgetFault::from)
            }
            Kind::Count { meter } => {
                let now = now_ms / 1000;
                meter.try_consume(per_turn, now).map_err(BudgetFault::from)
            }
        }
    }

    fn snapshot(&self) -> Result<Value> {
        match &self.kind {
            Kind::Cents { ledger, .. } => Ok(serde_json::to_value(ledger)?),
            Kind::Count { meter } => Ok(serde_json::to_value(meter)?),
        }
    }
}

/// af-meter 拒绝的原样映射，方便落库和断言。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum BudgetFault {
    #[error("spend_cents {spend_cents} exceeds spend_cap_cents {spend_cap_cents}")]
    OverCap {
        spend_cents: i64,
        spend_cap_cents: i64,
    },
    #[error("spend_cents {spend_cents} exceeds remaining_cents {remaining_cents}")]
    OverBudget {
        spend_cents: i64,
        remaining_cents: i64,
    },
    #[error("spend_cents {spend_cents} is negative")]
    NegativeSpend { spend_cents: i64 },
    #[error("insufficient: need={need}, available={available}, recovers_at={recovers_at:?}")]
    InsufficientCount {
        need: u64,
        available: u64,
        recovers_at: Option<u64>,
    },
    #[error("{0}")]
    Unsupported(String),
}

impl From<BudgetError> for BudgetFault {
    fn from(err: BudgetError) -> Self {
        match err {
            BudgetError::OverCap {
                spend_cents,
                spend_cap_cents,
            } => BudgetFault::OverCap {
                spend_cents,
                spend_cap_cents,
            },
            BudgetError::OverBudget {
                spend_cents,
                remaining_cents,
            } => BudgetFault::OverBudget {
                spend_cents,
                remaining_cents,
            },
            BudgetError::NegativeSpend { spend_cents } => {
                BudgetFault::NegativeSpend { spend_cents }
            }
            BudgetError::BudgetMismatch { .. } => BudgetFault::Unsupported("预算合并不一致".into()),
        }
    }
}

impl From<MeterError> for BudgetFault {
    fn from(err: MeterError) -> Self {
        match err {
            MeterError::Insufficient {
                need,
                available,
                recovers_at,
            } => BudgetFault::InsufficientCount {
                need,
                available,
                recovers_at: recovers_at.map(|ticks| ticks.get()),
            },
            MeterError::Unsupported(message) => BudgetFault::Unsupported(message.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_meter::Meter;

    #[test]
    fn count_probe_stops_when_stock_is_short_and_declared_budget_must_match() {
        let binding = BudgetBinding::Count {
            dimension: "llm.count".into(),
            meter: Meter::stock(1),
        };
        let mut record = BudgetRecord::fresh(&binding).unwrap();
        record.matches_launch(&binding).unwrap();
        let mut state = record.state().unwrap();
        state.charge(1, 5_000).unwrap();
        let err = state.probe(1, 5_000).unwrap_err();
        assert!(matches!(
            err,
            BudgetFault::InsufficientCount {
                need: 1,
                available: 0,
                recovers_at: None
            }
        ));
        record.write_current(&state).unwrap();
        let drifted = BudgetBinding::Count {
            dimension: "llm.count".into(),
            meter: Meter::stock(2),
        };
        assert!(matches!(
            record.matches_launch(&drifted).unwrap_err(),
            Error::DeclarationMismatch(_)
        ));
    }
}
