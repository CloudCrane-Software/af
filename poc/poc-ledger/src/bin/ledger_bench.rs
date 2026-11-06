//! 裁决一实测：4 进程持续写 30 秒，单库 vs 三库。
//!
//! 拓扑（对应三个 rusqlite 使用者 af-meter/af-station/af-effect-gateway）：
//!   meter×2（meter 是逐调用热路径，天然双 worker）+ station×1 + gateway×1 = 4 进程。
//!   single 模式：4 进程全开同一个 ledger.db（各写各表，竞争仅来自文件级单写者）。
//!   split  模式：meter.db(2 进程) + station.db(1) + gateway.db(1)。
//!
//! 每进程：WAL + busy_timeout=5s + synchronous=NORMAL，autocommit 单行 INSERT，
//! 写满 30 秒为止；统计行数、SQLITE_BUSY 次数、其他错误、单条延迟 p50/p99/max。
//!
//! 用法：
//!   ledger_bench parent --mode single|split --dir <dir> --secs 30
//!   ledger_bench writer --db <path> --table <t> --label <name> --secs 30

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use rusqlite::ffi::ErrorCode;
use rusqlite::Connection;

struct WriterResult {
    label: String,
    db: String,
    rows: u64,
    busy: u64,
    other: u64,
    p50_us: u64,
    p99_us: u64,
    max_us: u64,
}

fn percentile(sorted: &[u32], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = (((sorted.len() as f64) * p).ceil() as usize).saturating_sub(1);
    sorted[idx.min(sorted.len() - 1)] as u64
}

