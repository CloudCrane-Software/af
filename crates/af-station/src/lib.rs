//! af-station —— 人工任务账本（唯一事实源）。
//!
//! 一张 SQLite 表 `task` 记录所有需要人工/Agent 处理的任务：
//! 状态机 `queued -> claimed -> done`，认领/完成/心跳全部用
//! "带守卫条件的 UPDATE + 受影响行数" 判定成败，天然原子。
//!
//! 核心语义（本 PoC 验证的四条）：
//! 1. 原子认领：`UPDATE ... SET claimant=? WHERE id=? AND claimant IS NULL`，
//!    返回受影响行数 1 才算认领成功 —— 两个 worker 并发抢同一任务只有一个成功。
//! 2. 租约：认领时写入 `lease_until`（unix 毫秒）；`heartbeat` 续期；
//!    `claim` 入口先惰性回收过期租约（state 置回 queued、清 claimant），他人即可接管。
//! 3. 幂等键：`idempotency_key` 列 UNIQUE，重复入账返回 `Enqueued::Duplicate`，不产生第二行。
//! 4. 角色路由：`required_role` 与 worker 角色精确匹配才可被认领。
//! 5. 班次窗口：角色或工位绑定了 [`af_calendar::Calendar`] 时，只有
//!    `is_working_instant(now)` 为真才派单。未绑定不设门。非班次 `claim` 返回
//!    None，任务保持 queued。已认领的任务不因下班被收回；租约回收仍只看 `lease_until`。
//!
//! 开库是声明，不是「有表就迁就」：[`OpenStatus::Created`] 为首启（建表并写入
//! schema 版本与指纹），[`OpenStatus::AlreadyDeclared`] 为重入且库内声明与代码一致。
//! 版本、指纹或活表结构对不上则失败，不补表、不改已有行。
//!
//! 时刻经 [`Clock`] 注入。[`Station::open`] 与 [`Station::open_in_memory`]
//! 仍读本机墙钟；测试用 [`ManualClock`] 推进租约，不睡眠。班次判定把这一读数
//! 换成 [`chrono::DateTime<Utc>`]，不另读墙钟。
//!
//! SLA 截止是 [`chrono::DateTime<Utc>`]（与 af-workorder 的 `Sla::deadline` 同一形态），
//! 落盘为 unix 毫秒。逾期只比较这个绝对时刻。派单不看 deadline。

mod clock;
mod schema;

pub use clock::{Clock, ManualClock, SystemClock};
pub use schema::OpenStatus;

use af_calendar::Calendar;
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Arc;

const KIND_ROLE: &str = "role";
const KIND_WORKER: &str = "worker";

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// 库内声明与代码不一致，或库里已有对不上的对象。调用方应停机，而不是接着派单。
    #[error("声明与库内状态不符: {0}")]
    DeclarationMismatch(String),
    #[error("班次日历无法解析: {0}")]
    CalendarJson(String),
    #[error("班次主体不能为空")]
    EmptyShiftSubject,
    #[error("无法把 unix 毫秒 {0} 转成 UTC 时刻")]
    InvalidInstant(i64),
    /// 纳秒尾巴或越界时刻若截成毫秒再读回，会和调用方给的时刻不是同一个。
    #[error("deadline 必须是可落盘的毫秒 UTC 时刻，拒绝静默截断或越界: {0}")]
    DeadlineNotMillis(DateTime<Utc>),
    #[error("calendar: {0}")]
    Calendar(#[from] af_calendar::CalendarError),
}

/// [`Station::open`] 的结果：账本，以及这次是首启还是重入。
///
/// 方法调用经 [`Deref`](std::ops::Deref) 落到 [`Station`]。要区分首启与重入，看 [`Opened::status`]。
pub struct Opened {
    status: OpenStatus,
    station: Station,
}

impl std::fmt::Debug for Opened {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Opened")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl Opened {
    pub fn status(&self) -> OpenStatus {
        self.status
    }

    pub fn station(&self) -> &Station {
        &self.station
    }

    pub fn into_station(self) -> Station {
        self.station
    }
}

impl std::ops::Deref for Opened {
    type Target = Station;

    fn deref(&self) -> &Station {
        &self.station
    }
}

/// 任务状态机：queued -> claimed -> done。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Queued,
    Claimed,
    Done,
}

impl State {
    pub fn as_str(&self) -> &'static str {
        match self {
            State::Queued => "queued",
            State::Claimed => "claimed",
            State::Done => "done",
        }
    }
    fn from_db(s: &str) -> Self {
        match s {
            "claimed" => State::Claimed,
            "done" => State::Done,
            _ => State::Queued,
        }
    }
}

/// 一行任务账目。
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub payload_ref: String,
    pub required_role: String,
    pub state: State,
    pub claimant: Option<String>,
    /// unix 毫秒；queued/done 时为 None。
    pub lease_until: Option<i64>,
    pub idempotency_key: Option<String>,
    /// 绝对截止时刻（UTC）。账本按 unix 毫秒落盘；None 表示未设时限。
    pub deadline: Option<DateTime<Utc>>,
}

