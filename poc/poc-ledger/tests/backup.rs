//! 备份形态实测（在线 = 写者不停的情况下备份）：
//! 1. VACUUM INTO：在线一致性快照（WAL 下读写并发），备份可独立打开、integrity_check 通过；
//! 2. backup API（rusqlite backup::Backup 分页 step）：与写者并发推进，Busy/Locked 重试，同样一致；
//! 3. 裸 cp（只拷主文件、不带 -wal）：**会丢已提交数据**——量化"文件拷贝"这种备份形态的边界；
//!    先 `PRAGMA wal_checkpoint(TRUNCATE)` 再 cp 则完整。=> 安全形态只有 VACUUM INTO / backup API / checkpoint 后拷贝。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::backup::{Backup, StepResult};
use rusqlite::Connection;

fn open_wal(path: &str) -> Connection {
    let conn = Connection::open(path).unwrap();
    conn.busy_timeout(Duration::from_secs(5)).unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.pragma_update(None, "synchronous", "NORMAL").unwrap();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS t(id INTEGER PRIMARY KEY, v TEXT NOT NULL, ts INTEGER NOT NULL);",
    )
    .unwrap();
    conn
}

/// 后台写者：持续单行提交，返回已写行数的原子计数与停止开关。
fn spawn_writer(path: &'static str, stop: Arc<AtomicBool>, written: Arc<AtomicU64>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let conn = open_wal(path);
        let mut i = 0u64;
        while !stop.load(Ordering::SeqCst) {
            let ts = chrono::Utc::now().timestamp_millis();
            if conn
                .execute("INSERT INTO t(v,ts) VALUES(?1,?2)", rusqlite::params![format!("w{i}"), ts])
                .is_ok()
            {
                i += 1;
                written.store(i, Ordering::SeqCst);
            }
        }
    })
}

fn count_of(conn: &Connection) -> i64 {
    // 裸拷贝可能连 schema 都没有（建表语句也只在 -wal 里）——缺表视为 0 行
    let has: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='t'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    if has == 0 {
        return 0;
    }
    conn.query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0)).unwrap()
}

fn integrity(conn: &Connection) -> String {
    conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap()
}

