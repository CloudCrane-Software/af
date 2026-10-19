//! 计量账本。一份声明对应一个 SQLite 文件。
//!
//! 窗口起止按钟面日期解析，不用「现在加上若干毫秒」。
//! 扣减、事件、outbox 补偿在同一个 `BEGIN IMMEDIATE` 事务里提交。
//! 崩溃后已提交的行靠 WAL 还在；未标记发出的 outbox 由 [`Ledger::recover`] 再送一次。
//! 恰好一次靠事件的幂等键：接收方用同一键去重。
//!
//! 账本读数是 [`crate::Reading<crate::UnixMillis>`]。`recovers_at` 是窗口终点的 Unix 毫秒。
//! 内存 [`crate::Meter`] 的读数是 [`crate::Reading<crate::Ticks>`]，单位是注入的整数秒。
//! 两个参数类型不能比较。

mod bounds;
mod consume;
mod recover;

pub use bounds::window_bounds;
pub use consume::ConsumeResult;
pub use recover::OutboxRecord;

use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use chrono_tz::Tz;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use crate::Count;

/// 声明过的 schema。重入时 `user_version` 必须等于它。
pub const SCHEMA_VERSION: i64 = 1;

const BUSY_TIMEOUT_MS: i64 = 5_000;

/// 首开窗口里对 `database is locked` / `already exists` 的重试次数。
const COLD_START_ATTEMPTS: u32 = 16;

const EXPECTED_TABLES: &[&str] = &["declaration", "events", "outbox", "windows"];

