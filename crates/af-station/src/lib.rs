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

use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
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
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl Station {
    /// 打开（必要时创建）账本。WAL + busy_timeout，供多进程/多线程并发。
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// 内存账本（单连接独享，适合单 worker 测试）。
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.busy_timeout(Duration::from_secs(5))?;
        // WAL 提高并发读写；内存库该语句无效果（返回 memory），无害。
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS task (
                id              TEXT PRIMARY KEY,
                payload_ref     TEXT NOT NULL,
                required_role   TEXT NOT NULL,
                state           TEXT NOT NULL
                    CHECK (state IN ('queued','claimed','done')),
                claimant        TEXT,
                lease_until     INTEGER,
                idempotency_key TEXT UNIQUE
            );
            CREATE INDEX IF NOT EXISTS idx_task_role_state
                ON task (required_role, state);",
        )?;
        Ok(Station { conn })
    }

    /// 入账一条新任务（初始 queued）。
    ///
    /// 幂等键语义：`idempotency_key` 上有 UNIQUE 约束，`ON CONFLICT DO NOTHING`
    /// + 受影响行数判定 —— 撞键时不产生第二行，返回已存在任务的 id。
    /// `idempotency_key=None` 不参与去重（SQLite UNIQUE 允许多个 NULL）。
    pub fn new_task(
        &self,
        id: &str,
        payload_ref: &str,
        required_role: &str,
        idempotency_key: Option<&str>,
    ) -> Result<Enqueued> {
        let n = self.conn.execute(
            "INSERT INTO task (id, payload_ref, required_role, state, claimant, lease_until, idempotency_key)
             VALUES (?1, ?2, ?3, 'queued', NULL, NULL, ?4)
             ON CONFLICT(idempotency_key) DO NOTHING",
            params![id, payload_ref, required_role, idempotency_key],
        )?;
        if n == 1 {
            return Ok(Enqueued::Inserted { id: id.to_string() });
        }
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM task WHERE idempotency_key = ?1",
                params![idempotency_key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(Enqueued::Duplicate {
            existing_id: existing.unwrap_or_else(|| id.to_string()),
        })
    }

    /// 惰性回收：把租约已过期的 claimed 任务打回 queued。
    /// 在每次 `claim` 开头调用 —— 过期任务无需后台线程，被下一次查询顺手回炉。
    pub fn reclaim_expired(&self) -> Result<usize> {
        let n = self.conn.execute(
            "UPDATE task
             SET state = 'queued', claimant = NULL, lease_until = NULL
             WHERE state = 'claimed' AND lease_until IS NOT NULL AND lease_until < ?1",
            params![now_ms()],
        )?;
        Ok(n)
    }

    /// 原子认领一个属于 `role` 的 queued 任务，租约 `lease_ms` 毫秒。
    ///
    /// 关键点：候选 id 只是"提名"，成败由守卫 UPDATE 的受影响行数裁定 ——
    /// `WHERE ... AND claimant IS NULL` 保证并发下只有一个 worker 能置上 claimant。
    /// 行数为 0（被别人抢先）就换下一个候选重试，全部落空返回 None。
    pub fn claim(&self, worker_id: &str, role: &str, lease_ms: u64) -> Result<Option<Task>> {
        self.reclaim_expired()?; // 过期租约先回炉，他人即可接管
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
        let lease_until = now_ms() + lease_ms as i64;
        for tid in candidates {
            // 与任务书一致的原子认领模式：受影响行数判定成败。
            let n = self.conn.execute(
                "UPDATE task
                 SET state = 'claimed', claimant = ?1, lease_until = ?2
                 WHERE id = ?3 AND claimant IS NULL",
                params![worker_id, lease_until, tid],
            )?;
            if n == 1 {
                return Ok(self.get(&tid)?);
            }
        }
        Ok(None)
    }

    /// 心跳续期：把 `lease_until` 重置为 now + `extend_ms`。
    /// 只有"现任 claimant 且租约尚未过期"才续得上 —— 过期后原主人心跳失败，
    /// 防止僵尸 worker 复活已被接管的任务。
    pub fn heartbeat(&self, task_id: &str, worker_id: &str, extend_ms: u64) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE task SET lease_until = ?1
             WHERE id = ?2 AND claimant = ?3 AND state = 'claimed' AND lease_until > ?4",
            params![now_ms() + extend_ms as i64, task_id, worker_id, now_ms()],
        )?;
        Ok(n == 1)
    }

    /// 完成：只有现任 claimant 能把自己的任务置为 done。
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
            "SELECT id, payload_ref, required_role, state, claimant, lease_until, idempotency_key
             FROM task WHERE id = ?1",
        )?;
        stmt.query_row(params![id], row_to_task)
            .optional()
            .map_err(Error::from)
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

