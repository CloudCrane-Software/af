//! 题设验收场景 + 跨时区 + 假日表(黄金周/调休)集成测试。
//!
//! 日历:工作日(周一~周五)08:00–17:00 Asia/Shanghai;假日表为**测试夹具数据**
//! (2026 国庆周 10-01..10-07 停工;09-26 周六调休上班),非官方放假安排。

use af_calendar::{Calendar, CalendarError, Shift};
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Timelike, Utc};
use chrono_tz::{Asia::Shanghai, Tz};

/// 题设日历 + 测试夹具假日表(黄金周与调休)。
fn fixture_calendar() -> Calendar {
    let golden_week: Vec<NaiveDate> = (1..=7)
        .map(|d| NaiveDate::from_ymd_opt(2026, 10, d).unwrap())
        .collect();
    Calendar::weekday_8x5_shanghai()
        .with_holidays(golden_week)
        // 调休:2026-09-26 是周六,覆盖为正常白班
        .with_day_override(
            NaiveDate::from_ymd_opt(2026, 9, 26).unwrap(),
            vec![Shift::hm(8, 0, 17, 0).unwrap()],
        )
}

fn sh(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Tz> {
    Shanghai
        .with_ymd_and_hms(y, m, d, h, min, 0)
        .single()
        .unwrap()
}

// ---------- 题设验收 1:周五 16:00 下单承诺 4 工时 → 下周一截止 ----------

#[test]
fn friday_16h_order_4h_deadline_is_next_monday() {
    let cal = fixture_calendar();
    let order = sh(2026, 10, 23, 16, 0); // 周五 16:00(2026-10-23)
    assert_eq!(
        order.weekday().to_string(),
        "Fri",
        "夹具日期必须是周五(测试自身的护栏)"
    );
    let deadline = cal.sla_deadline(order, Duration::hours(4)).unwrap();
    // 周五 16:00-17:00 报 1 工时,余 3 工时落到下周一 08:00-11:00
    assert_eq!(deadline, sh(2026, 10, 26, 11, 0));
    assert_eq!(deadline.weekday().to_string(), "Mon");
}

// ---------- 题设验收 2:节假日跨越正确 ----------

#[test]
fn golden_week_crossing_holiday_table() {
    let cal = fixture_calendar();
    // 国庆假期前一天(周三 09-30)16:00 下单 4 工时:
    // 周三 16-17 报 1 工时 → 黄金周整周停工 → 10-08(周四)08:00-11:00 完成
    let order = sh(2026, 9, 30, 16, 0);
    assert_eq!(order.weekday().to_string(), "Wed");
    let deadline = cal.sla_deadline(order, Duration::hours(4)).unwrap();
    assert_eq!(deadline, sh(2026, 10, 8, 11, 0));
}

#[test]
fn makeup_workday_saturday_counts_as_working() {
    let cal = fixture_calendar();
    // 调休:09-25(周五)16:00 + 4 工时 → 09-26(周六,调休上班)11:00
    let order = sh(2026, 9, 25, 16, 0);
    assert_eq!(order.weekday().to_string(), "Fri");
    let deadline = cal.sla_deadline(order, Duration::hours(4)).unwrap();
    assert_eq!(deadline, sh(2026, 9, 26, 11, 0));
    assert_eq!(deadline.weekday().to_string(), "Sat");

    // 无覆盖的普通周六仍然停工:09-19(周六)任意时刻 + 1 工时 → 下周一开工
    let sat = sh(2026, 9, 19, 10, 0);
    assert_eq!(sat.weekday().to_string(), "Sat");
    assert_eq!(
        cal.sla_deadline(sat, Duration::hours(1)).unwrap(),
        sh(2026, 9, 21, 9, 0)
    );
}

#[test]
fn is_working_instant_respects_holiday_and_override() {
    let cal = fixture_calendar();
    assert!(cal.is_working_instant(sh(2026, 9, 30, 10, 0))); // 假日前的工作日
    assert!(!cal.is_working_instant(sh(2026, 10, 1, 10, 0))); // 黄金周四
    assert!(!cal.is_working_instant(sh(2026, 10, 3, 10, 0))); // 黄金周六
    assert!(cal.is_working_instant(sh(2026, 9, 26, 10, 0))); // 调休周六
    assert!(!cal.is_working_instant(sh(2026, 10, 10, 10, 0))); // 未调休的周六
}

// ---------- 跨时区:同一日历,UTC 消费者 ----------

#[test]
fn utc_consumer_gets_same_instant_in_utc() {
    let cal = fixture_calendar();
    // 用 UTC 直接下单(海外系统视图):周五 16:00+08:00 == 08:00Z
    let order_utc = Utc.with_ymd_and_hms(2026, 10, 23, 8, 0, 0).unwrap();
    let deadline_utc = cal.sla_deadline(order_utc, Duration::hours(4)).unwrap();
    // 与上海时区下单结果必须是同一时刻,且返回类型保持 UTC
    assert_eq!(deadline_utc, Utc.with_ymd_and_hms(2026, 10, 26, 3, 0, 0).unwrap());
    let deadline_sh = cal
        .sla_deadline(order_utc.with_timezone(&Shanghai), Duration::hours(4))
        .unwrap();
    assert_eq!(deadline_utc, deadline_sh.with_timezone(&Utc));
    // 日历时区不受调用方时区污染:同一时刻在上海读回是 11:00
    assert_eq!(deadline_sh.format("%H:%M").to_string(), "11:00");
}

#[test]
fn fixed_offset_input_also_accepted() {
    let cal = Calendar::weekday_8x5_shanghai();
    let order = DateTime::parse_from_rfc3339("2026-10-23T16:00:00+08:00").unwrap();
    let deadline = cal.sla_deadline(order, Duration::hours(4)).unwrap();
    assert_eq!(
        deadline.to_rfc3339(),
        "2026-10-26T11:00:00+08:00",
        "返回保持输入的 FixedOffset 时区"
    );
}

// ---------- 工时推进边界 ----------

#[test]
fn off_hours_and_weekend_roll_to_next_open() {
    let cal = Calendar::weekday_8x5_shanghai();
    // 周五 20:00(下班后)+ 1 工时 → 下周一 09:00(开工后 1 小时)
    assert_eq!(
        cal.next_working_instant(sh(2026, 10, 23, 20, 0), Duration::hours(1))
            .unwrap(),
        sh(2026, 10, 26, 9, 0)
    );
    // 周六中午 + 1 工时 → 下周一 09:00
    assert_eq!(
        cal.next_working_instant(sh(2026, 10, 24, 12, 0), Duration::hours(1))
            .unwrap(),
        sh(2026, 10, 26, 9, 0)
    );
    // 班次内整点续算:周三 10:00 + 8 工时 → 周三 10-17 报 7 工时,余 1 → 周四 09:00
    assert_eq!(
        cal.next_working_instant(sh(2026, 10, 21, 10, 0), Duration::hours(8))
            .unwrap(),
        sh(2026, 10, 22, 9, 0)
    );
}

#[test]
fn zero_duration_returns_next_startable_instant() {
    let cal = Calendar::weekday_8x5_shanghai();
    // 班次内 → 原地不动
    assert_eq!(
        cal.next_working_instant(sh(2026, 10, 23, 16, 0), Duration::zero())
            .unwrap(),
        sh(2026, 10, 23, 16, 0)
    );
    // 深夜 → 次日(工作日)开工点
    assert_eq!(
        cal.next_working_instant(sh(2026, 10, 23, 23, 59), Duration::zero())
            .unwrap(),
        sh(2026, 10, 26, 8, 0)
    );
}

#[test]
fn negative_duration_rejected() {
    let cal = Calendar::weekday_8x5_shanghai();
    assert_eq!(
        cal.next_working_instant(sh(2026, 10, 23, 16, 0), Duration::hours(-1)),
        Err(CalendarError::NegativeDuration { seconds: -3600 })
    );
}

#[test]
fn horizon_exceeded_when_never_enough_working_time() {
    let cal = Calendar::weekday_8x5_shanghai().with_horizon(14);
    let r = cal.sla_deadline(sh(2026, 10, 23, 16, 0), Duration::hours(10_000));
    assert!(matches!(r, Err(CalendarError::HorizonExceeded { searched_days: 14 })));
    // 空日历同样触发视界错误而不是死循环
    let empty = Calendar::new_empty(Shanghai).with_horizon(30);
    assert!(empty
        .next_working_instant(sh(2026, 10, 23, 16, 0), Duration::hours(1))
        .is_err());
}

// ---------- 截单派生(反向)与正反互逆 ----------

#[test]
fn latest_order_roundtrip_forward_equals_due() {
    let cal = fixture_calendar();
    // 情形 A:交付点在班次内部(工时可达)→ 正反严格互逆:
    // next_working_instant(latest_order_at(due, r), r) == due
    let exact_cases = [
        (sh(2026, 10, 26, 11, 0), Duration::hours(4)), // 题设场景的逆:应回到周五 16:00
        (sh(2026, 10, 28, 17, 0), Duration::hours(9)), // due 恰在收工点,恰好一整天
        (sh(2026, 10, 28, 12, 30), Duration::hours(3)), // due 在班次内
        (sh(2026, 10, 8, 11, 0), Duration::hours(4)),  // 交付点刚跨过黄金周
    ];
    for (due, req) in exact_cases {
        let latest = cal.latest_order_at(due, req).unwrap();
        let back = cal.next_working_instant(latest, req).unwrap();
        assert_eq!(back, due, "正反互逆失败:due={due} req={req} latest={latest}");
    }
    // 题设场景正向结果的逆过程:周一 11:00 - 4 工时 = 周五 16:00
    assert_eq!(
        cal.latest_order_at(sh(2026, 10, 26, 11, 0), Duration::hours(4))
            .unwrap(),
        sh(2026, 10, 23, 16, 0)
    );
}

#[test]
fn latest_order_is_maximal_even_when_due_off_shift() {
    let cal = fixture_calendar();
    // 情形 B:交付点不在班次内(下班后/开工前)→ 完工时刻只可能是工时时刻,
    // 逆向派生收敛到"≤ due 的最后一个工时时刻",正反互逆变为 back ≤ due;
    // 但派生仍是**最晚**可行下单点:再晚 1 分钟,完工必然错过交付承诺。
    let slack_cases = [
        (sh(2026, 10, 27, 8, 0), Duration::hours(8)), // due 恰在开工点:凑不满"恰好在 08:00 完工"
        (sh(2026, 10, 27, 20, 0), Duration::hours(2)), // due 在下班后:完工最晚 17:00
    ];
    for (due, req) in slack_cases {
        let latest = cal.latest_order_at(due, req).unwrap();
        let back = cal.next_working_instant(latest, req).unwrap();
        assert!(back <= due, "回算超期:due={due} req={req} latest={latest}");
        // 最晚性:下单点推后 1 分钟即不可行
        let late_by_1min = cal
            .next_working_instant(latest + Duration::minutes(1), req)
            .unwrap();
        assert!(
            late_by_1min > due,
            "派生不是最晚:due={due} req={req} latest={latest} +1min 仍可行({late_by_1min})"
        );
    }
    // 语义接缝(如实记录):req = 0 且 due 在班次外时,latest_order_at 返回的是
    // "≤ due 的最后一个**完工**时刻"(窗口末算完工,如周一 17:00);而正向 0 工时问的是
    // "下一个可**开工**时刻"(周二 08:00)——两个谓词不同,back ≤ due 不适用;
    // 最晚性仍然成立(再晚任一时刻,0 工时的"完工"按完工资历也在 due 之前仍可行,
    // 但日历上已无更晚的工时时刻)。
    let zero_latest = cal.latest_order_at(sh(2026, 10, 27, 6, 0), Duration::zero()).unwrap();
    assert_eq!(zero_latest, sh(2026, 10, 26, 17, 0), "≤ due 的最后完工时刻 = 周一 17:00");
    // 具体值:周二 20:00 交付、2 工时 → 最晚下单 = 周二 15:00(15:00+2h=17:00 ≤ 20:00)
    assert_eq!(
        cal.latest_order_at(sh(2026, 10, 27, 20, 0), Duration::hours(2))
            .unwrap(),
        sh(2026, 10, 27, 15, 0)
    );
}

#[test]
fn latest_order_across_golden_week_and_offset_epoch() {
    let cal = fixture_calendar();
    // 交付承诺 10-08(黄金周后首个工作日)15:00,需 10 工时:
    // 逆推 10-08 08:00-15:00 报 7 工时 → 黄金周与 09-30 全部不可用(09-30 只能算到 17:00)
    // → 09-30(周三)再逆推 3 工时 = 09-30 14:00?不对:09-30 08:00-17:00 有 9 工时,
    //    其中只有 16:00 之后…逐窗逆推:10-08 给 7h,余 3h → 09-30 从 17:00 退 3h = 14:00。
    //    但注意 09-29/09-28 等也在日历上,只是 3h 在 09-30 当天足够,故落 09-30 14:00。
    let latest = cal
        .latest_order_at(sh(2026, 10, 8, 15, 0), Duration::hours(10))
        .unwrap();
    assert_eq!(latest, sh(2026, 9, 30, 14, 0));
}

#[test]
fn utc_due_gives_utc_latest_order() {
    let cal = Calendar::weekday_8x5_shanghai();
    let due_utc = Utc.with_ymd_and_hms(2026, 10, 26, 3, 0, 0).unwrap(); // 周一 11:00+08
    let latest = cal.latest_order_at(due_utc, Duration::hours(4)).unwrap();
    assert_eq!(latest, Utc.with_ymd_and_hms(2026, 10, 23, 8, 0, 0).unwrap());
}

// ---------- 多班次(午休拆窗)与配置校验 ----------

#[test]
fn multi_shift_calendar_skips_lunch() {
    // 09:00-12:00 + 13:00-18:00(午休拆两班)
    let mut weekly: [Vec<Shift>; 7] = Default::default();
    for d in 0..5 {
        weekly[d] = vec![Shift::hm(9, 0, 12, 0).unwrap(), Shift::hm(13, 0, 18, 0).unwrap()];
    }
    let cal = Calendar::from_weekly(Shanghai, weekly);
    cal.validate().unwrap();

    // 11:00 + 3 工时:11-12 报 1h,午休跳过,13-16 报 2h → 15:00
    assert_eq!(
        cal.next_working_instant(sh(2026, 10, 20, 11, 0), Duration::hours(3))
            .unwrap(),
        sh(2026, 10, 20, 15, 0)
    );
    // 11:30 + 1 工时 → 跨午休 → 13:30
    assert_eq!(
        cal.next_working_instant(sh(2026, 10, 20, 11, 30), Duration::hours(1))
            .unwrap(),
        sh(2026, 10, 20, 13, 30)
    );
    // 反向同样跳午休:17:00 交付 - 5 工时 = 13:00 起算(13-17 恰 4h)→ 余 1h 落 11:00-12:00 → 11:00
    assert_eq!(
        cal.latest_order_at(sh(2026, 10, 20, 17, 0), Duration::hours(5))
            .unwrap(),
        sh(2026, 10, 20, 11, 0)
    );
}

#[test]
fn overlapping_shifts_fail_validation() {
    let mut weekly: [Vec<Shift>; 7] = Default::default();
    weekly[0] = vec![Shift::hm(8, 0, 13, 0).unwrap(), Shift::hm(12, 0, 17, 0).unwrap()];
    let cal = Calendar::from_weekly(Shanghai, weekly);
    assert!(matches!(
        cal.validate(),
        Err(CalendarError::OverlappingWindows { .. })
    ));
    assert!(Shift::new(
        chrono::NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        chrono::NaiveTime::from_hms_opt(2, 0, 0).unwrap()
    ) == Err(CalendarError::WindowInverted {
        start: chrono::NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
        end: chrono::NaiveTime::from_hms_opt(2, 0, 0).unwrap(),
    }));
}

#[test]
fn json_config_roundtrip_reproduces_deadline() {
    let cal = fixture_calendar();
    let json = serde_json::to_string_pretty(&cal).unwrap();
    let reloaded: Calendar = serde_json::from_str(&json).unwrap();
    reloaded.validate().unwrap();
    let order = sh(2026, 10, 23, 16, 0);
    assert_eq!(
        reloaded.sla_deadline(order, Duration::hours(4)).unwrap(),
        cal.sla_deadline(order, Duration::hours(4)).unwrap()
    );
    // 配置里确实带了假日表与调休覆盖
    assert!(json.contains("\"2026-10-01\""));
    assert!(json.contains("\"2026-09-26\""));
}

#[test]
fn exact_window_end_means_hours_exhausted_there() {
    let cal = Calendar::weekday_8x5_shanghai();
    // 13:00 + 4 工时 = 17:00 整:窗口末即完工
    assert_eq!(
        cal.next_working_instant(sh(2026, 10, 21, 13, 0), Duration::hours(4))
            .unwrap(),
        sh(2026, 10, 21, 17, 0)
    );
    // 多 1 秒即滚到次日开工点后 1 秒:周四 08:00:01
    let d = cal
        .next_working_instant(sh(2026, 10, 21, 13, 0), Duration::hours(4) + Duration::seconds(1))
        .unwrap();
    assert_eq!(d, sh(2026, 10, 22, 8, 0) + Duration::seconds(1));
}