const DDL: &str = "
CREATE TABLE IF NOT EXISTS declaration (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    meter_id    TEXT NOT NULL CHECK (length(meter_id) > 0),
    capacity    INTEGER NOT NULL CHECK (capacity >= 0),
    timezone    TEXT NOT NULL CHECK (length(timezone) > 0),
    window_kind TEXT NOT NULL CHECK (window_kind IN ('day', 'week'))
);
CREATE TABLE IF NOT EXISTS windows (
    start_ms INTEGER PRIMARY KEY,
    end_ms   INTEGER NOT NULL,
    capacity INTEGER NOT NULL CHECK (capacity >= 0),
    consumed INTEGER NOT NULL CHECK (consumed >= 0 AND consumed <= capacity),
    CHECK (end_ms > start_ms)
);
CREATE TABLE IF NOT EXISTS events (
    idempotency_key TEXT NOT NULL UNIQUE,
    amount          INTEGER NOT NULL CHECK (amount > 0),
    window_start_ms INTEGER NOT NULL REFERENCES windows(start_ms),
    at_ms           INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS outbox (
    idempotency_key TEXT NOT NULL UNIQUE REFERENCES events(idempotency_key),
    payload         TEXT NOT NULL,
    created_at_ms   INTEGER NOT NULL,
    sent_at_ms      INTEGER
);
CREATE INDEX IF NOT EXISTS idx_outbox_unsent
    ON outbox (created_at_ms, idempotency_key)
    WHERE sent_at_ms IS NULL;
";

pub type Result<T> = std::result::Result<T, LedgerError>;

/// 账本拒绝原因。声明和库里的版本或参数对不上时启动失败，不改存量。
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    /// `user_version` 不是本 crate 声明的 [`SCHEMA_VERSION`]。
    #[error("schema user_version {found} does not match declared {expected}")]
    SchemaMismatch { found: i64, expected: i64 },
    /// 库已经有表，但 `user_version` 仍是 0。不把半截初始化修成可用库。
    #[error("user_version is 0 but the database already has tables")]
    TornInitialization,
    /// 表集合或声明行和本 crate 写下的结构不一致。
    #[error("stored schema does not match the declared ledger: {detail}")]
    SchemaDrift { detail: String },
    /// 重入时某个声明字段和库里的值不同。
    #[error("declaration {field} stored {stored:?} does not match declared {declared:?}")]
    DeclarationMismatch {
        field: &'static str,
        stored: String,
        declared: String,
    },
    /// 打开参数不成立：空 id、无法解析的时区，或容量放不进 SQLite 整数。
    #[error("invalid declaration: {0}")]
    BadDeclaration(&'static str),
    /// 连接配方没有生效。
    #[error("connection recipe {name} did not stick")]
    Recipe { name: &'static str },
    /// `now_ms` 在声明时区里没有对应的钟面时刻。
    #[error("instant {now_ms} does not map in the declared timezone")]
    UnmappedInstant { now_ms: i64 },
    /// 日历日加减越界，或窗口终点不晚于起点。
    #[error("civil date overflow while resolving a window")]
    CivilOverflow,
    /// 当地 00:00 整日都不存在。
    #[error("civil midnight of the window does not exist in the declared timezone")]
    UnmappedMidnight,
    /// 窗口终点是负的 Unix 毫秒，放不进 [`crate::UnixMillis`]。
    #[error("window end {ms} is negative")]
    NegativeInstant { ms: i64 },
    /// 库存整数不能当成计数。
    #[error("stored integer {value} is not a valid count")]
    CorruptCount { value: i64 },
    /// 幂等键为空。
    #[error("idempotency key must be non-empty")]
    EmptyIdempotencyKey,
    /// 消耗必须是正数，并且放得进 SQLite 的有符号整数。
    #[error("amount {amount} must be positive and fit in i64")]
    BadAmount { amount: Count },
    /// 同一幂等键已经记过另一笔数量。
    #[error("idempotency key {key} already recorded amount {stored}, not {incoming}")]
    IdempotencyConflict {
        key: String,
        stored: Count,
        incoming: Count,
    },
    /// 本窗口剩余不够。`recovers_at` 是窗口终点的 Unix 毫秒。
    #[error("insufficient: need={need}, available={available}, recovers_at={recovers_at}")]
    Insufficient {
        need: Count,
        available: Count,
        recovers_at: crate::UnixMillis,
    },
    /// 同一事务里窗口行在读和写之间变了。事务会回滚。
    #[error("window update lost its compare-and-swap")]
    LostUpdate,
    /// 外发回调失败。失败的那条仍留在 outbox，已标记的保持已标记。
    #[error("dispatch failed: {0}")]
    Dispatch(String),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// 窗口怎么对齐钟面。周窗从周一开始，到下一个周一的 00:00 结束，两端都是当地钟面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowKind {
    /// 当地日历日 `[当天 00:00, 次日 00:00)`。
    Day,
    /// 当地周一 `[本周一 00:00, 下周一 00:00)`。
    Week,
}

impl WindowKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            WindowKind::Day => "day",
            WindowKind::Week => "week",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "day" => Some(WindowKind::Day),
            "week" => Some(WindowKind::Week),
            _ => None,
        }
    }
}

/// 打开账本时声明的参数。重入必须逐项相同，否则启动失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// 这份库只服务一个计量。
    pub meter_id: String,
    /// 每个窗口的计数容量。
    pub capacity: Count,
    /// IANA 时区名，例如 `Europe/Berlin`。比较是精确字符串。
    pub timezone: String,
    pub kind: WindowKind,
}

/// 首启或重入。
#[derive(Debug)]
#[must_use]
pub enum Opened {
    /// 空库：建表、写入声明、把 `user_version` 设成 [`SCHEMA_VERSION`]。
    Created(Ledger),
    /// 已有库：版本和声明都与本次参数一致。
    AlreadyDeclared(Ledger),
}

impl Opened {
    pub fn into_ledger(self) -> Ledger {
        match self {
            Opened::Created(ledger) | Opened::AlreadyDeclared(ledger) => ledger,
        }
    }

    pub const fn is_created(&self) -> bool {
        matches!(self, Opened::Created(_))
    }
}

/// 本连接实际生效的落盘配方。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    pub journal_mode: String,
    pub synchronous: i64,
    pub foreign_keys: i64,
    pub busy_timeout_ms: i64,
    pub user_version: i64,
}

/// 一条已入账的消耗。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    pub idempotency_key: String,
    pub amount: Count,
    pub window_start_ms: i64,
    pub at_ms: i64,
}

