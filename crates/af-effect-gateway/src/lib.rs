//! af-effect-gateway —— 一次性外部操作的防重复闸门（PoC）
//!
//! 解决的问题：workorder 的某个 step 要做一笔**只能做一次**的外部操作
//! （转账、发证、外部 API 调用）。进程可能在 declare 之后、外部调用前后
//! 任意点崩溃；恢复后重放不能导致重复发送。
//!
//! 核心流程（四步，全部先过闸门）：
//! 1. [`Gateway::declare_intent`] —— 关联工单须为 Released 或 InProgress，
//!    再用幂等键 (workorder_id, step_path, seq) 登记意图，载荷指纹 sha256
//!    一并入库，并记下声明时刻的订单状态；进入可撤回窗口。
//! 2. 窗口期内允许 [`Gateway::withdraw_intent`] 撤回（不发送）。
//! 3. [`Gateway::execute`] —— 工单当前为 InProgress 且窗口关闭后，在
//!    `BEGIN IMMEDIATE` 里把 `declared` 原子抢成 `executing`（记下纪元）。
//!    只有抢到的一方外呼。外呼成功后，同一纪元守卫下把回执落成 `executed`。
//!    已有回执则直接重放，**不重发**。闭包返回 `Err` 时把 `executing` 放回
//!    `declared`，窗口关闭后可重试。崩溃留在 `executing` 且没有回执时
//!    **不自动重发**：人工 [`Gateway::record_receipt`] 接管，或显式
//!    [`Gateway::resume_execution`] 放回 `declared`。
//! 4. [`Gateway::record_receipt`] —— 记录回执；`execute` 内部已自动记录，
//!    外部调用在别处发生、或 `executing` 需要人工接管时也可显式调用。
//!    落库与纪元校验在同一条 `BEGIN IMMEDIATE` 里。
//!
//! Fencing 纪元：[`Gateway::resume`] 每次把持久化的纪元 +1；调用方通过
//! [`Gateway::attach`] 拿到当前纪元的 [`Session`]，所有请求都带 session。
//! 网关发现请求纪元 ≠ 当前纪元（典型：恢复前发出的旧请求）一律拒绝
//! [`GatewayError::StaleEpoch`]。`resume` 只推进纪元，不把 `executing`
//! 放回 `declared`。注意：**旧纪元声明的意图本身仍有效**，
//! 恢复后的新 session 可以继续用（这正是崩溃恢复重放的路）。
//!
//! 同键不同载荷：指纹不一致 → [`GatewayError::PayloadConflict`]，要求人工
//! 升级处理，网关绝不替调用方二选一。
//!
//! 订单态：[`Gateway::declare_intent`] 必须递交关联工单。幂等键里的
//! `workorder_id` 要等于该工单 id，且声明当时的状态只能是 Released 或
//! InProgress，否则 [`GatewayError::OrderStateRejected`]（带允许状态）或
//! [`GatewayError::WorkOrderMismatch`]。[`Gateway::execute`] 只有工单当前
//! 状态是 InProgress 才真正发送。意图行记下 `order_id` 与声明时刻的状态快照，
//! 之后订单再迁移也不改写这份快照。
//!
//! 计时：撤回窗口的开关只看单调钟（[`Clock::monotonic_ms`] 折成库内只增不减的
//! 逻辑毫秒）。墙钟（[`Clock::wall_ms`]）只写入 `declared_at_ms` /
//! `executed_at_ms` 这类必须落库的绝对时刻，不参与执行或过期判定。
//! 生产默认 [`SystemClock`]，测试注入 [`ManualClock`]。进程停机期间不借用
//! 墙钟补时，避免 NTP 回拨把已关闭的窗口重新打开。

mod clock;

pub use clock::{Clock, ManualClock, SystemClock};

use af_workorder::{OrderState, WorkOrder};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, GatewayError>;

// ---------------------------------------------------------------------------
// 错误
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum GatewayError {
    /// 同一幂等键、不同载荷。网关拒绝并要求人工升级，绝不自动选边。
    #[error("payload conflict for {key}: stored={stored_sha256} incoming={incoming_sha256}，需人工升级处理")]
    PayloadConflict {
        key: String,
        stored_sha256: String,
        incoming_sha256: String,
    },
    /// 意图已被撤回，不再发送。
    #[error("intent {key} 已撤回，不发送")]
    Withdrawn { key: String },
    /// 撤回窗口未关闭，execute 拒绝发送（调用方应等待窗口关闭后重试）。
    ///
    /// `deadline_ms` 与 `now_ms` 都是单调逻辑毫秒，相减即剩余窗口。
    /// 它们不是 Unix 墙钟，不能与 `SystemTime` 比较。
    #[error("撤回窗口未关：deadline={deadline_ms}ms now={now_ms}ms，暂不发送")]
    WindowStillOpen { deadline_ms: i64, now_ms: i64 },
    /// 旧纪元请求被 fencing 拒绝。
    #[error("旧纪元请求被拒绝：request_epoch={request_epoch} current_epoch={current_epoch}")]
    StaleEpoch {
        request_epoch: i64,
        current_epoch: i64,
    },
    /// execute/record_receipt 时意图不存在，必须先 declare_intent。
    #[error("intent {key} 未声明，先 declare_intent")]
    NotDeclared { key: String },
    /// 撤回失败：窗口已关或状态不是 declared。
    /// `deadline_ms` 是单调逻辑毫秒上的截止点，不是 Unix 墙钟。
    #[error("撤回失败：key={key} state={state} deadline={deadline_ms}ms（窗口已关）")]
    WithdrawWindowClosed {
        key: String,
        state: String,
        deadline_ms: i64,
    },
    /// 外部调用失败；意图保持 declared，窗口关闭后可重试。
    #[error("外部调用失败（意图保持 declared，可重试）: {0}")]
    ExternalCall(String),
    /// 意图停在 executing，回执还没落库。外部调用可能已经发生。
    /// [`Gateway::execute`] 不会自动重发。人工用 [`Gateway::record_receipt`]
    /// 补上已知回执，或用 [`Gateway::resume_execution`] 显式放回 declared。
    #[error(
        "intent {key} 停在 executing（claim_epoch={claim_epoch}），无回执，不自动重发；人工 record_receipt 接管或显式 resume_execution"
    )]
    ExecutionInDoubt { key: String, claim_epoch: i64 },
    /// [`Gateway::resume_execution`] 只接受 executing 且没有回执的意图。
    #[error("intent {key} 状态是 {state}，不能 resume_execution")]
    ResumeNotInDoubt { key: String, state: String },
    /// 库里的意图状态不是已知的四态。
    #[error("intent {key} 的状态无法识别：{raw}")]
    CorruptIntentState { key: String, raw: String },
    /// executed 行缺回执字段。不能当成可重放的结果，也不能因此重发。
    #[error("intent {key} 的回执不完整")]
    CorruptReceipt { key: String },
    /// 幂等键上的 workorder 与本次递交的订单不是同一张。
    #[error(
        "订单关联不一致：intent_workorder_id={intent_workorder_id} presented_order_id={presented_order_id}"
    )]
    WorkOrderMismatch {
        intent_workorder_id: String,
        presented_order_id: String,
    },
    /// 当前订单状态不允许这一步。`allowed` 是类型化允许集合。
    #[error("订单态拒绝 {op}：order_id={order_id} state={state:?}，允许 {allowed:?}")]
    OrderStateRejected {
        op: IntentOrderOp,
        order_id: String,
        state: OrderState,
        allowed: &'static [OrderState],
    },
    /// 库里的声明时状态快照不是已知订单状态。
    #[error("intent {key} 的订单状态快照无法识别：{raw}")]
    CorruptOrderSnapshot { key: String, raw: String },
    #[error("db error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// 供外部调用闭包便捷地报错。
