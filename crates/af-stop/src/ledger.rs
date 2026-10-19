//! 指纹账本。一行是「指纹 → 计数 / 处置 / 时间窗」。
//!
//! 打开只有两条路：空库 [`OpenStatus::Created`]（建表并写入声明），或
//! [`OpenStatus::AlreadyDeclared`]（声明与已落盘的 schema 版本、阈值、时间窗、指纹库一致）。
//! 对不上就失败，不改库里的行。
//!
//! 计数只在工具错误上 +1。时间窗到期会把窗口滚到当前逻辑时刻，但不会把计数清零，
//! 也不改变 [`crate::StopEngine`] 的三条规则。过期比较用注入时钟的逻辑毫秒；
//! `observed_wall_ms` 只是墙钟审计。

use crate::clock::{Clock, SystemClock};
use crate::error::{Error, Result};
use crate::wire::{decode_wire, encode_wire, WIRE_VERSION};
use crate::{
    Decision, EngineSnapshot, Event, Fingerprint, FingerprintStore, StopEngine, StopPolicy,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const SCHEMA_VERSION: i64 = 1;

/// 打开账本时带上的声明。`v` 必须为 1。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerDeclaration {
    #[serde(deserialize_with = "crate::wire::require_v1")]
    pub v: u32,
    pub fingerprint_streak_limit: u64,
    pub normal_error_threshold: u64,
    /// 时间窗长度，毫秒。过期判定用注入时钟，不用墙钟。
    pub window_ms: u64,
    pub store: FingerprintStore,
}

impl LedgerDeclaration {
    pub fn new(
        fingerprint_streak_limit: u64,
        normal_error_threshold: u64,
        window_ms: u64,
        store: FingerprintStore,
    ) -> Self {
        Self {
            v: WIRE_VERSION,
            fingerprint_streak_limit,
            normal_error_threshold,
            window_ms,
            store,
        }
    }

    pub fn from_policy(policy: &StopPolicy, window_ms: u64) -> Result<Self> {
        Ok(Self {
            v: WIRE_VERSION,
            fingerprint_streak_limit: u64::try_from(policy.fingerprint_streak_limit)
                .map_err(|_| Error::Corrupt("连击阈值超出 u64".into()))?,
            normal_error_threshold: u64::try_from(policy.normal_error_threshold)
                .map_err(|_| Error::Corrupt("普通错误阈值超出 u64".into()))?,
            window_ms,
            store: policy.store.clone(),
        })
    }

    pub fn policy(&self) -> Result<StopPolicy> {
        Ok(StopPolicy {
            fingerprint_streak_limit: usize::try_from(self.fingerprint_streak_limit)
                .map_err(|_| Error::Corrupt("连击阈值超出 usize".into()))?,
            normal_error_threshold: usize::try_from(self.normal_error_threshold)
                .map_err(|_| Error::Corrupt("普通错误阈值超出 usize".into()))?,
            store: self.store.clone(),
        })
    }

    pub fn to_json(&self) -> Result<String> {
        crate::encode_doc(self)
    }

    pub fn from_json(text: &str) -> Result<Self> {
        crate::decode_doc(text)
    }
}

/// 账本里的一行。计数是该指纹的累计观测次数，只增不减。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FingerprintRecord {
    #[serde(deserialize_with = "crate::wire::require_v1")]
    pub v: u32,
    pub tool: String,
    pub error_code: String,
    pub count: u64,
    pub disposition: Decision,
    /// 当前时间窗起点，逻辑毫秒。
    pub window_start_ms: i64,
    /// 当前时间窗终点（不含）。逻辑毫秒 `>= window_end_ms` 视为到期。
    pub window_end_ms: i64,
    /// 最近一次观测的墙钟毫秒。不参与过期判定。
    pub observed_wall_ms: i64,
}

impl FingerprintRecord {
    pub fn to_json(&self) -> Result<String> {
        crate::encode_doc(self)
    }

