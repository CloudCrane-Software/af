//! MES 记账写缝（正式）+ 文件正本 writer。
//!
//! ADR-004 过渡态的文件 stub 在消债 B② 升格为缝的文件实现：正本
//! `mes-events.jsonl`（JSONL，权威记录——重放驱动的对账与补送都以它为准）
//! 由 [`MesFileWriter`] 恒定承载；CH 下游 [`crate::mes_ch::MesChSink`] 在
//! 同一缝上做读侧 event_id 对账后补差量（消债 B③，替换补丁期 marker）。
//! 驱动层只认 [`MesSink`]，订单面只声明目标（`MesChTarget`），不做写侧。

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use af_mes::MesEvent;
use serde::Serialize;

use crate::error::ExeclineError;

/// 驱动层 MES 写缝（正式；消债 B②）。
///
/// 两个面分开：
/// - `append`（记账面）：驱动每次生命周期跃迁调用。文件正本必写；
///   CH 下游不在行级触发——批语义由对账面统一。
/// - `sync`（对账面）：`drive` 收尾调用。文件实现 no-op；CH 实现 =
///   retry_outbox → 读侧按 `work_order_id` 对账 event_id → 补差量送批，
///   Parked = EX2003 fail-loud（真实落账，不接受"可能已写"）。
pub trait MesSink {
    /// 记账面：追加一条事件（文件正本必写）。
    fn append(&mut self, event: &MesEvent) -> Result<(), ExeclineError>;
    /// 对账面：驱动收尾对账/送批（文件实现 = no-op 报告）。
    fn sync(&mut self) -> Result<MesSyncReport, ExeclineError>;
    /// 正本文件路径。
    fn path(&self) -> &Path;
    /// 正本里是否已有该 event_id（红队 W1-C35：重放路径的 MES 补发对账
    /// ——正本文件缺席 = 未发；文件/CH 两实现正本同文件，默认实现通用）。
    fn has_event(&self, event_id: &str) -> Result<bool, ExeclineError> {
        if !self.path().is_file() {
            return Ok(false);
        }
        let events = MesFileWriter::load(self.path())?;
        Ok(events.iter().any(|e| e.event_id == event_id))
    }
}

/// 缝实现类别（回执自描述）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MesSyncKind {
    /// 仅文件正本（冒烟/无 CH 下游；sync = no-op）。
    FileStub,
    /// ClickHouse 下游（读侧 event_id 对账 + 差量送批）。
    ClickHouse,
}

/// 一次 `sync` 的回执。凭据零落：`endpoint_host` 只含 host:port。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MesSyncReport {
    pub kind: MesSyncKind,
    /// 正本事件总数。
    pub events_total: usize,
    /// 读侧对账认定的已在 CH 事件数。
    pub already_in_ch: usize,
    /// 本次补送差量事件数。
    pub shipped: usize,
    /// CH 实际接收行数（push/flush Sent 之和）。
    pub rows_delivered: usize,
    /// 送批后 outbox 残留批数。
    pub outbox_batches: usize,
    /// 补送的 event_id 清单（差量证据）。
    pub shipped_event_ids: Vec<String>,
    /// CH 端点（只含 host:port；文件实现为空串）。
    pub endpoint_host: String,
    /// 是否带口令认证（凭据本体永不进回执）。
    pub auth: bool,
}

impl MesSyncReport {
    /// 文件 stub 的 no-op 回执。
    pub fn file_stub() -> Self {
        Self {
            kind: MesSyncKind::FileStub,
            events_total: 0,
            already_in_ch: 0,
            shipped: 0,
            rows_delivered: 0,
            outbox_batches: 0,
            shipped_event_ids: Vec::new(),
            endpoint_host: String::new(),
            auth: false,
        }
    }
}

/// MesEvent 的 JSONL 文件 writer（缝的文件实现：正本权威记录）。
pub struct MesFileWriter {
    path: PathBuf,
}

impl MesFileWriter {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 追加一行事件（JSONL，append-only，fsync 后返回）。
    pub fn append_event(&mut self, event: &MesEvent) -> Result<(), ExeclineError> {
        let line = serde_json::to_string(event).map_err(|err| ExeclineError::LedgerCorrupt {
            detail: format!("MesEvent 序列化失败: {err}"),
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
        Ok(())
    }

    /// 重载全部事件。坏行 = EX2002（事件账本与订单账本同纪律）。
    pub fn load(path: &Path) -> Result<Vec<MesEvent>, ExeclineError> {
        if !path.is_file() {
            return Err(ExeclineError::LedgerIo {
                detail: format!("MES 事件文件不存在: {}", path.display()),
            });
        }
        let file = fs::File::open(path)?;
        let mut events = Vec::new();
        for (index, line) in BufReader::new(file).lines().enumerate() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let event: MesEvent =
                serde_json::from_str(trimmed).map_err(|err| ExeclineError::LedgerCorrupt {
                    detail: format!("第 {} 行不是合法 MesEvent: {err}", index + 1),
                })?;
            events.push(event);
        }
        Ok(events)
    }
}

impl MesSink for MesFileWriter {
    fn append(&mut self, event: &MesEvent) -> Result<(), ExeclineError> {
        self.append_event(event)
    }

    fn sync(&mut self) -> Result<MesSyncReport, ExeclineError> {
        Ok(MesSyncReport::file_stub())
    }

    fn path(&self) -> &Path {
        self.path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_then_load_roundtrips_mes_events() {
        let dir = std::env::temp_dir().join(format!(
            "af-execline-mes-unit-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mes-events.jsonl");

        let mut writer = MesFileWriter::new(&path);
        let event = MesEvent::new(
            "sid-wo-mes",
            1_767_225_600_000,
            "sid-wo-mes:release:1",
            "line-poc",
            "st-poc",
            "release",
            1,
        );
        writer.append(&event).unwrap();

        let loaded = MesFileWriter::load(&path).unwrap();
        assert_eq!(loaded, vec![event], "MesEvent 原样往返（字段不改写）");
        let _ = fs::remove_dir_all(&dir);
    }
}
