//! Run 状态库。
//!
//! 首启（库里没有本执行器的对象）建表并写入 schema 版本与指纹，返回 [`OpenStatus::Created`]。
//! 重入时逐项对照：版本、指纹、`sqlite_master` 里的建表语句。任何一项对不上就失败，
//! 不补表、不改已有行、不把旧库解释成新库。
//!
//! 连接配方：WAL、`busy_timeout` 5000ms、`synchronous=NORMAL`、`foreign_keys=ON`。
//! 这些 PRAGMA 是连接态，每次 open 都重设，并读回核对。

use crate::error::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::path::Path;
use std::time::Duration;

/// 本次 open 看到的是新库还是已声明且一致的库。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenStatus {
    /// 首启：建表并写入声明。
    Created,
    /// 重入：已存声明与代码里的声明一致。
    AlreadyDeclared,
}

pub(crate) const SCHEMA_VERSION: i64 = 1;
pub(crate) const SCHEMA_FINGERPRINT: &str = "af-harness/run-schema-1";

const SQL_DECL: &str = "CREATE TABLE harness_decl (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema_version INTEGER NOT NULL,
    schema_fingerprint TEXT NOT NULL
)";

const SQL_RUN: &str = "CREATE TABLE run_row (
    run_id TEXT PRIMARY KEY,
    spec_json TEXT NOT NULL,
    vars_json TEXT NOT NULL,
    user_message TEXT NOT NULL,
    namespace TEXT NOT NULL,
    spend_cap_cents INTEGER NOT NULL,
    tool_spend_cents INTEGER NOT NULL,
    order_id TEXT NOT NULL,
    stop_policy_json TEXT NOT NULL,
    budget_json TEXT NOT NULL,
    turn_index INTEGER NOT NULL,
    tool_free_streak INTEGER NOT NULL,
    transcript_json TEXT NOT NULL,
    stop_state_json TEXT NOT NULL,
    model_calls INTEGER NOT NULL,
    effect_fresh INTEGER NOT NULL,
    effect_replays INTEGER NOT NULL,
    terminal_json TEXT,
    whitelist_json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL
)";

/// `(type, name, sql)`，顺序即创建顺序。
const OBJECTS: &[(&str, &str, &str)] = &[
    ("table", "harness_decl", SQL_DECL),
    ("table", "run_row", SQL_RUN),
];

/// 读回的连接配方。`synchronous = 1` 即 NORMAL。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Recipe {
    pub journal_mode: String,
    pub synchronous: i64,
    pub foreign_keys: i64,
    pub busy_timeout_ms: i64,
}

/// 一行 Run。声明列只在插入时写；提交只更新运行列。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunRecord {
    pub run_id: String,
    pub spec_json: String,
    pub vars_json: String,
    pub user_message: String,
    pub namespace: String,
    pub spend_cap_cents: i64,
    pub tool_spend_cents: i64,
    pub order_id: String,
    pub stop_policy_json: String,
    pub budget_json: String,
    pub turn_index: u32,
    pub tool_free_streak: u32,
    pub transcript_json: String,
    pub stop_state_json: String,
    pub model_calls: u64,
    pub effect_fresh: u64,
    pub effect_replays: u64,
    pub terminal_json: Option<String>,
    pub whitelist_json: String,
    pub updated_at_ms: u64,
}

pub(crate) struct RunStore {
    conn: Connection,
}

impl RunStore {
    pub(crate) fn open(path: &Path) -> Result<(Self, OpenStatus)> {
        let mut conn = Connection::open(path)?;
        let status = prepare(&mut conn)?;
        verify_recipe(&conn)?;
        Ok((Self { conn }, status))
    }

    #[cfg(test)]
    pub(crate) fn recipe(&self) -> Result<Recipe> {
        read_recipe(&self.conn)
    }

    pub(crate) fn load(&self, run_id: &str) -> Result<Option<RunRecord>> {
        let raw = self
            .conn
            .query_row(
                "SELECT run_id, spec_json, vars_json, user_message, namespace,
                        spend_cap_cents, tool_spend_cents, order_id, stop_policy_json, budget_json,
                        turn_index, tool_free_streak, transcript_json, stop_state_json, model_calls,
                        effect_fresh, effect_replays, terminal_json, whitelist_json, updated_at_ms
                 FROM run_row WHERE run_id = ?1",
                params![run_id],
                read_raw,
            )
            .optional()?;
        raw.map(RawRun::into_record).transpose()
    }

