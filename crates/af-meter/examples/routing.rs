//! 路由消费示例：订阅（Min(周窗, 日窗)）+ 按量 API（Stock），
//! 按任务预计消耗选供给。演示核心场景：
//! **日窗耗尽但周窗有余 → 日窗恢复前任务转 API；日窗重置后切回订阅。**
//!
//! 运行：cargo run -j 4 -p af-meter --example routing

use af_meter::demo::{self, SCHEDULE};
use af_meter::{route, Time};

fn fmt_t(t: Time) -> String {
    let day = (t / 86400.0).floor() as i64;
    let rem = t - day as f64 * 86400.0;
    let h = (rem / 3600.0).floor() as i64;
    let m = ((rem - h as f64 * 3600.0) / 60.0).floor() as i64;
    let s = (rem - h as f64 * 3600.0 - m as f64 * 60.0).round() as i64;
    format!("d{day} {h:02}:{m:02}:{s:02}")
}

fn main() {
    let mut supplies = demo::build_supplies();

    println!("== 供给配置（组合表达式 JSON）==");
    for s in &supplies {
        println!(
            "  {:<13} priority={} {}",
            s.name,
            s.priority,
            serde_json::to_string(&s.meter).unwrap()
        );
    }

    println!();
    println!("== 任务路由时间线 ==");
    println!(
        "{:<14} {:>10}  {:<13} {}",
        "时刻", "预计消耗", "选中供给", "扣后各供给剩余"
    );
    for &(t, need) in SCHEDULE {
        let decision = route(&mut supplies, need, t);
        let chosen = demo::used(&decision).unwrap_or("<defer>");
        let parts: Vec<String> = supplies
            .iter_mut()
            .map(|s| {
                let r = s.meter.given(t);
                let leaves = demo::breakdown(&s.meter)
                    .into_iter()
                    .map(|(name, _)| name)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}: {:.0} [{}]", s.name, r.available, leaves)
            })
            .collect();
        println!(
            "{:<14} {:>10.0}  {:<13} {}",
            fmt_t(t),
            need,
            chosen,
            parts.join(" | ")
        );
    }
}