pub fn external_err<T>(msg: impl Into<String>) -> Result<T> {
    Err(GatewayError::ExternalCall(msg.into()))
}

/// declare / execute 被订单态拒绝时，指出是哪一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentOrderOp {
    Declare,
    Execute,
}

impl fmt::Display for IntentOrderOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            IntentOrderOp::Declare => "declare",
            IntentOrderOp::Execute => "execute",
        })
    }
}

/// declare 允许的订单状态。
const DECLARE_OK: &[OrderState] = &[OrderState::Released, OrderState::InProgress];
/// execute 真正发送时允许的订单状态。
const EXECUTE_OK: &[OrderState] = &[OrderState::InProgress];

// ---------------------------------------------------------------------------
// 键 / 会话 / 状态
// ---------------------------------------------------------------------------

/// 幂等键 = workorder_id + step_path + seq。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IntentKey {
    pub workorder_id: String,
    pub step_path: String,
    pub seq: i64,
}

impl fmt::Display for IntentKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}#{}", self.workorder_id, self.step_path, self.seq)
    }
}

/// 调用方持有的纪元戳。由 [`Gateway::attach`] 签发，所有请求都携带。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    pub epoch: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentState {
    Declared,
    /// 已抢占。外呼进行中，或进程死在外呼与回执之间。没有回执时不许自动重发。
    Executing,
    Executed,
    Withdrawn,
}

impl IntentState {
    fn as_str(self) -> &'static str {
        match self {
            IntentState::Declared => "declared",
            IntentState::Executing => "executing",
            IntentState::Executed => "executed",
            IntentState::Withdrawn => "withdrawn",
        }
    }
}

fn parse_intent_state(raw: &str) -> Option<IntentState> {
    match raw {
        "declared" => Some(IntentState::Declared),
        "executing" => Some(IntentState::Executing),
        "executed" => Some(IntentState::Executed),
        "withdrawn" => Some(IntentState::Withdrawn),
        _ => None,
    }
}

/// [`Gateway::resume_execution`] 的结果。这是人工放行，不是 [`Gateway::resume`] 的纪元 +1。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeExecutionOutcome {
    /// executing 且无回执，已放回 declared。下一次 execute 可以再抢占。
    Released,
    /// 本来就是 declared。重复调用不改行。
    AlreadyDeclared,
}

/// declare 的结果。
#[derive(Debug, Clone, Copy)]
pub enum DeclareOutcome {
    /// 新登记，撤回窗口从当前单调逻辑毫秒起算。
    /// `withdraw_deadline_ms` 不是 Unix 墙钟。
    Created { withdraw_deadline_ms: i64 },
    /// 同键同载荷的重复声明，幂等接受，不改动原意图。
    /// `withdraw_deadline_ms` 不是 Unix 墙钟。
    AlreadyDeclared { withdraw_deadline_ms: i64 },
}

/// withdraw 的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WithdrawOutcome {
    /// 窗口内成功撤回，外部操作不会再发生。
    Withdrawn,
    /// 之前已经撤回过，幂等。
    AlreadyWithdrawn,
}

/// execute 的结果。`RanFresh` 表示本次真的调用了外部操作；`Replayed`
/// 表示回执已存在，直接返回已存结果、**未**调用外部操作。
#[derive(Debug)]
pub enum ExecuteOutcome<T> {
    RanFresh {
        result: T,
    },
    Replayed {
        result: T,
        /// 落库时的墙钟 Unix 毫秒，只作审计。窗口判定不读它。
        original_executed_at_ms: i64,
        original_epoch: i64,
    },
}

impl<T> ExecuteOutcome<T> {
    pub fn into_inner(self) -> T {
        match self {
            ExecuteOutcome::RanFresh { result } | ExecuteOutcome::Replayed { result, .. } => result,
        }
    }
    pub fn is_replay(&self) -> bool {
        matches!(self, ExecuteOutcome::Replayed { .. })
    }
}

/// record_receipt 的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordOutcome {
    /// 回执落库，意图转 executed。
    Recorded,
    /// 回执早已存在（幂等，保留原回执不覆盖）。
    AlreadyRecorded,
}

/// 意图快照，供观测/测试断言。
#[derive(Debug, Clone)]
pub struct IntentSnapshot {
    pub state: IntentState,
    pub payload_sha256: String,
    /// 单调逻辑毫秒上的撤回截止点，不是 Unix 墙钟。
    pub withdraw_deadline_ms: i64,
    pub declared_epoch: i64,
    pub receipt_json: Option<String>,
    pub executed_epoch: Option<i64>,
    /// 抢到 executing 的纪元。declared / 尚未抢占时为 None。
    pub claim_epoch: Option<i64>,
    /// 声明时关联的订单 id，等于幂等键里的 workorder_id。
    pub order_id: String,
    /// 声明时刻的订单状态快照。之后订单再迁移也不改写。
    pub declared_order_state: OrderState,
}

// ---------------------------------------------------------------------------
// 网关
// ---------------------------------------------------------------------------

