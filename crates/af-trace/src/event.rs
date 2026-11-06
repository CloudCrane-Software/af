use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// 事件分级。写入通道的方法决定级别（`write_critical` / `write_normal`），
/// 事件本身携带级别供落盘记录与验证方审计。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Critical,
    Normal,
}

/// 一条 `kind="gap"` 的 critical span 所携带的缺口信息。
///
/// `count` 可以 > 1：当 critical 缓冲也满时，多次丢弃会合并进同一条 gap span，
/// 但计数不丢——所有 gap span 的 `count` 之和恒等于 gap_counter。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GapInfo {
    /// 发出该 span 时已丢弃 normal 事件的累计总数（运行值）
    pub total_dropped: u64,
    /// 该 span 覆盖的丢弃条数（合并后可 > 1）
    pub count: u64,
    /// 被丢弃 normal 事件的 seq 区间（闭区间）
    pub first_seq: u64,
    pub last_seq: u64,
}

/// 一条追踪事件。`seq` 由 Trace 全局单调分配，跨两个通道共用一个计数器，
/// 便于验证方精确对账（谁被丢弃、谁被落盘）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceEvent {
    pub seq: u64,
    pub level: Level,
    pub kind: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    pub ts_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<GapInfo>,
}

/// 把 gap span 的 `count` 加总。这是丢弃条数，不是 span 行数。
pub fn sum_gap_counts(events: &[TraceEvent]) -> u64 {
    events
        .iter()
        .filter_map(|event| event.gap.as_ref())
        .map(|gap| gap.count)
        .sum()
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
