//! MES 写入路径基准：官方 clickhouse crate（HTTP+RowBinary+lz4）vs 裸 HTTP INSERT（TSV 文本）
//! 用法: writer <crate|http> <batch_size> [total_rows=1_000_000]
//! 输出: mode, batch, total, inserts, elapsed_s, rows_per_s, avg_batch_ms, max_batch_ms (TSV 到 stdout)
use std::time::Instant;

use chrono::{DateTime, TimeZone, Utc};

#[derive(clickhouse::Row, serde::Serialize)]
struct Event {
    seq: u64,
    job_id: u64,
    line: String,
    station: String,
    event_type: String,
    #[serde(with = "clickhouse::serde::chrono::datetime")]
    ts: DateTime<Utc>, // 列类型 DateTime（秒）
    qty_good: u32,
    qty_defect: u32,
    defect_code: String,
    cycle_ms: u32,
}

struct XorShift(u64);
impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// 与 clickhouse-local 生成器同构的确定性数据（种子固定 → 两种路径喂同一批数据）
fn gen_rows(n: usize) -> Vec<Event> {
    let base = Utc.with_ymd_and_hms(2026, 9, 6, 22, 0, 0).unwrap(); // 2026-09-07 06:00 +08
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let mut v = Vec::with_capacity(n);
    for i in 0..n {
        let p = i % 10;
        let (event_type, station) = match p {
            0 | 3 | 5 | 7 => ("start", if p <= 1 { "ST01" } else if p <= 4 { "ST02" } else if p <= 6 { "ST03" } else { "ST04" }),
            1 | 4 | 6 | 8 => ("complete", if p <= 1 { "ST01" } else if p <= 4 { "ST02" } else if p <= 6 { "ST03" } else { "ST04" }),
            _ => ("defect", if p <= 1 { "ST01" } else if p <= 4 { "ST02" } else if p <= 6 { "ST03" } else { "ST04" }),
        };
        let r1 = rng.next(); let r2 = rng.next(); let r3 = rng.next(); let r4 = rng.next(); let r5 = rng.next();
        let complete = event_type == "complete";
        let defect = event_type == "defect";
        let station_ms = match station { "ST01" => 28000u32, "ST02" => 35000, "ST03" => 40000, _ => 30000 };
        v.push(Event {
            seq: i as u64,
            job_id: (i / 10) as u64,
            line: format!("L{}", (i / 10) % 5 + 1),
            station: station.to_string(),
            event_type: event_type.to_string(),
            ts: base + chrono::Duration::seconds((i as i64) + (r5 % 2) as i64),
            qty_good: if complete { 38 + (r1 % 23) as u32 } else { 0 },
            qty_defect: if defect { 1 + (r2 % 4) as u32 } else { 0 },
            defect_code: if defect { format!("D{:02}", 1 + (r3 % 8)) } else { String::new() },
            cycle_ms: if complete { station_ms + (r4 % 9000) as u32 } else { 0 },
        });
    }
    v
}

fn to_tsv(rows: &[Event]) -> Vec<u8> {
    let mut b = Vec::with_capacity(rows.len() * 120);
    use std::io::Write;
    for r in rows {
        let _ = write!(
            b, "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            r.seq, r.job_id, r.line, r.station, r.event_type, r.ts.timestamp(),
            r.qty_good, r.qty_defect, r.defect_code, r.cycle_ms
        );
    }
    b
}

async fn insert_crate(client: &clickhouse::Client, rows: &[Event]) -> Result<(), Box<dyn std::error::Error>> {
    let mut ins = client.insert::<Event>("bench_events").await?;
    for r in rows {
        ins.write(r).await?;
    }
    ins.end().await?;
    Ok(())
}

async fn insert_http(http: &reqwest::Client, url: &str, rows: &[Event]) -> Result<(), Box<dyn std::error::Error>> {
    let body = to_tsv(rows);
    // 端口 5221 = server http_port（任务指定 5220-5229 区间）
    let resp = http
        .post(format!("{url}/?query=INSERT%20INTO%20mes.bench_events%20FORMAT%20TSV"))
        .header("Content-Type", "text/tab-separated-values; charset=UTF-8")
        .body(body)
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        let t = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP INSERT failed: {status}: {t}").into());
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("crate").to_string();
    let batch: usize = args.get(2).map(|s| s.parse().expect("batch")).unwrap_or(10_000);
    let total: usize = args.get(3).map(|s| s.parse().expect("total")).unwrap_or(1_000_000);
    let url = std::env::var("CH_URL").unwrap_or_else(|_| "http://127.0.0.1:5221".into());
    assert!(mode == "crate" || mode == "http", "mode must be crate|http");
    assert!(batch > 0 && total >= batch, "bad batch/total");

    let client = clickhouse::Client::default().with_url(&url).with_database("mes");
    let http = reqwest::Client::builder().build().unwrap();

    // 清空基准表
    client.query("TRUNCATE TABLE mes.bench_events").execute().await.expect("truncate");

    // 预热 1 万行（不计入），建连+压缩表预热
    let warm = gen_rows(10_000);
    for c in warm.chunks(batch.min(10_000)) {
        let r = if mode == "crate" { insert_crate(&client, c).await } else { insert_http(&http, &url, c).await };
        r.expect("warmup insert");
    }
    client.query("TRUNCATE TABLE mes.bench_events").execute().await.expect("truncate after warmup");

    let rows = gen_rows(total);
    let mut n_inserts = 0usize;
    let mut max_batch_ms = 0.0f64;
    let mut sum_batch_ms = 0.0f64;
    let t0 = Instant::now();
    for chunk in rows.chunks(batch) {
        let bt = Instant::now();
        let r = if mode == "crate" { insert_crate(&client, chunk).await } else { insert_http(&http, &url, chunk).await };
        r.expect("bench insert");
        let ms = bt.elapsed().as_secs_f64() * 1000.0;
        max_batch_ms = max_batch_ms.max(ms);
        sum_batch_ms += ms;
        n_inserts += 1;
    }
    let el = t0.elapsed().as_secs_f64();
    println!(
        "mode={}\tbatch={}\ttotal={}\tinserts={}\telapsed_s={:.3}\trows_per_s={:.0}\tavg_batch_ms={:.2}\tmax_batch_ms={:.1}",
        mode, batch, total, n_inserts, el, total as f64 / el, sum_batch_ms / n_inserts as f64, max_batch_ms
    );
}