impl Task {
    /// 恰在 deadline 不算逾期。无 deadline 恒为 false。
    ///
    /// 判定式与 af-workorder `Sla::is_overdue` 相同：`deadline < now`。
    /// `now` 由调用方注入，本方法不读系统钟，也不问日历。
    pub fn is_overdue(&self, now: DateTime<Utc>) -> bool {
        self.deadline.is_some_and(|deadline| deadline < now)
    }
}

/// [`Station::enqueue`] 的入参。`deadline` 为 None 时与 [`Station::new_task`] 相同。
pub struct NewTask<'a> {
    pub id: &'a str,
    pub payload_ref: &'a str,
    pub required_role: &'a str,
    pub idempotency_key: Option<&'a str>,
    /// 须对齐到毫秒。带亚毫秒尾巴会拒绝，而不是截断后落盘。
    pub deadline: Option<DateTime<Utc>>,
}

/// 用日历把「下单时刻 + 承诺有效工时」解析成 UTC 截止。
///
/// 时长语义原样交给 [`Calendar::sla_deadline`]：零时长是下一个可开工时刻，
/// 负时长与视界耗尽返回 [`Error::Calendar`]。返回值再 `with_timezone(Utc)`。
pub fn sla_deadline_utc<Tz>(
    calendar: &Calendar,
    placed_at: DateTime<Tz>,
    promised_working: chrono::Duration,
) -> Result<DateTime<Utc>>
where
    Tz: TimeZone + Clone,
{
    Ok(calendar
        .sla_deadline(placed_at, promised_working)?
        .with_timezone(&Utc))
}

/// 入账结果：新插入，或命中幂等键返回已有任务 id。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Enqueued {
    Inserted { id: String },
    Duplicate { existing_id: String },
}

/// 任务账本。一个 `Station` 持有一条独立 SQLite 连接；
/// 多 worker = 各自 open 同一个 db 文件，正确性不依赖进程内锁。
pub struct Station {
    conn: Connection,
    clock: Arc<dyn Clock>,
}

impl Station {
    /// 打开（必要时创建）账本。WAL + busy_timeout + foreign_keys，供多进程/多线程并发。
    /// 时刻用 [`SystemClock`]。
    ///
    /// 空库返回 [`OpenStatus::Created`]。已声明且一致返回 [`OpenStatus::AlreadyDeclared`]。
    /// 声明对不上返回 [`Error::DeclarationMismatch`]，不改库里的表。
    pub fn open(path: impl AsRef<Path>) -> Result<Opened> {
        Self::open_with_clock(path, SystemClock::new())
    }

    /// 与 [`Station::open`] 相同，但租约读写使用 `clock`。
    pub fn open_with_clock(path: impl AsRef<Path>, clock: impl Clock + 'static) -> Result<Opened> {
        let conn = Connection::open(path)?;
        Self::init(conn, Arc::new(clock))
    }

    /// 内存账本（单连接独享，适合单 worker 测试）。时刻用 [`SystemClock`]。
    /// 每次调用都是一张新库，因此状态恒为 [`OpenStatus::Created`]。
    pub fn open_in_memory() -> Result<Opened> {
        Self::open_in_memory_with_clock(SystemClock::new())
    }

    /// 与 [`Station::open_in_memory`] 相同，但租约读写使用 `clock`。
    pub fn open_in_memory_with_clock(clock: impl Clock + 'static) -> Result<Opened> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn, Arc::new(clock))
    }

    fn init(mut conn: Connection, clock: Arc<dyn Clock>) -> Result<Opened> {
        let status = schema::prepare(&mut conn)?;
        Ok(Opened {
            status,
            station: Station { conn, clock },
        })
    }

    fn now_ms(&self) -> i64 {
        self.clock.now_ms()
    }

    /// 注入时钟的当前时刻，UTC。
    pub fn now_utc(&self) -> Result<DateTime<Utc>> {
        millis_to_utc(self.now_ms())
    }

    /// 入账一条新任务（初始 queued，无 deadline）。
    ///
    /// 幂等键语义：`idempotency_key` 上有 UNIQUE 约束。
    /// `ON CONFLICT DO NOTHING` 配合受影响行数判定：撞键时不产生第二行，返回已存在任务的 id。
    /// 撞键不改已有行的 deadline。`idempotency_key = None` 不参与去重（SQLite UNIQUE 允许多个 NULL）。
    pub fn new_task(
        &self,
        id: &str,
        payload_ref: &str,
        required_role: &str,
        idempotency_key: Option<&str>,
    ) -> Result<Enqueued> {
        self.enqueue(NewTask {
            id,
            payload_ref,
            required_role,
            idempotency_key,
            deadline: None,
        })
    }

