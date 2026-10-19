//! 抽样账本：谱系轨迹、当前水位、金标注入记录落在单个 SQLite 文件里。
//!
//! 恢复不另写升迁规则。重入时按轨迹里的 [`RunOutcome`] 重放 [`Engine::record`]，
//! 再与落盘的水位和窗口对账；对不上就拒绝启动。之后的 [`SamplingLedger::record`]
//! 仍走同一条 `Engine::record`。
//!
//! 连净回升不清窗口，而且优先于“窗口积满即评估”。因此窗口行数可以等于配方窗口，
//! 这不是损坏。
//!
//! 开库配方与 af-station 相同：WAL、`busy_timeout` 5000ms、`synchronous=NORMAL`、
//! `foreign_keys=ON`。绝对时刻只来自注入的 [`crate::Clock`]，测试不睡眠。

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{params, Connection, TransactionBehavior};

use crate::clock::Clock;
use crate::csp1::{Phase, Verdict};
use crate::golden::{GoldenLedger, InjectorConfig, Tagged};
use crate::promotion::{Action, Engine, Event, Lineage, Recipe, RunOutcome};
use crate::SamplingError;

/// 本账本 schema。版本对不上则启动失败，不迁移、不迁就。
pub const SCHEMA_VERSION: i64 = 1;

const BUSY_TIMEOUT: Duration = Duration::from_millis(5000);

/// 首开窗口里对 `database is locked` / `already exists` 的重试次数。
const COLD_START_ATTEMPTS: u32 = 16;

const EXPECTED_TABLES: &[&str] = &[
    "golden_injection",
    "ledger_root",
    "lineage_event",
    "sampling_decl",
    "watermark",
    "window_slot",
];

const SCHEMA_SQL: &str = "
CREATE TABLE ledger_root (
    id INTEGER PRIMARY KEY CHECK (id = 1)
);
CREATE TABLE sampling_decl (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema_version INTEGER NOT NULL,
    recipe_json TEXT NOT NULL,
    injector_p_bits INTEGER NOT NULL,
    injector_defect_share_bits INTEGER NOT NULL,
    root_id INTEGER NOT NULL REFERENCES ledger_root(id)
);
CREATE TABLE watermark (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    current TEXT NOT NULL CHECK (current IN ('agent','hybrid','deterministic')),
    attained TEXT NOT NULL CHECK (attained IN ('agent','hybrid','deterministic')),
    clean_streak INTEGER NOT NULL CHECK (clean_streak >= 0),
    updated_at_ms INTEGER NOT NULL,
    root_id INTEGER NOT NULL REFERENCES ledger_root(id),
    CHECK (
        CASE current WHEN 'agent' THEN 0 WHEN 'hybrid' THEN 1 ELSE 2 END
        <=
        CASE attained WHEN 'agent' THEN 0 WHEN 'hybrid' THEN 1 ELSE 2 END
    )
);
CREATE TABLE window_slot (
    pos INTEGER PRIMARY KEY CHECK (pos >= 0),
    success INTEGER NOT NULL CHECK (success IN (0, 1)),
    violations INTEGER NOT NULL CHECK (violations >= 0),
    consistency_bits INTEGER NOT NULL,
    root_id INTEGER NOT NULL REFERENCES ledger_root(id)
);
CREATE TABLE lineage_event (
    tick INTEGER PRIMARY KEY,
    action TEXT NOT NULL CHECK (action IN ('hold','promoted','demoted','recovered')),
    current_after TEXT NOT NULL CHECK (current_after IN ('agent','hybrid','deterministic')),
    success INTEGER NOT NULL CHECK (success IN (0, 1)),
    violations INTEGER NOT NULL CHECK (violations >= 0),
    consistency_bits INTEGER NOT NULL,
    recorded_at_ms INTEGER NOT NULL,
    root_id INTEGER NOT NULL REFERENCES ledger_root(id)
);
CREATE TABLE golden_injection (
    seq INTEGER PRIMARY KEY,
    defective INTEGER NOT NULL CHECK (defective IN (0, 1)),
    defect_caught INTEGER NOT NULL CHECK (defect_caught IN (0, 1)),
    disposition TEXT NOT NULL CHECK (
        disposition IN ('intercepted','released','false_reject','clean_passed')
    ),
    observed_at_ms INTEGER NOT NULL,
    root_id INTEGER NOT NULL REFERENCES ledger_root(id)
);
";

/// 首启建表，或重入且声明与库内一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenFork {
    /// 库里还没有本账本的表：建表，水位从 agent/agent 开始。
    Created,
    /// 已有声明且 schema 版本、配方、注入参数都一致：装回状态。
    AlreadyDeclared,
}

/// 打开账本时提交的声明。重入必须与首启解析出的关键参数一致。
#[derive(Debug, Clone)]
pub struct LedgerDeclaration {
    /// 升迁配方 JSON。比较的是校验后的规范形，空白差异不算不符。
    pub recipe_json: String,
    /// 金标注入率与缺陷占比。按 IEEE 位比较。
    pub injector: InjectorConfig,
}

/// 金标在放行点的处置。与 [`GoldenLedger::observe`] 的四类计数一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoldenDisposition {
    /// 缺陷金标被拦截。
    Intercepted,
    /// 缺陷金标被放行。告警 +1。
    Released,
    /// 干净金标被误拦。
    FalseReject,
    /// 干净金标放行。
    CleanPassed,
}

impl GoldenDisposition {
    fn from_flags(defective: bool, defect_caught: bool) -> Self {
        match (defective, defect_caught) {
            (true, true) => Self::Intercepted,
            (true, false) => Self::Released,
            (false, true) => Self::FalseReject,
            (false, false) => Self::CleanPassed,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Intercepted => "intercepted",
            Self::Released => "released",
            Self::FalseReject => "false_reject",
            Self::CleanPassed => "clean_passed",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        match name {
            "intercepted" => Some(Self::Intercepted),
            "released" => Some(Self::Released),
            "false_reject" => Some(Self::FalseReject),
            "clean_passed" => Some(Self::CleanPassed),
            _ => None,
        }
    }
}

/// 一条已落盘的金标注入记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldenInjectionRecord {
    pub seq: u64,
    pub defective: bool,
    pub defect_caught: bool,
    pub disposition: GoldenDisposition,
    pub observed_at_ms: i64,
}

/// 一条已落盘的谱系轨迹。`current_after` 是该步之后的当前水位。
#[derive(Debug, Clone, PartialEq)]
pub struct PersistedEvent {
    pub tick: u64,
    pub action: Action,
    pub current_after: Lineage,
    pub outcome: RunOutcome,
    pub recorded_at_ms: i64,
}

struct PreparedDecl {
    canonical: String,
    p_bits: i64,
    d_bits: i64,
}

struct EventRow {
    tick: i64,
    action: String,
    current_after: String,
    success: i64,
    violations: i64,
    consistency_bits: i64,
    recorded_at_ms: i64,
}

struct WindowRow {
    pos: i64,
    success: i64,
    violations: i64,
    consistency_bits: i64,
}

struct GoldenRow {
    seq: i64,
    defective: i64,
    defect_caught: i64,
    disposition: String,
    observed_at_ms: i64,
}