    pub fn from_json(text: &str) -> Result<Self> {
        crate::decode_doc(text)
    }
}

/// 首启或重入。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenStatus {
    /// 空库：建表并写入声明。
    Created,
    /// 声明与已落盘状态一致。
    AlreadyDeclared,
}

#[derive(Clone)]
struct StoredFp {
    format_v: i64,
    count: i64,
    disposition: String,
    window_start_ms: i64,
    window_end_ms: i64,
    observed_wall_ms: i64,
}

/// 持久化的止损账本。一个实例持有一条 SQLite 连接。
pub struct FingerprintLedger {
    conn: Connection,
    clock: Arc<dyn Clock>,
    decl: LedgerDeclaration,
    engine: StopEngine,
    /// 打开时库里已提交的逻辑毫秒。
    mono_base: i64,
    /// 打开时 [`Clock::monotonic_ms`] 的读数。只把打开之后的流逝折进逻辑毫秒。
    clock_at_open: i64,
    observed: Cell<i64>,
}

impl std::fmt::Debug for FingerprintLedger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FingerprintLedger")
            .field("declaration", &self.decl)
            .field("engine", &self.engine)
            .field("mono_base", &self.mono_base)
            .field("logical_now", &self.logical_now())
            .finish_non_exhaustive()
    }
}

impl FingerprintLedger {
    /// 打开（必要时创建）账本文件。时刻用 [`SystemClock`]。
    pub fn open(path: impl AsRef<Path>, decl: LedgerDeclaration) -> Result<(Self, OpenStatus)> {
        Self::open_with_clock(path, decl, SystemClock::new())
    }

    /// 同 [`FingerprintLedger::open`]，间隔与过期走 `clock`。
    pub fn open_with_clock(
        path: impl AsRef<Path>,
        decl: LedgerDeclaration,
        clock: impl Clock + 'static,
    ) -> Result<(Self, OpenStatus)> {
        let conn = Connection::open(path)?;
        Self::connect(conn, decl, Arc::new(clock))
    }

    /// 内存账本。没有跨进程恢复的意义，返回 [`OpenStatus::Created`]。
    pub fn open_in_memory(decl: LedgerDeclaration) -> Result<(Self, OpenStatus)> {
        Self::open_in_memory_with_clock(decl, SystemClock::new())
    }

    /// 同 [`FingerprintLedger::open_in_memory`]，间隔与过期走 `clock`。
    pub fn open_in_memory_with_clock(
        decl: LedgerDeclaration,
        clock: impl Clock + 'static,
    ) -> Result<(Self, OpenStatus)> {
        let conn = Connection::open_in_memory()?;
        Self::connect(conn, decl, Arc::new(clock))
    }

    fn connect(
        mut conn: Connection,
        decl: LedgerDeclaration,
        clock: Arc<dyn Clock>,
    ) -> Result<(Self, OpenStatus)> {
        configure(&conn)?;
        // 版本或阈值不合法时不建表。
        let policy = decl.policy()?;
        require_fit("时间窗", decl.window_ms)?;
        if decl.v != WIRE_VERSION {
            return Err(Error::UnknownVersion {
                found: decl.v,
                expected: WIRE_VERSION,
            });
        }

        let version = user_version(&conn)?;
        let has_decl = table_exists(&conn, "declaration")?;
        let has_runtime = table_exists(&conn, "runtime")?;
        let has_fingerprint = table_exists(&conn, "fingerprint")?;
        let tables = has_decl && has_runtime && has_fingerprint;
        let any_table = has_decl || has_runtime || has_fingerprint;

        let status = match (version, tables, any_table) {
            (0, false, false) => {
                create(&mut conn, &decl)?;
                OpenStatus::Created
            }
            (0, _, true) => {
                return Err(Error::Corrupt(
                    "user_version 为 0 但表已存在，拒绝当成首启".into(),
                ));
            }
            (SCHEMA_VERSION, true, _) => {
                let stored = read_declaration(&conn)?;
                if stored != decl {
                    return Err(mismatch(&stored, &decl));
                }
                OpenStatus::AlreadyDeclared
            }
            (SCHEMA_VERSION, false, _) => {
                return Err(Error::Corrupt(
                    "user_version 为 1 但表不完整，拒绝补建".into(),
                ));
            }
            (other, _, _) => {
                return Err(Error::UnknownSchema {
                    found: other,
                    expected: SCHEMA_VERSION,
                });
            }
        };

        let (snap, mono_base) = read_runtime(&conn)?;
        // 重入时先把每一行读出来。未知版本或坏窗口在启动时失败，而不是留到下一次 feed。
        let _ = load_all(&conn)?;
        let engine = StopEngine::from_snapshot(policy, snap)?;
        let clock_at_open = clock.monotonic_ms();
        Ok((
            FingerprintLedger {
                conn,
                clock,
                decl,
                engine,
                mono_base,
                clock_at_open,
                observed: Cell::new(mono_base),
            },
            status,
        ))
    }

