//! 订单 ledger：append-only JSONL，序号连续，整单快照随行。
//!
//! 恢复只信盘上账本（落地契约 v0 §6.2）：重载 ledger 即得当前订单与已完成
//! op 的幂等键集合。序号不连续 / 坏行 / 身份不一致一律 EX2002 拒绝，
//! 不静默修补——账本损坏是事故，不是可猜的输入。

use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use af_workorder::WorkOrder;
use serde::{Deserialize, Serialize};

use crate::error::ExeclineError;

/// ledger 一行。`order_after` 让账本自成恢复源（无需另存状态文件）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerRecord {
    /// 从 1 开始的连续序号。
    pub seq: u64,
    /// Unix 毫秒（注入时钟）。
    pub ts_ms: i64,
    pub order_id: String,
    pub stable_id: String,
    /// `create_order` / `bind_line` / `attach_voucher` / `release` / `start` /
    /// `attach_artifact` / `complete` / `close` / `replay_skip`。
    pub op: String,
    /// `{stable_id}/{op}`——at-least-once + 幂等键的落点。
    pub idem_key: String,
    /// 本 op 是否被引擎放行（拒绝也入账，`ok=false`）。
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_detail: Option<String>,
    /// 该 op 之后的整单快照（拒绝行为空——快照不变，以上一条为准）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_after: Option<WorkOrder>,
}

/// 订单 ledger 句柄。
pub struct OrderLedger {
    path: PathBuf,
    seq: u64,
    records: Vec<LedgerRecord>,
}

impl OrderLedger {
    /// 打开账本：不存在则新建空账本；存在则重载并校验（序号连续、身份一致）。
    pub fn open(path: &Path) -> Result<Self, ExeclineError> {
        if path.is_file() {
            let records = Self::load(path)?;
            let seq = records.last().map(|record| record.seq).unwrap_or(0);
            Ok(Self {
                path: path.to_path_buf(),
                seq,
                records,
            })
        } else {
            Ok(Self {
                path: path.to_path_buf(),
                seq: 0,
                records: Vec::new(),
            })
        }
    }

    /// 只读重载 + 校验。对账与测试用；调用方不获得追加能力。
    ///
    /// 校验三条：每行是合法记录（坏行 = EX2002）；序号从 1 起连续
    /// （断层 = EX2002，不猜、不补）；全账本身份一致（一张账本一个订单）。
    pub fn load(path: &Path) -> Result<Vec<LedgerRecord>, ExeclineError> {
        if !path.is_file() {
            return Err(ExeclineError::LedgerIo {
                detail: format!("ledger 文件不存在: {}", path.display()),
            });
        }
        let file = fs::File::open(path)?;
        let mut records = Vec::new();
        for (index, line) in BufReader::new(file).lines().enumerate() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let record: LedgerRecord =
                serde_json::from_str(trimmed).map_err(|err| ExeclineError::LedgerCorrupt {
                    detail: format!("第 {} 行不是合法 ledger 记录: {err}", index + 1),
                })?;
            records.push(record);
        }
        for (index, record) in records.iter().enumerate() {
            let expected = index as u64 + 1;
            if record.seq != expected {
                return Err(ExeclineError::LedgerCorrupt {
                    detail: format!(
                        "序号不连续: 第 {} 行期望 seq={expected}，实际 seq={}",
                        index + 1,
                        record.seq
                    ),
                });
            }
        }
        if let Some(first) = records.first() {
            for record in &records[1..] {
                if record.stable_id != first.stable_id || record.order_id != first.order_id {
                    return Err(ExeclineError::LedgerCorrupt {
                        detail: format!(
                            "身份不一致: 首行 order_id={} stable_id={}，第 {} 行 order_id={} stable_id={}",
                            first.order_id,
                            first.stable_id,
                            record.seq,
                            record.order_id,
                            record.stable_id
                        ),
                    });
                }
            }
        }
        Ok(records)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }

    pub fn records(&self) -> &[LedgerRecord] {
        &self.records
    }

    /// 追加一行。序号由账本盖上（上一行 + 1），落盘（含 fsync）后才更新
    /// 内存视图——崩溃时内存不领先于盘。
    pub fn append(&mut self, mut record: LedgerRecord) -> Result<LedgerRecord, ExeclineError> {
        record.seq = self.seq + 1;
        let line = serde_json::to_string(&record).map_err(|err| ExeclineError::LedgerCorrupt {
            detail: format!("记录序列化失败: {err}"),
        })?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{line}")?;
        file.flush()?;
        file.sync_all()?;
        self.seq = record.seq;
        self.records.push(record.clone());
        Ok(record)
    }

    /// 已成功完成的幂等键集合（`ok=true` 的行）。驱动层据此跳过重放。
    pub fn done_idem_keys(&self) -> BTreeSet<&str> {
        self.records
            .iter()
            .filter(|record| record.ok)
            .map(|record| record.idem_key.as_str())
            .collect()
    }

    /// 某 op 成功次数（终态去重用：close 只允许一条成功记录）。
    pub fn count_ok(&self, op: &str) -> usize {
        self.records
            .iter()
            .filter(|record| record.ok && record.op == op)
            .count()
    }

    /// 最近一条带快照的行 → 当前订单。拒绝行不带快照，向前回溯。
    pub fn current_order(&self) -> Option<&WorkOrder> {
        self.records
            .iter()
            .rev()
            .find_map(|record| record.order_after.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(seq: u64, op: &str) -> LedgerRecord {
        LedgerRecord {
            seq,
            ts_ms: 1_767_225_600_000,
            order_id: "wo-ledger".into(),
            stable_id: "sid-wo-ledger".into(),
            op: op.into(),
            idem_key: format!("sid-wo-ledger/{op}"),
            ok: true,
            state_after: Some("Created".into()),
            error_kind: None,
            error_detail: None,
            order_after: None,
        }
    }

    #[test]
    fn append_then_load_roundtrips_with_contiguous_seq() {
        let dir = std::env::temp_dir().join(format!(
            "af-execline-ledger-unit-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ledger.jsonl");

        let mut ledger = OrderLedger::open(&path).unwrap();
        assert_eq!(ledger.seq(), 0);
        let first = ledger.append(record(0, "create_order")).unwrap();
        let second = ledger.append(record(0, "bind_line")).unwrap();
        assert_eq!(first.seq, 1, "序号从 1 起，由账本盖");
        assert_eq!(second.seq, 2);
        assert_eq!(ledger.done_idem_keys().len(), 2);
        assert_eq!(ledger.count_ok("bind_line"), 1);
        drop(ledger);

        let loaded = OrderLedger::load(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(
            loaded.iter().map(|r| r.seq).collect::<Vec<_>>(),
            vec![1, 2],
            "重载后序号连续"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn seq_gap_is_ledger_corrupt_not_silent_repair() {
        let dir = std::env::temp_dir().join(format!(
            "af-execline-ledger-gap-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ledger.jsonl");
        let mut text = String::new();
        for seq in [1u64, 3] {
            let mut r = record(seq, "bind_line");
            r.seq = seq;
            text.push_str(&serde_json::to_string(&r).unwrap());
            text.push('\n');
        }
        fs::write(&path, text).unwrap();

        let err = OrderLedger::load(&path).unwrap_err();
        assert_eq!(err.code(), crate::error::ErrorCode::LedgerCorrupt);
        assert!(err.to_string().contains("序号不连续"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }
}
