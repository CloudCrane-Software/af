//! 投放闸的 SQLite 账本。
//!
//! 落盘的是 CONWIP 在制队列（件数、权重、准入分钟）和渐放游标
//! （配置、封顶、released / completed / defects、是否已晋升），以及会改变
//! 下一次准入结果的夜间信用、窗口观察和断粮边沿。
//!
//! [`DurableGate::open`] / [`DurableGate::open_with_clock`]：
//! - 空库 → [`OpenOutcome::Created`]，建表并写入声明；
//! - 已有声明且 schema 版本、WIP 上下限、夜间窗口都一致 → [`OpenOutcome::AlreadyDeclared`]，
//!   原样装回在制与游标；
//! - 版本或关键参数对不上 → 直接失败，不改库里的声明。
//!
//! 谓词是函数，不落库。重入时仅当库里有渐放行才传入同一个谓词。
//! 准入用的分钟仍由调用方每次传入。[`crate::Clock::wall_ms`] 只写 `updated_wall_ms`。

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use thiserror::Error;

use crate::clock::{Clock, SystemClock};
use crate::gate::{
    sum_weights, AdmitError, CanaryCursor, CanaryPredicate, CanaryStatus, CompletedItem,
    ConfigError, GateConfig, GateError, GateEvent, InProcessItem, Item, MinuteOfDay, NightView,
    NightWindow, ReleaseGate, RestoreError, RuntimeSnapshot, Ticket,
};

/// 账本 schema。对不上就拒绝启动，不在这里迁移。
pub const SCHEMA_VERSION: i64 = 1;

const BUSY_TIMEOUT_MS: u64 = 5_000;

/// 首开窗口里对 `database is locked` / `already exists` 的重试次数。
const COLD_START_ATTEMPTS: u32 = 16;

const SCHEMA_SQL: &str = "
CREATE TABLE gate_meta (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    schema_version      INTEGER NOT NULL,
    wip_upper_bits      INTEGER NOT NULL,
    wip_lower_bits      INTEGER NOT NULL,
    night_start         INTEGER,
    night_end           INTEGER,
    night_ratio_bits    INTEGER,
    wip_bits            INTEGER NOT NULL,
    in_process_count    INTEGER NOT NULL,
    refill_active       INTEGER NOT NULL CHECK (refill_active IN (0, 1)),
    night_credit_bits   INTEGER NOT NULL,
    night_requests      INTEGER NOT NULL,
    night_admitted      INTEGER NOT NULL,
    prev_in_window      INTEGER CHECK (prev_in_window IS NULL OR prev_in_window IN (0, 1)),
    updated_wall_ms     INTEGER NOT NULL
);
CREATE TABLE wip_item (
    seq           INTEGER PRIMARY KEY,
    gate_id       INTEGER NOT NULL REFERENCES gate_meta(id),
    item_id       INTEGER NOT NULL,
    config        TEXT NOT NULL,
    weight_bits   INTEGER NOT NULL,
    admitted_at   INTEGER NOT NULL
);
CREATE TABLE canary (
    id              INTEGER PRIMARY KEY CHECK (id = 1),
    gate_id         INTEGER NOT NULL REFERENCES gate_meta(id),
    config          TEXT NOT NULL,
    max_probes      INTEGER NOT NULL,
    released        INTEGER NOT NULL,
    completed       INTEGER NOT NULL,
    defects         INTEGER NOT NULL,
    open            INTEGER NOT NULL CHECK (open IN (0, 1)),
    updated_wall_ms INTEGER NOT NULL
);
";

/// 打开账本的结果。调用方据此区分首启和重入，两种路径都带回可用的闸门。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenOutcome {
    /// 空库：表已建好，声明已写入，在制为空。
    Created,
    /// 声明与库内记录一致，在制和渐放游标已装回。
    AlreadyDeclared,
}

/// 本连接上的 SQLite 配方，供测试核对。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageProfile {
    pub journal_mode: String,
    /// `synchronous=NORMAL` 时为 1。
    pub synchronous: i64,
    pub foreign_keys: bool,
    pub busy_timeout_ms: i64,
}

