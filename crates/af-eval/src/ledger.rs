//! 评测账本。基准事件、配方事件、运行绑定和运行结果都只追加。
//!
//! 打开配方与内核账本相同：WAL、`busy_timeout` 5000ms、`synchronous=NORMAL`、
//! `foreign_keys=ON`。内存库的 journal_mode 只能是 `memory`，文件库必须是 `wal`。
//! 重入时表、只追加触发器和 `ledger_id` 对不上就拒绝，不改已有的行。
//!
//! 当前阶段由事件重放得到：draft 上追加用例，frozen 写一次快照哈希，retired 只改阶段。
//! 离开 draft 之后没有改用例的事件路径。运行行在开始时写上快照哈希和被评对象版本。

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{params, Connection, Transaction, TransactionBehavior};

use crate::clock::{Clock, SystemClock};
use crate::error::{EvalError, Result};
use crate::phase::Phase;
use crate::route::{route_failure, Classified, ExecSignals, FailureCode};
use crate::snapshot::{
    contamination_overlap, decode_list, encode_list, snapshot_hash, BenchCase, FrozenSnapshot,
};
use crate::validate::{require_body, require_budget, require_ident, require_version, MAX_BODY_LEN};

/// 本账本 schema。对不上就拒绝打开，不迁移。
pub const SCHEMA_VERSION: i64 = 1;

const BUSY_TIMEOUT: Duration = Duration::from_millis(5_000);
const BUSY_TIMEOUT_MS: i64 = 5_000;

const TABLES: &[&str] = &[
    "schema_meta",
    "bench_event",
    "recipe_event",
    "run_open",
    "run_close",
];

const DDL: &str = "
CREATE TABLE schema_meta (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema_version INTEGER NOT NULL CHECK (schema_version = 1),
    ledger_id TEXT NOT NULL CHECK (length(ledger_id) > 0)
);
CREATE TABLE bench_event (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    lineage_id TEXT NOT NULL CHECK (length(lineage_id) > 0),
    version INTEGER NOT NULL CHECK (version >= 1),
    kind TEXT NOT NULL CHECK (kind IN ('created','case_set','case_remove','frozen','retired')),
    case_id TEXT,
    payload TEXT,
    parent_lineage_id TEXT,
    parent_version INTEGER,
    parent_snapshot_hash TEXT,
    snapshot_hash TEXT,
    at_ms INTEGER NOT NULL,
    CHECK (
        (kind = 'created'
            AND case_id IS NULL AND payload IS NULL AND snapshot_hash IS NULL
            AND (
                (parent_lineage_id IS NULL AND parent_version IS NULL AND parent_snapshot_hash IS NULL)
                OR (parent_lineage_id IS NOT NULL AND parent_version >= 1
                    AND parent_snapshot_hash IS NOT NULL AND length(parent_snapshot_hash) = 64)
            ))
        OR (kind = 'case_set'
            AND case_id IS NOT NULL AND length(case_id) > 0
            AND payload IS NOT NULL AND length(payload) > 0
            AND parent_lineage_id IS NULL AND parent_version IS NULL
            AND parent_snapshot_hash IS NULL AND snapshot_hash IS NULL)
        OR (kind = 'case_remove'
            AND case_id IS NOT NULL AND length(case_id) > 0
            AND payload IS NULL
            AND parent_lineage_id IS NULL AND parent_version IS NULL
            AND parent_snapshot_hash IS NULL AND snapshot_hash IS NULL)
        OR (kind = 'frozen'
            AND snapshot_hash IS NOT NULL AND length(snapshot_hash) = 64
            AND case_id IS NULL AND payload IS NULL
            AND parent_lineage_id IS NULL AND parent_version IS NULL AND parent_snapshot_hash IS NULL)
        OR (kind = 'retired'
            AND case_id IS NULL AND payload IS NULL AND snapshot_hash IS NULL
            AND parent_lineage_id IS NULL AND parent_version IS NULL AND parent_snapshot_hash IS NULL)
    )
);
CREATE TABLE recipe_event (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    recipe_id TEXT NOT NULL CHECK (length(recipe_id) > 0),
    body TEXT NOT NULL CHECK (length(body) > 0),
    at_ms INTEGER NOT NULL
);
CREATE TABLE run_open (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL UNIQUE,
    lineage_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version >= 1),
    snapshot_hash TEXT NOT NULL CHECK (length(snapshot_hash) = 64),
    subject_id TEXT NOT NULL CHECK (length(subject_id) > 0),
    subject_version TEXT NOT NULL CHECK (length(subject_version) > 0),
    budget_ms INTEGER NOT NULL CHECK (budget_ms > 0),
    started_at_ms INTEGER NOT NULL
);
CREATE TABLE run_close (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL UNIQUE REFERENCES run_open(run_id),
    outcome TEXT NOT NULL CHECK (outcome IN ('passed','failed','halted')),
    failure_code TEXT,
    overlap TEXT,
    recorded_at_ms INTEGER NOT NULL,
    CHECK (
        (outcome = 'passed' AND failure_code IS NULL AND overlap IS NULL)
        OR (outcome = 'failed' AND failure_code IN ('E_INFRA','E_SUBJECT','E_BENCH','E_TIMEOUT')
            AND overlap IS NULL)
        OR (outcome = 'halted' AND failure_code IS NULL AND overlap IS NOT NULL AND length(overlap) > 0)
    )
);
";

/// 首启建表，或重入且声明一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenStatus {
    /// 空库：建表、写声明、装上只追加触发器。
    Created,
    /// 声明、表和触发器都与本次一致。
    AlreadyDeclared,
}

/// 文件上的持久化配方。`journal_mode` 在文件库是 `wal`，内存库是 `memory`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Durability {
    pub journal_mode: String,
    pub synchronous: i64,
    pub foreign_keys: i64,
    pub busy_timeout_ms: i64,
    pub user_version: i64,
}

/// 当前配方。运行视图里没有这一型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    pub recipe_id: String,
    pub body: String,
    pub recorded_at_ms: i64,
}

/// fork 时钉住的上一版。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentPin {
    pub lineage_id: String,
    pub version: u64,
    pub snapshot_hash: String,
}

/// 重放后的基准。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Benchmark {
    pub lineage_id: String,
    pub version: u64,
    pub phase: Phase,
    pub cases: Vec<BenchCase>,
    pub parent: Option<ParentPin>,
    pub snapshot_hash: Option<String>,
    pub created_at_ms: i64,
    pub frozen_at_ms: Option<i64>,
    pub retired_at_ms: Option<i64>,
}

/// 一次评测的输入。`input` 与冻结快照的 payload 做精确交集。
#[derive(Debug, Clone, Copy)]
pub struct RunSpec<'a> {
    pub run_id: &'a str,
    pub lineage_id: &'a str,
    pub version: u64,
    pub subject_id: &'a str,
    pub subject_version: &'a str,
    pub input: &'a [String],
    pub budget_ms: i64,
}

/// 运行允许看到的东西：冻结快照和被评对象版本。没有当前配方。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunView {
    pub run_id: String,
    pub snapshot: FrozenSnapshot,
    pub subject_id: String,
    pub subject_version: String,
    pub budget_ms: i64,
    pub started_at_ms: i64,
}

impl RunView {
    /// 盲评。视图上没有配方，询问也拒绝，避免只读旁路。
    pub fn current_recipe(&self) -> Result<Recipe> {
        Err(EvalError::InformationBarrier {
            run_id: self.run_id.clone(),
        })
    }
}