/// 抽样账本。一个实例持有一条 SQLite 连接。
///
/// 内存里的 [`Engine`] 与 [`GoldenLedger`] 是库的镜像。写入先落盘成功，再替换镜像。
pub struct SamplingLedger {
    conn: Connection,
    clock: Arc<dyn Clock>,
    engine: Engine,
    golden: GoldenLedger,
}

impl std::fmt::Debug for SamplingLedger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SamplingLedger")
            .field("current", &self.current())
            .field("attained", &self.attained())
            .field("clean_streak", &self.clean_streak())
            .field("events", &self.events().len())
            .field("golden_seen", &self.golden.golden_seen())
            .finish()
    }
}

impl SamplingLedger {
    /// 打开（必要时创建）账本文件。
    ///
    /// `clock` 的 [`Clock::now_ms`] 写入绝对时刻。升迁判据不读这个钟。
    /// 先读 `user_version`：已经是 [`SCHEMA_VERSION`] 就不再做 delete→WAL。
    /// 空库在一个 `BEGIN IMMEDIATE` 里建表并写下版本。`user_version` 仍是 0
    /// 但表和声明都对得上的旧库按 [`OpenFork::AlreadyDeclared`] 打开，不改水位。
    /// `database is locked` 或 `already exists` 在首开窗口内按注入的退避重试，
    /// 用尽仍失败则原样返回。
    pub fn open(
        path: impl AsRef<Path>,
        declaration: &LedgerDeclaration,
        clock: impl Clock + 'static,
    ) -> Result<(Self, OpenFork), SamplingError> {
        let prepared = prepare_declaration(declaration)?;
        let clock: Arc<dyn Clock> = Arc::new(clock);
        let path = path.as_ref();
        let mut attempt = 0u32;
        loop {
            let opened = match Connection::open(path) {
                Ok(conn) => Self::from_conn(conn, &prepared, Arc::clone(&clock)),
                Err(err) => Err(SamplingError::from(err)),
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

    /// 内存账本。连接关掉就没了，不能用来验证重启。
    pub fn open_in_memory(
        declaration: &LedgerDeclaration,
        clock: impl Clock + 'static,
    ) -> Result<(Self, OpenFork), SamplingError> {
        let prepared = prepare_declaration(declaration)?;
        let conn = Connection::open_in_memory()?;
        Self::from_conn(conn, &prepared, Arc::new(clock))
    }

    fn from_conn(
        mut conn: Connection,
        prepared: &PreparedDecl,
        clock: Arc<dyn Clock>,
    ) -> Result<(Self, OpenFork), SamplingError> {
        connection_pragmas(&conn)?;
        // ① 先读版本。已声明的库不再碰 delete→WAL 转换。
        let version = user_version(&conn)?;
        let fork = if version == SCHEMA_VERSION {
            ensure_journal_mode(&conn)?;
            assert_connection_recipe(&conn)?;
            load_declared(&mut conn, prepared)?
        } else if version != 0 {
            return Err(SamplingError::DeclarationMismatch(format!(
                "user_version {version} 不是抽样账本 schema {SCHEMA_VERSION}"
            )));
        } else {
            // delete→WAL 要排他锁。SQLite 禁止在事务里改 WAL，所以这一步紧挨在 BEGIN 之前。
            ensure_journal_mode(&conn)?;
            assert_connection_recipe(&conn)?;
            let now_ms = clock.now_ms();
            bootstrap(&mut conn, prepared, now_ms)?
        };
        let recipe_json = stored_recipe(&conn)?;
        let engine = Engine::from_recipe_json(&recipe_json)
            .map_err(|err| SamplingError::LedgerCorrupt(format!("库内配方无法复原: {err}")))?;
        let mut ledger = SamplingLedger {
            conn,
            clock,
            engine,
            golden: GoldenLedger::default(),
        };
        ledger.load_verified()?;
        Ok((ledger, fork))
    }

    /// 记录一次运行结果并落盘。返回的处置与未持久化的 [`Engine::record`] 相同。
    pub fn record(&mut self, outcome: RunOutcome) -> Result<Action, SamplingError> {
        let at = self.clock.now_ms();
        let mut next = self.engine.clone();
        let action = next.record(outcome);
        let event = match next.events().last().copied() {
            Some(event) => event,
            None => return Err(SamplingError::LedgerCorrupt("record 未留下谱系事件".into())),
        };
        if event.action != action {
            return Err(SamplingError::LedgerCorrupt("谱系事件与处置不一致".into()));
        }
        self.persist_promotion(&next, outcome, event, at)?;
        self.engine = next;
        Ok(action)
    }

    /// 放行点对账。真实件忽略。金标件追加一条注入记录。
    ///
    /// 同一 `seq` 再写会被拒绝，计数保持原样。
    pub fn observe(&mut self, item: &Tagged, verdict: &Verdict) -> Result<(), SamplingError> {
        let Tagged::Golden { seq, defective } = *item else {
            return Ok(());
        };
        let seq_i = i64::try_from(seq)
            .map_err(|_| SamplingError::Rejected(format!("金标序号 {seq} 超出 sqlite 整数范围")))?;
        let at = self.clock.now_ms();
        let caught = verdict.defect_caught;
        let disposition = GoldenDisposition::from_flags(defective, caught);
        let mut next = self.golden.clone();
        next.observe(item, verdict);
        let tx = self.conn.transaction()?;
        match tx.execute(
            "INSERT INTO golden_injection (
                seq, defective, defect_caught, disposition, observed_at_ms, root_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, 1)",
            params![
                seq_i,
                bool_i(defective),
                bool_i(caught),
                disposition.as_str(),
                at
            ],
        ) {
            Ok(1) => {}
            Ok(n) => {
                return Err(SamplingError::LedgerCorrupt(format!(
                    "金标写入影响行数 {n}"
                )))
            }
            Err(err) if is_constraint(&err) => {
                return Err(SamplingError::Rejected(format!("金标序号 {seq} 已入账")))
            }
            Err(err) => return Err(err.into()),
        }
        tx.commit()?;
        self.golden = next;
        Ok(())
    }

    pub fn current(&self) -> Lineage {
        self.engine.current()
    }

    pub fn attained(&self) -> Lineage {
        self.engine.attained()
    }

    pub fn clean_streak(&self) -> u32 {
        self.engine.clean_streak()
    }

    pub fn events(&self) -> &[Event] {
        self.engine.events()
    }

    pub fn pending_window(&self) -> &[RunOutcome] {
        self.engine.pending_window()
    }

    pub fn recipe(&self) -> &Recipe {
        self.engine.recipe()
    }

    pub fn golden(&self) -> &GoldenLedger {
        &self.golden
    }

    /// 从库里读谱系轨迹（含注入时刻与当时的运行结果）。
    pub fn trajectory(&self) -> Result<Vec<PersistedEvent>, SamplingError> {
        let rows = load_event_rows(&self.conn)?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(row.into_persisted()?);
        }
        Ok(out)
    }

    /// 从库里读金标注入记录。
    pub fn golden_records(&self) -> Result<Vec<GoldenInjectionRecord>, SamplingError> {
        let rows = load_golden_rows(&self.conn)?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(row.to_record()?);
        }
        Ok(out)
    }

