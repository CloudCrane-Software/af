//! af-meter —— 供给计量组合表达式（PoC，id=poc-meter）。
//!
//! 模型：`Meter` 树。
//! - 叶子：[`Meter::Stock`]（剩余额度池）、[`Meter::Window`]（固定窗口配额）、
//!   [`Meter::Concurrency`]（并发占用）、[`Meter::Rate`]（令牌桶限速，派生计量）。
//! - 组合子：[`Meter::Min`]（同时受所有子计量约束，消耗时每个子计量各扣一份）、
//!   [`Meter::Max`](择优/故障转移，消耗只记在选中者上)。
//! - 评估：带模拟时钟（`now` = 自任选纪元起的秒数，不依赖真实墙钟），
//!   [`Meter::given`] 返回 [`Reading`]：剩余可用量 + 最早恢复时刻。
//!
//! 语义约定（PoC 简化，均已用测试固定）：
//! - `recovers_at` = 可用量下次**严格增加**的时刻；`None` = 不会自动恢复（如 Stock 用尽）。
//! - `Min` 的 `recovers_at` = 所有"绑定子计量"（available 等于最小值者）恢复时刻的**最大值**；
//!   任一绑定者永不恢复则整体 `None`。
//! - `Max` 的 `recovers_at` = 各子计量恢复时刻的**最小值**（下界语义：最早"可能出现"
//!   更多可用量的时刻）。
//! - 单消费者假设：评估期间没有其他主体消耗同一计量树（并发占用除外，见 `acquire`）。

pub mod demo;

use serde::{Deserialize, Serialize};

/// 模拟时钟：自任选纪元起的秒数。
pub type Time = f64;
/// 计量数量（调用次数、token 数等）。
pub type Amount = f64;

const EPS: f64 = 1e-9;

/// `given(now)` 的读数：剩余可用量 + 最早恢复时刻。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    /// 当前剩余可用量。
    pub available: Amount,
    /// 可用量下次严格增加的时刻；`None` = 不会自动恢复。
    pub recovers_at: Option<Time>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MeterError {
    /// 可用量不足以覆盖本次消耗。
    #[error("insufficient: need={need}, available={available}, recovers_at={recovers_at:?}")]
    Insufficient {
        need: Amount,
        available: Amount,
        recovers_at: Option<Time>,
    },
    /// 该节点类型不支持此操作（如对 Concurrency 调 `try_consume`）。
    #[error("unsupported operation: {0}")]
    Unsupported(&'static str),
}

/// 计量树：叶子 = 原子供给计量，分支 = 组合子。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Meter {
    /// 一次性额度池：用多少扣多少，用完不会自动恢复（按量付费余额）。
    Stock { remaining: Amount },
    /// 固定窗口配额：每 `window` 秒把 `remaining` 重置回 `capacity`；
    /// `reset_at` 为下次重置时刻（错过多个窗口不累积额度）。
    Window {
        window: Time,
        capacity: Amount,
        remaining: Amount,
        reset_at: Time,
    },
    /// 并发占用：最多 `limit` 个同时占用，每个占用带释放时刻（到期自动释放）。
    Concurrency {
        limit: u64,
        in_use: u64,
        releases: Vec<Time>,
    },
    /// 令牌桶限速（派生计量）：按 `per_second` 连续恢复，桶上限 `burst`。
    Rate {
        per_second: Amount,
        burst: Amount,
        tokens: Amount,
        last: Time,
    },
    /// 组合子 Min：同时受所有子计量约束；消耗时从每个子计量各扣一份。
    /// 典型用法：Min(周窗, 日窗) = 周窗套日窗。
    Min(Vec<Meter>),
    /// 组合子 Max：择优/故障转移，available = 各子计量最大值；消耗只记在选中者上。
    Max(Vec<Meter>),
}

impl Meter {
    pub fn stock(remaining: Amount) -> Meter {
        Meter::Stock { remaining }
    }