/// 账本上的一条运行。进行中的运行还没有 `recorded_at_ms`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    pub run_id: String,
    pub lineage_id: String,
    pub version: u64,
    pub snapshot_hash: String,
    pub subject_id: String,
    pub subject_version: String,
    pub budget_ms: i64,
    pub started_at_ms: i64,
    pub verdict: RunVerdict,
    pub recorded_at_ms: Option<i64>,
}

/// 运行结果。污染停机与四个失败码分开。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunVerdict {
    InProgress,
    Passed,
    Failed(FailureCode),
    Halted { overlap: Vec<String> },
}

struct BenchInner {
    phase: Phase,
    cases: BTreeMap<String, String>,
    parent: Option<ParentPin>,
    snapshot_hash: Option<String>,
    created_at_ms: i64,
    frozen_at_ms: Option<i64>,
    retired_at_ms: Option<i64>,
}

struct CloseInner {
    verdict: RunVerdict,
    recorded_at_ms: i64,
}

struct RunInner {
    lineage_id: String,
    version: u64,
    snapshot_hash: String,
    subject_id: String,
    subject_version: String,
    budget_ms: i64,
    started_at_ms: i64,
    close: Option<CloseInner>,
}

struct State {
    benches: BTreeMap<String, BTreeMap<u64, BenchInner>>,
    runs: BTreeMap<String, RunInner>,
    recipes: Vec<Recipe>,
}

struct BenchEvent {
    lineage_id: String,
    version: i64,
    kind: String,
    case_id: Option<String>,
    payload: Option<String>,
    parent_lineage_id: Option<String>,
    parent_version: Option<i64>,
    parent_snapshot_hash: Option<String>,
    snapshot_hash: Option<String>,
    at_ms: i64,
}

struct OpenRow {
    run_id: String,
    lineage_id: String,
    version: i64,
    snapshot_hash: String,
    subject_id: String,
    subject_version: String,
    budget_ms: i64,
    started_at_ms: i64,
}

struct CloseRow {
    run_id: String,
    outcome: String,
    failure_code: Option<String>,
    overlap: Option<String>,
    recorded_at_ms: i64,
}

enum Begin {
    Opened(RunView),
    Halted {
        run_id: String,
        overlap: Vec<String>,
    },
}

/// 一份评测账本连接。
pub struct Catalog {
    conn: Connection,
    clock: Arc<dyn Clock>,
    ledger_id: String,
}

impl std::fmt::Debug for Catalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Catalog")
            .field("ledger_id", &self.ledger_id)
            .finish_non_exhaustive()
    }
}

impl Catalog {
    /// 打开或创建文件账本。时刻用 [`SystemClock`]。
    pub fn open(path: impl AsRef<Path>, ledger_id: &str) -> Result<(Self, OpenStatus)> {
        Self::open_with_clock(path, ledger_id, SystemClock::new())
    }

    /// 同 [`Catalog::open`]，落盘时刻走 `clock`。
    pub fn open_with_clock(
        path: impl AsRef<Path>,
        ledger_id: &str,
        clock: impl Clock + 'static,
    ) -> Result<(Self, OpenStatus)> {
        let conn = Connection::open(path)?;
        Self::from_conn(conn, ledger_id, Arc::new(clock), false)
    }

    /// 内存账本。没有跨进程恢复的意义，返回 [`OpenStatus::Created`]。
    pub fn open_in_memory(ledger_id: &str) -> Result<(Self, OpenStatus)> {
        Self::open_in_memory_with_clock(ledger_id, SystemClock::new())
    }

    /// 同 [`Catalog::open_in_memory`]，时刻走 `clock`。
    pub fn open_in_memory_with_clock(
        ledger_id: &str,
        clock: impl Clock + 'static,
    ) -> Result<(Self, OpenStatus)> {
        let conn = Connection::open_in_memory()?;
        Self::from_conn(conn, ledger_id, Arc::new(clock), true)
    }

    fn from_conn(
        mut conn: Connection,
        ledger_id: &str,
        clock: Arc<dyn Clock>,
        memory: bool,
    ) -> Result<(Self, OpenStatus)> {
        require_ident("ledger_id", ledger_id)?;
        apply_pragmas(&conn, memory)?;
        let status = init_schema(&mut conn, ledger_id)?;
        Ok((
            Self {
                conn,
                clock,
                ledger_id: ledger_id.to_owned(),
            },
            status,
        ))
    }

    pub fn ledger_id(&self) -> &str {
        &self.ledger_id
    }

    /// 读本连接上的持久化配方。
    pub fn durability(&self) -> Result<Durability> {
        read_durability(&self.conn)
    }

    /// 新建谱系的 version 1，阶段是 draft。
    pub fn create_benchmark(&mut self, lineage_id: &str) -> Result<Benchmark> {
        require_ident("lineage_id", lineage_id)?;
        let lineage = lineage_id.to_owned();
        self.commit_write(|tx, now, state| {
            if state.benches.contains_key(&lineage) {
                return Err(EvalError::LineageExists {
                    lineage: lineage.clone(),
                });
            }
            insert_bench(
                tx,
                &BenchInsert {
                    lineage_id: &lineage,
                    version: 1,
                    kind: "created",
                    case_id: None,
                    payload: None,
                    parent_lineage_id: None,
                    parent_version: None,
                    parent_snapshot_hash: None,
                    snapshot_hash: None,
                    at_ms: now,
                },
            )?;
            Ok(())
        })?;
        self.benchmark(&lineage, 1)
    }

    /// 在 draft 上追加一道用例。同名用例以后一次 payload 为准。冻结或退役后拒绝。
    pub fn put_case(
        &mut self,
        lineage_id: &str,
        version: u64,
        case_id: &str,
        payload: &str,
    ) -> Result<Benchmark> {
        require_ident("lineage_id", lineage_id)?;
        require_ident("case_id", case_id)?;
        require_body("payload", payload, MAX_BODY_LEN)?;
        let version_i64 = require_version(version)?;
        let lineage = lineage_id.to_owned();
        let case_id = case_id.to_owned();
        let payload = payload.to_owned();
        self.commit_write(|tx, now, state| {
            let bench = bench_ref(state, &lineage, version)?;
            if bench.phase != Phase::Draft {
                return Err(sealed(&lineage, version, bench.phase));
            }
            insert_bench(
                tx,
                &BenchInsert {
                    lineage_id: &lineage,
                    version: version_i64,
                    kind: "case_set",
                    case_id: Some(&case_id),
                    payload: Some(&payload),
                    parent_lineage_id: None,
                    parent_version: None,
                    parent_snapshot_hash: None,
                    snapshot_hash: None,
                    at_ms: now,
                },
            )?;
            Ok(())
        })?;
        self.benchmark(&lineage, version)
    }

    /// 从 draft 去掉一道用例。冻结或退役后拒绝。
    pub fn remove_case(
        &mut self,
        lineage_id: &str,
        version: u64,
        case_id: &str,
    ) -> Result<Benchmark> {
        require_ident("lineage_id", lineage_id)?;
        require_ident("case_id", case_id)?;
        let version_i64 = require_version(version)?;
        let lineage = lineage_id.to_owned();
        let case_id = case_id.to_owned();
        self.commit_write(|tx, now, state| {
            let bench = bench_ref(state, &lineage, version)?;
            if bench.phase != Phase::Draft {
                return Err(sealed(&lineage, version, bench.phase));
            }
            if !bench.cases.contains_key(&case_id) {
                return Err(EvalError::CaseMissing {
                    case_id: case_id.clone(),
                });
            }
            insert_bench(
                tx,
                &BenchInsert {
                    lineage_id: &lineage,
                    version: version_i64,
                    kind: "case_remove",
                    case_id: Some(&case_id),
                    payload: None,
                    parent_lineage_id: None,
                    parent_version: None,
                    parent_snapshot_hash: None,
                    snapshot_hash: None,
                    at_ms: now,
                },
            )?;
            Ok(())
        })?;
        self.benchmark(&lineage, version)
    }

