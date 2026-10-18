//! RESULT.md ⑤节遗留项补测:DST(春跳/秋拨)与夜班跨午夜。
//!
//! 已知模型约束:`Shift` 强制 start<end(跨午夜窗口在构造期即被拒绝),
//! 夜班 22:00–06:00 只能按 lib.rs 文档建议拆成"两天两窗口":
//!   当天   [22:00, 24:00) —— 24:00 用 `NaiveTime::MAX`(23:59:59.999999999)逼近;
//!   次日凌晨 [00:00, 06:00)。
//! 该拆分在午夜接缝处丢 1ns 工时(半开区间 [22:00, 23:59:59.999999999) 与
//! [00:00, 06:00) 之间恰好空出一个纳秒),本文件如实断言这一副作用并计入结论。
//!
//! "加快时间流动"的全部手段都是注入固定时刻 + 网格扫描(几千个点=毫秒级跑完几年),
//! 无任何真实 sleep。

use af_calendar::{Calendar, CalendarError, Shift};
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::{America::New_York, Asia::Shanghai, Europe::Berlin, Tz};

/// 逼近"24:00"的当天最后时刻 23:59:59.999999999(chrono 0.4 的 NaiveTime::MAX 不公开)。
fn end_of_day() -> NaiveTime {
    NaiveTime::from_hms_nano_opt(23, 59, 59, 999_999_999).unwrap()
}

// ---------- 夹具:夜班两窗口模型 ----------

/// 次日凌晨窗 [00:00, 06:00)。
fn morning() -> Shift {
    Shift::new(NaiveTime::MIN, NaiveTime::from_hms_opt(6, 0, 0).unwrap()).unwrap()
}

/// 一个夜班的本地两窗口:凌晨 [00:00,06:00) + 晚间 [22:00, 24:00)(end_of_day 逼近 24:00)。
/// 名义每晚 8 工时;午夜接缝丢 1ns,实际 8h − 1ns。
fn night_shifts() -> Vec<Shift> {
    vec![
        morning(),
        Shift::new(NaiveTime::from_hms_opt(22, 0, 0).unwrap(), end_of_day()).unwrap(),
    ]
}

/// 全周夜班日历(每晚 22:00–06:00)。
fn night_calendar(tz: Tz) -> Calendar {
    let mut weekly: [Vec<Shift>; 7] = Default::default();
    for d in 0..7 {
        weekly[d] = night_shifts();
    }
    let cal = Calendar::from_weekly(tz, weekly);
    cal.validate().unwrap();
    cal
}

/// 周一~周五夜班,周六日停工(供假日/调休交互)。
fn weekday_night_calendar(tz: Tz) -> Calendar {
    let mut weekly: [Vec<Shift>; 7] = Default::default();
    for d in 0..5 {
        weekly[d] = night_shifts();
    }
    let cal = Calendar::from_weekly(tz, weekly);
    cal.validate().unwrap();
    cal
}

/// 24×7 全开工日历(每天 [00:00, 24:00)):扫描用最强校验——任何时刻都在班次内。
fn always_on_calendar(tz: Tz) -> Calendar {
    let mut weekly: [Vec<Shift>; 7] = Default::default();
    for d in 0..7 {
        weekly[d] = vec![Shift::new(NaiveTime::MIN, end_of_day()).unwrap()];
    }
    let cal = Calendar::from_weekly(tz, weekly);
    cal.validate().unwrap();
    cal
}

