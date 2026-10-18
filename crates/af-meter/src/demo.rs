//! 路由消费示例的场景与工具（examples/routing.rs 与 tests/routing.rs 共用）。
//!
//! 两路供给：
//! - `subscription`（优先级 0）= Min(周窗 1000, 日窗 200) —— 周窗套日窗的订阅配额；
//! - `api`（优先级 1）= Stock 10000 —— 按量付费，用完不恢复。
//!
//! 路由规则：按任务预计消耗，优先级序找第一路能覆盖的供给。

use crate::{Amount, Decision, Meter, Supply, Time};

pub const DAY: Time = 86400.0;
pub const WEEK: Time = 604800.0;
pub const DAILY_CAP: Amount = 200.0;
pub const WEEKLY_CAP: Amount = 1000.0;
pub const API_STOCK: Amount = 10000.0;

/// 任务时间线：(时刻, 任务预计消耗)。
///
/// 剧情：第 0 天上午两单把日窗（200）用满 → 日窗耗尽但周窗仍有 800，
/// 日窗恢复前的新任务全部转 API；次日 00:00 日窗重置后切回订阅；
/// 预计消耗超过日窗剩余/上限的任务（300、250）即使日窗有量也直接走 API。
pub const SCHEDULE: &[(Time, Amount)] = &[
    (0.0, 120.0),        // d0 00:00 订阅：日 200->80，周 1000->880
    (3_600.0, 80.0),     // d0 01:00 订阅：日 80->0，周 880->800（日窗耗尽，周窗有余 800）
    (7_200.0, 5.0),      // d0 02:00 日窗 0 -> 转 API
    (80_000.0, 7.0),     // d0 22:13 仍未恢复 -> API
    (90_000.0, 150.0),   // d1 01:00 日窗已重置 -> 切回订阅：日 200->50，周 800->650
    (95_000.0, 300.0),   // d1 02:23 预计 300 > 日窗上限 200 -> API
    (500_000.0, 250.0),  // d5 18:53 min(周 650, 日 200)=200 < 250 -> API
    (604_800.0, 150.0),  // d7 00:00 周窗重置 -> 订阅
];

/// 构建两路供给。
pub fn build_supplies() -> Vec<Supply> {
    vec![
        Supply {
            name: "subscription".into(),
            priority: 0,
            meter: Meter::min(vec![
                Meter::window(WEEK, WEEKLY_CAP, 0.0),
                Meter::window(DAY, DAILY_CAP, 0.0),
            ]),
        },
        Supply {
            name: "api".into(),
            priority: 1,
            meter: Meter::stock(API_STOCK),
        },
    ]
}

pub fn find<'a>(supplies: &'a [Supply], name: &str) -> &'a Supply {
    supplies
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("supply {name} 不存在"))
}

pub fn find_mut<'a>(supplies: &'a mut [Supply], name: &str) -> &'a mut Supply {
    supplies
        .iter_mut()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("supply {name} 不存在"))
}

/// Decision 里被选中的供给名。
pub fn used(d: &Decision) -> Option<&str> {
    match d {
        Decision::Use { supply, .. } => Some(supply),
        Decision::Defer { .. } => None,
    }
}

/// 把计量树展平成 (叶子描述, 剩余量)，供演示打印（状态应已被 given 推进过）。
pub fn breakdown(meter: &Meter) -> Vec<(String, Amount)> {
    match meter {
        Meter::Stock { remaining } => vec![(format!("stock={remaining}"), *remaining)],
        Meter::Window {
            remaining,
            capacity,
            ..
        } => vec![(format!("window({remaining}/{capacity})"), *remaining)],
        Meter::Concurrency {
            limit, in_use, ..
        } => vec![(
            format!("concurrency({in_use}/{limit})"),
            (*limit - *in_use) as Amount,
        )],
        Meter::Rate {
            tokens,
            burst,
            ..
        } => vec![(format!("rate({tokens}/{burst})"), *tokens)],
        Meter::Min(kids) | Meter::Max(kids) => {
            let label = if matches!(meter, Meter::Min(_)) { "min" } else { "max" };
            let mut out = Vec::new();
            for k in kids {
                for (n, a) in breakdown(k) {
                    out.push((format!("{label}[{n}]"), a));
                }
            }
            out
        }
    }
}
