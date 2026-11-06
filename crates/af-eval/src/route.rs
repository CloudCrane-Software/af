//! 四失败码。一次运行只落一个码。
//!
//! 优先级：超时，然后装置，然后基准，然后被评对象。
//! 高优先级命中后不再看低的，避免把装置故障记成对象失败，或把超时记成答错。
//! 污染停机不是这四个码：它在开跑前就结束，不产生成绩。

use std::fmt;

use crate::error::{EvalError, Result};

/// 评测失败的语义分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCode {
    /// 测量装置故障（库、执行器、环境）。不是被评对象的成绩。
    Infra,
    /// 被评对象做出了可判定的失败。
    Subject,
    /// 基准或裁决器不可用。不是污染停机。
    Bench,
    /// 注入时钟上的耗时达到预算。
    Timeout,
}

impl FailureCode {
    /// 账本里的稳定记号。
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Infra => "E_INFRA",
            Self::Subject => "E_SUBJECT",
            Self::Bench => "E_BENCH",
            Self::Timeout => "E_TIMEOUT",
        }
    }

    pub fn parse(code: &str) -> Option<Self> {
        match code {
            "E_INFRA" => Some(Self::Infra),
            "E_SUBJECT" => Some(Self::Subject),
            "E_BENCH" => Some(Self::Bench),
            "E_TIMEOUT" => Some(Self::Timeout),
            _ => None,
        }
    }
}

impl fmt::Display for FailureCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_code())
    }
}

/// 执行器交回来的故障位。全假且未超时就是通过。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExecSignals {
    pub infra: bool,
    pub bench: bool,
    pub subject: bool,
}

/// [`route_failure`] 的结果。通过与失败互斥。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classified {
    Passed,
    Failed(FailureCode),
}

/// 用注入时刻路由。`now_ms - started_at_ms >= budget_ms` 即为超时，不睡眠。
pub fn route_failure(
    started_at_ms: i64,
    now_ms: i64,
    budget_ms: i64,
    signals: ExecSignals,
) -> Result<Classified> {
    if budget_ms <= 0 {
        return Err(EvalError::BadBudget);
    }
    if now_ms < started_at_ms {
        return Err(EvalError::ClockRegressed {
            started_at_ms,
            now_ms,
        });
    }
    let Some(elapsed) = now_ms.checked_sub(started_at_ms) else {
        return Err(EvalError::Corrupt {
            detail: "运行耗时溢出".into(),
        });
    };
    if elapsed >= budget_ms {
        return Ok(Classified::Failed(FailureCode::Timeout));
    }
    if signals.infra {
        return Ok(Classified::Failed(FailureCode::Infra));
    }
    if signals.bench {
        return Ok(Classified::Failed(FailureCode::Bench));
    }
    if signals.subject {
        return Ok(Classified::Failed(FailureCode::Subject));
    }
    Ok(Classified::Passed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_signal_maps_to_its_own_code() {
        let start = 1_000;
        let budget = 50;
        let now = start + 10;
        assert_eq!(
            route_failure(start, now, budget, ExecSignals::default()).unwrap(),
            Classified::Passed
        );
        assert_eq!(
            route_failure(
                start,
                now,
                budget,
                ExecSignals {
                    infra: true,
                    ..ExecSignals::default()
                }
            )
            .unwrap(),
            Classified::Failed(FailureCode::Infra)
        );
        assert_eq!(
            route_failure(
                start,
                now,
                budget,
                ExecSignals {
                    bench: true,
                    ..ExecSignals::default()
                }
            )
            .unwrap(),
            Classified::Failed(FailureCode::Bench)
        );
        assert_eq!(
            route_failure(
                start,
                now,
                budget,
                ExecSignals {
                    subject: true,
                    ..ExecSignals::default()
                }
            )
            .unwrap(),
            Classified::Failed(FailureCode::Subject)
        );
        assert_eq!(
            route_failure(start, start + budget, budget, ExecSignals::default()).unwrap(),
            Classified::Failed(FailureCode::Timeout)
        );
    }

    #[test]
    fn priority_does_not_collapse_codes() {
        let start = 0;
        let budget = 20;
        let within = ExecSignals {
            infra: true,
            bench: true,
            subject: true,
        };
        assert_eq!(
            route_failure(start, 5, budget, within).unwrap(),
            Classified::Failed(FailureCode::Infra)
        );
        assert_eq!(
            route_failure(
                start,
                5,
                budget,
                ExecSignals {
                    infra: false,
                    bench: true,
                    subject: true,
                }
            )
            .unwrap(),
            Classified::Failed(FailureCode::Bench)
        );
        assert_eq!(
            route_failure(
                start,
                budget,
                budget,
                ExecSignals {
                    infra: true,
                    bench: true,
                    subject: true,
                }
            )
            .unwrap(),
            Classified::Failed(FailureCode::Timeout)
        );
        assert_eq!(FailureCode::Infra.as_code(), "E_INFRA");
        assert_eq!(FailureCode::parse("E_SUBJECT"), Some(FailureCode::Subject));
        assert_eq!(FailureCode::parse("E_BENCH"), Some(FailureCode::Bench));
        assert_eq!(FailureCode::parse("E_TIMEOUT"), Some(FailureCode::Timeout));
        assert_eq!(FailureCode::parse("E_OTHER"), None);
    }

    #[test]
    fn clock_regression_and_bad_budget_do_not_invent_a_verdict() {
        let err = route_failure(10, 9, 5, ExecSignals::default()).unwrap_err();
        assert!(matches!(
            err,
            EvalError::ClockRegressed {
                started_at_ms: 10,
                now_ms: 9
            }
        ));
        assert!(matches!(
            route_failure(0, 0, 0, ExecSignals::default()).unwrap_err(),
            EvalError::BadBudget
        ));
        assert_eq!(
            route_failure(0, 4, 5, ExecSignals::default()).unwrap(),
            Classified::Passed
        );
    }
}