fn ber_single(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<Tz> {
    Berlin.with_ymd_and_hms(y, m, d, h, mi, 0)
        .single()
        .expect("唯一存在的柏林本地时刻(非歧义、非间隙)")
}

const NS: Duration = Duration::nanoseconds(1);

// ---------- 1. 夜班跨午夜(两窗口模型) ----------

#[test]
fn night_shift_friday_22h_plus_4h_lands_saturday_02h() {
    let cal = night_calendar(Berlin);
    let fri = ber_single(2026, 6, 5, 22, 0); // 6-05 是周五,周末无 DST 干扰
    assert_eq!(fri.weekday().to_string(), "Fri", "夹具护栏");
    let done = cal.next_working_instant(fri, Duration::hours(4)).unwrap();
    // 周五晚窗报 2h−1ns(接缝损耗),余 2h+1ns 落在周六凌晨窗 → 周六 02:00:00.000000001
    assert_eq!(
        done,
        ber_single(2026, 6, 6, 2, 0) + NS,
        "周五 22:00 + 4 工时 → 周六 02:00(+1ns 午夜接缝)"
    );
    assert!(cal.is_working_instant(done));
    // 完工点距起点流逝 4h+1ns:多出的 1ns 正是接缝损耗
    assert_eq!(
        done.with_timezone(&Utc) - fri.with_timezone(&Utc),
        Duration::hours(4) + NS
    );
}

#[test]
fn night_shift_semantics_identical_without_dst() {
    // 上海无 DST:同一两窗口夜班给出同一钟面结果 → 上面的 +1ns 与 DST 无关,
    // 是"拆两天两窗口"模型本身的接缝损耗。
    let cal = night_calendar(Shanghai);
    let fri = Shanghai.with_ymd_and_hms(2026, 6, 5, 22, 0, 0).single().unwrap();
    let done = cal.next_working_instant(fri, Duration::hours(4)).unwrap();
    assert_eq!(
        done,
        Shanghai.with_ymd_and_hms(2026, 6, 6, 2, 0, 0).single().unwrap() + NS
    );
}

#[test]
fn night_shift_saturday_2330_plus_1h_crosses_to_sunday() {
    let cal = night_calendar(Berlin);
    let sat = ber_single(2026, 6, 6, 23, 30);
    let done = cal.next_working_instant(sat, Duration::hours(1)).unwrap();
    // 23:30 → 23:59:59.999999999 报 29m59.999999999s,余 30m+1ns → 周日 00:30:00.000000001
    assert_eq!(done, ber_single(2026, 6, 7, 0, 30) + NS, "周六 23:30 + 1 工时跨到周日 00:30");
    // 跨越点两侧钟面连续可工:23:59:59 仍在晚窗,00:00 是凌晨窗左端
    assert!(cal.is_working_instant(ber_single(2026, 6, 6, 23, 59)));
    assert!(cal.is_working_instant(ber_single(2026, 6, 7, 0, 0)));
    // 班次外对照:晚班开工前 / 凌晨窗末(右开)不可工
    assert!(!cal.is_working_instant(ber_single(2026, 6, 6, 21, 59)));
    assert!(!cal.is_working_instant(ber_single(2026, 6, 7, 6, 0)));
}

#[test]
fn two_consecutive_nights_forward_reverse_roundtrip() {
    let cal = night_calendar(Berlin);
    let start = ber_single(2026, 6, 5, 22, 0); // 周五 22:00
    // 连续两晚正向:4 工时 → 周六 02:00(+1ns);12 工时跨两晚 → 周日 02:00(+2ns,每跨一夜损 1ns)
    let due4 = cal.next_working_instant(start, Duration::hours(4)).unwrap();
    assert_eq!(due4, ber_single(2026, 6, 6, 2, 0) + NS);
    let due12 = cal.next_working_instant(start, Duration::hours(12)).unwrap();
    assert_eq!(due12, ber_single(2026, 6, 7, 2, 0) + Duration::nanoseconds(2),
        "两晚各损 1ns,累计 +2ns");

    // 逆向(截单)派生与正向互逆:4 工时与 12 工时的截单点都恰回到原始起点周五 22:00
    assert_eq!(
        cal.latest_order_at(due4, Duration::hours(4)).unwrap(),
        start,
        "4 工时逆推应回到周五 22:00"
    );
    assert_eq!(
        cal.latest_order_at(due12, Duration::hours(12)).unwrap(),
        start,
        "12 工时逆推同样回到周五 22:00"
    );
    for (due, req) in [(due4, Duration::hours(4)), (due12, Duration::hours(12))] {
        let latest = cal.latest_order_at(due, req).unwrap();
        let back = cal.next_working_instant(latest, req).unwrap();
        assert_eq!(back, due, "连续两晚正反互逆失败: due={due:?} latest={latest:?}");
    }
}

// ---------- 2. DST 春跳(Europe/Berlin 2026-03-29 02:00→03:00,跳失一小时) ----------

#[test]
fn spring_forward_gap_hour_not_counted_as_working_time() {
    let cal = night_calendar(Berlin);
    // 护栏:2026-03-29 02:30 本地时刻不存在(间隙)
    assert!(Berlin.with_ymd_and_hms(2026, 3, 29, 2, 30, 0).single().is_none());

    // 裁决:凌晨窗 [00:00,06:00) 钟面跨度 6h,流逝容量只有 5h —— 跳失的小时不计工时
    let midnight = ber_single(2026, 3, 29, 0, 0); // 00:00 CET = 前日 23:00Z
    assert_eq!(
        cal.next_working_instant(midnight, Duration::hours(5)).unwrap(),
        ber_single(2026, 3, 29, 6, 0),
        "5 工时恰好填满跨间隙窗口(流逝语义)"
    );
    assert_eq!(
        cal.next_working_instant(midnight, Duration::hours(6)).unwrap(),
        ber_single(2026, 3, 29, 23, 0),
        "6 工时装不下 → 溢出到当晚 22:00 窗口,23:00 完成"
    );

    // 整晚(周六 22:00 起)名义 8h、实际 7h−1ns:7h−1ns 恰在周日 06:00(窗口末)完工
    let sat = ber_single(2026, 3, 28, 22, 0);
    assert_eq!(
        cal.next_working_instant(sat, Duration::hours(7) - NS).unwrap(),
        ber_single(2026, 3, 29, 6, 0)
    );
    assert_eq!(
        cal.next_working_instant(sat, Duration::hours(7)).unwrap(),
        ber_single(2026, 3, 29, 22, 0) + NS,
        "比整晚实际容量多 1ns → 溢出到下一个夜班开头"
    );
}

#[test]
fn spring_forward_addition_into_missing_hour_lands_after_gap() {
    let cal = night_calendar(Berlin);
    let one_am = ber_single(2026, 3, 29, 1, 0); // 01:00 CET = 00:00Z,班次内
    assert!(cal.is_working_instant(one_am));

    // 未触界的加法按普通钟面推进
    assert_eq!(
        cal.next_working_instant(ber_single(2026, 3, 29, 1, 20), Duration::minutes(30)).unwrap(),
        ber_single(2026, 3, 29, 1, 50)
    );
    // 跨缺失段:01:00 + 1 工时 → 本地 03:00 CEST。钟面 +2h、流逝恰好 1h:
    // 02:00–02:59 的钟面不存在,加法按流逝时间直接落到间隙之后第一个可达时刻。
    let done = cal.next_working_instant(one_am, Duration::hours(1)).unwrap();
    assert_eq!(done, ber_single(2026, 3, 29, 3, 0), "跨缺失段的加法落在 03:00");
    assert_eq!(
        done.with_timezone(&Utc) - one_am.with_timezone(&Utc),
        Duration::hours(1),
        "流逝时间恰为 1h(而非钟面 +2h)"
    );
    // 01:30 + 30min 同样触界 → 03:00(钟面 +1h30m,流逝 30min)
    assert_eq!(
        cal.next_working_instant(ber_single(2026, 3, 29, 1, 30), Duration::minutes(30)).unwrap(),
        ber_single(2026, 3, 29, 3, 0)
    );
}

// ---------- 3. DST 秋拨(Europe/Berlin 2026-10-25 03:00→02:00,重复一小时) ----------

#[test]
fn fall_back_repeated_hour_counts_once_as_elapsed_time() {
    let cal = night_calendar(Berlin);
    let first = Berlin.with_ymd_and_hms(2026, 10, 25, 2, 30, 0).earliest().unwrap();
    let second = Berlin.with_ymd_and_hms(2026, 10, 25, 2, 30, 0).latest().unwrap();
    // 歧义时刻的两次出现:第一次 CEST(00:30Z),第二次 CET(01:30Z)
    assert_eq!(first.with_timezone(&Utc), Utc.with_ymd_and_hms(2026, 10, 25, 0, 30, 0).unwrap());
    assert_eq!(second.with_timezone(&Utc), Utc.with_ymd_and_hms(2026, 10, 25, 1, 30, 0).unwrap());

    // 裁决:凌晨窗 [00:00,06:00) 钟面 6h、流逝 7h —— 重复的一小时是真实时间,容量计入一次
    let midnight = Berlin.with_ymd_and_hms(2026, 10, 25, 0, 0, 0).earliest().unwrap(); // 00:00 CEST = 前日 22:00Z
    assert_eq!(
        cal.next_working_instant(midnight, Duration::hours(7)).unwrap(),
        ber_single(2026, 10, 25, 6, 0),
        "7 工时恰好填满秋拨夜的凌晨窗口"
    );
    assert_eq!(
        cal.next_working_instant(midnight, Duration::hours(7) + NS).unwrap(),
        ber_single(2026, 10, 25, 22, 0) + NS,
        "再多 1ns 即溢出到下一个夜班开头"
    );

    // 整晚:周六 22:00 + 8 工时 → 周日 05:00:00.000000001(CET)。
    // 钟面只走 7h+1ns,流逝走 8h+1ns:8 工时是真实的 8 小时,重复小时不重复计数。
    let sat = ber_single(2026, 10, 24, 22, 0);
    let done = cal.next_working_instant(sat, Duration::hours(8)).unwrap();
    assert_eq!(done, ber_single(2026, 10, 25, 5, 0) + NS);
    assert_eq!(
        done.with_timezone(&Utc) - sat.with_timezone(&Utc),
        Duration::hours(8) + NS,
        "流逝 8h+1ns"
    );
    assert_eq!(
        done.with_timezone(&Berlin).naive_local() - sat.with_timezone(&Berlin).naive_local(),
        Duration::hours(7) + NS,
        "钟面只走 7h+1ns"
    );
}

#[test]
fn fall_back_ambiguous_instant_resolution() {
    // (a) is_working_instant 是纯钟面判定,不消歧:两次出现同答"班次内"
    let cal = night_calendar(Berlin);
    let first = Berlin.with_ymd_and_hms(2026, 10, 25, 2, 30, 0).earliest().unwrap();
    let second = Berlin.with_ymd_and_hms(2026, 10, 25, 2, 30, 0).latest().unwrap();
    assert!(cal.is_working_instant(first) && cal.is_working_instant(second));

    // (b) 工时算术按流逝时间:第一次 02:30 + 1 工时 → 第二次出现的 02:30(CET)
    assert_eq!(cal.next_working_instant(first, Duration::hours(1)).unwrap(), second,
        "跨重复段的加法落到第二次出现的同一钟面");
    assert_eq!(
        cal.next_working_instant(second, Duration::hours(1)).unwrap(),
        ber_single(2026, 10, 25, 3, 30),
        "第二次 02:30 + 1 工时 → 03:30(CET,单次出现)"
    );

    // (c) duration=0:两个出现都被判为"班次内可开工" → 原地不动
    assert_eq!(cal.next_working_instant(first, Duration::zero()).unwrap(), first);
    assert_eq!(cal.next_working_instant(second, Duration::zero()).unwrap(), second);

    // (d) 班次边界落在歧义时刻:crate 用 .earliest() 消歧(取较早偏移)。
    // 覆盖 10-25 当天只有 [00:00,02:30) 窗口 → 窗口末按第一次 02:30(CEST,00:30Z)计,容量 2.5h
    let sun = NaiveDate::from_ymd_opt(2026, 10, 25).unwrap();
    let cal2 = Calendar::new_empty(Berlin)
        .with_day_override(sun, vec![Shift::new(
            NaiveTime::MIN,
            NaiveTime::from_hms_opt(2, 30, 0).unwrap(),
        )
        .unwrap()])
        .with_horizon(5);
    cal2.validate().unwrap();
    let midnight = Berlin.with_ymd_and_hms(2026, 10, 25, 0, 0, 0).earliest().unwrap();
    assert_eq!(
        cal2.next_working_instant(midnight, Duration::hours(2) + Duration::minutes(30)).unwrap(),
        first,
        "2.5 工时恰达窗口末 = 第一次出现的 02:30"
    );
    // 若按最晚偏移消歧容量应为 3.5h;实测 2h40m 已装不下 → 证明取较早偏移
    assert_eq!(
        cal2.next_working_instant(midnight, Duration::hours(2) + Duration::minutes(40)),
        Err(CalendarError::HorizonExceeded { searched_days: 5 })
    );
}

// ---------- 4. 夜班班次定义 × 假日/调休交互(RESULT ④的二元假日表语义) ----------

#[test]
fn night_shift_with_holiday_and_makeup_override() {
    let fri = ber_single(2026, 6, 5, 22, 0); // 周五 22:00
    let sat_date = NaiveDate::from_ymd_opt(2026, 6, 6).unwrap();

    // (1) 周末停工:周五 22:00 + 4 工时,周六凌晨窗缺失 → 跳到周一凌晨
    let weekday_cal = weekday_night_calendar(Berlin);
    assert_eq!(
        weekday_cal.next_working_instant(fri, Duration::hours(4)).unwrap(),
        ber_single(2026, 6, 8, 2, 0) + NS,
        "周五尾窗 2h−1ns + 周一凌晨 2h+1ns → 周一 02:00:00.000000001"
    );

    // (2) 调休(非空覆盖=上班):周六只给凌晨窗,周五夜班在周六凌晨收尾
    let makeup_morning = weekday_cal.clone().with_day_override(sat_date, vec![morning()]);
    assert_eq!(
        makeup_morning.next_working_instant(fri, Duration::hours(4)).unwrap(),
        ber_single(2026, 6, 6, 2, 0) + NS
    );
    // 调休给整晚(凌晨+晚间):周五+周六两晚 12 工时。注意 weekday 日历周日全天空窗,
    // 周六夜班的尾巴落不到周日凌晨 → 直落周一凌晨 02:00:00.000000002(每跨一夜损 1ns)
    let makeup_full = weekday_cal.clone().with_day_override(sat_date, night_shifts());
    assert_eq!(
        makeup_full.next_working_instant(fri, Duration::hours(12)).unwrap(),
        ber_single(2026, 6, 8, 2, 0) + Duration::nanoseconds(2)
    );
    // 再调休周日凌晨窗(尾窗):周五夜班 + 调休周六夜班 = 真正连续两晚,12 工时 → 周日 02:00:00.000000002
    let sun_date = NaiveDate::from_ymd_opt(2026, 6, 7).unwrap();
    let makeup_weekend = weekday_cal
        .clone()
        .with_day_override(sat_date, night_shifts())
        .with_day_override(sun_date, vec![morning()]);
    assert_eq!(
        makeup_weekend.next_working_instant(fri, Duration::hours(12)).unwrap(),
        ber_single(2026, 6, 7, 2, 0) + Duration::nanoseconds(2)
    );

    // (3) 假日表(空语义=该日全部窗口取消):周六整日放假
    let holiday = weekday_cal.with_holidays([sat_date]);
    assert!(!holiday.is_working_instant(ber_single(2026, 6, 6, 3, 0)), "假日凌晨不施工");
    assert_eq!(
        holiday.next_working_instant(fri, Duration::hours(3)).unwrap(),
        ber_single(2026, 6, 8, 1, 0) + NS,
        "周五 22:00 + 3 工时:尾窗 2h−1ns,周六假日跳过,周一凌晨报 1h+1ns → 周一 01:00:00.000000001"
    );
    // 逆向同样跳过假日日:周一 01:00:00.000000001 交付 3 工时 → 最晚下单 = 周五 22:00,正反互逆
    let due = ber_single(2026, 6, 8, 1, 0) + NS;
    let latest = holiday.latest_order_at(due, Duration::hours(3)).unwrap();
    assert_eq!(latest, fri, "逆推:周六假日无窗口,工时全部落回周五夜班");
    assert_eq!(holiday.next_working_instant(latest, Duration::hours(3)).unwrap(), due);
}

// ---------- 5. 多年扫描稳定性(互逆性量化) ----------

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Mismatch {
    /// 严格互逆 back == p
    Exact,
    /// 已知坑④-3:交付点在班次外 → 收敛到"≤ due 的最后完工时刻",back < p
    OffShift,
    /// 边界坑(本次新发现并记录):交付点恰在开工点。is_working_instant=true,
    /// 但"恰在该时刻完工"不可达 → 回算点 < p(最晚性保持)
    WindowStart,
}

/// 对单个交付点做"逆向派生 + 正向回算",分类不互逆情形;出现已知坑之外的
/// 不一致(回算超期 / 班次内非开工点不互逆)直接 panic。
fn roundtrip_class<Tz: TimeZone + Clone>(
    cal: &Calendar,
    p: DateTime<Tz>,
    req: Duration,
    label: &str,
) -> Mismatch {
    let latest = cal
        .latest_order_at(p.clone(), req)
        .unwrap_or_else(|e| panic!("{label}: 逆向派生失败 p_utc={:?}: {e}", p.with_timezone(&Utc)));
    let back = cal.next_working_instant(latest.clone(), req).unwrap_or_else(|e| {
        panic!(
            "{label}: 正向回算失败 p_utc={:?} latest_utc={:?}: {e}",
            p.with_timezone(&Utc),
            latest.with_timezone(&Utc)
        )
    });
    if back == p {
        return Mismatch::Exact;
    }
    assert!(
        back <= p,
        "{label}: 回算超过交付点(新不一致!): p_utc={:?} wall={} back_utc={:?} latest_utc={:?}",
        p.with_timezone(&Utc),
        p.with_timezone(&cal.tz),
        back.with_timezone(&Utc),
        latest.with_timezone(&Utc)
    );
    let wall = p.with_timezone(&cal.tz);
    let on_start = cal
        .shifts_on(wall.date_naive())
        .iter()
        .any(|s| s.start == wall.time());
    if !cal.is_working_instant(p.clone()) {
        return Mismatch::OffShift;
    }
    assert!(
        on_start,
        "{label}: 班次内(非开工点)交付点互逆失败 —— 已知坑之外的新不一致!: \
         p_utc={:?} wall={} back_utc={:?} latest_utc={:?}",
        p.with_timezone(&Utc),
        wall,
        back.with_timezone(&Utc),
        latest.with_timezone(&Utc)
    );
    Mismatch::WindowStart
}

/// 从 2024-01-01T00:00Z 起按 `step` 步进扫 3 年(至 2027-01-01,UTC 网格无歧义),
/// 每点做"逆向 8 工时派生 → 正向 8 工时回算"。返回 (点数, 各类不互逆计数)。
fn scan_roundtrips<Tz: TimeZone + Clone>(
    cal: &Calendar,
    tz: Tz,
    label: &str,
    step: Duration,
) -> (usize, [usize; 3]) {
    let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap();
    let req = Duration::hours(8);
    let mut t = start;
    let (mut n, mut counts) = (0usize, [0usize; 3]);
    while t < end {
        let p = t.with_timezone(&tz);
        let class = roundtrip_class(cal, p, req, label);
        counts[class as usize] += 1;
        n += 1;
        t = t + step; // UTC 无 DST,步进无歧义
    }
    (n, counts)
}

#[test]
fn three_year_scan_6h_grid_berlin_and_new_york() {
    // 每时区 4384 个点(1096 天 × 4),覆盖 Berlin/NY 各 3 次 DST 春跳 + 3 次秋拨
    for (tz, name) in [(Berlin, "Europe/Berlin"), (New_York, "America/New_York")] {
        // 夜班日历:不互逆只允许落在已知坑(班次外)与新记录的边界坑(开工点)
        let night = night_calendar(tz);
        let (n, c) = scan_roundtrips(&night, tz, &format!("{name} 夜班 6h 网格"), Duration::hours(6));
        assert_eq!(n, 4384, "扫描点数(1096 天 × 4)");
        assert_eq!(c[2], 0, "6h UTC 网格不应落到本地开工点");
        assert!(c[1] > 0, "夜班日历必然存在班次外交付点样本(已知坑④-3)");
        assert_eq!(c[0] + c[1], n, "除已知坑外不允许任何不互逆点");

        // 24×7 日历:任何交付点都在班次内 → 除开工点外必须逐点严格互逆(最强校验)
        let alwayson = always_on_calendar(tz);
        let (n2, c2) =
            scan_roundtrips(&alwayson, tz, &format!("{name} 24×7 6h 网格"), Duration::hours(6));
        assert_eq!(n2, 4384);
        assert_eq!(c2[1] + c2[2], 0, "24×7 日历 6h UTC 网格上必须全部严格互逆(含全部 DST 边界日)");
    }
}

/// 慢速全表扫描(#[ignore],`cargo test -p af-calendar -- --ignored` 执行):
/// 1h 网格 3 年 × 2 时区 × 2 日历 ≈ 10.5 万个交付点。
/// 1h 网格会精确命中:本地开工点 00:00/22:00、全部 DST 转换时刻
/// (Berlin 01:00Z、NY 06:00Z/07:00Z)与重复小时的两次出现。
#[test]
#[ignore = "慢速全表扫描:约 10.5 万点,毫秒-秒级,验证用"]
fn dense_hourly_scan_3y_berlin_and_new_york() {
    for (tz, name) in [(Berlin, "Europe/Berlin"), (New_York, "America/New_York")] {
        let night = night_calendar(tz);
        let (n, c) = scan_roundtrips(&night, tz, &format!("{name} 夜班 1h 网格"), Duration::hours(1));
        assert_eq!(n, 26304);
        assert!(c[2] > 0, "1h 网格应命中本地开工点(00:00/22:00)样本");
        assert!(c[1] > 0);
        assert_eq!(c[0] + c[1] + c[2], n, "除已知坑/开工点边界坑外不允许任何不互逆点");

        let alwayson = always_on_calendar(tz);
        let (n2, c2) =
            scan_roundtrips(&alwayson, tz, &format!("{name} 24×7 1h 网格"), Duration::hours(1));
        assert_eq!(n2, 26304);
        assert!(c2[2] > 0, "1h 网格应命中每日开工点样本");
        assert_eq!(c2[1], 0);
        assert_eq!(c2[0] + c2[2], n2, "24×7 除开工点外全部严格互逆");
    }
}

// ---------- 6. 新发现边界坑的定点记录(不修,由人拍板) ----------

#[test]
fn due_at_window_start_is_not_completion_reachable() {
    // 交付承诺恰在开工点(周一 00:00):is_working_instant=true(窗口左闭),
    // 但"恰好在该时刻完工"不可达 —— 完工时刻只可能严格晚于开工点。
    // 逆向派生把该班次整窗跳过(ws >= bound),回算点落得更早;≤ 与最晚性保持。
    // 与 sla.rs 中"due 恰在开工点"的 slack 情形一致,但 lib.rs 文档
    // "due 在班次内恒互逆"的措辞在开工点不成立,建议修文档或修语义(此处只记录)。
    let cal = night_calendar(Shanghai); // 无 DST,隔离变量
    let mon = Shanghai.with_ymd_and_hms(2026, 6, 8, 0, 0, 0).single().unwrap();
    assert_eq!(mon.weekday().to_string(), "Mon", "夹具护栏");
    assert!(cal.is_working_instant(mon), "开工点是班次内时刻");

    let latest = cal.latest_order_at(mon, Duration::hours(8)).unwrap();
    let back = cal.next_working_instant(latest, Duration::hours(8)).unwrap();
    assert!(back < mon, "开工点不可作为完工时刻:回算 {back:?} < due {mon:?}");

    // 最晚性:下单点再晚 1ns,完工必然错过交付承诺
    let late = cal
        .next_working_instant(latest + Duration::nanoseconds(1), Duration::hours(8))
        .unwrap();
    assert!(late > mon, "派生仍是最晚可行下单点");
}
