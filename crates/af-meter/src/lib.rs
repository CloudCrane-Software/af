//! af-meter —— 供给计量组合表达式（PoC，id=poc-meter）。
//!
//! 模型：`Meter` 树。数量是 [`Count`]（u64：调用次数、令牌个数、配额）。
//! 金额是 [`Cents`]（i64 美分），记在 [`BudgetLedger`] 上。
//! `budget_cents` / `spend_cap_cents` / `spend_cents` 与 af-credential 同词同型。
//! 时钟 [`Time`] 是自任选纪元起的整数秒，由调用方注入。
//!
//! - 叶子：[`Meter::Stock`]（剩余额度池）、[`Meter::Window`]（固定窗口配额）、
//!   [`Meter::Concurrency`]（并发占用）、[`Meter::Rate`]（令牌桶限速，派生计量）。
//! - 组合子：[`Meter::Min`]（同时受所有子计量约束，消耗时每个子计量各扣一份）、
//!   [`Meter::Max`]（择优/故障转移，消耗只记在选中者上）。
//! - 评估：[`Meter::given`] 返回 [`Reading`]：剩余可用量 + 最早恢复时刻。
//!
//! 语义约定（PoC 简化，均已用测试固定）：
//! - `recovers_at` = 可用量下次**严格增加**的时刻；`None` = 不会自动恢复（如 Stock 用尽）。
//!   内存树上这个时刻是 [`Ticks`]（注入的整数秒）。账本上是 [`UnixMillis`]（窗口终点的 Unix 毫秒）。
//!   两个类型不能比较、不能相加。
//! - `Min` 的 `recovers_at` = 所有"绑定子计量"（available 等于最小值者）恢复时刻的**最大值**；
//!   任一绑定者永不恢复则整体 `None`。
//! - `Max` 的 `recovers_at` = 各子计量恢复时刻的**最小值**（下界语义：最早"可能出现"
//!   更多可用量的时刻）。
//! - 单消费者假设：评估期间没有其他主体消耗同一计量树（并发占用除外，见 `acquire`）。
//! - 令牌桶按整数秒回填：推进 `elapsed` 秒增加 `elapsed * per_second` 个整令牌，封顶 `burst`。
//!   `recovers_at` 是令牌数实际回到 `burst` 的那个整数秒。
//!
//! 持久化账本在 [`ledger`]：一个计量一份 SQLite 文件。窗口起止按钟面日期解析，
//! 扣减和 outbox 补偿写在同一个事务里。这里不改内存 [`Meter`] 的语义。
//!
//! Bifrost 接线在 [`bifrost`]。它只读 [`Meter::given`] 的可用量和
//! [`BudgetLedger::remaining_cents`]，不改这两处已经验证的语义。

pub mod bifrost;
mod budget;
pub mod demo;
pub mod ledger;

pub use budget::{BudgetError, BudgetLedger};
pub use ledger::{
    window_bounds, ConsumeResult, Declaration, EventRecord, Ledger, LedgerError, Opened,
    OutboxRecord, Recipe, Totals, WindowKind, WindowSnapshot, SCHEMA_VERSION,
};

use serde::{Deserialize, Serialize};

/// 模拟时钟：自任选纪元起的整数秒。由调用方注入，计量器不读墙钟。
///
/// 这是裸整数，方便窗口算术。读数里的恢复时刻用 [`Ticks`]，账本用 [`UnixMillis`]，
/// 避免把这里的秒和 Unix 毫秒放进同一次比较。
pub type Time = u64;

/// 内存计量的恢复时刻。单位是调用方注入的整数秒，与 [`Time`] 相同，不是 Unix 毫秒。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Ticks(pub u64);

impl Ticks {
    pub const fn new(seconds: u64) -> Self {
        Self(seconds)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for Ticks {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}s", self.0)
    }
}

/// 账本窗口终点。单位是 Unix 毫秒。
///
/// 没有通向 [`Ticks`] 的 `From` / `PartialEq`。秒和毫秒要换算必须写在调用处。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnixMillis(pub u64);

impl UnixMillis {
    pub const fn new(milliseconds: u64) -> Self {
        Self(milliseconds)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for UnixMillis {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}ms", self.0)
    }
}

