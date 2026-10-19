//! `consume`：`BEGIN IMMEDIATE` 里做幂等扣减和惰性开窗。

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;

use super::bounds::window_bounds_tz;
use super::{
    count_from_i64, ms_to_time, EventRecord, Ledger, LedgerError, Result, Totals, WindowKind,
    WindowSnapshot,
};
use crate::{Count, Reading, Time};

/// 一次 `consume` 的结果。两种都带当前窗口的 [`Reading`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum ConsumeResult {
    /// 本事务首次入账，已经扣减并写入 outbox。
    Applied(Reading),
    /// 幂等键已存在且数量相同。没有再次扣减。
    AlreadyApplied(Reading),
}

impl ConsumeResult {
    pub const fn reading(self) -> Reading {
        match self {
            ConsumeResult::Applied(reading) | ConsumeResult::AlreadyApplied(reading) => reading,
        }
    }

    pub const fn is_duplicate(self) -> bool {
        matches!(self, ConsumeResult::AlreadyApplied(_))
    }
}

struct Charge<'a> {
    key: &'a str,
    amount: Count,
    amount_i64: i64,
    now_ms: i64,
    start_ms: i64,
    end_ms: i64,
    recovers_at: Time,
    capacity_i64: i64,
    meter_id: &'a str,
    timezone: &'a str,
    kind: WindowKind,
}

#[derive(Serialize)]
struct Compensation<'a> {
    idempotency_key: &'a str,
    meter_id: &'a str,
    amount: u64,
    window_start_ms: i64,
    window_end_ms: i64,
    at_ms: i64,
    kind: &'a str,
    timezone: &'a str,
}

enum ChargeOutcome {
    Applied(Reading),
    AlreadyApplied(Reading),
}

impl Ledger {
    /// 在 `now_ms` 所在的钟面窗口扣 `amount` 个计数。
    ///
    /// 幂等键已存在且数量相同：不扣、不新写 outbox，返回 [`ConsumeResult::AlreadyApplied`]。
    /// 数量不同：[`LedgerError::IdempotencyConflict`]，账本不变。
    /// 剩余不足：[`LedgerError::Insufficient`]，事务回滚，不留空窗口。
    ///
    /// 返回的 [`Reading::recovers_at`] 是该窗口终点的 Unix 毫秒。
    /// `now_ms` 由调用方注入，本函数不读墙钟。
    pub fn consume(&mut self, key: &str, amount: Count, now_ms: i64) -> Result<ConsumeResult> {
        if key.is_empty() {
            return Err(LedgerError::EmptyIdempotencyKey);
        }
        let amount_i64 = i64::try_from(amount).map_err(|_| LedgerError::BadAmount { amount })?;
        if amount_i64 <= 0 {
            return Err(LedgerError::BadAmount { amount });
        }
        let (start_ms, end_ms) = window_bounds_tz(now_ms, self.tz, self.decl.kind)?;
        let recovers_at = ms_to_time(end_ms)?;
        let kind = self.decl.kind;
        let capacity_i64 = self.capacity_i64;
        let meter_id = self.decl.meter_id.as_str();
        let timezone = self.decl.timezone.as_str();

        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let outcome = charge(
            &tx,
            &Charge {
                key,
                amount,
                amount_i64,
                now_ms,
                start_ms,
                end_ms,
                recovers_at,
                capacity_i64,
                meter_id,
                timezone,
                kind,
            },
        )?;
        match outcome {
            ChargeOutcome::Applied(reading) => {
                tx.commit()?;
                Ok(ConsumeResult::Applied(reading))
            }
            ChargeOutcome::AlreadyApplied(reading) => {
                tx.rollback()?;
                Ok(ConsumeResult::AlreadyApplied(reading))
            }
        }
    }

    /// 不写库。没有窗口行时剩余量等于容量。
    pub fn reading(&self, now_ms: i64) -> Result<Reading> {
        let (start_ms, end_ms) = window_bounds_tz(now_ms, self.tz, self.decl.kind)?;
        let consumed = peek_consumed(&self.conn, start_ms)?;
        reading_of(self.capacity_i64, consumed, end_ms)
    }