    /// draft → frozen。至少一个用例。快照哈希按规范字节计算后封存。
    pub fn freeze(&mut self, lineage_id: &str, version: u64) -> Result<Benchmark> {
        require_ident("lineage_id", lineage_id)?;
        let version_i64 = require_version(version)?;
        let lineage = lineage_id.to_owned();
        self.commit_write(|tx, now, state| {
            let bench = bench_ref(state, &lineage, version)?;
            if bench.phase != Phase::Draft {
                return Err(EvalError::IllegalTransition {
                    phase: bench.phase,
                    action: "冻结",
                });
            }
            if bench.cases.is_empty() {
                return Err(EvalError::EmptyBenchmark);
            }
            let cases = cases_of(bench);
            let hash =
                snapshot_hash(&lineage, version, &cases).map_err(|err| EvalError::Corrupt {
                    detail: format!("冻结时无法计算快照哈希: {err}"),
                })?;
            insert_bench(
                tx,
                &BenchInsert {
                    lineage_id: &lineage,
                    version: version_i64,
                    kind: "frozen",
                    case_id: None,
                    payload: None,
                    parent_lineage_id: None,
                    parent_version: None,
                    parent_snapshot_hash: None,
                    snapshot_hash: Some(&hash),
                    at_ms: now,
                },
            )?;
            Ok(())
        })?;
        self.benchmark(&lineage, version)
    }

    /// frozen → retired。draft 不能跳过冻结。
    pub fn retire(&mut self, lineage_id: &str, version: u64) -> Result<Benchmark> {
        require_ident("lineage_id", lineage_id)?;
        let version_i64 = require_version(version)?;
        let lineage = lineage_id.to_owned();
        self.commit_write(|tx, now, state| {
            let bench = bench_ref(state, &lineage, version)?;
            if bench.phase != Phase::Frozen {
                return Err(EvalError::IllegalTransition {
                    phase: bench.phase,
                    action: "退役",
                });
            }
            insert_bench(
                tx,
                &BenchInsert {
                    lineage_id: &lineage,
                    version: version_i64,
                    kind: "retired",
                    case_id: None,
                    payload: None,
                    parent_lineage_id: None,
                    parent_version: None,
                    parent_snapshot_hash: None,
                    snapshot_hash: None,
                    at_ms: now,
                },
            )?;
            Ok(())
        })?;
        self.benchmark(&lineage, version)
    }

    /// 从已封存的最新版 fork 出下一版 draft，用例原样复制。draft 不能 fork。
    pub fn fork(&mut self, lineage_id: &str, version: u64) -> Result<Benchmark> {
        require_ident("lineage_id", lineage_id)?;
        require_version(version)?;
        let lineage = lineage_id.to_owned();
        let new_version = self.commit_write(|tx, now, state| {
            let bench = bench_ref(state, &lineage, version)?;
            if bench.phase == Phase::Draft {
                return Err(EvalError::IllegalTransition {
                    phase: bench.phase,
                    action: "fork",
                });
            }
            let head = head_version(state, &lineage)?;
            if head != version {
                return Err(EvalError::IllegalTransition {
                    phase: bench.phase,
                    action: "fork 非最新版",
                });
            }
            let new_version = version
                .checked_add(1)
                .ok_or_else(|| EvalError::invalid("version"))?;
            let new_version_i64 = require_version(new_version)?;
            let parent_hash = bench
                .snapshot_hash
                .clone()
                .ok_or_else(|| EvalError::Corrupt {
                    detail: format!("{lineage}@{version} 已封存但没有快照哈希"),
                })?;
            let copied = cases_of(bench);
            insert_bench(
                tx,
                &BenchInsert {
                    lineage_id: &lineage,
                    version: new_version_i64,
                    kind: "created",
                    case_id: None,
                    payload: None,
                    parent_lineage_id: Some(&lineage),
                    parent_version: Some(require_version(version)?),
                    parent_snapshot_hash: Some(&parent_hash),
                    snapshot_hash: None,
                    at_ms: now,
                },
            )?;
            for case in &copied {
                insert_bench(
                    tx,
                    &BenchInsert {
                        lineage_id: &lineage,
                        version: new_version_i64,
                        kind: "case_set",
                        case_id: Some(&case.case_id),
                        payload: Some(&case.payload),
                        parent_lineage_id: None,
                        parent_version: None,
                        parent_snapshot_hash: None,
                        snapshot_hash: None,
                        at_ms: now,
                    },
                )?;
            }
            Ok(new_version)
        })?;
        self.benchmark(&lineage, new_version)
    }

    pub fn benchmark(&self, lineage_id: &str, version: u64) -> Result<Benchmark> {
        require_ident("lineage_id", lineage_id)?;
        require_version(version)?;
        let state = load_state(&self.conn)?;
        let bench = bench_ref(&state, lineage_id, version)?;
        Ok(to_benchmark(lineage_id, version, bench))
    }

    /// 追加当前配方。旧配方行保留。评测运行读不到这里。
    pub fn set_current_recipe(&mut self, recipe_id: &str, body: &str) -> Result<Recipe> {
        require_ident("recipe_id", recipe_id)?;
        require_body("recipe_body", body, MAX_BODY_LEN)?;
        let recipe_id = recipe_id.to_owned();
        let body = body.to_owned();
        self.commit_write(|tx, now, _state| {
            tx.execute(
                "INSERT INTO recipe_event (recipe_id, body, at_ms) VALUES (?1, ?2, ?3)",
                params![recipe_id, body, now],
            )?;
            Ok(())
        })?;
        self.current_recipe()
    }

    /// 账本外的读取。运行中的盲评不走这个方法。
    pub fn current_recipe(&self) -> Result<Recipe> {
        let state = load_state(&self.conn)?;
        state
            .recipes
            .last()
            .cloned()
            .ok_or(EvalError::RecipeMissing)
    }

    /// 绑定冻结快照。输入与基准集相交则写停机行并返回错误，不给成绩。
    pub fn begin_run(&mut self, spec: &RunSpec<'_>) -> Result<RunView> {
        require_ident("run_id", spec.run_id)?;
        require_ident("lineage_id", spec.lineage_id)?;
        require_ident("subject_id", spec.subject_id)?;
        require_ident("subject_version", spec.subject_version)?;
        let version_i64 = require_version(spec.version)?;
        require_budget(spec.budget_ms)?;
        let spec_owned = OwnedSpec {
            run_id: spec.run_id.to_owned(),
            lineage_id: spec.lineage_id.to_owned(),
            version: spec.version,
            version_i64,
            subject_id: spec.subject_id.to_owned(),
            subject_version: spec.subject_version.to_owned(),
            input: spec.input.to_vec(),
            budget_ms: spec.budget_ms,
        };
        let begun = self.commit_write(|tx, now, state| begin_in_tx(tx, now, state, &spec_owned))?;
        match begun {
            Begin::Opened(view) => Ok(view),
            Begin::Halted { run_id, overlap } => Err(EvalError::Contaminated { run_id, overlap }),
        }
    }