    pub fn declaration(&self) -> &LedgerDeclaration {
        &self.decl
    }

    /// 打开之后的逻辑毫秒：已提交高水位，加上本次打开后注入钟的流逝。只增不减。
    ///
    /// 账本关闭期间注入钟的读数不计入。`Instant` 不能跨进程比较，停机时段也不借用墙钟补时。
    pub fn logical_now(&self) -> i64 {
        let tick = self.clock.monotonic_ms();
        let delta = tick.saturating_sub(self.clock_at_open);
        let logical = self.mono_base.saturating_add(delta);
        logical.max(self.observed.get())
    }

    pub fn current_streak(&self) -> Option<(&Fingerprint, usize)> {
        self.engine.current_streak()
    }

    pub fn normal_error_count(&self) -> usize {
        self.engine.normal_error_count()
    }

    /// 把事件交给引擎，并把新的运行状态写进账本。
    ///
    /// 工具错误额外把该指纹的计数 +1，更新处置与时间窗。写入失败时内存引擎回到调用前。
    pub fn feed(&mut self, event: Event) -> Result<Decision> {
        let before = self.engine.snapshot();
        let decision = self.engine.feed(event.clone());
        let logical = self.logical_now();
        let wall = self.clock.wall_ms();
        let after = self.engine.snapshot();
        match self.persist(&event, &decision, &after, logical, wall) {
            Ok(()) => {
                self.observed.set(self.observed.get().max(logical));
                Ok(decision)
            }
            Err(err) => {
                match StopEngine::from_snapshot(self.decl.policy()?, before) {
                    Ok(engine) => self.engine = engine,
                    Err(restore) => {
                        return Err(Error::Corrupt(format!(
                            "回滚失败：{restore}；写入失败：{err}"
                        )));
                    }
                }
                Err(err)
            }
        }
    }

