//! outbox：与账本同一事务写入，未标记发出的行由 [`Ledger::recover`] 再送。

use std::fmt;

use rusqlite::{params, Connection};

use super::{Ledger, LedgerError, Result};

/// 一条待外发或已标记的补偿记录。`payload` 是 JSON。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxRecord {
    pub idempotency_key: String,
    pub payload: String,
    pub created_at_ms: i64,
    pub sent_at_ms: Option<i64>,
}

impl Ledger {
    /// 按入库顺序扫描 `sent_at_ms IS NULL` 的行，交给 `dispatch`。
    ///
    /// `dispatch` 返回 `Ok` 之后才把 `sent_at_ms` 写成注入的 `now_ms`。
    /// 回调失败则停在该条：该条仍未标记，之前已标记的保持原样。
    /// 进程若在回调成功和标记之间停掉，下次扫描会再送同一幂等键。
    /// 接收方用这个键去重，效果是恰好一次。
    ///
    /// 返回本次新标记的条数。
    pub fn recover<E, F>(&mut self, now_ms: i64, mut dispatch: F) -> Result<usize>
    where
        F: FnMut(&OutboxRecord) -> std::result::Result<(), E>,
        E: fmt::Display,
    {
        let pending = query_outbox(&self.conn, true)?;
        let mut marked = 0usize;
        for record in &pending {
            dispatch(record).map_err(|err| LedgerError::Dispatch(err.to_string()))?;
            let updated = self.conn.execute(
                "UPDATE outbox SET sent_at_ms = ?1
                 WHERE idempotency_key = ?2 AND sent_at_ms IS NULL",
                params![now_ms, record.idempotency_key],
            )?;
            if updated == 1 {
                marked += 1;
            }
        }
        Ok(marked)
    }

    /// 全部 outbox 行，按 `created_at_ms`、幂等键排序。
    pub fn outbox(&self) -> Result<Vec<OutboxRecord>> {
        query_outbox(&self.conn, false)
    }
}

fn query_outbox(conn: &Connection, unsent_only: bool) -> Result<Vec<OutboxRecord>> {
    let sql = if unsent_only {
        "SELECT idempotency_key, payload, created_at_ms, sent_at_ms
         FROM outbox
         WHERE sent_at_ms IS NULL
         ORDER BY created_at_ms, idempotency_key"
    } else {
        "SELECT idempotency_key, payload, created_at_ms, sent_at_ms
         FROM outbox
         ORDER BY created_at_ms, idempotency_key"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(OutboxRecord {
            idempotency_key: row.get(0)?,
            payload: row.get(1)?,
            created_at_ms: row.get(2)?,
            sent_at_ms: row.get(3)?,
        })
    })?;
    let mut records = Vec::new();
    for record in rows {
        records.push(record?);
    }
    Ok(records)
}