    /// 运行路径上的配方读取。运行存在就拒绝，不区分配方有没有登记。
    pub fn recipe_during_run(&self, run_id: &str) -> Result<Recipe> {
        require_ident("run_id", run_id)?;
        let state = load_state(&self.conn)?;
        if !state.runs.contains_key(run_id) {
            return Err(EvalError::RunMissing {
                run_id: run_id.to_owned(),
            });
        }
        Err(EvalError::InformationBarrier {
            run_id: run_id.to_owned(),
        })
    }

    /// 运行能读到的冻结快照。哈希必须与开跑时绑定的一致。
    pub fn run_view(&self, run_id: &str) -> Result<RunView> {
        require_ident("run_id", run_id)?;
        let state = load_state(&self.conn)?;
        view_of(&state, run_id)
    }

    /// 按注入时刻路由四个失败码，并把结果追加到账本。
    pub fn finish_run(&mut self, run_id: &str, signals: ExecSignals) -> Result<RunRecord> {
        require_ident("run_id", run_id)?;
        let run_id = run_id.to_owned();
        self.commit_write(|tx, now, state| {
            let run = state
                .runs
                .get(&run_id)
                .ok_or_else(|| EvalError::RunMissing {
                    run_id: run_id.clone(),
                })?;
            if run.close.is_some() {
                return Err(EvalError::RunClosed {
                    run_id: run_id.clone(),
                });
            }
            confirm_snapshot(state, run)?;
            let class = route_failure(run.started_at_ms, now, run.budget_ms, signals)?;
            let (outcome, failure_code) = match class {
                Classified::Passed => ("passed", None),
                Classified::Failed(code) => ("failed", Some(code.as_code())),
            };
            tx.execute(
                "INSERT INTO run_close (run_id, outcome, failure_code, overlap, recorded_at_ms)
                 VALUES (?1, ?2, ?3, NULL, ?4)",
                params![run_id, outcome, failure_code, now],
            )?;
            Ok(())
        })?;
        self.run_record(&run_id)
    }

    pub fn run_record(&self, run_id: &str) -> Result<RunRecord> {
        require_ident("run_id", run_id)?;
        let state = load_state(&self.conn)?;
        let run = state
            .runs
            .get(run_id)
            .ok_or_else(|| EvalError::RunMissing {
                run_id: run_id.to_owned(),
            })?;
        Ok(to_record(run_id, run))
    }

    /// 试图改写已绑定的被评对象版本。只追加账本拒绝，行保持原样。
    pub fn update_run_subject_version(
        &mut self,
        run_id: &str,
        subject_version: &str,
    ) -> Result<()> {
        require_ident("run_id", run_id)?;
        require_ident("subject_version", subject_version)?;
        self.reject_mutation(
            |state| {
                if state.runs.contains_key(run_id) {
                    Ok(())
                } else {
                    Err(EvalError::RunMissing {
                        run_id: run_id.to_owned(),
                    })
                }
            },
            "UPDATE run_open SET subject_version = ?1 WHERE run_id = ?2",
            params![subject_version, run_id],
        )
    }

    /// 试图改写已落盘的结果。没有结果行时返回 [`EvalError::RunOpen`]。
    pub fn update_run_outcome(&mut self, run_id: &str) -> Result<()> {
        require_ident("run_id", run_id)?;
        self.reject_mutation(
            |state| match state.runs.get(run_id) {
                None => Err(EvalError::RunMissing {
                    run_id: run_id.to_owned(),
                }),
                Some(run) if run.close.is_none() => Err(EvalError::RunOpen {
                    run_id: run_id.to_owned(),
                }),
                Some(_) => Ok(()),
            },
            "UPDATE run_close
             SET outcome = 'passed', failure_code = NULL, overlap = NULL
             WHERE run_id = ?1",
            params![run_id],
        )
    }

    /// 试图删掉运行行。有结果就删结果，否则删绑定。两者都被触发器拒绝。
    pub fn delete_run(&mut self, run_id: &str) -> Result<()> {
        require_ident("run_id", run_id)?;
        let sql = {
            let state = load_state(&self.conn)?;
            match state.runs.get(run_id) {
                None => {
                    return Err(EvalError::RunMissing {
                        run_id: run_id.to_owned(),
                    });
                }
                Some(run) if run.close.is_some() => "DELETE FROM run_close WHERE run_id = ?1",
                Some(_) => "DELETE FROM run_open WHERE run_id = ?1",
            }
        };
        self.reject_mutation(|_| Ok(()), sql, params![run_id])
    }

    fn commit_write<T>(
        &mut self,
        body: impl FnOnce(&Transaction<'_>, i64, &State) -> Result<T>,
    ) -> Result<T> {
        let now = self.clock.now_ms();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let state = load_state(&tx)?;
        let value = body(&tx, now, &state)?;
        tx.commit()?;
        Ok(value)
    }

    fn reject_mutation(
        &mut self,
        guard: impl FnOnce(&State) -> Result<()>,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let state = load_state(&tx)?;
        guard(&state)?;
        let outcome = match tx.execute(sql, params) {
            Ok(0) => Err(EvalError::Corrupt {
                detail: "改写没有命中行，也没有被触发器拒绝".into(),
            }),
            Ok(_) => Err(EvalError::Corrupt {
                detail: "只追加触发器没有挡住改写".into(),
            }),
            Err(err) if is_append_only(&err) => Err(EvalError::AppendOnly),
            Err(err) => Err(err.into()),
        };
        drop(tx);
        outcome
    }
}

struct OwnedSpec {
    run_id: String,
    lineage_id: String,
    version: u64,
    version_i64: i64,
    subject_id: String,
    subject_version: String,
    input: Vec<String>,
    budget_ms: i64,
}

struct BenchInsert<'a> {
    lineage_id: &'a str,
    version: i64,
    kind: &'a str,
    case_id: Option<&'a str>,
    payload: Option<&'a str>,
    parent_lineage_id: Option<&'a str>,
    parent_version: Option<i64>,
    parent_snapshot_hash: Option<&'a str>,
    snapshot_hash: Option<&'a str>,
    at_ms: i64,
}

fn begin_in_tx(tx: &Transaction<'_>, now: i64, state: &State, spec: &OwnedSpec) -> Result<Begin> {
    if state.runs.contains_key(&spec.run_id) {
        return Err(EvalError::RunExists {
            run_id: spec.run_id.clone(),
        });
    }
    let bench = bench_ref(state, &spec.lineage_id, spec.version)?;
    if bench.phase != Phase::Frozen {
        return Err(EvalError::NotFrozen {
            lineage: spec.lineage_id.clone(),
            version: spec.version,
            phase: bench.phase,
        });
    }
    let cases = cases_of(bench);
    let hash = bench
        .snapshot_hash
        .clone()
        .ok_or_else(|| EvalError::Corrupt {
            detail: format!(
                "{}@{} 处于 frozen 但没有快照哈希",
                spec.lineage_id, spec.version
            ),
        })?;
    let overlap = contamination_overlap(&cases, &spec.input);
    insert_open(tx, spec, &hash, now)?;
    if overlap.is_empty() {
        return Ok(Begin::Opened(RunView {
            run_id: spec.run_id.clone(),
            snapshot: FrozenSnapshot {
                lineage_id: spec.lineage_id.clone(),
                version: spec.version,
                cases,
                hash,
            },
            subject_id: spec.subject_id.clone(),
            subject_version: spec.subject_version.clone(),
            budget_ms: spec.budget_ms,
            started_at_ms: now,
        }));
    }
    let encoded = encode_list(&overlap);
    tx.execute(
        "INSERT INTO run_close (run_id, outcome, failure_code, overlap, recorded_at_ms)
         VALUES (?1, 'halted', NULL, ?2, ?3)",
        params![spec.run_id, encoded, now],
    )?;
    Ok(Begin::Halted {
        run_id: spec.run_id.clone(),
        overlap,
    })
}