    /// 从 `now` 起的第一个窗口：remaining = capacity，reset_at = now + window。
    pub fn window(window: Time, capacity: Amount, now: Time) -> Meter {
        Meter::Window {
            window,
            capacity,
            remaining: capacity,
            reset_at: now + window,
        }
    }

    /// 满桶令牌桶：tokens = burst，从 `now` 开始按 per_second 回填。
    pub fn rate(per_second: Amount, burst: Amount, now: Time) -> Meter {
        Meter::Rate {
            per_second,
            burst,
            tokens: burst,
            last: now,
        }
    }

    pub fn concurrency(limit: u64) -> Meter {
        Meter::Concurrency {
            limit,
            in_use: 0,
            releases: Vec::new(),
        }
    }

    pub fn min(kids: Vec<Meter>) -> Meter {
        Meter::Min(kids)
    }

    pub fn max(kids: Vec<Meter>) -> Meter {
        Meter::Max(kids)
    }

    /// 推进模拟时钟到 `now`：应用窗口重置 / 释放到期占用 / 令牌补充。幂等。
    pub fn refresh(&mut self, now: Time) {
        match self {
            Meter::Stock { .. } => {}
            Meter::Window {
                window,
                capacity,
                remaining,
                reset_at,
            } => {
                if *window <= 0.0 {
                    return; // 防御：非法窗口长度，避免死循环
                }
                while now + EPS >= *reset_at {
                    *remaining = *capacity;
                    *reset_at += *window;
                }
            }
            Meter::Concurrency {
                in_use, releases, ..
            } => {
                let due = releases.iter().filter(|&&r| r <= now + EPS).count();
                if due > 0 {
                    releases.retain(|&r| r > now + EPS);
                    *in_use = in_use.saturating_sub(due as u64);
                }
            }
            Meter::Rate {
                per_second,
                burst,
                tokens,
                last,
            } => {
                if now > *last && *per_second > 0.0 {
                    *tokens = (*tokens + (now - *last) * *per_second).min(*burst);
                    *last = now;
                }
            }
            Meter::Min(kids) | Meter::Max(kids) => {
                for k in kids {
                    k.refresh(now);
                }
            }
        }
    }

    /// 只读评估（状态应已被最近的 `refresh`/`given` 推进过）。
    fn read(&self) -> Reading {
        match self {
            Meter::Stock { remaining } => Reading {
                available: *remaining,
                recovers_at: None, // 不会自动恢复，需外部充值
            },
            Meter::Window {
                capacity,
                remaining,
                reset_at,
                ..
            } => Reading {
                available: *remaining,
                recovers_at: if *remaining < *capacity - EPS {
                    Some(*reset_at)
                } else {
                    None
                },
            },
            Meter::Concurrency {
                limit,
                in_use,
                releases,
            } => Reading {
                available: (*limit - *in_use) as Amount,
                recovers_at: if *in_use >= *limit {
                    releases.iter().copied().fold(None, |acc: Option<Time>, t| {
                        Some(acc.map_or(t, |e| e.min(t)))
                    })
                } else {
                    None
                },
            },
            Meter::Rate {
                per_second,
                burst,
                tokens,
                last,
            } => Reading {
                available: *tokens,
                recovers_at: if *tokens < *burst - EPS {
                    Some(*last + (*burst - *tokens) / per_second.max(EPS))
                } else {
                    None
                },
            },
            Meter::Min(kids) => {
                if kids.is_empty() {
                    return Reading {
                        available: 0.0,
                        recovers_at: None,
                    };
                }
                let readings: Vec<Reading> = kids.iter().map(|k| k.read()).collect();
                let min = readings
                    .iter()
                    .map(|r| r.available)
                    .fold(f64::INFINITY, f64::min);
                // 绑定子计量 = available 等于最小值者；整体恢复 = 它们恢复时刻的最大值
                let mut binding: Vec<Option<Time>> = Vec::new();
                for r in &readings {
                    if (r.available - min).abs() <= EPS {
                        binding.push(r.recovers_at);
                    }
                }
                let recovers_at = if binding.iter().any(|r| r.is_none()) {
                    None
                } else {
                    Some(
                        binding
                            .iter()
                            .filter_map(|r| *r)
                            .fold(f64::NEG_INFINITY, f64::max),
                    )
                };
                Reading {
                    available: min,
                    recovers_at,
                }
            }
            Meter::Max(kids) => {
                if kids.is_empty() {
                    return Reading {
                        available: 0.0,
                        recovers_at: None,
                    };
                }
                let readings: Vec<Reading> = kids.iter().map(|k| k.read()).collect();
                let max = readings
                    .iter()
                    .map(|r| r.available)
                    .fold(f64::NEG_INFINITY, f64::max);
                // 下界语义：最早可能出现更多可用量的时刻
                let recovers_at = readings
                    .iter()
                    .filter_map(|r| r.recovers_at)
                    .fold(None, |acc: Option<Time>, t| {
                        Some(acc.map_or(t, |e| e.min(t)))
                    });
                Reading {
                    available: max,
                    recovers_at,
                }
            }
        }
    }

