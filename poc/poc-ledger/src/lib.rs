//! poc-ledger — 账本部署形态（单库 vs 分库）+ meter 窗口对齐日历。
//!
//! 两块交付：
//! 1. `window` 模块：把 af-meter 的窗口对齐到日历时区的自然日 / 自然周（周一 00:00），
//!    泛型 `Tz: TimeZone`，同一函数跑 Asia/Shanghai 与 Europe/Berlin。
//! 2. `SCHEMA_SQL`：meter 账本 schema 草案（DDL）+ 最小消费原型（惰性窗口重置、幂等键、fencing 纪元）。
//!
//! 并发写实测 / 崩溃一致性 / 备份形态见 src/bin/ledger_bench.rs、src/bin/crash_tx.rs、tests/。

use chrono::{DateTime, Datelike, Days, TimeZone};
use rusqlite::OptionalExtension;

// ---------------------------------------------------------------------------
// 窗口对齐（裁决二）
// ---------------------------------------------------------------------------

/// 窗口种类：自然日（当日 00:00 起）或自然周（周一 00:00 起，ISO 周）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowKind {
    Day,
    WeekMon,
}

impl WindowKind {
    pub fn as_str(self) -> &'static str {
        match self {
            WindowKind::Day => "day",
            WindowKind::WeekMon => "week_mon",
        }
    }
}

/// 解析"钟面午夜"为本时区时刻。取最早偏移（与 af-calendar 的消歧策略一致；
/// 柏林/上海的 DST 切换都不发生在 00:00，这里实际恒为 single，仅防御）。
fn local_midnight<Tz: TimeZone + Clone>(tz: &Tz, date: chrono::NaiveDate) -> Option<DateTime<Tz>> {
    let naive = date.and_hms_opt(0, 0, 0)?;
    tz.from_local_datetime(&naive).earliest()
}

/// 窗口边界 `[start, end)`：按 `now` 所在时区的钟面计算。
/// - Day：`[当日 00:00, 次日 00:00)`
/// - WeekMon：`[本周一 00:00, 下周一 00:00)`（ISO 周，周一为一周之首）
///
/// 关键实现纪律：跨 DST 时**不能用绝对时间加法**（167h 后不是下周一 00:00 钟面），
/// 必须先算目标钟面日期，再解析该钟面的午夜。DST 周 elapsed 因此是 167h/169h。
pub fn window_bounds<Tz: TimeZone + Clone>(
    kind: WindowKind,
    now: &DateTime<Tz>,
) -> (DateTime<Tz>, DateTime<Tz>) {
    let tz = now.timezone();
    let date = now.date_naive();
    let start_date = match kind {
        WindowKind::Day => date,
        WindowKind::WeekMon => {
            // 回退到本周一（num_days_from_monday: Mon=0..Sun=6）
            date.checked_sub_days(Days::new(date.weekday().num_days_from_monday() as u64))
                .expect("date back to Monday underflows only far outside representable range")
        }
    };
    let days_ahead: u64 = match kind {
        WindowKind::Day => 1,
        WindowKind::WeekMon => 7,
    };
    let start = local_midnight(&tz, start_date).expect("midnight always representable");
    let end = local_midnight(
        &tz,
        start_date
            .checked_add_days(Days::new(days_ahead))
            .expect("window end date overflow"),
    )
    .expect("midnight always representable");
    (start, end)
}

/// 便捷包装：自然日边界。
pub fn day_bounds<Tz: TimeZone + Clone>(now: &DateTime<Tz>) -> (DateTime<Tz>, DateTime<Tz>) {
    window_bounds(WindowKind::Day, now)
}

/// 便捷包装：自然周（周一 00:00）边界。
pub fn week_bounds<Tz: TimeZone + Clone>(now: &DateTime<Tz>) -> (DateTime<Tz>, DateTime<Tz>) {
    window_bounds(WindowKind::WeekMon, now)
}

/// 窗口标签（用于 window_id / 账目审计）：Day → "2026-10-19"；WeekMon → "2026-W43"（ISO 周）。
pub fn window_label<Tz: TimeZone + Clone>(kind: WindowKind, now: &DateTime<Tz>) -> String {
    let d = now.date_naive();
    match kind {
        WindowKind::Day => d.format("%Y-%m-%d").to_string(),
        WindowKind::WeekMon => format!("{}-W{:02}", d.iso_week().year(), d.iso_week().week()),
    }
}

// ---------------------------------------------------------------------------
// 账本 schema 草案（裁决一的落点：单库形态即三服务共库各表；分库形态即本 DDL 拆三文件）
// ---------------------------------------------------------------------------