#[derive(Debug, Error)]
pub enum PersistError {
    #[error(transparent)]
    Admit(#[from] AdmitError),
    #[error(transparent)]
    Gate(#[from] GateError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("schema version {stored} != {expected}")]
    SchemaMismatch { stored: i64, expected: i64 },
    #[error("declaration mismatch on {field}")]
    DeclarationMismatch { field: &'static str },
    #[error("persisted canary has no predicate to resume")]
    CanaryPredicateRequired,
    #[error("predicate supplied but no canary is persisted")]
    CanaryPredicateUnexpected,
    #[error("persisted state is corrupt: {detail}")]
    Corrupt { detail: String },
}

struct Meta {
    schema_version: i64,
    wip_upper_bits: i64,
    wip_lower_bits: i64,
    night_start: Option<i64>,
    night_end: Option<i64>,
    night_ratio_bits: Option<i64>,
    wip_bits: i64,
    in_process_count: i64,
    refill_active: i64,
    night_credit_bits: i64,
    night_requests: i64,
    night_admitted: i64,
    prev_in_window: Option<i64>,
    updated_wall_ms: i64,
}

/// 带账本的投放闸。一个实例持有一条连接。
pub struct DurableGate {
    conn: Connection,
    gate: ReleaseGate,
    clock: Arc<dyn Clock>,
    persisted: RuntimeSnapshot,
    updated_wall_ms: i64,
}

impl std::fmt::Debug for DurableGate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DurableGate")
            .field("wip", &self.wip_minutes())
            .field("in_process", &self.in_process_count())
            .field("updated_wall_ms", &self.updated_wall_ms)
            .finish()
    }
}

impl DurableGate {
    /// 打开账本，墙钟用 [`SystemClock`]。
    pub fn open(
        path: impl AsRef<Path>,
        declaration: &GateConfig,
        resume_predicate: Option<CanaryPredicate>,
    ) -> Result<(Self, OpenOutcome), PersistError> {
        Self::open_with_clock(path, declaration, resume_predicate, SystemClock::new())
    }

    /// 同 [`Self::open`]，绝对时刻戳走 `clock`。
    ///
    /// 先读 `user_version`。已经是 [`SCHEMA_VERSION`] 就不再做 delete→WAL。
    /// 空库在一个 `BEGIN IMMEDIATE` 里建表、写入声明并打下版本。
    /// `database is locked` 或 `already exists` 在首开窗口内按注入的退避重试，
    /// 用尽仍失败则原样返回，不改已有行。非法声明在创建文件之前失败。
    pub fn open_with_clock(
        path: impl AsRef<Path>,
        declaration: &GateConfig,
        resume_predicate: Option<CanaryPredicate>,
        clock: impl Clock + 'static,
    ) -> Result<(Self, OpenOutcome), PersistError> {
        let clock: Arc<dyn Clock> = Arc::new(clock);
        let mut resume_predicate = resume_predicate;
        let mut attempt = 0u32;
        loop {
            // 非法声明在创建文件之前失败。每次重试重新构造闸门，谓词只在装回成功时取走。
            let gate = ReleaseGate::new(declaration.clone())?;
            let opened = match Connection::open(path.as_ref()) {
                Ok(conn) => open_prepared(
                    conn,
                    declaration,
                    gate,
                    &mut resume_predicate,
                    Arc::clone(&clock),
                ),
                Err(err) => Err(err.into()),
            };
            match opened {
                Ok(pair) => return Ok(pair),
                Err(err) if cold_start_conflict(&err) && attempt + 1 < COLD_START_ATTEMPTS => {
                    cold_start_pause(attempt);
                    attempt += 1;
                }
                Err(err) => return Err(err),
            }
        }
    }

    pub fn config(&self) -> &GateConfig {
        self.gate.config()
    }

    pub fn wip_minutes(&self) -> f64 {
        self.gate.wip_minutes()
    }

    pub fn in_process_count(&self) -> usize {
        self.gate.in_process_count()
    }

    pub fn in_process(&self) -> Vec<InProcessItem> {
        self.gate.in_process()
    }

    pub fn refill_needed(&self) -> bool {
        self.gate.refill_needed()
    }

    pub fn in_night_window(&self, now: MinuteOfDay) -> bool {
        self.gate.in_night_window(now)
    }

    pub fn canary_status(&self) -> Option<CanaryStatus> {
        self.gate.canary_status()
    }