    /// 水位行上的绝对时刻。重入不会改写它。
    pub fn watermark_updated_at_ms(&self) -> Result<i64, SamplingError> {
        query_i64(
            &self.conn,
            "SELECT updated_at_ms FROM watermark WHERE id = 1",
            "水位行缺失",
        )
    }

    fn persist_promotion(
        &mut self,
        next: &Engine,
        outcome: RunOutcome,
        event: Event,
        at: i64,
    ) -> Result<(), SamplingError> {
        let tick = i64::try_from(event.tick).map_err(|_| {
            SamplingError::Rejected(format!("谱系序号 {} 超出 sqlite 整数范围", event.tick))
        })?;
        let (kind, level) = action_parts(event.action, next.current());
        let tx = self.conn.transaction()?;
        let inserted = tx.execute(
            "INSERT INTO lineage_event (
                tick, action, current_after, success, violations, consistency_bits,
                recorded_at_ms, root_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
            params![
                tick,
                kind,
                level.name(),
                bool_i(outcome.success),
                i64::from(outcome.violations),
                f64_to_i64_bits(outcome.consistency),
                at
            ],
        )?;
        if inserted != 1 {
            return Err(SamplingError::LedgerCorrupt(format!(
                "谱系写入影响行数 {inserted}"
            )));
        }
        tx.execute("DELETE FROM window_slot", [])?;
        for (pos, slot) in next.pending_window().iter().enumerate() {
            let pos_i = i64::try_from(pos).map_err(|_| {
                SamplingError::LedgerCorrupt(format!("窗口下标 {pos} 超出 sqlite 整数范围"))
            })?;
            tx.execute(
                "INSERT INTO window_slot (
                    pos, success, violations, consistency_bits, root_id
                 ) VALUES (?1, ?2, ?3, ?4, 1)",
                params![
                    pos_i,
                    bool_i(slot.success),
                    i64::from(slot.violations),
                    f64_to_i64_bits(slot.consistency)
                ],
            )?;
        }
        let updated = tx.execute(
            "UPDATE watermark
             SET current = ?1, attained = ?2, clean_streak = ?3, updated_at_ms = ?4
             WHERE id = 1",
            params![
                next.current().name(),
                next.attained().name(),
                i64::from(next.clean_streak()),
                at
            ],
        )?;
        if updated != 1 {
            return Err(SamplingError::LedgerCorrupt("水位行缺失".into()));
        }
        tx.commit()?;
        Ok(())
    }

    fn load_verified(&mut self) -> Result<(), SamplingError> {
        let canonical = stored_recipe(&self.conn)?;
        let mut engine = Engine::from_recipe_json(&canonical)
            .map_err(|err| SamplingError::LedgerCorrupt(format!("库内配方无法复原: {err}")))?;
        replay_events(&self.conn, &mut engine)?;
        assert_watermark(&self.conn, &engine)?;
        assert_window(&self.conn, &engine)?;
        if engine.current() > engine.attained() {
            return Err(SamplingError::LedgerCorrupt(
                "重放后 current 超过 attained".into(),
            ));
        }
        let golden = load_golden(&self.conn)?;
        self.engine = engine;
        self.golden = golden;
        Ok(())
    }

    #[cfg(test)]
    fn conn(&self) -> &Connection {
        &self.conn
    }
}

fn prepare_declaration(decl: &LedgerDeclaration) -> Result<PreparedDecl, SamplingError> {
    decl.injector.validate()?;
    let recipe: Recipe = serde_json::from_str(&decl.recipe_json)?;
    recipe.validate()?;
    let canonical = serde_json::to_string(&recipe)?;
    Ok(PreparedDecl {
        canonical,
        p_bits: f64_to_i64_bits(decl.injector.p),
        d_bits: f64_to_i64_bits(decl.injector.defect_share),
    })
}

fn connection_pragmas(conn: &Connection) -> Result<(), SamplingError> {
    conn.busy_timeout(BUSY_TIMEOUT)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

fn assert_connection_recipe(conn: &Connection) -> Result<(), SamplingError> {
    let fk: i64 = conn.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    if fk != 1 {
        return Err(SamplingError::LedgerCorrupt(
            "PRAGMA foreign_keys 未能打开".into(),
        ));
    }
    let mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    if !journal_mode_is_usable(&mode) {
        return Err(SamplingError::LedgerCorrupt(format!(
            "journal_mode 不是 WAL: {mode}"
        )));
    }
    let sync: i64 = conn.pragma_query_value(None, "synchronous", |row| row.get(0))?;
    if sync != 1 {
        return Err(SamplingError::LedgerCorrupt(format!(
            "synchronous 不是 NORMAL: {sync}"
        )));
    }
    let busy: i64 = conn.pragma_query_value(None, "busy_timeout", |row| row.get(0))?;
    if busy != 5000 {
        return Err(SamplingError::LedgerCorrupt(format!(
            "busy_timeout 不是 5000ms: {busy}"
        )));
    }
    Ok(())
}

fn load_declared(
    conn: &mut Connection,
    prepared: &PreparedDecl,
) -> Result<OpenFork, SamplingError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version = user_version(&tx)?;
    if version != SCHEMA_VERSION {
        return Err(SamplingError::DeclarationMismatch(format!(
            "user_version {version} 不是抽样账本 schema {SCHEMA_VERSION}"
        )));
    }
    let tables = list_tables(&tx)?;
    check_tables(&tables)?;
    check_declaration(&tx, prepared)?;
    tx.rollback()?;
    Ok(OpenFork::AlreadyDeclared)
}

fn bootstrap(
    conn: &mut Connection,
    prepared: &PreparedDecl,
    now_ms: i64,
) -> Result<OpenFork, SamplingError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.pragma_update(None, "foreign_keys", "ON")?;
    tx.pragma_update(None, "synchronous", "NORMAL")?;
    let version = user_version(&tx)?;
    if version == SCHEMA_VERSION {
        let tables = list_tables(&tx)?;
        check_tables(&tables)?;
        check_declaration(&tx, prepared)?;
        tx.rollback()?;
        return Ok(OpenFork::AlreadyDeclared);
    }
    if version != 0 {
        return Err(SamplingError::DeclarationMismatch(format!(
            "user_version {version} 不是抽样账本 schema {SCHEMA_VERSION}"
        )));
    }
    let tables = list_tables(&tx)?;
    let fork = if tables.is_empty() {
        tx.execute_batch(SCHEMA_SQL)?;
        insert_initial(&tx, prepared, now_ms)?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        OpenFork::Created
    } else {
        // 旧库没有写 user_version。表和声明一致就重入，不改水位、不补版本。
        check_tables(&tables)?;
        check_declaration(&tx, prepared)?;
        OpenFork::AlreadyDeclared
    };
    tx.commit()?;
    Ok(fork)
}

/// 注入的首开退避。车道号让同时失败的线程错开醒来，不读业务时钟。
fn cold_start_pause(attempt: u32) {
    static LANES: AtomicU64 = AtomicU64::new(0);
    let lane = LANES.fetch_add(1, Ordering::Relaxed) % 8;
    let millis = u64::from(attempt.saturating_add(1)).saturating_add(lane);
    std::thread::sleep(Duration::from_millis(millis));
}