    /// 入账，并可带 UTC 截止时刻。幂等键语义与 [`Station::new_task`] 相同。
    pub fn enqueue(&self, task: NewTask<'_>) -> Result<Enqueued> {
        let deadline_ms = match task.deadline {
            Some(deadline) => Some(utc_to_millis(deadline)?),
            None => None,
        };
        let n = self.conn.execute(
            "INSERT INTO task (
                id, payload_ref, required_role, state, claimant, lease_until, idempotency_key, deadline_ms
             )
             VALUES (?1, ?2, ?3, 'queued', NULL, NULL, ?4, ?5)
             ON CONFLICT(idempotency_key) DO NOTHING",
            params![
                task.id,
                task.payload_ref,
                task.required_role,
                task.idempotency_key,
                deadline_ms
            ],
        )?;
        if n == 1 {
            return Ok(Enqueued::Inserted {
                id: task.id.to_string(),
            });
        }
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM task WHERE idempotency_key = ?1",
                params![task.idempotency_key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(Enqueued::Duplicate {
            existing_id: existing.unwrap_or_else(|| task.id.to_string()),
        })
    }

    /// 把角色的可派单时段绑到日历。同一角色再次绑定会替换。空角色名拒绝。
    ///
    /// 日历先 `validate()`。未绑定与「绑定一张没有班次的日历」不同：前者不设门，后者永不派单。
    pub fn bind_role_shift(&self, role: &str, calendar: &Calendar) -> Result<()> {
        self.bind_shift(KIND_ROLE, role, calendar)
    }

    /// 把工位的可接单时段绑到日历。与角色日历同时存在时，两边都在班次内才派。
    pub fn bind_worker_shift(&self, worker_id: &str, calendar: &Calendar) -> Result<()> {
        self.bind_shift(KIND_WORKER, worker_id, calendar)
    }

    /// 读回角色班次。没有绑定则 None。JSON 坏了或校验失败则返回错误，不当成「没绑定」。
    pub fn role_shift(&self, role: &str) -> Result<Option<Calendar>> {
        self.load_shift(KIND_ROLE, role)
    }

    /// 读回工位班次。
    pub fn worker_shift(&self, worker_id: &str) -> Result<Option<Calendar>> {
        self.load_shift(KIND_WORKER, worker_id)
    }

    fn bind_shift(&self, kind: &str, subject: &str, calendar: &Calendar) -> Result<()> {
        if subject.is_empty() {
            return Err(Error::EmptyShiftSubject);
        }
        calendar.validate()?;
        let json =
            serde_json::to_string(calendar).map_err(|err| Error::CalendarJson(err.to_string()))?;
        self.conn.execute(
            "INSERT INTO shift_binding (kind, subject, calendar_json)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(kind, subject) DO UPDATE SET calendar_json = excluded.calendar_json",
            params![kind, subject, json],
        )?;
        Ok(())
    }

