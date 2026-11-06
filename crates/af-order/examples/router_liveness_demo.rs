//! router_liveness_demo —— 收件箱活性安灯读数腿的可运行示范（证据/接线用）：
//! 评估一次 [`af_order::router_liveness::evaluate_liveness`] 并把 PoC-D 同款
//! 快照落到指定路径。非产品 CLI 面（af-order CLI 是下单门面，安灯读数腿
//! 与消费腿 router 同为库面）；聚合接线批可用本文件作调用形态参考。
//!
//! 用法：`router_liveness_demo <triggers-home> <snapshot-out> [now-epoch-s]`
//! （now 缺省取系统时钟；显式值供对齐复算/测试复现）。

use std::path::Path;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(home), Some(out)) = (args.next(), args.next()) else {
        eprintln!("用法: router_liveness_demo <triggers-home> <snapshot-out> [now-epoch-s]");
        std::process::exit(2);
    };
    let now = args
        .next()
        .and_then(|text| text.parse::<f64>().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("系统时钟")
                .as_secs_f64()
        });
    match af_order::router_liveness::evaluate_liveness(Path::new(&home), now) {
        Ok(reading) => {
            af_order::router_liveness::write_snapshot(&reading, Path::new(&out)).unwrap_or_else(
                |err| {
                    eprintln!("af-order: 快照落盘失败: {err}");
                    std::process::exit(1);
                },
            );
            println!(
                "status={} silent_s={:.3} threshold_min={} since_epoch_s={:.3} \
                 anchor_source={} anchor_created={} alarm_code={}",
                reading.status.as_str(),
                reading.silent_s,
                reading.threshold_min,
                reading.since_epoch_s,
                reading.anchor_source.as_str(),
                reading.anchor_created,
                reading.alarm_code()
            );
        }
        Err(err) => {
            eprintln!("af-order: {err}");
            std::process::exit(1);
        }
    }
}