    pub fn canary_cursor(&self) -> Option<CanaryCursor> {
        self.gate.canary_cursor()
    }

    pub fn night_view(&self) -> NightView {
        self.gate.night_view()
    }

    pub fn night_stats(&self) -> (u64, u64) {
        self.gate.night_stats()
    }

    pub fn drain_events(&mut self) -> Vec<GateEvent> {
        self.gate.drain_events()
    }

    /// 最近一次成功落库的墙钟毫秒。重入打开本身不改这个值。
    pub fn updated_wall_ms(&self) -> i64 {
        self.updated_wall_ms
    }

    pub fn storage_profile(&self) -> Result<StorageProfile, PersistError> {
        let journal_mode: String = self
            .conn
            .pragma_query_value(None, "journal_mode", |row| row.get(0))?;
        let synchronous: i64 = self
            .conn
            .pragma_query_value(None, "synchronous", |row| row.get(0))?;
        let foreign_keys: i64 = self
            .conn
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
        let busy_timeout_ms: i64 = self
            .conn
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))?;
        Ok(StorageProfile {
            journal_mode,
            synchronous,
            foreign_keys: foreign_keys == 1,
            busy_timeout_ms,
        })
    }

    pub fn begin_canary(
        &mut self,
        config: &str,
        max_probes: usize,
        predicate: CanaryPredicate,
    ) -> Result<(), PersistError> {
        self.gate.begin_canary(config, max_probes, predicate)?;
        self.persist_if_changed()?;
        Ok(())
    }

    pub fn try_admit(&mut self, item: &Item, now: MinuteOfDay) -> Result<Ticket, PersistError> {
        let result = self.gate.try_admit(item, now);
        self.persist_if_changed()?;
        result.map_err(PersistError::from)
    }

    pub fn complete(
        &mut self,
        item_id: u64,
        now: MinuteOfDay,
        defects: u64,
    ) -> Result<CompletedItem, PersistError> {
        let result = self.gate.complete(item_id, now, defects);
        self.persist_if_changed()?;
        result.map_err(PersistError::from)
    }

    fn persist_if_changed(&mut self) -> Result<(), PersistError> {
        let snap = self.gate.runtime_snapshot();
        if snap == self.persisted {
            return Ok(());
        }
        let wall_ms = self.clock.wall_ms();
        self.write_snapshot(&snap, wall_ms)?;
        self.persisted = snap;
        self.updated_wall_ms = wall_ms;
        Ok(())
    }

    fn write_snapshot(&mut self, snap: &RuntimeSnapshot, wall_ms: i64) -> Result<(), PersistError> {
        let count = usize_to_sql(snap.items.len(), "in_process_count")?;
        let requests = u64_count_to_sql(snap.night_requests, "night_requests")?;
        let admitted = u64_count_to_sql(snap.night_admitted, "night_admitted")?;
        let tx = self.conn.transaction()?;
        let updated = tx.execute(
            "UPDATE gate_meta SET
                wip_bits = ?1,
                in_process_count = ?2,
                refill_active = ?3,
                night_credit_bits = ?4,
                night_requests = ?5,
                night_admitted = ?6,
                prev_in_window = ?7,
                updated_wall_ms = ?8
             WHERE id = 1",
            params![
                f64_to_sql(snap.wip),
                count,
                i64::from(snap.refill_active),
                f64_to_sql(snap.night_credit),
                requests,
                admitted,
                snap.prev_in_window.map(i64::from),
                wall_ms,
            ],
        )?;
        if updated != 1 {
            return Err(PersistError::Corrupt {
                detail: "gate_meta missing".to_string(),
            });
        }
        tx.execute("DELETE FROM wip_item", [])?;
        tx.execute("DELETE FROM canary", [])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO wip_item (seq, gate_id, item_id, config, weight_bits, admitted_at)
                 VALUES (?1, 1, ?2, ?3, ?4, ?5)",
            )?;
            for (seq, item) in snap.items.iter().enumerate() {
                let seq = usize_to_sql(seq, "seq")?;
                stmt.execute(params![
                    seq,
                    u64_to_sql(item.id),
                    item.config,
                    f64_to_sql(item.weight),
                    i64::from(item.admitted_at),
                ])?;
            }
        }
        if let Some(canary) = &snap.canary {
            let max_probes = usize_to_sql(canary.max_probes, "max_probes")?;
            let released = usize_to_sql(canary.released, "released")?;
            let completed = usize_to_sql(canary.completed, "completed")?;
            tx.execute(
                "INSERT INTO canary (
                    id, gate_id, config, max_probes, released, completed, defects, open, updated_wall_ms
                 ) VALUES (1, 1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    canary.config,
                    max_probes,
                    released,
                    completed,
                    u64_to_sql(canary.defects),
                    i64::from(canary.open),
                    wall_ms,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

fn open_prepared(
    mut conn: Connection,
    declaration: &GateConfig,
    gate: ReleaseGate,
    resume_predicate: &mut Option<CanaryPredicate>,
    clock: Arc<dyn Clock>,
) -> Result<(DurableGate, OpenOutcome), PersistError> {
    connection_pragmas(&conn)?;
    // ① 先读版本。已声明的库不再碰 delete→WAL 转换。
    let version = read_user_version(&conn)?;
    if version == SCHEMA_VERSION {
        ensure_journal_mode(&conn)?;
        return open_existing(conn, declaration, gate, resume_predicate, clock);
    }
    if version != 0 {
        return Err(PersistError::SchemaMismatch {
            stored: version,
            expected: SCHEMA_VERSION,
        });
    }
    // delete→WAL 要排他锁。SQLite 禁止在事务里改 WAL，所以这一步紧挨在 BEGIN 之前。
    ensure_journal_mode(&conn)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version = read_user_version(&tx)?;
    if version == SCHEMA_VERSION {
        tx.rollback()?;
        return open_existing(conn, declaration, gate, resume_predicate, clock);
    }
    if version != 0 {
        return Err(PersistError::SchemaMismatch {
            stored: version,
            expected: SCHEMA_VERSION,
        });
    }
    if table_exists(&tx, "gate_meta")? {
        tx.rollback()?;
        return open_existing(conn, declaration, gate, resume_predicate, clock);
    }
    if any_user_table(&tx)? {
        return Err(PersistError::Corrupt {
            detail: "database has tables but no gate_meta".to_string(),
        });
    }
    if resume_predicate.is_some() {
        return Err(PersistError::CanaryPredicateUnexpected);
    }
    let wall_ms = clock.wall_ms();
    let snap = gate.runtime_snapshot();
    tx.execute_batch(SCHEMA_SQL)?;
    insert_initial(&tx, declaration, &snap, wall_ms)?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok((
        DurableGate {
            conn,
            gate,
            clock,
            persisted: snap,
            updated_wall_ms: wall_ms,
        },
        OpenOutcome::Created,
    ))
}

fn open_existing(
    conn: Connection,
    declaration: &GateConfig,
    mut gate: ReleaseGate,
    resume_predicate: &mut Option<CanaryPredicate>,
    clock: Arc<dyn Clock>,
) -> Result<(DurableGate, OpenOutcome), PersistError> {
    if !table_exists(&conn, "wip_item")? || !table_exists(&conn, "canary")? {
        return Err(PersistError::Corrupt {
            detail: "gate schema is incomplete".to_string(),
        });
    }
    let user_version = read_user_version(&conn)?;
    if user_version != SCHEMA_VERSION {
        return Err(PersistError::SchemaMismatch {
            stored: user_version,
            expected: SCHEMA_VERSION,
        });
    }
    let meta = read_meta(&conn)?;
    if meta.schema_version != SCHEMA_VERSION {
        return Err(PersistError::SchemaMismatch {
            stored: meta.schema_version,
            expected: SCHEMA_VERSION,
        });
    }
    ensure_declaration(&meta, declaration)?;
    let loaded = load_runtime(&conn, &meta)?;
    match (loaded.canary.is_some(), resume_predicate.is_some()) {
        (true, false) => return Err(PersistError::CanaryPredicateRequired),
        (false, true) => return Err(PersistError::CanaryPredicateUnexpected),
        _ => {}
    }
    let predicate = resume_predicate.take();
    gate.restore_runtime(loaded.clone(), predicate)
        .map_err(map_restore)?;
    if gate.runtime_snapshot() != loaded {
        return Err(PersistError::Corrupt {
            detail: "restore diverged from the row".to_string(),
        });
    }
    let updated_wall_ms = meta.updated_wall_ms;
    Ok((
        DurableGate {
            conn,
            gate,
            clock,
            persisted: loaded,
            updated_wall_ms,
        },
        OpenOutcome::AlreadyDeclared,
    ))
}

fn map_restore(err: RestoreError) -> PersistError {
    let detail = match err {
        RestoreError::PredicateMismatch => "canary predicate presence",
        RestoreError::InconsistentWip => "wip does not match items",
    };
    PersistError::Corrupt {
        detail: detail.to_string(),
    }
}

fn connection_pragmas(conn: &Connection) -> Result<(), PersistError> {
    conn.busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS))?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

/// 注入的首开退避。车道号让同时失败的线程错开醒来，不读业务时钟。
fn cold_start_pause(attempt: u32) {
    static LANES: AtomicU64 = AtomicU64::new(0);
    let lane = LANES.fetch_add(1, Ordering::Relaxed) % 8;
    let millis = u64::from(attempt.saturating_add(1)).saturating_add(lane);
    std::thread::sleep(Duration::from_millis(millis));
}

fn cold_start_conflict(err: &PersistError) -> bool {
    let text = err.to_string().to_ascii_lowercase();
    text.contains("database is locked")
        || text.contains("database is busy")
        || text.contains("database table is locked")
        || text.contains("already exists")
}

fn ensure_journal_mode(conn: &Connection) -> Result<(), PersistError> {
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
        Some("database is locked".to_string()),
    )
}

