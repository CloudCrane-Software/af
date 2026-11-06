//! 根密钥的持久形态。
//!
//! `<dir>/root.key` 是当前代数的 ed25519 种子，权限必须是 `0600`。
//! `<dir>/credential.db` 记下声明、每一代公钥、宽限截止和引导凭证原文。
//! 配方与 af-station 相同：WAL、`busy_timeout=5000ms`、`synchronous=NORMAL`、`foreign_keys=ON`。
//!
//! 首启 [`open`] 返回 [`OpenOutcome::Created`]：生成密钥、写 `root.key`、签发引导凭证并落库。
//! 两件先写在目录里的 `.init`，就绪后先把库、再把密钥 rename 进活目录，活目录才同时看得到。
//! 崩溃落在发布中途时，下一次 [`open`] 用同一把暂存密钥把发布做完，不另生成。
//! 活目录里只剩一件、且 `.init` 续不上时返回 [`RootError::Incomplete`]。
//! [`quarantine_incomplete`] 把那一件改名挪走，不删除字节；调用之后才能重新首启。
//! 重入返回 [`OpenOutcome::AlreadyDeclared`]：读回同一把密钥，引导凭证按原文重放，不重新签名。
//! 文件损坏、权限不是 `0600`、schema 或声明对不上，都直接失败，不生成新密钥。
//!
//! [`rotate`] 换一把新根并记下代数。旧根签发的凭证只在调用方给出的宽限内通过 [`OpenedRoot::verify`]。
//! 宽限用注入的 [`Clock`] 比较，不读本机睡眠。
//!
//! 验签不跑 Datalog。执行限额仍只在 `authorize` / `inspect` 里显式配置（见 `verify_limits`）。

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use biscuit_auth::{Algorithm, Biscuit, KeyPair, PrivateKey, PublicKey, UnverifiedBiscuit};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use thiserror::Error;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use crate::clock::Clock;
use crate::credential::{build_root_biscuit, RootSpec};

const ROOT_KEY_NAME: &str = "root.key";
const DB_NAME: &str = "credential.db";
const PENDING_NAME: &str = "root.key.pending";
const LOCK_NAME: &str = ".root.lock";
/// 首启暂存目录。活目录的存在性检查不看它。
const STAGE_DIR_NAME: &str = ".init";
const READY_NAME: &str = "READY";
const ABANDONED_KEY_NAME: &str = "root.key.abandoned";
const ABANDONED_DB_NAME: &str = "credential.db.abandoned";
/// 测试注入：密钥已在 `.init`，库还没有。
const FAULT_AFTER_STAGE_KEY: u8 = 1;
/// 测试注入：`.init` 里两件和 READY 都在，还没 rename 进活目录。
const FAULT_AFTER_READY: u8 = 2;
/// 测试注入：库已经进活目录，密钥还在 `.init`。
const FAULT_AFTER_PUBLISH_DB: u8 = 3;

/// `credential.db` 里 `schema_meta.schema_version` 的唯一接受值。
pub const SCHEMA_VERSION: i64 = 1;

const FILE_SCHEMA: u32 = 1;
const KEY_MAGIC: &[u8; 8] = b"AFROOT01";
const PENDING_MAGIC: &[u8; 8] = b"AFROTP01";
const KEY_LEN: usize = 80;
const PENDING_LEN: usize = 100;

/// 首启或重入。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenOutcome {
    /// 目录里还没有根。本次生成密钥并签发了引导凭证。
    Created,
    /// 根已经在。本次读回同一把密钥，引导凭证是落库原文。
    AlreadyDeclared,
}

/// 旧根自退役时刻起还能验签多久。
///
/// `grace == 0` 时，轮换生效的同一时刻起旧凭证被拒绝。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GracePolicy {
    pub grace: Duration,
}

/// 一次成功的轮换。时刻是注入时钟的 Unix 毫秒。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rotation {
    pub previous_generation: u32,
    pub generation: u32,
    pub retired_at_ms: i64,
    pub grace_until_ms: i64,
    pub public_key: PublicKey,
}

/// 打开后的根。句柄不缓存私钥；签发和验签都在文件锁里读盘。
pub struct OpenedRoot {
    dir: PathBuf,
    conn: Connection,
    clock: Arc<dyn Clock>,
    outcome: OpenOutcome,
    bootstrap_bytes: Vec<u8>,
    bootstrap_generation: u32,
    bootstrap_issued_at_ms: i64,
}

impl std::fmt::Debug for OpenedRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenedRoot")
            .field("dir", &self.dir)
            .field("outcome", &self.outcome)
            .field("bootstrap_generation", &self.bootstrap_generation)
            .field("bootstrap_issued_at_ms", &self.bootstrap_issued_at_ms)
            .finish()
    }
}

/// 根存储失败。这些失败都不会另生成一把密钥。
#[derive(Debug, Error)]
pub enum RootError {
    #[error("root store is not initialized: {path}")]
    NotInitialized { path: PathBuf },
    #[error("root store is incomplete: {what}")]
    Incomplete { what: String },
    #[error("root key file is corrupt ({path}): {reason}")]
    CorruptKey { path: PathBuf, reason: String },
    #[error("permissions of {path} are {mode:o}, want 0600")]
    BadPermissions { path: PathBuf, mode: u32 },
    #[error("schema version {found} does not match {expected}")]
    SchemaMismatch { found: i64, expected: i64 },
    #[error("declaration mismatch on {field}")]
    DeclarationMismatch { field: &'static str },
    #[error("root store is inconsistent: {what}")]
    Inconsistent { what: String },
    #[error("token has no root key id")]
    MissingRootKeyId,
    #[error("unknown root generation {generation}")]
    UnknownGeneration { generation: u32 },
    #[error("generation {generation} is outside its grace window")]
    GraceExpired { generation: u32 },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("token: {0}")]
    Token(Box<biscuit_auth::error::Token>),
    #[error("signature: {0}")]
    Format(Box<biscuit_auth::error::Format>),
}

impl From<biscuit_auth::error::Token> for RootError {
    fn from(err: biscuit_auth::error::Token) -> Self {
        RootError::Token(Box::new(err))
    }
}

impl From<biscuit_auth::error::Format> for RootError {
    fn from(err: biscuit_auth::error::Format) -> Self {
        RootError::Format(Box::new(err))
    }
}

struct KeyFile {
    generation: u32,
    keypair: KeyPair,
    public_key: PublicKey,
}

struct PendingFile {
    generation: u32,
    public_key: PublicKey,
    retired_generation: u32,
    retired_at_ms: i64,
    grace_until_ms: i64,
    seed: [u8; 32],
}

#[derive(Clone)]
struct GenerationRow {
    generation: u32,
    public_key: PublicKey,
    is_current: bool,
    retired_at_ms: Option<i64>,
    grace_until_ms: Option<i64>,
}

struct StoredDecl {
    schema_version: i64,
    namespace: String,
    tools_json: String,
    budget_cents: i64,
}

struct BootstrapRow {
    generation: u32,
    token: Vec<u8>,
    issued_at_ms: i64,
}

/// 打开根目录。时刻用 [`crate::SystemClock`]。
///
/// 目录不存在时会创建。`spec` 是引导凭证的声明：重入时必须与首启逐项一致
/// （工具列表按顺序比较）。
pub fn open(dir: impl AsRef<Path>, spec: &RootSpec) -> Result<OpenedRoot, RootError> {
    open_with_clock(dir, spec, crate::SystemClock::new())
}

/// 同 [`open`]，宽限与签发时刻走 `clock`。
pub fn open_with_clock(
    dir: impl AsRef<Path>,
    spec: &RootSpec,
    clock: impl Clock + 'static,
) -> Result<OpenedRoot, RootError> {
    open_inner(dir.as_ref(), spec, clock, true)
}

/// 轮换已存在的根。目录还没有根时失败，不创建新密钥。时刻用 [`crate::SystemClock`]。
pub fn rotate(
    dir: impl AsRef<Path>,
    spec: &RootSpec,
    grace: GracePolicy,
) -> Result<(OpenedRoot, Rotation), RootError> {
    rotate_with_clock(dir, spec, grace, crate::SystemClock::new())
}

