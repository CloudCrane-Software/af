//! 日历本体:时区 + 每周班次模板 + 假日表 + 指定日覆盖(调休)。
//!
//! 日期 → 班次的解析优先级(高到低):
//!   1. `day_overrides`(指定日覆盖:调休上班给窗口;空 Vec = 该日放假)
//!   2. `holidays`(假日表:整日不开工)
//!   3. `weekly`(周模板,按周一..周日索引)
//!
//! 日历内容是运营者配置:全部字段 serde 可序列化,可从 JSON 加载(见 examples/sla_demo.rs)。
//! 窗口约定 `[start, end)` 左闭右开;班次必须落在单个本地日内(跨午夜请拆两天)。

use crate::error::CalendarError;
use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeZone};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// 一天内的一个班次窗口 `[start, end)`:
/// 恰好推进到 `end` 的时刻表示"工时在该时刻用尽"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Shift {
    pub start: NaiveTime,
    pub end: NaiveTime,
}

impl Shift {
    pub fn new(start: NaiveTime, end: NaiveTime) -> Result<Self, CalendarError> {
        if start >= end {
            return Err(CalendarError::WindowInverted { start, end });
        }
        Ok(Shift { start, end })
    }

    /// `Shift::hm(9, 0, 12, 0)` = 09:00–12:00。
    pub fn hm(sh: u32, sm: u32, eh: u32, em: u32) -> Result<Self, CalendarError> {
        Shift::new(
            NaiveTime::from_hms_opt(sh, sm, 0).expect("合法时分"),
            NaiveTime::from_hms_opt(eh, em, 0).expect("合法时分"),
        )
    }
}

/// 通用日历。日历本体不含"业务":工位计时 / SLA 承诺 / 时间窗放置 / 容量池都是它的消费者。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Calendar {
    /// 日历所属时区。工时推进一律先换算到此时区再计算;对外接口泛型接受任意时区的输入输出。
    pub tz: Tz,
    /// 周模板:按周一(0)..周日(6)索引。
    weekly: [Vec<Shift>; 7],
    /// 假日表:整日不开工(优先级低于 day_overrides)。
    holidays: BTreeSet<NaiveDate>,
    /// 指定日覆盖:调休上班给窗口;空 Vec = 该日放假。优先级最高。
    day_overrides: BTreeMap<NaiveDate, Vec<Shift>>,
    /// 向前/向后搜索的最大天数视界(默认 10 年,防日历全空/需求过大时死循环)。
    pub horizon_days: u32,
}

const WEEKDAY_NAMES: [&str; 7] = ["一", "二", "三", "四", "五", "六", "日"];

impl Calendar {
    /// 空日历(全周无班次)作为 builder 基底。
    pub fn new_empty(tz: Tz) -> Self {
        Calendar {
            tz,
            weekly: Default::default(),
            holidays: BTreeSet::new(),
            day_overrides: BTreeMap::new(),
            horizon_days: 366 * 10,
        }
    }

    /// 周一至周五同一班次的日历。
    pub fn weekday_single_shift(tz: Tz, shift: Shift) -> Self {
        let mut cal = Calendar::new_empty(tz);
        for slot in cal.weekly.iter_mut().take(5) {
            *slot = vec![shift];
        }
        cal
    }

    /// 全量构造:自定义周模板(按周一..周日索引,支持一天多班次)。
    /// 加载后仍需 `validate()`(与反序列化同责)。
    pub fn from_weekly(tz: Tz, weekly: [Vec<Shift>; 7]) -> Self {
        let mut cal = Calendar::new_empty(tz);
        cal.weekly = weekly;
        cal
    }

    /// 题设默认日历:工作日(周一~周五)08:00–17:00,Asia/Shanghai,无假日。
    pub fn weekday_8x5_shanghai() -> Self {
        Calendar::weekday_single_shift(chrono_tz::Asia::Shanghai, Shift::hm(8, 0, 17, 0).unwrap())
    }

    /// 追加假日表。
    pub fn with_holidays(mut self, dates: impl IntoIterator<Item = NaiveDate>) -> Self {
        self.holidays.extend(dates);
        self
    }

    /// 指定日覆盖(调休)。空 `shifts` = 该日强制放假;非空 = 按给定窗口上班。
    pub fn with_day_override(mut self, date: NaiveDate, shifts: Vec<Shift>) -> Self {
        self.day_overrides.insert(date, shifts);
        self
    }

    /// 收紧搜索视界(供测试"找不到工时"路径)。
    pub fn with_horizon(mut self, days: u32) -> Self {
        self.horizon_days = days;
        self
    }

    /// 校验:周模板与全部覆盖日内,班次必须升序且互不重叠。
    /// 反序列化绕过构造器,加载配置后必须调用(见 examples/sla_demo.rs)。
    pub fn validate(&self) -> Result<(), CalendarError> {
        for (day, name) in WEEKDAY_NAMES.iter().enumerate() {
            self.check_sorted(&self.weekly[day], format!("周{name}"))?;
        }
        for (date, shifts) in &self.day_overrides {
            self.check_sorted(shifts, date.to_string())?;
        }
        Ok(())
    }

    fn check_sorted(&self, shifts: &[Shift], date: String) -> Result<(), CalendarError> {
        for pair in shifts.windows(2) {
            if pair[0].end > pair[1].start {
                return Err(CalendarError::OverlappingWindows {
                    date,
                    a_start: pair[0].start,
                    a_end: pair[0].end,
                    b_start: pair[1].start,
                    b_end: pair[1].end,
                });
            }
        }
        Ok(())
    }

    /// 某个本地日期当天的班次(解析优先级:覆盖 > 假日 > 周模板)。
    pub fn shifts_on(&self, date: NaiveDate) -> &[Shift] {
        if let Some(shifts) = self.day_overrides.get(&date) {
            return shifts.as_slice();
        }
        if self.holidays.contains(&date) {
            return &[];
        }
        let idx = date.weekday().num_days_from_monday() as usize;
        self.weekly[idx].as_slice()
    }

    /// 该日期是否有班次(只看日期解析,不看时刻)。
    pub fn is_working_date(&self, date: NaiveDate) -> bool {
        !self.shifts_on(date).is_empty()
    }

    /// 该时刻是否处于班次窗口内(先换算到日历时区再判定)。
    pub fn is_working_instant<Tz: TimeZone>(&self, at: DateTime<Tz>) -> bool {
        let local = at.with_timezone(&self.tz);
        let date = local.date_naive();
        let t = local.time();
        self.shifts_on(date)
            .iter()
            .any(|s| t >= s.start && t < s.end)
    }

    /// 在本地日期 date 构造日历时区时刻;DST 歧义取较早,间隙(不存在该本地时刻)返回 None。
    /// Asia/Shanghai 无夏令时,恒为 Some。
    pub(crate) fn local_dt(&self, date: NaiveDate, time: NaiveTime) -> Option<DateTime<Tz>> {
        self.tz.from_local_datetime(&date.and_time(time)).earliest()
    }
}
