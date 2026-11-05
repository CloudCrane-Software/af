//! J2 内存防线三读数消费腿（计划/产品化工作方案-v0.3 §二 J2 / 债表 J2 行）。
//!
//! 观测面：`tools/mem-readings.sh` 与 factory-keepalive 把三读数
//! （mem_available_kb / swap_used_kb / tmpfs_used_b）追加进
//! `状态观测/memory-readings.jsonl`。本件之前 af-mes 源码零引用 ⇒ 消费腿未建。
//!
//! 本模块是那条消费腿（库内件，由一次性进程/测试驱动，零守护线程）：
//!   · [`MemReadingsLog::open`] 只读解析 JSONL（路径由调用方注入，同端点注入约定）；
//!   · [`MemReadings::to_events`] 把一行读数映射成三个 [`MesEvent`]
//!     （kind 分读数名、quantity=读值、event_id 确定性生成——不调 `rand64()`，
//!     与 crate 纪律一致）。机器防线读数不是订单事件，真库订单级键行 > 0 不必；
//!   · [`MemDefenseGate`] 把三读数门化：阈值由调用方注入，越线即红；
//!     fail-closed——缺件/零可解析行/读不到最新行 ⇒ Err，调用方不得猜绿。
//!
//! 只读：不碰 ClickHouse、不起线程、不改观测面。

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::event::MesEvent;

/// 三读数事件共用的伪工单号。机器防线不是订单；`quantity` 承载读值后，
/// 按工单 + 时刻的排序键仍可对账回同一防线行。
pub const MEM_DEFENSE_WORK_ORDER: &str = "mem-defense";

/// 三个 kind 字面值。kind 本是自由字符串，但本腿消费/对账按这三个值过滤——
/// 改名 = 判据变更，须同批改测试。
pub const KIND_MEM_AVAILABLE_KB: &str = "mem.mem_available_kb";
pub const KIND_SWAP_USED_KB: &str = "mem.swap_used_kb";
pub const KIND_TMPFS_USED_B: &str = "mem.tmpfs_used_b";

/// 一行 `memory-readings.jsonl` 的三读数视图。
///
/// 全量行（mem-readings.sh 手跑版）另带 tmpfs_total/tmpfs_avail/shmem/daemons
/// 等字段——serde 忽略未知字段，本腿只消费契约三读数；三读数缺一即整行作废
/// （记入坏行，见 [`MemReadingsLog::bad_lines`]）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemReadings {
    /// ISO8601 原文，审计面保留。
    pub ts: String,
    /// epoch 秒。`occurred_at` 锚（毫秒 = epoch × 1000）。
    pub epoch: i64,
    /// 采集者：factory-keepalive | mem-readings。进 `station_id`。
    pub src: String,
    /// /tmp tmpfs 已用字节（`df -B1`）。
    pub tmpfs_used_b: u64,
    /// /proc/meminfo MemAvailable（KB）。
    pub mem_available_kb: u64,
    /// SwapTotal − SwapFree（KB，meminfo 无直读键）。
    pub swap_used_kb: u64,
}

impl MemReadings {
    /// MES 事件面时刻：epoch 秒 → 毫秒。
    pub fn occurred_at_ms(&self) -> i64 {
        self.epoch.saturating_mul(1000)
    }

    /// 一行读数 → 三个 MES 事件（每读数一个，`quantity` = 读值）。
    ///
    /// `line_id` 由调用方注入（机器/防线别名）；`station_id` 取采集者 src。
    /// `event_id` 确定性：`mem-defense:{epoch}:{kind}`——同一行重复摄入时
    /// 调用方按 `event_id` 自行认重；本腿不生成 `rand64()`。
    pub fn to_events(&self, line_id: impl Into<String>) -> [MesEvent; 3] {
        let line_id = line_id.into();
        let mk = |kind: &'static str, quantity: u64| {
            MesEvent::new(
                MEM_DEFENSE_WORK_ORDER,
                self.occurred_at_ms(),
                format!("{MEM_DEFENSE_WORK_ORDER}:{}:{kind}", self.epoch),
                line_id.clone(),
                self.src.clone(),
                kind,
                quantity,
            )
        };
        [
            mk(KIND_MEM_AVAILABLE_KB, self.mem_available_kb),
            mk(KIND_SWAP_USED_KB, self.swap_used_kb),
            mk(KIND_TMPFS_USED_B, self.tmpfs_used_b),
        ]
    }
}

/// 消费腿错误。fail-closed：观测面不可证时宁可 Err，调用方不得猜绿。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MemDefenseError {
    /// 文件缺件/不可读。
    #[error("内存防线读数不可读 {path}: {detail}")]
    Unreadable { path: PathBuf, detail: String },
    /// 文件在但零可解析行（坏行计数随报错上抛，留痕）。
    #[error("内存防线读数零可解析行 {path}（坏行 {bad_lines}）")]
    NoParseableRows { path: PathBuf, bad_lines: usize },
}