fn cold_start_conflict(err: &SamplingError) -> bool {
    let text = err.to_string().to_ascii_lowercase();
    text.contains("database is locked")
        || text.contains("database is busy")
        || text.contains("database table is locked")
        || text.contains("already exists")
}

fn journal_mode_is_usable(mode: &str) -> bool {
    let mode = mode.to_ascii_lowercase();
    mode == "wal" || mode == "memory"
}

fn ensure_journal_mode(conn: &Connection) -> Result<(), SamplingError> {
    let mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    if journal_mode_is_usable(&mode) {
        return Ok(());
    }
    let mode: String =
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
    if journal_mode_is_usable(&mode) {
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

fn user_version(conn: &Connection) -> Result<i64, SamplingError> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

fn insert_initial(
    conn: &Connection,
    prepared: &PreparedDecl,
    now_ms: i64,
) -> Result<(), SamplingError> {
    conn.execute("INSERT INTO ledger_root (id) VALUES (1)", [])?;
    conn.execute(
        "INSERT INTO sampling_decl (
            id, schema_version, recipe_json, injector_p_bits, injector_defect_share_bits, root_id
         ) VALUES (1, ?1, ?2, ?3, ?4, 1)",
        params![
            SCHEMA_VERSION,
            prepared.canonical,
            prepared.p_bits,
            prepared.d_bits
        ],
    )?;
    conn.execute(
        "INSERT INTO watermark (id, current, attained, clean_streak, updated_at_ms, root_id)
         VALUES (1, 'agent', 'agent', 0, ?1, 1)",
        params![now_ms],
    )?;
    Ok(())
}

fn list_tables(conn: &Connection) -> Result<Vec<String>, SamplingError> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
         ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut names = Vec::new();
    for row in rows {
        names.push(row?);
    }
    Ok(names)
}

fn check_tables(tables: &[String]) -> Result<(), SamplingError> {
    let got: Vec<&str> = tables.iter().map(String::as_str).collect();
    if got.as_slice() != EXPECTED_TABLES {
        return Err(SamplingError::DeclarationMismatch(format!(
            "库内表与抽样账本 schema {SCHEMA_VERSION} 不符: {got:?}"
        )));
    }
    Ok(())
}

fn check_declaration(conn: &Connection, prepared: &PreparedDecl) -> Result<(), SamplingError> {
    let (version, recipe, p_bits, d_bits) = conn
        .query_row(
            "SELECT schema_version, recipe_json, injector_p_bits, injector_defect_share_bits
             FROM sampling_decl WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .map_err(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => {
                SamplingError::LedgerCorrupt("声明行缺失".into())
            }
            other => SamplingError::Sqlite(other),
        })?;
    if version != SCHEMA_VERSION {
        return Err(SamplingError::DeclarationMismatch(format!(
            "schema 版本不符: 库内 {version}，本进程 {SCHEMA_VERSION}"
        )));
    }
    if recipe != prepared.canonical {
        return Err(SamplingError::DeclarationMismatch(
            "升迁配方与已声明配方不一致".into(),
        ));
    }
    if p_bits != prepared.p_bits {
        return Err(SamplingError::DeclarationMismatch(
            "金标注入率 p 与已声明不一致".into(),
        ));
    }
    if d_bits != prepared.d_bits {
        return Err(SamplingError::DeclarationMismatch(
            "金标缺陷占比 defect_share 与已声明不一致".into(),
        ));
    }
    Ok(())
}

fn stored_recipe(conn: &Connection) -> Result<String, SamplingError> {
    conn.query_row(
        "SELECT recipe_json FROM sampling_decl WHERE id = 1",
        [],
        |row| row.get(0),
    )
    .map_err(|err| match err {
        rusqlite::Error::QueryReturnedNoRows => SamplingError::LedgerCorrupt("声明行缺失".into()),
        other => SamplingError::Sqlite(other),
    })
}

fn replay_events(conn: &Connection, engine: &mut Engine) -> Result<(), SamplingError> {
    let rows = load_event_rows(conn)?;
    for (idx, row) in rows.iter().enumerate() {
        let expect_tick = i64::try_from(idx + 1).unwrap_or(i64::MAX);
        if row.tick != expect_tick {
            return Err(SamplingError::LedgerCorrupt(format!(
                "谱系轨迹序号不连续: 第 {idx} 行 tick={}",
                row.tick
            )));
        }
        let outcome = row.outcome()?;
        let action = engine.record(outcome);
        let (kind, level) = action_parts(action, engine.current());
        if row.action != kind || row.current_after != level.name() {
            return Err(SamplingError::LedgerCorrupt(format!(
                "tick {} 的落盘处置 {}:{} 与重放 {:?}/{} 不一致",
                row.tick,
                row.action,
                row.current_after,
                action,
                level.name()
            )));
        }
    }
    Ok(())
}

fn assert_watermark(conn: &Connection, engine: &Engine) -> Result<(), SamplingError> {
    let (current, attained, streak) = conn
        .query_row(
            "SELECT current, attained, clean_streak FROM watermark WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .map_err(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => {
                SamplingError::LedgerCorrupt("水位行缺失".into())
            }
            other => SamplingError::Sqlite(other),
        })?;
    let current_level = Lineage::from_name(&current)
        .ok_or_else(|| SamplingError::LedgerCorrupt(format!("水位 current 无法识别: {current}")))?;
    let attained_level = Lineage::from_name(&attained).ok_or_else(|| {
        SamplingError::LedgerCorrupt(format!("水位 attained 无法识别: {attained}"))
    })?;
    let streak_u = u32::try_from(streak)
        .map_err(|_| SamplingError::LedgerCorrupt(format!("clean_streak 超出范围: {streak}")))?;
    if current_level != engine.current()
        || attained_level != engine.attained()
        || streak_u != engine.clean_streak()
    {
        return Err(SamplingError::LedgerCorrupt(format!(
            "水位与轨迹重放不一致: 库内 {current}/{attained} streak={streak_u}，重放 {}/{} streak={}",
            engine.current().name(),
            engine.attained().name(),
            engine.clean_streak()
        )));
    }
    Ok(())
}

fn assert_window(conn: &Connection, engine: &Engine) -> Result<(), SamplingError> {
    let rows = load_window_rows(conn)?;
    let pending = engine.pending_window();
    if rows.len() != pending.len() {
        return Err(SamplingError::LedgerCorrupt(format!(
            "窗口长度不一致: 库内 {}，重放 {}",
            rows.len(),
            pending.len()
        )));
    }
    for (idx, (row, outcome)) in rows.iter().zip(pending.iter()).enumerate() {
        let expect_pos = i64::try_from(idx).unwrap_or(i64::MAX);
        if row.pos != expect_pos {
            return Err(SamplingError::LedgerCorrupt(format!(
                "窗口下标不连续: 第 {idx} 行 pos={}",
                row.pos
            )));
        }
        let success = sql_bool(row.success)?;
        let violations = sql_u32(row.violations)?;
        if success != outcome.success
            || violations != outcome.violations
            || row.consistency_bits != f64_to_i64_bits(outcome.consistency)
        {
            return Err(SamplingError::LedgerCorrupt(format!(
                "窗口第 {idx} 槽与重放不一致"
            )));
        }
    }
    Ok(())
}