fn insert_initial(
    tx: &Connection,
    declaration: &GateConfig,
    snap: &RuntimeSnapshot,
    wall_ms: i64,
) -> Result<(), PersistError> {
    let (night_start, night_end, night_ratio) = night_columns(declaration);
    let count = usize_to_sql(snap.items.len(), "in_process_count")?;
    let requests = u64_count_to_sql(snap.night_requests, "night_requests")?;
    let admitted = u64_count_to_sql(snap.night_admitted, "night_admitted")?;
    tx.execute(
        "INSERT INTO gate_meta (
            id, schema_version, wip_upper_bits, wip_lower_bits,
            night_start, night_end, night_ratio_bits,
            wip_bits, in_process_count, refill_active,
            night_credit_bits, night_requests, night_admitted,
            prev_in_window, updated_wall_ms
         ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            SCHEMA_VERSION,
            f64_to_sql(declaration.wip_upper_minutes),
            f64_to_sql(declaration.wip_lower_minutes),
            night_start,
            night_end,
            night_ratio,
            f64_to_sql(snap.wip),
            count,
            i64::from(snap.refill_active),
            f64_to_sql(snap.night_credit),
            requests,
            admitted,
            snap.prev_in_window.map(i64::from),
            wall_ms,
        ],
    )?;
    Ok(())
}

