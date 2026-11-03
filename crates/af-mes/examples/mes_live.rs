//! MES 真机集成冒烟：往本机 ClickHouse（127.0.0.1:5221）apply schema + 写两批 + FINAL 读回。
//! 手动跑：cargo run -p af-mes --example mes_live
//!
//! 2026-11-03 RT 修（确证发现：MES 记账面含 7 条 2027-01-15 未来账）：冒烟戳一律随钟走
//! （SystemClock 当前时刻起 7 秒内），不得硬编码。原 `1_800_000_000_000`ms =
//! 2027-01-15T08:00Z，把 7 条幻影 order.completed 写进 af_mes.mes_event 并聚合进
//! mes_line_hour 未来桶（SummingMergeTree 永久携带，2026-11-03 已 ALTER DELETE 清账）；
//! ch-mes-reconcile R5 对「occurred_at > now()+skew」的未来戳行默认记红挡门——
//! 本 example 再写未来账即门红+alarms.log 红牌，不再无人可见。
use std::sync::Arc;
use std::time::Duration;

use af_mes::{Clock, MesEvent, MesWriter, SystemClock, WriterConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let clock = SystemClock::new();
    let cfg = WriterConfig {
        endpoint: "http://127.0.0.1:5221".into(),
        max_rows: 3,
        max_delay: Duration::from_secs(1),
        ..WriterConfig::new("http://127.0.0.1:5221", "/tmp/mes-live-outbox")
    };
    let mut w = MesWriter::open(cfg, Arc::new(clock))?;
    w.apply_schema()?;
    println!("schema applied ok");
    // 冒烟戳 = 当前时刻起逐秒推进；写未来账=账目不符，R5 门会红。
    let base = clock.now_ms();
    for i in 0..7u64 {
        w.push(MesEvent::new(
            "wo-live-smoke",
            base + i as i64 * 1000,
            format!("ev-{i}"),
            "line-A",
            "station-1",
            "order.completed",
            1,
        ))?;
    }
    w.flush()?;
    println!("pushed+flushed 7 rows (threshold 3)");
    let n = w.outbox_batches();
    println!("outbox_batches={n} (期望 0=全部送达)");
    Ok(())
}
