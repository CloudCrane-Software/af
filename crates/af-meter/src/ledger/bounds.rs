//! 窗口起止。只解析钟面日期。
//!
//! 注入的 Unix 毫秒先映到时区里的日历日，再取当天或本周一的当地 00:00，
//! 以及下一个日历边界的当地 00:00。不把绝对毫秒加到「现在」上：
//! 夏令时切换日的钟面一天不是 86400000 毫秒。

use chrono::{Datelike, Days, MappedLocalTime, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use chrono_tz::Tz;

use super::{parse_tz, LedgerError, Result, WindowKind};

/// 含 `now_ms` 的窗口，左闭右开，单位是 Unix 毫秒。
///
/// 日窗终点是下一个日历日的当地 00:00。周窗从当周周一开始，终点是下一个周一的当地 00:00。
/// 当地 00:00 若因时区缺口不存在，取当天钟面上第一个存在的时刻。歧义钟面取较早的一次。
pub fn window_bounds(now_ms: i64, timezone: &str, kind: WindowKind) -> Result<(i64, i64)> {
    let tz = parse_tz(timezone)?;
    window_bounds_tz(now_ms, tz, kind)
}

pub(super) fn window_bounds_tz(now_ms: i64, tz: Tz, kind: WindowKind) -> Result<(i64, i64)> {
    let date = civil_date(now_ms, tz)?;
    let (start_date, end_date) = window_dates(date, kind)?;
    let start_ms = civil_midnight_ms(start_date, tz)?;
    let end_ms = civil_midnight_ms(end_date, tz)?;
    if end_ms <= start_ms {
        return Err(LedgerError::CivilOverflow);
    }
    Ok((start_ms, end_ms))
}

fn civil_date(now_ms: i64, tz: Tz) -> Result<NaiveDate> {
    tz.timestamp_millis_opt(now_ms)
        .earliest()
        .map(|dt| dt.date_naive())
        .ok_or(LedgerError::UnmappedInstant { now_ms })
}

fn window_dates(date: NaiveDate, kind: WindowKind) -> Result<(NaiveDate, NaiveDate)> {
    match kind {
        WindowKind::Day => {
            let end = date
                .checked_add_days(Days::new(1))
                .ok_or(LedgerError::CivilOverflow)?;
            Ok((date, end))
        }
        WindowKind::Week => {
            let back = u64::from(date.weekday().num_days_from_monday());
            let start = date
                .checked_sub_days(Days::new(back))
                .ok_or(LedgerError::CivilOverflow)?;
            let end = start
                .checked_add_days(Days::new(7))
                .ok_or(LedgerError::CivilOverflow)?;
            Ok((start, end))
        }
    }
}

fn civil_midnight_ms(date: NaiveDate, tz: Tz) -> Result<i64> {
    let midnight = NaiveTime::from_hms_opt(0, 0, 0).expect("00:00:00 is a valid civil time");
    let naive = NaiveDateTime::new(date, midnight);
    if let Some(ms) = earliest_millis(tz.from_local_datetime(&naive)) {
        return Ok(ms);
    }
    // 当地 00:00 落在缺口里。沿钟面逐分钟找当天第一个存在的时刻，不加绝对毫秒。
    for minute in 1_u32..(24 * 60) {
        let Some(time) = NaiveTime::from_num_seconds_from_midnight_opt(minute * 60, 0) else {
            break;
        };
        let candidate = NaiveDateTime::new(date, time);
        if let Some(ms) = earliest_millis(tz.from_local_datetime(&candidate)) {
            return Ok(ms);
        }
    }
    Err(LedgerError::UnmappedMidnight)
}

fn earliest_millis(mapped: MappedLocalTime<chrono::DateTime<Tz>>) -> Option<i64> {
    mapped.earliest().map(|dt| dt.timestamp_millis())
}
