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

    /// 追加一行事件（JSONL，append-only）。
    pub fn append(&mut self, _event: &MesEvent) -> Result<(), ExeclineError> {
        unimplemented!("RED：MesFileWriter::append 待最小实现")
    }

    /// 重载全部事件。坏行 = EX2002 账本类损坏（事件账本与订单账本同纪律）。
    pub fn load(_path: &Path) -> Result<Vec<MesEvent>, ExeclineError> {
        unimplemented!("RED：MesFileWriter::load 待最小实现")
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