    /// given(now)：把时钟推进到 `now` 并给出读数（剩余可用量 + 最早恢复时刻）。
    pub fn given(&mut self, now: Time) -> Reading {
        self.refresh(now);
        self.read()
    }

    /// 可用量是否足以覆盖 `need`；足够则扣减
    /// （Min 扣所有子计量，Max 扣 available 最大者，Concurrency 不支持）。
    pub fn try_consume(&mut self, need: Amount, now: Time) -> Result<(), MeterError> {
        let r = self.given(now);
        if r.available + EPS < need {
            return Err(MeterError::Insufficient {
                need,
                available: r.available,
                recovers_at: r.recovers_at,
            });
        }
        self.deduct(need);
        Ok(())
    }

    /// Concurrency 专用：占用一个槽位，`release_at` 到期后由 `refresh` 自动释放。
    pub fn acquire(&mut self, now: Time, release_at: Time) -> Result<(), MeterError> {
        let r = self.given(now);
        match self {
            Meter::Concurrency {
                limit,
                in_use,
                releases,
            } => {
                if *in_use < *limit {
                    releases.push(release_at);
                    *in_use += 1;
                    Ok(())
                } else {
                    Err(MeterError::Insufficient {
                        need: 1.0,
                        available: 0.0,
                        recovers_at: r.recovers_at,
                    })
                }
            }
            _ => Err(MeterError::Unsupported("acquire 仅适用于 Concurrency 节点")),
        }
    }

    /// 前提：已用 `given`/`try_consume` 确认可覆盖。Min 扣所有子计量；Max 扣最大者。
    fn deduct(&mut self, amount: Amount) {
        match self {
            Meter::Stock { remaining } | Meter::Window { remaining, .. } => {
                *remaining -= amount;
            }
            Meter::Rate { tokens, .. } => {
                *tokens -= amount;
            }
            Meter::Concurrency { .. } => {
                // 并发占用走 acquire/release，不走数量扣减
            }
            Meter::Min(kids) => {
                for k in kids {
                    k.deduct(amount);
                }
            }
            Meter::Max(kids) => {
                let mut best = 0usize;
                let mut best_available = f64::NEG_INFINITY;
                for (i, k) in kids.iter().enumerate() {
                    let a = k.read().available;
                    if a > best_available {
                        best_available = a;
                        best = i;
                    }
                }
                if let Some(k) = kids.get_mut(best) {
                    k.deduct(amount);
                }
            }
        }
    }
}

/// 一路供给：名字 + 优先级（数字越小越优先）+ 计量树。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Supply {
    pub name: String,
    pub priority: u32,
    pub meter: Meter,
}

/// 路由决策。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Decision {
    /// 使用该供给，已按预计消耗 `need` 扣减。
    Use {
        supply: String,
        remaining_after: Amount,
    },
    /// 没有供给能覆盖预计消耗；`recovers_at` 为各供给中最早的恢复时刻。
    Defer { recovers_at: Option<Time> },
}