pub struct Gateway {
    conn: Connection,
    withdraw_window: Duration,
    clock: Arc<dyn Clock>,
    /// 打开时库里已提交的单调逻辑毫秒。
    mono_base: i64,
    /// 打开时 [`Clock::monotonic_ms`] 的读数，用来把进程内流逝折成逻辑毫秒。
    clock_at_open: i64,
    /// 本进程见过的逻辑毫秒高水位，只增不减。
    observed_mono: Cell<i64>,
}

impl Gateway {
    /// 打开（或创建）持久化网关。崩溃恢复 = 重新 open + [`Gateway::resume`]。
    /// 间隔判定用 [`SystemClock`]。
    pub fn open(path: impl AsRef<Path>, withdraw_window: Duration) -> Result<Self> {
        Self::open_with_clock(path, withdraw_window, SystemClock::new())
    }

    /// 同 [`Gateway::open`]，间隔判定走调用方注入的钟。
    pub fn open_with_clock(
        path: impl AsRef<Path>,
        withdraw_window: Duration,
        clock: impl Clock + 'static,
    ) -> Result<Self> {
        let conn = Connection::open(path)?;
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        Self::init(conn, withdraw_window, Arc::new(clock))
    }

    /// 内存网关（纯单测/演示用，无崩溃恢复意义）。间隔判定用 [`SystemClock`]。
    pub fn open_in_memory(withdraw_window: Duration) -> Result<Self> {
        Self::open_in_memory_with_clock(withdraw_window, SystemClock::new())
    }

    /// 同 [`Gateway::open_in_memory`]，间隔判定走调用方注入的钟。
    pub fn open_in_memory_with_clock(
        withdraw_window: Duration,
        clock: impl Clock + 'static,
    ) -> Result<Self> {
        Self::init(
            Connection::open_in_memory()?,
            withdraw_window,
            Arc::new(clock),
        )
    }

    fn init(conn: Connection, withdraw_window: Duration, clock: Arc<dyn Clock>) -> Result<Self> {
        // 第二条连接的 BEGIN IMMEDIATE 要等写锁，而不是立刻 SQLITE_BUSY。
        conn.busy_timeout(BUSY_TIMEOUT)?;
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        let tx = rusqlite::Transaction::new_unchecked(&conn, TransactionBehavior::Immediate)?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS gateway_meta (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             INSERT OR IGNORE INTO gateway_meta(key, value) VALUES ('epoch', '0');
             -- mono_ms：已提交的单调逻辑毫秒，只增不减。窗口比较用它，不用墙钟。
             INSERT OR IGNORE INTO gateway_meta(key, value) VALUES ('mono_ms', '0');",
        )?;
        ensure_intent_schema(&tx)?;
        tx.commit()?;
        let persisted: String = conn.query_row(
            "SELECT value FROM gateway_meta WHERE key='mono_ms'",
            [],
            |r| r.get(0),
        )?;
        // 与纪元同一坑：meta.value 是 TEXT，不能按 i64 取列。
        let mono_base = persisted.parse().unwrap_or(0);
        let clock_at_open = clock.monotonic_ms();
        Ok(Gateway {
            conn,
            withdraw_window,
            clock,
            mono_base,
            clock_at_open,
            observed_mono: Cell::new(mono_base),
        })
    }

    // -- 纪元 fencing --------------------------------------------------------

    /// 当前持久化纪元（每次读取，不缓存，多实例打开同一库也不会拿到旧值）。
    pub fn current_epoch(&self) -> Result<i64> {
        let v: String = self.conn.query_row(
            "SELECT value FROM gateway_meta WHERE key='epoch'",
            [],
            |r| r.get(0),
        )?;
        Ok(v.parse().unwrap_or(0))
    }

    /// 恢复/接管时调用：纪元 +1 并持久化。之后旧纪元的请求全部被拒。
    ///
    /// 只推进纪元。停在 executing、还没有回执的意图保持原样，下一次
    /// [`Gateway::execute`] 不会因此自动重发。要再试一次，先
    /// [`Gateway::resume_execution`]。
    pub fn resume(&self) -> Result<Session> {
        let v: String = self.conn.query_row(
            "UPDATE gateway_meta SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
             WHERE key='epoch' RETURNING value",
            [],
            |r| r.get(0),
        )?;
        Ok(Session {
            epoch: v.parse().unwrap_or(0),
        })
    }

    /// 给调用方签发当前纪元的会话。
    pub fn attach(&self) -> Result<Session> {
        Ok(Session {
            epoch: self.current_epoch()?,
        })
    }

    fn fence(&self, session: &Session) -> Result<()> {
        let current = self.current_epoch()?;
        if session.epoch != current {
            return Err(GatewayError::StaleEpoch {
                request_epoch: session.epoch,
                current_epoch: current,
            });
        }
        Ok(())
    }

    /// 单调逻辑毫秒。进程内加上注入钟相对打开时刻的流逝；跨进程从库里的高水位接上。
    /// 高水位只增不减，注入钟若回退也不会把已关闭的窗口重新打开。
    fn logical_now(&self) -> i64 {
        let tick = self.clock.monotonic_ms();
        let delta = tick.saturating_sub(self.clock_at_open);
        let logical = self.mono_base.saturating_add(delta);
        let prev = self.observed_mono.get();
        let now = logical.max(prev);
        self.observed_mono.set(now);
        now
    }

    /// 读逻辑毫秒，并把更高的值写回 `gateway_meta.mono_ms`。
    fn stamp_mono(&self) -> Result<i64> {
        let now = self.logical_now();
        self.conn.execute(
            "UPDATE gateway_meta
             SET value = ?1
             WHERE key = 'mono_ms'
               AND CAST(value AS INTEGER) < CAST(?1 AS INTEGER)",
            params![now.to_string()],
        )?;
        Ok(now)
    }

    // -- ① declare_intent ----------------------------------------------------

