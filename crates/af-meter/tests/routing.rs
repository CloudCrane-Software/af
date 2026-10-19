//! 路由消费场景的集成测试。
//!
//! 核心验证：**日窗耗尽但周窗有余 → 日窗恢复前任务转 API，日窗重置后切回订阅。**

use af_meter::demo::{self, DAY, SCHEDULE, WEEK};
use af_meter::{route, Decision, Meter, Ticks};

#[test]
fn daily_exhausted_but_weekly_surplus_switches_to_api_until_daily_resets() {
    let mut supplies = demo::build_supplies();

    // 第 0 天：120 + 80 两单把日窗（200）用满，周窗剩 800。
    assert_eq!(
        demo::used(&route(&mut supplies, 120, 0)),
        Some("subscription")
    );
    assert_eq!(
        demo::used(&route(&mut supplies, 80, 3_600)),
        Some("subscription")
    );

    // 周窗有余：周窗还剩 800。
    let sub = demo::find(&supplies, "subscription");
    let Meter::Min(kids) = &sub.meter else {
        panic!("订阅应是 Min(周窗, 日窗)")
    };
    let Meter::Window {
        remaining: week_remaining,
        ..
    } = &kids[0]
    else {
        panic!("第一个子计量应是周窗")
    };
    assert_eq!(*week_remaining, 800, "周窗应余 800");

    // 日窗耗尽：订阅 available=0，最早恢复时刻 = 次日 00:00（日窗重置）。
    let r = demo::find_mut(&mut supplies, "subscription")
        .meter
        .given(7_200);
    assert_eq!(r.available, 0);
    assert_eq!(r.recovers_at, Some(Ticks(DAY)));

    // 日窗恢复前 → 转 API。
    assert_eq!(demo::used(&route(&mut supplies, 5, 7_200)), Some("api"));
    assert_eq!(demo::used(&route(&mut supplies, 7, 80_000)), Some("api"));

    // 次日 01:00（日窗已重置）→ 切回订阅。
    assert_eq!(
        demo::used(&route(&mut supplies, 150, 90_000)),
        Some("subscription")
    );
}

#[test]
fn estimate_above_daily_capacity_routes_to_api() {
    let mut supplies = demo::build_supplies();
    // 日窗满、周窗满，但预计消耗 300 > 日窗上限 200：订阅在日窗恢复前永远覆盖不了 → API。
    assert_eq!(demo::used(&route(&mut supplies, 300, 90_000)), Some("api"));
    // 后续小任务仍走订阅。
    assert_eq!(
        demo::used(&route(&mut supplies, 150, 90_001)),
        Some("subscription")
    );
}

#[test]
fn golden_timeline_over_schedule() {
    let mut supplies = demo::build_supplies();
    let expected = [
        "subscription", // d0 00:00 need=120
        "subscription", // d0 01:00 need=80（日窗耗尽）
        "api",          // d0 02:00 need=5（日窗恢复前）
        "api",          // d0 22:13 need=7
        "subscription", // d1 01:00 need=150（日窗重置，切回）
        "api",          // d1 02:23 need=300（超日窗上限）
        "api",          // d5 18:53 need=250（min(周650,日200)=200 不够）
        "subscription", // d7 00:00 need=150（周窗重置）
    ];
    for (i, &(t, need)) in SCHEDULE.iter().enumerate() {
        let d = route(&mut supplies, need, t);
        assert_eq!(
            demo::used(&d),
            Some(expected[i]),
            "第 {i} 个任务 t={t} need={need}"
        );
    }
    // API 只被扣 5+7+300+250 = 562。
    let api = demo::find(&supplies, "api");
    let Meter::Stock { remaining } = &api.meter else {
        panic!("API 应是 Stock")
    };
    assert_eq!(*remaining, 9_438, "API 应剩 9438");
    // 订阅周窗 1000 - 120 - 80 - 150 = 850。
    let sub = demo::find(&supplies, "subscription");
    let Meter::Min(kids) = &sub.meter else {
        panic!("订阅应是 Min")
    };
    let Meter::Window { remaining: wk, .. } = &kids[0] else {
        panic!("第一个子计量应是周窗")
    };
    assert_eq!(*wk, 850, "周窗应剩 850");
}

#[test]
fn weekly_binding_recovers_at_weekly_reset() {
    let mut supplies = demo::build_supplies();
    // 连吃 5 天满日窗（每天 200），把周窗 1000 也吃光。
    for k in 0..5u32 {
        let t = u64::from(k) * DAY + 1;
        assert_eq!(
            demo::used(&route(&mut supplies, 200, t)),
            Some("subscription"),
            "第 {k} 天"
        );
    }
    // 第 5 天日窗仍有 200，但周窗为 0 → 转 API（周窗绑定）。
    assert_eq!(
        demo::used(&route(&mut supplies, 10, 4 * DAY + 100)),
        Some("api")
    );
    // 恢复时刻 = 周窗重置（t=WEEK），而非日窗重置。
    let r = demo::find_mut(&mut supplies, "subscription")
        .meter
        .given(4 * DAY + 200);
    assert_eq!(r.recovers_at, Some(Ticks(WEEK)));
    // 周窗重置后恢复订阅。
    assert_eq!(
        demo::used(&route(&mut supplies, 10, WEEK)),
        Some("subscription")
    );
}

#[test]
fn defer_when_nothing_covers() {
    use af_meter::Supply;

    // 场景 A：订阅日窗已耗尽（明天恢复），API 余额只剩 10；需要 20 → 暂时没人覆盖，
    // 但订阅恢复后能覆盖 → Defer 且 recovers_at = 日窗重置时刻。
    let mut supplies = vec![
        Supply {
            name: "subscription".into(),
            priority: 0,
            meter: Meter::window(DAY, 200, 0),
        },
        Supply {
            name: "api".into(),
            priority: 1,
            meter: Meter::stock(10),
        },
    ];
    supplies[0].meter.try_consume(200, 0).unwrap();
    let d = route(&mut supplies, 20, 10);
    match d {
        Decision::Defer { recovers_at } => assert_eq!(recovers_at, Some(Ticks(DAY))),
        other => panic!("应 Defer，实际 {other:?}"),
    }

    // 场景 B：只有不恢复的 Stock 且不够 → Defer 且 recovers_at = None。
    let mut only_stock = vec![Supply {
        name: "api".into(),
        priority: 0,
        meter: Meter::stock(10),
    }];
    let d = route(&mut only_stock, 20, 0);
    match d {
        Decision::Defer { recovers_at } => assert_eq!(recovers_at, None),
        other => panic!("应 Defer，实际 {other:?}"),
    }
}