    pub(crate) fn insert(&self, row: &RunRecord) -> Result<()> {
        let nums = row.numbers()?;
        self.conn.execute(
            "INSERT INTO run_row (
                run_id, spec_json, vars_json, user_message, namespace,
                spend_cap_cents, tool_spend_cents, order_id, stop_policy_json, budget_json,
                turn_index, tool_free_streak, transcript_json, stop_state_json, model_calls,
                effect_fresh, effect_replays, terminal_json, whitelist_json, updated_at_ms
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20
             )",
            params![
                row.run_id,
                row.spec_json,
                row.vars_json,
                row.user_message,
                row.namespace,
                row.spend_cap_cents,
                row.tool_spend_cents,
                row.order_id,
                row.stop_policy_json,
                row.budget_json,
                nums.turn_index,
                nums.tool_free_streak,
                row.transcript_json,
                row.stop_state_json,
                nums.model_calls,
                nums.effect_fresh,
                nums.effect_replays,
                row.terminal_json,
                row.whitelist_json,
                nums.updated_at_ms,
            ],
        )?;
        Ok(())
    }

    /// 只改运行列。声明列（规格、变量、工单、白名单、花费上限）保持插入时的值。
    pub(crate) fn update_runtime(&self, row: &RunRecord) -> Result<()> {
        let nums = row.numbers()?;
        let changed = self.conn.execute(
            "UPDATE run_row SET
                budget_json = ?1,
                turn_index = ?2,
                tool_free_streak = ?3,
                transcript_json = ?4,
                stop_state_json = ?5,
                model_calls = ?6,
                effect_fresh = ?7,
                effect_replays = ?8,
                terminal_json = ?9,
                updated_at_ms = ?10
             WHERE run_id = ?11",
            params![
                row.budget_json,
                nums.turn_index,
                nums.tool_free_streak,
                row.transcript_json,
                row.stop_state_json,
                nums.model_calls,
                nums.effect_fresh,
                nums.effect_replays,
                row.terminal_json,
                nums.updated_at_ms,
                row.run_id,
            ],
        )?;
        if changed != 1 {
            return Err(Error::DeclarationMismatch(format!(
                "run {} 不在库里，拒绝更新",
                row.run_id
            )));
        }
        Ok(())
    }
}

struct SqlNumbers {
    turn_index: i64,
    tool_free_streak: i64,
    model_calls: i64,
    effect_fresh: i64,
    effect_replays: i64,
    updated_at_ms: i64,
}

impl RunRecord {
    fn numbers(&self) -> Result<SqlNumbers> {
        Ok(SqlNumbers {
            turn_index: i64::from(self.turn_index),
            tool_free_streak: i64::from(self.tool_free_streak),
            model_calls: fit_u64(self.model_calls, "model_calls")?,
            effect_fresh: fit_u64(self.effect_fresh, "effect_fresh")?,
            effect_replays: fit_u64(self.effect_replays, "effect_replays")?,
            updated_at_ms: fit_u64(self.updated_at_ms, "updated_at_ms")?,
        })
    }
}

fn fit_u64(value: u64, what: &str) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| Error::DeclarationMismatch(format!("{what} 超出 sqlite 有符号整数")))
}

struct RawRun {
    run_id: String,
    spec_json: String,
    vars_json: String,
    user_message: String,
    namespace: String,
    spend_cap_cents: i64,
    tool_spend_cents: i64,
    order_id: String,
    stop_policy_json: String,
    budget_json: String,
    turn_index: i64,
    tool_free_streak: i64,
    transcript_json: String,
    stop_state_json: String,
    model_calls: i64,
    effect_fresh: i64,
    effect_replays: i64,
    terminal_json: Option<String>,
    whitelist_json: String,
    updated_at_ms: i64,
}

fn read_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRun> {
    Ok(RawRun {
        run_id: row.get(0)?,
        spec_json: row.get(1)?,
        vars_json: row.get(2)?,
        user_message: row.get(3)?,
        namespace: row.get(4)?,
        spend_cap_cents: row.get(5)?,
        tool_spend_cents: row.get(6)?,
        order_id: row.get(7)?,
        stop_policy_json: row.get(8)?,
        budget_json: row.get(9)?,
        turn_index: row.get(10)?,
        tool_free_streak: row.get(11)?,
        transcript_json: row.get(12)?,
        stop_state_json: row.get(13)?,
        model_calls: row.get(14)?,
        effect_fresh: row.get(15)?,
        effect_replays: row.get(16)?,
        terminal_json: row.get(17)?,
        whitelist_json: row.get(18)?,
        updated_at_ms: row.get(19)?,
    })
}

impl RawRun {
    fn into_record(self) -> Result<RunRecord> {
        Ok(RunRecord {
            run_id: self.run_id,
            spec_json: self.spec_json,
            vars_json: self.vars_json,
            user_message: self.user_message,
            namespace: self.namespace,
            spend_cap_cents: self.spend_cap_cents,
            tool_spend_cents: self.tool_spend_cents,
            order_id: self.order_id,
            stop_policy_json: self.stop_policy_json,
            budget_json: self.budget_json,
            turn_index: fit_i64_u32(self.turn_index, "turn_index")?,
            tool_free_streak: fit_i64_u32(self.tool_free_streak, "tool_free_streak")?,
            transcript_json: self.transcript_json,
            stop_state_json: self.stop_state_json,
            model_calls: fit_i64_u64(self.model_calls, "model_calls")?,
            effect_fresh: fit_i64_u64(self.effect_fresh, "effect_fresh")?,
            effect_replays: fit_i64_u64(self.effect_replays, "effect_replays")?,
            terminal_json: self.terminal_json,
            whitelist_json: self.whitelist_json,
            updated_at_ms: fit_i64_u64(self.updated_at_ms, "updated_at_ms")?,
        })
    }
}