    /// 登记意图。幂等：同键同载荷重复声明返回 `AlreadyDeclared`；
    /// 同键不同载荷返回 `PayloadConflict`（要求人工升级）。
    ///
    /// `order` 是声明当时的工单。`key.workorder_id` 必须等于 `order.id()`，
    /// 状态必须是 Released 或 InProgress。首次写入把订单 id 和这个状态记进意图，
    /// 重复声明不改写快照。
    pub fn declare_intent(
        &self,
        session: &Session,
        key: &IntentKey,
        payload: &Value,
        order: &WorkOrder,
    ) -> Result<DeclareOutcome> {
        self.fence(session)?;
        ensure_same_order(&key.workorder_id, order)?;
        ensure_order_state(order, IntentOrderOp::Declare, DECLARE_OK)?;
        let now = self.stamp_mono()?;
        let deadline = now.saturating_add(clock::duration_to_ms(self.withdraw_window));
        // 声明时刻的墙钟只审计。截止点是单调逻辑毫秒，不由墙钟相加得到。
        let declared_at_ms = self.clock.wall_ms();
        let fp = fingerprint(payload);
        let payload_json = serde_json::to_string(payload)?;
        let declared_order_state = order_state_token(order.state());

        let inserted = self.conn.execute(
            "INSERT INTO effect_intent
                 (workorder_id, step_path, seq, payload_json, payload_sha256,
                  state, declared_epoch, declared_at_ms, withdraw_deadline_ms,
                  order_id, declared_order_state)
             VALUES (?1, ?2, ?3, ?4, ?5, 'declared', ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(workorder_id, step_path, seq) DO NOTHING",
            params![
                key.workorder_id,
                key.step_path,
                key.seq,
                payload_json,
                fp,
                session.epoch,
                declared_at_ms,
                deadline,
                order.id(),
                declared_order_state
            ],
        )?;
        if inserted == 1 {
            return Ok(DeclareOutcome::Created {
                withdraw_deadline_ms: deadline,
            });
        }

        // 已存在：比较载荷指纹
        let stored: String = self
            .conn
            .query_row(
                "SELECT payload_sha256 FROM effect_intent
                 WHERE workorder_id=?1 AND step_path=?2 AND seq=?3",
                params![key.workorder_id, key.step_path, key.seq],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| GatewayError::NotDeclared {
                key: key.to_string(),
            })?; // 理论不可达
        if stored != fp {
            return Err(GatewayError::PayloadConflict {
                key: key.to_string(),
                stored_sha256: stored,
                incoming_sha256: fp,
            });
        }
        let deadline: i64 = self.conn.query_row(
            "SELECT withdraw_deadline_ms FROM effect_intent
             WHERE workorder_id=?1 AND step_path=?2 AND seq=?3",
            params![key.workorder_id, key.step_path, key.seq],
            |r| r.get(0),
        )?;
        Ok(DeclareOutcome::AlreadyDeclared {
            withdraw_deadline_ms: deadline,
        })
    }

    // -- ② withdraw_intent ---------------------------------------------------

    /// 撤回窗口内撤回：外部操作将不会发生。窗口已关或已执行则失败。
    pub fn withdraw_intent(&self, session: &Session, key: &IntentKey) -> Result<WithdrawOutcome> {
        self.fence(session)?;
        let now = self.stamp_mono()?;
        let n = self.conn.execute(
            "UPDATE effect_intent SET state='withdrawn'
             WHERE workorder_id=?1 AND step_path=?2 AND seq=?3
               AND state='declared' AND withdraw_deadline_ms > ?4",
            params![key.workorder_id, key.step_path, key.seq, now],
        )?;
        if n == 1 {
            return Ok(WithdrawOutcome::Withdrawn);
        }
        let snap = self
            .snapshot_intent(key)?
            .ok_or_else(|| GatewayError::NotDeclared {
                key: key.to_string(),
            })?;
        if snap.state == IntentState::Withdrawn {
            Ok(WithdrawOutcome::AlreadyWithdrawn)
        } else {
            Err(GatewayError::WithdrawWindowClosed {
                key: key.to_string(),
                state: snap.state.as_str().to_string(),
                deadline_ms: snap.withdraw_deadline_ms,
            })
        }
    }

    // -- ③ execute -----------------------------------------------------------

    /// 执行外部操作。判定顺序：
    /// 订单 id 不一致 → 拒绝；withdrawn → 拒发；executed → 返回已存回执
    /// （**不调用** `effect`，不再看当前订单态）；executing 且无回执 →
    /// [`GatewayError::ExecutionInDoubt`]，不外呼；declared 但当前不是
    /// InProgress → [`GatewayError::OrderStateRejected`]；declared 但窗口未关
    /// → 拒发等窗口；否则在 `BEGIN IMMEDIATE` 里把行抢成 executing，
    /// **提交之后**才调用 `effect`，成功后再用纪元守卫落回执。
    ///
    /// `effect` 收到声明时的载荷（JSON）。返回 Err 表示调用方确认这次没发出去，
    /// 意图放回 declared，窗口关闭后可重试。闭包恐慌或进程死在回执之前，
    /// 行留在 executing，下一次 execute 不自动重发。
    ///
    /// `order` 是执行当时的工单，不是声明时的快照。尚未发送的意图只有
    /// 当前状态为 InProgress 才调用 `effect`；Released 可以声明但不能执行。
    /// 已经执行过的意图仍重放已存回执：发送已经发生，订单之后离开
    /// InProgress 也不再发一次。已经撤回的仍返回 [`GatewayError::Withdrawn`]。
    pub fn execute<T, F>(
        &self,
        session: &Session,
        key: &IntentKey,
        order: &WorkOrder,
        effect: F,
    ) -> Result<ExecuteOutcome<T>>
    where
        F: FnOnce(&Value) -> Result<T>,
        T: Serialize + DeserializeOwned,
    {
        match self.prepare_execute(session, key, order)? {
            PreparedSend::Replay {
                receipt_json,
                executed_at_ms,
                executed_epoch,
            } => {
                let result = serde_json::from_str(&receipt_json)?;
                Ok(ExecuteOutcome::Replayed {
                    result,
                    original_executed_at_ms: executed_at_ms,
                    original_epoch: executed_epoch,
                })
            }
            PreparedSend::Send { payload_json } => {
                self.finish_send(session, key, &payload_json, effect)
            }
        }
    }

    /// 外呼已经由本会话抢下。解析失败或闭包明确失败时放回 declared；
    /// 成功则在纪元守卫下把回执写成 executed。
    fn finish_send<T, F>(
        &self,
        session: &Session,
        key: &IntentKey,
        payload_json: &str,
        effect: F,
    ) -> Result<ExecuteOutcome<T>>
    where
        F: FnOnce(&Value) -> Result<T>,
        T: Serialize + DeserializeOwned,
    {
        let payload: Value = match serde_json::from_str(payload_json) {
            Ok(value) => value,
            Err(err) => {
                // 闭包还没被调用，这次确定没有外呼。
                let _ = self.release_claim(session, key);
                return Err(err.into());
            }
        };
        let result = match effect(&payload) {
            Ok(value) => value,
            Err(err) => {
                if self.release_claim(session, key)? {
                    return Err(err);
                }
                // 纪元已经换过，旧会话不能改行。行留在 executing，不自动重发。
                self.fence(session)?;
                return Err(err);
            }
        };
        let executed_at_ms = self.clock.wall_ms();
        match self.commit_claimed_receipt(session, key, &result, executed_at_ms)? {
            ClaimedStore::Stored => Ok(ExecuteOutcome::RanFresh { result }),
            ClaimedStore::AlreadyStored {
                receipt_json,
                executed_at_ms,
                executed_epoch,
            } => {
                let stored = serde_json::from_str(&receipt_json)?;
                Ok(ExecuteOutcome::Replayed {
                    result: stored,
                    original_executed_at_ms: executed_at_ms,
                    original_epoch: executed_epoch,
                })
            }
        }
    }

    // -- ④ record_receipt ----------------------------------------------------

    /// 显式记录回执（外部调用发生在闸门之外，或 executing 人工接管）。
    /// 允许 declared 或 executing → executed；已 executed 则幂等保留原回执。
    /// 注意：回执记录不受撤回窗口约束，也不看当前订单态——外呼已经发生，
    /// 回执必须落库。订单态门控在 declare，以及尚未发送的 execute 上。
    ///
    /// 纪元守卫在同一条 `BEGIN IMMEDIATE` 里：请求纪元不是当前纪元时不写。
    pub fn record_receipt<T: Serialize>(
        &self,
        session: &Session,
        key: &IntentKey,
        receipt: &T,
    ) -> Result<RecordOutcome> {
        self.fence(session)?;
        // 回执落库不受窗口约束。墙钟只给审计字段，逻辑毫秒只推进高水位。
        self.stamp_mono()?;
        let executed_at_ms = self.clock.wall_ms();
        let receipt_json = serde_json::to_string(receipt)?;
        let tx = self.begin_immediate()?;
        self.fence(session)?;
        let n = tx.execute(
            "UPDATE effect_intent
             SET state='executed', receipt_json=?1, executed_at_ms=?2, executed_epoch=?3
             WHERE workorder_id=?4 AND step_path=?5 AND seq=?6
               AND state IN ('declared', 'executing')
               AND (SELECT CAST(value AS INTEGER) FROM gateway_meta WHERE key='epoch') = ?3",
            params![
                receipt_json,
                executed_at_ms,
                session.epoch,
                key.workorder_id,
                key.step_path,
                key.seq
            ],
        )?;
        if n == 1 {
            tx.commit()?;
            return Ok(RecordOutcome::Recorded);
        }
        let outcome = receipt_not_written(&tx, session, key)?;
        tx.rollback()?;
        Ok(outcome)
    }

    /// 把停在 executing、没有回执的意图放回 declared。
    ///
    /// 这是人工确认可以再试一次之后的入口。[`Gateway::execute`] 看到
    /// executing 不会调用它。[`Gateway::resume`] 只加纪元，也不调用它。
    pub fn resume_execution(
        &self,
        session: &Session,
        key: &IntentKey,
    ) -> Result<ResumeExecutionOutcome> {
        let tx = self.begin_immediate()?;
        self.fence(session)?;
        let n = tx.execute(
            "UPDATE effect_intent
             SET state='declared', claim_epoch=NULL
             WHERE workorder_id=?1 AND step_path=?2 AND seq=?3
               AND state='executing'
               AND receipt_json IS NULL
               AND (SELECT CAST(value AS INTEGER) FROM gateway_meta WHERE key='epoch') = ?4",
            params![key.workorder_id, key.step_path, key.seq, session.epoch],
        )?;
        if n == 1 {
            tx.commit()?;
            return Ok(ResumeExecutionOutcome::Released);
        }
        let outcome = resume_not_released(&tx, key)?;
        tx.rollback()?;
        Ok(outcome)
    }

    /// 在写锁里决定这次 execute 是重放、拒绝，还是由本会话抢到外呼权。
    /// 抢到时事务已经提交，调用方再调闭包；崩溃会留下 executing。
    fn prepare_execute(
        &self,
        session: &Session,
        key: &IntentKey,
        order: &WorkOrder,
    ) -> Result<PreparedSend> {
        self.fence(session)?;
        ensure_same_order(&key.workorder_id, order)?;
        let now = self.stamp_mono()?;

        let tx = self.begin_immediate()?;
        self.fence(session)?;
        let row = load_intent(&tx, key)?.ok_or_else(|| GatewayError::NotDeclared {
            key: key.to_string(),
        })?;
        // 行上的 order_id 是声明时写入的。与本次递交的订单不一致就不发送、不重放。
        ensure_same_order(&row.order_id, order)?;
        let state = require_state(key, &row.state)?;
        let decision = match state {
            IntentState::Withdrawn => Err(GatewayError::Withdrawn {
                key: key.to_string(),
            }),
            IntentState::Executed => replay_of(key, &row),
            IntentState::Executing => Err(GatewayError::ExecutionInDoubt {
                key: key.to_string(),
                claim_epoch: row.claim_epoch.unwrap_or(session.epoch),
            }),
            IntentState::Declared => {
                // 订单态先于撤回窗口：Released 不能靠“等到窗口关闭”变成可发送。
                ensure_order_state(order, IntentOrderOp::Execute, EXECUTE_OK)?;
                if now < row.deadline {
                    return Err(GatewayError::WindowStillOpen {
                        deadline_ms: row.deadline,
                        now_ms: now,
                    });
                }
                let n = tx.execute(
                    "UPDATE effect_intent
                     SET state='executing', claim_epoch=?1
                     WHERE workorder_id=?2 AND step_path=?3 AND seq=?4
                       AND state='declared'
                       AND (SELECT CAST(value AS INTEGER) FROM gateway_meta WHERE key='epoch') = ?1",
                    params![session.epoch, key.workorder_id, key.step_path, key.seq],
                )?;
                if n != 1 {
                    return lost_claim(&tx, session, key);
                }
                let payload_json = row.payload_json.clone();
                tx.commit()?;
                return Ok(PreparedSend::Send { payload_json });
            }
        };
        // 没抢占。事务里没有写入，回滚后把判定交出去。
        tx.rollback()?;
        decision
    }

    /// 本会话抢到的 executing 放回 declared。纪元或 claim 对不上则不改行。
    fn release_claim(&self, session: &Session, key: &IntentKey) -> Result<bool> {
        let tx = self.begin_immediate()?;
        let current = self.current_epoch()?;
        if session.epoch != current {
            return Ok(false);
        }
        let n = tx.execute(
            "UPDATE effect_intent
             SET state='declared', claim_epoch=NULL
             WHERE workorder_id=?1 AND step_path=?2 AND seq=?3
               AND state='executing'
               AND claim_epoch=?4
               AND receipt_json IS NULL
               AND (SELECT CAST(value AS INTEGER) FROM gateway_meta WHERE key='epoch') = ?4",
            params![key.workorder_id, key.step_path, key.seq, session.epoch],
        )?;
        tx.commit()?;
        Ok(n == 1)
    }

    /// 把本会话抢到的 executing 写成 executed。纪元或 claim 变了就不写。
    fn commit_claimed_receipt<T: Serialize>(
        &self,
        session: &Session,
        key: &IntentKey,
        receipt: &T,
        executed_at_ms: i64,
    ) -> Result<ClaimedStore> {
        let receipt_json = serde_json::to_string(receipt)?;
        let tx = self.begin_immediate()?;
        self.fence(session)?;
        let n = tx.execute(
            "UPDATE effect_intent
             SET state='executed', receipt_json=?1, executed_at_ms=?2, executed_epoch=?3
             WHERE workorder_id=?4 AND step_path=?5 AND seq=?6
               AND state='executing'
               AND claim_epoch=?3
               AND (SELECT CAST(value AS INTEGER) FROM gateway_meta WHERE key='epoch') = ?3",
            params![
                receipt_json,
                executed_at_ms,
                session.epoch,
                key.workorder_id,
                key.step_path,
                key.seq
            ],
        )?;
        if n == 1 {
            tx.commit()?;
            return Ok(ClaimedStore::Stored);
        }
        let stored = claimed_store_miss(&tx, session, key)?;
        tx.rollback()?;
        Ok(stored)
    }

    fn begin_immediate(&self) -> Result<rusqlite::Transaction<'_>> {
        Ok(rusqlite::Transaction::new_unchecked(
            &self.conn,
            TransactionBehavior::Immediate,
        )?)
    }