/// `now_ms` 所在窗口的快照。没有行时 `persisted` 为 false，消耗按 0 计。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSnapshot {
    pub start_ms: i64,
    pub end_ms: i64,
    pub capacity: Count,
    pub consumed: Count,
    /// 库里已经有这一窗的行。
    pub persisted: bool,
}

/// 账目合计。守恒时 `event_amount == window_consumed` 且 `event_count == outbox_count`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Totals {
    pub event_count: u64,
    pub event_amount: Count,
    pub window_consumed: Count,
    pub outbox_count: u64,
    pub outbox_unsent: u64,
}

/// 一个计量的账本连接。多消费者各自 [`Ledger::open`] 同一文件。
pub struct Ledger {
    pub(super) conn: Connection,
    pub(super) decl: Declaration,
    pub(super) tz: Tz,
    pub(super) capacity_i64: i64,
}

impl fmt::Debug for Ledger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ledger")
            .field("meter_id", &self.decl.meter_id)
            .field("capacity", &self.decl.capacity)
            .field("timezone", &self.decl.timezone)
            .field("kind", &self.decl.kind)
            .finish()
    }
}

impl Ledger {
    /// 打开或创建账本文件。
    ///
    /// 每条连接都设置 WAL、`busy_timeout` 5000 ms、`synchronous=NORMAL`、`foreign_keys=ON`。
    /// 先读 `user_version`：已经是 [`SCHEMA_VERSION`] 就走 [`Opened::AlreadyDeclared`]，
    /// 不再做 delete→WAL 的排他转换。空库在一个 `BEGIN IMMEDIATE` 里建表并写下版本。
    /// `database is locked` 或 `already exists` 在首开窗口内按注入的退避重试，
    /// 用尽仍失败则原样返回，不改已有行。
    /// `user_version == 0` 且没有任何用户表 → [`Opened::Created`]。
    /// 其余情况返回错误，不改库里的行。
    pub fn open(path: impl AsRef<Path>, decl: &Declaration) -> Result<Opened> {
        let (tz, capacity_i64) = validate(decl)?;
        let path = path.as_ref();
        let mut attempt = 0u32;
        loop {
            match open_once(path, decl, tz, capacity_i64) {
                Ok(opened) => return Ok(opened),
                Err(err) if cold_start_conflict(&err) && attempt + 1 < COLD_START_ATTEMPTS => {
                    cold_start_pause(attempt);
                    attempt += 1;
                }
                Err(err) => return Err(err),
            }
        }
    }

    pub fn declaration(&self) -> &Declaration {
        &self.decl
    }

    /// 读本连接上的配方。`journal_mode` 与 `user_version` 落在文件头。
    /// `synchronous`、`foreign_keys`、`busy_timeout` 只对这条连接有效，每次打开都重新设置。
    pub fn recipe(&self) -> Result<Recipe> {
        read_recipe(&self.conn)
    }
}

pub(super) fn parse_tz(timezone: &str) -> Result<Tz> {
    if timezone.is_empty() {
        return Err(LedgerError::BadDeclaration("timezone"));
    }
    timezone
        .parse::<Tz>()
        .map_err(|_| LedgerError::BadDeclaration("timezone"))
}

pub(super) fn count_from_i64(value: i64) -> Result<Count> {
    u64::try_from(value).map_err(|_| LedgerError::CorruptCount { value })
}

pub(super) fn ms_to_time(ms: i64) -> Result<crate::UnixMillis> {
    u64::try_from(ms)
        .map(crate::UnixMillis)
        .map_err(|_| LedgerError::NegativeInstant { ms })
}

fn validate(decl: &Declaration) -> Result<(Tz, i64)> {
    if decl.meter_id.is_empty() {
        return Err(LedgerError::BadDeclaration("meter_id"));
    }
    let capacity_i64 =
        i64::try_from(decl.capacity).map_err(|_| LedgerError::BadDeclaration("capacity"))?;
    let tz = parse_tz(&decl.timezone)?;
    Ok((tz, capacity_i64))
}

