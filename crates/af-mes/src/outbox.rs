//! 失败批的文件 outbox。
//!
//! 一个文件是一整批。先写 `*.partial`，`sync_all` 文件和目录后再改名为 `*.batch`。
//! 改名成功才算落盘。重试成功后删文件；删之前崩溃会再送一次（至少一次）。
//! 不按墙钟命名，序号只保证同一目录里的先后。

use crate::error::MesError;
use crate::event::MesEvent;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

const VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct BatchHeader {
    v: u32,
    rows: usize,
}

#[derive(Debug)]
pub struct Outbox {
    dir: PathBuf,
    next_seq: u64,
    ids: Vec<String>,
}

impl Outbox {
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, MesError> {
        let dir = dir.into();
        fs::create_dir_all(&dir)
            .map_err(|err| MesError::Io(format!("创建 outbox 目录 {}: {err}", dir.display())))?;
        let mut ids = Vec::new();
        let mut max_seq = 0u64;
        let entries = fs::read_dir(&dir).map_err(|err| MesError::Io(err.to_string()))?;
        for entry in entries {
            let entry = entry.map_err(|err| MesError::Io(err.to_string()))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return Err(MesError::Io(format!(
                    "outbox 文件名不是 UTF-8: {}",
                    entry.path().display()
                )));
            };
            if name.ends_with(".partial") {
                fs::remove_file(entry.path()).map_err(|err| MesError::Io(err.to_string()))?;
                continue;
            }
            if let Some(stem) = name.strip_suffix(".batch") {
                let seq: u64 = stem.parse().map_err(|_| MesError::CorruptOutbox {
                    path: entry.path(),
                    detail: "批文件名不是序号".into(),
                })?;
                max_seq = max_seq.max(seq);
                ids.push(stem.to_string());
                continue;
            }
            return Err(MesError::Io(format!(
                "outbox 目录里有不认识的文件: {}",
                entry.path().display()
            )));
        }
        ids.sort();
        Ok(Self {
            dir,
            next_seq: max_seq.saturating_add(1),
            ids,
        })
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    /// 把这一批写成耐久文件。成功后调用方才能丢掉内存副本。
    pub fn park(&mut self, events: &[MesEvent]) -> Result<PathBuf, String> {
        if events.is_empty() {
            return Err("拒绝把空批写入 outbox".into());
        }
        let id = format!("{:016}", self.next_seq);
        let partial = self.dir.join(format!("{id}.partial"));
        let final_path = self.dir.join(format!("{id}.batch"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
            .map_err(|err| format!("创建 {}: {err}", partial.display()))?;
        let header = BatchHeader {
            v: VERSION,
            rows: events.len(),
        };
        let header_line = serde_json::to_string(&header).map_err(|err| err.to_string())?;
        writeln!(file, "{header_line}").map_err(|err| err.to_string())?;
        for event in events {
            serde_json::to_writer(&mut file, event).map_err(|err| err.to_string())?;
            file.write_all(b"\n").map_err(|err| err.to_string())?;
        }
        file.sync_all().map_err(|err| err.to_string())?;
        drop(file);
        fs::rename(&partial, &final_path)
            .map_err(|err| format!("改名 {}: {err}", final_path.display()))?;
        sync_dir(&self.dir)?;
        self.next_seq = self.next_seq.saturating_add(1);
        self.ids.push(id);
        Ok(final_path)
    }

    pub fn load(&self, id: &str) -> Result<Vec<MesEvent>, MesError> {
        let path = self.path(id);
        let file = File::open(&path).map_err(|err| MesError::Io(err.to_string()))?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let header_line = lines
            .next()
            .ok_or_else(|| MesError::CorruptOutbox {
                path: path.clone(),
                detail: "缺少头".into(),
            })?
            .map_err(|err| MesError::Io(err.to_string()))?;
        let header: BatchHeader =
            serde_json::from_str(&header_line).map_err(|err| MesError::CorruptOutbox {
                path: path.clone(),
                detail: format!("头不是 JSON: {err}"),
            })?;
        if header.v != VERSION {
            return Err(MesError::CorruptOutbox {
                path,
                detail: format!("不认识的 outbox 版本 {}", header.v),
            });
        }
        let mut events = Vec::with_capacity(header.rows);
        for line in lines {
            let line = line.map_err(|err| MesError::Io(err.to_string()))?;
            if line.is_empty() {
                continue;
            }
            let event = serde_json::from_str(&line).map_err(|err| MesError::CorruptOutbox {
                path: path.clone(),
                detail: format!("行无法解析: {err}"),
            })?;
            events.push(event);
        }
        if events.len() != header.rows {
            return Err(MesError::CorruptOutbox {
                path,
                detail: format!("头声明 {} 行，文件里有 {} 行", header.rows, events.len()),
            });
        }
        Ok(events)
    }

    pub fn remove(&mut self, id: &str) -> Result<(), MesError> {
        let path = self.path(id);
        fs::remove_file(&path).map_err(|err| MesError::Io(err.to_string()))?;
        sync_dir(&self.dir).map_err(MesError::Io)?;
        self.ids.retain(|existing| existing != id);
        Ok(())
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.batch"))
    }
}

fn sync_dir(dir: &Path) -> Result<(), String> {
    let file = File::open(dir).map_err(|err| format!("打开目录 {}: {err}", dir.display()))?;
    file.sync_all()
        .map_err(|err| format!("fsync 目录 {}: {err}", dir.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "af-mes-outbox-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        path
    }

    fn sample(id: &str) -> MesEvent {
        MesEvent::new("wo", 1_700_000_000_000, id, "line-a", "st", "start", 2)
    }

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn parked_batch_survives_reopen() {
        let dir = Scratch(scratch());
        let mut outbox = Outbox::open(&dir.0).unwrap();
        let events = vec![sample("e1"), sample("e2")];
        outbox.park(&events).unwrap();
        drop(outbox);

        let again = Outbox::open(&dir.0).unwrap();
        assert_eq!(again.len(), 1);
        let loaded = again.load(&again.ids()[0]).unwrap();
        assert_eq!(loaded, events);
    }

    #[test]
    fn partial_file_is_dropped_on_open() {
        let dir = Scratch(scratch());
        fs::create_dir_all(&dir.0).unwrap();
        fs::write(dir.0.join("0000000000000001.partial"), b"truncated").unwrap();
        let outbox = Outbox::open(&dir.0).unwrap();
        assert!(outbox.is_empty());
        assert!(!dir.0.join("0000000000000001.partial").exists());
    }
}
