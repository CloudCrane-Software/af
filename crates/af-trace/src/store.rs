//! critical 轨的声明与缺口计数。
//!
//! 单文件 `trace.db`：WAL、`busy_timeout=5000`、`synchronous=NORMAL`、`foreign_keys=ON`。
//! 事件正文在 JSONL 里；这里只保存「这份目录是什么」以及与 critical 文件对齐的缺口计数。
//!
//! [`Store::open`] 首启返回 [`OpenOutcome::Created`]（建表并写入声明与零计数）。
//! 重入返回 [`OpenOutcome::AlreadyDeclared`]，并核对 schema 版本、行格式、critical 耐久性。
//! 对不上就失败，不改已有行、不按调用方声明静默迁就。

use crate::error::TraceError;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const SCHEMA_VERSION: i64 = 1;
pub const LINE_FORMAT: &str = "jsonl";
pub const CRITICAL_DURABILITY: &str = "sync";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS declaration (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema_version INTEGER NOT NULL,
    line_format TEXT NOT NULL,
    critical_durability TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS gap_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    gap_counter INTEGER NOT NULL,
    gap_reported INTEGER NOT NULL,
    FOREIGN KEY (id) REFERENCES declaration(id)
);
";

/// 打开落盘目录的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenOutcome {
    /// 目录里还没有声明。本次建表并写入初始缺口计数 0。
    Created,
    /// 声明已在，且与本进程认识的 schema / 格式 / 耐久性一致。
    AlreadyDeclared,
}

/// 已提交的缺口计数。两个数在干净关闭后都等于 critical 文件里 `gap.count` 之和。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GapState {
    pub gap_counter: u64,
    pub gap_reported: u64,
}

pub struct Store {
    conn: Connection,
}

pub fn store_path(dir: &Path) -> PathBuf {
    dir.join("trace.db")
}

impl Store {
    /// 打开（必要时创建）`dir/trace.db`。调用方负责先建好目录。
    pub fn open(dir: &Path) -> Result<(Self, OpenOutcome), TraceError> {
        let path = store_path(dir);
        let mut conn = Connection::open(&path).map_err(sql_err)?;
        apply_pragmas(&conn)?;
        let outcome = init_or_check(&mut conn)?;
        Ok((Store { conn }, outcome))
    }