fn night_columns(declaration: &GateConfig) -> (Option<i64>, Option<i64>, Option<i64>) {
    match &declaration.night_window {
        None => (None, None, None),
        Some(window) => (
            Some(i64::from(window.start)),
            Some(i64::from(window.end)),
            Some(f64_to_sql(window.ratio)),
        ),
    }
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, PersistError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        params![name],
        |row| row.get(0),
    )?;
    Ok(count == 1)
}

fn any_user_table(conn: &Connection) -> Result<bool, PersistError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    Ok(count != 0)
}

fn read_user_version(conn: &Connection) -> Result<i64, PersistError> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    Ok(version)
}

fn read_meta(conn: &Connection) -> Result<Meta, PersistError> {
    conn.query_row(
        "SELECT schema_version, wip_upper_bits, wip_lower_bits,
                night_start, night_end, night_ratio_bits,
                wip_bits, in_process_count, refill_active,
                night_credit_bits, night_requests, night_admitted,
                prev_in_window, updated_wall_ms
         FROM gate_meta WHERE id = 1",
        [],
        |row| {
            Ok(Meta {
                schema_version: row.get(0)?,
                wip_upper_bits: row.get(1)?,
                wip_lower_bits: row.get(2)?,
                night_start: row.get(3)?,
                night_end: row.get(4)?,
                night_ratio_bits: row.get(5)?,
                wip_bits: row.get(6)?,
                in_process_count: row.get(7)?,
                refill_active: row.get(8)?,
                night_credit_bits: row.get(9)?,
                night_requests: row.get(10)?,
                night_admitted: row.get(11)?,
                prev_in_window: row.get(12)?,
                updated_wall_ms: row.get(13)?,
            })
        },
    )
    .map_err(|err| match err {
        rusqlite::Error::QueryReturnedNoRows => PersistError::Corrupt {
            detail: "gate_meta empty".to_string(),
        },
        other => PersistError::Sqlite(other),
    })
}

