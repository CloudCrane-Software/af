//! 裁决二实测：meter 窗口对齐 Asia/Shanghai 自然日 / 自然周（周一 00:00），
//! 覆盖跨年、黄金周调休；同函数对照 Europe/Berlin 的 DST 周。
//! 全部断言真实执行（`cargo test -j4 --test window`）。

use chrono::{DateTime, Datelike, Duration, TimeZone, Timelike};
use chrono_tz::Asia::Shanghai;
use chrono_tz::Europe::Berlin;
use chrono_tz::Tz;
use poc_ledger::{day_bounds, week_bounds, window_bounds, window_label, WindowKind};

fn at_tz(tz: Tz, y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Tz> {
    tz.from_local_datetime(
        &chrono::NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, mi, s)
            .unwrap(),
    )
    .unwrap()
}

fn fmt(t: &DateTime<Tz>) -> String {
    t.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

// --------------------------- Asia/Shanghai ---------------------------

#[test]
fn shanghai_ordinary_day_and_week_bounds() {
    let now = at_tz(Shanghai, 2026, 10, 19, 14, 30, 5); // 周一
    let (ds, de) = day_bounds(&now);
    assert_eq!(fmt(&ds), "2026-10-19T00:00:00+08:00");
    assert_eq!(fmt(&de), "2026-10-20T00:00:00+08:00");
    assert_eq!((de - ds).num_hours(), 24);
    let (ws, we) = week_bounds(&now);
    assert_eq!(fmt(&ws), "2026-10-19T00:00:00+08:00");
    assert_eq!(fmt(&we), "2026-10-26T00:00:00+08:00");
    assert_eq!((we - ws).num_hours(), 168);
    assert_eq!(ws.weekday(), chrono::Weekday::Mon);
    // 恰在周一 00:00 的点属于本周而非上周（窗口左闭右开）
    let edge = at_tz(Shanghai, 2026, 10, 19, 0, 0, 0);
    let (ws2, we2) = week_bounds(&edge);
    assert_eq!(fmt(&ws2), "2026-10-19T00:00:00+08:00");
    assert_eq!(fmt(&we2), "2026-10-26T00:00:00+08:00");
}

#[test]
fn shanghai_golden_week_and_makeup_saturday_do_not_shift_natural_windows() {
    // 夹具与 af-calendar RESULT.md 一致：10-01..10-07 假日、09-26（周六）调休上班。
    // 语义：自然日/自然周窗口是纯钟面对齐，假日与调休**都不移动窗口边界**；
    //       "假日不扣量/调休照常扣量"属于工作日历叠加层（af-calendar），不是窗口函数的事。
    let d1 = at_tz(Shanghai, 2026, 10, 1, 23, 59, 59); // 国庆假日（周四）
    let (s, e) = day_bounds(&d1);
    assert_eq!(fmt(&s), "2026-10-01T00:00:00+08:00");
    assert_eq!(fmt(&e), "2026-10-02T00:00:00+08:00");
    assert_eq!(window_label(WindowKind::Day, &d1), "2026-10-01");

    let makeup = at_tz(Shanghai, 2026, 9, 26, 9, 0, 0); // 调休上班的周六
    let (s, e) = day_bounds(&makeup);
    assert_eq!(fmt(&s), "2026-09-26T00:00:00+08:00");
    assert_eq!(fmt(&e), "2026-09-27T00:00:00+08:00");
    assert_eq!(makeup.weekday(), chrono::Weekday::Sat);
    // 该周六所在自然周仍是周一 09-21 对齐
    let (ws, we) = week_bounds(&makeup);
    assert_eq!(fmt(&ws), "2026-09-21T00:00:00+08:00");
    assert_eq!(fmt(&we), "2026-09-28T00:00:00+08:00");
    // 黄金周跨度的自然周不受假日影响：10-01（周四）所在周 = 09-28 对齐
    let (ws, we) = week_bounds(&d1);
    assert_eq!(fmt(&ws), "2026-09-28T00:00:00+08:00");
    assert_eq!(fmt(&we), "2026-10-05T00:00:00+08:00");
}

#[test]
fn shanghai_cross_year_iso_week() {
    // 2026-12-31 是周四；2027-01-01 是周五。ISO 周：这两年交界周 = 2026-W53，
    // 2027-W01 从 2027-01-04（周一）00:00 开始 —— 跨年不产生"新的一周从元旦开始"的错位。
    let nye = at_tz(Shanghai, 2026, 12, 31, 23, 59, 59);
    let (ds, de) = day_bounds(&nye);
    assert_eq!(fmt(&ds), "2026-12-31T00:00:00+08:00");
    assert_eq!(fmt(&de), "2027-01-01T00:00:00+08:00"); // 跨年点即自然日边界
    let (ws, we) = week_bounds(&nye);
    assert_eq!(fmt(&ws), "2026-12-28T00:00:00+08:00");
    assert_eq!(fmt(&we), "2027-01-04T00:00:00+08:00");
    assert_eq!(window_label(WindowKind::WeekMon, &nye), "2026-W53");
    assert_eq!((we - ws).num_hours(), 168);

    let jan1 = at_tz(Shanghai, 2027, 1, 1, 12, 0, 0); // 元旦（周五）
    let (ws, we) = week_bounds(&jan1);
    assert_eq!(fmt(&ws), "2026-12-28T00:00:00+08:00"); // 与 12-31 同一周
    assert_eq!(fmt(&we), "2027-01-04T00:00:00+08:00");
    assert_eq!(window_label(WindowKind::WeekMon, &jan1), "2026-W53");

    let jan4 = at_tz(Shanghai, 2027, 1, 4, 0, 0, 0); // 下一窗口起点
    let (ws, _) = week_bounds(&jan4);
    assert_eq!(fmt(&ws), "2027-01-04T00:00:00+08:00");
    assert_eq!(window_label(WindowKind::WeekMon, &jan4), "2027-W01");
    assert_eq!(jan4.iso_week().year(), 2027);
    assert_eq!(jan4.iso_week().week(), 1);
}

#[test]
fn shanghai_every_week_is_exactly_168h_over_two_years() {
    // 上海无 DST：任意自然周恒 168h（扫 2026-01-05 起的 106 周）
    let mut monday = at_tz(Shanghai, 2026, 1, 5, 0, 0, 0);
    for _ in 0..106 {
        let (ws, we) = week_bounds(&monday);
        assert_eq!(fmt(&ws), fmt(&monday));
        assert_eq!((we - ws), Duration::hours(168));
        monday += Duration::hours(168); // 无 DST 区，绝对时间步进安全
    }
}

// --------------------------- Europe/Berlin（同函数对照） ---------------------------

#[test]
fn berlin_spring_forward_week_is_167h_with_shifting_offsets() {
    // 2026-03-29（周日）02:00 CET→03:00 CEST。包含该周日的自然周：
    // 周一 03-23 00:00 钟面是 +01:00，下周一 03-30 00:00 钟面已是 +02:00 → elapsed = 167h。
    let now = at_tz(Berlin, 2026, 3, 25, 12, 0, 0); // 周三
    let (ws, we) = week_bounds(&now);
    assert_eq!(fmt(&ws), "2026-03-23T00:00:00+01:00");
    assert_eq!(fmt(&we), "2026-03-30T00:00:00+02:00");
    assert_eq!((we - ws).num_hours(), 167); // 春跳周：钟面 168h，流逝只有 167h
    assert_eq!(window_label(WindowKind::WeekMon, &now), "2026-W13");

    // 当日自然日 = 23h（00:00+01 → 00:00+02）
    let sun = at_tz(Berlin, 2026, 3, 29, 12, 0, 0);
    let (ds, de) = day_bounds(&sun);
    assert_eq!(fmt(&ds), "2026-03-29T00:00:00+01:00");
    assert_eq!(fmt(&de), "2026-03-30T00:00:00+02:00");
    assert_eq!((de - ds).num_hours(), 23);
}

#[test]
fn berlin_fall_back_week_is_169h_with_shifting_offsets() {
    // 2026-10-25（周日）03:00 CEST→02:00 CET。自然周 elapsed = 169h。
    let now = at_tz(Berlin, 2026, 10, 21, 12, 0, 0); // 周三
    let (ws, we) = week_bounds(&now);
    assert_eq!(fmt(&ws), "2026-10-19T00:00:00+02:00");
    assert_eq!(fmt(&we), "2026-10-26T00:00:00+01:00");
    assert_eq!((we - ws).num_hours(), 169); // 秋拨周：多出的一小时计入一次
    assert_eq!(window_label(WindowKind::WeekMon, &now), "2026-W43");

    let sun = at_tz(Berlin, 2026, 10, 25, 12, 0, 0);
    let (ds, de) = day_bounds(&sun);
    assert_eq!(fmt(&ds), "2026-10-25T00:00:00+02:00");
    assert_eq!(fmt(&de), "2026-10-26T00:00:00+01:00");
    assert_eq!((de - ds).num_hours(), 25);
}

#[test]
fn berlin_ordinary_week_is_168h_and_transition_sundays_are_not_week_bounds() {
    let now = at_tz(Berlin, 2026, 2, 11, 8, 0, 0); // 无 DST 参与的周
    let (ws, we) = week_bounds(&now);
    assert_eq!(fmt(&ws), "2026-02-09T00:00:00+01:00");
    assert_eq!(fmt(&we), "2026-02-16T00:00:00+01:00");
    assert_eq!((we - ws).num_hours(), 168);
    // DST 切换发生在周日 02:00/03:00 钟面，周一 00:00 边界本身永不落在歧义/间隙时刻
    let (ms, me) = day_bounds(&at_tz(Berlin, 2026, 3, 29, 0, 0, 0));
    assert_eq!(ms.hour(), 0);
    assert_eq!(me.hour(), 0);
}

#[test]
fn berlin_two_year_week_scan_exactly_two_spring_two_fall() {
    // 扫 2026-2027 每个周一：柏林只应有 167h（春跳）与 169h（秋拨）各 2 次，其余 168h；
    // 窗口标签始终单调推进且终点=下一起点（钟面连续性）。
    let mut monday = at_tz(Berlin, 2026, 1, 5, 0, 0, 0);
    let mut n167 = 0;
    let mut n168 = 0;
    let mut n169 = 0;
    for _ in 0..105 {
        let (ws, we) = week_bounds(&monday);
        assert_eq!(ws, monday, "窗口起点必须是该周一 00:00 钟面");
        let h = (we - ws).num_hours();
        match h {
            167 => n167 += 1,
            168 => n168 += 1,
            169 => n169 += 1,
            _ => panic!("意外周长 {h}h at {}", fmt(&ws)),
        }
        // 下一轮的"周一"推进：钟面日期 +7 天后重新解析午夜
        //（不能加 168h 绝对时间——DST 周只有 167h/169h，会漂移到周日 23:00/周一 01:00）
        let next_date = monday.date_naive() + Duration::days(7);
        monday = Berlin
            .from_local_datetime(&next_date.and_hms_opt(0, 0, 0).unwrap())
            .unwrap();
        assert_eq!(monday.weekday(), chrono::Weekday::Mon);
        assert_eq!(monday.hour(), 0);
    }
    assert_eq!(n167, 2, "两年应恰有 2 个春跳周(167h)");
    assert_eq!(n169, 2, "两年应恰有 2 个秋拨周(169h)");
    assert_eq!(n168, 101, "其余周应恒 168h");
}

#[test]
fn same_function_two_timezones_same_instant_agreement() {
    // 同一物理时刻（UTC 2026-03-25T04:00Z）：上海读数与柏林读数同属"当周"，但边界钟面不同时区。
    // 这验证窗口是**时区参数化**的：一个函数，两个时区，各自钟面各自对。
    let utc: DateTime<Tz> = chrono_tz::UTC.from_utc_datetime(
        &chrono::NaiveDate::from_ymd_opt(2026, 3, 25).unwrap().and_hms_opt(4, 0, 0).unwrap(),
    );
    let sh: DateTime<Tz> = utc.with_timezone(&Shanghai);
    let be: DateTime<Tz> = utc.with_timezone(&Berlin);
    let (shs, _) = week_bounds(&sh);
    let (bes, _) = week_bounds(&be);
    assert_eq!(fmt(&shs), "2026-03-23T00:00:00+08:00");
    assert_eq!(fmt(&bes), "2026-03-23T00:00:00+01:00");
    assert_eq!(shs.to_utc() + Duration::hours(7), bes.to_utc()); // 同为"周一 00:00"钟面，时刻差 = 时区差 7h
    // 两者 ISO 周标签一致（同一周）
    assert_eq!(window_label(WindowKind::WeekMon, &sh), window_label(WindowKind::WeekMon, &be));
}

#[test]
fn window_reset_semantics_feed_recovers_at() {
    // 与 af-meter `Reading.recovers_at` 的衔接：日窗不足 → 恢复点=下一个对齐午夜；
    // 周窗不足 → 恢复点=下一个周一 00:00（钟面）。周四 18:00 的例子：
    let now = at_tz(Shanghai, 2026, 10, 22, 18, 0, 0); // 周四
    let (_, de) = day_bounds(&now);
    let (_, we) = week_bounds(&now);
    assert_eq!(fmt(&de), "2026-10-23T00:00:00+08:00"); // 日窗 recovers_at
    assert_eq!(fmt(&we), "2026-10-26T00:00:00+08:00"); // 周窗 recovers_at（下周一）
    assert_eq!((we - now).num_hours(), 78); // 周四18:00→下周一00:00 = 3天+6h
    // 柏林秋拨夜的日窗恢复点：10-25 的"次日 00:00"钟面 = 25h 后（多拨的一小时真实存在）
    let sun = at_tz(Berlin, 2026, 10, 25, 23, 0, 0);
    let (_, de) = day_bounds(&sun);
    assert_eq!((de - sun).num_hours(), 1);
    let _ = window_bounds(WindowKind::Day, &sun);
}