    fn load_shift(&self, kind: &str, subject: &str) -> Result<Option<Calendar>> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT calendar_json FROM shift_binding WHERE kind = ?1 AND subject = ?2",
                params![kind, subject],
                |r| r.get(0),
            )
            .optional()?;
        let Some(json) = json else {
            return Ok(None);
        };
        let calendar: Calendar =
            serde_json::from_str(&json).map_err(|err| Error::CalendarJson(err.to_string()))?;
        calendar.validate()?;
        Ok(Some(calendar))
    }

    /// 角色日历与工位日历都要放行。两边都没绑定则放行，保持只按角色匹配的旧语义。
    fn dispatchable(&self, worker_id: &str, role: &str, now_ms: i64) -> Result<bool> {
        let role_cal = self.load_shift(KIND_ROLE, role)?;
        let worker_cal = self.load_shift(KIND_WORKER, worker_id)?;
        if role_cal.is_none() && worker_cal.is_none() {
            return Ok(true);
        }
        let now = millis_to_utc(now_ms)?;
        if let Some(calendar) = &role_cal {
            if !calendar.is_working_instant(now) {
                return Ok(false);
            }
        }
        if let Some(calendar) = &worker_cal {
            if !calendar.is_working_instant(now) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// 惰性回收：把租约已过期的 claimed 任务打回 queued。
    /// 在每次 `claim` 开头调用 —— 过期任务无需后台线程，被下一次查询顺手回炉。
    pub fn reclaim_expired(&self) -> Result<usize> {
        self.reclaim_before(self.now_ms())
    }

    fn reclaim_before(&self, now: i64) -> Result<usize> {
        let n = self.conn.execute(
            "UPDATE task
             SET state = 'queued', claimant = NULL, lease_until = NULL
             WHERE state = 'claimed' AND lease_until IS NOT NULL AND lease_until < ?1",
            params![now],
        )?;
        Ok(n)
    }

    /// 原子认领一个属于 `role` 的 queued 任务，租约 `lease_ms` 毫秒。
    ///
    /// 先按注入时刻回收过期租约，再看班次门。非班次返回 None，任务保持 queued
    /// （过期租约已经回炉）。角色仍是 SQL 里的 `required_role` 精确匹配。
    ///
    /// 关键点：候选 id 只是"提名"，成败由守卫 UPDATE 的受影响行数裁定 ——
    /// `WHERE ... AND claimant IS NULL` 保证并发下只有一个 worker 能置上 claimant。
    /// 行数为 0（被别人抢先）就换下一个候选重试，全部落空返回 None。
    pub fn claim(&self, worker_id: &str, role: &str, lease_ms: u64) -> Result<Option<Task>> {
        // 回收与租约截止共用同一次读数，注入的时刻在这一次认领里不会中途漂移。
        let now = self.now_ms();
        self.reclaim_before(now)?; // 过期租约先回炉，他人即可接管
        if !self.dispatchable(worker_id, role, now)? {
            return Ok(None);
        }
        let mut stmt = self.conn.prepare(
            "SELECT id FROM task
             WHERE state = 'queued' AND required_role = ?1
             ORDER BY rowid LIMIT 64",
        )?;
        let candidates: Vec<String> = {
            let rows = stmt.query_map(params![role], |r| r.get::<_, String>(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        drop(stmt);
        let lease_until = now + lease_ms as i64;
        for tid in candidates {
            // 与任务书一致的原子认领模式：受影响行数判定成败。
            let n = self.conn.execute(
                "UPDATE task
                 SET state = 'claimed', claimant = ?1, lease_until = ?2
                 WHERE id = ?3 AND claimant IS NULL",
                params![worker_id, lease_until, tid],
            )?;
            if n == 1 {
                return self.get(&tid);
            }
        }
        Ok(None)
    }

    /// 心跳续期：把 `lease_until` 重置为 now + `extend_ms`。
    /// 只有"现任 claimant 且租约尚未过期"才续得上 —— 过期后原主人心跳失败，
    /// 防止僵尸 worker 复活已被接管的任务。不看班次：已经领走的任务，下班后仍按租约续期。
    pub fn heartbeat(&self, task_id: &str, worker_id: &str, extend_ms: u64) -> Result<bool> {
        // 截止点与守卫用同一个 now：心跳以该时刻为基准重置，而不是在旧值上累加。
        let now = self.now_ms();
        let n = self.conn.execute(
            "UPDATE task SET lease_until = ?1
             WHERE id = ?2 AND claimant = ?3 AND state = 'claimed' AND lease_until > ?4",
            params![now + extend_ms as i64, task_id, worker_id, now],
        )?;
        Ok(n == 1)
    }

    /// 完成：只有现任 claimant 能把自己的任务置为 done。
    /// 不看班次，也不看租约是否已过期：回收发生前，现任 claimant 仍可完成。
    pub fn complete(&self, task_id: &str, worker_id: &str) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE task SET state = 'done'
             WHERE id = ?1 AND claimant = ?2 AND state = 'claimed'",
            params![task_id, worker_id],
        )?;
        Ok(n == 1)
    }

    /// 按 id 取一行任务。
    pub fn get(&self, id: &str) -> Result<Option<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, payload_ref, required_role, state, claimant, lease_until, idempotency_key, deadline_ms
             FROM task WHERE id = ?1",
        )?;
        let row = stmt.query_row(params![id], row_to_parts).optional()?;
        row.map(task_from_parts).transpose()
    }

    /// 某状态的任务数（测试/观测用）。
    pub fn count_by_state(&self, state: State) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM task WHERE state = ?1",
            params![state.as_str()],
            |r| r.get(0),
        )?)
    }
}

struct TaskParts {
    id: String,
    payload_ref: String,
    required_role: String,
    state: State,
    claimant: Option<String>,
    lease_until: Option<i64>,
    idempotency_key: Option<String>,
    deadline_ms: Option<i64>,
}

fn row_to_parts(r: &rusqlite::Row<'_>) -> rusqlite::Result<TaskParts> {
    Ok(TaskParts {
        id: r.get(0)?,
        payload_ref: r.get(1)?,
        required_role: r.get(2)?,
        state: State::from_db(&r.get::<_, String>(3)?),
        claimant: r.get(4)?,
        lease_until: r.get(5)?,
        idempotency_key: r.get(6)?,
        deadline_ms: r.get(7)?,
    })
}

fn task_from_parts(row: TaskParts) -> Result<Task> {
    let deadline = match row.deadline_ms {
        Some(ms) => Some(millis_to_utc(ms)?),
        None => None,
    };
    Ok(Task {
        id: row.id,
        payload_ref: row.payload_ref,
        required_role: row.required_role,
        state: row.state,
        claimant: row.claimant,
        lease_until: row.lease_until,
        idempotency_key: row.idempotency_key,
        deadline,
    })
}

fn millis_to_utc(ms: i64) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp_millis(ms).ok_or(Error::InvalidInstant(ms))
}