    // -- 观测 ----------------------------------------------------------------

    pub fn snapshot_intent(&self, key: &IntentKey) -> Result<Option<IntentSnapshot>> {
        let row = self
            .conn
            .query_row(
                "SELECT state, payload_sha256, withdraw_deadline_ms, declared_epoch,
                        receipt_json, executed_epoch, order_id, declared_order_state,
                        claim_epoch
                 FROM effect_intent
                 WHERE workorder_id=?1 AND step_path=?2 AND seq=?3",
                params![key.workorder_id, key.step_path, key.seq],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, Option<String>>(4)?,
                        r.get::<_, Option<i64>>(5)?,
                        r.get::<_, String>(6)?,
                        r.get::<_, String>(7)?,
                        r.get::<_, Option<i64>>(8)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            state,
            payload_sha256,
            withdraw_deadline_ms,
            declared_epoch,
            receipt_json,
            executed_epoch,
            order_id,
            declared_raw,
            claim_epoch,
        )) = row
        else {
            return Ok(None);
        };
        let declared_order_state =
            parse_order_state(&declared_raw).ok_or_else(|| GatewayError::CorruptOrderSnapshot {
                key: key.to_string(),
                raw: declared_raw,
            })?;
        Ok(Some(IntentSnapshot {
            state: require_state(key, &state)?,
            payload_sha256,
            withdraw_deadline_ms,
            declared_epoch,
            receipt_json,
            executed_epoch,
            order_id,
            declared_order_state,
            claim_epoch,
        }))
    }
}

