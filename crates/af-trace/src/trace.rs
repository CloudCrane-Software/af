use crate::error::TraceError;
use crate::event::{now_ms, GapInfo, Level, TraceEvent};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// Trace 通道配置。两个 delay 字段用于在测试里模拟慢消费者/慢落盘（生产默认 0）。
#[derive(Debug, Clone)]
pub struct TraceConfig {
    /// critical 有界缓冲容量（满则 write_critical 返回 Err，fail-closed）
    pub critical_capacity: usize,
    /// normal 有界队列容量（满则丢弃并递增 gap_counter）
    pub normal_capacity: usize,
    /// critical 落盘线程每处理一条事件前模拟的耗时（µs），0 = 不模拟
    pub critical_consume_delay_us: u64,
    /// normal 消费线程每处理一条事件前模拟的耗时（µs），0 = 不模拟
    pub normal_consume_delay_us: u64,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            critical_capacity: 4096,
            normal_capacity: 4096,
            critical_consume_delay_us: 0,
            normal_consume_delay_us: 0,
        }
    }
}

/// 落盘结果：两个内存 sink，各存 JSON line（一条事件一行）。
/// 真实实现里把 consumer 换成写文件即可，接口不变。
#[derive(Debug, Clone)]
pub struct TraceSinks {
    pub critical: Arc<Mutex<Vec<String>>>,
    pub normal: Arc<Mutex<Vec<String>>>,
}

/// shutdown 之后返回的对账数据。
#[derive(Debug, Clone)]
pub struct ShutdownReport {
    /// 丢弃的 normal 事件总数（= gap_counter 终值）
    pub gap_counter: u64,
    /// 已通过 critical span 报告的丢弃总数（shutdown 后应 == gap_counter）
    pub gap_reported: u64,
    /// critical sink 落盘行数
    pub critical_persisted: usize,
    /// normal sink 落盘行数
    pub normal_persisted: usize,
    pub sinks: TraceSinks,
}

pub struct Trace {
    cfg: TraceConfig,
    /// 全局单调 seq，跨两个通道共用，供对账
    seq: AtomicU64,
    critical_tx: Mutex<Option<SyncSender<TraceEvent>>>,
    normal_tx: Mutex<Option<SyncSender<TraceEvent>>>,
    critical_handle: Mutex<Option<JoinHandle<()>>>,
    normal_handle: Mutex<Option<JoinHandle<()>>>,
    sinks: TraceSinks,
    gap_counter: AtomicU64,
    gap_reported: AtomicU64,
    /// 尚未成功报告进 critical 通道的丢弃（critical 也满时在此合并，不丢计数）
    pending_gap: Mutex<Option<GapInfo>>,
}

impl Trace {
    /// 创建双通道并启动两个后台落盘线程。
    pub fn spawn(cfg: TraceConfig) -> Arc<Self> {
        let (critical_tx, critical_rx) = sync_channel::<TraceEvent>(cfg.critical_capacity);
        let (normal_tx, normal_rx) = sync_channel::<TraceEvent>(cfg.normal_capacity);
        let sinks = TraceSinks {
            critical: Arc::new(Mutex::new(Vec::new())),
            normal: Arc::new(Mutex::new(Vec::new())),
        };
        let critical_handle = spawn_consumer(
            "critical",
            critical_rx,
            sinks.critical.clone(),
            cfg.critical_consume_delay_us,
        );
        let normal_handle = spawn_consumer(
            "normal",
            normal_rx,
            sinks.normal.clone(),
            cfg.normal_consume_delay_us,
        );
        Arc::new(Trace {
            cfg,
            seq: AtomicU64::new(0),
            critical_tx: Mutex::new(Some(critical_tx)),
            normal_tx: Mutex::new(Some(normal_tx)),
            critical_handle: Mutex::new(Some(critical_handle)),
            normal_handle: Mutex::new(Some(normal_handle)),
            sinks,
            gap_counter: AtomicU64::new(0),
            gap_reported: AtomicU64::new(0),
            pending_gap: Mutex::new(None),
        })
    }

    pub fn config(&self) -> &TraceConfig {
        &self.cfg
    }

    /// 当前丢弃计数（运行值，可随时读）。
    pub fn gap_counter(&self) -> u64 {
        self.gap_counter.load(Ordering::Relaxed)
    }