pub const SCHEMA_SQL: &str = r#"
-- ============ af-meter 账本 schema 草案 v0（poc-ledger） ============
-- 金额/数量一律整数最小单位（call / cent / token），禁 f64（af-meter RESULT ④-4、af-credential *_cents 对齐）。
-- 时间一律 unix ms INTEGER（af-station RESULT ④-4）。
-- 窗口边界按 meter_account.tz 对齐后以 UTC 时刻存储（unix ms），重放可由 (tz, window_kind) 复算。

CREATE TABLE IF NOT EXISTS ledger_meta (
  key   TEXT PRIMARY KEY,              -- 'schema_version' | 'fencing_epoch'
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS meter_account (
  subject     TEXT NOT NULL,           -- 被计量主体（agent id / workorder id）
  meter_id    TEXT NOT NULL,           -- 计量器路径 id（'subscription:daily' / 'subscription:weekly' / 'api:stock'）
  kind        TEXT NOT NULL CHECK (kind IN ('stock','window','concurrency','rate')),
  window_kind TEXT,                    -- window 型: 'day' | 'week_mon'；其他型 NULL
  tz          TEXT NOT NULL DEFAULT 'Asia/Shanghai',  -- 窗口对齐时区（IANA 名）
  unit        TEXT NOT NULL,           -- 'call' | 'cent' | 'token'
  capacity    INTEGER NOT NULL,        -- 初始额度 / 上限（最小单位）
  parent_id   TEXT,                    -- Min 组合树父节点；NULL = 根（Max 择优路由在内存做）
  created_at  INTEGER NOT NULL,        -- unix ms
  PRIMARY KEY (subject, meter_id),
  FOREIGN KEY (subject, parent_id) REFERENCES meter_account(subject, meter_id)
);

CREATE TABLE IF NOT EXISTS meter_window_state (   -- window 型当前窗口投影（可由事件重放重建）
  subject      TEXT NOT NULL,
  meter_id     TEXT NOT NULL,
  window_start INTEGER NOT NULL,       -- 对齐后的窗口起点 unix ms
  window_end   INTEGER NOT NULL,       -- 对齐后的窗口终点 unix ms（开区间；亦即 recovers_at）
  remaining    INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL,
  PRIMARY KEY (subject, meter_id),
  FOREIGN KEY (subject, meter_id) REFERENCES meter_account(subject, meter_id)
);

CREATE TABLE IF NOT EXISTS meter_event (          -- append-only，事实来源
  seq             INTEGER PRIMARY KEY AUTOINCREMENT,
  subject         TEXT NOT NULL,
  meter_id        TEXT NOT NULL,
  op              TEXT NOT NULL CHECK (op IN ('consume','reserve','release','refill','reset','adjust')),
  amount          INTEGER NOT NULL,
  window_start    INTEGER,              -- 记账时所属对齐窗口（stock 型 NULL）
  available_after INTEGER,              -- 事件后余额（审计友好；重放校验锚点）
  idempotency_key TEXT UNIQUE,          -- 显式命名幂等键（af-station 坑④-3：禁裸 ON CONFLICT DO NOTHING）
  ref_type        TEXT,                 -- 溯源：'workorder' / 'step' / ...
  ref_id          TEXT,
  epoch           INTEGER NOT NULL DEFAULT 0,   -- fencing 纪元（af-effect-gateway 语义）
  ts              INTEGER NOT NULL,     -- unix ms
  FOREIGN KEY (subject, meter_id) REFERENCES meter_account(subject, meter_id)
);
CREATE INDEX IF NOT EXISTS idx_event_window ON meter_event(subject, meter_id, window_start);
CREATE INDEX IF NOT EXISTS idx_event_ts     ON meter_event(ts);

CREATE TABLE IF NOT EXISTS meter_outbox (         -- 分库形态的跨库补偿信箱（本库=发起方）
  id         TEXT PRIMARY KEY,          -- = 对端库的幂等键
  payload    TEXT NOT NULL,             -- 目标操作（JSON：目标库/表/键/量）
  state      TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','done','dead')),
  created_at INTEGER NOT NULL,
  done_at    INTEGER,
  attempts   INTEGER NOT NULL DEFAULT 0
);
"#;

/// 开账本连接：WAL + busy_timeout 5s + synchronous NORMAL + foreign_keys ON（与 af-station 同款），
/// 再执行 SCHEMA_SQL。`:memory:` 时不设 WAL（内存库无意义且 pragma 报错）。
pub fn open_ledger(path: &str) -> rusqlite::Result<rusqlite::Connection> {
    let conn = rusqlite::Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    if path != ":memory:" {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
    }
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(SCHEMA_SQL)?;
    Ok(conn)
}

// ---------------------------------------------------------------------------
// 最小消费原型：惰性窗口重置 + 幂等 + Reading 语义（available / recovers_at）
// ---------------------------------------------------------------------------

/// 对齐 af-meter `Reading{available, recovers_at}`：recovers_at = 可用量下次严格增加的时刻。
/// window 型恒为窗口终点（对齐后的钟面重置点）；stock 型 None（不自动恢复）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub available: i64,
    pub recovers_at_ms: Option<i64>,
}