/// 同 [`rotate`]，退役时刻与宽限截止走 `clock`。
pub fn rotate_with_clock(
    dir: impl AsRef<Path>,
    spec: &RootSpec,
    grace: GracePolicy,
    clock: impl Clock + 'static,
) -> Result<(OpenedRoot, Rotation), RootError> {
    let mut opened = open_inner(dir.as_ref(), spec, clock, false)?;
    let rotation = opened.rotate(grace)?;
    Ok((opened, rotation))
}

fn open_inner(
    dir: &Path,
    spec: &RootSpec,
    clock: impl Clock + 'static,
    allow_create: bool,
) -> Result<OpenedRoot, RootError> {
    if dir
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err(RootError::Inconsistent {
            what: format!("{} is a symlink", dir.display()),
        });
    }
    if dir.exists() && !dir.is_dir() {
        return Err(RootError::Inconsistent {
            what: format!("{} is not a directory", dir.display()),
        });
    }
    if !dir.exists() {
        if !allow_create {
            return Err(RootError::NotInitialized {
                path: dir.to_path_buf(),
            });
        }
        fs::create_dir_all(dir)?;
    }

    // flock 不可重入。这个锁盖住整个 open；里面不再调用会加锁的公开方法。
    let _lock = DirLock::acquire(dir)?;
    let key_exists = root_key_path(dir).is_file();
    let db_exists = db_path(dir).is_file();
    let clock: Arc<dyn Clock> = Arc::new(clock);
    match (key_exists, db_exists) {
        (false, false) => {
            if pending_path(dir).exists() {
                return Err(RootError::Inconsistent {
                    what: "orphan rotation journal".into(),
                });
            }
            if publish_can_resume(dir)? {
                finish_publish(dir)?;
                return reopen_store(dir, spec, clock);
            }
            if !allow_create {
                return Err(RootError::NotInitialized {
                    path: dir.to_path_buf(),
                });
            }
            discard_incomplete_stage(dir)?;
            create_store(dir, spec, clock)
        }
        (true, true) => {
            if publish_can_resume(dir)? {
                finish_publish(dir)?;
            } else {
                discard_stray_stage(dir)?;
            }
            reopen_store(dir, spec, clock)
        }
        _ => {
            if publish_can_resume(dir)? {
                finish_publish(dir)?;
                reopen_store(dir, spec, clock)
            } else {
                Err(incomplete_pair(key_exists, db_exists))
            }
        }
    }
}

/// 人工把残缺根挪到旁边。不删除字节，也不另生成密钥。
///
/// 只接受活目录里恰好一件、并且 `.init` 不能由 [`open`] 续完的目录。
/// 两件都在、一件都没有、或暂存区还能续上时拒绝。
/// 成功之后下一轮 [`open`] 才会走 [`OpenOutcome::Created`]。
pub fn quarantine_incomplete(dir: impl AsRef<Path>) -> Result<(), RootError> {
    let dir = dir.as_ref();
    if dir
        .symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err(RootError::Inconsistent {
            what: format!("{} is a symlink", dir.display()),
        });
    }
    if !dir.is_dir() {
        return Err(RootError::Inconsistent {
            what: format!("{} is not a directory", dir.display()),
        });
    }
    let _lock = DirLock::acquire(dir)?;
    if publish_can_resume(dir)? {
        return Err(RootError::Inconsistent {
            what: "init stage can be finished by open; refusing to quarantine".into(),
        });
    }
    let key_exists = root_key_path(dir).is_file();
    let db_exists = db_path(dir).is_file();
    match (key_exists, db_exists) {
        (false, false) => Err(RootError::Inconsistent {
            what: "nothing incomplete to quarantine".into(),
        }),
        (true, true) => Err(RootError::Inconsistent {
            what: "root store is complete; refusing to quarantine".into(),
        }),
        (true, false) => rename_aside(&root_key_path(dir), &dir.join(ABANDONED_KEY_NAME)),
        (false, true) => {
            rename_aside(&db_path(dir), &dir.join(ABANDONED_DB_NAME))?;
            if wal_path(dir).exists() {
                rename_aside(
                    &wal_path(dir),
                    &dir.join(format!("{ABANDONED_DB_NAME}-wal")),
                )?;
            }
            if shm_path(dir).exists() {
                rename_aside(
                    &shm_path(dir),
                    &dir.join(format!("{ABANDONED_DB_NAME}-shm")),
                )?;
            }
            Ok(())
        }
    }
}

impl OpenedRoot {
    pub fn outcome(&self) -> OpenOutcome {
        self.outcome
    }

    /// 首启签发、之后原样读回的引导凭证字节。
    pub fn bootstrap_bytes(&self) -> &[u8] {
        &self.bootstrap_bytes
    }

    pub fn bootstrap_generation(&self) -> u32 {
        self.bootstrap_generation
    }

    /// 引导凭证落库时的注入时钟 Unix 毫秒。
    pub fn bootstrap_issued_at_ms(&self) -> i64 {
        self.bootstrap_issued_at_ms
    }

    /// 当前代数。读盘，不看句柄缓存。
    pub fn generation(&self) -> Result<u32, RootError> {
        let _lock = DirLock::acquire(&self.dir)?;
        self.generation_unlocked()
    }

    /// 当前根公钥。
    pub fn public_key(&self) -> Result<PublicKey, RootError> {
        let _lock = DirLock::acquire(&self.dir)?;
        Ok(self.current_unlocked()?.public_key)
    }

    /// 某一代的宽限截止。当前代是 `None`。未知代数报错。
    pub fn grace_until(&self, generation: u32) -> Result<Option<i64>, RootError> {
        let _lock = DirLock::acquire(&self.dir)?;
        reconcile(&self.dir, &self.conn)?;
        let rows = load_generations(&self.conn)?;
        let row = rows
            .iter()
            .find(|g| g.generation == generation)
            .ok_or(RootError::UnknownGeneration { generation })?;
        Ok(row.grace_until_ms)
    }

    /// 用当前根签发一张新凭证。`root_key_id` 等于当前代数。
    ///
    /// 不改引导凭证。引导凭证只在首启写一次。
    pub fn issue(&self, spec: &RootSpec) -> Result<Biscuit, RootError> {
        let _lock = DirLock::acquire(&self.dir)?;
        let current = self.current_unlocked()?;
        let file = read_key_file(&root_key_path(&self.dir))?;
        if file.generation != current.generation || file.public_key != current.public_key {
            return Err(RootError::Inconsistent {
                what: "root.key does not match the current generation".into(),
            });
        }
        Ok(build_root_biscuit(
            spec,
            &file.keypair,
            Some(file.generation),
        )?)
    }

    /// 验签。`root_key_id` 选出代数；退役代数只在 `now < grace_until_ms` 时通过。
    ///
    /// 不跑 authorizer。Datalog 限额仍由 [`crate::authorize`] 负责。
    pub fn verify(&self, bytes: &[u8]) -> Result<Biscuit, RootError> {
        let _lock = DirLock::acquire(&self.dir)?;
        reconcile(&self.dir, &self.conn)?;
        let rows = load_generations(&self.conn)?;
        let unverified = UnverifiedBiscuit::from(bytes)?;
        let id = unverified
            .root_key_id()
            .ok_or(RootError::MissingRootKeyId)?;
        let row = rows
            .iter()
            .find(|g| g.generation == id)
            .ok_or(RootError::UnknownGeneration { generation: id })?;
        if !row.is_current {
            match row.grace_until_ms {
                Some(until) if self.clock.now_ms() < until => {}
                _ => return Err(RootError::GraceExpired { generation: id }),
            }
        }
        Ok(unverified.verify(row.public_key)?)
    }

    /// 生成下一把根，把当前代标成退役，并写上宽限截止。
    pub fn rotate(&mut self, grace: GracePolicy) -> Result<Rotation, RootError> {
        let _lock = DirLock::acquire(&self.dir)?;
        self.rotate_locked(grace)
    }