fn ensure_declaration(meta: &Meta, declared: &GateConfig) -> Result<(), PersistError> {
    let upper = sql_to_f64(meta.wip_upper_bits);
    if upper.to_bits() != declared.wip_upper_minutes.to_bits() {
        return Err(PersistError::DeclarationMismatch {
            field: "wip_upper_minutes",
        });
    }
    let lower = sql_to_f64(meta.wip_lower_bits);
    if lower.to_bits() != declared.wip_lower_minutes.to_bits() {
        return Err(PersistError::DeclarationMismatch {
            field: "wip_lower_minutes",
        });
    }
    let stored = stored_night(meta)?;
    match (stored.as_ref(), declared.night_window.as_ref()) {
        (None, None) => Ok(()),
        (Some(left), Some(right)) => {
            if left.start != right.start {
                return Err(PersistError::DeclarationMismatch {
                    field: "night_start",
                });
            }
            if left.end != right.end {
                return Err(PersistError::DeclarationMismatch { field: "night_end" });
            }
            if left.ratio.to_bits() != right.ratio.to_bits() {
                return Err(PersistError::DeclarationMismatch {
                    field: "night_ratio",
                });
            }
            Ok(())
        }
        _ => Err(PersistError::DeclarationMismatch {
            field: "night_window",
        }),
    }
}

fn stored_night(meta: &Meta) -> Result<Option<NightWindow>, PersistError> {
    match (meta.night_start, meta.night_end, meta.night_ratio_bits) {
        (None, None, None) => Ok(None),
        (Some(start), Some(end), Some(bits)) => {
            let ratio = sql_to_f64(bits);
            if !ratio.is_finite() {
                return Err(PersistError::Corrupt {
                    detail: "night ratio".to_string(),
                });
            }
            Ok(Some(NightWindow {
                start: minute_from_sql(start)?,
                end: minute_from_sql(end)?,
                ratio,
            }))
        }
        _ => Err(PersistError::Corrupt {
            detail: "night window columns".to_string(),
        }),
    }
}

fn load_runtime(conn: &Connection, meta: &Meta) -> Result<RuntimeSnapshot, PersistError> {
    let items = load_items(conn)?;
    let canary = load_canary(conn)?;
    let count = usize_from_sql(meta.in_process_count, "in_process_count")?;
    if count != items.len() {
        return Err(PersistError::Corrupt {
            detail: format!("in_process_count {count} != items {}", items.len()),
        });
    }
    let wip = sql_to_f64(meta.wip_bits);
    if !wip.is_finite() {
        return Err(PersistError::Corrupt {
            detail: "wip".to_string(),
        });
    }
    // wip_bits 必须是在制权重按 seq 的左折叠，与内存闸同一条定义。
    // 增量累加的 f64 与这条重求和会差 1 ULP，那是写入错误，不是这里放宽比较。
    let sum = sum_weights(items.iter().map(|item| item.weight));
    if sum.to_bits() != wip.to_bits() {
        return Err(PersistError::Corrupt {
            detail: format!(
                "wip bits {:x} != item sum {:x}",
                wip.to_bits(),
                sum.to_bits()
            ),
        });
    }
    let credit = sql_to_f64(meta.night_credit_bits);
    if !credit.is_finite() {
        return Err(PersistError::Corrupt {
            detail: "night credit".to_string(),
        });
    }
    if let Some(canary) = &canary {
        if canary.released > canary.max_probes {
            return Err(PersistError::Corrupt {
                detail: format!(
                    "canary released {} > max_probes {}",
                    canary.released, canary.max_probes
                ),
            });
        }
        if canary.open && canary.completed == 0 {
            return Err(PersistError::Corrupt {
                detail: "canary open with no completion".to_string(),
            });
        }
    }
    let prev_in_window = match meta.prev_in_window {
        None => None,
        Some(value) => Some(flag(value, "prev_in_window")?),
    };
    Ok(RuntimeSnapshot {
        wip,
        items,
        night_credit: credit,
        night_requests: u64_count_from_sql(meta.night_requests, "night_requests")?,
        night_admitted: u64_count_from_sql(meta.night_admitted, "night_admitted")?,
        prev_in_window,
        refill_active: flag(meta.refill_active, "refill_active")?,
        canary,
    })
}