/// 计数最小单位：调用次数、令牌个数、窗口配额。无符号。
pub type Count = u64;

/// 金额最小单位：美分，有符号。
///
/// 与 `af_credential::Cents` 一样是 `i64` 的别名。
/// `budget_cents`、`spend_cap_cents`、`spend_cents` 在两个 crate 里都是这个类型，
/// 凭证签发的限额可以原样交给 [`BudgetLedger`]。
pub type Cents = i64;

/// `given(now)` 或账本扣减的读数：剩余可用量 + 最早恢复时刻。
///
/// `U` 默认是 [`Ticks`]（内存计量，整数秒）。账本路径用 [`Reading<UnixMillis>`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reading<U = Ticks> {
    /// 当前剩余可用量（计数）。
    pub available: Count,
    /// 可用量下次严格增加的时刻。单位是 `U`。`None` = 不会自动恢复。
    pub recovers_at: Option<U>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MeterError {
    /// 可用量不足以覆盖本次消耗。
    #[error("insufficient: need={need}, available={available}, recovers_at={recovers_at:?}")]
    Insufficient {
        need: Count,
        available: Count,
        recovers_at: Option<Ticks>,
    },
    /// 该节点类型不支持此操作（如对 Concurrency 调 `try_consume`）。
    #[error("unsupported operation: {0}")]
    Unsupported(&'static str),
}

/// 计量树：叶子 = 原子供给计量，分支 = 组合子。
///
/// 这棵树上的数量全是 [`Count`]。金额走 [`BudgetLedger`]（[`Cents`]）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Meter {
    /// 一次性额度池：用多少扣多少，用完不会自动恢复（按量付费余额）。
    Stock { remaining: Count },
    /// 固定窗口配额：每 `window` 秒把 `remaining` 重置回 `capacity`；
    /// `reset_at` 为下次重置时刻（错过多个窗口不累积额度）。
    Window {
        window: Time,
        capacity: Count,
        remaining: Count,
        reset_at: Time,
    },
    /// 并发占用：最多 `limit` 个同时占用，每个占用带释放时刻（到期自动释放）。
    Concurrency {
        limit: u64,
        in_use: u64,
        releases: Vec<Time>,
    },
    /// 令牌桶限速（派生计量）：每秒补充 `per_second` 个整令牌，桶上限 `burst`。
    ///
    /// 回填取整：`elapsed` 为整数秒，增加 `elapsed * per_second` 个令牌后封顶 `burst`。
    /// 时间不前进、或 `per_second == 0` 时不补充。时钟倒退不回滚已有令牌。
    Rate {
        per_second: Count,
        burst: Count,
        tokens: Count,
        last: Time,
    },
    /// 组合子 Min：同时受所有子计量约束；消耗时从每个子计量各扣一份。
    /// 典型用法：Min(周窗, 日窗) = 周窗套日窗。
    Min(Vec<Meter>),
    /// 组合子 Max：择优/故障转移，available = 各子计量最大值；消耗只记在选中者上。
    /// 可用量相同时保留靠前的子计量。
    Max(Vec<Meter>),
}

impl Meter {
    pub fn stock(remaining: Count) -> Meter {
        Meter::Stock { remaining }
    }

    /// 从 `now` 起的第一个窗口：remaining = capacity，reset_at = now + window。
    pub fn window(window: Time, capacity: Count, now: Time) -> Meter {
        Meter::Window {
            window,
            capacity,
            remaining: capacity,
            reset_at: now.saturating_add(window),
        }
    }