    fn generation_unlocked(&self) -> Result<u32, RootError> {
        Ok(self.current_unlocked()?.generation)
    }

    fn current_unlocked(&self) -> Result<GenerationRow, RootError> {
        reconcile(&self.dir, &self.conn)?;
        let rows = load_generations(&self.conn)?;
        Ok(require_current(&rows)?.clone())
    }

    fn rotate_locked(&mut self, grace: GracePolicy) -> Result<Rotation, RootError> {
        reconcile(&self.dir, &self.conn)?;
        if pending_path(&self.dir).exists() {
            return Err(RootError::Inconsistent {
                what: "rotation journal still present after reconcile".into(),
            });
        }
        let file = read_key_file(&root_key_path(&self.dir))?;
        let rows = load_generations(&self.conn)?;
        let current = require_current(&rows)?.clone();
        if file.generation != current.generation || file.public_key != current.public_key {
            return Err(RootError::Inconsistent {
                what: "root.key does not match the current generation".into(),
            });
        }
        let now = self.clock.now_ms();
        let grace_until = now.saturating_add(duration_ms(grace.grace));
        let next = file
            .generation
            .checked_add(1)
            .ok_or_else(|| RootError::Inconsistent {
                what: "generation overflow".into(),
            })?;
        let new_kp = KeyPair::new();
        let new_pk = new_kp.public();
        let pending = pending_path(&self.dir);
        write_pending_file(
            &pending,
            &PendingFile {
                generation: next,
                public_key: new_pk,
                retired_generation: file.generation,
                retired_at_ms: now,
                grace_until_ms: grace_until,
                seed: seed_of(&new_kp)?,
            },
        )?;

        if let Err(err) = commit_rotation(
            &mut self.conn,
            file.generation,
            next,
            &new_pk,
            now,
            grace_until,
        ) {
            let _ = fs::remove_file(&pending);
            return Err(err);
        }
        // 库已经记下新代数。导出失败时留下 journal，下次打开用同一把密钥补写 root.key。
        write_key_file(&root_key_path(&self.dir), next, &new_kp)?;
        let _ = fs::remove_file(&pending);
        Ok(Rotation {
            previous_generation: file.generation,
            generation: next,
            retired_at_ms: now,
            grace_until_ms: grace_until,
            public_key: new_pk,
        })
    }
}

fn commit_rotation(
    conn: &mut Connection,
    previous: u32,
    next: u32,
    new_pk: &PublicKey,
    now: i64,
    grace_until: i64,
) -> Result<(), RootError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let n = tx.execute(
        "UPDATE root_generation
         SET is_current = 0, retired_at_ms = ?1, grace_until_ms = ?2
         WHERE generation = ?3 AND is_current = 1",
        params![now, grace_until, i64::from(previous)],
    )?;
    if n != 1 {
        return Err(RootError::Inconsistent {
            what: "current generation changed during rotation".into(),
        });
    }
    tx.execute(
        "INSERT INTO root_generation
            (generation, public_key, created_at_ms, retired_at_ms, grace_until_ms, is_current)
         VALUES (?1, ?2, ?3, NULL, NULL, 1)",
        params![i64::from(next), new_pk.to_bytes(), now],
    )?;
    tx.commit()?;
    Ok(())
}

fn create_store(
    dir: &Path,
    spec: &RootSpec,
    clock: Arc<dyn Clock>,
) -> Result<OpenedRoot, RootError> {
    let result = create_store_inner(dir, spec, clock);
    if let Err(err) = &result {
        // 注入故障模拟进程停掉，不跑清理。真实失败且暂存区还不能续上时，删掉半截 `.init`。
        if !fault_is_injected(err) && !publish_can_resume(dir).unwrap_or(false) {
            let _ = fs::remove_dir_all(stage_dir(dir));
        }
    }
    result
}

fn create_store_inner(
    dir: &Path,
    spec: &RootSpec,
    clock: Arc<dyn Clock>,
) -> Result<OpenedRoot, RootError> {
    let stage = reject_stage_symlink(dir)?;
    if stage.exists() {
        return Err(RootError::Inconsistent {
            what: "init stage already exists".into(),
        });
    }
    fs::create_dir(&stage)?;
    #[cfg(unix)]
    {
        fs::set_permissions(&stage, fs::Permissions::from_mode(0o700))?;
    }

    let keypair = KeyPair::new();
    let generation = 1u32;
    let token = build_root_biscuit(spec, &keypair, Some(generation))?;
    let bytes = token.to_vec()?;
    let now = clock.now_ms();
    write_key_file(&stage.join(ROOT_KEY_NAME), generation, &keypair)?;
    injected_create_fault(FAULT_AFTER_STAGE_KEY, "after_stage_key")?;
    write_staged_db(&stage, spec, &keypair, &bytes, now)?;
    write_ready(&stage)?;
    injected_create_fault(FAULT_AFTER_READY, "after_ready")?;
    finish_publish(dir)?;

    let conn = prepare_conn(&db_path(dir))?;
    Ok(OpenedRoot {
        dir: dir.to_path_buf(),
        conn,
        clock,
        outcome: OpenOutcome::Created,
        bootstrap_bytes: bytes,
        bootstrap_generation: generation,
        bootstrap_issued_at_ms: now,
    })
}

fn write_staged_db(
    stage: &Path,
    spec: &RootSpec,
    keypair: &KeyPair,
    bytes: &[u8],
    now: i64,
) -> Result<(), RootError> {
    let db = stage.join(DB_NAME);
    let mut conn = prepare_conn(&db)?;
    init_schema(&conn)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let tools = tools_json(&spec.tools)?;
    tx.execute(
        "INSERT INTO schema_meta (id, schema_version, namespace, tools_json, budget_cents)
         VALUES (1, ?1, ?2, ?3, ?4)",
        params![SCHEMA_VERSION, &spec.namespace, tools, spec.budget_cents],
    )?;
    tx.execute(
        "INSERT INTO root_generation
            (generation, public_key, created_at_ms, retired_at_ms, grace_until_ms, is_current)
         VALUES (1, ?1, ?2, NULL, NULL, 1)",
        params![keypair.public().to_bytes(), now],
    )?;
    tx.execute(
        "INSERT INTO bootstrap_credential (id, generation, token, issued_at_ms)
         VALUES (1, 1, ?1, ?2)",
        params![bytes, now],
    )?;
    tx.commit()?;
    checkpoint_truncate(&conn)?;
    drop(conn);
    let _ = fs::remove_file(stage.join(format!("{DB_NAME}-wal")));
    let _ = fs::remove_file(stage.join(format!("{DB_NAME}-shm")));
    fsync_file(&db)?;
    fsync_dir(stage)?;
    Ok(())
}

fn checkpoint_truncate(conn: &Connection) -> Result<(), RootError> {
    let (busy, _log, _checkpointed): (i64, i64, i64) =
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    if busy != 0 {
        return Err(RootError::Inconsistent {
            what: "wal checkpoint did not finish".into(),
        });
    }
    Ok(())
}

fn write_ready(stage: &Path) -> Result<(), RootError> {
    let path = stage.join(READY_NAME);
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    opts.mode(0o600);
    let mut file = opts.open(&path)?;
    file.write_all(b"ready")?;
    file.sync_all()?;
    fsync_dir(stage)?;
    Ok(())
}

fn reopen_store(
    dir: &Path,
    spec: &RootSpec,
    clock: Arc<dyn Clock>,
) -> Result<OpenedRoot, RootError> {
    // 权限和完整性先于任何补写。损坏或权限不对时根文件一个字节都不改。
    ensure_mode_0600(&root_key_path(dir))?;
    read_key_file(&root_key_path(dir))?;

    let conn = prepare_conn(&db_path(dir))?;
    ensure_tables(&conn)?;
    let stored = load_decl(&conn)?;
    check_decl(&stored, spec)?;
    let _ = load_generations(&conn)?;
    reconcile(dir, &conn)?;

    let rows = load_generations(&conn)?;
    let boot = load_bootstrap(&conn)?;
    check_bootstrap(&boot, &rows)?;
    Ok(OpenedRoot {
        dir: dir.to_path_buf(),
        conn,
        clock,
        outcome: OpenOutcome::AlreadyDeclared,
        bootstrap_bytes: boot.token,
        bootstrap_generation: boot.generation,
        bootstrap_issued_at_ms: boot.issued_at_ms,
    })
}

