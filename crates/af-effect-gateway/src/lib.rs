//! af-effect-gateway —— 一次性外部操作的防重复闸门（PoC）
//!
//! 解决的问题：workorder 的某个 step 要做一笔**只能做一次**的外部操作
//! （转账、发证、外部 API 调用）。进程可能在 declare 之后、外部调用前后
//! 任意点崩溃；恢复后重放不能导致重复发送。
//!
//! 核心流程（四步，全部先过闸门）：
//! 1. [`Gateway::declare_intent`] —— 用幂等键 (workorder_id, step_path, seq) 登记
//!    意图，载荷指纹 sha256 一并入库；进入可撤回窗口。
//! 2. 窗口期内允许 [`Gateway::withdraw_intent`] 撤回（不发送）。
//! 3. [`Gateway::execute`] —— 窗口关闭后真正执行外部调用；执行前若发现
//!    回执已存在（崩溃前已执行过），直接返回已存结果，**不重发**。
//! 4. [`Gateway::record_receipt`] —— 记录回执；`execute` 内部已自动记录，
//!    外部调用在别处发生时也可显式调用。
//!
//! Fencing 纪元：[`Gateway::resume`] 每次把持久化的纪元 +1；调用方通过
//! [`Gateway::attach`] 拿到当前纪元的 [`Session`]，所有请求都带 session。
//! 网关发现请求纪元 ≠ 当前纪元（典型：恢复前发出的旧请求）一律拒绝
//! [`GatewayError::StaleEpoch`]。注意：**旧纪元声明的意图本身仍有效**，
//! 恢复后的新 session 可以继续用（这正是崩溃恢复重放的路）。
//!
//! 同键不同载荷：指纹不一致 → [`GatewayError::PayloadConflict`]，要求人工
//! 升级处理，网关绝不替调用方二选一。

mod clock;

pub use clock::{Clock, ManualClock, SystemClock};

use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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
    #[error("撤回失败：key={key} state={state} deadline={deadline_ms}ms（窗口已关）")]
    WithdrawWindowClosed {
        key: String,
        state: String,
        deadline_ms: i64,
    },
    /// 外部调用失败；意图保持 declared，窗口关闭后可重试。
    #[error("外部调用失败（意图保持 declared，可重试）: {0}")]
    ExternalCall(String),
    #[error("db error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// 供外部调用闭包便捷地报错。
pub fn external_err<T>(msg: impl Into<String>) -> Result<T> {
    Err(GatewayError::ExternalCall(msg.into()))
}

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
    Executed,
    Withdrawn,
}

impl IntentState {
    fn as_str(self) -> &'static str {
        match self {
            IntentState::Declared => "declared",
            IntentState::Executed => "executed",
            IntentState::Withdrawn => "withdrawn",
        }
    }
    fn from_db(s: &str) -> Self {
        match s {
            "executed" => IntentState::Executed,
            "withdrawn" => IntentState::Withdrawn,
            _ => IntentState::Declared,
        }
    }
}

/// declare 的结果。
#[derive(Debug, Clone, Copy)]
pub enum DeclareOutcome {
    /// 新登记，撤回窗口从此刻起算。
    Created { withdraw_deadline_ms: i64 },
    /// 同键同载荷的重复声明，幂等接受，不改动原意图。
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
    pub withdraw_deadline_ms: i64,
    pub declared_epoch: i64,
    pub receipt_json: Option<String>,
    pub executed_epoch: Option<i64>,
}

// ---------------------------------------------------------------------------
// 网关
// ---------------------------------------------------------------------------

pub struct Gateway {
    conn: Connection,
    withdraw_window: Duration,
}

impl Gateway {
    /// 打开（或创建）持久化网关。崩溃恢复 = 重新 open + [`Gateway::resume`]。
    pub fn open(path: impl AsRef<Path>, withdraw_window: Duration) -> Result<Self> {
        let conn = Connection::open(path)?;
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        Self::init(conn, withdraw_window)
    }