fn load_golden(conn: &Connection) -> Result<GoldenLedger, SamplingError> {
    let rows = load_golden_rows(conn)?;
    let mut ledger = GoldenLedger::default();
    let mut intercepted = 0u64;
    let mut released = 0u64;
    let mut false_rejects = 0u64;
    let mut clean_passed = 0u64;
    for row in &rows {
        let record = row.to_record()?;
        match record.disposition {
            GoldenDisposition::Intercepted => intercepted += 1,
            GoldenDisposition::Released => released += 1,
            GoldenDisposition::FalseReject => false_rejects += 1,
            GoldenDisposition::CleanPassed => clean_passed += 1,
        }
        let item = Tagged::Golden {
            seq: record.seq,
            defective: record.defective,
        };
        // observe 只看缺陷标记和 defect_caught，相位不参与台账。
        let verdict = Verdict {
            inspected: record.defect_caught,
            defect_caught: record.defect_caught,
            leaked_defect: record.defective && !record.defect_caught,
            phase_after: Phase::Screening,
        };
        ledger.observe(&item, &verdict);
    }
    if ledger.alarms() != released
        || ledger.defective_released() != released
        || ledger.defective_intercepted() != intercepted
        || ledger.false_rejects() != false_rejects
        || ledger.clean_passed() != clean_passed
    {
        return Err(SamplingError::LedgerCorrupt(
            "金标计数与注入记录不一致".into(),
        ));
    }
    Ok(ledger)
}

