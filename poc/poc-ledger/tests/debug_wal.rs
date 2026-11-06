// 临时调试：naked cp 测试里 WAL 状态逐步追踪
use std::time::Duration;
use rusqlite::Connection;

fn open_wal(path: &str) -> Connection {
    let conn = Connection::open(path).unwrap();
    conn.busy_timeout(Duration::from_secs(5)).unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.pragma_update(None, "synchronous", "NORMAL").unwrap();
    conn.execute_batch("CREATE TABLE IF NOT EXISTS t(id INTEGER PRIMARY KEY, v TEXT NOT NULL, ts INTEGER NOT NULL);").unwrap();
    conn
}

fn walinfo(db: &str, tag: &str) {
    let main = std::fs::metadata(db).map(|m| m.len()).unwrap_or(0);
    let wal = std::fs::metadata(format!("{db}-wal")).map(|m| m.len()).unwrap_or(0);
    println!("[{tag}] main={main} wal={wal}");
}

#[test]
fn debug_wal_lifecycle() {
    let dir = env!("CARGO_TARGET_TMPDIR");
    let db = format!("{dir}/debug_wal.db");
    for p in [&db, &format!("{db}-wal"), &format!("{db}-shm")] {
        let _ = std::fs::remove_file(p);
    }
    let conn = open_wal(&db);
    walinfo(&db, "after open");
    for i in 0..200 {
        conn.execute("INSERT INTO t(v,ts) VALUES(?1,0)", rusqlite::params![format!("r{i}")]).unwrap();
    }
    walinfo(&db, "after 200 inserts");
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0)).unwrap();
    walinfo(&db, "after count read");
    println!("live count = {n}");
    let r: (i64, i64, i64) = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?))).unwrap();
    println!("checkpoint = {r:?}");
    walinfo(&db, "after checkpoint");
    assert!(true);
}

#[test]
fn debug_checkpoint_return_values() {
    let dir = env!("CARGO_TARGET_TMPDIR");
    let db = format!("{dir}/debug_wal2.db");
    for p in [&db, &format!("{db}-wal")] { let _ = std::fs::remove_file(p); }
    let conn = open_wal(&db);
    for i in 0..200 {
        conn.execute("INSERT INTO t(v,ts) VALUES(?1,0)", rusqlite::params![format!("r{i}")]).unwrap();
    }
    let mut st = conn.prepare("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
    let mut rows = st.query([]).unwrap();
    while let Some(row) = rows.next().unwrap() {
        let v0: rusqlite::types::Value = row.get(0).unwrap();
        let v1: rusqlite::types::Value = row.get(1).unwrap();
        let v2: rusqlite::types::Value = row.get(2).unwrap();
        println!("raw row: {v0:?} {v1:?} {v2:?}");
    }
    let wal = std::fs::metadata(format!("{db}-wal")).map(|m| m.len()).unwrap_or(0);
    println!("wal after = {wal}");
}