fn row_to_task(r: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        payload_ref: r.get(1)?,
        required_role: r.get(2)?,
        state: State::from_db(&r.get::<_, String>(3)?),
        claimant: r.get(4)?,
        lease_until: r.get(5)?,
        idempotency_key: r.get(6)?,
    })
}

// ---------------------------------------------------------------------------
// 测试：真实执行，多连接并发的用例用磁盘库 + std 线程。
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::thread;

    /// 唯一的临时目录，多连接并发用例共享同一个磁盘库。
    fn tmpdb(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "af-station-{}-{}-{}",
            tag,
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("station.db")
    }

    #[test]
    fn enqueue_and_get_roundtrip() {
        let s = Station::open_in_memory().unwrap();
        let e = s.new_task("t1", "payload://a", "reviewer", Some("k1")).unwrap();
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
        let e1 = s.new_task("t1", "payload://a", "reviewer", Some("k1")).unwrap();
        // 同一幂等键、不同任务 id 再入账：必须被拦下，返回已有任务 id
        let e2 = s.new_task("t9", "payload://b", "coder", Some("k1")).unwrap();
        assert_eq!(e1, Enqueued::Inserted { id: "t1".into() });
        assert_eq!(e2, Enqueued::Duplicate { existing_id: "t1".into() });
        assert_eq!(s.count_by_state(State::Queued).unwrap(), 1);
        // 无幂等键的任务不去重，各自独立入账
        s.new_task("a", "p", "r", None).unwrap();
        s.new_task("b", "p", "r", None).unwrap();
        assert_eq!(s.count_by_state(State::Queued).unwrap(), 3);
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
        let s = Station::open_in_memory().unwrap();
        s.new_task("t1", "p", "reviewer", None).unwrap();
        let t = s.claim("wA", "reviewer", 1_000).unwrap().unwrap();
        let before = t.lease_until.unwrap();
        // 他人心跳：无效
        assert!(!s.heartbeat("t1", "wB", 1_000).unwrap());
        thread::sleep(Duration::from_millis(60));
        // 现任心跳：续期到 now+1000，明显晚于原租约
        assert!(s.heartbeat("t1", "wA", 1_000).unwrap());
        let after = s.get("t1").unwrap().unwrap().lease_until.unwrap();
        assert!(after > before, "heartbeat must push lease forward: {before} -> {after}");
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
        let s = Station::open_in_memory().unwrap();
        s.new_task("t1", "p", "reviewer", None).unwrap();
        s.claim("wA", "reviewer", 150).unwrap().unwrap();
        thread::sleep(Duration::from_millis(350)); // 租约过期
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
        let winners: Vec<Task> = handles.into_iter().map(|h| h.join().unwrap()).flatten().collect();
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
        let mut all: Vec<String> = handles.into_iter().flat_map(|h| h.join().unwrap()).collect();
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
            s.new_task("w-1", "payload://order-1", "reviewer", Some("order-1:v1")).unwrap(),
            Enqueued::Inserted { id: "w-1".into() }
        );
        // 重复入账被幂等键拦下
        assert_eq!(
            s.new_task("w-1-dup", "payload://order-1", "reviewer", Some("order-1:v1")).unwrap(),
            Enqueued::Duplicate { existing_id: "w-1".into() }
        );
        let t = s.claim("human-alice", "reviewer", 60_000).unwrap().unwrap();
        assert_eq!(t.id, "w-1");
        assert!(s.heartbeat("w-1", "human-alice", 60_000).unwrap());
        assert!(s.complete("w-1", "human-alice").unwrap());
        assert_eq!(s.get("w-1").unwrap().unwrap().state, State::Done);
        assert_eq!(s.count_by_state(State::Done).unwrap(), 1);
    }
}
