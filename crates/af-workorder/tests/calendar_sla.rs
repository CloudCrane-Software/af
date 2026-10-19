//! 日历解析出的 deadline 进入工单后的 SLA 判定。
//!
//! 时刻全部由测试注入,不读系统钟,不含 sleep。
//! DST 与 af-calendar 已验证的裁决一致:工时按流逝时间、窗口按本地钟面。
//! 本文件不重测日历推进算法,只锁"解析结果以 `DateTime<Utc>` 存进订单后,
//! 生命周期内的逾期判定比较的是这个绝对时刻"。

use af_calendar::Shift;
use af_workorder::{
    BudgetVoucher, Calendar, GraphVersion, LineBinding, OrderState, Priority, Sla, WorkArtifact,
    WorkOrder,
};
use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use chrono_tz::Europe::Berlin;

fn utc_hms(y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, mi, s).unwrap()
}

/// 唯一存在的柏林本地时刻(非间隙、非歧义)。
fn berlin(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<chrono_tz::Tz> {
    Berlin
        .with_ymd_and_hms(y, m, d, h, mi, 0)
        .single()
        .expect("该本地时刻唯一存在")
}

/// 工作日 08:00–17:00,Europe/Berlin。周一至周五,与上海题设日历同一班次形状。
fn berlin_weekday() -> Calendar {
    let cal = Calendar::weekday_single_shift(Berlin, Shift::hm(8, 0, 17, 0).unwrap());
    cal.validate().unwrap();
    cal
}

fn arm(wo: &mut WorkOrder) {
    wo.bind_line(LineBinding {
        line_id: "line-A".into(),
        line_version: "lv-7".into(),
    })
    .unwrap();
    wo.attach_budget_voucher(BudgetVoucher {
        voucher_id: "bv-1".into(),
        amount: 500,
        signed: true,
    })
    .unwrap();
}

fn close_with_final(wo: &mut WorkOrder) {
    wo.attach_artifact(WorkArtifact {
        artifact_id: "fa-1".into(),
        is_final: true,
        qualified: true,
    })
    .unwrap();
    wo.complete().unwrap();
    wo.close().unwrap();
}

#[test]
fn calendar_deadline_enters_lifecycle_and_judges_by_absolute_instant() {
    // 题设日历:周五 16:00+08 下单,承诺 4 工时 → 下周一 11:00+08 = 03:00Z。
    // 周五 16:00–17:00 消耗 1 工时,余 3 工时落到周一 08:00–11:00。
    let cal = Calendar::weekday_8x5_shanghai();
    cal.validate().unwrap();
    let placed = DateTime::parse_from_rfc3339("2026-10-23T16:00:00+08:00").unwrap();
    assert_eq!(placed.weekday().to_string(), "Fri", "夹具护栏");

    let sla = Sla::from_calendar(&cal, placed, Duration::hours(4), Priority::Critical).unwrap();
    let deadline = utc_hms(2026, 10, 26, 3, 0, 0);
    assert_eq!(sla.deadline, Some(deadline));

    let mut wo = WorkOrder::new("wo-cal", GraphVersion::new("g-3"), sla);
    assert_eq!(wo.state(), OrderState::Created);
    // 周五下班 17:00+08 = 09:00Z:只走完 1 个工时,离周一截止还远。
    assert!(!wo.is_overdue(utc_hms(2026, 10, 23, 9, 0, 0)));
    // 10:59+08 = 02:59Z,绝对时刻仍早于 03:00Z。
    // 旧字典序会把 "2026-10-26T03:00:00Z" < "2026-10-26T10:59:00+08:00" 判成已逾期。
    let trap = DateTime::parse_from_rfc3339("2026-10-26T10:59:00+08:00")
        .unwrap()
        .with_timezone(&Utc);
    assert_eq!(trap, utc_hms(2026, 10, 26, 2, 59, 0));
    assert!(!wo.is_overdue(trap));
    assert!(!wo.is_overdue(deadline), "恰在截止不算逾期");

    arm(&mut wo);
    assert_eq!(wo.release().unwrap(), OrderState::Released);
    assert_eq!(wo.start().unwrap(), OrderState::InProgress);
    assert_eq!(wo.sla().deadline, Some(deadline), "开工不改写已解析的截止");
    assert!(!wo.is_overdue(trap));
    assert_eq!(wo.graph_version().as_str(), "g-3");

    close_with_final(&mut wo);
    assert_eq!(wo.state(), OrderState::Closed);
    assert_eq!(wo.history().len(), 4);
    assert_eq!(
        wo.sla().deadline,
        Some(deadline),
        "关单后截止仍是同一个绝对时刻"
    );
    assert_eq!(wo.sla().priority, Priority::Critical);
    assert!(!wo.is_overdue(deadline));
    assert!(wo.is_overdue(deadline + Duration::seconds(1)));
}

#[test]
fn spring_forward_order_deadline_is_post_gap_utc_instant() {
    // Europe/Berlin 2026-03-29 02:00 CET → 03:00 CEST。
    // 周五 16:00 CET + 4 工时 → 周一 11:00 CEST = 09:00Z。
    // 若忽略春跳、把周一 11:00 仍按 CET(+01)折算,会得到 10:00Z,晚一小时。
    let cal = berlin_weekday();
    assert!(
        Berlin
            .with_ymd_and_hms(2026, 3, 29, 2, 30, 0)
            .single()
            .is_none(),
        "护栏:春跳间隙 02:30 不存在"
    );
    let placed = berlin(2026, 3, 27, 16, 0);
    assert_eq!(placed.weekday().to_string(), "Fri");
    assert_eq!(placed.with_timezone(&Utc), utc_hms(2026, 3, 27, 15, 0, 0));

    let sla = Sla::from_calendar(&cal, placed, Duration::hours(4), Priority::High).unwrap();
    let deadline = utc_hms(2026, 3, 30, 9, 0, 0);
    assert_eq!(sla.deadline, Some(deadline));
    assert_eq!(
        berlin(2026, 3, 30, 11, 0).with_timezone(&Utc),
        deadline,
        "周一 11:00 的偏移是 CEST(+02)"
    );

    let mut wo = WorkOrder::new("wo-spring", GraphVersion::new("g-3"), sla);
    // 间隙刚结束:周日 03:00 CEST = 01:00Z,仍是周末,未到周一截止。
    let just_after_gap = berlin(2026, 3, 29, 3, 0).with_timezone(&Utc);
    assert_eq!(just_after_gap, utc_hms(2026, 3, 29, 1, 0, 0));
    assert!(!wo.is_overdue(just_after_gap));

    arm(&mut wo);
    wo.release().unwrap();
    wo.start().unwrap();
    assert_eq!(wo.state(), OrderState::InProgress);
    assert_eq!(wo.sla().deadline, Some(deadline));

    // 周一 11:30 CEST = 09:30Z:已过真截止。固定 +01 的假截止 10:00Z 这时还没到。
    let probe = utc_hms(2026, 3, 30, 9, 30, 0);
    let false_fixed_cet = utc_hms(2026, 3, 30, 10, 0, 0);
    assert!(deadline < probe && probe < false_fixed_cet);
    assert!(wo.is_overdue(probe));
    assert!(!wo.is_overdue(deadline));

    close_with_final(&mut wo);
    assert_eq!(wo.state(), OrderState::Closed);
    assert_eq!(
        wo.sla().deadline,
        Some(deadline),
        "跨春跳的截止在关单后仍是同一个绝对时刻"
    );
    assert!(wo.is_overdue(deadline + Duration::seconds(1)));
}

#[test]
fn fall_back_order_deadline_is_post_fold_utc_instant() {
    // Europe/Berlin 2026-10-25 03:00 CEST → 02:00 CET。
    // 周五 16:00 CEST + 4 工时 → 周一 11:00 CET = 10:00Z。
    // 若夏令时偏移残留、把周一 11:00 仍按 CEST(+02)折算,会得到 09:00Z,早一小时。
    let cal = berlin_weekday();
    let placed = berlin(2026, 10, 23, 16, 0);
    assert_eq!(placed.weekday().to_string(), "Fri");
    assert_eq!(
        placed.with_timezone(&Utc),
        utc_hms(2026, 10, 23, 14, 0, 0),
        "下单当天下午仍是夏令时"
    );

    let sla = Sla::from_calendar(&cal, placed, Duration::hours(4), Priority::High).unwrap();
    let deadline = utc_hms(2026, 10, 26, 10, 0, 0);
    assert_eq!(sla.deadline, Some(deadline));
    assert_eq!(
        berlin(2026, 10, 26, 11, 0).with_timezone(&Utc),
        deadline,
        "周一 11:00 的偏移已是 CET(+01)"
    );

    let mut wo = WorkOrder::new("wo-fall", GraphVersion::new("g-3"), sla);
    // 拨回之后:周日 03:00 CET = 02:00Z,周末,未逾期。
    let just_after_fold = berlin(2026, 10, 25, 3, 0).with_timezone(&Utc);
    assert_eq!(just_after_fold, utc_hms(2026, 10, 25, 2, 0, 0));
    assert!(!wo.is_overdue(just_after_fold));

    arm(&mut wo);
    wo.release().unwrap();
    wo.start().unwrap();
    assert_eq!(wo.sla().deadline, Some(deadline));

    // 周一 10:30 CET = 09:30Z:早于真截止 10:00Z。残留夏令时的假截止 09:00Z 这时已经过了。
    let probe = utc_hms(2026, 10, 26, 9, 30, 0);
    let false_lingering_cest = utc_hms(2026, 10, 26, 9, 0, 0);
    assert!(false_lingering_cest < probe && probe < deadline);
    assert!(!wo.is_overdue(probe));
    assert!(!wo.is_overdue(deadline), "恰在截止不算逾期");
    assert!(wo.is_overdue(deadline + Duration::seconds(1)));

    close_with_final(&mut wo);
    assert_eq!(wo.state(), OrderState::Closed);
    assert_eq!(wo.sla().deadline, Some(deadline));
    assert_eq!(wo.graph_version().as_str(), "g-3");
}