fn load_items(conn: &Connection) -> Result<Vec<crate::gate::SnapshotItem>, PersistError> {
    let mut stmt = conn.prepare(
        "SELECT gate_id, item_id, config, weight_bits, admitted_at
         FROM wip_item ORDER BY seq ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;
    let mut items = Vec::new();
    for row in rows {
        let (gate_id, item_id, config, weight_bits, admitted_at) = row?;
        if gate_id != 1 {
            return Err(PersistError::Corrupt {
                detail: "wip_item.gate_id".to_string(),
            });
        }
        let weight = sql_to_f64(weight_bits);
        if !weight.is_finite() {
            return Err(PersistError::Corrupt {
                detail: "item weight".to_string(),
            });
        }
        items.push(crate::gate::SnapshotItem {
            id: sql_to_u64(item_id),
            config,
            weight,
            admitted_at: u32_from_sql(admitted_at, "admitted_at")?,
        });
    }
    Ok(items)
}

fn load_canary(conn: &Connection) -> Result<Option<crate::gate::CanarySnapshot>, PersistError> {
    let row = conn
        .query_row(
            "SELECT gate_id, config, max_probes, released, completed, defects, open
             FROM canary WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .optional()?;
    let Some((gate_id, config, max_probes, released, completed, defects, open)) = row else {
        return Ok(None);
    };
    if gate_id != 1 {
        return Err(PersistError::Corrupt {
            detail: "canary.gate_id".to_string(),
        });
    }
    Ok(Some(crate::gate::CanarySnapshot {
        config,
        max_probes: usize_from_sql(max_probes, "max_probes")?,
        released: usize_from_sql(released, "released")?,
        completed: usize_from_sql(completed, "completed")?,
        defects: sql_to_u64(defects),
        open: flag(open, "canary.open")?,
    }))
}

fn f64_to_sql(value: f64) -> i64 {
    value.to_bits() as i64
}

fn sql_to_f64(value: i64) -> f64 {
    f64::from_bits(value as u64)
}

fn u64_to_sql(value: u64) -> i64 {
    value as i64
}

fn sql_to_u64(value: i64) -> u64 {
    value as u64
}

fn usize_to_sql(value: usize, field: &'static str) -> Result<i64, PersistError> {
    i64::try_from(value).map_err(|_| PersistError::Corrupt {
        detail: field.to_string(),
    })
}

fn usize_from_sql(value: i64, field: &'static str) -> Result<usize, PersistError> {
    usize::try_from(value).map_err(|_| PersistError::Corrupt {
        detail: field.to_string(),
    })
}

fn u64_count_to_sql(value: u64, field: &'static str) -> Result<i64, PersistError> {
    i64::try_from(value).map_err(|_| PersistError::Corrupt {
        detail: field.to_string(),
    })
}

fn u64_count_from_sql(value: i64, field: &'static str) -> Result<u64, PersistError> {
    u64::try_from(value).map_err(|_| PersistError::Corrupt {
        detail: field.to_string(),
    })
}

fn u32_from_sql(value: i64, field: &'static str) -> Result<u32, PersistError> {
    u32::try_from(value).map_err(|_| PersistError::Corrupt {
        detail: field.to_string(),
    })
}

fn minute_from_sql(value: i64) -> Result<u32, PersistError> {
    let minute = u32_from_sql(value, "night minute")?;
    if minute >= 1440 {
        return Err(PersistError::Corrupt {
            detail: "night minute".to_string(),
        });
    }
    Ok(minute)
}

fn flag(value: i64, field: &'static str) -> Result<bool, PersistError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(PersistError::Corrupt {
            detail: field.to_string(),
        }),
    }
}