fn insert_open(tx: &Transaction<'_>, spec: &OwnedSpec, hash: &str, now: i64) -> Result<()> {
    tx.execute(
        "INSERT INTO run_open (
            run_id, lineage_id, version, snapshot_hash, subject_id, subject_version,
            budget_ms, started_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            spec.run_id,
            spec.lineage_id,
            spec.version_i64,
            hash,
            spec.subject_id,
            spec.subject_version,
            spec.budget_ms,
            now
        ],
    )
    .map_err(|err| map_unique(err, &spec.run_id))?;
    Ok(())
}

fn insert_bench(tx: &Transaction<'_>, row: &BenchInsert<'_>) -> Result<()> {
    tx.execute(
        "INSERT INTO bench_event (
            lineage_id, version, kind, case_id, payload, parent_lineage_id, parent_version,
            parent_snapshot_hash, snapshot_hash, at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            row.lineage_id,
            row.version,
            row.kind,
            row.case_id,
            row.payload,
            row.parent_lineage_id,
            row.parent_version,
            row.parent_snapshot_hash,
            row.snapshot_hash,
            row.at_ms
        ],
    )?;
    Ok(())
}

fn confirm_snapshot(state: &State, run: &RunInner) -> Result<()> {
    let bench = bench_ref(state, &run.lineage_id, run.version)?;
    match bench.snapshot_hash.as_deref() {
        Some(hash) if hash == run.snapshot_hash => Ok(()),
        _ => Err(EvalError::Corrupt {
            detail: format!(
                "运行绑定的快照哈希与基准 {}@{} 不一致",
                run.lineage_id, run.version
            ),
        }),
    }
}

fn view_of(state: &State, run_id: &str) -> Result<RunView> {
    let run = state
        .runs
        .get(run_id)
        .ok_or_else(|| EvalError::RunMissing {
            run_id: run_id.to_owned(),
        })?;
    confirm_snapshot(state, run)?;
    let bench = bench_ref(state, &run.lineage_id, run.version)?;
    Ok(RunView {
        run_id: run_id.to_owned(),
        snapshot: FrozenSnapshot {
            lineage_id: run.lineage_id.clone(),
            version: run.version,
            cases: cases_of(bench),
            hash: run.snapshot_hash.clone(),
        },
        subject_id: run.subject_id.clone(),
        subject_version: run.subject_version.clone(),
        budget_ms: run.budget_ms,
        started_at_ms: run.started_at_ms,
    })
}

fn to_record(run_id: &str, run: &RunInner) -> RunRecord {
    let (verdict, recorded_at_ms) = match &run.close {
        None => (RunVerdict::InProgress, None),
        Some(close) => (close.verdict.clone(), Some(close.recorded_at_ms)),
    };
    RunRecord {
        run_id: run_id.to_owned(),
        lineage_id: run.lineage_id.clone(),
        version: run.version,
        snapshot_hash: run.snapshot_hash.clone(),
        subject_id: run.subject_id.clone(),
        subject_version: run.subject_version.clone(),
        budget_ms: run.budget_ms,
        started_at_ms: run.started_at_ms,
        verdict,
        recorded_at_ms,
    }
}

fn to_benchmark(lineage_id: &str, version: u64, bench: &BenchInner) -> Benchmark {
    Benchmark {
        lineage_id: lineage_id.to_owned(),
        version,
        phase: bench.phase,
        cases: cases_of(bench),
        parent: bench.parent.clone(),
        snapshot_hash: bench.snapshot_hash.clone(),
        created_at_ms: bench.created_at_ms,
        frozen_at_ms: bench.frozen_at_ms,
        retired_at_ms: bench.retired_at_ms,
    }
}

fn cases_of(bench: &BenchInner) -> Vec<BenchCase> {
    bench
        .cases
        .iter()
        .map(|(case_id, payload)| BenchCase {
            case_id: case_id.clone(),
            payload: payload.clone(),
        })
        .collect()
}

fn bench_ref<'a>(state: &'a State, lineage_id: &str, version: u64) -> Result<&'a BenchInner> {
    state
        .benches
        .get(lineage_id)
        .and_then(|versions| versions.get(&version))
        .ok_or_else(|| EvalError::BenchmarkMissing {
            lineage: lineage_id.to_owned(),
            version,
        })
}

fn head_version(state: &State, lineage_id: &str) -> Result<u64> {
    state
        .benches
        .get(lineage_id)
        .and_then(|versions| versions.keys().next_back().copied())
        .ok_or_else(|| EvalError::BenchmarkMissing {
            lineage: lineage_id.to_owned(),
            version: 0,
        })
}

fn sealed(lineage: &str, version: u64, phase: Phase) -> EvalError {
    EvalError::Sealed {
        lineage: lineage.to_owned(),
        version,
        phase,
    }
}

fn init_schema(conn: &mut Connection, ledger_id: &str) -> Result<OpenStatus> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version = user_version(&tx)?;
    let tables = table_names(&tx)?;
    let triggers = trigger_names(&tx)?;
    if version == 0 {
        if !tables.is_empty() || !triggers.is_empty() {
            return Err(EvalError::TornInitialization);
        }
        let ddl = format!("{DDL}{}", guard_sql());
        tx.execute_batch(&ddl)?;
        tx.execute(
            "INSERT INTO schema_meta (id, schema_version, ledger_id) VALUES (1, ?1, ?2)",
            params![SCHEMA_VERSION, ledger_id],
        )?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        ensure_names(&table_names(&tx)?, &expected_tables(), "表")?;
        ensure_names(&trigger_names(&tx)?, &expected_triggers(), "触发器")?;
        tx.commit()?;
        return Ok(OpenStatus::Created);
    }
    if version != SCHEMA_VERSION {
        return Err(EvalError::UnknownSchema {
            found: version,
            expected: SCHEMA_VERSION,
        });
    }
    ensure_names(&tables, &expected_tables(), "表")?;
    ensure_names(&triggers, &expected_triggers(), "触发器")?;
    let stored = read_ledger_id(&tx)?;
    if stored != ledger_id {
        return Err(EvalError::DeclarationMismatch {
            detail: format!("ledger_id 已落盘为 {stored}"),
        });
    }
    let declared: i64 = tx.query_row(
        "SELECT schema_version FROM schema_meta WHERE id = 1",
        [],
        |row| row.get(0),
    )?;
    if declared != SCHEMA_VERSION {
        return Err(EvalError::DeclarationMismatch {
            detail: format!("schema_meta.schema_version = {declared}"),
        });
    }
    load_state(&tx)?;
    tx.rollback()?;
    Ok(OpenStatus::AlreadyDeclared)
}

fn read_ledger_id(conn: &Connection) -> Result<String> {
    conn.query_row(
        "SELECT ledger_id FROM schema_meta WHERE id = 1",
        [],
        |row| row.get(0),
    )
    .map_err(|err| match err {
        rusqlite::Error::QueryReturnedNoRows => EvalError::Corrupt {
            detail: "schema_meta 缺少声明行".into(),
        },
        other => EvalError::Sqlite(other),
    })
}