#[derive(Debug)]
pub enum Consume {
    /// 扣减成功（available_after 为扣后余额）
    Applied(Reading),
    /// 幂等键已存在：本次请求为重放，不产生第二条账目
    AlreadyApplied,
    /// 余量不足：recovers_at = 对齐窗口终点
    Insufficient { reading: Reading },
}

pub fn consume<Tz: TimeZone + Clone>(
    conn: &rusqlite::Connection,
    subject: &str,
    meter_id: &str,
    amount: i64,
    now: &DateTime<Tz>,
    idempotency_key: &str,
) -> rusqlite::Result<Consume> {
    // BEGIN IMMEDIATE：入口先取写锁，避免"deferred 读快照 → 写时升级"撞上
    // SQLITE_BUSY_SNAPSHOT（busy_timeout 不重试该错误）。busy_timeout=5s 在此生效排队。
    conn.execute("BEGIN IMMEDIATE", [])?;
    match consume_tx(conn, subject, meter_id, amount, now, idempotency_key) {
        Ok(outcome) => {
            conn.execute_batch("COMMIT")?;
            Ok(outcome)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

fn consume_tx<Tz: TimeZone + Clone>(
    conn: &rusqlite::Connection,
    subject: &str,
    meter_id: &str,
    amount: i64,
    now: &DateTime<Tz>,
    idempotency_key: &str,
) -> rusqlite::Result<Consume> {
    // 幂等快路径：键已存在即重放（UNIQUE 兜底并发路径，见下方 changes()==0 分支）
    let dup: Option<i64> = conn
        .query_row(
            "SELECT seq FROM meter_event WHERE idempotency_key = ?1",
            [idempotency_key],
            |r| r.get(0),
        )
        .optional()?;
    if dup.is_some() {
        return Ok(Consume::AlreadyApplied);
    }

    let (kind, window_kind, capacity): (String, Option<String>, i64) = conn.query_row(
        "SELECT kind, window_kind, capacity FROM meter_account WHERE subject=?1 AND meter_id=?2",
        rusqlite::params![subject, meter_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    let ts = now.timestamp_millis();
    let (ws_ms, we_ms, reading) = match (kind.as_str(), window_kind.as_deref()) {
        ("window", Some("day")) | ("window", Some("week_mon")) => {
            let wk = if window_kind.as_deref() == Some("day") {
                WindowKind::Day
            } else {
                WindowKind::WeekMon
            };
            let (start, end) = window_bounds(wk, now);
            let (ws, we) = (start.timestamp_millis(), end.timestamp_millis());
            // 惰性窗口重置：投影的窗口起点 ≠ 当前应属窗口起点 → 视为跨窗，重置余量
            let cur: Option<(i64, i64, i64)> = conn
                .query_row(
                    "SELECT window_start, window_end, remaining FROM meter_window_state
                     WHERE subject=?1 AND meter_id=?2",
                    rusqlite::params![subject, meter_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let remaining = match cur {
                Some((sws, _swe, rem)) if sws == ws => rem,
                Some(_) | None => {
                    conn.execute(
                        "INSERT INTO meter_window_state(subject,meter_id,window_start,window_end,remaining,updated_at)
                         VALUES(?1,?2,?3,?4,?5,?6)
                         ON CONFLICT(subject,meter_id) DO UPDATE SET
                           window_start=?3, window_end=?4, remaining=?5, updated_at=?6",
                        rusqlite::params![subject, meter_id, ws, we, capacity, ts],
                    )?;
                    capacity
                }
            };
            (ws, we, Reading { available: remaining, recovers_at_ms: Some(we) })
        }
        ("stock", _) => {
            let rem: i64 = conn
                .query_row(
                    "SELECT COALESCE((SELECT remaining FROM meter_window_state
                                      WHERE subject=?1 AND meter_id=?2), ?3)",
                    rusqlite::params![subject, meter_id, capacity],
                    |r| r.get(0),
                )?;
            (ts, ts, Reading { available: rem, recovers_at_ms: None })
        }
        other => {
            // window 缺 window_kind / concurrency / rate：原型不落库
            //（占坑与派生计量语义不同，见 af-meter RESULT ④-5）
            return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    format!("kind={other:?} 未入库原型（schema 已支持，语义另定）"),
                ),
            )));
        }
    };

    if reading.available < amount {
        // 余量不足不扣量；但上面发生的惰性窗口重置随事务一并提交（窗口翻页是事实，与本次扣减无关）
        return Ok(Consume::Insufficient { reading });
    }
    let after = reading.available - amount;
    let n = conn.execute(
        "INSERT INTO meter_event(subject,meter_id,op,amount,window_start,available_after,
                                 idempotency_key,ref_type,ref_id,epoch,ts)
         VALUES(?1,?2,'consume',?3,?4,?5,?6,NULL,NULL,0,?7)
         ON CONFLICT(idempotency_key) DO NOTHING",
        rusqlite::params![subject, meter_id, amount, ws_ms, after, idempotency_key, ts],
    )?;
    if n == 0 {
        // 并发下另一进程抢先用了同一幂等键 → 本事务不产生第二条账目
        return Ok(Consume::AlreadyApplied);
    }
    if ws_ms != we_ms {
        conn.execute(
            "UPDATE meter_window_state SET remaining=?3, updated_at=?4
             WHERE subject=?1 AND meter_id=?2",
            rusqlite::params![subject, meter_id, after, ts],
        )?;
    } else {
        // stock 型：无窗口，直接把余量写投影（不存在则初始化）
        conn.execute(
            "INSERT INTO meter_window_state(subject,meter_id,window_start,window_end,remaining,updated_at)
             VALUES(?1,?2,0,0,?3,?4)
             ON CONFLICT(subject,meter_id) DO UPDATE SET remaining=?3, updated_at=?4",
            rusqlite::params![subject, meter_id, after, ts],
        )?;
    }
    Ok(Consume::Applied(Reading { available: after, recovers_at_ms: reading.recovers_at_ms }))
}