fn open_once(path: &Path, decl: &Declaration, tz: Tz, capacity_i64: i64) -> Result<Opened> {
    let mut conn = Connection::open(path)?;
    connection_pragmas(&conn)?;
    // ① 先读版本。已声明的库不再碰 delete→WAL 转换。
    let version = user_version(&conn)?;
    if version == SCHEMA_VERSION {
        ensure_journal_mode(&conn)?;
        assert_recipe(&conn)?;
        return finish_declared(conn, decl, tz, capacity_i64);
    }
    if version != 0 {
        return Err(LedgerError::SchemaMismatch {
            found: version,
            expected: SCHEMA_VERSION,
        });
    }
    // delete→WAL 要排他锁，且连接上已有共享锁时不走 busy handler。
    // SQLite 禁止在事务里改 WAL，所以这一步紧挨在 BEGIN IMMEDIATE 之前。
    ensure_journal_mode(&conn)?;
    assert_recipe(&conn)?;
    // ② 建库：建表与 user_version 同一个 IMMEDIATE 事务。
    // foreign_keys/synchronous 是连接态 pragma，已在 connection_pragmas 设置；
    // SQLite 禁止在事务内改 synchronous——F4 修复的核心纪律。
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version = user_version(&tx)?;
    if version == SCHEMA_VERSION {
        tx.rollback()?;
        return finish_declared(conn, decl, tz, capacity_i64);
    }
    if version != 0 {
        return Err(LedgerError::SchemaMismatch {
            found: version,
            expected: SCHEMA_VERSION,
        });
    }
    let names = table_names(&tx)?;
    if !names.is_empty() {
        return Err(LedgerError::TornInitialization);
    }
    tx.execute_batch(DDL)?;
    tx.execute(
        "INSERT INTO declaration (id, meter_id, capacity, timezone, window_kind)
         VALUES (1, ?1, ?2, ?3, ?4)",
        params![
            decl.meter_id,
            capacity_i64,
            decl.timezone,
            decl.kind.as_str()
        ],
    )?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    let stored_version = user_version(&conn)?;
    if stored_version != SCHEMA_VERSION {
        return Err(LedgerError::SchemaMismatch {
            found: stored_version,
            expected: SCHEMA_VERSION,
        });
    }
    Ok(Opened::Created(Ledger {
        conn,
        decl: decl.clone(),
        tz,
        capacity_i64,
    }))
}

fn finish_declared(
    mut conn: Connection,
    decl: &Declaration,
    tz: Tz,
    capacity_i64: i64,
) -> Result<Opened> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version = user_version(&tx)?;
    if version != SCHEMA_VERSION {
        return Err(LedgerError::SchemaMismatch {
            found: version,
            expected: SCHEMA_VERSION,
        });
    }
    let names = table_names(&tx)?;
    exact_tables(&names)?;
    let stored = load_declaration(&tx)?;
    ensure_same(decl, &stored)?;
    tx.rollback()?;
    Ok(Opened::AlreadyDeclared(Ledger {
        conn,
        decl: decl.clone(),
        tz,
        capacity_i64,
    }))
}

fn connection_pragmas(conn: &Connection) -> Result<()> {
    conn.busy_timeout(Duration::from_millis(5_000))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

fn assert_recipe(conn: &Connection) -> Result<()> {
    let recipe = read_recipe(conn)?;
    if recipe.journal_mode != "wal" {
        return Err(LedgerError::Recipe {
            name: "journal_mode",
        });
    }
    if recipe.synchronous != 1 {
        return Err(LedgerError::Recipe {
            name: "synchronous",
        });
    }
    if recipe.foreign_keys != 1 {
        return Err(LedgerError::Recipe {
            name: "foreign_keys",
        });
    }
    if recipe.busy_timeout_ms != BUSY_TIMEOUT_MS {
        return Err(LedgerError::Recipe {
            name: "busy_timeout",
        });
    }
    Ok(())
}

/// 注入的首开退避。车道号让同时失败的线程错开醒来，不读业务时钟。
fn cold_start_pause(attempt: u32) {
    static LANES: AtomicU64 = AtomicU64::new(0);
    let lane = LANES.fetch_add(1, Ordering::Relaxed) % 8;
    let millis = u64::from(attempt.saturating_add(1)).saturating_add(lane);
    std::thread::sleep(Duration::from_millis(millis));
}

fn cold_start_conflict(err: &LedgerError) -> bool {
    let text = err.to_string().to_ascii_lowercase();
    text.contains("database is locked")
        || text.contains("database is busy")
        || text.contains("database table is locked")
        || text.contains("already exists")
}

fn ensure_journal_mode(conn: &Connection) -> Result<()> {
    let mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    if mode.eq_ignore_ascii_case("wal") {
        return Ok(());
    }
    let mode: String =
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
    if mode.eq_ignore_ascii_case("wal") {
        Ok(())
    } else {
        Err(busy_locked().into())
    }
}

fn busy_locked() -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
        Some("database is locked".into()),
    )
}

