//! 开库声明。
//!
//! 首启（库里没有本账本的对象）建表并写入 schema 版本与指纹，返回 [`OpenStatus::Created`]。
//! 重入时逐项对照：版本、指纹、`sqlite_master` 里的建表语句。任何一项对不上就失败，
//! 不补表、不改已有行、不把旧库解释成新库。
//!
//! 连接配方与既有账本一致，并打开外键：WAL、`busy_timeout` 5000ms、`synchronous=NORMAL`、
//! `foreign_keys=ON`。这些 PRAGMA 是连接态，每次 open 都重设。

use rusqlite::{params, Connection};
use std::time::Duration;

use crate::{Error, Result};

/// 本次 open 看到的是新库还是已声明且一致的库。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenStatus {
    /// 首启：建表并写入声明。
    Created,
    /// 重入：已存声明与代码里的声明一致。
    AlreadyDeclared,
}

pub(crate) const SCHEMA_VERSION: i64 = 1;
pub(crate) const SCHEMA_FINGERPRINT: &str = "af-station/schema-1";

const SQL_DECL: &str = "CREATE TABLE station_decl (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema_version INTEGER NOT NULL,
    schema_fingerprint TEXT NOT NULL
)";

const SQL_TASK: &str = "CREATE TABLE task (
    id              TEXT PRIMARY KEY,
    payload_ref     TEXT NOT NULL,
    required_role   TEXT NOT NULL,
    state           TEXT NOT NULL CHECK (state IN ('queued','claimed','done')),
    claimant        TEXT,
    lease_until     INTEGER,
    idempotency_key TEXT UNIQUE,
    deadline_ms     INTEGER
)";

const SQL_INDEX: &str = "CREATE INDEX idx_task_role_state ON task (required_role, state)";

const SQL_SHIFT: &str = "CREATE TABLE shift_binding (
    kind TEXT NOT NULL CHECK (kind IN ('role', 'worker')),
    subject TEXT NOT NULL CHECK (length(subject) > 0),
    calendar_json TEXT NOT NULL,
    PRIMARY KEY (kind, subject)
)";

/// `(type, name, sql)`，顺序即创建顺序。
const OBJECTS: &[(&str, &str, &str)] = &[
    ("table", "station_decl", SQL_DECL),
    ("table", "task", SQL_TASK),
    ("index", "idx_task_role_state", SQL_INDEX),
    ("table", "shift_binding", SQL_SHIFT),
];

pub(crate) fn prepare(conn: &mut Connection) -> Result<OpenStatus> {
    conn.busy_timeout(Duration::from_millis(5_000))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    if user_objects(conn)?.is_empty() {
        create_schema(conn)?;
        verify(conn)?;
        return Ok(OpenStatus::Created);
    }
    verify(conn)?;
    Ok(OpenStatus::AlreadyDeclared)
}

fn create_schema(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute(SQL_DECL, [])?;
    tx.execute(SQL_TASK, [])?;
    tx.execute(SQL_INDEX, [])?;
    tx.execute(SQL_SHIFT, [])?;
    tx.execute(
        "INSERT INTO station_decl (id, schema_version, schema_fingerprint) VALUES (1, ?1, ?2)",
        params![SCHEMA_VERSION, SCHEMA_FINGERPRINT],
    )?;
    tx.commit()?;
    Ok(())
}

fn user_objects(conn: &Connection) -> Result<Vec<(String, String, Option<String>)>> {
    let mut stmt = conn.prepare("SELECT type, name, sql FROM sqlite_master")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let row = row?;
        if row.1.starts_with("sqlite_") {
            continue;
        }
        out.push(row);
    }
    Ok(out)
}

fn verify(conn: &Connection) -> Result<()> {
    let rows = user_objects(conn)?;
    let mut seen = [false; OBJECTS.len()];
    for (ty, name, sql) in &rows {
        let Some(idx) = OBJECTS
            .iter()
            .position(|(ety, ename, _)| ety == ty && ename == name)
        else {
            return Err(Error::DeclarationMismatch(format!(
                "未声明的对象 {ty} {name}"
            )));
        };
        let got = sql.as_deref().unwrap_or("");
        if got != OBJECTS[idx].2 {
            return Err(Error::DeclarationMismatch(format!(
                "{name} 的 SQL 与声明不符\n库内: {got}\n声明: {}",
                OBJECTS[idx].2
            )));
        }
        if seen[idx] {
            return Err(Error::DeclarationMismatch(format!("重复对象 {name}")));
        }
        seen[idx] = true;
    }
    for (idx, was_seen) in seen.iter().enumerate() {
        if !was_seen {
            return Err(Error::DeclarationMismatch(format!(
                "缺少对象 {}",
                OBJECTS[idx].1
            )));
        }
    }
    verify_decl_row(conn)
}

fn verify_decl_row(conn: &Connection) -> Result<()> {
    let mut stmt =
        conn.prepare("SELECT schema_version, schema_fingerprint FROM station_decl WHERE id = 1")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
    let rows = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    if rows.len() != 1 {
        return Err(Error::DeclarationMismatch(format!(
            "station_decl 应恰好一行，实际 {}",
            rows.len()
        )));
    }
    let (version, fingerprint) = &rows[0];
    if *version != SCHEMA_VERSION || fingerprint != SCHEMA_FINGERPRINT {
        return Err(Error::DeclarationMismatch(format!(
            "schema_version={version} fingerprint={fingerprint}，声明要求 version={SCHEMA_VERSION} fingerprint={SCHEMA_FINGERPRINT}"
        )));
    }
    Ok(())
}