fn fit_i64_u32(value: i64, what: &str) -> Result<u32> {
    u32::try_from(value)
        .map_err(|_| Error::DeclarationMismatch(format!("{what} 不是非负 32 位整数")))
}

fn fit_i64_u64(value: i64, what: &str) -> Result<u64> {
    u64::try_from(value).map_err(|_| Error::DeclarationMismatch(format!("{what} 为负")))
}

fn prepare(conn: &mut Connection) -> Result<OpenStatus> {
    conn.busy_timeout(Duration::from_millis(5_000))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    if user_objects(conn)?.is_empty() {
        match create_schema(conn)? {
            CreateSchema::Created => {
                verify(conn)?;
                return Ok(OpenStatus::Created);
            }
            CreateSchema::AdoptedExisting => {
                verify(conn)?;
                return Ok(OpenStatus::AlreadyDeclared);
            }
        }
    }
    verify(conn)?;
    Ok(OpenStatus::AlreadyDeclared)
}

enum CreateSchema {
    Created,
    AdoptedExisting,
}

fn create_schema(conn: &mut Connection) -> Result<CreateSchema> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if !user_objects(&tx)?.is_empty() {
        tx.rollback()?;
        return Ok(CreateSchema::AdoptedExisting);
    }
    tx.execute(SQL_DECL, [])?;
    tx.execute(SQL_RUN, [])?;
    tx.execute(
        "INSERT INTO harness_decl (id, schema_version, schema_fingerprint) VALUES (1, ?1, ?2)",
        params![SCHEMA_VERSION, SCHEMA_FINGERPRINT],
    )?;
    tx.commit()?;
    Ok(CreateSchema::Created)
}

fn user_objects(conn: &Connection) -> Result<Vec<(String, String, Option<String>)>> {
    let mut stmt = conn.prepare("SELECT type, name, sql FROM sqlite_master")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
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
        conn.prepare("SELECT schema_version, schema_fingerprint FROM harness_decl WHERE id = 1")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let rows = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    if rows.len() != 1 {
        return Err(Error::DeclarationMismatch(format!(
            "harness_decl 应恰好一行，实际 {}",
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

fn verify_recipe(conn: &Connection) -> Result<()> {
    let recipe = read_recipe(conn)?;
    if !recipe.journal_mode.eq_ignore_ascii_case("wal") {
        return Err(Error::Recipe(format!(
            "journal_mode={}",
            recipe.journal_mode
        )));
    }
    if recipe.synchronous != 1 {
        return Err(Error::Recipe(format!("synchronous={}", recipe.synchronous)));
    }
    if recipe.foreign_keys != 1 {
        return Err(Error::Recipe(format!(
            "foreign_keys={}",
            recipe.foreign_keys
        )));
    }
    if recipe.busy_timeout_ms != 5_000 {
        return Err(Error::Recipe(format!(
            "busy_timeout={}",
            recipe.busy_timeout_ms
        )));
    }
    Ok(())
}

fn read_recipe(conn: &Connection) -> Result<Recipe> {
    let journal_mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    let synchronous: i64 = conn.pragma_query_value(None, "synchronous", |row| row.get(0))?;
    let foreign_keys: i64 = conn.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    let busy_timeout_ms: i64 = conn.pragma_query_value(None, "busy_timeout", |row| row.get(0))?;
    Ok(Recipe {
        journal_mode,
        synchronous,
        foreign_keys,
        busy_timeout_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch() -> std::path::PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "af-harness-store-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn file_open_is_created_then_already_declared_with_wal_recipe() {
        let dir = scratch();
        let path = dir.join("harness.db");
        let (store, status) = RunStore::open(&path).unwrap();
        assert_eq!(status, OpenStatus::Created);
        let recipe = store.recipe().unwrap();
        assert_eq!(recipe.journal_mode, "wal");
        assert_eq!(recipe.synchronous, 1);
        assert_eq!(recipe.foreign_keys, 1);
        assert_eq!(recipe.busy_timeout_ms, 5_000);
        drop(store);
        let (_store, status) = RunStore::open(&path).unwrap();
        assert_eq!(status, OpenStatus::AlreadyDeclared);
    }

    #[test]
    fn tampered_fingerprint_and_extra_table_fail_closed() {
        let dir = scratch();
        let path = dir.join("harness.db");
        drop(RunStore::open(&path).unwrap());
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute(
                "UPDATE harness_decl SET schema_fingerprint = 'nope' WHERE id = 1",
                [],
            )
            .unwrap();
        }
        match RunStore::open(&path) {
            Err(err) => assert!(matches!(err, Error::DeclarationMismatch(_)), "{err}"),
            Ok(_) => panic!("指纹被改过的库必须拒绝"),
        }

        let dir = scratch();
        let path = dir.join("harness.db");
        drop(RunStore::open(&path).unwrap());
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute("CREATE TABLE extra (id INTEGER)", []).unwrap();
        }
        match RunStore::open(&path) {
            Err(err) => assert!(matches!(err, Error::DeclarationMismatch(_)), "{err}"),
            Ok(_) => panic!("多出来的表必须拒绝"),
        }
    }
}