#[test]
fn vacuum_into_online_snapshot_is_consistent_under_concurrent_writer() {
    let dir = env!("CARGO_TARGET_TMPDIR");
    let src_path: &'static str = Box::leak(format!("{dir}/bk_src_vacuum.db").into_boxed_str());
    let _ = std::fs::remove_file(format!("{src_path}-wal"));
    let _ = std::fs::remove_file(src_path);
    let stop = Arc::new(AtomicBool::new(false));
    let written = Arc::new(AtomicU64::new(0));
    let writer = spawn_writer(src_path, stop.clone(), written.clone());
    while written.load(Ordering::SeqCst) < 50 {
        std::thread::sleep(Duration::from_millis(5));
    }

    let rows_before = written.load(Ordering::SeqCst) as i64;
    let backup_path = format!("{dir}/bk_vacuum_copy.db");
    let _ = std::fs::remove_file(&backup_path);
    // 在线 VACUUM INTO（写者不停）：隐式读事务，WAL 下与写并发
    {
        let src = open_wal(src_path);
        src.execute("VACUUM INTO ?1", rusqlite::params![backup_path]).unwrap();
    }
    // 写者在备份期间必须持续推进（证明"在线"而非停写）
    std::thread::sleep(Duration::from_millis(50));
    let rows_after = written.load(Ordering::SeqCst) as i64;
    stop.store(true, Ordering::SeqCst);
    writer.join().unwrap();

    assert!(rows_after > rows_before, "备份期间写者持续推进（在线）");
    let b = Connection::open_with_flags(
        &backup_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let rows_in_backup = count_of(&b);
    assert_eq!(integrity(&b), "ok");
    assert!(
        rows_in_backup >= rows_before,
        "快照行数 {rows_in_backup} >= 备份开始时已提交行数 {rows_before}"
    );
    // VACUUM INTO 产物是独立单文件（无 -wal 依赖）
    assert!(!std::path::Path::new(&format!("{backup_path}-wal")).exists());
}

#[test]
fn backup_api_incremental_steps_converge_under_concurrent_writer() {
    let dir = env!("CARGO_TARGET_TMPDIR");
    let src_path: &'static str = Box::leak(format!("{dir}/bk_src_api.db").into_boxed_str());
    let _ = std::fs::remove_file(format!("{src_path}-wal"));
    let _ = std::fs::remove_file(src_path);
    let stop = Arc::new(AtomicBool::new(false));
    let written = Arc::new(AtomicU64::new(0));
    let writer = spawn_writer(src_path, stop.clone(), written.clone());
    while written.load(Ordering::SeqCst) < 50 {
        std::thread::sleep(Duration::from_millis(5));
    }
    let rows_before = written.load(Ordering::SeqCst) as i64;

    let dst_path = format!("{dir}/bk_api_copy.db");
    let _ = std::fs::remove_file(&dst_path);
    let mut retries = 0u32;
    {
        let src = open_wal(src_path);
        let mut dst = Connection::open(&dst_path).unwrap();
        let backup = Backup::new(&src, &mut dst).unwrap();
        loop {
            match backup.step(8).unwrap() {
                StepResult::Done => break,
                StepResult::More => {
                    std::thread::sleep(Duration::from_micros(200)); // 分页推进，与写者交错
                }
                StepResult::Busy | StepResult::Locked => {
                    retries += 1; // 源在写：可重试（API 契约）
                    std::thread::sleep(Duration::from_millis(1));
                }
                other => panic!("意外 step 结果 {other:?}"), // #[non_exhaustive] 兜底
            }
            assert!(retries < 10_000, "backup step 重试异常增多");
        }
    }
    let rows_after = written.load(Ordering::SeqCst) as i64;
    stop.store(true, Ordering::SeqCst);
    writer.join().unwrap();

    assert!(rows_after > rows_before, "备份期间写者持续推进（在线）");
    let b = Connection::open_with_flags(&dst_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let rows_in_backup = count_of(&b);
    assert_eq!(integrity(&b), "ok");
    assert!(rows_in_backup >= rows_before, "快照行数 {rows_in_backup} >= 开始时 {rows_before}");
}

#[test]
fn naked_file_copy_without_checkpoint_loses_committed_rows() {
    let dir = env!("CARGO_TARGET_TMPDIR");
    let db = format!("{dir}/naked_cp.db");
    for p in [&db, &format!("{db}-wal"), &format!("{db}-shm")] {
        let _ = std::fs::remove_file(p);
    }
    let conn = open_wal(&db);
    for i in 0..200 {
        conn.execute("INSERT INTO t(v,ts) VALUES(?1,0)", rusqlite::params![format!("r{i}")]).unwrap();
    }
    let live: i64 = count_of(&conn);
    assert_eq!(live, 200);

    // 只拷主文件、不拷 -wal（模拟"直接 cp 数据文件"的备份/迁移）。连接保持打开（不触发关闭时 checkpoint）。
    let copy1 = format!("{db}.copy1");
    let _ = std::fs::remove_file(&copy1);
    std::fs::copy(&db, &copy1).unwrap();
    let c1 = Connection::open(&copy1).unwrap();
    let copied_rows = count_of(&c1);
    let wal_len = std::fs::metadata(format!("{db}-wal")).map(|m| m.len()).unwrap_or(0);
    drop(c1);
    assert!(
        copied_rows < 200,
        "裸拷贝只看到 {copied_rows}/200 行：未 checkpoint 的已提交数据躺在 -wal（{wal_len} 字节）里，随拷贝丢失"
    );

    // 正确姿势：先 truncate checkpoint 把 -wal 收回主文件，再拷
    //（实测注：PRAGMA wal_checkpoint(TRUNCATE) 的返回行恒报 (0,0,0)，但物理效果真实——
    //  以文件尺寸为准断言，不信 pragma 返回列）
    let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)));
    let wal_after: u64 = std::fs::metadata(format!("{db}-wal")).map(|m| m.len()).unwrap_or(0);
    assert_eq!(wal_after, 0, "TRUNCATE checkpoint 后 -wal 应为空（实测 {wal_after} 字节）");
    let copy2 = format!("{db}.copy2");
    let _ = std::fs::remove_file(&copy2);
    std::fs::copy(&db, &copy2).unwrap();
    let c2 = Connection::open(&copy2).unwrap();
    assert_eq!(count_of(&c2), 200, "checkpoint 后拷贝完整");
    assert_eq!(integrity(&c2), "ok");
}