fn load_event_rows(conn: &Connection) -> Result<Vec<EventRow>, SamplingError> {
    let mut stmt = conn.prepare(
        "SELECT tick, action, current_after, success, violations, consistency_bits, recorded_at_ms
         FROM lineage_event ORDER BY tick",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(EventRow {
            tick: row.get(0)?,
            action: row.get(1)?,
            current_after: row.get(2)?,
            success: row.get(3)?,
            violations: row.get(4)?,
            consistency_bits: row.get(5)?,
            recorded_at_ms: row.get(6)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn load_window_rows(conn: &Connection) -> Result<Vec<WindowRow>, SamplingError> {
    let mut stmt = conn.prepare(
        "SELECT pos, success, violations, consistency_bits
         FROM window_slot ORDER BY pos",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(WindowRow {
            pos: row.get(0)?,
            success: row.get(1)?,
            violations: row.get(2)?,
            consistency_bits: row.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn load_golden_rows(conn: &Connection) -> Result<Vec<GoldenRow>, SamplingError> {
    let mut stmt = conn.prepare(
        "SELECT seq, defective, defect_caught, disposition, observed_at_ms
         FROM golden_injection ORDER BY seq",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(GoldenRow {
            seq: row.get(0)?,
            defective: row.get(1)?,
            defect_caught: row.get(2)?,
            disposition: row.get(3)?,
            observed_at_ms: row.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

impl EventRow {
    fn outcome(&self) -> Result<RunOutcome, SamplingError> {
        Ok(RunOutcome {
            success: sql_bool(self.success)?,
            violations: sql_u32(self.violations)?,
            consistency: f64_from_i64_bits(self.consistency_bits),
        })
    }

    fn into_persisted(self) -> Result<PersistedEvent, SamplingError> {
        let tick = u64::try_from(self.tick)
            .map_err(|_| SamplingError::LedgerCorrupt(format!("谱系序号 {} 为负", self.tick)))?;
        let current_after = Lineage::from_name(&self.current_after).ok_or_else(|| {
            SamplingError::LedgerCorrupt(format!("轨迹谱系无法识别: {}", self.current_after))
        })?;
        let outcome = self.outcome()?;
        let action = parse_action(&self.action, current_after)?;
        Ok(PersistedEvent {
            tick,
            action,
            current_after,
            outcome,
            recorded_at_ms: self.recorded_at_ms,
        })
    }
}

impl GoldenRow {
    fn to_record(&self) -> Result<GoldenInjectionRecord, SamplingError> {
        let seq = u64::try_from(self.seq)
            .map_err(|_| SamplingError::LedgerCorrupt(format!("金标序号 {} 为负", self.seq)))?;
        let defective = sql_bool(self.defective)?;
        let defect_caught = sql_bool(self.defect_caught)?;
        let disposition = GoldenDisposition::parse(&self.disposition).ok_or_else(|| {
            SamplingError::LedgerCorrupt(format!("未知金标处置 {}", self.disposition))
        })?;
        let expected = GoldenDisposition::from_flags(defective, defect_caught);
        if disposition != expected {
            return Err(SamplingError::LedgerCorrupt(format!(
                "金标序号 {seq} 的处置 {} 与缺陷标记不符",
                self.disposition
            )));
        }
        Ok(GoldenInjectionRecord {
            seq,
            defective,
            defect_caught,
            disposition,
            observed_at_ms: self.observed_at_ms,
        })
    }
}

fn parse_action(kind: &str, level: Lineage) -> Result<Action, SamplingError> {
    match kind {
        "hold" => Ok(Action::Hold),
        "promoted" => Ok(Action::Promoted(level)),
        "demoted" => Ok(Action::Demoted(level)),
        "recovered" => Ok(Action::Recovered(level)),
        other => Err(SamplingError::LedgerCorrupt(format!("未知处置 {other}"))),
    }
}

fn action_parts(action: Action, current_after: Lineage) -> (&'static str, Lineage) {
    match action {
        Action::Hold => ("hold", current_after),
        Action::Promoted(level) => ("promoted", level),
        Action::Demoted(level) => ("demoted", level),
        Action::Recovered(level) => ("recovered", level),
    }
}

fn query_i64(conn: &Connection, sql: &str, missing: &str) -> Result<i64, SamplingError> {
    conn.query_row(sql, [], |row| row.get(0))
        .map_err(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => {
                SamplingError::LedgerCorrupt(missing.to_string())
            }
            other => SamplingError::Sqlite(other),
        })
}

fn sql_bool(value: i64) -> Result<bool, SamplingError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(SamplingError::LedgerCorrupt(format!("布尔列值 {other}"))),
    }
}

fn sql_u32(value: i64) -> Result<u32, SamplingError> {
    u32::try_from(value)
        .map_err(|_| SamplingError::LedgerCorrupt(format!("计数值超出范围: {value}")))
}

fn bool_i(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

/// 位型转写，保留 NaN 与符号位。不是数值截断。
fn f64_to_i64_bits(value: f64) -> i64 {
    value.to_bits() as i64
}

fn f64_from_i64_bits(bits: i64) -> f64 {
    f64::from_bits(bits as u64)
}

fn is_constraint(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{ManualClock, SystemClock};
    use crate::csp1::{Phase, Verdict};
    use crate::golden::InjectorConfig;
    use crate::promotion::{Action, Engine, Lineage, RunOutcome};
    use crate::SamplingError;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Barrier};

    const RECIPE: &str = r#"{
        "window": 5,
        "min_successes": 5,
        "max_violations": 0,
        "min_consistency": 0.98,
        "clean_to_recover": 3
    }"#;

    fn decl_with(recipe_json: &str, p: f64, defect_share: f64) -> LedgerDeclaration {
        LedgerDeclaration {
            recipe_json: recipe_json.to_string(),
            injector: InjectorConfig { p, defect_share },
        }
    }

    fn ok_decl() -> LedgerDeclaration {
        decl_with(RECIPE, 0.04, 0.7)
    }

    fn clean() -> RunOutcome {
        RunOutcome {
            success: true,
            violations: 0,
            consistency: 1.0,
        }
    }

    fn fail() -> RunOutcome {
        RunOutcome {
            success: false,
            violations: 0,
            consistency: 0.0,
        }
    }

    fn verdict(defect_caught: bool) -> Verdict {
        Verdict {
            inspected: defect_caught,
            defect_caught,
            leaked_defect: !defect_caught,
            phase_after: Phase::Screening,
        }
    }

    struct TempDb {
        path: PathBuf,
    }

    impl TempDb {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("af-sampling-ledger-{}-{n}.db", std::process::id()));
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
            let mut wal = self.path.clone().into_os_string();
            wal.push("-wal");
            let _ = std::fs::remove_file(wal);
            let mut shm = self.path.clone().into_os_string();
            shm.push("-shm");
            let _ = std::fs::remove_file(shm);
        }
    }

    fn open_at(
        path: &Path,
        decl: &LedgerDeclaration,
        now_ms: i64,
    ) -> Result<(SamplingLedger, OpenFork), SamplingError> {
        SamplingLedger::open(path, decl, ManualClock::new(now_ms))
    }

    #[test]
    fn invalid_declaration_does_not_create_a_file() {
        let tmp = TempDb::new();
        let bad_json = decl_with("{", 0.1, 0.7);
        let err = open_at(tmp.path(), &bad_json, 1).unwrap_err();
        assert!(matches!(err, SamplingError::RecipeJson(_)), "{err}");
        assert!(!tmp.path().exists());

        let bad_p = decl_with(RECIPE, 1.5, 0.7);
        let err = open_at(tmp.path(), &bad_p, 1).unwrap_err();
        assert!(
            matches!(err, SamplingError::InvalidInjectorConfig(_)),
            "{err}"
        );
        assert!(!tmp.path().exists());

        let bad_window = decl_with(
            r#"{"window":0,"min_successes":0,"max_violations":0,"min_consistency":0.5,"clean_to_recover":1}"#,
            0.1,
            0.7,
        );
        let err = open_at(tmp.path(), &bad_window, 1).unwrap_err();
        assert!(matches!(err, SamplingError::InvalidRecipe(_)), "{err}");
        assert!(!tmp.path().exists());
    }

    #[test]
    fn first_open_is_created_and_reentry_is_already_declared() {
        let tmp = TempDb::new();
        let pretty = ok_decl();
        let (ledger, fork) = open_at(tmp.path(), &pretty, 1_234).unwrap();
        assert_eq!(fork, OpenFork::Created);
        assert_eq!(ledger.current(), Lineage::Agent);
        assert_eq!(ledger.attained(), Lineage::Agent);
        assert_eq!(ledger.clean_streak(), 0);
        assert!(ledger.events().is_empty());
        assert!(ledger.pending_window().is_empty());
        assert!(ledger.trajectory().unwrap().is_empty());
        assert!(ledger.golden_records().unwrap().is_empty());
        assert_eq!(ledger.golden().golden_seen(), 0);
        assert_eq!(ledger.watermark_updated_at_ms().unwrap(), 1_234);
        assert_eq!(SCHEMA_VERSION, 1);
        drop(ledger);

        let compact = decl_with(
            r#"{"window":5,"min_successes":5,"max_violations":0,"min_consistency":0.98,"clean_to_recover":3}"#,
            0.04,
            0.7,
        );
        let (ledger, fork) = open_at(tmp.path(), &compact, 9_999).unwrap();
        assert_eq!(fork, OpenFork::AlreadyDeclared);
        assert_eq!(ledger.current(), Lineage::Agent);
        assert_eq!(
            ledger.watermark_updated_at_ms().unwrap(),
            1_234,
            "重入不得改写水位时刻"
        );
    }

    #[test]
    fn file_pragmas_and_foreign_key_are_enforced() {
        let tmp = TempDb::new();
        let (ledger, _) = open_at(tmp.path(), &ok_decl(), 1).unwrap();
        let mode: String = ledger
            .conn()
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        let sync: i64 = ledger
            .conn()
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .unwrap();
        assert_eq!(sync, 1);
        let fk: i64 = ledger
            .conn()
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        let busy: i64 = ledger
            .conn()
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        assert_eq!(busy, 5000);
        let err = ledger
            .conn()
            .execute(
                "INSERT INTO golden_injection (
                    seq, defective, defect_caught, disposition, observed_at_ms, root_id
                 ) VALUES (1, 1, 1, 'intercepted', 0, 99)",
                [],
            )
            .unwrap_err();
        assert!(is_constraint(&err), "{err}");
    }

    #[test]
    fn memory_open_uses_foreign_keys_and_normal_sync() {
        let (ledger, fork) =
            SamplingLedger::open_in_memory(&ok_decl(), SystemClock::new()).unwrap();
        assert_eq!(fork, OpenFork::Created);
        let mode: String = ledger
            .conn()
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "memory");
        let sync: i64 = ledger
            .conn()
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .unwrap();
        assert_eq!(sync, 1);
        let fk: i64 = ledger
            .conn()
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert_eq!(fk, 1);
    }

    #[test]
    fn declaration_mismatch_fails_closed_and_keeps_prior_state() {
        let tmp = TempDb::new();
        let decl = ok_decl();
        let (mut ledger, _) = open_at(tmp.path(), &decl, 10).unwrap();
        assert_eq!(ledger.record(clean()).unwrap(), Action::Hold);
        drop(ledger);

        let cases = [
            decl_with(
                r#"{"window":4,"min_successes":4,"max_violations":0,"min_consistency":0.98,"clean_to_recover":3}"#,
                0.04,
                0.7,
            ),
            decl_with(RECIPE, 0.2, 0.7),
            decl_with(RECIPE, 0.04, 0.3),
        ];
        for bad in cases {
            let err = open_at(tmp.path(), &bad, 11).unwrap_err();
            assert!(
                matches!(err, SamplingError::DeclarationMismatch(_)),
                "{err}"
            );
        }

        let (ledger, fork) = open_at(tmp.path(), &decl, 12).unwrap();
        assert_eq!(fork, OpenFork::AlreadyDeclared);
        assert_eq!(ledger.events().len(), 1);
        assert_eq!(ledger.clean_streak(), 1);
        assert_eq!(ledger.pending_window().len(), 1);
    }

    #[test]
    fn schema_version_mismatch_fails_closed() {
        let tmp = TempDb::new();
        let decl = ok_decl();
        let (ledger, _) = open_at(tmp.path(), &decl, 10).unwrap();
        ledger
            .conn()
            .execute(
                "UPDATE sampling_decl SET schema_version = 99 WHERE id = 1",
                [],
            )
            .unwrap();
        drop(ledger);
        let err = open_at(tmp.path(), &decl, 11).unwrap_err();
        assert!(
            matches!(err, SamplingError::DeclarationMismatch(_)),
            "{err}"
        );
    }

    #[test]
    fn foreign_table_is_not_this_ledger() {
        let tmp = TempDb::new();
        let conn = Connection::open(tmp.path()).unwrap();
        conn.execute_batch("CREATE TABLE other (id INTEGER);")
            .unwrap();
        drop(conn);
        let err = open_at(tmp.path(), &ok_decl(), 1).unwrap_err();
        assert!(
            matches!(err, SamplingError::DeclarationMismatch(_)),
            "{err}"
        );
    }

    #[test]
    fn non_sqlite_file_fails_closed() {
        let tmp = TempDb::new();
        std::fs::write(tmp.path(), b"this is not sqlite").unwrap();
        let err = open_at(tmp.path(), &ok_decl(), 1).unwrap_err();
        assert!(matches!(err, SamplingError::Sqlite(_)), "{err}");
    }

    #[test]
    fn tampered_watermark_or_window_refuses_startup() {
        let tmp = TempDb::new();
        let decl = ok_decl();
        let (mut ledger, _) = open_at(tmp.path(), &decl, 10).unwrap();
        ledger.record(clean()).unwrap();
        ledger.record(clean()).unwrap();
        ledger
            .conn()
            .execute("UPDATE watermark SET clean_streak = 99 WHERE id = 1", [])
            .unwrap();
        drop(ledger);
        let err = open_at(tmp.path(), &decl, 11).unwrap_err();
        assert!(matches!(err, SamplingError::LedgerCorrupt(_)), "{err}");

        let tmp = TempDb::new();
        let (mut ledger, _) = open_at(tmp.path(), &decl, 10).unwrap();
        ledger.record(clean()).unwrap();
        ledger.record(clean()).unwrap();
        ledger
            .conn()
            .execute("DELETE FROM window_slot", [])
            .unwrap();
        drop(ledger);
        let err = open_at(tmp.path(), &decl, 11).unwrap_err();
        assert!(matches!(err, SamplingError::LedgerCorrupt(_)), "{err}");
    }

    #[test]
    fn tampered_trajectory_action_refuses_startup() {
        let tmp = TempDb::new();
        let decl = ok_decl();
        let (mut ledger, _) = open_at(tmp.path(), &decl, 10).unwrap();
        assert_eq!(ledger.record(clean()).unwrap(), Action::Hold);
        ledger
            .conn()
            .execute(
                "UPDATE lineage_event SET action = 'promoted', current_after = 'hybrid' WHERE tick = 1",
                [],
            )
            .unwrap();
        drop(ledger);
        let err = open_at(tmp.path(), &decl, 11).unwrap_err();
        assert!(matches!(err, SamplingError::LedgerCorrupt(_)), "{err}");
    }

    #[test]
    fn restart_restores_promotion_and_continues_like_the_live_engine() {
        let tmp = TempDb::new();
        let decl = ok_decl();
        let mut oracle = Engine::from_recipe_json(decl.recipe_json.as_str()).unwrap();
        let (mut ledger, fork) = open_at(tmp.path(), &decl, 5_000).unwrap();
        assert_eq!(fork, OpenFork::Created);

        // 8 步停在熔断后的窗口中段：current=agent，attained=hybrid，streak=2，窗口 2 条。
        let prefix = [
            clean(),
            clean(),
            clean(),
            clean(),
            clean(),
            fail(),
            clean(),
            clean(),
        ];
        for outcome in prefix {
            let expect = oracle.record(outcome);
            let got = ledger.record(outcome).unwrap();
            assert_eq!(got, expect);
        }
        assert_eq!(oracle.current(), Lineage::Agent);
        assert_eq!(oracle.attained(), Lineage::Hybrid);
        assert_eq!(oracle.clean_streak(), 2);
        assert_eq!(oracle.pending_window().len(), 2);
        drop(ledger);

        let (mut ledger, fork) = open_at(tmp.path(), &decl, 8_000).unwrap();
        assert_eq!(fork, OpenFork::AlreadyDeclared);
        assert_eq!(ledger.current(), oracle.current());
        assert_eq!(ledger.attained(), oracle.attained());
        assert_eq!(ledger.clean_streak(), oracle.clean_streak());
        assert_eq!(ledger.pending_window(), oracle.pending_window());
        assert_eq!(ledger.events(), oracle.events());
        let traj = ledger.trajectory().unwrap();
        assert_eq!(traj.len(), 8);
        assert!(traj.iter().all(|event| event.recorded_at_ms == 5_000));
        assert_eq!(ledger.watermark_updated_at_ms().unwrap(), 5_000);

        // 连净第 3 次必须回升。窗口若在重启时丢了，这一步仍可能回升，但后面的晋升会错。
        let recovered = clean();
        assert_eq!(
            ledger.record(recovered).unwrap(),
            Action::Recovered(Lineage::Hybrid)
        );
        assert_eq!(oracle.record(recovered), Action::Recovered(Lineage::Hybrid));
        assert_eq!(ledger.pending_window().len(), 3, "回升不清窗口");
        assert_eq!(ledger.watermark_updated_at_ms().unwrap(), 8_000);

        let hold = clean();
        assert_eq!(ledger.record(hold).unwrap(), Action::Hold);
        assert_eq!(oracle.record(hold), Action::Hold);
        let promoted = clean();
        assert_eq!(
            ledger.record(promoted).unwrap(),
            Action::Promoted(Lineage::Deterministic)
        );
        assert_eq!(
            oracle.record(promoted),
            Action::Promoted(Lineage::Deterministic)
        );
        assert_eq!(ledger.current(), oracle.current());
        assert_eq!(ledger.attained(), oracle.attained());
        assert_eq!(ledger.clean_streak(), oracle.clean_streak());
        assert_eq!(ledger.pending_window(), oracle.pending_window());
        assert_eq!(ledger.events(), oracle.events());

        drop(ledger);
        let (ledger, _) = open_at(tmp.path(), &decl, 1).unwrap();
        assert_eq!(ledger.current(), Lineage::Deterministic);
        assert_eq!(ledger.attained(), Lineage::Deterministic);
        assert_eq!(ledger.events().len(), 11);
        let traj = ledger.trajectory().unwrap();
        assert_eq!(traj[7].recorded_at_ms, 5_000);
        assert_eq!(traj[8].recorded_at_ms, 8_000);
        assert_eq!(traj[8].action, Action::Recovered(Lineage::Hybrid));
        assert_eq!(traj[10].action, Action::Promoted(Lineage::Deterministic));
    }

    #[test]
    fn restart_keeps_full_window_left_by_recover() {
        let recipe = r#"{"window":2,"min_successes":2,"max_violations":0,"min_consistency":0.0,"clean_to_recover":2}"#;
        let decl = decl_with(recipe, 0.0, 0.7);
        let tmp = TempDb::new();
        let mut oracle = Engine::from_recipe_json(recipe).unwrap();
        let (mut ledger, _) = open_at(tmp.path(), &decl, 100).unwrap();
        for _ in 0..2 {
            let outcome = clean();
            assert_eq!(ledger.record(outcome).unwrap(), oracle.record(outcome));
        }
        assert_eq!(ledger.record(fail()).unwrap(), oracle.record(fail()));
        assert_eq!(ledger.record(clean()).unwrap(), oracle.record(clean()));
        drop(ledger);

        let (mut ledger, fork) = open_at(tmp.path(), &decl, 200).unwrap();
        assert_eq!(fork, OpenFork::AlreadyDeclared);
        assert_eq!(ledger.pending_window(), oracle.pending_window());
        let outcome = clean();
        assert_eq!(
            ledger.record(outcome).unwrap(),
            Action::Recovered(Lineage::Hybrid)
        );
        assert_eq!(oracle.record(outcome), Action::Recovered(Lineage::Hybrid));
        assert_eq!(ledger.pending_window().len(), 2);
        assert_eq!(ledger.pending_window(), oracle.pending_window());
        drop(ledger);

        let (ledger, _) = open_at(tmp.path(), &decl, 300).unwrap();
        assert_eq!(ledger.pending_window().len(), 2);
        assert_eq!(ledger.current(), Lineage::Hybrid);
        assert_eq!(ledger.clean_streak(), 0);
    }

    #[test]
    fn restart_restores_golden_records_and_rejects_duplicates() {
        let tmp = TempDb::new();
        let decl = ok_decl();
        let clock = ManualClock::new(4_000);
        let (mut ledger, _) = SamplingLedger::open(tmp.path(), &decl, clock.clone()).unwrap();
        let real = Tagged::Real {
            seq: 7,
            conforming: true,
        };
        ledger.observe(&real, &verdict(false)).unwrap();
        assert!(ledger.golden_records().unwrap().is_empty());

        let released = Tagged::Golden {
            seq: 1,
            defective: true,
        };
        ledger.observe(&released, &verdict(false)).unwrap();
        assert_eq!(ledger.golden().alarms(), 1);
        let err = ledger.observe(&released, &verdict(true)).unwrap_err();
        assert!(matches!(err, SamplingError::Rejected(_)), "{err}");
        assert_eq!(ledger.golden().alarms(), 1, "拒绝写入不得改计数");

        clock.set(4_500);
        let intercepted = Tagged::Golden {
            seq: 4,
            defective: true,
        };
        ledger.observe(&intercepted, &verdict(true)).unwrap();
        let clean_pass = Tagged::Golden {
            seq: 2,
            defective: false,
        };
        ledger.observe(&clean_pass, &verdict(false)).unwrap();
        let false_reject = Tagged::Golden {
            seq: 9,
            defective: false,
        };
        ledger.observe(&false_reject, &verdict(true)).unwrap();
        drop(ledger);

        let (mut ledger, fork) = open_at(tmp.path(), &decl, 7_000).unwrap();
        assert_eq!(fork, OpenFork::AlreadyDeclared);
        assert_eq!(ledger.golden().alarms(), 1);
        assert_eq!(ledger.golden().defective_released(), 1);
        assert_eq!(ledger.golden().defective_intercepted(), 1);
        assert_eq!(ledger.golden().clean_passed(), 1);
        assert_eq!(ledger.golden().false_rejects(), 1);
        assert_eq!(ledger.golden().golden_seen(), 4);
        let records = ledger.golden_records().unwrap();
        assert_eq!(
            records
                .iter()
                .map(|row| (row.seq, row.disposition, row.observed_at_ms))
                .collect::<Vec<_>>(),
            vec![
                (1, GoldenDisposition::Released, 4_000),
                (2, GoldenDisposition::CleanPassed, 4_500),
                (4, GoldenDisposition::Intercepted, 4_500),
                (9, GoldenDisposition::FalseReject, 4_500),
            ]
        );

        let again = Tagged::Golden {
            seq: 11,
            defective: true,
        };
        ledger.observe(&again, &verdict(false)).unwrap();
        assert_eq!(ledger.golden().alarms(), 2);
        drop(ledger);

        let (ledger, _) = open_at(tmp.path(), &decl, 1).unwrap();
        assert_eq!(ledger.golden().alarms(), 2);
        let records = ledger.golden_records().unwrap();
        assert_eq!(records.last().unwrap().seq, 11);
        assert_eq!(records.last().unwrap().observed_at_ms, 7_000);
        assert_eq!(records[0].observed_at_ms, 4_000);
    }

    #[test]
    fn outcome_bits_roundtrip_across_restart() {
        let tmp = TempDb::new();
        let decl = ok_decl();
        let (mut ledger, _) = open_at(tmp.path(), &decl, 42).unwrap();
        let outcome = RunOutcome {
            success: true,
            violations: 2,
            consistency: 0.98,
        };
        assert_eq!(ledger.record(outcome).unwrap(), Action::Hold);
        drop(ledger);
        let (ledger, _) = open_at(tmp.path(), &decl, 43).unwrap();
        let traj = ledger.trajectory().unwrap();
        assert_eq!(traj.len(), 1);
        assert_eq!(traj[0].outcome.violations, 2);
        assert_eq!(traj[0].outcome.consistency.to_bits(), 0.98f64.to_bits());
        assert_eq!(
            ledger.pending_window()[0].consistency.to_bits(),
            0.98f64.to_bits()
        );
        assert_eq!(traj[0].recorded_at_ms, 42);
    }

    #[test]
    fn seq_above_i64_is_rejected() {
        let (mut ledger, _) =
            SamplingLedger::open_in_memory(&ok_decl(), ManualClock::new(1)).unwrap();
        let item = Tagged::Golden {
            seq: u64::MAX,
            defective: true,
        };
        let err = ledger.observe(&item, &verdict(true)).unwrap_err();
        assert!(matches!(err, SamplingError::Rejected(_)), "{err}");
        assert_eq!(ledger.golden().golden_seen(), 0);
    }

    /// F4：八个线程同时第一次打开同一个空库。只允许一个 `Created`。
    #[test]
    fn concurrent_first_open_one_creator() {
        let tmp = TempDb::new();
        let decl = ok_decl();
        let threads = 8;
        let barrier = Arc::new(Barrier::new(threads));
        let results = std::thread::scope(|scope| {
            let mut guards = Vec::new();
            for _ in 0..threads {
                let path = tmp.path().to_path_buf();
                let decl = decl.clone();
                let barrier = Arc::clone(&barrier);
                guards.push(scope.spawn(move || {
                    barrier.wait();
                    open_at(&path, &decl, 50).map(|(_, fork)| fork)
                }));
            }
            guards
                .into_iter()
                .map(|guard| guard.join().expect("open thread"))
                .collect::<Vec<_>>()
        });
        let mut created = 0usize;
        let mut declared = 0usize;
        for result in &results {
            match result {
                Ok(OpenFork::Created) => created += 1,
                Ok(OpenFork::AlreadyDeclared) => declared += 1,
                Err(err) => panic!("cold start failed: {err}"),
            }
        }
        assert_eq!(created, 1, "{results:?}");
        assert_eq!(declared, threads - 1, "{results:?}");
        let (ledger, fork) = open_at(tmp.path(), &decl, 99).unwrap();
        assert_eq!(fork, OpenFork::AlreadyDeclared);
        assert_eq!(ledger.watermark_updated_at_ms().unwrap(), 50);
        let mode: String = ledger
            .conn()
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(mode.to_ascii_lowercase(), "wal");
    }
}