/// 把崩溃窗口里的 journal 收尾。只复用 journal 里已经生成的密钥，不另生成。
fn reconcile(dir: &Path, conn: &Connection) -> Result<(), RootError> {
    let key_path = root_key_path(dir);
    ensure_mode_0600(&key_path)?;
    let file = read_key_file(&key_path)?;
    let rows = load_generations(conn)?;
    let current = require_current(&rows)?.clone();
    let pending_path = pending_path(dir);
    if !pending_path.exists() {
        if file.generation != current.generation || file.public_key != current.public_key {
            return Err(RootError::Inconsistent {
                what: "root.key does not match the current generation".into(),
            });
        }
        return Ok(());
    }

    ensure_mode_0600(&pending_path)?;
    let pending = read_pending_file(&pending_path)?;
    let Some(row) = rows.iter().find(|g| g.generation == pending.generation) else {
        if file.generation == current.generation && file.public_key == current.public_key {
            fs::remove_file(&pending_path)?;
            return Ok(());
        }
        return Err(RootError::Inconsistent {
            what: "uncommitted journal does not match root.key".into(),
        });
    };
    if !row.is_current || row.public_key != pending.public_key {
        return Err(RootError::Inconsistent {
            what: "pending generation is not the current key".into(),
        });
    }
    let retired = rows
        .iter()
        .find(|g| g.generation == pending.retired_generation)
        .ok_or_else(|| RootError::Inconsistent {
            what: "pending journal names an unknown retired generation".into(),
        })?;
    if retired.is_current
        || retired.retired_at_ms != Some(pending.retired_at_ms)
        || retired.grace_until_ms != Some(pending.grace_until_ms)
    {
        return Err(RootError::Inconsistent {
            what: "pending journal does not match the retired generation".into(),
        });
    }
    if file.generation != pending.generation || file.public_key != pending.public_key {
        let keypair = keypair_from_seed(&pending.seed).map_err(|err| RootError::CorruptKey {
            path: pending_path.clone(),
            reason: err,
        })?;
        write_key_file(&key_path, pending.generation, &keypair)?;
    }
    fs::remove_file(&pending_path)?;
    Ok(())
}

fn duration_ms(grace: Duration) -> i64 {
    i64::try_from(grace.as_millis()).unwrap_or(i64::MAX)
}

fn root_key_path(dir: &Path) -> PathBuf {
    dir.join(ROOT_KEY_NAME)
}

fn db_path(dir: &Path) -> PathBuf {
    dir.join(DB_NAME)
}

fn pending_path(dir: &Path) -> PathBuf {
    dir.join(PENDING_NAME)
}

fn wal_path(dir: &Path) -> PathBuf {
    dir.join(format!("{DB_NAME}-wal"))
}

fn shm_path(dir: &Path) -> PathBuf {
    dir.join(format!("{DB_NAME}-shm"))
}

fn stage_dir(dir: &Path) -> PathBuf {
    dir.join(STAGE_DIR_NAME)
}

fn reject_stage_symlink(dir: &Path) -> Result<PathBuf, RootError> {
    let stage = stage_dir(dir);
    if stage
        .symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err(RootError::Inconsistent {
            what: ".init is a symlink".into(),
        });
    }
    Ok(stage)
}

/// `.init/READY` 在，并且密钥和库各自只在暂存区或活目录里出现一次。
fn publish_can_resume(dir: &Path) -> Result<bool, RootError> {
    let stage = reject_stage_symlink(dir)?;
    if !stage.join(READY_NAME).is_file() {
        return Ok(false);
    }
    let key_live = root_key_path(dir).is_file();
    let db_live = db_path(dir).is_file();
    let key_staged = stage.join(ROOT_KEY_NAME).is_file();
    let db_staged = stage.join(DB_NAME).is_file();
    if (key_live && key_staged) || (db_live && db_staged) {
        return Err(RootError::Inconsistent {
            what: "init stage overlaps a live root file".into(),
        });
    }
    Ok((key_live || key_staged) && (db_live || db_staged))
}

fn finish_publish(dir: &Path) -> Result<(), RootError> {
    let stage = reject_stage_symlink(dir)?;
    if !stage.join(READY_NAME).is_file() {
        return Err(RootError::Inconsistent {
            what: "init stage is not ready".into(),
        });
    }
    move_staged(&stage.join(DB_NAME), &db_path(dir))?;
    move_staged(&stage.join(format!("{DB_NAME}-wal")), &wal_path(dir))?;
    move_staged(&stage.join(format!("{DB_NAME}-shm")), &shm_path(dir))?;
    fsync_dir(dir)?;
    injected_create_fault(FAULT_AFTER_PUBLISH_DB, "after_publish_db")?;
    move_staged(&stage.join(ROOT_KEY_NAME), &root_key_path(dir))?;
    ensure_mode_0600(&root_key_path(dir))?;
    let _ = read_key_file(&root_key_path(dir))?;
    if !db_path(dir).is_file() {
        return Err(RootError::Incomplete {
            what: "credential.db missing after publish".into(),
        });
    }
    fsync_dir(dir)?;
    let _ = fs::remove_file(stage.join(READY_NAME));
    fs::remove_dir_all(&stage)?;
    Ok(())
}

fn move_staged(from: &Path, to: &Path) -> Result<(), RootError> {
    let from_exists = from.is_file();
    let to_exists = to.is_file();
    if from_exists && to_exists {
        return Err(RootError::Inconsistent {
            what: format!("both staged and live exist: {}", to.display()),
        });
    }
    if from_exists {
        fs::rename(from, to)?;
    }
    Ok(())
}

fn discard_incomplete_stage(dir: &Path) -> Result<(), RootError> {
    if fs::symlink_metadata(stage_dir(dir)).is_err() {
        return Ok(());
    }
    let stage = reject_stage_symlink(dir)?;
    if stage.exists() {
        fs::remove_dir_all(&stage)?;
    }
    Ok(())
}

fn discard_stray_stage(dir: &Path) -> Result<(), RootError> {
    if fs::symlink_metadata(stage_dir(dir)).is_err() {
        return Ok(());
    }
    let stage = reject_stage_symlink(dir)?;
    if !stage.exists() {
        return Ok(());
    }
    let stray = stage.join(ROOT_KEY_NAME).exists() || stage.join(DB_NAME).exists();
    if stray {
        return Err(RootError::Inconsistent {
            what: "init stage holds a root file beside a complete store".into(),
        });
    }
    fs::remove_dir_all(&stage)?;
    Ok(())
}

fn incomplete_pair(key_exists: bool, db_exists: bool) -> RootError {
    let (present, missing) = match (key_exists, db_exists) {
        (true, false) => ("root.key", "credential.db"),
        (false, true) => ("credential.db", "root.key"),
        _ => ("root store", "its pair"),
    };
    RootError::Incomplete {
        what: format!(
            "{present} exists but {missing} does not; call quarantine_incomplete to move the leftover aside (open does not delete it or mint a replacement)"
        ),
    }
}

fn rename_aside(from: &Path, to: &Path) -> Result<(), RootError> {
    if to.exists() {
        return Err(RootError::Inconsistent {
            what: format!("abandon target already exists: {}", to.display()),
        });
    }
    fs::rename(from, to)?;
    Ok(())
}

fn fault_is_injected(err: &RootError) -> bool {
    matches!(err, RootError::Inconsistent { what } if what.starts_with("injected create fault"))
}

