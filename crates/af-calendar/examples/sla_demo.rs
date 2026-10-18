//! 端到端演示:运营者 JSON 配置加载日历 → SLA 截止 → 截单派生 → 跨时区消费。
//! 运行:cargo run -p af-calendar --example sla_demo

use af_calendar::Calendar;
use chrono::{Duration, TimeZone, Utc};
use chrono_tz::Asia::Shanghai;

/// 运营者配置:白班 8×5(08:00–17:00 Asia/Shanghai)+ 国庆黄金周假日 + 周六调休。
/// (假日表为演示数据,非官方放假安排。)
const FACTORY_CONFIG: &str = r#"
{
  "tz": "Asia/Shanghai",
  "weekly": [
    [{"start": "08:00:00", "end": "17:00:00"}],
    [{"start": "08:00:00", "end": "17:00:00"}],
    [{"start": "08:00:00", "end": "17:00:00"}],
    [{"start": "08:00:00", "end": "17:00:00"}],
    [{"start": "08:00:00", "end": "17:00:00"}],
    [],
    []
  ],
  "holidays": [
    "2026-10-01", "2026-10-02", "2026-10-03", "2026-10-04",
    "2026-10-05", "2026-10-06", "2026-10-07"
  ],
  "day_overrides": {
    "2026-09-26": [{"start": "08:00:00", "end": "17:00:00"}]
  },
  "horizon_days": 3650
}
"#;

fn sh(y: i32, m: u32, d: u32, h: u32, min: u32) -> chrono::DateTime<chrono_tz::Tz> {
    Shanghai.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
}

fn main() {
    // 1. 加载运营者配置并校验
    let cal: Calendar = serde_json::from_str(FACTORY_CONFIG).expect("日历配置解析失败");
    cal.validate().expect("日历配置不合法");
    println!("== 日历配置加载并校验通过:{} 白班 8×5,国庆周(夹具数据)停工,09-26 调休上班 ==", cal.tz);

    // 2. SLA 正向:周五 16:00 下单,承诺 4 工时 → 下周一截止
    let order = sh(2026, 10, 23, 16, 0);
    let deadline = cal.sla_deadline(order, Duration::hours(4)).unwrap();
    println!("\n[SLA 正向] 周五 16:00 下单,承诺 4 工时(16-17 报 1h,周末跳过,周一 08-11 报 3h)");
    println!("  下单  : {}", order);
    println!("  截止  : {}  ← 下周一 11:00", deadline);
    println!("  UTC 视图: {}", deadline.with_timezone(&Utc));
    assert_eq!(deadline, sh(2026, 10, 26, 11, 0));

    // 3. 假日跨越:国庆前一天(周三)16:00 下单 4 工时 → 黄金周后首个工作日 11:00
    let pre_holiday = sh(2026, 9, 30, 16, 0);
    let d2 = cal.sla_deadline(pre_holiday, Duration::hours(4)).unwrap();
    println!("\n[假日跨越] 国庆前周三 16:00 下单 4 工时(周三报 1h,黄金周停工)");
    println!("  截止  : {}  ← 黄金周后首个工作日 11:00", d2);
    assert_eq!(d2, sh(2026, 10, 8, 11, 0));

    // 4. 截单派生(反向):交付承诺周一 11:00、需 4 工时 → 最晚周五 16:00 下单
    let due = sh(2026, 10, 26, 11, 0);
    let latest = cal.latest_order_at(due, Duration::hours(4)).unwrap();
    println!("\n[截单派生] 交付承诺 {} 需 4 工时", due.format("%m-%d %H:%M %a"));
    println!("  最晚下单: {}  ← 正反互逆(与 SLA 正向同一对时刻)", latest.format("%m-%d %H:%M %a"));

    // 5. 跨时区:同一日历,UTC 消费者拿 UTC(2026-10-23T08:00:00Z == 上海周五 16:00)
    let order_utc = Utc.with_ymd_and_hms(2026, 10, 23, 8, 0, 0).unwrap();
    let deadline_utc = cal.sla_deadline(order_utc, Duration::hours(4)).unwrap();
    println!("\n[跨时区] UTC 消费者下单 {}(=上海周五 16:00)", order_utc.to_rfc3339());
    println!("  截止  : {}(返回保持 UTC;上海读回 = {})", deadline_utc.to_rfc3339(), deadline_utc.with_timezone(&Shanghai));
    assert_eq!(deadline_utc, deadline.with_timezone(&Utc));

    println!("\n全部演示断言通过。");
}