// ---------------------------------------------------------------------------
// 工具
// ---------------------------------------------------------------------------

/// 第二条连接等待写锁的上限。锁冲突时返回忙，而不是把外呼发出去。
const BUSY_TIMEOUT: Duration = Duration::from_millis(5_000);

const INTENT_DDL: &str = "
CREATE TABLE effect_intent (
    workorder_id         TEXT NOT NULL,
    step_path            TEXT NOT NULL,
    seq                  INTEGER NOT NULL,
    payload_json         TEXT NOT NULL,
    payload_sha256       TEXT NOT NULL,
    state                TEXT NOT NULL
                         CHECK(state IN ('declared','executing','executed','withdrawn')),
    declared_epoch       INTEGER NOT NULL,
    -- declared_at_ms / executed_at_ms：墙钟 Unix 毫秒，只审计。
    declared_at_ms       INTEGER NOT NULL,
    -- withdraw_deadline_ms：单调逻辑毫秒截止点，只与逻辑 now 比较。
    withdraw_deadline_ms INTEGER NOT NULL,
    -- order_id / declared_order_state：声明时刻的订单关联与状态快照，只审计。
    order_id             TEXT NOT NULL,
    declared_order_state TEXT NOT NULL,
    -- claim_epoch：抢到 executing 的纪元。回执写入时必须仍是当前纪元。
    claim_epoch          INTEGER,
    receipt_json         TEXT,
    executed_at_ms       INTEGER,
    executed_epoch       INTEGER,
    PRIMARY KEY (workorder_id, step_path, seq)
);";

enum PreparedSend {
    Replay {
        receipt_json: String,
        executed_at_ms: i64,
        executed_epoch: i64,
    },
    Send {
        payload_json: String,
    },
}

enum ClaimedStore {
    Stored,
    AlreadyStored {
        receipt_json: String,
        executed_at_ms: i64,
        executed_epoch: i64,
    },
}

struct IntentRow {
    state: String,
    payload_json: String,
    deadline: i64,
    order_id: String,
    receipt_json: Option<String>,
    executed_at_ms: Option<i64>,
    executed_epoch: Option<i64>,
    claim_epoch: Option<i64>,
}

fn ensure_intent_schema(conn: &Connection) -> Result<()> {
    let sql: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='effect_intent'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let Some(sql) = sql else {
        conn.execute_batch(INTENT_DDL)?;
        return Ok(());
    };
    if sql.contains("'executing'") && sql.contains("claim_epoch") {
        return Ok(());
    }
    // 早于订单快照列的库维持原样。缺列时读写仍会失败，这是已记录的遗留。
    if !sql.contains("declared_order_state") {
        return Ok(());
    }
    migrate_intent_add_executing(conn)
}