    /// 满桶令牌桶：tokens = burst，从 `now` 开始按每秒 `per_second` 个整令牌回填。
    pub fn rate(per_second: Count, burst: Count, now: Time) -> Meter {
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
                if *window == 0 {
                    return;
                }
                while now >= *reset_at {
                    *remaining = *capacity;
                    let next = reset_at.saturating_add(*window);
                    if next == *reset_at {
                        break;
                    }
                    *reset_at = next;
                }
            }
            Meter::Concurrency {
                in_use, releases, ..
            } => {
                let due = releases.iter().filter(|&&r| r <= now).count();
                if due > 0 {
                    releases.retain(|&r| r > now);
                    let due = u64::try_from(due).unwrap_or(u64::MAX);
                    *in_use = in_use.saturating_sub(due);
                }
            }
            Meter::Rate {
                per_second,
                burst,
                tokens,
                last,
            } => {
                if now > *last && *per_second > 0 {
                    let elapsed = now - *last;
                    let add = elapsed.saturating_mul(*per_second);
                    *tokens = tokens.saturating_add(add).min(*burst);
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
                recovers_at: None,
            },
            Meter::Window {
                capacity,
                remaining,
                reset_at,
                ..
            } => Reading {
                available: *remaining,
                recovers_at: if *remaining < *capacity {
                    Some(Ticks(*reset_at))
                } else {
                    None
                },
            },
            Meter::Concurrency {
                limit,
                in_use,
                releases,
            } => Reading {
                available: limit.saturating_sub(*in_use),
                recovers_at: if *in_use >= *limit {
                    releases.iter().copied().min().map(Ticks)
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
                recovers_at: if *tokens < *burst && *per_second > 0 {
                    let deficit = *burst - *tokens;
                    Some(Ticks(last.saturating_add(deficit.div_ceil(*per_second))))
                } else {
                    None
                },
            },
            Meter::Min(kids) => {
                if kids.is_empty() {
                    return Reading {
                        available: 0,
                        recovers_at: None,
                    };
                }
                let readings: Vec<Reading> = kids.iter().map(|k| k.read()).collect();
                let min = readings.iter().map(|r| r.available).min().unwrap();
                // 绑定子计量 = available 等于最小值者；整体恢复 = 它们恢复时刻的最大值。
                // 任一绑定者永不恢复则整体不恢复。
                let mut forever = false;
                let mut latest: Option<Ticks> = None;
                for r in &readings {
                    if r.available != min {
                        continue;
                    }
                    match r.recovers_at {
                        None => forever = true,
                        Some(t) => latest = Some(latest.map_or(t, |e| e.max(t))),
                    }
                }
                Reading {
                    available: min,
                    recovers_at: if forever { None } else { latest },
                }
            }
            Meter::Max(kids) => {
                if kids.is_empty() {
                    return Reading {
                        available: 0,
                        recovers_at: None,
                    };
                }
                let readings: Vec<Reading> = kids.iter().map(|k| k.read()).collect();
                let max = readings.iter().map(|r| r.available).max().unwrap();
                // 下界语义：最早可能出现更多可用量的时刻。
                let recovers_at = readings.iter().filter_map(|r| r.recovers_at).min();
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
    pub fn try_consume(&mut self, need: Count, now: Time) -> Result<(), MeterError> {
        let r = self.given(now);
        if r.available < need {
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
                        need: 1,
                        available: 0,
                        recovers_at: r.recovers_at,
                    })
                }
            }
            _ => Err(MeterError::Unsupported("acquire 仅适用于 Concurrency 节点")),
        }
    }

    /// 前提：已用 `given`/`try_consume` 确认可覆盖。Min 扣所有子计量；Max 扣最大者。
    fn deduct(&mut self, amount: Count) {
        match self {
            Meter::Stock { remaining } | Meter::Window { remaining, .. } => {
                *remaining -= amount;
            }
            Meter::Rate { tokens, .. } => {
                *tokens -= amount;
            }
            Meter::Concurrency { .. } => {
                // 并发占用走 acquire/release，不走数量扣减。
            }
            Meter::Min(kids) => {
                for k in kids {
                    k.deduct(amount);
                }
            }
            Meter::Max(kids) => {
                let mut best = 0usize;
                let mut best_available: Option<Count> = None;
                for (i, k) in kids.iter().enumerate() {
                    let a = k.read().available;
                    let replace = match best_available {
                        Some(b) => a > b,
                        None => true,
                    };
                    if replace {
                        best_available = Some(a);
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    /// 使用该供给，已按预计消耗 `need` 扣减。
    Use {
        supply: String,
        remaining_after: Count,
    },
    /// 没有供给能覆盖预计消耗；`recovers_at` 为各供给中最早的恢复时刻（整数秒）。
    Defer { recovers_at: Option<Ticks> },
}

/// 路由：按任务预计消耗选供给。按优先级序找第一个 available >= need 的供给并扣减；
/// 都不够则 `Defer` 并给出最早恢复时刻。
pub fn route(supplies: &mut [Supply], need: Count, now: Time) -> Decision {
    let mut order: Vec<usize> = (0..supplies.len()).collect();
    order.sort_by_key(|&i| supplies[i].priority);
    let mut earliest: Option<Ticks> = None;
    for &i in &order {
        let r = supplies[i].meter.given(now);
        if r.available >= need {
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

    const DAY: Time = 86_400;
    const WEEK: Time = 604_800;

    #[test]
    fn ticks_and_unix_millis_are_different_types() {
        assert_ne!(
            std::any::TypeId::of::<Ticks>(),
            std::any::TypeId::of::<UnixMillis>()
        );
        assert_ne!(
            std::any::TypeId::of::<Reading<Ticks>>(),
            std::any::TypeId::of::<Reading<UnixMillis>>()
        );
        assert_eq!(Ticks::new(86_400).get(), 86_400);
        assert_eq!(UnixMillis::new(86_400_000).get(), 86_400_000);
        let memory = Reading {
            available: 1,
            recovers_at: Some(Ticks::new(86_400)),
        };
        let ledger = Reading {
            available: 1,
            recovers_at: Some(UnixMillis::new(86_400_000)),
        };
        assert_eq!(memory.recovers_at, Some(Ticks(86_400)));
        assert_eq!(ledger.recovers_at, Some(UnixMillis(86_400_000)));
    }

    #[test]
    fn stock_exhaust_reports_no_recovery() {
        let mut m = Meter::stock(10);
        m.try_consume(6, 0).unwrap();
        let r = m.given(0);
        assert_eq!(r.available, 4);
        assert_eq!(r.recovers_at, None);
        let err = m.try_consume(5, 1).unwrap_err();
        match err {
            MeterError::Insufficient {
                need,
                available,
                recovers_at,
            } => {
                assert_eq!(need, 5);
                assert_eq!(available, 4);
                assert_eq!(recovers_at, None);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn window_resets_and_does_not_accumulate() {
        let mut m = Meter::window(DAY, 200, 0);
        m.try_consume(200, 0).unwrap();
        let r = m.given(DAY - 1);
        assert_eq!(r.available, 0);
        assert_eq!(r.recovers_at, Some(Ticks(DAY)));
        assert!(m.try_consume(1, DAY - 1).is_err());
        // 重置后满血；空闲多个窗口不累积额度。
        let r = m.given(3 * DAY);
        assert_eq!(r.available, 200);
        assert_eq!(r.recovers_at, None);
    }

    #[test]
    fn min_deducts_all_children() {
        let mut m = Meter::min(vec![
            Meter::window(WEEK, 1_000, 0),
            Meter::window(DAY, 200, 0),
        ]);
        m.try_consume(150, 0).unwrap();
        let Meter::Min(kids) = &m else {
            panic!("应是 Min")
        };
        let Meter::Window { remaining: wk, .. } = &kids[0] else {
            panic!("应是周窗")
        };
        let Meter::Window { remaining: dy, .. } = &kids[1] else {
            panic!("应是日窗")
        };
        assert_eq!(*wk, 850);
        assert_eq!(*dy, 50);
        let r = m.given(1);
        assert_eq!(r.available, 50);
    }

    #[test]
    fn nested_weekly_window_binds_and_recovers_at_weekly_reset() {
        // 周窗套日窗：连吃 5 天满日窗，把周窗也吃光。
        let mut m = Meter::min(vec![
            Meter::window(WEEK, 1_000, 0),
            Meter::window(DAY, 200, 0),
        ]);
        for k in 0..5u32 {
            m.try_consume(200, u64::from(k) * DAY + 1).unwrap();
        }
        // 第 5 天：日窗仍有 200，但周窗为 0 → available = 0（周窗绑定）。
        let r = m.given(4 * DAY + 100);
        assert_eq!(r.available, 0);
        // 周窗、日窗同时绑定（都为 0）：最早恢复 = 两者恢复时刻的较大值 = 周重置。
        assert_eq!(r.recovers_at, Some(Ticks(WEEK)));
    }

    #[test]
    fn min_blocked_by_dead_stock_never_recovers() {
        let mut m = Meter::min(vec![Meter::stock(0), Meter::window(DAY, 200, 0)]);
        let r = m.given(1);
        assert_eq!(r.available, 0);
        assert_eq!(r.recovers_at, None, "Stock 绑定且不恢复 → 整体不恢复");
    }

    #[test]
    fn rate_token_bucket_refills_continuously() {
        let mut m = Meter::rate(1, 10, 0);
        m.try_consume(10, 0).unwrap();
        // 整数秒采样与按秒连续回填一致：t=2 有 2 个令牌，回满仍是 t=10。
        let r = m.given(2);
        assert_eq!(r.available, 2);
        assert_eq!(r.recovers_at, Some(Ticks(10)));
        assert!(m.try_consume(3, 2).is_err());
        let r = m.given(10);
        assert_eq!(r.available, 10);
        assert_eq!(r.recovers_at, None);
    }

    #[test]
    fn rate_full_time_ceils_to_the_whole_second_it_actually_fills() {
        // per_second 不能整除 deficit 时，回满落在令牌数真正回到 burst 的整数秒。
        let mut m = Meter::rate(3, 10, 0);
        m.try_consume(10, 0).unwrap();
        let r = m.given(3);
        assert_eq!(r.available, 9);
        assert_eq!(r.recovers_at, Some(Ticks(4)));
        let r = m.given(4);
        assert_eq!(r.available, 10);
        assert_eq!(r.recovers_at, None);
    }

    #[test]
    fn concurrency_acquire_and_release() {
        let mut m = Meter::concurrency(2);
        m.acquire(0, 30).unwrap();
        m.acquire(0, 45).unwrap();
        let r = m.given(0);
        assert_eq!(r.available, 0);
        assert_eq!(r.recovers_at, Some(Ticks(30)));
        assert!(m.acquire(0, 60).is_err());
        let r = m.given(30);
        assert_eq!(r.available, 1);
        assert_eq!(r.recovers_at, None);
        m.acquire(30, 100).unwrap();
    }

    #[test]
    fn max_deducts_only_chosen_child() {
        let mut m = Meter::max(vec![Meter::stock(5), Meter::stock(100)]);
        let r = m.given(0);
        assert_eq!(r.available, 100);
        m.try_consume(60, 0).unwrap();
        let Meter::Max(kids) = &m else {
            panic!("应是 Max")
        };
        let Meter::Stock { remaining: a } = &kids[0] else {
            panic!()
        };
        let Meter::Stock { remaining: b } = &kids[1] else {
            panic!()
        };
        assert_eq!(*a, 5);
        assert_eq!(*b, 40);
        // 再把大池子扣光：剩 max(5, 0)=5。Stock 不看时刻。
        m.try_consume(40, 1).unwrap();
        let r = m.given(1);
        assert_eq!(r.available, 5);
        // Max 不聚合：max=5 时消耗 8 失败（不会把两个池子加起来用）。
        let err = m.try_consume(8, 1).unwrap_err();
        assert!(matches!(err, MeterError::Insufficient { .. }));
    }

    #[test]
    fn serde_roundtrip_nested_meter() {
        let m = Meter::min(vec![
            Meter::window(WEEK, 1_000, 0),
            Meter::window(DAY, 200, 0),
        ]);
        let json = serde_json::to_string_pretty(&m).unwrap();
        let mut back: Meter = serde_json::from_str(&json).unwrap();
        let mut m = m;
        assert_eq!(m.given(0).available, back.given(0).available);
        assert_eq!(m.given(1).available, back.given(1).available);
        assert_eq!(m.given(DAY + 1).available, back.given(DAY + 1).available);
    }

    #[test]
    fn defer_when_no_supply_covers() {
        let mut s1 = vec![Supply {
            name: "only".into(),
            priority: 0,
            meter: Meter::stock(5),
        }];
        match route(&mut s1, 10, 0) {
            Decision::Defer { recovers_at } => assert_eq!(recovers_at, None),
            other => panic!("{other:?}"),
        }
        let mut s2 = vec![Supply {
            name: "win".into(),
            priority: 0,
            meter: Meter::window(DAY, 200, 0),
        }];
        s2[0].meter.try_consume(200, 0).unwrap();
        match route(&mut s2, 1, 0) {
            Decision::Defer { recovers_at } => assert_eq!(recovers_at, Some(Ticks(DAY))),
            other => panic!("{other:?}"),
        }
    }
}