impl MemDefenseError {
    fn unreadable(path: &Path, err: impl std::fmt::Display) -> Self {
        Self::Unreadable {
            path: path.to_path_buf(),
            detail: err.to_string(),
        }
    }
}

/// 只读载入的读数面。坏行逐条记账可查（[`MemReadingsLog::bad_lines`]），不静默。
#[derive(Debug, Clone)]
pub struct MemReadingsLog {
    path: PathBuf,
    rows: Vec<MemReadings>,
    bad_lines: Vec<String>,
}

impl MemReadingsLog {
    /// 解析整份 JSONL。只读；坏行逐条记账，零可解析行 ⇒ Err（fail-closed）。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MemDefenseError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|e| MemDefenseError::unreadable(path, e))?;
        let mut rows = Vec::new();
        let mut bad_lines = Vec::new();
        for (idx, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<MemReadings>(line) {
                Ok(row) => rows.push(row),
                Err(e) => bad_lines.push(format!("L{}: {e}", idx + 1)),
            }
        }
        if rows.is_empty() {
            return Err(MemDefenseError::NoParseableRows {
                path: path.to_path_buf(),
                bad_lines: bad_lines.len(),
            });
        }
        Ok(Self {
            path: path.to_path_buf(),
            rows,
            bad_lines,
        })
    }

    /// 来源路径（对账面）。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 全部可解析行（文件顺序）。
    pub fn rows(&self) -> &[MemReadings] {
        &self.rows
    }

    /// 坏行明细（行号 + serde 报错）。只读消费不因坏行转红——判据面留痕，
    /// 门化走 [`MemDefenseGate`] 的 fail-closed 分支。
    pub fn bad_lines(&self) -> &[String] {
        &self.bad_lines
    }

    /// 最新一行 = epoch 最大者；并列取文件中最后者（append 序即时间序）。
    pub fn latest(&self) -> Result<&MemReadings, MemDefenseError> {
        // open 已保证 rows 非空；这里仍走防御形态，防未来直接构造的空日志。
        self.rows
            .iter()
            .enumerate()
            .max_by_key(|(idx, row)| (row.epoch, *idx))
            .map(|(_, row)| row)
            .ok_or_else(|| MemDefenseError::NoParseableRows {
                path: self.path.clone(),
                bad_lines: self.bad_lines.len(),
            })
    }

    /// 近 n 行窗口（可解析行、文件序）。
    pub fn window(&self, n: usize) -> &[MemReadings] {
        let start = self.rows.len().saturating_sub(n);
        &self.rows[start..]
    }
}

/// 三线门化裁决。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemDefenseVerdict {
    /// 三线全在阈值内。
    Pass,
    /// 越线清单（人读明细）；机读判据 = 非 Pass 即红。
    Breach(Vec<String>),
}

impl MemDefenseVerdict {
    /// 机读门位：true = 绿。
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }
}

/// 三读数阈值门。阈值由调用方注入（各机器防线预算不同，crate 不内置预算）。
///
/// 三线联防口径（T4 终裁 #3）：单看一线会误判——tmpfs 多数已换出到 swap，
/// 清 /tmp 的即时收益主要在 swap 空间，三线必须联防着看。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemDefenseGate {
    /// MemAvailable 下界（KB）。低于即红。
    pub min_mem_available_kb: u64,
    /// SwapUsed 上界（KB）。高于即红。
    pub max_swap_used_kb: u64,
    /// /tmp tmpfs 已用字节上界。高于即红。
    pub max_tmpfs_used_b: u64,
}

impl MemDefenseGate {
    /// 判一行读数。三读数缺一的情形在解析层已被整行作废，到不了这里。
    pub fn judge(&self, row: &MemReadings) -> MemDefenseVerdict {
        let mut breach = Vec::new();
        if row.mem_available_kb < self.min_mem_available_kb {
            breach.push(format!(
                "mem_available_kb={} 低于下界 {}",
                row.mem_available_kb, self.min_mem_available_kb
            ));
        }
        if row.swap_used_kb > self.max_swap_used_kb {
            breach.push(format!(
                "swap_used_kb={} 高于上界 {}",
                row.swap_used_kb, self.max_swap_used_kb
            ));
        }
        if row.tmpfs_used_b > self.max_tmpfs_used_b {
            breach.push(format!(
                "tmpfs_used_b={} 高于上界 {}",
                row.tmpfs_used_b, self.max_tmpfs_used_b
            ));
        }
        if breach.is_empty() {
            MemDefenseVerdict::Pass
        } else {
            MemDefenseVerdict::Breach(breach)
        }
    }

    /// 直接门最新一行；读数面不可证 ⇒ Err——fail-closed，调用方不得当绿。
    pub fn judge_latest(&self, log: &MemReadingsLog) -> Result<MemDefenseVerdict, MemDefenseError> {
        Ok(self.judge(log.latest()?))
    }
}