#[cfg(test)]
std::thread_local! {
    static CREATE_FAULT: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

fn injected_create_fault(code: u8, label: &str) -> Result<(), RootError> {
    #[cfg(test)]
    {
        let armed = CREATE_FAULT.with(std::cell::Cell::get);
        if armed != 0 && armed == code {
            return Err(RootError::Inconsistent {
                what: format!("injected create fault: {label}"),
            });
        }
    }
    #[cfg(not(test))]
    {
        let _ = (code, label);
    }
    Ok(())
}

#[cfg(test)]
fn set_create_fault(code: u8) {
    CREATE_FAULT.with(|cell| cell.set(code));
}

fn seed_of(keypair: &KeyPair) -> Result<[u8; 32], RootError> {
    let bytes = keypair.private().to_bytes();
    let seed: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| RootError::Inconsistent {
            what: "ed25519 seed is not 32 bytes".into(),
        })?;
    Ok(seed)
}

fn keypair_from_seed(seed: &[u8; 32]) -> Result<KeyPair, String> {
    let private =
        PrivateKey::from_bytes(seed, Algorithm::Ed25519).map_err(|err| format!("seed: {err}"))?;
    Ok(KeyPair::from(&private))
}

fn prepare_conn(path: &Path) -> Result<Connection, RootError> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_millis(5000))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // 库里有引导凭证原文。收紧到 0600，避免 umask 把它变成组可读。
    // root.key / journal 的权限错误仍然直接失败，不在这里改那些文件。
    set_mode_0600(path)?;
    if let Some(dir) = path.parent() {
        for extra in [wal_path(dir), shm_path(dir)] {
            if extra.exists() {
                set_mode_0600(&extra)?;
            }
        }
    }
    Ok(conn)
}

fn init_schema(conn: &Connection) -> Result<(), RootError> {
    conn.execute_batch(
        "CREATE TABLE schema_meta (
            id              INTEGER PRIMARY KEY CHECK (id = 1),
            schema_version  INTEGER NOT NULL,
            namespace       TEXT NOT NULL,
            tools_json      TEXT NOT NULL,
            budget_cents    INTEGER NOT NULL
        );
        CREATE TABLE root_generation (
            generation      INTEGER PRIMARY KEY,
            public_key      BLOB NOT NULL,
            created_at_ms   INTEGER NOT NULL,
            retired_at_ms   INTEGER,
            grace_until_ms  INTEGER,
            is_current      INTEGER NOT NULL CHECK (is_current IN (0, 1)),
            CHECK (
                (is_current = 1 AND retired_at_ms IS NULL AND grace_until_ms IS NULL)
                OR
                (is_current = 0 AND retired_at_ms IS NOT NULL AND grace_until_ms IS NOT NULL)
            )
        );
        CREATE UNIQUE INDEX root_generation_one_current
            ON root_generation(is_current) WHERE is_current = 1;
        CREATE TABLE bootstrap_credential (
            id              INTEGER PRIMARY KEY CHECK (id = 1),
            generation      INTEGER NOT NULL REFERENCES root_generation(generation),
            token           BLOB NOT NULL,
            issued_at_ms    INTEGER NOT NULL
        );",
    )?;
    Ok(())
}

fn ensure_tables(conn: &Connection) -> Result<(), RootError> {
    for name in ["schema_meta", "root_generation", "bootstrap_credential"] {
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |row| row.get(0),
        )?;
        if n != 1 {
            return Err(RootError::Incomplete {
                what: format!("missing table {name}"),
            });
        }
    }
    Ok(())
}

fn tools_json(tools: &[String]) -> Result<String, RootError> {
    Ok(serde_json::to_string(tools)?)
}

fn load_decl(conn: &Connection) -> Result<StoredDecl, RootError> {
    let row = conn
        .query_row(
            "SELECT schema_version, namespace, tools_json, budget_cents
             FROM schema_meta WHERE id = 1",
            [],
            |row| {
                Ok(StoredDecl {
                    schema_version: row.get(0)?,
                    namespace: row.get(1)?,
                    tools_json: row.get(2)?,
                    budget_cents: row.get(3)?,
                })
            },
        )
        .optional()?;
    row.ok_or_else(|| RootError::Incomplete {
        what: "schema_meta is empty".into(),
    })
}

fn check_decl(stored: &StoredDecl, spec: &RootSpec) -> Result<(), RootError> {
    if stored.schema_version != SCHEMA_VERSION {
        return Err(RootError::SchemaMismatch {
            found: stored.schema_version,
            expected: SCHEMA_VERSION,
        });
    }
    if stored.namespace != spec.namespace {
        return Err(RootError::DeclarationMismatch { field: "namespace" });
    }
    if stored.tools_json != tools_json(&spec.tools)? {
        return Err(RootError::DeclarationMismatch { field: "tools" });
    }
    if stored.budget_cents != spec.budget_cents {
        return Err(RootError::DeclarationMismatch {
            field: "budget_cents",
        });
    }
    Ok(())
}

fn load_generations(conn: &Connection) -> Result<Vec<GenerationRow>, RootError> {
    let mut stmt = conn.prepare(
        "SELECT generation, public_key, is_current, retired_at_ms, grace_until_ms
         FROM root_generation ORDER BY generation",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Vec<u8>>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, Option<i64>>(3)?,
            row.get::<_, Option<i64>>(4)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (generation, public_bytes, is_current, retired_at_ms, grace_until_ms) = row?;
        let generation = u32::try_from(generation).map_err(|_| RootError::Inconsistent {
            what: format!("generation {generation} is out of range"),
        })?;
        if public_bytes.len() != 32 {
            return Err(RootError::Inconsistent {
                what: format!("generation {generation} public key is not 32 bytes"),
            });
        }
        let public_key =
            PublicKey::from_bytes(&public_bytes, Algorithm::Ed25519).map_err(|err| {
                RootError::Inconsistent {
                    what: format!("generation {generation} public key: {err}"),
                }
            })?;
        let is_current = match is_current {
            1 => true,
            0 => false,
            other => {
                return Err(RootError::Inconsistent {
                    what: format!("generation {generation} is_current={other}"),
                })
            }
        };
        if is_current {
            if retired_at_ms.is_some() || grace_until_ms.is_some() {
                return Err(RootError::Inconsistent {
                    what: format!("current generation {generation} has a grace window"),
                });
            }
        } else if retired_at_ms.is_none() || grace_until_ms.is_none() {
            return Err(RootError::Inconsistent {
                what: format!("retired generation {generation} has no grace window"),
            });
        }
        out.push(GenerationRow {
            generation,
            public_key,
            is_current,
            retired_at_ms,
            grace_until_ms,
        });
    }
    if out.is_empty() {
        return Err(RootError::Inconsistent {
            what: "no generations".into(),
        });
    }
    for (index, row) in out.iter().enumerate() {
        let expect = u32::try_from(index + 1).unwrap_or(u32::MAX);
        if row.generation != expect {
            return Err(RootError::Inconsistent {
                what: format!("generation {} is not contiguous", row.generation),
            });
        }
    }
    let _ = require_current(&out)?;
    Ok(out)
}

fn require_current(rows: &[GenerationRow]) -> Result<&GenerationRow, RootError> {
    let mut found: Option<&GenerationRow> = None;
    for row in rows {
        if row.is_current {
            if found.is_some() {
                return Err(RootError::Inconsistent {
                    what: "more than one current generation".into(),
                });
            }
            found = Some(row);
        }
    }
    found.ok_or_else(|| RootError::Inconsistent {
        what: "no current generation".into(),
    })
}

fn load_bootstrap(conn: &Connection) -> Result<BootstrapRow, RootError> {
    let row = conn
        .query_row(
            "SELECT generation, token, issued_at_ms FROM bootstrap_credential WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((generation, token, issued_at_ms)) = row else {
        return Err(RootError::Incomplete {
            what: "bootstrap credential is missing".into(),
        });
    };
    if token.is_empty() {
        return Err(RootError::Inconsistent {
            what: "bootstrap credential is empty".into(),
        });
    }
    let generation = u32::try_from(generation).map_err(|_| RootError::Inconsistent {
        what: format!("bootstrap generation {generation} is out of range"),
    })?;
    Ok(BootstrapRow {
        generation,
        token,
        issued_at_ms,
    })
}