/// 路由：按任务预计消耗选供给。按优先级序找第一个 available >= need 的供给并扣减；
/// 都不够则 `Defer` 并给出最早恢复时刻。
pub fn route(supplies: &mut [Supply], need: Amount, now: Time) -> Decision {
    let mut order: Vec<usize> = (0..supplies.len()).collect();
    order.sort_by_key(|&i| supplies[i].priority);
    let mut earliest: Option<Time> = None;
    for &i in &order {
        let r = supplies[i].meter.given(now);
        if r.available + EPS >= need {
            supplies[i].meter.deduct(need);
            let remaining_after = supplies[i].meter.given(now).available;
            return Decision::Use {
                supply: supplies[i].name.clone(),
                remaining_after,
            };
        }
        earliest = match (earliest, r.recovers_at) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, b) => b,
        };
    }
    Decision::Defer {
        recovers_at: earliest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: Time = 86400.0;
    const WEEK: Time = 604800.0;

    fn approx(a: Amount, b: Amount) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    fn approx_time(a: Option<Time>, b: Option<Time>) {
        match (a, b) {
            (Some(x), Some(y)) => assert!((x - y).abs() < 1e-9, "{x:?} != {y:?}"),
            (None, None) => {}
            _ => panic!("{a:?} != {b:?}"),
        }
    }

    #[test]
    fn stock_exhaust_reports_no_recovery() {
        let mut m = Meter::stock(10.0);
        m.try_consume(6.0, 0.0).unwrap();
        let r = m.given(0.0);
        approx(r.available, 4.0);
        assert_eq!(r.recovers_at, None);
        let err = m.try_consume(5.0, 1.0).unwrap_err();
        match err {
            MeterError::Insufficient {
                need,
                available,
                recovers_at,
            } => {
                approx(need, 5.0);
                approx(available, 4.0);
                assert_eq!(recovers_at, None);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn window_resets_and_does_not_accumulate() {
        let mut m = Meter::window(DAY, 200.0, 0.0);
        m.try_consume(200.0, 0.0).unwrap();
        let r = m.given(DAY - 1.0);
        approx(r.available, 0.0);
        approx_time(r.recovers_at, Some(DAY));
        assert!(m.try_consume(1.0, DAY - 1.0).is_err());
        // 重置后满血；空闲多个窗口不累积额度
        let r = m.given(3.0 * DAY);
        approx(r.available, 200.0);
        assert_eq!(r.recovers_at, None);
    }

    #[test]
    fn min_deducts_all_children() {
        let mut m = Meter::min(vec![
            Meter::window(WEEK, 1000.0, 0.0),
            Meter::window(DAY, 200.0, 0.0),
        ]);
        m.try_consume(150.0, 0.0).unwrap();
        let Meter::Min(kids) = &m else {
            panic!("应是 Min")
        };
        let Meter::Window { remaining: wk, .. } = &kids[0] else {
            panic!("应是周窗")
        };
        let Meter::Window { remaining: dy, .. } = &kids[1] else {
            panic!("应是日窗")
        };
        approx(*wk, 850.0); // 周窗同扣一份
        approx(*dy, 50.0);
        let r = m.given(1.0);
        approx(r.available, 50.0); // available = min
    }

    #[test]
    fn nested_weekly_window_binds_and_recovers_at_weekly_reset() {
        // 周窗套日窗：连吃 5 天满日窗，把周窗也吃光
        let mut m = Meter::min(vec![
            Meter::window(WEEK, 1000.0, 0.0),
            Meter::window(DAY, 200.0, 0.0),
        ]);
        for k in 0..5u32 {
            m.try_consume(200.0, k as f64 * DAY + 1.0).unwrap();
        }
        // 第 5 天：日窗仍有 200，但周窗为 0 → available = 0（周窗绑定）
        let r = m.given(4.0 * DAY + 100.0);
        approx(r.available, 0.0);
        // 周窗、日窗同时绑定（都为 0）：最早恢复 = 两者恢复时刻的较大值 = 周重置
        approx_time(r.recovers_at, Some(WEEK));
    }

    #[test]
    fn min_blocked_by_dead_stock_never_recovers() {
        let mut m = Meter::min(vec![Meter::stock(0.0), Meter::window(DAY, 200.0, 0.0)]);
        let r = m.given(1.0);
        approx(r.available, 0.0);
        assert_eq!(r.recovers_at, None, "Stock 绑定且不恢复 → 整体不恢复");
    }

    #[test]
    fn rate_token_bucket_refills_continuously() {
        let mut m = Meter::rate(1.0, 10.0, 0.0);
        m.try_consume(10.0, 0.0).unwrap();
        let r = m.given(2.5);
        approx(r.available, 2.5); // 2.5s 回填 2.5 个
        approx_time(r.recovers_at, Some(10.0)); // 回满还需 7.5s
        assert!(m.try_consume(3.0, 2.5).is_err());
        let r = m.given(10.0);
        approx(r.available, 10.0);
    }

    #[test]
    fn concurrency_acquire_and_release() {
        let mut m = Meter::concurrency(2);
        m.acquire(0.0, 30.0).unwrap();
        m.acquire(0.0, 45.0).unwrap();
        let r = m.given(0.0);
        approx(r.available, 0.0);
        approx_time(r.recovers_at, Some(30.0)); // 最早释放时刻
        assert!(m.acquire(0.0, 60.0).is_err());
        let r = m.given(30.0); // t=30 释放一个
        approx(r.available, 1.0);
        assert_eq!(r.recovers_at, None);
        m.acquire(30.0, 100.0).unwrap();
    }

    #[test]
    fn max_deducts_only_chosen_child() {
        let mut m = Meter::max(vec![Meter::stock(5.0), Meter::stock(100.0)]);
        let r = m.given(0.0);
        approx(r.available, 100.0); // 择优
        m.try_consume(60.0, 0.0).unwrap();
        let Meter::Max(kids) = &m else {
            panic!("应是 Max")
        };
        let Meter::Stock { remaining: a } = &kids[0] else {
            panic!()
        };
        let Meter::Stock { remaining: b } = &kids[1] else {
            panic!()
        };
        approx(*a, 5.0); // 只扣选中的
        approx(*b, 40.0);
        // 再把大池子扣光：剩 max(5, 0)=5
        m.try_consume(40.0, 0.5).unwrap();
        let r = m.given(1.0);
        approx(r.available, 5.0);
        // Max 不聚合：max=5 时消耗 8 失败（不会把两个池子加起来用）
        let err = m.try_consume(8.0, 1.0).unwrap_err();
        assert!(matches!(err, MeterError::Insufficient { .. }));
    }

    #[test]
    fn serde_roundtrip_nested_meter() {
        let m = Meter::min(vec![
            Meter::window(WEEK, 1000.0, 0.0),
            Meter::window(DAY, 200.0, 0.0),
        ]);
        let json = serde_json::to_string_pretty(&m).unwrap();
        let mut back: Meter = serde_json::from_str(&json).unwrap();
        let mut m = m;
        approx(m.given(0.0).available, back.given(0.0).available);
        approx(m.given(1.0).available, back.given(1.0).available);
        approx(
            m.given(DAY + 1.0).available,
            back.given(DAY + 1.0).available,
        );
    }

    #[test]
    fn defer_when_no_supply_covers() {
        let mut s1 = vec![Supply {
            name: "only".into(),
            priority: 0,
            meter: Meter::stock(5.0),
        }];
        match route(&mut s1, 10.0, 0.0) {
            Decision::Defer { recovers_at } => assert_eq!(recovers_at, None),
            other => panic!("{other:?}"),
        }
        let mut s2 = vec![Supply {
            name: "win".into(),
            priority: 0,
            meter: Meter::window(DAY, 200.0, 0.0),
        }];
        s2[0].meter.try_consume(200.0, 0.0).unwrap();
        match route(&mut s2, 1.0, 0.0) {
            Decision::Defer { recovers_at } => approx_time(recovers_at, Some(DAY)),
            other => panic!("{other:?}"),
        }
    }
}