fn apply_pragmas(conn: &Connection, memory: bool) -> Result<()> {
    conn.busy_timeout(BUSY_TIMEOUT)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    let recipe = read_durability(conn)?;
    let mode_ok = recipe.journal_mode == "wal" || (memory && recipe.journal_mode == "memory");
    if !mode_ok {
        return Err(EvalError::Corrupt {
            detail: format!("journal_mode 不是 WAL: {}", recipe.journal_mode),
        });
    }
    if recipe.synchronous != 1 {
        return Err(EvalError::Corrupt {
            detail: format!("synchronous 不是 NORMAL: {}", recipe.synchronous),
        });
    }
    if recipe.foreign_keys != 1 {
        return Err(EvalError::Corrupt {
            detail: format!("foreign_keys 未打开: {}", recipe.foreign_keys),
        });
    }
    if recipe.busy_timeout_ms != BUSY_TIMEOUT_MS {
        return Err(EvalError::Corrupt {
            detail: format!("busy_timeout 不是 5000ms: {}", recipe.busy_timeout_ms),
        });
    }
    Ok(())
}

fn read_durability(conn: &Connection) -> Result<Durability> {
    let journal_mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    let synchronous: i64 = conn.pragma_query_value(None, "synchronous", |row| row.get(0))?;
    let foreign_keys: i64 = conn.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    let busy_timeout_ms: i64 = conn.pragma_query_value(None, "busy_timeout", |row| row.get(0))?;
    let user_version = user_version(conn)?;
    Ok(Durability {
        journal_mode: journal_mode.to_ascii_lowercase(),
        synchronous,
        foreign_keys,
        busy_timeout_ms,
        user_version,
    })
}

fn user_version(conn: &Connection) -> Result<i64> {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(EvalError::from)
}

fn table_names(conn: &Connection) -> Result<Vec<String>> {
    names_from(
        conn,
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
         ORDER BY name",
    )
}

fn trigger_names(conn: &Connection) -> Result<Vec<String>> {
    names_from(
        conn,
        "SELECT name FROM sqlite_master WHERE type = 'trigger' ORDER BY name",
    )
}

fn names_from(conn: &Connection, sql: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    let mut names = Vec::new();
    for name in rows {
        names.push(name?);
    }
    Ok(names)
}

fn expected_tables() -> Vec<String> {
    let mut names: Vec<String> = TABLES.iter().map(|name| (*name).to_owned()).collect();
    names.sort();
    names
}

fn expected_triggers() -> Vec<String> {
    let mut names = Vec::new();
    for table in TABLES {
        names.push(format!("{table}_no_delete"));
        names.push(format!("{table}_no_update"));
    }
    names.sort();
    names
}

fn guard_sql() -> String {
    let mut sql = String::new();
    for table in TABLES {
        sql.push_str(&format!(
            "CREATE TRIGGER {table}_no_update BEFORE UPDATE ON {table}
             BEGIN SELECT RAISE(ABORT, 'append-only'); END;
             CREATE TRIGGER {table}_no_delete BEFORE DELETE ON {table}
             BEGIN SELECT RAISE(ABORT, 'append-only'); END;"
        ));
    }
    sql
}

fn ensure_names(found: &[String], expected: &[String], label: &str) -> Result<()> {
    if found == expected {
        Ok(())
    } else {
        Err(EvalError::SchemaDrift {
            detail: format!("{label} 实际 {found:?}，声明 {expected:?}"),
        })
    }
}

fn load_state(conn: &Connection) -> Result<State> {
    let mut state = State {
        benches: BTreeMap::new(),
        runs: BTreeMap::new(),
        recipes: Vec::new(),
    };
    for event in load_bench_events(conn)? {
        apply_bench(&mut state, event)?;
    }
    for recipe in load_recipes(conn)? {
        state.recipes.push(recipe);
    }
    for row in load_opens(conn)? {
        apply_open(&mut state, row)?;
    }
    for row in load_closes(conn)? {
        apply_close(&mut state, row)?;
    }
    Ok(state)
}

fn load_bench_events(conn: &Connection) -> Result<Vec<BenchEvent>> {
    let mut stmt = conn.prepare(
        "SELECT lineage_id, version, kind, case_id, payload, parent_lineage_id, parent_version,
                parent_snapshot_hash, snapshot_hash, at_ms
         FROM bench_event ORDER BY seq",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(BenchEvent {
            lineage_id: row.get(0)?,
            version: row.get(1)?,
            kind: row.get(2)?,
            case_id: row.get(3)?,
            payload: row.get(4)?,
            parent_lineage_id: row.get(5)?,
            parent_version: row.get(6)?,
            parent_snapshot_hash: row.get(7)?,
            snapshot_hash: row.get(8)?,
            at_ms: row.get(9)?,
        })
    })?;
    collect_rows(rows)
}

fn load_recipes(conn: &Connection) -> Result<Vec<Recipe>> {
    let mut stmt = conn.prepare("SELECT recipe_id, body, at_ms FROM recipe_event ORDER BY seq")?;
    let rows = stmt.query_map([], |row| {
        Ok(Recipe {
            recipe_id: row.get(0)?,
            body: row.get(1)?,
            recorded_at_ms: row.get(2)?,
        })
    })?;
    let recipes = collect_rows(rows)?;
    for recipe in &recipes {
        if recipe.recipe_id.is_empty() || recipe.body.is_empty() {
            return Err(EvalError::Corrupt {
                detail: "配方事件含空字段".into(),
            });
        }
    }
    Ok(recipes)
}

fn load_opens(conn: &Connection) -> Result<Vec<OpenRow>> {
    let mut stmt = conn.prepare(
        "SELECT run_id, lineage_id, version, snapshot_hash, subject_id, subject_version,
                budget_ms, started_at_ms
         FROM run_open ORDER BY seq",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(OpenRow {
            run_id: row.get(0)?,
            lineage_id: row.get(1)?,
            version: row.get(2)?,
            snapshot_hash: row.get(3)?,
            subject_id: row.get(4)?,
            subject_version: row.get(5)?,
            budget_ms: row.get(6)?,
            started_at_ms: row.get(7)?,
        })
    })?;
    collect_rows(rows)
}

fn load_closes(conn: &Connection) -> Result<Vec<CloseRow>> {
    let mut stmt = conn.prepare(
        "SELECT run_id, outcome, failure_code, overlap, recorded_at_ms
         FROM run_close ORDER BY seq",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CloseRow {
            run_id: row.get(0)?,
            outcome: row.get(1)?,
            failure_code: row.get(2)?,
            overlap: row.get(3)?,
            recorded_at_ms: row.get(4)?,
        })
    })?;
    collect_rows(rows)
}

