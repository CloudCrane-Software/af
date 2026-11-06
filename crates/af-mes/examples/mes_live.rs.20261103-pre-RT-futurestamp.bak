//! MES 真机集成冒烟：往本机 ClickHouse（127.0.0.1:5221）apply schema + 写两批 + FINAL 读回。
//! 手动跑：cargo run -p af-mes --example mes_live
use std::sync::Arc;
use std::time::Duration;

use af_mes::{MesEvent, MesWriter, SystemClock, WriterConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = WriterConfig {
        endpoint: "http://127.0.0.1:5221".into(),
        max_rows: 3,
        max_delay: Duration::from_secs(1),
        ..WriterConfig::new("http://127.0.0.1:5221", "/tmp/mes-live-outbox")
    };
    let mut w = MesWriter::open(cfg, Arc::new(SystemClock::new()))?;
    w.apply_schema()?;
    println!("schema applied ok");
    for i in 0..7u64 {
        w.push(MesEvent::new(
            "wo-live-smoke",
            1_800_000_000_000 + i as i64 * 1000,
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