fn check_bootstrap(boot: &BootstrapRow, rows: &[GenerationRow]) -> Result<(), RootError> {
    let unverified =
        UnverifiedBiscuit::from(&boot.token).map_err(|err| RootError::Inconsistent {
            what: format!("bootstrap token: {err}"),
        })?;
    let id = unverified
        .root_key_id()
        .ok_or_else(|| RootError::Inconsistent {
            what: "bootstrap token has no root key id".into(),
        })?;
    if id != boot.generation {
        return Err(RootError::Inconsistent {
            what: format!(
                "bootstrap root key id {id} != stored generation {}",
                boot.generation
            ),
        });
    }
    let row = rows
        .iter()
        .find(|g| g.generation == id)
        .ok_or_else(|| RootError::Inconsistent {
            what: format!("bootstrap generation {id} is missing"),
        })?;
    unverified
        .verify(row.public_key)
        .map_err(|err| RootError::Inconsistent {
            what: format!("bootstrap signature: {err}"),
        })?;
    Ok(())
}

fn read_key_file(path: &Path) -> Result<KeyFile, RootError> {
    let bytes = read_to_vec(path)?;
    if bytes.len() != KEY_LEN {
        return Err(RootError::CorruptKey {
            path: path.to_path_buf(),
            reason: format!("length {} is not {KEY_LEN}", bytes.len()),
        });
    }
    if bytes[0..8] != KEY_MAGIC[..] {
        return Err(RootError::CorruptKey {
            path: path.to_path_buf(),
            reason: "magic".into(),
        });
    }
    let schema = u32_at(&bytes, 8);
    if schema != FILE_SCHEMA {
        return Err(RootError::CorruptKey {
            path: path.to_path_buf(),
            reason: format!("file schema {schema}"),
        });
    }
    let generation = u32_at(&bytes, 12);
    if generation == 0 {
        return Err(RootError::CorruptKey {
            path: path.to_path_buf(),
            reason: "generation 0".into(),
        });
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&bytes[16..48]);
    let mut pubkey = [0u8; 32];
    pubkey.copy_from_slice(&bytes[48..80]);
    let keypair = keypair_from_seed(&seed).map_err(|reason| RootError::CorruptKey {
        path: path.to_path_buf(),
        reason,
    })?;
    let derived = keypair.public();
    if derived.to_bytes().as_slice() != pubkey {
        return Err(RootError::CorruptKey {
            path: path.to_path_buf(),
            reason: "derived public key does not match the file".into(),
        });
    }
    Ok(KeyFile {
        generation,
        keypair,
        public_key: derived,
    })
}

fn read_pending_file(path: &Path) -> Result<PendingFile, RootError> {
    let bytes = read_to_vec(path)?;
    if bytes.len() != PENDING_LEN {
        return Err(RootError::CorruptKey {
            path: path.to_path_buf(),
            reason: format!("length {} is not {PENDING_LEN}", bytes.len()),
        });
    }
    if bytes[0..8] != PENDING_MAGIC[..] {
        return Err(RootError::CorruptKey {
            path: path.to_path_buf(),
            reason: "magic".into(),
        });
    }
    let schema = u32_at(&bytes, 8);
    if schema != FILE_SCHEMA {
        return Err(RootError::CorruptKey {
            path: path.to_path_buf(),
            reason: format!("file schema {schema}"),
        });
    }
    let generation = u32_at(&bytes, 12);
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&bytes[16..48]);
    let mut pubkey = [0u8; 32];
    pubkey.copy_from_slice(&bytes[48..80]);
    let retired_generation = u32_at(&bytes, 80);
    let retired_at_ms = i64_at(&bytes, 84);
    let grace_until_ms = i64_at(&bytes, 92);
    if generation == 0
        || retired_generation == 0
        || Some(generation) != retired_generation.checked_add(1)
    {
        return Err(RootError::CorruptKey {
            path: path.to_path_buf(),
            reason: "generation link".into(),
        });
    }
    let keypair = keypair_from_seed(&seed).map_err(|reason| RootError::CorruptKey {
        path: path.to_path_buf(),
        reason,
    })?;
    if keypair.public().to_bytes().as_slice() != pubkey {
        return Err(RootError::CorruptKey {
            path: path.to_path_buf(),
            reason: "derived public key does not match the journal".into(),
        });
    }
    Ok(PendingFile {
        generation,
        public_key: keypair.public(),
        retired_generation,
        retired_at_ms,
        grace_until_ms,
        seed,
    })
}

fn write_key_file(path: &Path, generation: u32, keypair: &KeyPair) -> Result<(), RootError> {
    let seed = seed_of(keypair)?;
    let public_key = keypair.public().to_bytes();
    let mut bytes = Vec::with_capacity(KEY_LEN);
    bytes.extend_from_slice(KEY_MAGIC);
    bytes.extend_from_slice(&FILE_SCHEMA.to_le_bytes());
    bytes.extend_from_slice(&generation.to_le_bytes());
    bytes.extend_from_slice(&seed);
    bytes.extend_from_slice(&public_key);
    debug_assert_eq!(bytes.len(), KEY_LEN);
    write_atomic(path, &bytes)
}

fn write_pending_file(path: &Path, pending: &PendingFile) -> Result<(), RootError> {
    let public_key = pending.public_key.to_bytes();
    let mut bytes = Vec::with_capacity(PENDING_LEN);
    bytes.extend_from_slice(PENDING_MAGIC);
    bytes.extend_from_slice(&FILE_SCHEMA.to_le_bytes());
    bytes.extend_from_slice(&pending.generation.to_le_bytes());
    bytes.extend_from_slice(&pending.seed);
    bytes.extend_from_slice(&public_key);
    bytes.extend_from_slice(&pending.retired_generation.to_le_bytes());
    bytes.extend_from_slice(&pending.retired_at_ms.to_le_bytes());
    bytes.extend_from_slice(&pending.grace_until_ms.to_le_bytes());
    debug_assert_eq!(bytes.len(), PENDING_LEN);
    write_atomic(path, &bytes)
}

fn read_to_vec(path: &Path) -> Result<Vec<u8>, RootError> {
    let mut file = File::open(path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(buf)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), RootError> {
    let mut tmp_name = path.as_os_str().to_os_string();
    tmp_name.push(".tmp");
    let tmp = PathBuf::from(tmp_name);
    {
        let mut opts = OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        opts.mode(0o600);
        let mut file = opts.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    set_mode_0600(&tmp)?;
    fs::rename(&tmp, path)?;
    set_mode_0600(path)?;
    if let Some(parent) = path.parent() {
        fsync_dir(parent)?;
    }
    Ok(())
}

fn fsync_file(path: &Path) -> Result<(), RootError> {
    let file = File::open(path)?;
    file.sync_all()?;
    Ok(())
}

fn fsync_dir(path: &Path) -> Result<(), RootError> {
    let dir = File::open(path)?;
    dir.sync_all()?;
    Ok(())
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&bytes[at..at + 4]);
    u32::from_le_bytes(buf)
}

fn i64_at(bytes: &[u8], at: usize) -> i64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[at..at + 8]);
    i64::from_le_bytes(buf)
}

fn set_mode_0600(path: &Path) -> Result<(), RootError> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(RootError::Inconsistent {
            what: "0600 permissions require unix".into(),
        })
    }
}

fn ensure_mode_0600(path: &Path) -> Result<(), RootError> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Err(RootError::Inconsistent {
            what: format!("{} is a symlink", path.display()),
        });
    }
    if !meta.is_file() {
        return Err(RootError::Inconsistent {
            what: format!("{} is not a file", path.display()),
        });
    }
    #[cfg(unix)]
    {
        let mode = meta.permissions().mode() & 0o777;
        if mode != 0o600 {
            return Err(RootError::BadPermissions {
                path: path.to_path_buf(),
                mode,
            });
        }
    }
    Ok(())
}

struct DirLock {
    file: File,
}

impl DirLock {
    fn acquire(dir: &Path) -> Result<Self, RootError> {
        let path = dir.join(LOCK_NAME);
        let mut opts = OpenOptions::new();
        opts.create(true).read(true).write(true);
        #[cfg(unix)]
        opts.mode(0o600);
        let file = opts.open(&path)?;
        let _ = set_mode_0600(&path);
        file.lock()?;
        Ok(Self { file })
    }
}

