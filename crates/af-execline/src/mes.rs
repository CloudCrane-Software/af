//! MES 事件文件 stub writer（ADR-004 过渡态）。
//!
//! 只用 af-mes 的 `MesEvent` 类型，JSONL 落 out 目录，不连 ClickHouse——
//! 证明新执行面从第一天起只走 af-mes 类型记账，不新立旁路 schema。
//! 目标态由 af-mes outbox（`MesWriter` + at-least-once 重发）承接；
//! 本 stub 的 `load` 供对账与测试重放。

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use af_mes::MesEvent;

use crate::error::ExeclineError;

/// MesEvent 的 JSONL 文件 writer。
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
    pub fn append(&mut self, event: &MesEvent) -> Result<(), ExeclineError> {
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
