//! 工时推进:按日历把"有效工时"映射为真实时刻。
//!
//! 三条公开语义(全部泛型 `Tz: TimeZone + Clone`——同一日历服务任意时区的消费者,
//! 输入输出保持调用方时区,内部一律换算到日历时区计算):
//!
//! 1. `next_working_instant(now, duration)`:`now` 起推进 `duration` 有效工时后的时刻。
//!    `now` 在非班次时间则先滚动到下一个开工点;`duration = 0` 即"下一个可开工时刻"。
//! 2. `sla_deadline(placed_at, promised)`:SLA 正向——下单 + 承诺工时 → 实际日历截止。
//! 3. `latest_order_at(due_at, required)`:截单派生(反向)——交付承诺时刻 − 所需工时 → 最晚下单时刻。
//!    与 1 互逆:恒有 `next_working_instant(latest_order_at(due, r), r) == due`(due 落在
//!    工时可达时刻时)。

use crate::calendar::Calendar;
use crate::error::CalendarError;
use chrono::{DateTime, Duration, TimeZone};

impl Calendar {
    /// 按日历从 `now` 推进 `duration` 的有效工时,返回完成时刻(保持 `now` 的时区)。
    pub fn next_working_instant<Tz>(
        &self,
        now: DateTime<Tz>,
        duration: Duration,
    ) -> Result<DateTime<Tz>, CalendarError>
    where
        Tz: TimeZone + Clone,
    {
        if duration < Duration::zero() {
            return Err(CalendarError::NegativeDuration {
                seconds: duration.num_seconds(),
            });
        }
        if duration.is_zero() {
            return self.roll_forward(now);
        }

        let mut t = now.with_timezone(&self.tz);
        let mut remaining = duration;
        for _ in 0..=self.horizon_days {
            let date = t.date_naive();
            for shift in self.shifts_on(date) {
                let (Some(ws), Some(we)) = (
                    self.local_dt(date, shift.start),
                    self.local_dt(date, shift.end),
                ) else {
                    continue; // DST 间隙:该班次本地时刻不存在,跳过
                };
                if we <= t {
                    continue; // 本班次已过
                }
                let entry = t.max(ws); // now 在班次内则续算,否则从开工点起算
                let avail = we - entry;
                if remaining <= avail {
                    let done = entry + remaining;
                    return Ok(done.with_timezone(&now.timezone()));
                }
                remaining -= avail;
            }
            // 本日班次耗尽,跳到次日 00:00
            let Some(next) = date.succ_opt() else {
                break;
            };
            t = self
                .local_dt(next, chrono::NaiveTime::MIN)
                .expect("00:00 恒为合法本地时刻");
        }
        Err(CalendarError::HorizonExceeded {
            searched_days: self.horizon_days,
        })
    }

    /// `duration = 0`:滚动到 `now` 之后(含)的第一个可开工时刻。
    fn roll_forward<Tz>(&self, now: DateTime<Tz>) -> Result<DateTime<Tz>, CalendarError>
    where
        Tz: TimeZone + Clone,
    {
        let mut t = now.with_timezone(&self.tz);
        for _ in 0..=self.horizon_days {
            let date = t.date_naive();
            for shift in self.shifts_on(date) {
                let (Some(ws), Some(we)) = (
                    self.local_dt(date, shift.start),
                    self.local_dt(date, shift.end),
                ) else {
                    continue;
                };
                if we > t {
                    return Ok(t.max(ws).with_timezone(&now.timezone()));
                }
            }
            let Some(next) = date.succ_opt() else {
                break;
            };
            t = self
                .local_dt(next, chrono::NaiveTime::MIN)
                .expect("00:00 恒为合法本地时刻");
        }
        Err(CalendarError::HorizonExceeded {
            searched_days: self.horizon_days,
        })
    }

    /// SLA 正向:下单时刻 + 承诺有效工时 → 实际日历截止时刻。
    /// 与 `next_working_instant` 同一算法,保留独立名字让调用点可读(工单/合同语义)。
    pub fn sla_deadline<Tz>(
        &self,
        placed_at: DateTime<Tz>,
        promised_working: Duration,
    ) -> Result<DateTime<Tz>, CalendarError>
    where
        Tz: TimeZone + Clone,
    {
        self.next_working_instant(placed_at, promised_working)
    }

    /// 截单派生(反向):交付承诺时刻 − 所需有效工时 → 最晚下单时刻。
    ///
    /// 语义:从 `due_at` 起沿日历**逆向**消耗 `required` 工时,落点即最晚下单时刻;
    /// 该时刻下单、正向推进 `required` 工时,恰好(且仅在日历上才可能)在 `due_at` 完成。
    /// 若 `due_at` 不在班次内,先收敛到"≤ due_at 的最后一个工时时刻"再逆向消耗。
    pub fn latest_order_at<Tz>(
        &self,
        due_at: DateTime<Tz>,
        required: Duration,
    ) -> Result<DateTime<Tz>, CalendarError>
    where
        Tz: TimeZone + Clone,
    {
        if required < Duration::zero() {
            return Err(CalendarError::NegativeDuration {
                seconds: required.num_seconds(),
            });
        }

        // 反向走日历:`day` 是当前考察的本地日,`bound` 是该日可耗工时的截止上界
        //(仅 due 当天有界;往前走的日子窗口全部可用)。
        let mut day = due_at.with_timezone(&self.tz).date_naive();
        let mut bound = Some(due_at.with_timezone(&self.tz));
        let mut remaining = required;
        for _ in 0..=self.horizon_days {
            for shift in self.shifts_on(day).iter().rev() {
                let (Some(ws), Some(we)) = (
                    self.local_dt(day, shift.start),
                    self.local_dt(day, shift.end),
                ) else {
                    continue; // DST 间隙
                };
                let exit = match bound {
                    Some(b) => {
                        if ws >= b {
                            continue; // 本班次在 due 之后
                        }
                        b.min(we) // due 在班次内取 due,在班次后取窗口末
                    }
                    None => we, // 更早的日子:窗口完整可用
                };
                let avail = exit - ws;
                if remaining <= avail {
                    let placed = exit - remaining;
                    return Ok(placed.with_timezone(&due_at.timezone()));
                }
                remaining -= avail;
            }
            // 本日班次耗尽,退到前一日(其窗口完整可用)
            let Some(prev) = day.pred_opt() else {
                break;
            };
            day = prev;
            bound = None;
        }
        Err(CalendarError::HorizonExceeded {
            searched_days: self.horizon_days,
        })
    }
}