    fn persist(
        &mut self,
        event: &Event,
        decision: &Decision,
        after: &EngineSnapshot,
        logical: i64,
        wall: i64,
    ) -> Result<()> {
        if logical < 0 {
            return Err(Error::Corrupt(format!("逻辑毫秒为负：{logical}")));
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        write_runtime(&tx, after, logical)?;
        if let Event::ToolError { tool, error_code } = event {
            upsert_fingerprint(
                &tx,
                tool,
                error_code,
                decision,
                logical,
                wall,
                self.decl.window_ms,
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn record(&self, fp: &Fingerprint) -> Result<Option<FingerprintRecord>> {
        let Some(row) = load_one(&self.conn, &fp.tool, &fp.error_code)? else {
            return Ok(None);
        };
        Ok(Some(record_from_parts(&fp.tool, &fp.error_code, row)?))
    }

    pub fn records(&self) -> Result<Vec<FingerprintRecord>> {
        let mut rows = load_all(&self.conn)?;
        rows.sort_by(|left, right| {
            (&left.tool, &left.error_code).cmp(&(&right.tool, &right.error_code))
        });
        Ok(rows)
    }
}

fn configure(conn: &Connection) -> Result<()> {
    conn.busy_timeout(Duration::from_millis(5_000))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

fn require_fit(name: &str, value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::Corrupt(format!("{name} 超出 i64")))
}

fn user_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

fn create(conn: &mut Connection, decl: &LedgerDeclaration) -> Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        "CREATE TABLE declaration (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            format_v INTEGER NOT NULL,
            streak_limit INTEGER NOT NULL CHECK (streak_limit >= 0),
            threshold INTEGER NOT NULL CHECK (threshold >= 0),
            window_ms INTEGER NOT NULL CHECK (window_ms >= 0),
            store_json TEXT NOT NULL
        );
        CREATE TABLE runtime (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            declaration_id INTEGER NOT NULL REFERENCES declaration(id),
            mono_ms INTEGER NOT NULL CHECK (mono_ms >= 0),
            streak_tool TEXT,
            streak_code TEXT,
            streak_len INTEGER NOT NULL CHECK (streak_len >= 0),
            normal_errors INTEGER NOT NULL CHECK (normal_errors >= 0),
            CHECK (
                (streak_tool IS NULL AND streak_code IS NULL AND streak_len = 0)
                OR (streak_tool IS NOT NULL AND streak_code IS NOT NULL AND streak_len >= 1)
            )
        );
        CREATE TABLE fingerprint (
            tool TEXT NOT NULL,
            error_code TEXT NOT NULL,
            format_v INTEGER NOT NULL,
            count INTEGER NOT NULL CHECK (count >= 1),
            disposition TEXT NOT NULL,
            window_start_ms INTEGER NOT NULL,
            window_end_ms INTEGER NOT NULL,
            observed_wall_ms INTEGER NOT NULL,
            PRIMARY KEY (tool, error_code),
            CHECK (window_end_ms >= window_start_ms),
            CHECK (window_start_ms >= 0)
        );",
    )?;
    let streak_limit = require_fit("连击阈值", decl.fingerprint_streak_limit)?;
    let threshold = require_fit("普通错误阈值", decl.normal_error_threshold)?;
    let window_ms = require_fit("时间窗", decl.window_ms)?;
    let store_json = encode_wire(&decl.store)?;
    tx.execute(
        "INSERT INTO declaration (id, format_v, streak_limit, threshold, window_ms, store_json)
         VALUES (1, ?1, ?2, ?3, ?4, ?5)",
        params![
            i64::from(WIRE_VERSION),
            streak_limit,
            threshold,
            window_ms,
            store_json
        ],
    )?;
    tx.execute(
        "INSERT INTO runtime (
            id, declaration_id, mono_ms, streak_tool, streak_code, streak_len, normal_errors
         ) VALUES (1, 1, 0, NULL, NULL, 0, 0)",
        [],
    )?;
    tx.execute_batch("PRAGMA user_version = 1")?;
    tx.commit()?;
    Ok(())
}