fn read_recipe(conn: &Connection) -> Result<Recipe> {
    let journal_mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    let synchronous: i64 = conn.pragma_query_value(None, "synchronous", |row| row.get(0))?;
    let foreign_keys: i64 = conn.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    let busy_timeout_ms: i64 = conn.pragma_query_value(None, "busy_timeout", |row| row.get(0))?;
    let version = user_version(conn)?;
    Ok(Recipe {
        journal_mode: journal_mode.to_ascii_lowercase(),
        synchronous,
        foreign_keys,
        busy_timeout_ms,
        user_version: version,
    })
}

fn user_version(conn: &Connection) -> Result<i64> {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(LedgerError::from)
}

fn table_names(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
         ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    let mut names = Vec::new();
    for name in rows {
        names.push(name?);
    }
    Ok(names)
}

fn exact_tables(names: &[String]) -> Result<()> {
    let same = names.len() == EXPECTED_TABLES.len()
        && names
            .iter()
            .zip(EXPECTED_TABLES.iter())
            .all(|(got, expected)| got == expected);
    if same {
        Ok(())
    } else {
        Err(LedgerError::SchemaDrift {
            detail: format!("tables {names:?}, declared {EXPECTED_TABLES:?}"),
        })
    }
}

fn load_declaration(conn: &Connection) -> Result<Declaration> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM declaration", [], |row| row.get(0))?;
    if n != 1 {
        return Err(LedgerError::SchemaDrift {
            detail: format!("declaration rows {n}"),
        });
    }
    let row = conn
        .query_row(
            "SELECT id, meter_id, capacity, timezone, window_kind
             FROM declaration WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?;
    let Some((id, meter_id, capacity, timezone, kind_text)) = row else {
        return Err(LedgerError::SchemaDrift {
            detail: "declaration row 1 is missing".to_string(),
        });
    };
    if id != 1 {
        return Err(LedgerError::SchemaDrift {
            detail: format!("declaration id {id}"),
        });
    }
    let kind = WindowKind::parse(&kind_text).ok_or_else(|| LedgerError::SchemaDrift {
        detail: format!("window_kind {kind_text}"),
    })?;
    if capacity < 0 {
        return Err(LedgerError::SchemaDrift {
            detail: format!("capacity {capacity}"),
        });
    }
    Ok(Declaration {
        meter_id,
        capacity: count_from_i64(capacity)?,
        timezone,
        kind,
    })
}

fn ensure_same(declared: &Declaration, stored: &Declaration) -> Result<()> {
    if declared.meter_id != stored.meter_id {
        return Err(LedgerError::DeclarationMismatch {
            field: "meter_id",
            stored: stored.meter_id.clone(),
            declared: declared.meter_id.clone(),
        });
    }
    if declared.capacity != stored.capacity {
        return Err(LedgerError::DeclarationMismatch {
            field: "capacity",
            stored: stored.capacity.to_string(),
            declared: declared.capacity.to_string(),
        });
    }
    if declared.timezone != stored.timezone {
        return Err(LedgerError::DeclarationMismatch {
            field: "timezone",
            stored: stored.timezone.clone(),
            declared: declared.timezone.clone(),
        });
    }
    if declared.kind != stored.kind {
        return Err(LedgerError::DeclarationMismatch {
            field: "window_kind",
            stored: stored.kind.as_str().to_string(),
            declared: declared.kind.as_str().to_string(),
        });
    }
    Ok(())
}