    /// **critical：同步写，fail-closed。**
    ///
    /// 有界缓冲满时立即返回 `Err(TraceError::CriticalFull)`，事件原样退还，
    /// 调用方必须处理（重试或中止）；返回 `Ok(seq)` 则保证该事件最终落盘。
    pub fn write_critical(
        &self,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<u64, TraceError> {
        let ev = self.build(Level::Critical, kind, payload, None);
        let seq = ev.seq;
        let guard = self.critical_tx.lock().unwrap();
        match guard.as_ref() {
            None => Err(TraceError::Shutdown(ev)),
            Some(tx) => match tx.try_send(ev) {
                Ok(()) => Ok(seq),
                Err(TrySendError::Full(e)) => Err(TraceError::CriticalFull(e)),
                Err(TrySendError::Disconnected(e)) => Err(TraceError::Shutdown(e)),
            },
        }
    }

    /// **normal：异步写，有损。**
    ///
    /// 有界队列满时立即丢弃该事件、`gap_counter += 1`，并尝试发出一条
    /// `kind="gap"` 的 critical span 报告缺口；critical 也满时合并进 pending，
    /// shutdown 时强制补发。绝不阻塞、绝不 panic。
    pub fn write_normal(&self, kind: &str, payload: serde_json::Value) {
        let ev = self.build(Level::Normal, kind, payload, None);
        let seq = ev.seq;
        let sent = {
            let guard = self.normal_tx.lock().unwrap();
            match guard.as_ref() {
                Some(tx) => tx.try_send(ev).is_ok(),
                None => false, // 已 shutdown：按丢弃处理（契约要求先停生产者）
            }
        };
        if !sent {
            self.record_gap(seq);
        }
    }

    /// 关闭流程（必须在所有生产者结束后调用）：
    /// 1) 关 normal 通道，等 normal 消费线程清空队列退出；
    /// 2) 强制补发 pending 的合并 gap span；
    /// 3) 关 critical 通道，等 critical 落盘线程清空退出。
    pub fn shutdown(&self) -> ShutdownReport {
        if let Some(tx) = self.normal_tx.lock().unwrap().take() {
            drop(tx);
        }
        if let Some(h) = self.normal_handle.lock().unwrap().take() {
            let _ = h.join();
        }
        self.flush_pending_gap();
        if let Some(tx) = self.critical_tx.lock().unwrap().take() {
            drop(tx);
        }
        if let Some(h) = self.critical_handle.lock().unwrap().take() {
            let _ = h.join();
        }
        let critical_persisted = self.sinks.critical.lock().unwrap().len();
        let normal_persisted = self.sinks.normal.lock().unwrap().len();
        ShutdownReport {
            gap_counter: self.gap_counter.load(Ordering::Relaxed),
            gap_reported: self.gap_reported.load(Ordering::Relaxed),
            critical_persisted,
            normal_persisted,
            sinks: self.sinks.clone(),
        }
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    fn build(
        &self,
        level: Level,
        kind: &str,
        payload: serde_json::Value,
        gap: Option<GapInfo>,
    ) -> TraceEvent {
        TraceEvent {
            seq: self.next_seq(),
            level,
            kind: kind.to_string(),
            payload,
            ts_ms: now_ms(),
            gap,
        }
    }

    /// 记一次丢弃：计数 +1，并尝试发 gap critical span；发不出去就在 pending 里合并。
    fn record_gap(&self, dropped_seq: u64) {
        let total = self.gap_counter.fetch_add(1, Ordering::Relaxed) + 1;
        let mut guard = self.pending_gap.lock().unwrap();
        let pending = guard.get_or_insert(GapInfo {
            total_dropped: total,
            count: 0,
            first_seq: dropped_seq,
            last_seq: dropped_seq,
        });
        pending.count += 1;
        pending.last_seq = dropped_seq;
        pending.total_dropped = total;
        let snapshot = pending.clone();
        let gap_ev = self.build(Level::Critical, "gap", json!({}), Some(snapshot.clone()));
        let tx_guard = self.critical_tx.lock().unwrap();
        if let Some(tx) = tx_guard.as_ref() {
            match tx.try_send(gap_ev) {
                Ok(()) => {
                    self.gap_reported
                        .fetch_add(snapshot.count, Ordering::Relaxed);
                    *guard = None;
                }
                Err(_) => { /* critical 也满：保留 pending，后续丢弃继续合并 */ }
            }
        }
    }

    /// shutdown 阶段强制补发 pending gap。此时 normal 已无新丢弃，
    /// critical 消费线程仍在清空缓冲，重试必然成功。
    fn flush_pending_gap(&self) {
        let Some(snapshot) = self.pending_gap.lock().unwrap().take() else {
            return;
        };
        let gap_ev = self.build(
            Level::Critical,
            "gap",
            json!({ "flushed_at_shutdown": true }),
            Some(snapshot.clone()),
        );
        let mut spins: u64 = 0;
        loop {
            let tx_guard = self.critical_tx.lock().unwrap();
            match tx_guard.as_ref() {
                None => break, // 通道已被关闭（重复 shutdown 等异常路径），放弃补发
                Some(tx) => match tx.try_send(gap_ev.clone()) {
                    Ok(()) => {
                        self.gap_reported
                            .fetch_add(snapshot.count, Ordering::Relaxed);
                        break;
                    }
                    Err(TrySendError::Disconnected(_)) => break,
                    Err(TrySendError::Full(_)) => {
                        drop(tx_guard);
                        std::thread::sleep(Duration::from_micros(100));
                        spins += 1;
                        if spins > 2_000_000 {
                            break; // 理论不可达；防御性上限
                        }
                    }
                },
            }
        }
    }
}

/// 落盘（内存 sink）后台线程：先模拟耗时再 recv，保证“突发写入期间消费者一条都
/// 处理不完”，让溢出/丢弃行为可确定性测试。所有 sender 关闭且队列清空后自然退出。
fn spawn_consumer(
    name: &str,
    rx: Receiver<TraceEvent>,
    sink: Arc<Mutex<Vec<String>>>,
    delay_us: u64,
) -> JoinHandle<()> {
    let delay = if delay_us > 0 {
        Some(Duration::from_micros(delay_us))
    } else {
        None
    };
    std::thread::Builder::new()
        .name(format!("af-trace-{name}"))
        .spawn(move || loop {
            if let Some(d) = delay {
                std::thread::sleep(d);
            }
            match rx.recv() {
                Ok(ev) => {
                    let line = serde_json::to_string(&ev).expect("trace event serialize");
                    sink.lock().unwrap().push(line);
                }
                Err(_) => break,
            }
        })
        .expect("spawn af-trace consumer thread")
}

#[cfg(test)]
pub fn parse_sink_line(line: &str) -> TraceEvent {
    serde_json::from_str(line).expect("sink line 是合法 JSON TraceEvent")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TraceError;
    use std::collections::HashSet;

    fn drain_lines(sink: &Mutex<Vec<String>>) -> Vec<TraceEvent> {
        sink.lock()
            .unwrap()
            .iter()
            .map(|l| parse_sink_line(l))
            .collect()
    }

    /// critical 缓冲满 → Err(fail-closed)，事件原样退还；腾出空间后重试成功且零丢失。
    #[test]
    fn critical_full_returns_err_and_event_is_preserved() {
        let t = Trace::spawn(TraceConfig {
            critical_capacity: 4,
            normal_capacity: 16,
            critical_consume_delay_us: 500_000, // 500ms/条，制造确定性积压（抗调度抖动）
            normal_consume_delay_us: 0,
        });
        let mut errs = Vec::new();
        let mut oks = Vec::new();
        for i in 0..6 {
            match t.write_critical("audit", json!({ "i": i })) {
                Ok(seq) => oks.push(seq),
                Err(e) => errs.push(e),
            }
        }
        assert_eq!(oks.len(), 4, "容量 4，前 4 条应成功");
        assert_eq!(errs.len(), 2, "后 2 条应 fail-closed 返回 Err");
        for (idx, e) in errs.iter().enumerate() {
            let ev = e.event();
            assert_eq!(ev.level, Level::Critical);
            assert_eq!(ev.kind, "audit");
            assert_eq!(
                ev.payload,
                json!({ "i": 4 + idx }),
                "Err 必须原样退还原事件"
            );
        }
        let rejected = errs[0].event().clone();
        // fail-closed 重试模式：不依赖精确 sleep，循环重试直到腾出空间（5s 超时兜底）
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let seq = loop {
            match t.write_critical("audit", rejected.payload.clone()) {
                Ok(seq) => break seq,
                Err(_) => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "重试超时：缓冲未被消费"
                    );
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        };
        let report = t.shutdown();
        let lines = drain_lines(&report.sinks.critical);
        let seqs: HashSet<u64> = lines.iter().map(|e| e.seq).collect();
        assert_eq!(lines.len(), 5);
        assert!(seqs.contains(&seq), "重试成功的事件必须落盘");
        assert!(
            !seqs.contains(&rejected.seq),
            "被拒事件在重试成功前不应出现"
        );
    }

    /// normal 队列满 → 丢弃并计数；gap_counter == 发送数 - 实际落盘数。
    #[test]
    fn normal_overflow_drops_and_counts_gap() {
        let t = Trace::spawn(TraceConfig {
            critical_capacity: 64,
            normal_capacity: 8,
            critical_consume_delay_us: 0,
            normal_consume_delay_us: 200_000, // 200ms/条，突发期间一条都消费不掉
        });
        for i in 0..20 {
            t.write_normal("step", json!({ "i": i }));
        }
        assert_eq!(t.gap_counter(), 12, "队列容量 8，20 条应丢弃 12 条");
        let report = t.shutdown();
        assert_eq!(report.gap_counter, 12);
        assert_eq!(report.normal_persisted, 8);
        assert_eq!(report.gap_reported, 12, "所有缺口最终都应被报告");
        let crit = drain_lines(&report.sinks.critical);
        let gap_sum: u64 = crit
            .iter()
            .filter_map(|e| e.gap.as_ref())
            .map(|g| g.count)
            .sum();
        assert_eq!(gap_sum, 12, "Σ(gap span.count) == gap_counter");
        assert!(
            crit.iter().any(|e| e.kind == "gap"),
            "丢弃必须产生 critical gap span"
        );
    }

    /// critical 也满时 gap span 合并（count>1），但计数不丢；shutdown 强制补发尾差。
    #[test]
    fn gap_spans_coalesce_when_critical_is_also_full() {
        let t = Trace::spawn(TraceConfig {
            critical_capacity: 2,
            normal_capacity: 8,
            critical_consume_delay_us: 500_000, // critical 落盘极慢 → gap span 只进得去 2 条
            normal_consume_delay_us: 200_000,
        });
        for i in 0..30 {
            t.write_normal("step", json!({ "i": i }));
        }
        assert_eq!(t.gap_counter(), 22);
        let report = t.shutdown();
        assert_eq!(report.gap_counter, 22);
        assert_eq!(
            report.gap_reported, 22,
            "shutdown 补发后 reported == counter"
        );
        assert_eq!(report.normal_persisted, 8);
        let crit = drain_lines(&report.sinks.critical);
        let gap_spans: Vec<&GapInfo> = crit.iter().filter_map(|e| e.gap.as_ref()).collect();
        assert!(
            gap_spans.len() < 22,
            "critical 也满时应发生合并（实际 gap span 数 {}）",
            gap_spans.len()
        );
        let gap_sum: u64 = gap_spans.iter().map(|g| g.count).sum();
        assert_eq!(gap_sum, 22, "合并不丢计数");
        assert!(
            gap_spans.iter().any(|g| g.count > 1),
            "至少一条合并 span（count>1）"
        );
    }

    /// 容量充足时零丢弃零丢失；shutdown 后再写 critical 返回 Err(Shutdown)。
    #[test]
    fn zero_loss_when_under_capacity_and_closed_channel_errors() {
        let t = Trace::spawn(TraceConfig::default());
        let mut crit_seqs = Vec::new();
        for i in 0..50 {
            crit_seqs.push(t.write_critical("audit", json!({ "i": i })).unwrap());
        }
        for i in 0..500 {
            t.write_normal("step", json!({ "i": i }));
        }
        assert_eq!(t.gap_counter(), 0);
        let report = t.shutdown();
        assert_eq!(report.gap_counter, 0);
        assert_eq!(report.normal_persisted, 500);
        let crit = drain_lines(&report.sinks.critical);
        let seqs: HashSet<u64> = crit.iter().map(|e| e.seq).collect();
        assert_eq!(crit.len(), 50);
        assert!(
            crit_seqs.iter().all(|s| seqs.contains(s)),
            "critical 零丢失"
        );
        assert!(crit.iter().all(|e| e.level == Level::Critical));

        let err = t.write_critical("after", json!(null)).unwrap_err();
        assert!(matches!(err, TraceError::Shutdown(_)));
        t.write_normal("after", json!(null)); // 已关闭：按丢弃计，不 panic
        assert_eq!(t.gap_counter(), 1);
    }
}