fn read_declaration(conn: &Connection) -> Result<LedgerDeclaration> {
    let (format_v, streak_limit, threshold, window_ms, store_json): (i64, i64, i64, i64, String) =
        conn.query_row(
            "SELECT format_v, streak_limit, threshold, window_ms, store_json
             FROM declaration WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?;
    let v = parse_format_v(format_v)?;
    let store = decode_stored_wire(&store_json)?;
    Ok(LedgerDeclaration {
        v,
        fingerprint_streak_limit: non_negative("连击阈值", streak_limit)?,
        normal_error_threshold: non_negative("普通错误阈值", threshold)?,
        window_ms: non_negative("时间窗", window_ms)?,
        store,
    })
}

fn read_runtime(conn: &Connection) -> Result<(EngineSnapshot, i64)> {
    let (tool, code, len, normal, mono): (Option<String>, Option<String>, i64, i64, i64) = conn
        .query_row(
            "SELECT streak_tool, streak_code, streak_len, normal_errors, mono_ms
             FROM runtime WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?;
    if mono < 0 || len < 0 || normal < 0 {
        return Err(Error::Corrupt("运行状态出现负数".into()));
    }
    let streak = match (tool, code) {
        (None, None) => {
            if len != 0 {
                return Err(Error::Corrupt("空连击的长度不是 0".into()));
            }
            None
        }
        (Some(tool), Some(code)) => {
            if len < 1 {
                return Err(Error::Corrupt("连击长度小于 1".into()));
            }
            Some((Fingerprint::new(tool, code), non_negative("连击长度", len)?))
        }
        _ => return Err(Error::Corrupt("连击指纹只有一半".into())),
    };
    let snap = EngineSnapshot {
        v: WIRE_VERSION,
        streak,
        normal_errors: non_negative("普通错误计数", normal)?,
    };
    Ok((snap, mono))
}

fn write_runtime(conn: &Connection, snap: &EngineSnapshot, mono: i64) -> Result<()> {
    if snap.v != WIRE_VERSION {
        return Err(Error::UnknownVersion {
            found: snap.v,
            expected: WIRE_VERSION,
        });
    }
    let (tool, code, len) = match &snap.streak {
        None => (None, None, 0i64),
        Some((fp, len)) => {
            if *len == 0 {
                return Err(Error::Corrupt("连击长度为 0".into()));
            }
            let len = i64::try_from(*len).map_err(|_| Error::Corrupt("连击长度超出 i64".into()))?;
            (Some(fp.tool.clone()), Some(fp.error_code.clone()), len)
        }
    };
    let normal = i64::try_from(snap.normal_errors)
        .map_err(|_| Error::Corrupt("普通错误计数超出 i64".into()))?;
    // 普通错误计数和逻辑毫秒只允许变大。连击可以因打断落到空或 1，这是引擎规则，不是计数回退。
    let changed = conn.execute(
        "UPDATE runtime
         SET streak_tool = ?1, streak_code = ?2, streak_len = ?3,
             normal_errors = ?4, mono_ms = ?5
         WHERE id = 1 AND ?4 >= normal_errors AND ?5 >= mono_ms",
        params![tool, code, len, normal, mono],
    )?;
    if changed == 1 {
        return Ok(());
    }
    // 值没变时 SQLite 可能报告 0 行。内容已经是目标状态就不是回退。
    let (current, current_mono) = read_runtime(conn)?;
    if current == *snap && current_mono == mono {
        return Ok(());
    }
    Err(Error::Corrupt("运行状态拒绝回退".into()))
}

fn upsert_fingerprint(
    conn: &Connection,
    tool: &str,
    code: &str,
    decision: &Decision,
    logical: i64,
    wall: i64,
    window_ms: u64,
) -> Result<()> {
    let existing = load_one(conn, tool, code)?;
    let (count, start, end) = match existing {
        None => (1i64, logical, add_window(logical, window_ms)),
        Some(row) => {
            let _ = record_from_parts(tool, code, row.clone())?;
            let count = row
                .count
                .checked_add(1)
                .ok_or_else(|| Error::Corrupt("指纹计数溢出".into()))?;
            if logical >= row.window_end_ms {
                (count, logical, add_window(logical, window_ms))
            } else {
                (count, row.window_start_ms, row.window_end_ms)
            }
        }
    };
    let disposition = encode_wire(decision)?;
    // WHERE 保证新计数严格大于旧计数。插不进去就失败，不把更小的数写回去。
    let changed = conn.execute(
        "INSERT INTO fingerprint (
            tool, error_code, format_v, count, disposition,
            window_start_ms, window_end_ms, observed_wall_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(tool, error_code) DO UPDATE SET
            format_v = excluded.format_v,
            count = excluded.count,
            disposition = excluded.disposition,
            window_start_ms = excluded.window_start_ms,
            window_end_ms = excluded.window_end_ms,
            observed_wall_ms = excluded.observed_wall_ms
         WHERE excluded.count > fingerprint.count
           AND excluded.format_v = fingerprint.format_v",
        params![
            tool,
            code,
            i64::from(WIRE_VERSION),
            count,
            disposition,
            start,
            end,
            wall
        ],
    )?;
    if changed != 1 {
        return Err(Error::Corrupt("指纹计数拒绝回退".into()));
    }
    Ok(())
}

fn add_window(start: i64, window_ms: u64) -> i64 {
    match i64::try_from(window_ms) {
        Ok(window) => start.saturating_add(window),
        Err(_) => i64::MAX,
    }
}

fn load_one(conn: &Connection, tool: &str, code: &str) -> Result<Option<StoredFp>> {
    let row = conn
        .query_row(
            "SELECT format_v, count, disposition, window_start_ms, window_end_ms, observed_wall_ms
             FROM fingerprint WHERE tool = ?1 AND error_code = ?2",
            params![tool, code],
            read_stored_fp,
        )
        .optional()?;
    Ok(row)
}

fn load_all(conn: &Connection) -> Result<Vec<FingerprintRecord>> {
    let mut stmt = conn.prepare(
        "SELECT tool, error_code, format_v, count, disposition,
                window_start_ms, window_end_ms, observed_wall_ms
         FROM fingerprint",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            read_stored_fp_at(row, 2)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (tool, code, stored) = row?;
        out.push(record_from_parts(&tool, &code, stored)?);
    }
    Ok(out)
}

fn read_stored_fp(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredFp> {
    read_stored_fp_at(row, 0)
}

fn read_stored_fp_at(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<StoredFp> {
    Ok(StoredFp {
        format_v: row.get(offset)?,
        count: row.get(offset + 1)?,
        disposition: row.get(offset + 2)?,
        window_start_ms: row.get(offset + 3)?,
        window_end_ms: row.get(offset + 4)?,
        observed_wall_ms: row.get(offset + 5)?,
    })
}

fn record_from_parts(tool: &str, code: &str, row: StoredFp) -> Result<FingerprintRecord> {
    let v = parse_format_v(row.format_v)?;
    if row.count < 1 || row.window_start_ms < 0 || row.window_end_ms < row.window_start_ms {
        return Err(Error::Corrupt(format!(
            "指纹 {tool}/{code} 的计数或时间窗损坏"
        )));
    }
    let count = u64::try_from(row.count).map_err(|_| Error::Corrupt("指纹计数超出 u64".into()))?;
    let disposition = decode_stored_wire(&row.disposition)?;
    Ok(FingerprintRecord {
        v,
        tool: tool.to_string(),
        error_code: code.to_string(),
        count,
        disposition,
        window_start_ms: row.window_start_ms,
        window_end_ms: row.window_end_ms,
        observed_wall_ms: row.observed_wall_ms,
    })
}

fn parse_format_v(raw: i64) -> Result<u32> {
    let Ok(found) = u32::try_from(raw) else {
        return Err(Error::UnreadableVersion {
            raw: raw.to_string(),
        });
    };
    if found != WIRE_VERSION {
        return Err(Error::UnknownVersion {
            found,
            expected: WIRE_VERSION,
        });
    }
    Ok(found)
}

fn non_negative(name: &str, raw: i64) -> Result<u64> {
    u64::try_from(raw).map_err(|_| Error::Corrupt(format!("{name} = {raw}")))
}

fn decode_stored_wire<T: DeserializeOwned>(text: &str) -> Result<T> {
    decode_wire(text).map_err(|err| match err {
        Error::Json(json) => Error::Corrupt(json.to_string()),
        other => other,
    })
}

fn mismatch(stored: &LedgerDeclaration, decl: &LedgerDeclaration) -> Error {
    let detail = if stored.fingerprint_streak_limit != decl.fingerprint_streak_limit {
        format!(
            "fingerprint_streak_limit 已存 {} 与声明 {} 不符",
            stored.fingerprint_streak_limit, decl.fingerprint_streak_limit
        )
    } else if stored.normal_error_threshold != decl.normal_error_threshold {
        format!(
            "normal_error_threshold 已存 {} 与声明 {} 不符",
            stored.normal_error_threshold, decl.normal_error_threshold
        )
    } else if stored.window_ms != decl.window_ms {
        format!(
            "window_ms 已存 {} 与声明 {} 不符",
            stored.window_ms, decl.window_ms
        )
    } else if stored.store != decl.store {
        format!(
            "store 不一致：已存 {} 条，声明 {} 条",
            stored.store.len(),
            decl.store.len()
        )
    } else if stored.v != decl.v {
        format!("版本已存 {} 与声明 {} 不符", stored.v, decl.v)
    } else {
        "声明与已落盘状态不符".to_string()
    };
    Error::DeclarationMismatch { detail }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;
    use crate::{FingerprintAction, FingerprintEntry};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    struct TempDb {
        path: PathBuf,
    }

    impl TempDb {
        fn new() -> Self {
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("af-stop-ledger-{}-{n}.db", std::process::id()));
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            for suffix in ["", "-wal", "-shm", "-journal"] {
                let path = if suffix.is_empty() {
                    self.path.clone()
                } else {
                    PathBuf::from(format!("{}{suffix}", self.path.display()))
                };
                let _ = std::fs::remove_file(path);
            }
        }
    }

    fn decl() -> LedgerDeclaration {
        LedgerDeclaration::new(3, 5, 1_000, FingerprintStore::new())
    }

    fn clock() -> ManualClock {
        ManualClock::new(0, 1_700_000_000_000)
    }

    #[test]
    fn file_pragmas_match_the_ledger_recipe() {
        let db = TempDb::new();
        let (ledger, status) =
            FingerprintLedger::open_with_clock(db.path(), decl(), clock()).unwrap();
        assert_eq!(status, OpenStatus::Created);
        let journal: String = ledger
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal.to_ascii_lowercase(), "wal");
        let sync: i64 = ledger
            .conn
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .unwrap();
        assert_eq!(sync, 1, "synchronous=NORMAL");
        let foreign_keys: i64 = ledger
            .conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);
        let busy: i64 = ledger
            .conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(busy, 5_000);
        let version: i64 = ledger
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
        let err = ledger
            .conn
            .execute("DELETE FROM declaration", [])
            .unwrap_err();
        let message = err.to_string();
        assert!(
            message.to_ascii_lowercase().contains("foreign key"),
            "{message}"
        );
    }

    #[test]
    fn memory_ledger_still_enables_foreign_keys_and_busy_timeout() {
        let (ledger, status) =
            FingerprintLedger::open_in_memory_with_clock(decl(), clock()).unwrap();
        assert_eq!(status, OpenStatus::Created);
        let journal: String = ledger
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal.to_ascii_lowercase(), "memory");
        let foreign_keys: i64 = ledger
            .conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);
        let busy: i64 = ledger
            .conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(busy, 5_000);
    }

    #[test]
    fn unknown_schema_user_version_fails_closed() {
        let db = TempDb::new();
        let sample = decl();
        let (ledger, _) =
            FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock()).unwrap();
        drop(ledger);
        let conn = Connection::open(db.path()).unwrap();
        conn.pragma_update(None, "user_version", 2i64).unwrap();
        drop(conn);
        let err = FingerprintLedger::open_with_clock(db.path(), sample, clock()).unwrap_err();
        assert!(
            matches!(
                err,
                Error::UnknownSchema {
                    found: 2,
                    expected: 1
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn user_version_zero_with_tables_is_not_treated_as_create() {
        let db = TempDb::new();
        let sample = decl();
        let (ledger, _) =
            FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock()).unwrap();
        drop(ledger);
        let conn = Connection::open(db.path()).unwrap();
        conn.pragma_update(None, "user_version", 0i64).unwrap();
        drop(conn);
        let err = FingerprintLedger::open_with_clock(db.path(), sample, clock()).unwrap_err();
        assert!(matches!(err, Error::Corrupt(_)), "{err}");
    }

    #[test]
    fn incomplete_schema_is_not_repaired() {
        let db = TempDb::new();
        let sample = decl();
        let (ledger, _) =
            FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock()).unwrap();
        drop(ledger);
        let conn = Connection::open(db.path()).unwrap();
        conn.execute_batch("DROP TABLE fingerprint").unwrap();
        drop(conn);
        let err = FingerprintLedger::open_with_clock(db.path(), sample, clock()).unwrap_err();
        assert!(matches!(err, Error::Corrupt(_)), "{err}");
    }

    #[test]
    fn tampered_disposition_version_fails_at_open() {
        let db = TempDb::new();
        let sample = decl();
        let (mut ledger, _) =
            FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock()).unwrap();
        ledger
            .feed(Event::ToolError {
                tool: "search".into(),
                error_code: "E_TIMEOUT".into(),
            })
            .unwrap();
        drop(ledger);
        let conn = Connection::open(db.path()).unwrap();
        conn.execute(
            "UPDATE fingerprint SET disposition = ?1",
            params![r#"{"v":2,"body":"Continue"}"#],
        )
        .unwrap();
        drop(conn);
        let err = FingerprintLedger::open_with_clock(db.path(), sample, clock()).unwrap_err();
        assert!(
            matches!(
                err,
                Error::UnknownVersion {
                    found: 2,
                    expected: 1
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn tampered_row_format_version_fails_at_open() {
        let db = TempDb::new();
        let sample = decl();
        let (mut ledger, _) =
            FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock()).unwrap();
        ledger
            .feed(Event::ToolError {
                tool: "search".into(),
                error_code: "E_TIMEOUT".into(),
            })
            .unwrap();
        drop(ledger);
        let conn = Connection::open(db.path()).unwrap();
        conn.execute("UPDATE fingerprint SET format_v = 2", [])
            .unwrap();
        drop(conn);
        let err =
            FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock()).unwrap_err();
        assert!(
            matches!(
                err,
                Error::UnknownVersion {
                    found: 2,
                    expected: 1
                }
            ),
            "{err}"
        );

        let conn = Connection::open(db.path()).unwrap();
        conn.execute("UPDATE fingerprint SET format_v = -1", [])
            .unwrap();
        drop(conn);
        let err = FingerprintLedger::open_with_clock(db.path(), sample, clock()).unwrap_err();
        assert!(matches!(err, Error::UnreadableVersion { .. }), "{err}");
    }

    #[test]
    fn declaration_json_rejects_unknown_version() {
        let mut sample = decl();
        sample.store.register(
            Fingerprint::new("deploy", "E_LOCKED"),
            FingerprintEntry {
                streak_limit: Some(0),
                action: FingerprintAction::StopNoInfluence,
            },
        );
        let text = sample.to_json().unwrap();
        assert!(text.contains("\"v\":1"), "{text}");
        assert_eq!(LedgerDeclaration::from_json(&text).unwrap(), sample);

        let missing = LedgerDeclaration::from_json(&text.replacen("\"v\":1,", "", 1)).unwrap_err();
        assert!(matches!(missing, Error::MissingVersion), "{missing}");

        let bad = text.replacen("\"v\":1", "\"v\":2", 1);
        let err = LedgerDeclaration::from_json(&bad).unwrap_err();
        assert!(
            matches!(
                err,
                Error::UnknownVersion {
                    found: 2,
                    expected: 1
                }
            ),
            "{err}"
        );
        sample.v = 4;
        let err = sample.to_json().unwrap_err();
        assert!(
            matches!(
                err,
                Error::UnknownVersion {
                    found: 4,
                    expected: 1
                }
            ),
            "{err}"
        );
    }
}