    pub fn gap_state(&self) -> Result<GapState, TraceError> {
        let row: Option<(i64, i64)> = self
            .conn
            .query_row(
                "SELECT gap_counter, gap_reported FROM gap_state WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_err)?;
        let Some((counter, reported)) = row else {
            return Err(TraceError::Inconsistent("gap_state 缺少 id=1 的行".into()));
        };
        Ok(GapState {
            gap_counter: nonneg(counter, "gap_counter")?,
            gap_reported: nonneg(reported, "gap_reported")?,
        })
    }

    pub fn set_gap_state(&self, gap_counter: u64, gap_reported: u64) -> Result<(), TraceError> {
        let n = self
            .conn
            .execute(
                "UPDATE gap_state SET gap_counter = ?1, gap_reported = ?2 WHERE id = 1",
                params![as_i64(gap_counter)?, as_i64(gap_reported)?],
            )
            .map_err(sql_err)?;
        if n != 1 {
            return Err(TraceError::Inconsistent(
                "gap_state 更新没有命中 id=1".into(),
            ));
        }
        Ok(())
    }

    #[cfg(test)]
    fn connection(&self) -> &Connection {
        &self.conn
    }
}

fn apply_pragmas(conn: &Connection) -> Result<(), TraceError> {
    conn.busy_timeout(Duration::from_millis(5000))
        .map_err(sql_err)?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(sql_err)?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(sql_err)?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(sql_err)?;
    Ok(())
}

fn init_or_check(conn: &mut Connection) -> Result<OpenOutcome, TraceError> {
    let tx = conn.transaction().map_err(sql_err)?;
    tx.execute_batch(SCHEMA).map_err(sql_err)?;
    let row: Option<(i64, String, String)> = tx
        .query_row(
            "SELECT schema_version, line_format, critical_durability
             FROM declaration WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(sql_err)?;
    let outcome = match row {
        None => {
            tx.execute(
                "INSERT INTO declaration (id, schema_version, line_format, critical_durability)
                 VALUES (1, ?1, ?2, ?3)",
                params![SCHEMA_VERSION, LINE_FORMAT, CRITICAL_DURABILITY],
            )
            .map_err(sql_err)?;
            tx.execute(
                "INSERT INTO gap_state (id, gap_counter, gap_reported) VALUES (1, 0, 0)",
                [],
            )
            .map_err(sql_err)?;
            OpenOutcome::Created
        }
        Some((version, format, durability)) => {
            check_declaration(version, &format, &durability)?;
            OpenOutcome::AlreadyDeclared
        }
    };
    tx.commit().map_err(sql_err)?;
    Ok(outcome)
}

fn check_declaration(version: i64, format: &str, durability: &str) -> Result<(), TraceError> {
    if version != SCHEMA_VERSION {
        return Err(TraceError::DeclarationMismatch(format!(
            "schema_version={version}，本进程只接受 {SCHEMA_VERSION}"
        )));
    }
    if format != LINE_FORMAT {
        return Err(TraceError::DeclarationMismatch(format!(
            "line_format={format}，本进程只接受 {LINE_FORMAT}"
        )));
    }
    if durability != CRITICAL_DURABILITY {
        return Err(TraceError::DeclarationMismatch(format!(
            "critical_durability={durability}，本进程只接受 {CRITICAL_DURABILITY}"
        )));
    }
    Ok(())
}

fn sql_err(err: rusqlite::Error) -> TraceError {
    TraceError::Open(err.to_string())
}

fn as_i64(n: u64) -> Result<i64, TraceError> {
    i64::try_from(n)
        .map_err(|_| TraceError::Inconsistent(format!("计数 {n} 超出 sqlite 有符号整数范围")))
}

fn nonneg(n: i64, name: &str) -> Result<u64, TraceError> {
    u64::try_from(n).map_err(|_| TraceError::Inconsistent(format!("{name}={n} 为负")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("af-trace-store-{label}-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn pragma_i64(conn: &Connection, name: &str) -> i64 {
        conn.pragma_query_value(None, name, |row| row.get(0))
            .unwrap()
    }

    fn pragma_string(conn: &Connection, name: &str) -> String {
        conn.pragma_query_value(None, name, |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn pragmas_created_then_already_declared_keeps_gap() {
        let dir = TempDir::new("pragma");
        let (store, outcome) = Store::open(dir.path()).unwrap();
        assert_eq!(outcome, OpenOutcome::Created);
        assert_eq!(
            store.gap_state().unwrap(),
            GapState {
                gap_counter: 0,
                gap_reported: 0,
            }
        );
        assert_eq!(pragma_string(store.connection(), "journal_mode"), "wal");
        assert_eq!(pragma_i64(store.connection(), "synchronous"), 1);
        assert_eq!(pragma_i64(store.connection(), "foreign_keys"), 1);
        assert_eq!(pragma_i64(store.connection(), "busy_timeout"), 5000);
        let fk = store
            .connection()
            .execute("DELETE FROM declaration WHERE id = 1", []);
        assert!(
            fk.is_err(),
            "foreign_keys=ON 应拒绝删掉仍被 gap_state 引用的声明"
        );
        store.set_gap_state(4, 4).unwrap();
        drop(store);

        let (store, outcome) = Store::open(dir.path()).unwrap();
        assert_eq!(outcome, OpenOutcome::AlreadyDeclared);
        assert_eq!(
            store.gap_state().unwrap(),
            GapState {
                gap_counter: 4,
                gap_reported: 4,
            }
        );
    }

    #[test]
    fn declaration_mismatch_fails_closed_without_rewriting() {
        let dir = TempDir::new("decl");
        let (store, _) = Store::open(dir.path()).unwrap();
        store.set_gap_state(3, 3).unwrap();
        drop(store);

        for (sql, needle) in [
            (
                "UPDATE declaration SET schema_version = 99 WHERE id = 1",
                "schema_version",
            ),
            (
                "UPDATE declaration SET line_format = 'bin' WHERE id = 1",
                "line_format",
            ),
            (
                "UPDATE declaration SET critical_durability = 'async' WHERE id = 1",
                "critical_durability",
            ),
        ] {
            let conn = Connection::open(store_path(dir.path())).unwrap();
            conn.execute(sql, []).unwrap();
            drop(conn);
            let err = match Store::open(dir.path()) {
                Ok(_) => panic!("声明不符时必须拒绝打开"),
                Err(err) => err,
            };
            match err {
                TraceError::DeclarationMismatch(msg) => {
                    assert!(msg.contains(needle), "声明不符应点名字段，实际: {msg}")
                }
                other => panic!("期望 DeclarationMismatch，实际 {other:?}"),
            }
            let conn = Connection::open(store_path(dir.path())).unwrap();
            conn.execute(
                "UPDATE declaration
                 SET schema_version = ?1, line_format = ?2, critical_durability = ?3
                 WHERE id = 1",
                params![SCHEMA_VERSION, LINE_FORMAT, CRITICAL_DURABILITY],
            )
            .unwrap();
            drop(conn);
        }

        let (store, outcome) = Store::open(dir.path()).unwrap();
        assert_eq!(outcome, OpenOutcome::AlreadyDeclared);
        assert_eq!(store.gap_state().unwrap().gap_counter, 3);
    }
}