fn collect_rows<T, I>(rows: I) -> Result<Vec<T>>
where
    I: IntoIterator<Item = rusqlite::Result<T>>,
{
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn apply_bench(state: &mut State, event: BenchEvent) -> Result<()> {
    let version = u64::try_from(event.version).map_err(|_| EvalError::Corrupt {
        detail: format!("版本 {} 超出 u64", event.version),
    })?;
    match event.kind.as_str() {
        "created" => apply_created(state, &event, version),
        "case_set" => {
            let bench = bench_mut(state, &event.lineage_id, version)?;
            if bench.phase != Phase::Draft {
                return Err(EvalError::Corrupt {
                    detail: format!("{}@{version} 离开 draft 后仍有用例写入", event.lineage_id),
                });
            }
            let case_id = required_text(event.case_id, "case_set 缺少 case_id")?;
            let payload = required_text(event.payload, "case_set 缺少 payload")?;
            bench.cases.insert(case_id, payload);
            Ok(())
        }
        "case_remove" => {
            let bench = bench_mut(state, &event.lineage_id, version)?;
            if bench.phase != Phase::Draft {
                return Err(EvalError::Corrupt {
                    detail: format!("{}@{version} 离开 draft 后仍有用例删除", event.lineage_id),
                });
            }
            let case_id = required_text(event.case_id, "case_remove 缺少 case_id")?;
            if bench.cases.remove(&case_id).is_none() {
                return Err(EvalError::Corrupt {
                    detail: format!(
                        "{}@{version} 删除了不存在的用例 {case_id}",
                        event.lineage_id
                    ),
                });
            }
            Ok(())
        }
        "frozen" => {
            let hash = required_text(event.snapshot_hash, "frozen 缺少快照哈希")?;
            let bench = bench_mut(state, &event.lineage_id, version)?;
            if bench.phase != Phase::Draft {
                return Err(EvalError::Corrupt {
                    detail: format!("{}@{version} 重复冻结或阶段不对", event.lineage_id),
                });
            }
            if bench.cases.is_empty() {
                return Err(EvalError::Corrupt {
                    detail: format!("{}@{version} 空基准被冻结", event.lineage_id),
                });
            }
            let cases = cases_of(bench);
            let computed = snapshot_hash(&event.lineage_id, version, &cases)?;
            if computed != hash {
                return Err(EvalError::Corrupt {
                    detail: format!(
                        "{}@{version} 快照哈希不符，落盘 {hash}，重算 {computed}",
                        event.lineage_id
                    ),
                });
            }
            bench.phase = Phase::Frozen;
            bench.snapshot_hash = Some(hash);
            bench.frozen_at_ms = Some(event.at_ms);
            Ok(())
        }
        "retired" => {
            let bench = bench_mut(state, &event.lineage_id, version)?;
            if bench.phase != Phase::Frozen {
                return Err(EvalError::Corrupt {
                    detail: format!("{}@{version} 从 {:?} 退役", event.lineage_id, bench.phase),
                });
            }
            bench.phase = Phase::Retired;
            bench.retired_at_ms = Some(event.at_ms);
            Ok(())
        }
        other => Err(EvalError::Corrupt {
            detail: format!("未知基准事件 {other}"),
        }),
    }
}

fn apply_created(state: &mut State, event: &BenchEvent, version: u64) -> Result<()> {
    if state
        .benches
        .get(&event.lineage_id)
        .is_some_and(|versions| versions.contains_key(&version))
    {
        return Err(EvalError::Corrupt {
            detail: format!("重复 created {}@{version}", event.lineage_id),
        });
    }
    let parent = match (
        event.parent_lineage_id.clone(),
        event.parent_version,
        event.parent_snapshot_hash.clone(),
    ) {
        (None, None, None) => {
            if version != 1 || state.benches.contains_key(&event.lineage_id) {
                return Err(EvalError::Corrupt {
                    detail: format!("{}@{version} 不是空谱系的首版", event.lineage_id),
                });
            }
            None
        }
        (Some(parent_lineage), Some(parent_version), Some(parent_hash)) => {
            let parent_version = u64::try_from(parent_version).map_err(|_| EvalError::Corrupt {
                detail: "父版本超出 u64".into(),
            })?;
            if parent_lineage != event.lineage_id || parent_version.saturating_add(1) != version {
                return Err(EvalError::Corrupt {
                    detail: format!("{}@{version} 的 fork 父指针不连续", event.lineage_id),
                });
            }
            let parent = bench_ref(state, &parent_lineage, parent_version)?;
            if parent.phase == Phase::Draft {
                return Err(EvalError::Corrupt {
                    detail: format!("{}@{parent_version} 仍是 draft 就被 fork", event.lineage_id),
                });
            }
            let actual = parent
                .snapshot_hash
                .as_deref()
                .ok_or_else(|| EvalError::Corrupt {
                    detail: format!("{}@{parent_version} 缺少快照哈希", event.lineage_id),
                })?;
            if actual != parent_hash {
                return Err(EvalError::Corrupt {
                    detail: format!("{}@{version} 的父快照哈希不一致", event.lineage_id),
                });
            }
            Some(ParentPin {
                lineage_id: parent_lineage,
                version: parent_version,
                snapshot_hash: parent_hash,
            })
        }
        _ => {
            return Err(EvalError::Corrupt {
                detail: format!("{}@{version} 的父指针不完整", event.lineage_id),
            });
        }
    };
    state
        .benches
        .entry(event.lineage_id.clone())
        .or_default()
        .insert(
            version,
            BenchInner {
                phase: Phase::Draft,
                cases: BTreeMap::new(),
                parent,
                snapshot_hash: None,
                created_at_ms: event.at_ms,
                frozen_at_ms: None,
                retired_at_ms: None,
            },
        );
    Ok(())
}

fn apply_open(state: &mut State, row: OpenRow) -> Result<()> {
    if state.runs.contains_key(&row.run_id) {
        return Err(EvalError::Corrupt {
            detail: format!("重复运行 {}", row.run_id),
        });
    }
    let version = u64::try_from(row.version).map_err(|_| EvalError::Corrupt {
        detail: format!("运行 {} 的版本超出 u64", row.run_id),
    })?;
    let bench = bench_ref(state, &row.lineage_id, version)?;
    if bench.phase == Phase::Draft {
        return Err(EvalError::Corrupt {
            detail: format!("运行 {} 绑定了 draft", row.run_id),
        });
    }
    let hash = bench
        .snapshot_hash
        .as_deref()
        .ok_or_else(|| EvalError::Corrupt {
            detail: format!("运行 {} 的基准没有快照哈希", row.run_id),
        })?;
    if hash != row.snapshot_hash {
        return Err(EvalError::Corrupt {
            detail: format!("运行 {} 的快照哈希与基准不一致", row.run_id),
        });
    }
    if row.budget_ms <= 0 || row.subject_id.is_empty() || row.subject_version.is_empty() {
        return Err(EvalError::Corrupt {
            detail: format!("运行 {} 的绑定字段不完整", row.run_id),
        });
    }
    state.runs.insert(
        row.run_id,
        RunInner {
            lineage_id: row.lineage_id,
            version,
            snapshot_hash: row.snapshot_hash,
            subject_id: row.subject_id,
            subject_version: row.subject_version,
            budget_ms: row.budget_ms,
            started_at_ms: row.started_at_ms,
            close: None,
        },
    );
    Ok(())
}

fn apply_close(state: &mut State, row: CloseRow) -> Result<()> {
    let run = state
        .runs
        .get_mut(&row.run_id)
        .ok_or_else(|| EvalError::Corrupt {
            detail: format!("结束了不存在的运行 {}", row.run_id),
        })?;
    if run.close.is_some() {
        return Err(EvalError::Corrupt {
            detail: format!("运行 {} 被结束了两次", row.run_id),
        });
    }
    let verdict = match row.outcome.as_str() {
        "passed" => {
            if row.failure_code.is_some() || row.overlap.is_some() {
                return Err(EvalError::Corrupt {
                    detail: format!("运行 {} 的通过行带了失败信息", row.run_id),
                });
            }
            RunVerdict::Passed
        }
        "failed" => {
            let code = row
                .failure_code
                .as_deref()
                .ok_or_else(|| EvalError::Corrupt {
                    detail: format!("运行 {} 的失败行没有失败码", row.run_id),
                })?;
            let failure = FailureCode::parse(code).ok_or_else(|| EvalError::Corrupt {
                detail: format!("运行 {} 的失败码无法识别: {code}", row.run_id),
            })?;
            if row.overlap.is_some() {
                return Err(EvalError::Corrupt {
                    detail: format!("运行 {} 的失败行带了污染交集", row.run_id),
                });
            }
            RunVerdict::Failed(failure)
        }
        "halted" => {
            if row.failure_code.is_some() {
                return Err(EvalError::Corrupt {
                    detail: format!("运行 {} 的停机行带了失败码", row.run_id),
                });
            }
            let text = row.overlap.as_deref().ok_or_else(|| EvalError::Corrupt {
                detail: format!("运行 {} 的停机行没有交集", row.run_id),
            })?;
            let overlap = decode_list(text)?;
            if overlap.is_empty() {
                return Err(EvalError::Corrupt {
                    detail: format!("运行 {} 的停机交集是空的", row.run_id),
                });
            }
            RunVerdict::Halted { overlap }
        }
        other => {
            return Err(EvalError::Corrupt {
                detail: format!("运行 {} 的结果 {} 无法识别", row.run_id, other),
            });
        }
    };
    run.close = Some(CloseInner {
        verdict,
        recorded_at_ms: row.recorded_at_ms,
    });
    Ok(())
}

fn bench_mut<'a>(
    state: &'a mut State,
    lineage_id: &str,
    version: u64,
) -> Result<&'a mut BenchInner> {
    state
        .benches
        .get_mut(lineage_id)
        .and_then(|versions| versions.get_mut(&version))
        .ok_or_else(|| EvalError::Corrupt {
            detail: format!("事件指向不存在的基准 {lineage_id}@{version}"),
        })
}