    /// 不写库。给出 `now_ms` 所在窗口的起止和已消耗。
    pub fn window_at(&self, now_ms: i64) -> Result<WindowSnapshot> {
        let (start_ms, end_ms) = window_bounds_tz(now_ms, self.tz, self.decl.kind)?;
        let row = self
            .conn
            .query_row(
                "SELECT end_ms, capacity, consumed FROM windows WHERE start_ms = ?1",
                params![start_ms],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        match row {
            None => Ok(WindowSnapshot {
                start_ms,
                end_ms,
                capacity: self.decl.capacity,
                consumed: 0,
                persisted: false,
            }),
            Some((stored_end, stored_cap, consumed)) => {
                if stored_end != end_ms || stored_cap != self.capacity_i64 {
                    return Err(LedgerError::SchemaDrift {
                        detail: format!(
                            "window {start_ms} stored end={stored_end} capacity={stored_cap}"
                        ),
                    });
                }
                Ok(WindowSnapshot {
                    start_ms,
                    end_ms,
                    capacity: count_from_i64(stored_cap)?,
                    consumed: count_from_i64(consumed)?,
                    persisted: true,
                })
            }
        }
    }

    pub fn event(&self, key: &str) -> Result<Option<EventRecord>> {
        let row = self
            .conn
            .query_row(
                "SELECT idempotency_key, amount, window_start_ms, at_ms
                 FROM events WHERE idempotency_key = ?1",
                params![key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((idempotency_key, amount, window_start_ms, at_ms)) => Ok(Some(EventRecord {
                idempotency_key,
                amount: count_from_i64(amount)?,
                window_start_ms,
                at_ms,
            })),
        }
    }

    pub fn totals(&self) -> Result<Totals> {
        let event_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
        let event_amount = sum_column(&self.conn, "SELECT amount FROM events")?;
        let window_consumed = sum_column(&self.conn, "SELECT consumed FROM windows")?;
        let outbox_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM outbox", [], |row| row.get(0))?;
        let outbox_unsent: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM outbox WHERE sent_at_ms IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(Totals {
            event_count: count_from_i64(event_count)?,
            event_amount: i128_to_count(event_amount)?,
            window_consumed: i128_to_count(window_consumed)?,
            outbox_count: count_from_i64(outbox_count)?,
            outbox_unsent: count_from_i64(outbox_unsent)?,
        })
    }
}

fn charge(tx: &Transaction<'_>, charge: &Charge<'_>) -> Result<ChargeOutcome> {
    if let Some(stored) = event_amount(tx, charge.key)? {
        if stored != charge.amount {
            return Err(LedgerError::IdempotencyConflict {
                key: charge.key.to_string(),
                stored,
                incoming: charge.amount,
            });
        }
        let consumed = peek_consumed(tx, charge.start_ms)?;
        let reading = reading_of(charge.capacity_i64, consumed, charge.end_ms)?;
        return Ok(ChargeOutcome::AlreadyApplied(reading));
    }

    let consumed = ensure_window(tx, charge.start_ms, charge.end_ms, charge.capacity_i64)?;
    if consumed < 0 || consumed > charge.capacity_i64 {
        return Err(LedgerError::SchemaDrift {
            detail: format!("window {} consumed {consumed}", charge.start_ms),
        });
    }
    let available = charge.capacity_i64 - consumed;
    let Some(next) = consumed.checked_add(charge.amount_i64) else {
        return Err(LedgerError::BadAmount {
            amount: charge.amount,
        });
    };
    if next > charge.capacity_i64 {
        return Err(LedgerError::Insufficient {
            need: charge.amount,
            available: count_from_i64(available)?,
            recovers_at: charge.recovers_at,
        });
    }
    let updated = tx.execute(
        "UPDATE windows SET consumed = ?1 WHERE start_ms = ?2 AND consumed = ?3",
        params![next, charge.start_ms, consumed],
    )?;
    if updated != 1 {
        return Err(LedgerError::LostUpdate);
    }
    tx.execute(
        "INSERT INTO events (idempotency_key, amount, window_start_ms, at_ms)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            charge.key,
            charge.amount_i64,
            charge.start_ms,
            charge.now_ms
        ],
    )?;
    let payload = serde_json::to_string(&Compensation {
        idempotency_key: charge.key,
        meter_id: charge.meter_id,
        amount: charge.amount,
        window_start_ms: charge.start_ms,
        window_end_ms: charge.end_ms,
        at_ms: charge.now_ms,
        kind: charge.kind.as_str(),
        timezone: charge.timezone,
    })?;
    tx.execute(
        "INSERT INTO outbox (idempotency_key, payload, created_at_ms, sent_at_ms)
         VALUES (?1, ?2, ?3, NULL)",
        params![charge.key, payload, charge.now_ms],
    )?;
    Ok(ChargeOutcome::Applied(reading_of(
        charge.capacity_i64,
        next,
        charge.end_ms,
    )?))
}

fn event_amount(conn: &Connection, key: &str) -> Result<Option<Count>> {
    let amount: Option<i64> = conn
        .query_row(
            "SELECT amount FROM events WHERE idempotency_key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()?;
    match amount {
        None => Ok(None),
        Some(value) => Ok(Some(count_from_i64(value)?)),
    }
}

fn ensure_window(tx: &Transaction<'_>, start_ms: i64, end_ms: i64, capacity: i64) -> Result<i64> {
    tx.execute(
        "INSERT INTO windows (start_ms, end_ms, capacity, consumed)
         VALUES (?1, ?2, ?3, 0)
         ON CONFLICT(start_ms) DO NOTHING",
        params![start_ms, end_ms, capacity],
    )?;
    let (stored_end, stored_cap, consumed): (i64, i64, i64) = tx.query_row(
        "SELECT end_ms, capacity, consumed FROM windows WHERE start_ms = ?1",
        params![start_ms],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if stored_end != end_ms || stored_cap != capacity {
        return Err(LedgerError::SchemaDrift {
            detail: format!(
                "window {start_ms} stored end={stored_end} capacity={stored_cap}, declared end={end_ms} capacity={capacity}"
            ),
        });
    }
    Ok(consumed)
}

fn peek_consumed(conn: &Connection, start_ms: i64) -> Result<i64> {
    let consumed: Option<i64> = conn
        .query_row(
            "SELECT consumed FROM windows WHERE start_ms = ?1",
            params![start_ms],
            |row| row.get(0),
        )
        .optional()?;
    Ok(consumed.unwrap_or(0))
}

fn reading_of(capacity_i64: i64, consumed: i64, end_ms: i64) -> Result<Reading> {
    if consumed < 0 || consumed > capacity_i64 {
        return Err(LedgerError::SchemaDrift {
            detail: format!("consumed {consumed} against capacity {capacity_i64}"),
        });
    }
    Ok(Reading {
        available: count_from_i64(capacity_i64 - consumed)?,
        recovers_at: Some(ms_to_time(end_ms)?),
    })
}

fn sum_column(conn: &Connection, sql: &str) -> Result<i128> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
    let mut sum: i128 = 0;
    for value in rows {
        sum += i128::from(value?);
    }
    Ok(sum)
}

fn i128_to_count(value: i128) -> Result<Count> {
    u64::try_from(value).map_err(|_| LedgerError::CorruptCount {
        value: i64::try_from(value).unwrap_or(i64::MAX),
    })
}