fn migrate_intent_add_executing(conn: &Connection) -> Result<()> {
    let create_v2 = INTENT_DDL.replacen("effect_intent", "effect_intent_v2", 1);
    conn.execute_batch(&create_v2)?;
    conn.execute_batch(
        "INSERT INTO effect_intent_v2 (
             workorder_id, step_path, seq, payload_json, payload_sha256, state,
             declared_epoch, declared_at_ms, withdraw_deadline_ms, order_id,
             declared_order_state, claim_epoch, receipt_json, executed_at_ms, executed_epoch
         )
         SELECT
             workorder_id, step_path, seq, payload_json, payload_sha256, state,
             declared_epoch, declared_at_ms, withdraw_deadline_ms, order_id,
             declared_order_state, NULL, receipt_json, executed_at_ms, executed_epoch
         FROM effect_intent;
         DROP TABLE effect_intent;
         ALTER TABLE effect_intent_v2 RENAME TO effect_intent;",
    )?;
    Ok(())
}

fn load_intent(conn: &Connection, key: &IntentKey) -> Result<Option<IntentRow>> {
    conn.query_row(
        "SELECT state, payload_json, withdraw_deadline_ms, order_id,
                receipt_json, executed_at_ms, executed_epoch, claim_epoch
         FROM effect_intent
         WHERE workorder_id=?1 AND step_path=?2 AND seq=?3",
        params![key.workorder_id, key.step_path, key.seq],
        |r| {
            Ok(IntentRow {
                state: r.get(0)?,
                payload_json: r.get(1)?,
                deadline: r.get(2)?,
                order_id: r.get(3)?,
                receipt_json: r.get(4)?,
                executed_at_ms: r.get(5)?,
                executed_epoch: r.get(6)?,
                claim_epoch: r.get(7)?,
            })
        },
    )
    .optional()
    .map_err(GatewayError::from)
}

fn require_state(key: &IntentKey, raw: &str) -> Result<IntentState> {
    parse_intent_state(raw).ok_or_else(|| GatewayError::CorruptIntentState {
        key: key.to_string(),
        raw: raw.to_string(),
    })
}

fn replay_of(key: &IntentKey, row: &IntentRow) -> Result<PreparedSend> {
    let (receipt_json, executed_at_ms, executed_epoch) = replay_fields(key, row)?;
    Ok(PreparedSend::Replay {
        receipt_json,
        executed_at_ms,
        executed_epoch,
    })
}

fn corrupt_receipt(key: &IntentKey) -> GatewayError {
    GatewayError::CorruptReceipt {
        key: key.to_string(),
    }
}

fn replay_fields(key: &IntentKey, row: &IntentRow) -> Result<(String, i64, i64)> {
    Ok((
        row.receipt_json
            .clone()
            .ok_or_else(|| corrupt_receipt(key))?,
        row.executed_at_ms.ok_or_else(|| corrupt_receipt(key))?,
        row.executed_epoch.ok_or_else(|| corrupt_receipt(key))?,
    ))
}

/// 写锁内抢占没写上。按当前行给出重放或拒绝，不再外呼。
fn lost_claim(conn: &Connection, session: &Session, key: &IntentKey) -> Result<PreparedSend> {
    let current = epoch_of(conn)?;
    if session.epoch != current {
        return Err(GatewayError::StaleEpoch {
            request_epoch: session.epoch,
            current_epoch: current,
        });
    }
    let row = load_intent(conn, key)?.ok_or_else(|| GatewayError::NotDeclared {
        key: key.to_string(),
    })?;
    match require_state(key, &row.state)? {
        IntentState::Withdrawn => Err(GatewayError::Withdrawn {
            key: key.to_string(),
        }),
        IntentState::Executed => replay_of(key, &row),
        IntentState::Executing => Err(GatewayError::ExecutionInDoubt {
            key: key.to_string(),
            claim_epoch: row.claim_epoch.unwrap_or(session.epoch),
        }),
        IntentState::Declared => Err(GatewayError::CorruptIntentState {
            key: key.to_string(),
            raw: row.state,
        }),
    }
}

fn receipt_not_written(
    conn: &Connection,
    session: &Session,
    key: &IntentKey,
) -> Result<RecordOutcome> {
    let row = load_intent(conn, key)?.ok_or_else(|| GatewayError::NotDeclared {
        key: key.to_string(),
    })?;
    match require_state(key, &row.state)? {
        IntentState::Executed => Ok(RecordOutcome::AlreadyRecorded),
        IntentState::Withdrawn => Err(GatewayError::Withdrawn {
            key: key.to_string(),
        }),
        IntentState::Declared | IntentState::Executing => {
            let current = epoch_of(conn)?;
            if current != session.epoch {
                Err(GatewayError::StaleEpoch {
                    request_epoch: session.epoch,
                    current_epoch: current,
                })
            } else {
                Err(GatewayError::CorruptIntentState {
                    key: key.to_string(),
                    raw: row.state,
                })
            }
        }
    }
}

fn resume_not_released(conn: &Connection, key: &IntentKey) -> Result<ResumeExecutionOutcome> {
    let row = load_intent(conn, key)?.ok_or_else(|| GatewayError::NotDeclared {
        key: key.to_string(),
    })?;
    match require_state(key, &row.state)? {
        IntentState::Declared => Ok(ResumeExecutionOutcome::AlreadyDeclared),
        IntentState::Withdrawn => Err(GatewayError::Withdrawn {
            key: key.to_string(),
        }),
        IntentState::Executed | IntentState::Executing => Err(GatewayError::ResumeNotInDoubt {
            key: key.to_string(),
            state: row.state,
        }),
    }
}

fn claimed_store_miss(
    conn: &Connection,
    session: &Session,
    key: &IntentKey,
) -> Result<ClaimedStore> {
    let current = epoch_of(conn)?;
    if session.epoch != current {
        return Err(GatewayError::StaleEpoch {
            request_epoch: session.epoch,
            current_epoch: current,
        });
    }
    let row = load_intent(conn, key)?.ok_or_else(|| GatewayError::NotDeclared {
        key: key.to_string(),
    })?;
    match require_state(key, &row.state)? {
        IntentState::Executed => {
            let (receipt_json, executed_at_ms, executed_epoch) = replay_fields(key, &row)?;
            Ok(ClaimedStore::AlreadyStored {
                receipt_json,
                executed_at_ms,
                executed_epoch,
            })
        }
        IntentState::Withdrawn => Err(GatewayError::Withdrawn {
            key: key.to_string(),
        }),
        IntentState::Executing | IntentState::Declared => Err(GatewayError::ExecutionInDoubt {
            key: key.to_string(),
            claim_epoch: row.claim_epoch.unwrap_or(session.epoch),
        }),
    }
}