impl Drop for DirLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use biscuit_auth::{Biscuit, KeyPair};
    use rusqlite::params;

    use super::{
        commit_rotation, db_path, open_with_clock, pending_path, quarantine_incomplete,
        read_key_file, root_key_path, seed_of, set_create_fault, set_mode_0600, stage_dir,
        write_key_file, write_pending_file, OpenOutcome, PendingFile, RootError,
    };
    use crate::{issue_root, serialize, RootSpec};

    fn spec() -> RootSpec {
        RootSpec {
            tools: vec!["git".into(), "test".into()],
            namespace: "line-a".into(),
            budget_cents: 1000,
        }
    }

    struct Temp(PathBuf);

    impl Temp {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "af-credential-root-unit-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// 发现 A：密钥还在 `.init`、READY 没写时进程停掉。活目录两件都不在，下一次 open 重新首启。
    #[test]
    fn finding_a_crash_before_ready_does_not_publish_half_a_store() {
        let dir = Temp::new();
        set_create_fault(1);
        let err = open_with_clock(&dir.0, &spec(), crate::ManualClock::new(50)).unwrap_err();
        assert!(err.to_string().contains("injected create fault"), "{err}");
        assert!(!root_key_path(&dir.0).is_file());
        assert!(!db_path(&dir.0).is_file());
        assert!(stage_dir(&dir.0).join("root.key").is_file());
        let staged = fs::read(stage_dir(&dir.0).join("root.key")).unwrap();
        set_create_fault(0);
        let opened = open_with_clock(&dir.0, &spec(), crate::ManualClock::new(50)).unwrap();
        assert_eq!(opened.outcome(), OpenOutcome::Created);
        assert!(root_key_path(&dir.0).is_file() && db_path(&dir.0).is_file());
        assert!(!stage_dir(&dir.0).exists());
        assert_ne!(fs::read(root_key_path(&dir.0)).unwrap(), staged);
        assert!(opened.verify(opened.bootstrap_bytes()).is_ok());
    }

    /// 发现 A：两件和 READY 都在暂存区、还没 rename。下一次 open 用同一把密钥发布。
    #[test]
    fn finding_a_crash_after_ready_resumes_the_same_key() {
        let dir = Temp::new();
        set_create_fault(2);
        let err = open_with_clock(&dir.0, &spec(), crate::ManualClock::new(50)).unwrap_err();
        assert!(err.to_string().contains("after_ready"), "{err}");
        assert!(!root_key_path(&dir.0).is_file());
        assert!(!db_path(&dir.0).is_file());
        let staged = fs::read(stage_dir(&dir.0).join("root.key")).unwrap();
        assert!(stage_dir(&dir.0).join("READY").is_file());
        assert!(stage_dir(&dir.0).join("credential.db").is_file());
        set_create_fault(0);
        let opened = open_with_clock(&dir.0, &spec(), crate::ManualClock::new(50)).unwrap();
        assert_eq!(opened.outcome(), OpenOutcome::AlreadyDeclared);
        assert_eq!(fs::read(root_key_path(&dir.0)).unwrap(), staged);
        assert!(!stage_dir(&dir.0).exists());
        assert_eq!(opened.bootstrap_issued_at_ms(), 50);
        assert!(opened.verify(opened.bootstrap_bytes()).is_ok());
    }

    /// 发现 A：库已进活目录、密钥还在 `.init`。下一次 open 把同一把密钥补上，不另生成。
    #[test]
    fn finding_a_crash_after_db_rename_resumes_the_same_key() {
        let dir = Temp::new();
        set_create_fault(3);
        let err = open_with_clock(&dir.0, &spec(), crate::ManualClock::new(50)).unwrap_err();
        assert!(err.to_string().contains("after_publish_db"), "{err}");
        assert!(db_path(&dir.0).is_file());
        assert!(!root_key_path(&dir.0).is_file());
        let staged = fs::read(stage_dir(&dir.0).join("root.key")).unwrap();
        set_create_fault(0);
        let opened = open_with_clock(&dir.0, &spec(), crate::ManualClock::new(50)).unwrap();
        assert_eq!(opened.outcome(), OpenOutcome::AlreadyDeclared);
        assert_eq!(fs::read(root_key_path(&dir.0)).unwrap(), staged);
        assert!(!stage_dir(&dir.0).exists());
        assert!(opened.verify(opened.bootstrap_bytes()).is_ok());
        assert_eq!(
            fs::symlink_metadata(root_key_path(&dir.0))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    /// 发现 A：活目录只剩一件、没有可续的 `.init`。错误点名 quarantine_incomplete，调用后才能再首启。
    #[test]
    fn finding_a_half_store_names_quarantine_and_quarantine_unblocks() {
        let dir = Temp::new();
        let clock = crate::ManualClock::new(50);
        let opened = open_with_clock(&dir.0, &spec(), clock.clone()).unwrap();
        drop(opened);
        let key = fs::read(root_key_path(&dir.0)).unwrap();
        fs::remove_file(db_path(&dir.0)).unwrap();
        let err = open_with_clock(&dir.0, &spec(), clock.clone()).unwrap_err();
        match err {
            RootError::Incomplete { what } => {
                assert!(what.contains("quarantine_incomplete"), "{what}");
            }
            other => panic!("{other}"),
        }
        assert_eq!(fs::read(root_key_path(&dir.0)).unwrap(), key);
        assert!(!db_path(&dir.0).exists());

        let refused = quarantine_incomplete(dir.0.join("missing")).unwrap_err();
        assert!(
            matches!(refused, RootError::Inconsistent { .. }),
            "{refused}"
        );

        quarantine_incomplete(&dir.0).unwrap();
        assert!(!root_key_path(&dir.0).exists());
        assert_eq!(
            fs::read(dir.0.join("root.key.abandoned")).unwrap(),
            key,
            "隔离不删除密钥字节"
        );
        let again = quarantine_incomplete(&dir.0).unwrap_err();
        assert!(matches!(again, RootError::Inconsistent { .. }), "{again}");

        let created = open_with_clock(&dir.0, &spec(), clock).unwrap();
        assert_eq!(created.outcome(), OpenOutcome::Created);
        assert_ne!(fs::read(root_key_path(&dir.0)).unwrap(), key);
        assert!(created.verify(created.bootstrap_bytes()).is_ok());
        assert_eq!(fs::read(dir.0.join("root.key.abandoned")).unwrap(), key);
    }

    #[test]
    fn sqlite_pragmas_and_foreign_key() {
        let dir = Temp::new();
        let opened = open_with_clock(&dir.0, &spec(), crate::ManualClock::new(1)).unwrap();
        let fk: i64 = opened
            .conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        let journal: String = opened
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal.to_lowercase(), "wal");
        let sync: i64 = opened
            .conn
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .unwrap();
        assert_eq!(sync, 1);
        let busy: i64 = opened
            .conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(busy, 5000);

        let err = opened
            .conn
            .execute(
                "UPDATE bootstrap_credential SET generation = 99 WHERE id = 1",
                [],
            )
            .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("foreign key"),
            "{err}"
        );
        let generation: i64 = opened
            .conn
            .query_row(
                "SELECT generation FROM bootstrap_credential WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(generation, 1);
    }

    #[test]
    fn schema_mismatch_fail_closed() {
        let dir = Temp::new();
        let clock = crate::ManualClock::new(1);
        let opened = open_with_clock(&dir.0, &spec(), clock.clone()).unwrap();
        opened
            .conn
            .execute("UPDATE schema_meta SET schema_version = 9", [])
            .unwrap();
        let before = fs::read(root_key_path(&dir.0)).unwrap();
        drop(opened);
        let err = open_with_clock(&dir.0, &spec(), clock).unwrap_err();
        assert!(
            matches!(
                err,
                RootError::SchemaMismatch {
                    found: 9,
                    expected: 1
                }
            ),
            "{err}"
        );
        assert_eq!(fs::read(root_key_path(&dir.0)).unwrap(), before);
    }

    #[test]
    fn committed_journal_reexports_the_same_key() {
        let dir = Temp::new();
        let clock = crate::ManualClock::new(5_000);
        let mut opened = open_with_clock(&dir.0, &spec(), clock.clone()).unwrap();
        let old_pk = opened.public_key().unwrap();
        let new_kp = KeyPair::new();
        let new_pk = new_kp.public();
        write_pending_file(
            &pending_path(&dir.0),
            &PendingFile {
                generation: 2,
                public_key: new_pk,
                retired_generation: 1,
                retired_at_ms: 5_000,
                grace_until_ms: 6_000,
                seed: seed_of(&new_kp).unwrap(),
            },
        )
        .unwrap();
        commit_rotation(&mut opened.conn, 1, 2, &new_pk, 5_000, 6_000).unwrap();
        drop(opened);

        let again = open_with_clock(&dir.0, &spec(), clock).unwrap();
        assert_eq!(again.outcome(), OpenOutcome::AlreadyDeclared);
        assert_eq!(again.public_key().unwrap(), new_pk);
        assert_ne!(again.public_key().unwrap(), old_pk);
        assert_eq!(again.generation().unwrap(), 2);
        assert_eq!(again.grace_until(1).unwrap(), Some(6_000));
        assert!(!pending_path(&dir.0).exists());
        let file = read_key_file(&root_key_path(&dir.0)).unwrap();
        assert_eq!(file.generation, 2);
        assert_eq!(file.public_key, new_pk);
    }

    #[test]
    fn uncommitted_journal_is_rolled_back() {
        let dir = Temp::new();
        let clock = crate::ManualClock::new(1);
        let opened = open_with_clock(&dir.0, &spec(), clock.clone()).unwrap();
        let pk = opened.public_key().unwrap();
        let new_kp = KeyPair::new();
        write_pending_file(
            &pending_path(&dir.0),
            &PendingFile {
                generation: 2,
                public_key: new_kp.public(),
                retired_generation: 1,
                retired_at_ms: 1,
                grace_until_ms: 2,
                seed: seed_of(&new_kp).unwrap(),
            },
        )
        .unwrap();
        drop(opened);

        let again = open_with_clock(&dir.0, &spec(), clock).unwrap();
        assert_eq!(again.public_key().unwrap(), pk);
        assert_eq!(again.generation().unwrap(), 1);
        assert!(!pending_path(&dir.0).exists());
    }

    #[test]
    fn corrupt_journal_fail_closed() {
        let dir = Temp::new();
        let clock = crate::ManualClock::new(1);
        let opened = open_with_clock(&dir.0, &spec(), clock.clone()).unwrap();
        drop(opened);
        let key = fs::read(root_key_path(&dir.0)).unwrap();
        let pending = pending_path(&dir.0);
        fs::write(&pending, b"not-a-journal").unwrap();
        set_mode_0600(&pending).unwrap();
        let err = open_with_clock(&dir.0, &spec(), clock).unwrap_err();
        assert!(matches!(err, RootError::CorruptKey { .. }), "{err}");
        assert_eq!(fs::read(root_key_path(&dir.0)).unwrap(), key);
        assert_eq!(fs::read(&pending).unwrap(), b"not-a-journal");
    }

    #[test]
    fn journal_bad_permissions_fail_closed() {
        let dir = Temp::new();
        let clock = crate::ManualClock::new(1);
        let opened = open_with_clock(&dir.0, &spec(), clock.clone()).unwrap();
        drop(opened);
        let key = fs::read(root_key_path(&dir.0)).unwrap();
        let pending = pending_path(&dir.0);
        fs::write(&pending, b"x").unwrap();
        fs::set_permissions(&pending, fs::Permissions::from_mode(0o644)).unwrap();
        let err = open_with_clock(&dir.0, &spec(), clock).unwrap_err();
        match err {
            RootError::BadPermissions { mode, .. } => assert_eq!(mode, 0o644),
            other => panic!("expected bad permissions, got {other}"),
        }
        assert_eq!(fs::read(root_key_path(&dir.0)).unwrap(), key);
        assert_eq!(
            fs::symlink_metadata(&pending).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }

    #[test]
    fn forged_key_file_is_not_replaced() {
        let dir = Temp::new();
        let clock = crate::ManualClock::new(1);
        let opened = open_with_clock(&dir.0, &spec(), clock.clone()).unwrap();
        drop(opened);
        let forged = KeyPair::new();
        write_key_file(&root_key_path(&dir.0), 1, &forged).unwrap();
        let bytes = fs::read(root_key_path(&dir.0)).unwrap();
        let err = open_with_clock(&dir.0, &spec(), clock).unwrap_err();
        assert!(matches!(err, RootError::Inconsistent { .. }), "{err}");
        assert_eq!(fs::read(root_key_path(&dir.0)).unwrap(), bytes);
    }

    #[test]
    fn tampered_bootstrap_fail_closed() {
        let dir = Temp::new();
        let clock = crate::ManualClock::new(1);
        let opened = open_with_clock(&dir.0, &spec(), clock.clone()).unwrap();
        opened
            .conn
            .execute(
                "UPDATE bootstrap_credential SET token = X'00010203' WHERE id = 1",
                [],
            )
            .unwrap();
        let key = fs::read(root_key_path(&dir.0)).unwrap();
        drop(opened);
        let err = open_with_clock(&dir.0, &spec(), clock).unwrap_err();
        assert!(matches!(err, RootError::Inconsistent { .. }), "{err}");
        assert_eq!(fs::read(root_key_path(&dir.0)).unwrap(), key);
    }

    #[test]
    fn generation_gap_fail_closed() {
        let dir = Temp::new();
        let clock = crate::ManualClock::new(1);
        let opened = open_with_clock(&dir.0, &spec(), clock.clone()).unwrap();
        let public_key = opened.public_key().unwrap().to_bytes();
        opened
            .conn
            .execute(
                "UPDATE root_generation
                 SET is_current = 0, retired_at_ms = 1, grace_until_ms = 2
                 WHERE generation = 1",
                [],
            )
            .unwrap();
        opened
            .conn
            .execute(
                "INSERT INTO root_generation
                    (generation, public_key, created_at_ms, retired_at_ms, grace_until_ms, is_current)
                 VALUES (3, ?1, 1, NULL, NULL, 1)",
                params![public_key],
            )
            .unwrap();
        let key = fs::read(root_key_path(&dir.0)).unwrap();
        drop(opened);
        let err = open_with_clock(&dir.0, &spec(), clock).unwrap_err();
        match err {
            RootError::Inconsistent { what } => assert!(what.contains("contiguous"), "{what}"),
            other => panic!("expected inconsistent generations, got {other}"),
        }
        assert_eq!(fs::read(root_key_path(&dir.0)).unwrap(), key);
    }

    #[test]
    fn wrong_signature_and_unknown_generation() {
        let dir = Temp::new();
        let opened = open_with_clock(&dir.0, &spec(), crate::ManualClock::new(1)).unwrap();
        let other = KeyPair::new();
        let wrong_key = Biscuit::builder()
            .fact(r#"tool("git")"#)
            .unwrap()
            .root_key_id(1)
            .build(&other)
            .unwrap();
        let err = opened.verify(&wrong_key.to_vec().unwrap()).unwrap_err();
        assert!(matches!(err, RootError::Format(_)), "{err}");

        let unknown = Biscuit::builder()
            .fact(r#"tool("git")"#)
            .unwrap()
            .root_key_id(9)
            .build(&other)
            .unwrap();
        let err = opened.verify(&unknown.to_vec().unwrap()).unwrap_err();
        assert!(
            matches!(err, RootError::UnknownGeneration { generation: 9 }),
            "{err}"
        );

        let ephemeral = issue_root(&spec()).unwrap();
        let err = opened
            .verify(&serialize(&ephemeral.token).unwrap())
            .unwrap_err();
        assert!(matches!(err, RootError::MissingRootKeyId), "{err}");
    }
}