fn run_writer(db: &str, table: &str, label: &str, secs: u64) -> anyhow_no::Result<WriterResult> {
    // 初始化重试：journal_mode=WAL 切换需要短暂排他锁，busy_timeout 对其不生效
    //（实测两 writer 同时建库会互报 SQLITE_BUSY），退避重试直到稳定。
    let conn = (|| -> anyhow_no::Result<Connection> {
        for attempt in 0..50 {
            let r = (|| -> anyhow_no::Result<Connection> {
                let conn = Connection::open(db)?;
                conn.busy_timeout(Duration::from_secs(5))?;
                conn.pragma_update(None, "journal_mode", "WAL")?;
                conn.pragma_update(None, "synchronous", "NORMAL")?;
                conn.execute_batch(&format!(
                    "CREATE TABLE IF NOT EXISTS {table}(
                       id INTEGER PRIMARY KEY AUTOINCREMENT,
                       w  TEXT NOT NULL,
                       n  INTEGER NOT NULL,
                       ts INTEGER NOT NULL);"
                ))?;
                Ok(conn)
            })();
            match r {
                Ok(c) => return Ok(c),
                Err(e) if attempt < 49 => {
                    eprintln!("[{label}] setup retry #{attempt}: {e}");
                    std::thread::sleep(Duration::from_millis(50 + attempt * 10));
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    })()?;
    let mut ins = conn.prepare(&format!("INSERT INTO {table}(w,n,ts) VALUES(?1,?2,?3)"))?;

    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut rows = 0u64;
    let mut busy = 0u64;
    let mut other = 0u64;
    let mut lat_us: Vec<u32> = Vec::new();
    let mut n: u64 = 0;
    while Instant::now() < deadline {
        let ts = chrono::Utc::now().timestamp_millis();
        let t0 = Instant::now();
        match ins.execute(rusqlite::params![label, n as i64, ts]) {
            Ok(_) => rows += 1,
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == ErrorCode::DatabaseBusy =>
            {
                busy += 1
            }
            Err(_) => other += 1,
        }
        lat_us.push(t0.elapsed().as_micros().min(u32::MAX as u128) as u32);
        n += 1;
    }
    lat_us.sort_unstable();
    Ok(WriterResult {
        label: label.to_string(),
        db: db.to_string(),
        rows,
        busy,
        other,
        p50_us: percentile(&lat_us, 0.50),
        p99_us: percentile(&lat_us, 0.99),
        max_us: lat_us.last().copied().unwrap_or(0) as u64,
    })
}

// 极简结果序列化（手拼 JSON，避免为 bench 拉 serde）
fn writer_json(r: &WriterResult) -> String {
    format!(
        "{{\"label\":\"{}\",\"db\":\"{}\",\"rows\":{},\"busy\":{},\"other\":{},\"p50_us\":{},\"p99_us\":{},\"max_us\":{}}}",
        r.label, r.db, r.rows, r.busy, r.other, r.p50_us, r.p99_us, r.max_us
    )
}

struct Topology {
    label: &'static str,
    db: &'static str,
    table: &'static str,
}

fn writers_for(mode: &str) -> Vec<Topology> {
    match mode {
        "single" => vec![
            Topology { label: "meter0", db: "ledger.db", table: "meter_event" },
            Topology { label: "meter1", db: "ledger.db", table: "meter_event" },
            Topology { label: "station0", db: "ledger.db", table: "station_task" },
            Topology { label: "gateway0", db: "ledger.db", table: "effect_intent" },
        ],
        "split" => vec![
            Topology { label: "meter0", db: "meter.db", table: "meter_event" },
            Topology { label: "meter1", db: "meter.db", table: "meter_event" },
            Topology { label: "station0", db: "station.db", table: "station_task" },
            Topology { label: "gateway0", db: "gateway.db", table: "effect_intent" },
        ],
        _ => panic!("mode 必须是 single|split"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let role = args.get(1).map(String::as_str).unwrap_or("");
    if role == "writer" {
        let get = |k: &str| {
            let i = args.iter().position(|a| a == k).expect(k) + 1;
            args[i].clone()
        };
        let r = run_writer(&get("--db"), &get("--table"), &get("--label"), get("--secs").parse().unwrap())
            .expect("writer failed");
        println!("{}", writer_json(&r));
        return;
    }
    if role != "parent" {
        eprintln!("usage: ledger_bench parent|writer ...");
        std::process::exit(2);
    }
    let get = |k: &str| {
        let i = args.iter().position(|a| a == k).expect(k) + 1;
        args[i].clone()
    };
    let mode = get("--mode");
    let dir = get("--dir");
    let secs: u64 = get("--secs").parse().unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let tops = writers_for(&mode);
    // 父进程先把每个库建成 WAL，避免子进程并发切 journal_mode 互报 BUSY
    {
        use std::collections::BTreeSet;
        let mut seen = BTreeSet::new();
        for t in &tops {
            if seen.insert(t.db.to_string()) {
                let p = format!("{}/{}", dir, t.db);
                let c = Connection::open(&p).unwrap();
                c.busy_timeout(Duration::from_secs(5)).unwrap();
                c.pragma_update(None, "journal_mode", "WAL").unwrap();
                drop(c);
            }
        }
    }

    let mut children = Vec::new();
    for t in &tops {
        let exe = std::env::current_exe().unwrap();
        let db = format!("{}/{}", dir, t.db);
        children.push((
            t.label,
            Command::new(exe)
                .args(["writer", "--db", &db, "--table", t.table, "--label", t.label, "--secs", &secs.to_string()])
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap_or_else(|e| panic!("spawn {label}: {e}", label = t.label)),
        ));
    }
    let mut results = Vec::new();
    for (label, ch) in children {
        let out = ch.wait_with_output().expect("wait");
        assert!(out.status.success(), "writer {label} 非零退出");
        let line = String::from_utf8_lossy(&out.stdout);
        results.push((label, line.trim().to_string()));
    }

    // 汇总
    let mut total_rows = 0u64;
    let mut total_busy = 0u64;
    let mut total_other = 0u64;
    println!("[{mode}] per-writer:");
    for (label, line) in &results {
        println!("  {label}: {line}");
        let num = |key: &str| -> u64 {
            line.split(',')
                .find(|p| p.contains(key))
                .and_then(|p| p.split(':').next_back())
                .and_then(|v| v.trim_end_matches('}').trim().parse().ok())
                .unwrap_or(0)
        };
        total_rows += num("\"rows\"");
        total_busy += num("\"busy\"");
        total_other += num("\"other\"");
    }
    println!(
        "[{mode}] TOTAL rows={} rows/s={:.0} busy={} other={} secs={}",
        total_rows,
        total_rows as f64 / secs as f64,
        total_busy,
        total_other,
        secs
    );
    // 每库文件与 WAL 尾巴大小（checkpoint 压力参考）
    let mut seen = std::collections::BTreeSet::new();
    for t in &tops {
        if seen.insert(t.db) {
            for suffix in ["", "-wal"] {
                let p = format!("{}/{}{}", dir, t.db, suffix);
                if let Ok(md) = std::fs::metadata(&p) {
                    println!("[{mode}] file {} = {} bytes", p, md.len());
                }
            }
        }
    }
}

// 避免为 bench 引入 anyhow：本地小错误别名
mod anyhow_no {
    pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
}