fn required_text(value: Option<String>, detail: &str) -> Result<String> {
    match value {
        Some(text) if !text.is_empty() => Ok(text),
        _ => Err(EvalError::Corrupt {
            detail: detail.to_owned(),
        }),
    }
}

fn is_append_only(err: &rusqlite::Error) -> bool {
    err.to_string().to_ascii_lowercase().contains("append-only")
}

fn map_unique(err: rusqlite::Error, run_id: &str) -> EvalError {
    let text = err.to_string().to_ascii_lowercase();
    if text.contains("unique") {
        EvalError::RunExists {
            run_id: run_id.to_owned(),
        }
    } else {
        EvalError::Sqlite(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn memory() -> Catalog {
        let (catalog, status) =
            Catalog::open_in_memory_with_clock("ledger-1", ManualClock::new(10)).unwrap();
        assert_eq!(status, OpenStatus::Created);
        catalog
    }

    fn reject_sql(conn: &Connection, sql: &str) {
        let err = conn.execute(sql, []).unwrap_err();
        assert!(
            is_append_only(&err),
            "expected append-only, got {err} for {sql}"
        );
    }

    #[test]
    fn memory_durability_and_every_trigger_rejects_mutation() {
        let mut catalog = memory();
        let durability = catalog.durability().unwrap();
        assert_eq!(durability.journal_mode, "memory");
        assert_eq!(durability.synchronous, 1);
        assert_eq!(durability.foreign_keys, 1);
        assert_eq!(durability.busy_timeout_ms, 5000);
        assert_eq!(durability.user_version, 1);

        catalog.create_benchmark("math").unwrap();
        catalog.put_case("math", 1, "c1", "gold").unwrap();
        catalog.freeze("math", 1).unwrap();
        catalog.set_current_recipe("optim", "body").unwrap();
        let view = catalog
            .begin_run(&RunSpec {
                run_id: "run-1",
                lineage_id: "math",
                version: 1,
                subject_id: "agent",
                subject_version: "rev-1",
                input: &[],
                budget_ms: 30,
            })
            .unwrap();
        catalog
            .finish_run(
                "run-1",
                ExecSignals {
                    subject: true,
                    ..ExecSignals::default()
                },
            )
            .unwrap();
        assert_eq!(view.subject_version, "rev-1");

        reject_sql(
            &catalog.conn,
            "UPDATE schema_meta SET ledger_id = 'other' WHERE id = 1",
        );
        reject_sql(
            &catalog.conn,
            "UPDATE bench_event SET payload = 'tamper' WHERE kind = 'case_set'",
        );
        reject_sql(
            &catalog.conn,
            "UPDATE recipe_event SET body = 'tamper' WHERE recipe_id = 'optim'",
        );
        reject_sql(
            &catalog.conn,
            "UPDATE run_open SET subject_version = 'tamper' WHERE run_id = 'run-1'",
        );
        reject_sql(
            &catalog.conn,
            "UPDATE run_close SET outcome = 'passed' WHERE run_id = 'run-1'",
        );
        reject_sql(
            &catalog.conn,
            "DELETE FROM run_close WHERE run_id = 'run-1'",
        );
        reject_sql(&catalog.conn, "DELETE FROM run_open WHERE run_id = 'run-1'");
        reject_sql(
            &catalog.conn,
            "DELETE FROM bench_event WHERE kind = 'case_set'",
        );
        reject_sql(&catalog.conn, "DELETE FROM recipe_event");
        reject_sql(&catalog.conn, "DELETE FROM schema_meta");

        let record = catalog.run_record("run-1").unwrap();
        assert_eq!(record.subject_version, "rev-1");
        assert_eq!(record.verdict, RunVerdict::Failed(FailureCode::Subject));
        assert_eq!(
            catalog.benchmark("math", 1).unwrap().cases[0].payload,
            "gold"
        );
        assert_eq!(catalog.current_recipe().unwrap().body, "body");
        assert_eq!(catalog.ledger_id(), "ledger-1");
    }

    #[test]
    fn file_open_is_wal_and_rejects_torn_or_foreign_declaration() {
        let tmp = TempDb::new();
        let clock = ManualClock::new(5);
        let (catalog, status) =
            Catalog::open_with_clock(tmp.path(), "ledger-a", clock.clone()).unwrap();
        assert_eq!(status, OpenStatus::Created);
        let durability = catalog.durability().unwrap();
        assert_eq!(durability.journal_mode, "wal");
        assert_eq!(durability.synchronous, 1);
        assert_eq!(durability.foreign_keys, 1);
        assert_eq!(durability.busy_timeout_ms, 5000);
        drop(catalog);

        let err = Catalog::open_with_clock(tmp.path(), "ledger-b", clock.clone()).unwrap_err();
        assert!(
            matches!(err, EvalError::DeclarationMismatch { .. }),
            "{err}"
        );
        let (reopened, status) = Catalog::open_with_clock(tmp.path(), "ledger-a", clock).unwrap();
        assert_eq!(status, OpenStatus::AlreadyDeclared);
        assert_eq!(reopened.ledger_id(), "ledger-a");
        drop(reopened);

        let conn = Connection::open(tmp.path()).unwrap();
        conn.pragma_update(None, "user_version", 9).unwrap();
        drop(conn);
        let err = Catalog::open(tmp.path(), "ledger-a").unwrap_err();
        assert!(
            matches!(
                err,
                EvalError::UnknownSchema {
                    found: 9,
                    expected: 1
                }
            ),
            "{err}"
        );

        let torn = TempDb::new();
        let conn = Connection::open(torn.path()).unwrap();
        conn.execute_batch("CREATE TABLE stray (id INTEGER);")
            .unwrap();
        drop(conn);
        let err = Catalog::open(torn.path(), "ledger-a").unwrap_err();
        assert!(matches!(err, EvalError::TornInitialization), "{err}");
    }

    struct TempDb {
        path: std::path::PathBuf,
    }

    impl TempDb {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("af-eval-ledger-{}-{n}.db", std::process::id()));
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
                    std::path::PathBuf::from(format!("{}{suffix}", self.path.display()))
                };
                let _ = std::fs::remove_file(path);
            }
        }
    }
}