fn utc_to_millis(dt: DateTime<Utc>) -> Result<i64> {
    let ms = dt.timestamp_millis();
    match DateTime::from_timestamp_millis(ms) {
        Some(restored) if restored == dt => Ok(ms),
        _ => Err(Error::DeadlineNotMillis(dt)),
    }
}

// ---------------------------------------------------------------------------
// 测试：真实执行，多连接并发的用例用磁盘库 + std 线程。
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 唯一的临时目录，多连接并发用例共享同一个磁盘库。
    fn tmpdb(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "af-station-{}-{}-{}",
            tag,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("station.db")
    }

    #[test]
    fn enqueue_and_get_roundtrip() {
        let s = Station::open_in_memory().unwrap();
        let e = s
            .new_task("t1", "payload://a", "reviewer", Some("k1"))
            .unwrap();
        assert_eq!(e, Enqueued::Inserted { id: "t1".into() });
        let t = s.get("t1").unwrap().unwrap();
        assert_eq!(t.state, State::Queued);
        assert_eq!(t.claimant, None);
        assert_eq!(t.lease_until, None);
        assert_eq!(t.idempotency_key.as_deref(), Some("k1"));
    }

    #[test]
    fn idempotency_key_prevents_duplicate_enqueue() {
        let s = Station::open_in_memory().unwrap();
        let e1 = s
            .new_task("t1", "payload://a", "reviewer", Some("k1"))
            .unwrap();
        // 同一幂等键、不同任务 id 再入账：必须被拦下，返回已有任务 id
        let e2 = s
            .new_task("t9", "payload://b", "coder", Some("k1"))
            .unwrap();
        assert_eq!(e1, Enqueued::Inserted { id: "t1".into() });
        assert_eq!(
            e2,
            Enqueued::Duplicate {
                existing_id: "t1".into()
            }
        );
        assert_eq!(s.count_by_state(State::Queued).unwrap(), 1);
        // 无幂等键的任务不去重，各自独立入账
        s.new_task("a", "p", "r", None).unwrap();
        s.new_task("b", "p", "r", None).unwrap();
        assert_eq!(s.count_by_state(State::Queued).unwrap(), 3);
    }

    /// RESULT.md ④.3：`ON CONFLICT(idempotency_key)` 只吞幂等键冲突。
    /// 同一 id、不同 key（或 key 为 NULL）是主键冲突，表现为 Sqlite 错误，不是 Duplicate。
    /// 同一 id 且同一 key 仍走幂等键，返回 Duplicate，不产生第二行。
    #[test]
    fn primary_key_conflict_is_not_reported_as_duplicate() {
        let s = Station::open_in_memory().unwrap();
        assert_eq!(
            s.new_task("t1", "p", "r0", Some("k1")).unwrap(),
            Enqueued::Inserted { id: "t1".into() }
        );
        assert_eq!(
            s.new_task("t2", "p", "r1", Some("k1")).unwrap(),
            Enqueued::Duplicate {
                existing_id: "t1".into()
            }
        );
        let diff_key = s.new_task("t1", "p", "r0", Some("k2")).unwrap_err();
        assert!(
            matches!(diff_key, Error::Sqlite(_)),
            "same id, different key must be a sqlite error, got {diff_key}"
        );
        let null_key = s.new_task("t1", "p", "r0", None).unwrap_err();
        assert!(
            matches!(null_key, Error::Sqlite(_)),
            "same id, null key must be a sqlite error, got {null_key}"
        );
        assert_eq!(
            s.new_task("t1", "p", "r0", Some("k1")).unwrap(),
            Enqueued::Duplicate {
                existing_id: "t1".into()
            }
        );
        assert_eq!(s.count_by_state(State::Queued).unwrap(), 1);
    }

    #[test]
    fn role_routes_claim_to_matching_worker_only() {
        let s = Station::open_in_memory().unwrap();
        s.new_task("t1", "payload://a", "reviewer", None).unwrap();
        // 角色不匹配：coder 认领不到 reviewer 的任务
        assert!(s.claim("w-coder", "coder", 60_000).unwrap().is_none());
        assert_eq!(s.count_by_state(State::Queued).unwrap(), 1);
        // 角色匹配：reviewer 认领成功
        let t = s.claim("w-reviewer", "reviewer", 60_000).unwrap().unwrap();
        assert_eq!(t.id, "t1");
        assert_eq!(t.state, State::Claimed);
        assert_eq!(t.claimant.as_deref(), Some("w-reviewer"));
        assert!(t.lease_until.is_some());
        // 已被认领，别人（哪怕角色对）也领不到
        assert!(s.claim("w2", "reviewer", 60_000).unwrap().is_none());
    }

    #[test]
    fn heartbeat_renews_lease_and_only_claimant_can_use_it() {
        let clock = ManualClock::new(1_700_000_000_000);
        let s = Station::open_in_memory_with_clock(clock.clone()).unwrap();
        s.new_task("t1", "p", "reviewer", None).unwrap();
        let t = s.claim("wA", "reviewer", 1_000).unwrap().unwrap();
        let before = t.lease_until.unwrap();
        assert_eq!(before, 1_700_000_000_000 + 1_000);
        // 他人心跳：无效，租约不动
        assert!(!s.heartbeat("t1", "wB", 1_000).unwrap());
        assert_eq!(s.get("t1").unwrap().unwrap().lease_until, Some(before));
        clock.advance(60);
        // 现任心跳：以注入的 now 为基准重置到 now+1000，而不是在旧截止点上累加
        assert!(s.heartbeat("t1", "wA", 1_000).unwrap());
        let after = s.get("t1").unwrap().unwrap().lease_until.unwrap();
        assert!(
            after > before,
            "heartbeat must push lease forward: {before} -> {after}"
        );
        assert_eq!(after, before + 60);
        assert!(s.complete("t1", "wA").unwrap());
        assert_eq!(s.get("t1").unwrap().unwrap().state, State::Done);
        // 非 claimant 不能完成别人的任务
        s.new_task("t2", "p", "reviewer", None).unwrap();
        s.claim("wA", "reviewer", 60_000).unwrap().unwrap();
        assert!(!s.complete("t2", "wB").unwrap());
        assert_eq!(s.get("t2").unwrap().unwrap().state, State::Claimed);
    }

    #[test]
    fn expired_lease_is_reclaimed_and_takeover_works() {
        let clock = ManualClock::new(5_000_000);
        let s = Station::open_in_memory_with_clock(clock.clone()).unwrap();
        s.new_task("t1", "p", "reviewer", None).unwrap();
        let claimed = s.claim("wA", "reviewer", 150).unwrap().unwrap();
        assert_eq!(claimed.lease_until, Some(5_000_150));
        clock.advance(350); // 注入时刻越过租约，不睡眠
                            // 过期后原主人心跳失败（防僵尸复活）
        assert!(!s.heartbeat("t1", "wA", 60_000).unwrap());
        // 他人认领：惰性回收先回炉，wB 接管成功
        let t = s.claim("wB", "reviewer", 60_000).unwrap().unwrap();
        assert_eq!(t.claimant.as_deref(), Some("wB"));
        // 旧主人不能完成、不能续期
        assert!(!s.complete("t1", "wA").unwrap());
        assert!(!s.heartbeat("t1", "wA", 60_000).unwrap());
        // 新主人正常续期并完成
        assert!(s.heartbeat("t1", "wB", 60_000).unwrap());
        assert!(s.complete("t1", "wB").unwrap());
        assert_eq!(s.get("t1").unwrap().unwrap().state, State::Done);
    }

    /// 核心并发用例：两个线程同时抢同一任务，有且只有一个成功。
    #[test]
    fn two_threads_race_on_same_task_exactly_one_wins() {
        let db = tmpdb("race");
        {
            let s = Station::open(&db).unwrap();
            s.new_task("t1", "payload://hot", "reviewer", None).unwrap();
        }
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = ["w1", "w2"]
            .iter()
            .map(|w| {
                let barrier = barrier.clone();
                let db = db.clone();
                let w = *w;
                thread::spawn(move || {
                    let s = Station::open(&db).unwrap(); // 每线程独立连接
                    barrier.wait(); // 同一起跑线
                    s.claim(w, "reviewer", 60_000).unwrap()
                })
            })
            .collect();
        let winners: Vec<Task> = handles
            .into_iter()
            .filter_map(|h| h.join().unwrap())
            .collect();
        assert_eq!(winners.len(), 1, "两线程抢同一任务必须恰好一个成功");
        // 账本终态与赢家一致
        let s = Station::open(&db).unwrap();
        let t = s.get("t1").unwrap().unwrap();
        assert_eq!(t.state, State::Claimed);
        assert_eq!(t.claimant.as_deref(), winners[0].claimant.as_deref());
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }

    /// 压测：4 线程清空 40 个任务，任何任务不得被认领两次。
    #[test]
    fn stress_four_threads_drain_forty_tasks_without_double_claim() {
        let db = tmpdb("stress");
        {
            let s = Station::open(&db).unwrap();
            for i in 0..40 {
                s.new_task(&format!("t{i}"), "p", "coder", None).unwrap();
            }
        }
        let barrier = Arc::new(Barrier::new(4));
        let handles: Vec<_> = (0..4)
            .map(|w| {
                let barrier = barrier.clone();
                let db = db.clone();
                thread::spawn(move || {
                    let s = Station::open(&db).unwrap();
                    let mut claimed = Vec::new();
                    barrier.wait();
                    while let Some(t) = s.claim(&format!("w{w}"), "coder", 60_000).unwrap() {
                        claimed.push(t.id);
                    }
                    claimed
                })
            })
            .collect();
        let mut all: Vec<String> = handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect();
        all.sort();
        let total = all.len();
        all.dedup();
        assert_eq!(total, 40, "40 个任务应被认领 40 次");
        assert_eq!(all.len(), 40, "任何任务不得被认领两次");
        let s = Station::open(&db).unwrap();
        assert_eq!(s.count_by_state(State::Claimed).unwrap(), 40);
        assert_eq!(s.count_by_state(State::Queued).unwrap(), 0);
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }

    /// 端到端走一遍账本生命周期，作为语义总检。
    #[test]
    fn end_to_end_lifecycle() {
        let s = Station::open_in_memory().unwrap();
        assert_eq!(
            s.new_task("w-1", "payload://order-1", "reviewer", Some("order-1:v1"))
                .unwrap(),
            Enqueued::Inserted { id: "w-1".into() }
        );
        // 重复入账被幂等键拦下
        assert_eq!(
            s.new_task(
                "w-1-dup",
                "payload://order-1",
                "reviewer",
                Some("order-1:v1")
            )
            .unwrap(),
            Enqueued::Duplicate {
                existing_id: "w-1".into()
            }
        );
        let t = s.claim("human-alice", "reviewer", 60_000).unwrap().unwrap();
        assert_eq!(t.id, "w-1");
        assert!(s.heartbeat("w-1", "human-alice", 60_000).unwrap());
        assert!(s.complete("w-1", "human-alice").unwrap());
        assert_eq!(s.get("w-1").unwrap().unwrap().state, State::Done);
        assert_eq!(s.count_by_state(State::Done).unwrap(), 1);
    }

    #[test]
    fn open_in_memory_is_created() {
        let opened = Station::open_in_memory().unwrap();
        assert_eq!(opened.status(), OpenStatus::Created);
    }

    #[test]
    fn open_file_is_created_then_already_declared() {
        let db = tmpdb("decl");
        let first = Station::open(&db).unwrap();
        assert_eq!(first.status(), OpenStatus::Created);
        drop(first);
        let second = Station::open(&db).unwrap();
        assert_eq!(second.status(), OpenStatus::AlreadyDeclared);
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }

    #[test]
    fn pragmas_follow_the_ledger_recipe_and_foreign_keys_reject() {
        let db = tmpdb("pragma");
        let opened = Station::open(&db).unwrap();
        let fk: i64 = opened
            .conn
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        let busy: i64 = opened
            .conn
            .pragma_query_value(None, "busy_timeout", |r| r.get(0))
            .unwrap();
        let sync: i64 = opened
            .conn
            .pragma_query_value(None, "synchronous", |r| r.get(0))
            .unwrap();
        let journal: String = opened
            .conn
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        assert_eq!(busy, 5_000);
        assert_eq!(sync, 1, "synchronous=NORMAL");
        assert_eq!(journal.to_lowercase(), "wal");

        opened
            .conn
            .execute_batch(
                "CREATE TABLE parent_probe (id INTEGER PRIMARY KEY);
                 CREATE TABLE child_probe (
                     id INTEGER PRIMARY KEY,
                     pid INTEGER REFERENCES parent_probe(id)
                 );",
            )
            .unwrap();
        let rejected = opened
            .conn
            .execute("INSERT INTO child_probe (id, pid) VALUES (1, 99)", []);
        assert!(rejected.is_err(), "foreign_keys=ON 必须拒绝悬空引用");
        opened
            .conn
            .execute_batch("DROP TABLE child_probe; DROP TABLE parent_probe;")
            .unwrap();
        drop(opened);
        assert_eq!(
            Station::open(&db).unwrap().status(),
            OpenStatus::AlreadyDeclared
        );
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }

    #[test]
    fn schema_version_mismatch_fails_closed() {
        let db = tmpdb("ver");
        drop(Station::open(&db).unwrap());
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute(
            "UPDATE station_decl SET schema_version = schema_version + 1",
            [],
        )
        .unwrap();
        drop(conn);
        let err = Station::open(&db).unwrap_err();
        assert!(
            matches!(err, Error::DeclarationMismatch(_)),
            "version drift must fail closed, got {err}"
        );
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }

    #[test]
    fn fingerprint_mismatch_fails_closed() {
        let db = tmpdb("fp");
        drop(Station::open(&db).unwrap());
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute("UPDATE station_decl SET schema_fingerprint = 'other'", [])
            .unwrap();
        drop(conn);
        let err = Station::open(&db).unwrap_err();
        assert!(matches!(err, Error::DeclarationMismatch(_)), "{err}");
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }

    #[test]
    fn preexisting_task_table_is_not_adopted() {
        let db = tmpdb("adopt");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE task (id TEXT PRIMARY KEY);")
            .unwrap();
        drop(conn);
        let err = Station::open(&db).unwrap_err();
        assert!(
            matches!(err, Error::DeclarationMismatch(_)),
            "unmanaged task table must not be silently adopted, got {err}"
        );
        let _ = std::fs::remove_dir_all(db.parent().unwrap());
    }

    #[test]
    fn live_schema_drift_fails_closed() {
        let db = tmpdb("drift");
        drop(Station::open(&db).unwrap());
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("ALTER TABLE task ADD COLUMN rogue TEXT;")
            .unwrap();
        drop(conn);
        let err = Station::open(&db).unwrap_err();
        assert!(matches!(err, Error::DeclarationMismatch(_)), "{err}");

        let db_drop = tmpdb("dropcol");
        drop(Station::open(&db_drop).unwrap());
        let conn = rusqlite::Connection::open(&db_drop).unwrap();
        conn.execute_batch("ALTER TABLE task DROP COLUMN deadline_ms;")
            .unwrap();
        drop(conn);
        let err = Station::open(&db_drop).unwrap_err();
        assert!(matches!(err, Error::DeclarationMismatch(_)), "{err}");

        let db_missing = tmpdb("notable");
        drop(Station::open(&db_missing).unwrap());
        let conn = rusqlite::Connection::open(&db_missing).unwrap();
        conn.execute_batch("DROP TABLE shift_binding;").unwrap();
        drop(conn);
        let err = Station::open(&db_missing).unwrap_err();
        assert!(matches!(err, Error::DeclarationMismatch(_)), "{err}");

        let _ = std::fs::remove_dir_all(db.parent().unwrap());
        let _ = std::fs::remove_dir_all(db_drop.parent().unwrap());
        let _ = std::fs::remove_dir_all(db_missing.parent().unwrap());
    }

    /// 2026-10-19 10:00 Asia/Shanghai = 02:00Z，落在默认班次 08:00–17:00 内。
    fn monday_shift_clock() -> ManualClock {
        let utc = chrono::Utc.with_ymd_and_hms(2026, 10, 19, 2, 0, 0).unwrap();
        ManualClock::new(utc.timestamp_millis())
    }

    #[test]
    fn overlapping_calendar_is_rejected_and_not_stored() {
        use af_calendar::Shift;

        let s = Station::open_in_memory_with_clock(monday_shift_clock()).unwrap();
        let tz = af_calendar::Calendar::weekday_8x5_shanghai().tz;
        let mut weekly: [Vec<Shift>; 7] = Default::default();
        weekly[0] = vec![
            Shift::hm(8, 0, 12, 0).unwrap(),
            Shift::hm(11, 0, 15, 0).unwrap(),
        ];
        let bad = af_calendar::Calendar::from_weekly(tz, weekly);
        let err = s.bind_role_shift("reviewer", &bad).unwrap_err();
        assert!(matches!(err, Error::Calendar(_)), "{err}");
        assert!(s.role_shift("reviewer").unwrap().is_none());
        s.new_task("t1", "p", "reviewer", None).unwrap();
        // 绑定失败等于没绑定：班次内仍按角色派单。
        assert!(s.claim("w", "reviewer", 1_000).unwrap().is_some());
        assert!(matches!(
            s.bind_role_shift("", &af_calendar::Calendar::weekday_8x5_shanghai())
                .unwrap_err(),
            Error::EmptyShiftSubject
        ));
    }

    #[test]
    fn corrupt_or_invalid_stored_calendar_fails_closed() {
        use af_calendar::Shift;

        let s = Station::open_in_memory_with_clock(monday_shift_clock()).unwrap();
        s.new_task("t1", "p", "reviewer", None).unwrap();
        s.bind_role_shift("reviewer", &af_calendar::Calendar::weekday_8x5_shanghai())
            .unwrap();
        s.conn
            .execute("UPDATE shift_binding SET calendar_json = '{'", [])
            .unwrap();
        let err = s.claim("w", "reviewer", 1_000).unwrap_err();
        assert!(matches!(err, Error::CalendarJson(_)), "{err}");
        assert_eq!(s.get("t1").unwrap().unwrap().state, State::Queued);

        let tz = af_calendar::Calendar::weekday_8x5_shanghai().tz;
        let mut weekly: [Vec<Shift>; 7] = Default::default();
        weekly[0] = vec![
            Shift::hm(8, 0, 12, 0).unwrap(),
            Shift::hm(11, 0, 15, 0).unwrap(),
        ];
        let bad = af_calendar::Calendar::from_weekly(tz, weekly);
        let json = serde_json::to_string(&bad).unwrap();
        s.conn
            .execute(
                "UPDATE shift_binding SET calendar_json = ?1 WHERE kind = 'role'",
                rusqlite::params![json],
            )
            .unwrap();
        let err = s.claim("w", "reviewer", 1_000).unwrap_err();
        assert!(
            matches!(err, Error::Calendar(_)),
            "校验失败不得当成未绑定而放行, got {err}"
        );
        assert_eq!(s.get("t1").unwrap().unwrap().state, State::Queued);
    }
}