fn epoch_of(conn: &Connection) -> Result<i64> {
    let value: String = conn.query_row(
        "SELECT value FROM gateway_meta WHERE key='epoch'",
        [],
        |r| r.get(0),
    )?;
    Ok(value.parse().unwrap_or(0))
}

fn ensure_same_order(intent_workorder_id: &str, order: &WorkOrder) -> Result<()> {
    if intent_workorder_id == order.id() {
        return Ok(());
    }
    Err(GatewayError::WorkOrderMismatch {
        intent_workorder_id: intent_workorder_id.to_string(),
        presented_order_id: order.id().to_string(),
    })
}

fn ensure_order_state(
    order: &WorkOrder,
    op: IntentOrderOp,
    allowed: &'static [OrderState],
) -> Result<()> {
    let state = order.state();
    if allowed.contains(&state) {
        return Ok(());
    }
    Err(GatewayError::OrderStateRejected {
        op,
        order_id: order.id().to_string(),
        state,
        allowed,
    })
}

fn order_state_token(state: OrderState) -> &'static str {
    match state {
        OrderState::Created => "Created",
        OrderState::Released => "Released",
        OrderState::InProgress => "InProgress",
        OrderState::Completed => "Completed",
        OrderState::Closed => "Closed",
        OrderState::Cancelled => "Cancelled",
    }
}

fn parse_order_state(raw: &str) -> Option<OrderState> {
    Some(match raw {
        "Created" => OrderState::Created,
        "Released" => OrderState::Released,
        "InProgress" => OrderState::InProgress,
        "Completed" => OrderState::Completed,
        "Closed" => OrderState::Closed,
        "Cancelled" => OrderState::Cancelled,
        _ => return None,
    })
}

/// 载荷指纹：sha256(规范 JSON 字节)。serde_json 的 Map 默认按 key 排序
/// （BTreeMap），序列化是确定性的，同载荷必同指纹。
fn fingerprint(payload: &Value) -> String {
    let bytes = serde_json::to_vec(payload).expect("serialize payload");
    let digest = Sha256::digest(&bytes);
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

#[cfg(test)]
mod schema_tests {
    use super::*;
    use af_workorder::{BudgetVoucher, GraphVersion, LineBinding, Priority, Sla};

    fn in_progress(id: &str) -> WorkOrder {
        let mut wo = WorkOrder::new(
            id,
            GraphVersion::new("g-f2"),
            Sla::new(None, Priority::Normal),
        );
        wo.bind_line(LineBinding {
            line_id: "line-A".into(),
            line_version: "lv-1".into(),
        })
        .unwrap();
        wo.attach_budget_voucher(BudgetVoucher {
            voucher_id: "bv-1".into(),
            amount: 1,
            signed: true,
        })
        .unwrap();
        wo.release().unwrap();
        wo.start().unwrap();
        wo
    }

    /// 已有的 W3B 库没有 executing / claim_epoch。打开时要迁过来，旧 declared 行仍可抢占一次。
    #[test]
    fn w3b_table_migrates_and_declared_row_can_be_claimed() {
        let path = std::env::temp_dir().join(format!("af_eg_migrate_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE gateway_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO gateway_meta(key, value) VALUES ('epoch', '0');
                 INSERT INTO gateway_meta(key, value) VALUES ('mono_ms', '0');
                 CREATE TABLE effect_intent (
                     workorder_id TEXT NOT NULL,
                     step_path TEXT NOT NULL,
                     seq INTEGER NOT NULL,
                     payload_json TEXT NOT NULL,
                     payload_sha256 TEXT NOT NULL,
                     state TEXT NOT NULL
                           CHECK(state IN ('declared','executed','withdrawn')),
                     declared_epoch INTEGER NOT NULL,
                     declared_at_ms INTEGER NOT NULL,
                     withdraw_deadline_ms INTEGER NOT NULL,
                     order_id TEXT NOT NULL,
                     declared_order_state TEXT NOT NULL,
                     receipt_json TEXT,
                     executed_at_ms INTEGER,
                     executed_epoch INTEGER,
                     PRIMARY KEY (workorder_id, step_path, seq)
                 );",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO effect_intent (
                     workorder_id, step_path, seq, payload_json, payload_sha256,
                     state, declared_epoch, declared_at_ms, withdraw_deadline_ms,
                     order_id, declared_order_state
                 ) VALUES ('WO-MIG', 'pay', 1, '{\"a\":1}', 'abc',
                           'declared', 0, 1, 0, 'WO-MIG', 'InProgress')",
                [],
            )
            .unwrap();
        }

        let clock = ManualClock::new(0, 1_700_000_000_000);
        let gw = Gateway::open_with_clock(&path, Duration::from_millis(1), clock).unwrap();
        let key = IntentKey {
            workorder_id: "WO-MIG".into(),
            step_path: "pay".into(),
            seq: 1,
        };
        let snap = gw.snapshot_intent(&key).unwrap().unwrap();
        assert_eq!(snap.state, IntentState::Declared);
        assert_eq!(snap.claim_epoch, None);
        assert_eq!(snap.declared_order_state, OrderState::InProgress);
        assert_eq!(snap.order_id, "WO-MIG");

        let calls = Cell::new(0);
        let wo = in_progress("WO-MIG");
        let s = gw.attach().unwrap();
        let exec = gw
            .execute(&s, &key, &wo, |_| {
                calls.set(calls.get() + 1);
                Ok(serde_json::json!({ "txn_id": "T-mig" }))
            })
            .unwrap();
        assert!(!exec.is_replay());
        assert_eq!(calls.get(), 1);
        let snap = gw.snapshot_intent(&key).unwrap().unwrap();
        assert_eq!(snap.state, IntentState::Executed);
        assert_eq!(snap.claim_epoch, Some(0));
        assert_eq!(snap.receipt_json.as_deref(), Some("{\"txn_id\":\"T-mig\"}"));

        // 第二次打开认出新表，不再重建。
        drop(gw);
        let gw = Gateway::open_with_clock(&path, Duration::from_millis(1), ManualClock::new(0, 1))
            .unwrap();
        let snap = gw.snapshot_intent(&key).unwrap().unwrap();
        assert_eq!(snap.state, IntentState::Executed);
        assert_eq!(snap.claim_epoch, Some(0));

        let _ = std::fs::remove_file(&path);
    }
}