/// outbox 补偿（分库形态跨库一致性，tests/consistency.rs 实测）：
/// 发起库事务内写业务行 + outbox(pending)；对端库幂等落回执；再标 done。
/// 崩溃恢复 = 扫描 pending 重放（对端以主键幂等，恰好一次）。
pub fn outbox_recover(
    src: &rusqlite::Connection,
    apply: &mut dyn FnMut(&str, &str) -> rusqlite::Result<()>,
) -> rusqlite::Result<usize> {
    let mut stmt = src.prepare("SELECT id, payload FROM meter_outbox WHERE state='pending'")?;
    let rows: Vec<(String, String)> =
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
    let mut n = 0;
    for (id, payload) in rows {
        apply(&id, &payload)?; // 对端幂等：同 id 重放不产生第二条
        src.execute(
            "UPDATE meter_outbox SET state='done', done_at=?2, attempts=attempts+1 WHERE id=?1",
            rusqlite::params![id, chrono::Utc::now().timestamp_millis()],
        )?;
        n += 1;
    }
    Ok(n)
}

// ---------------------------------------------------------------------------
// 测试辅助：为测试建两库原型（meter + gateway 两文件）
// ---------------------------------------------------------------------------

pub const GATEWAY_RECEIPT_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS effect_receipt (
  intent_id TEXT PRIMARY KEY,       -- = outbox.id = 幂等键
  payload   TEXT NOT NULL,
  applied_at INTEGER NOT NULL
);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Asia::Shanghai;
    use chrono_tz::Tz;

    fn at(y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Tz> {
        Shanghai.from_local_datetime(&chrono::NaiveDate::from_ymd_opt(y, m, d)
            .unwrap().and_hms_opt(h, mi, s).unwrap()).unwrap()
    }

    #[test]
    fn shanghai_day_and_week_smoke() {
        let now = at(2026, 10, 19, 14, 30, 0); // 周一
        let (ds, de) = day_bounds(&now);
        assert_eq!(ds.format("%Y-%m-%dT%H:%M:%S%:z").to_string(), "2026-10-19T00:00:00+08:00");
        assert_eq!(de.format("%Y-%m-%dT%H:%M:%S%:z").to_string(), "2026-10-20T00:00:00+08:00");
        let (ws, we) = week_bounds(&now);
        assert_eq!(ws.format("%Y-%m-%dT%H:%M:%S%:z").to_string(), "2026-10-19T00:00:00+08:00");
        assert_eq!(we.format("%Y-%m-%dT%H:%M:%S%:z").to_string(), "2026-10-26T00:00:00+08:00");
        assert_eq!((we - ws).num_hours(), 168);
    }
}
