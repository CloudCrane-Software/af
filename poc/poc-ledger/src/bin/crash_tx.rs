//! 跨库一致性边界实测：SQLite 单文件事务的原子性边界在哪？
//!
//! 方法：子进程打开 A.db 并 ATTACH B.db，单个事务里向两库各插 rows 行，
//! 在 BEGIN→(写两库)→COMMIT 的"COMMIT 即将开始"点向 stdout 打标记并立即被父进程 SIGKILL。
//! 父进程随后重开两库（触发热日志/WAL 恢复）统计两库行数，观测提交落点：
//!   (0,0)=回滚成功、(rows,rows)=提交完成、(rows,0)/(0,rows)=**跨库分裂**。
//!
//! 预期（SQLite 文档语义）：rollback-journal（DELETE）模式带 master journal 的多库提交是
//! 原子的（崩溃后全回滚）；WAL 模式多库提交**不跨库协调**（逐库落 commit frame）→ 可观测分裂。
//!
//! 用法：crash_tx <wal|delete> <dir> <iterations> <rows>

use std::io::BufRead;
use std::process::{Command, Stdio};
use std::time::Duration;

use rusqlite::Connection;

fn count(conn: &Connection, db: &str) -> rusqlite::Result<i64> {
    conn.query_row(&format!("SELECT COUNT(*) FROM {db}.t"), [], |r| r.get(0))
}

fn child_main(db_a: &str, db_b: &str, mode: &str, rows: usize) -> rusqlite::Result<()> {
    let conn = Connection::open(db_a)?;
    // journal_mode 是每文件属性：主库与 ATTACH 库都必须显式设成同一模式，
    // 否则 b 会留在默认 DELETE——混合模式下跨库原子性结论无效。
    let jm = if mode == "wal" { "WAL" } else { "DELETE" };
    conn.pragma_update(None, "journal_mode", jm)?;
    conn.execute_batch(&format!(
        "ATTACH DATABASE '{db_b}' AS b;
         CREATE TABLE t(n INTEGER PRIMARY KEY, v TEXT);
         CREATE TABLE b.t(n INTEGER PRIMARY KEY, v TEXT);"
    ))?;
    conn.pragma_update(Some(rusqlite::DatabaseName::Attached("b")), "journal_mode", jm)?;
    // FULL：让 COMMIT 带真实 fsync（WAL+NORMAL 的提交只有内存写，窗口太窄打不中）
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.execute_batch("BEGIN IMMEDIATE")?;
    {
        let mut st = conn.prepare("INSERT INTO t(n,v) VALUES(?1, ?2)")?;
        let mut sb = conn.prepare("INSERT INTO b.t(n,v) VALUES(?1, ?2)")?;
        for i in 0..rows {
            let s = format!("payload-{i}");
            st.execute(rusqlite::params![i as i64, s])?;
            sb.execute(rusqlite::params![i as i64, s])?;
        }
    }
    use std::io::Write;
    println!("COMMITTING");
    std::io::stdout().flush().unwrap();
    conn.execute_batch("COMMIT")?;
    println!("DONE");
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 6 && args[1] == "child" {
        child_main(&args[2], &args[3], &args[4], args[5].parse().unwrap()).expect("child failed");
        return;
    }
    if args.len() != 5 {
        eprintln!("usage: crash_tx <wal|delete> <dir> <iterations> <rows>  (或 child ...)");
        std::process::exit(2);
    }
    let mode = args[1].as_str();
    let dir = args[2].as_str();
    let iterations: usize = args[3].parse().unwrap();
    let rows: usize = args[4].parse().unwrap();
    std::fs::create_dir_all(dir).unwrap();
    let exe = std::env::current_exe().unwrap();

    let mut atomic = 0usize;
    let mut split = 0usize;
    let mut before_commit = 0usize;
    println!("mode={mode} iterations={iterations} rows_per_db={rows}");
    println!("iter  a_count      b_count      判定");
    for i in 0..iterations {
        let a = format!("{dir}/a_{mode}_{i}.db");
        let b = format!("{dir}/b_{mode}_{i}.db");
        let mut ch = Command::new(&exe)
            .args(["child", &a, &b, mode, &rows.to_string()])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        // 等到"COMMIT 即将开始"标记再杀
        let stdout = ch.stdout.take().unwrap();
        let mut seen_committing = false;
        for line in std::io::BufReader::new(stdout).lines() {
            match line.as_deref() {
                Ok("COMMITTING") => {
                    seen_committing = true;
                    break;
                }
                Ok("DONE") => break,
                _ => {}
            }
        }
        if seen_committing {
            // SIGKILL：模拟进程崩溃（commit 可能已开始甚至已完成——这正是要观测的落点分布）
            std::thread::sleep(Duration::from_micros(1000)); // 让 kill 落进提交窗口内（fsync 中段）
            libc_like_kill9(ch.id());
        }
        ch.wait().unwrap();

        // 重开（触发恢复）；ATTACH 两库一起开，模拟"服务重启"
        let conn = Connection::open(&a).unwrap();
        conn.execute_batch(&format!("ATTACH DATABASE '{b}' AS b")).unwrap();
        let ca = count(&conn, "main").unwrap();
        let cb = count(&conn, "b").unwrap();
        let verdict = if ca == 0 && cb == 0 {
            before_commit += 1;
            "回滚(0,0)"
        } else if ca == rows as i64 && cb == rows as i64 {
            atomic += 1;
            "全提交(N,N)"
        } else {
            split += 1;
            "分裂(!)"
        };
        println!("{}  {:>10} {:>10}  {}", i, ca, cb, verdict);
    }
    println!(
        "SUMMARY mode={mode}: 全提交={atomic} 回滚={before_commit} 分裂={split}"
    );
    if mode == "delete" && split > 0 {
        eprintln!("INVARIANT 违反：rollback-journal 多库提交不应分裂");
        std::process::exit(1);
    }
}

// 避免只为一个 kill 拉 libc crate：直接 syscall via std？std 无 kill。
// 用 /bin/kill？需要外部命令。这里内联最小 extern "C" 声明（libc 符号在std动态链接的libc里）。
mod libc_like {
    extern "C" {
        pub fn kill(pid: i32, sig: i32) -> i32;
    }
}
fn libc_like_kill9(pid: u32) {
    unsafe {
        libc_like::kill(pid as i32, 9);
    }
}