    /// 内存网关（纯单测/演示用，无崩溃恢复意义）。
    pub fn open_in_memory(withdraw_window: Duration) -> Result<Self> {
        Self::init(Connection::open_in_memory()?, withdraw_window)
    }

    fn init(conn: Connection, withdraw_window: Duration) -> Result<Self> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS gateway_meta (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             INSERT OR IGNORE INTO gateway_meta(key, value) VALUES ('epoch', '0');
             CREATE TABLE IF NOT EXISTS effect_intent (
                 workorder_id         TEXT NOT NULL,
                 step_path            TEXT NOT NULL,
                 seq                  INTEGER NOT NULL,
                 payload_json         TEXT NOT NULL,
                 payload_sha256       TEXT NOT NULL,
                 state                TEXT NOT NULL
                                      CHECK(state IN ('declared','executed','withdrawn')),
                 declared_epoch       INTEGER NOT NULL,
                 declared_at_ms       INTEGER NOT NULL,
                 withdraw_deadline_ms INTEGER NOT NULL,
                 receipt_json         TEXT,
                 executed_at_ms       INTEGER,
                 executed_epoch       INTEGER,
                 PRIMARY KEY (workorder_id, step_path, seq)
             );",
        )?;
        Ok(Gateway {
            conn,
            withdraw_window,
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

    // -- ① declare_intent ----------------------------------------------------

    /// 登记意图。幂等：同键同载荷重复声明返回 `AlreadyDeclared`；
    /// 同键不同载荷返回 `PayloadConflict`（要求人工升级）。
    pub fn declare_intent(
        &self,
        session: &Session,
        key: &IntentKey,
        payload: &Value,
    ) -> Result<DeclareOutcome> {
        self.fence(session)?;
        let now = now_ms();
        let deadline = now + self.withdraw_window.as_millis() as i64;
        let fp = fingerprint(payload);
        let payload_json = serde_json::to_string(payload)?;

        let inserted = self.conn.execute(
            "INSERT INTO effect_intent
                 (workorder_id, step_path, seq, payload_json, payload_sha256,
                  state, declared_epoch, declared_at_ms, withdraw_deadline_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, 'declared', ?6, ?7, ?8)
             ON CONFLICT(workorder_id, step_path, seq) DO NOTHING",
            params![
                key.workorder_id,
                key.step_path,
                key.seq,
                payload_json,
                fp,
                session.epoch,
                now,
                deadline
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
        let now = now_ms();
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
    /// withdrawn → 拒发；executed → 返回已存回执（**不调用** `effect`）；
    /// declared 但窗口未关 → 拒发等窗口；否则调用 `effect` 并立刻落回执。
    ///
    /// `effect` 收到声明时的载荷（JSON）；返回 Err 时意图保持 declared，
    /// 窗口关闭后可重试。
    pub fn execute<T, F>(
        &self,
        session: &Session,
        key: &IntentKey,
        effect: F,
    ) -> Result<ExecuteOutcome<T>>
    where
        F: FnOnce(&Value) -> Result<T>,
        T: Serialize + DeserializeOwned,
    {
        self.fence(session)?;
        let (state, payload_json, deadline): (String, String, i64) = self
            .conn
            .query_row(
                "SELECT state, payload_json, withdraw_deadline_ms FROM effect_intent
                 WHERE workorder_id=?1 AND step_path=?2 AND seq=?3",
                params![key.workorder_id, key.step_path, key.seq],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| GatewayError::NotDeclared {
                key: key.to_string(),
            })?;

        match IntentState::from_db(&state) {
            IntentState::Withdrawn => Err(GatewayError::Withdrawn {
                key: key.to_string(),
            }),
            IntentState::Executed => {
                let (receipt_json, at, ep): (String, i64, i64) = self.conn.query_row(
                    "SELECT receipt_json, executed_at_ms, executed_epoch FROM effect_intent
                     WHERE workorder_id=?1 AND step_path=?2 AND seq=?3",
                    params![key.workorder_id, key.step_path, key.seq],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?;
                let result = serde_json::from_str(&receipt_json)?;
                Ok(ExecuteOutcome::Replayed {
                    result,
                    original_executed_at_ms: at,
                    original_epoch: ep,
                })
            }
            IntentState::Declared => {
                let now = now_ms();
                if now < deadline {
                    return Err(GatewayError::WindowStillOpen {
                        deadline_ms: deadline,
                        now_ms: now,
                    });
                }
                let payload: Value = serde_json::from_str(&payload_json)?;
                let result = effect(&payload)?; // 失败则保持 declared，可重试
                self.store_receipt(key, &result, now, session.epoch)?;
                Ok(ExecuteOutcome::RanFresh { result })
            }
        }
    }

    // -- ④ record_receipt ----------------------------------------------------

    /// 显式记录回执（外部调用发生在闸门之外的分步流程用）。
    /// 只允许 declared → executed；已 executed 则幂等保留原回执。
    /// 注意：回执记录不受撤回窗口约束——外呼已经发生，回执必须落库。
    pub fn record_receipt<T: Serialize>(
        &self,
        session: &Session,
        key: &IntentKey,
        receipt: &T,
    ) -> Result<RecordOutcome> {
        self.fence(session)?;
        let now = now_ms();
        let stored = self.store_receipt(key, receipt, now, session.epoch)?;
        Ok(if stored {
            RecordOutcome::Recorded
        } else {
            RecordOutcome::AlreadyRecorded
        })
    }

    /// 返回 true=本次写入，false=此前已有回执（幂等保留）。
    fn store_receipt<T: Serialize>(
        &self,
        key: &IntentKey,
        receipt: &T,
        now_ms: i64,
        epoch: i64,
    ) -> Result<bool> {
        let receipt_json = serde_json::to_string(receipt)?;
        let n = self.conn.execute(
            "UPDATE effect_intent
             SET state='executed', receipt_json=?1, executed_at_ms=?2, executed_epoch=?3
             WHERE workorder_id=?4 AND step_path=?5 AND seq=?6 AND state='declared'",
            params![
                receipt_json,
                now_ms,
                epoch,
                key.workorder_id,
                key.step_path,
                key.seq
            ],
        )?;
        if n == 1 {
            return Ok(true);
        }
        // 没写入：要么不存在，要么已 executed / withdrawn
        let snap = self
            .snapshot_intent(key)?
            .ok_or_else(|| GatewayError::NotDeclared {
                key: key.to_string(),
            })?;
        match snap.state {
            IntentState::Executed => Ok(false), // 幂等：保留原回执
            IntentState::Withdrawn => Err(GatewayError::Withdrawn {
                key: key.to_string(),
            }),
            IntentState::Declared => unreachable!("declared 行的 UPDATE 必然生效"),
        }
    }

    // -- 观测 ----------------------------------------------------------------

    pub fn snapshot_intent(&self, key: &IntentKey) -> Result<Option<IntentSnapshot>> {
        let snap = self
            .conn
            .query_row(
                "SELECT state, payload_sha256, withdraw_deadline_ms, declared_epoch,
                        receipt_json, executed_epoch
                 FROM effect_intent
                 WHERE workorder_id=?1 AND step_path=?2 AND seq=?3",
                params![key.workorder_id, key.step_path, key.seq],
                |r| {
                    Ok(IntentSnapshot {
                        state: IntentState::from_db(&r.get::<_, String>(0)?),
                        payload_sha256: r.get(1)?,
                        withdraw_deadline_ms: r.get(2)?,
                        declared_epoch: r.get(3)?,
                        receipt_json: r.get(4)?,
                        executed_epoch: r.get(5)?,
                    })
                },
            )
            .optional()?;
        Ok(snap)
    }
}

// ---------------------------------------------------------------------------
// 工具
// ---------------------------------------------------------------------------

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_millis() as i64
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
