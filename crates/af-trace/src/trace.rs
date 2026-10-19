use crate::anchor::{self, AnchorHandle, AnchorSettings};
use crate::clock::{Clock, SystemClock};
use crate::error::TraceError;
use crate::event::{sum_gap_counts, GapInfo, Level, TraceEvent};
use crate::otel::{self, OtelMode};
use crate::store::{store_path, OpenOutcome, Store};
use serde_json::json;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

/// 注入失败的哨兵。生产路径保持这个值，落盘线程不会走注入分支。
const NO_INJECTED_FAIL: u64 = u64::MAX;
const INJECTED_CRITICAL_FAIL: &str = "injected critical persist failure";

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
    /// JSONL 与 `trace.db` 的目录，由调用方给出。空路径会在打开时失败。
    pub dir: PathBuf,
    /// OTLP HTTP 端点，由配方层写入，原样交给 `with_endpoint`（完整 URL，含路径）。
    /// `None` 或空白表示非关键 lane 离线，只计数。
    /// 例：`http://127.0.0.1:4318/v1/traces`。feature `otel` 关闭时忽略该值，lane 仍离线。
    pub otel_endpoint: Option<String>,
    /// OTLP 批大小，由配方层写入。`0` 按 512。在线时也是该 lane 的有界队列容量。
    pub otel_batch_size: usize,
    /// critical 事件的 rekor 锚定。端点为空则关闭，不改变落盘语义。
    pub anchor: AnchorSettings,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            critical_capacity: 4096,
            normal_capacity: 4096,
            critical_consume_delay_us: 0,
            normal_consume_delay_us: 0,
            dir: PathBuf::new(),
            otel_endpoint: None,
            otel_batch_size: 512,
            anchor: AnchorSettings::default(),
        }
    }
}

fn configured_endpoint(cfg: &TraceConfig) -> Option<&str> {
    cfg.otel_endpoint
        .as_deref()
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
}

/// 本次进程的镜像：各存 JSON line（一条事件一行）。
///
/// 镜像只在对应写盘成功后追加，因此 shutdown 成功时与本次写入的文件后缀一致。
/// 持久事实以目录里的 `critical.jsonl` / `normal.jsonl` 为准；重开后的历史不在镜像里。
/// [`Trace::hold_consumers`] 拿的就是这两把锁，落盘线程在写文件之前获取。
#[derive(Debug, Clone)]
pub struct TraceSinks {
    pub critical: Arc<Mutex<Vec<String>>>,
    pub normal: Arc<Mutex<Vec<String>>>,
}

/// shutdown 之后返回的对账数据。
#[derive(Debug, Clone)]
pub struct ShutdownReport {
    /// 丢弃的 normal 事件总数（= gap_counter 终值，含本次打开时已持久化的基数）
    pub gap_counter: u64,
    /// 已通过 critical span 报告的丢弃总数（干净 shutdown 后应 == gap_counter）
    pub gap_reported: u64,
    /// 本次进程写入 critical 镜像的行数（不含重开前的历史）
    pub critical_persisted: usize,
    /// 本次进程写入 normal 镜像的行数（不含重开前的历史）
    pub normal_persisted: usize,
    pub sinks: TraceSinks,
    /// 关闭时非关键 lane 的形态。
    pub otel_mode: OtelMode,
    /// 离线：只计数的 normal 条数。在线：导出成功的条数。失败进 `gap_counter`。
    pub otel_counted: u64,
    /// 已落盘但还没有锚定成功的 critical 事件数。
    /// 与 [`ShutdownReport::gap_counter`] 分开：后者只计丢弃，`Σ(gap.count) == gap_counter` 不变。
    pub anchor_gap: u64,
    /// rekor 不可达，或锚定队列里还有事件。
    pub anchor_alert: bool,
    /// 告警原因。没有告警时为 `None`。
    pub anchor_alert_reason: Option<String>,
}

/// [`Trace::hold_consumers`] 的守卫。drop 之后落盘线程继续排空队列。
///
/// 调用 [`Trace::shutdown`] 之前必须先 drop，否则落盘线程拿不到镜像锁，
/// shutdown 补发缺口时会一直看到 critical 缓冲为满。
#[must_use = "drop 之后落盘线程才会继续排空队列"]
pub struct ConsumerHold<'a> {
    _critical: MutexGuard<'a, Vec<String>>,
    _normal: MutexGuard<'a, Vec<String>>,
}

#[derive(Clone, Copy)]
enum Lane {
    Critical,
    Normal,
}

impl Lane {
    fn name(self) -> &'static str {
        match self {
            Lane::Critical => "critical",
            Lane::Normal => "normal",
        }
    }
}

struct Inner {
    cfg: TraceConfig,
    seq: AtomicU64,
    critical_tx: Mutex<Option<SyncSender<TraceEvent>>>,
    normal_tx: Mutex<Option<SyncSender<TraceEvent>>>,
    sinks: TraceSinks,
    gap_counter: AtomicU64,
    gap_reported: AtomicU64,
    pending_gap: Mutex<Option<GapInfo>>,
    persist_error: Mutex<Option<String>>,
    unpersisted: Mutex<Vec<TraceEvent>>,
    clock: Arc<dyn Clock>,
    store: Mutex<Store>,
    fail_critical_after: AtomicU64,
    fail_normal_after: AtomicU64,
    otel_mode: OtelMode,
    otel_counted: Arc<AtomicU64>,
    otel_offline_reason: Mutex<Option<String>>,
    otel_tx: Mutex<Option<SyncSender<TraceEvent>>>,
    /// `try_send` 成功的次数。单测用来确认批次已经进队列，再放开被挡住的 exporter。
    #[cfg(all(test, feature = "otel"))]
    otel_queued: AtomicU64,
    anchor: Option<AnchorHandle>,
}

pub struct Trace {
    inner: Arc<Inner>,
    critical_handle: Mutex<Option<JoinHandle<()>>>,
    normal_handle: Mutex<Option<JoinHandle<()>>>,
    otel_handle: Mutex<Option<JoinHandle<()>>>,
    anchor_join: Mutex<Option<JoinHandle<()>>>,
}

impl Trace {
    /// 创建双通道并启动两个后台落盘线程。等价于 [`Trace::open`] 丢掉打开结果。
    #[allow(clippy::result_large_err)]
    pub fn spawn(cfg: TraceConfig) -> Result<Arc<Self>, TraceError> {
        Ok(Self::open(cfg)?.0)
    }

    /// 打开调用方目录里的落盘。时刻用 [`SystemClock`]。
    #[allow(clippy::result_large_err)]
    pub fn open(cfg: TraceConfig) -> Result<(Arc<Self>, OpenOutcome), TraceError> {
        Self::open_with_clock(cfg, SystemClock::new())
    }

    /// 同 [`Trace::open`]，事件的 `ts_ms` 走调用方注入的钟。
    #[allow(clippy::result_large_err)]
    pub fn open_with_clock(
        cfg: TraceConfig,
        clock: impl Clock + 'static,
    ) -> Result<(Arc<Self>, OpenOutcome), TraceError> {
        let prepared = otel::prepare(configured_endpoint(&cfg), cfg.otel_batch_size);
        Self::open_prepared(cfg, Arc::new(clock), prepared)
    }

    /// 用 mock exporter 代替 OTLP。端点配置不参与安装；lane 视为在线。
    #[cfg(all(test, feature = "otel"))]
    fn open_with_mock(
        cfg: TraceConfig,
        clock: impl Clock + 'static,
        mock: crate::otel::MockExporter,
    ) -> Result<(Arc<Self>, OpenOutcome), TraceError> {
        let prepared = otel::prepare_with_mock(mock, cfg.otel_batch_size);
        Self::open_prepared(cfg, Arc::new(clock), prepared)
    }

    #[allow(clippy::result_large_err)]
    fn open_prepared(
        cfg: TraceConfig,
        clock: Arc<dyn Clock>,
        mut prepared: otel::Prepared,
    ) -> Result<(Arc<Self>, OpenOutcome), TraceError> {
        if cfg.dir.as_os_str().is_empty() {
            return Err(TraceError::Open(
                "TraceConfig.dir 为空，调用方必须提供落盘目录".into(),
            ));
        }
        fs::create_dir_all(&cfg.dir)
            .map_err(|err| TraceError::Open(format!("创建目录 {}: {err}", cfg.dir.display())))?;
        reject_orphan_logs(&cfg.dir)?;

        let (store, outcome) = Store::open(&cfg.dir)?;
        let gap = store.gap_state()?;
        let critical_path = critical_log_path(&cfg.dir);
        let normal_path = normal_log_path(&cfg.dir);
        let critical_history = read_trace_log(&critical_path)?;
        let normal_history = read_trace_log(&normal_path)?;
        let file_sum = sum_gap_counts(&critical_history);
        if gap.gap_counter != file_sum || gap.gap_reported != file_sum {
            return Err(TraceError::Inconsistent(format!(
                "缺口计数器与 critical 文件不一致: db=({}, {}) file={file_sum}",
                gap.gap_counter, gap.gap_reported
            )));
        }
        let seq_start = next_seq_start(&critical_history, &normal_history);
        let anchor_parts =
            anchor::spawn(&cfg.dir, &cfg.anchor, &critical_history, Arc::clone(&clock))?;
        let (anchor, anchor_join) = match anchor_parts {
            Some(parts) => (Some(parts.handle), Some(parts.join)),
            None => (None, None),
        };
        let critical_file = open_log(&critical_path)?;
        let normal_file = open_log(&normal_path)?;

        let (critical_tx, critical_rx) = sync_channel(cfg.critical_capacity);
        let (normal_tx, normal_rx) = sync_channel(cfg.normal_capacity);
        let sinks = TraceSinks {
            critical: Arc::new(Mutex::new(Vec::new())),
            normal: Arc::new(Mutex::new(Vec::new())),
        };
        let inner = Arc::new(Inner {
            cfg,
            seq: AtomicU64::new(seq_start),
            critical_tx: Mutex::new(Some(critical_tx)),
            normal_tx: Mutex::new(Some(normal_tx)),
            sinks,
            gap_counter: AtomicU64::new(gap.gap_counter),
            gap_reported: AtomicU64::new(gap.gap_reported),
            pending_gap: Mutex::new(None),
            persist_error: Mutex::new(None),
            unpersisted: Mutex::new(Vec::new()),
            clock,
            store: Mutex::new(store),
            fail_critical_after: AtomicU64::new(NO_INJECTED_FAIL),
            fail_normal_after: AtomicU64::new(NO_INJECTED_FAIL),
            otel_mode: prepared.mode,
            otel_counted: Arc::new(AtomicU64::new(0)),
            otel_offline_reason: Mutex::new(prepared.reason),
            otel_tx: Mutex::new(prepared.tx),
            #[cfg(all(test, feature = "otel"))]
            otel_queued: AtomicU64::new(0),
            anchor,
        });
        let otel_handle = prepared.spawn.take().map(|spawn| {
            let counted = Arc::clone(&inner.otel_counted);
            let hook_inner = Arc::downgrade(&inner);
            let on_fail: otel::FailHook = Arc::new(move |seq| {
                if let Some(inner) = hook_inner.upgrade() {
                    inner.record_gap(seq);
                }
            });
            spawn(counted, on_fail)
        });
        let critical_handle = spawn_consumer(
            Lane::Critical,
            critical_rx,
            inner.sinks.critical.clone(),
            inner.cfg.critical_consume_delay_us,
            critical_file,
            Arc::clone(&inner),
        );
        let normal_handle = spawn_consumer(
            Lane::Normal,
            normal_rx,
            inner.sinks.normal.clone(),
            inner.cfg.normal_consume_delay_us,
            normal_file,
            Arc::clone(&inner),
        );
        Ok((
            Arc::new(Trace {
                inner,
                critical_handle: Mutex::new(Some(critical_handle)),
                normal_handle: Mutex::new(Some(normal_handle)),
                otel_handle: Mutex::new(otel_handle),
                anchor_join: Mutex::new(anchor_join),
            }),
            outcome,
        ))
    }

    pub fn config(&self) -> &TraceConfig {
        &self.inner.cfg
    }

    /// 挡住两个落盘线程，直到 guard drop。
    ///
    /// 消费者在把事件写入文件和镜像之前要拿对应的锁。持有期间有界队列不会被排空，
    /// 基准可以据此把 normal 队列稳定打满，从而测到丢弃/背压路径。
    /// 不改变 [`Trace::write_critical`] / [`Trace::write_normal`] 的语义。
    /// 锁顺序固定为 critical → normal。
    pub fn hold_consumers(&self) -> ConsumerHold<'_> {
        ConsumerHold {
            _critical: self.inner.sinks.critical.lock().unwrap(),
            _normal: self.inner.sinks.normal.lock().unwrap(),
        }
    }

    /// 当前丢弃计数（含本次打开时从 `trace.db` 恢复的基数）。
    ///
    /// OTel lane 离线时只含磁盘丢弃。lane 在线时还含导出失败和该 lane 队列满。
    pub fn gap_counter(&self) -> u64 {
        self.inner.gap_counter.load(Ordering::Relaxed)
    }

    pub fn otel_mode(&self) -> OtelMode {
        self.inner.otel_mode
    }

    /// 离线：只计数的 normal 条数。在线：导出成功的条数。
    pub fn otel_counted(&self) -> u64 {
        self.inner.otel_counted.load(Ordering::Relaxed)
    }

    /// 离线原因。在线时为 `None`。
    pub fn otel_offline_reason(&self) -> Option<String> {
        self.inner.otel_offline_reason.lock().unwrap().clone()
    }

    /// 已落盘但还没有锚定成功的 critical 事件数。锚定关闭时为 0。
    /// 不含还在等 N 或 T 的开放批次。
    pub fn anchor_gap(&self) -> u64 {
        self.inner
            .anchor
            .as_ref()
            .map(|anchor| anchor.gap())
            .unwrap_or(0)
    }

    /// rekor 不可达或锚定队列非空。锚定关闭时为 false。
    pub fn anchor_alert(&self) -> bool {
        self.inner
            .anchor
            .as_ref()
            .is_some_and(|anchor| anchor.alert())
    }

    /// 告警原因。没有告警时为 `None`。
    pub fn anchor_alert_reason(&self) -> Option<String> {
        self.inner
            .anchor
            .as_ref()
            .and_then(|anchor| anchor.reason())
    }

    /// 锚线程已经处理完的 critical 条数（含提交失败后入队的）。
    pub fn anchor_seen(&self) -> u64 {
        self.inner
            .anchor
            .as_ref()
            .map(|anchor| anchor.seen())
            .unwrap_or(0)
    }

    /// 按注入的钟检查未封口批次，到龄则提交。
    ///
    /// 会等到这次检查做完（含一次 rekor 往返）。不要在 critical 写路径上调用。
    /// 锚线程自身不睡眠；没有新事件时，时间窗要靠这次调用或 shutdown 才会被看见。
    pub fn poll_anchor(&self) {
        let Some(anchor) = &self.inner.anchor else {
            return;
        };
        let Some(token) = anchor.poll() else {
            return;
        };
        loop {
            if anchor.poll_done() >= token {
                return;
            }
            let finished = self
                .anchor_join
                .lock()
                .unwrap()
                .as_ref()
                .is_none_or(|handle| handle.is_finished());
            if finished {
                return;
            }
            std::thread::yield_now();
        }
    }

    /// **critical：移交同步，落盘线程同步写盘后再确认。fail-closed。**
    ///
    /// 有界缓冲满时立即返回 `Err(TraceError::CriticalFull)`，事件原样退还。
    /// `Ok(seq)` 表示事件已进入缓冲；落盘线程会在 [`Trace::shutdown`] 返回 `Ok` 之前
    /// 把它追加进 `critical.jsonl` 并 `sync_data`。写盘失败时事件留在
    /// [`TraceError::PersistFailed`] 里，不进文件。
    /// 落盘一旦失败，后续 `write_critical` 直接退还新事件，不再入队。
    #[allow(clippy::result_large_err)]
    pub fn write_critical(
        &self,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<u64, TraceError> {
        let ev = self.inner.build(Level::Critical, kind, payload, None);
        let seq = ev.seq;
        if let Some(detail) = self.inner.persist_detail() {
            return Err(TraceError::PersistFailed {
                detail,
                events: Box::new(vec![ev]),
            });
        }
        let guard = self.inner.critical_tx.lock().unwrap();
        match guard.as_ref() {
            None => Err(TraceError::Shutdown(Box::new(ev))),
            Some(tx) => match tx.try_send(ev) {
                Ok(()) => Ok(seq),
                Err(TrySendError::Full(event)) => Err(TraceError::CriticalFull(Box::new(event))),
                Err(TrySendError::Disconnected(event)) => {
                    Err(TraceError::Shutdown(Box::new(event)))
                }
            },
        }
    }

    /// **normal：异步写，有损。**
    ///
    /// 有界队列满，或落盘线程写 `normal.jsonl` 失败时，立即丢弃该事件、`gap_counter += 1`，
    /// 并尝试发出一条 `kind="gap"` 的 critical span。critical 也满时合并进 pending，
    /// shutdown 时强制补发。绝不阻塞、绝不 panic。
    pub fn write_normal(&self, kind: &str, payload: serde_json::Value) {
        self.inner.write_normal(kind, payload);
    }

    /// 关闭流程（必须在所有生产者结束后调用）：
    /// 1) 关 normal 通道，等 normal 消费线程清空队列退出；
    /// 2) 关 OTel lane，等已接受的 span 导出完（失败只记缺口）；
    /// 3) 强制补发 pending 的合并 gap span；
    /// 4) 关 critical 通道，等 critical 落盘线程清空退出；
    /// 5) 停锚线程，未封口的批次提交一次；rekor 不可达则入队，不把本次关闭变成落盘失败；
    /// 6) 把缺口计数写成与 critical 文件一致的值。
    ///
    /// 干净关闭时 `Ok` 的报告满足 `gap_counter == gap_reported == Σ(文件内 gap.count)`。
    /// critical 写盘失败时返回 [`TraceError::PersistFailed`]，事件在错误里。
    #[allow(clippy::result_large_err)]
    pub fn shutdown(&self) -> Result<ShutdownReport, TraceError> {
        if let Some(tx) = self.inner.normal_tx.lock().unwrap().take() {
            drop(tx);
        }
        let normal_join = join_consumer(&self.normal_handle, "normal");
        if let Some(tx) = self.inner.otel_tx.lock().unwrap().take() {
            drop(tx);
        }
        let otel_join = join_consumer(&self.otel_handle, "otel");
        self.inner.flush_pending_gap();
        if let Some(tx) = self.inner.critical_tx.lock().unwrap().take() {
            drop(tx);
        }
        let critical_result = join_consumer(&self.critical_handle, "critical");
        if let Some(anchor) = &self.inner.anchor {
            anchor.disconnect();
        }
        let anchor_result = join_consumer(&self.anchor_join, "anchor");
        normal_join?;
        otel_join?;
        critical_result?;
        anchor_result?;

        let critical_persisted = self.inner.sinks.critical.lock().unwrap().len();
        let normal_persisted = self.inner.sinks.normal.lock().unwrap().len();
        let report = ShutdownReport {
            gap_counter: self.inner.gap_counter.load(Ordering::Relaxed),
            gap_reported: self.inner.gap_reported.load(Ordering::Relaxed),
            critical_persisted,
            normal_persisted,
            sinks: self.inner.sinks.clone(),
            otel_mode: self.inner.otel_mode,
            otel_counted: self.inner.otel_counted.load(Ordering::Relaxed),
            anchor_gap: self.anchor_gap(),
            anchor_alert: self.anchor_alert(),
            anchor_alert_reason: self.anchor_alert_reason(),
        };
        self.checkpoint(&report)
    }

    fn checkpoint(&self, report: &ShutdownReport) -> Result<ShutdownReport, TraceError> {
        let durable = sum_gap_counts(&read_trace_log(&critical_log_path(&self.inner.cfg.dir))?);
        let failed = self.inner.persist_detail();
        let unpersisted = self.inner.take_unpersisted();
        if failed.is_none() && unpersisted.is_empty() {
            if report.gap_counter != durable || report.gap_reported != durable {
                return Err(TraceError::Inconsistent(format!(
                    "shutdown 对账失败: gap_counter={} gap_reported={} file={durable}",
                    report.gap_counter, report.gap_reported
                )));
            }
            self.inner
                .store
                .lock()
                .unwrap()
                .set_gap_state(report.gap_counter, report.gap_reported)?;
            return Ok(report.clone());
        }
        self.inner
            .store
            .lock()
            .unwrap()
            .set_gap_state(durable, durable)?;
        Err(TraceError::PersistFailed {
            detail: failed.unwrap_or_else(|| "critical 落盘失败".into()),
            events: Box::new(unpersisted),
        })
    }

    /// 让下一次起的第 `n` 条 critical 写盘失败（`n == 0` 表示第一条就失败）。
    /// 只给测试用，生产打开路径不会调用。
    #[cfg(test)]
    fn inject_critical_fail_after(&self, n: u64) {
        self.inner.fail_critical_after.store(n, Ordering::Relaxed);
    }

    /// 让 normal 落盘线程从第 `n` 条成功写盘之后开始失败，失败按缺口计数。
    #[cfg(test)]
    fn inject_normal_fail_after(&self, n: u64) {
        self.inner.fail_normal_after.store(n, Ordering::Relaxed);
    }
}

impl Inner {
    fn write_normal(&self, kind: &str, payload: serde_json::Value) {
        let ev = self.build(Level::Normal, kind, payload, None);
        let seq = ev.seq;
        let sent = {
            let guard = self.normal_tx.lock().unwrap();
            match guard.as_ref() {
                Some(tx) => tx.try_send(ev).is_ok(),
                None => false,
            }
        };
        if !sent {
            self.record_gap(seq);
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
            ts_ms: self.clock.wall_ms(),
            gap,
        }
    }

    fn persist_detail(&self) -> Option<String> {
        self.persist_error.lock().unwrap().clone()
    }

    fn persist_failed(&self) -> bool {
        self.persist_error.lock().unwrap().is_some()
    }

    fn fail_persist(&self, detail: String, event: TraceEvent) {
        let mut slot = self.persist_error.lock().unwrap();
        if slot.is_none() {
            *slot = Some(detail);
        }
        drop(slot);
        self.unpersisted.lock().unwrap().push(event);
    }

    fn push_unpersisted(&self, event: TraceEvent) {
        self.unpersisted.lock().unwrap().push(event);
    }

    fn take_unpersisted(&self) -> Vec<TraceEvent> {
        std::mem::take(&mut *self.unpersisted.lock().unwrap())
    }

    /// 已落盘的 critical 事件交给锚线程。通道是无界的，不在落盘线程里做 HTTP。
    fn submit_critical_for_anchor(&self, event: TraceEvent) {
        if let Some(anchor) = &self.anchor {
            anchor.submit(event);
        }
    }

    /// 落盘成功的 normal 事件交给非关键 lane。离线只计数；在线 `try_send`，满了记缺口。
    fn submit_normal_persisted(&self, event: TraceEvent) {
        if event.level != Level::Normal {
            return;
        }
        if self.otel_mode == OtelMode::Offline {
            self.otel_counted.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let guard = self.otel_tx.lock().unwrap();
        match guard.as_ref() {
            Some(tx) => match tx.try_send(event) {
                Ok(()) => {
                    #[cfg(all(test, feature = "otel"))]
                    self.otel_queued.fetch_add(1, Ordering::Relaxed);
                }
                Err(TrySendError::Full(event) | TrySendError::Disconnected(event)) => {
                    drop(guard);
                    self.record_gap(event.seq);
                }
            },
            None => {
                drop(guard);
                self.record_gap(event.seq);
            }
        }
    }

    fn injected_fail(&self, lane: Lane, written: u64) -> bool {
        let limit = match lane {
            Lane::Critical => self.fail_critical_after.load(Ordering::Relaxed),
            Lane::Normal => self.fail_normal_after.load(Ordering::Relaxed),
        };
        limit != NO_INJECTED_FAIL && written >= limit
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
                None => break,
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
                            break;
                        }
                    }
                },
            }
        }
    }
}

pub fn critical_log_path(dir: &Path) -> PathBuf {
    dir.join("critical.jsonl")
}

pub fn normal_log_path(dir: &Path) -> PathBuf {
    dir.join("normal.jsonl")
}

/// 读一份 JSONL。文件不存在视为空日志。坏行或 IO 错误失败返回，不跳过。
#[allow(clippy::result_large_err)]
pub fn read_trace_log(path: &Path) -> Result<Vec<TraceEvent>, TraceError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)
        .map_err(|err| TraceError::Open(format!("读取 {}: {err}", path.display())))?;
    let mut events = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<TraceEvent>(line) {
            Ok(event) => events.push(event),
            Err(err) => {
                return Err(TraceError::Inconsistent(format!(
                    "{} 第 {} 行不是合法 TraceEvent: {err}",
                    path.display(),
                    idx + 1
                )));
            }
        }
    }
    Ok(events)
}

fn reject_orphan_logs(dir: &Path) -> Result<(), TraceError> {
    if store_path(dir).exists() {
        return Ok(());
    }
    if critical_log_path(dir).exists() || normal_log_path(dir).exists() {
        return Err(TraceError::Inconsistent(
            "目录里已有 jsonl 但没有 trace.db，拒绝当成首启".into(),
        ));
    }
    Ok(())
}

fn next_seq_start(critical: &[TraceEvent], normal: &[TraceEvent]) -> u64 {
    critical
        .iter()
        .chain(normal.iter())
        .map(|event| event.seq)
        .max()
        .map(|seq| seq.saturating_add(1))
        .unwrap_or(0)
}

fn open_log(path: &Path) -> Result<File, TraceError> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| TraceError::Open(format!("打开 {}: {err}", path.display())))
}

fn append_line(file: &mut File, line: &str, sync: bool) -> Result<(), String> {
    let mut bytes = Vec::with_capacity(line.len() + 1);
    bytes.extend_from_slice(line.as_bytes());
    bytes.push(b'\n');
    file.write_all(&bytes).map_err(|err| err.to_string())?;
    file.flush().map_err(|err| err.to_string())?;
    if sync {
        file.sync_data().map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn join_consumer(slot: &Mutex<Option<JoinHandle<()>>>, name: &str) -> Result<(), TraceError> {
    if let Some(handle) = slot.lock().unwrap().take() {
        handle
            .join()
            .map_err(|_| TraceError::Open(format!("{name} 落盘线程异常退出")))?;
    }
    Ok(())
}

/// 落盘线程。critical：写 JSONL 并 `sync_data`，成功后才推进镜像。
/// normal：追加 JSONL（不逐条 fsync）；写失败按缺口计数，不让生产者看见错误。
///
/// 先模拟耗时再 recv，保证“突发写入期间消费者一条都处理不完”。
/// 拿到事件后、写文件前获取镜像锁，因此 [`Trace::hold_consumers`] 仍能把队列钉满。
fn spawn_consumer(
    lane: Lane,
    rx: Receiver<TraceEvent>,
    sink: Arc<Mutex<Vec<String>>>,
    delay_us: u64,
    mut file: File,
    inner: Arc<Inner>,
) -> JoinHandle<()> {
    let delay = (delay_us > 0).then(|| Duration::from_micros(delay_us));
    std::thread::Builder::new()
        .name(format!("af-trace-{}", lane.name()))
        .spawn(move || {
            let mut written: u64 = 0;
            loop {
                if let Some(pause) = delay {
                    std::thread::sleep(pause);
                }
                match rx.recv() {
                    Ok(event) => {
                        let line = serde_json::to_string(&event).expect("trace event serialize");
                        let mut guard = sink.lock().unwrap();
                        if matches!(lane, Lane::Critical) && inner.persist_failed() {
                            drop(guard);
                            inner.push_unpersisted(event);
                            continue;
                        }
                        if inner.injected_fail(lane, written) {
                            drop(guard);
                            match lane {
                                Lane::Critical => {
                                    inner.fail_persist(INJECTED_CRITICAL_FAIL.to_string(), event);
                                }
                                Lane::Normal => inner.record_gap(event.seq),
                            }
                            continue;
                        }
                        match append_line(&mut file, &line, matches!(lane, Lane::Critical)) {
                            Ok(()) => {
                                written = written.saturating_add(1);
                                match lane {
                                    Lane::Critical => inner.submit_critical_for_anchor(event),
                                    Lane::Normal => inner.submit_normal_persisted(event),
                                }
                                guard.push(line);
                            }
                            Err(detail) => {
                                drop(guard);
                                match lane {
                                    Lane::Critical => inner.fail_persist(detail, event),
                                    Lane::Normal => inner.record_gap(event.seq),
                                }
                            }
                        }
                    }
                    Err(_) => break,
                }
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
    use crate::clock::ManualClock;
    use crate::error::TraceError;
    use crate::event::Level;
    use crate::otel::OtelMode;
    use crate::store::store_path;
    use rusqlite::Connection;
    use std::collections::HashSet;
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("af-trace-{label}-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn cfg_in(dir: &Path) -> TraceConfig {
        TraceConfig {
            critical_capacity: 64,
            normal_capacity: 64,
            critical_consume_delay_us: 0,
            normal_consume_delay_us: 0,
            dir: dir.to_path_buf(),
            otel_endpoint: None,
            otel_batch_size: 512,
            anchor: AnchorSettings::default(),
        }
    }

    fn cfg_with(dir: &Path, endpoint: Option<&str>, batch: usize) -> TraceConfig {
        TraceConfig {
            otel_endpoint: endpoint.map(str::to_string),
            otel_batch_size: batch,
            ..cfg_in(dir)
        }
    }

    fn drain_lines(sink: &Mutex<Vec<String>>) -> Vec<TraceEvent> {
        sink.lock()
            .unwrap()
            .iter()
            .map(|line| parse_sink_line(line))
            .collect()
    }

    fn nonempty_lines(path: &Path) -> usize {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .filter(|line| !line.is_empty())
            .count()
    }

    /// critical 缓冲满 → Err(fail-closed)，事件原样退还；腾出空间后重试成功且零丢失。
    #[test]
    fn critical_full_returns_err_and_event_is_preserved() {
        let dir = TempDir::new("critical-full");
        let t = Trace::spawn(TraceConfig {
            critical_capacity: 4,
            normal_capacity: 16,
            critical_consume_delay_us: 500_000,
            normal_consume_delay_us: 0,
            dir: dir.path().to_path_buf(),
            otel_endpoint: None,
            otel_batch_size: 512,
            anchor: AnchorSettings::default(),
        })
        .unwrap();
        let mut errs = Vec::new();
        let mut oks = Vec::new();
        for i in 0..6 {
            match t.write_critical("audit", json!({ "i": i })) {
                Ok(seq) => oks.push(seq),
                Err(err) => errs.push(err),
            }
        }
        assert_eq!(oks.len(), 4, "容量 4，前 4 条应成功");
        assert_eq!(errs.len(), 2, "后 2 条应 fail-closed 返回 Err");
        for (idx, err) in errs.iter().enumerate() {
            let ev = err.event().expect("fail-closed 退还事件");
            assert_eq!(ev.level, Level::Critical);
            assert_eq!(ev.kind, "audit");
            assert_eq!(
                ev.payload,
                json!({ "i": 4 + idx }),
                "Err 必须原样退还原事件"
            );
        }
        let rejected = errs[0].event().expect("退还事件").clone();
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
        let report = t.shutdown().unwrap();
        let lines = drain_lines(&report.sinks.critical);
        let seqs: HashSet<u64> = lines.iter().map(|event| event.seq).collect();
        assert_eq!(lines.len(), 5);
        assert!(seqs.contains(&seq), "重试成功的事件必须落盘");
        assert!(
            !seqs.contains(&rejected.seq),
            "被拒事件在重试成功前不应出现"
        );
        let file = read_trace_log(&critical_log_path(dir.path())).unwrap();
        assert_eq!(file.len(), lines.len());
        assert!(file.iter().any(|event| event.seq == seq));
    }

    /// normal 队列满 → 丢弃并计数；gap_counter == 发送数 - 实际落盘数。
    #[test]
    fn normal_overflow_drops_and_counts_gap() {
        let dir = TempDir::new("normal-overflow");
        let t = Trace::spawn(TraceConfig {
            critical_capacity: 64,
            normal_capacity: 8,
            critical_consume_delay_us: 0,
            normal_consume_delay_us: 200_000,
            dir: dir.path().to_path_buf(),
            otel_endpoint: None,
            otel_batch_size: 512,
            anchor: AnchorSettings::default(),
        })
        .unwrap();
        for i in 0..20 {
            t.write_normal("step", json!({ "i": i }));
        }
        assert_eq!(t.gap_counter(), 12, "队列容量 8，20 条应丢弃 12 条");
        let report = t.shutdown().unwrap();
        assert_eq!(report.gap_counter, 12);
        assert_eq!(report.normal_persisted, 8);
        assert_eq!(report.gap_reported, 12, "所有缺口最终都应被报告");
        let crit = drain_lines(&report.sinks.critical);
        assert_eq!(
            sum_gap_counts(&crit),
            12,
            "Σ(gap span.count) == gap_counter"
        );
        assert!(
            crit.iter().any(|event| event.kind == "gap"),
            "丢弃必须产生 critical gap span"
        );
        let file = read_trace_log(&critical_log_path(dir.path())).unwrap();
        assert_eq!(sum_gap_counts(&file), 12);
        assert_eq!(
            nonempty_lines(&normal_log_path(dir.path())),
            report.normal_persisted
        );
    }

    /// critical 也满时 gap span 合并（count>1），但计数不丢；shutdown 强制补发尾差。
    #[test]
    fn gap_spans_coalesce_when_critical_is_also_full() {
        let dir = TempDir::new("coalesce");
        let t = Trace::spawn(TraceConfig {
            critical_capacity: 2,
            normal_capacity: 8,
            critical_consume_delay_us: 500_000,
            normal_consume_delay_us: 200_000,
            dir: dir.path().to_path_buf(),
            otel_endpoint: None,
            otel_batch_size: 512,
            anchor: AnchorSettings::default(),
        })
        .unwrap();
        for i in 0..30 {
            t.write_normal("step", json!({ "i": i }));
        }
        assert_eq!(t.gap_counter(), 22);
        let report = t.shutdown().unwrap();
        assert_eq!(report.gap_counter, 22);
        assert_eq!(
            report.gap_reported, 22,
            "shutdown 补发后 reported == counter"
        );
        assert_eq!(report.normal_persisted, 8);
        let crit = drain_lines(&report.sinks.critical);
        let gap_spans: Vec<&GapInfo> = crit.iter().filter_map(|event| event.gap.as_ref()).collect();
        assert!(
            gap_spans.len() < 22,
            "critical 也满时应发生合并（实际 gap span 数 {}）",
            gap_spans.len()
        );
        let gap_sum: u64 = gap_spans.iter().map(|gap| gap.count).sum();
        assert_eq!(gap_sum, 22, "合并不丢计数");
        assert!(
            gap_spans.iter().any(|gap| gap.count > 1),
            "至少一条合并 span（count>1）"
        );
    }

    /// 容量充足时零丢弃零丢失；shutdown 后再写 critical 返回 Err(Shutdown)。
    #[test]
    fn zero_loss_when_under_capacity_and_closed_channel_errors() {
        let dir = TempDir::new("zero-loss");
        let cfg = TraceConfig {
            dir: dir.path().to_path_buf(),
            otel_endpoint: None,
            otel_batch_size: 512,
            ..TraceConfig::default()
        };
        let t = Trace::spawn(cfg).unwrap();
        let mut crit_seqs = Vec::new();
        for i in 0..50 {
            crit_seqs.push(t.write_critical("audit", json!({ "i": i })).unwrap());
        }
        for i in 0..500 {
            t.write_normal("step", json!({ "i": i }));
        }
        assert_eq!(t.gap_counter(), 0);
        let report = t.shutdown().unwrap();
        assert_eq!(report.gap_counter, 0);
        assert_eq!(report.normal_persisted, 500);
        let crit = drain_lines(&report.sinks.critical);
        let seqs: HashSet<u64> = crit.iter().map(|event| event.seq).collect();
        assert_eq!(crit.len(), 50);
        assert!(
            crit_seqs.iter().all(|seq| seqs.contains(seq)),
            "critical 零丢失"
        );
        assert!(crit.iter().all(|event| event.level == Level::Critical));

        let err = t.write_critical("after", json!(null)).unwrap_err();
        assert!(matches!(err, TraceError::Shutdown(_)));
        t.write_normal("after", json!(null));
        assert_eq!(t.gap_counter(), 1);
    }

    /// 写完 drop 再打开：critical 文件、缺口计数、seq 与注入时刻都还在。
    #[test]
    fn restart_restores_critical_log_gap_counter_seq_and_clock() {
        let dir = TempDir::new("restart");
        let cfg = TraceConfig {
            critical_capacity: 64,
            normal_capacity: 4,
            critical_consume_delay_us: 0,
            normal_consume_delay_us: 0,
            dir: dir.path().to_path_buf(),
            otel_endpoint: None,
            otel_batch_size: 512,
            anchor: AnchorSettings::default(),
        };
        let clock = ManualClock::new(1_700_000_000_042);
        let (trace, outcome) = Trace::open_with_clock(cfg.clone(), clock.clone()).unwrap();
        assert_eq!(outcome, OpenOutcome::Created);
        let _hold = trace.hold_consumers();
        let mut audits = Vec::new();
        for i in 0..3 {
            audits.push(trace.write_critical("audit", json!({ "i": i })).unwrap());
        }
        let sent = 32u64;
        for i in 0..sent {
            trace.write_normal("step", json!({ "i": i }));
        }
        let dropped = trace.gap_counter();
        assert!(
            dropped + 2 >= sent - 4,
            "消费者被挡住时，超出容量的 normal 事件应记成缺口，实际 {dropped}"
        );
        drop(_hold);
        let report = trace.shutdown().unwrap();
        assert_eq!(
            report.normal_persisted as u64 + report.gap_counter,
            sent,
            "落盘条数 + 缺口计数 = 发送条数"
        );
        assert_eq!(report.gap_reported, report.gap_counter);
        let crit_path = critical_log_path(dir.path());
        let norm_path = normal_log_path(dir.path());
        let crit = read_trace_log(&crit_path).unwrap();
        let norm = read_trace_log(&norm_path).unwrap();
        assert!(crit
            .iter()
            .chain(norm.iter())
            .all(|event| event.ts_ms == 1_700_000_000_042));
        assert_eq!(sum_gap_counts(&crit), report.gap_counter);
        assert!(audits
            .iter()
            .all(|seq| crit.iter().any(|event| event.seq == *seq)));
        let gap_lines = crit.iter().filter(|event| event.kind == "gap").count();
        let event_count = crit.iter().filter(|event| event.kind != "gap").count() + norm.len();
        assert_eq!(
            nonempty_lines(&crit_path) + nonempty_lines(&norm_path),
            event_count + gap_lines,
            "落盘文件行数 = 事件数 + 缺口数"
        );
        let max_seq = crit
            .iter()
            .chain(norm.iter())
            .map(|event| event.seq)
            .max()
            .unwrap();
        drop(trace);

        let (again, outcome) = Trace::open_with_clock(cfg.clone(), clock).unwrap();
        assert_eq!(outcome, OpenOutcome::AlreadyDeclared);
        assert_eq!(again.gap_counter(), report.gap_counter);
        assert_eq!(read_trace_log(&crit_path).unwrap(), crit);
        let next = again.write_critical("audit", json!({ "i": 99 })).unwrap();
        assert_eq!(next, max_seq + 1, "seq 从文件里的最大值之后继续");
        let report2 = again.shutdown().unwrap();
        assert_eq!(report2.gap_counter, report.gap_counter);
        let crit_after = read_trace_log(&crit_path).unwrap();
        assert_eq!(crit_after.len(), crit.len() + 1);
        assert_eq!(crit_after.last().unwrap().seq, next);
        assert_eq!(crit_after.last().unwrap().ts_ms, 1_700_000_000_042);
    }

    #[test]
    fn critical_persist_failure_is_fail_closed() {
        let dir = TempDir::new("crit-fail");
        let trace = Trace::spawn(cfg_in(dir.path())).unwrap();
        trace.inject_critical_fail_after(0);
        let seq = trace.write_critical("audit", json!({ "k": 1 })).unwrap();
        let err = trace.shutdown().unwrap_err();
        match err {
            TraceError::PersistFailed { detail, events } => {
                assert_eq!(detail, INJECTED_CRITICAL_FAIL);
                assert_eq!(events.len(), 1);
                assert_eq!(events[0].seq, seq);
                assert_eq!(events[0].payload, json!({ "k": 1 }));
            }
            other => panic!("期望 PersistFailed，实际 {other:?}"),
        }
        assert!(read_trace_log(&critical_log_path(dir.path()))
            .unwrap()
            .is_empty());
        let err = trace
            .write_critical("audit", json!({ "k": 2 }))
            .unwrap_err();
        let returned = err.event().expect("后续写入退还事件");
        assert_eq!(returned.payload, json!({ "k": 2 }));
        assert!(matches!(err, TraceError::PersistFailed { .. }));
    }

    #[test]
    fn normal_persist_failure_counts_gap() {
        let dir = TempDir::new("norm-fail");
        let trace = Trace::spawn(TraceConfig {
            critical_capacity: 32,
            normal_capacity: 16,
            critical_consume_delay_us: 0,
            normal_consume_delay_us: 0,
            dir: dir.path().to_path_buf(),
            otel_endpoint: None,
            otel_batch_size: 512,
            anchor: AnchorSettings::default(),
        })
        .unwrap();
        trace.inject_normal_fail_after(0);
        let sent = 5;
        for i in 0..sent {
            trace.write_normal("step", json!({ "i": i }));
        }
        let report = trace.shutdown().unwrap();
        assert_eq!(report.normal_persisted, 0);
        assert_eq!(report.gap_counter, sent);
        assert_eq!(report.gap_reported, sent);
        assert!(read_trace_log(&normal_log_path(dir.path()))
            .unwrap()
            .is_empty());
        let crit = read_trace_log(&critical_log_path(dir.path())).unwrap();
        assert_eq!(sum_gap_counts(&crit), sent);
    }

    #[test]
    fn declaration_gap_and_corrupt_log_fail_closed() {
        let dir = TempDir::new("mismatch");
        let cfg = cfg_in(dir.path());
        let (trace, outcome) = Trace::open(cfg.clone()).unwrap();
        assert_eq!(outcome, OpenOutcome::Created);
        trace.write_critical("audit", json!(1)).unwrap();
        trace.shutdown().unwrap();
        drop(trace);

        let conn = Connection::open(store_path(dir.path())).unwrap();
        conn.execute(
            "UPDATE declaration SET schema_version = 99 WHERE id = 1",
            [],
        )
        .unwrap();
        drop(conn);
        let err = match Trace::open(cfg.clone()) {
            Ok(_) => panic!("schema 不符时必须拒绝打开"),
            Err(err) => err,
        };
        assert!(matches!(err, TraceError::DeclarationMismatch(_)));

        let conn = Connection::open(store_path(dir.path())).unwrap();
        conn.execute("UPDATE declaration SET schema_version = 1 WHERE id = 1", [])
            .unwrap();
        conn.execute("UPDATE gap_state SET gap_counter = 9 WHERE id = 1", [])
            .unwrap();
        drop(conn);
        let err = match Trace::open(cfg.clone()) {
            Ok(_) => panic!("缺口计数与文件对不上必须失败"),
            Err(err) => err,
        };
        assert!(
            matches!(err, TraceError::Inconsistent(_)),
            "缺口计数与文件对不上必须失败，实际 {err:?}"
        );

        let conn = Connection::open(store_path(dir.path())).unwrap();
        conn.execute("UPDATE gap_state SET gap_counter = 0 WHERE id = 1", [])
            .unwrap();
        drop(conn);
        let mut file = OpenOptions::new()
            .append(true)
            .open(critical_log_path(dir.path()))
            .unwrap();
        writeln!(file, "not-json").unwrap();
        drop(file);
        let err = match Trace::open(cfg) {
            Ok(_) => panic!("坏 JSONL 必须拒绝打开"),
            Err(err) => err,
        };
        assert!(matches!(err, TraceError::Inconsistent(_)));
    }

    #[test]
    fn orphan_jsonl_and_bad_dir_fail_closed() {
        let err = match Trace::spawn(TraceConfig::default()) {
            Ok(_) => panic!("空目录必须拒绝打开"),
            Err(err) => err,
        };
        assert!(matches!(err, TraceError::Open(_)));

        let dir = TempDir::new("orphan");
        std::fs::write(critical_log_path(dir.path()), "{}\n").unwrap();
        let err = match Trace::open(cfg_in(dir.path())) {
            Ok(_) => panic!("只有 jsonl 时必须拒绝打开"),
            Err(err) => err,
        };
        assert!(matches!(err, TraceError::Inconsistent(_)));
        assert!(
            !store_path(dir.path()).exists(),
            "拒绝首启时不应先把 trace.db 建出来"
        );

        let file_dir = TempDir::new("notdir");
        let blocking = file_dir.path().join("blocked");
        std::fs::write(&blocking, b"x").unwrap();
        let err = match Trace::open(cfg_in(&blocking)) {
            Ok(_) => panic!("目录路径是文件时必须拒绝打开"),
            Err(err) => err,
        };
        assert!(matches!(err, TraceError::Open(_)));
    }

    #[test]
    fn append_line_on_dev_full_returns_error() {
        let mut file = OpenOptions::new().write(true).open("/dev/full").unwrap();
        let err = append_line(&mut file, "{}", true).unwrap_err();
        assert!(!err.is_empty());
    }

    /// 端点空白：lane 离线，只计落盘成功的 normal，critical 不计数，缺口不变。
    #[test]
    fn offline_lane_counts_persisted_normals_and_skips_critical() {
        let dir = TempDir::new("otel-off");
        let clock = ManualClock::new(50);
        let (trace, outcome) =
            Trace::open_with_clock(cfg_with(dir.path(), Some("  "), 4), clock).unwrap();
        assert_eq!(outcome, OpenOutcome::Created);
        assert_eq!(trace.otel_mode(), OtelMode::Offline);
        let reason = trace.otel_offline_reason().expect("离线要有原因");
        assert!(!reason.is_empty(), "{reason}");
        let audit = trace.write_critical("audit", json!({ "k": 1 })).unwrap();
        for i in 0..3 {
            trace.write_normal("step", json!({ "i": i }));
        }
        let report = trace.shutdown().unwrap();
        assert_eq!(report.otel_mode, OtelMode::Offline);
        assert_eq!(report.normal_persisted, 3);
        assert_eq!(report.otel_counted, 3);
        assert_eq!(report.gap_counter, 0);
        assert_eq!(report.gap_reported, 0);
        let crit = read_trace_log(&critical_log_path(dir.path())).unwrap();
        assert!(crit
            .iter()
            .any(|event| event.seq == audit && event.level == Level::Critical));
    }

    #[cfg(not(feature = "otel"))]
    #[test]
    fn configured_endpoint_without_feature_stays_offline() {
        let dir = TempDir::new("otel-nofeature");
        let clock = ManualClock::new(7);
        let (trace, _) = Trace::open_with_clock(
            cfg_with(dir.path(), Some("http://127.0.0.1:4318"), 3),
            clock,
        )
        .unwrap();
        assert_eq!(trace.otel_mode(), OtelMode::Offline);
        trace.write_normal("step", json!(1));
        trace.write_critical("audit", json!(1)).unwrap();
        let report = trace.shutdown().unwrap();
        assert_eq!(report.otel_counted, 1);
        assert_eq!(report.normal_persisted, 1);
        assert_eq!(report.gap_counter, 0);
    }

    #[cfg(feature = "otel")]
    fn spin_until(mut ready: impl FnMut() -> bool, message: &str) {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !ready() {
            assert!(std::time::Instant::now() < deadline, "{message}");
            std::thread::yield_now();
        }
    }

    #[cfg(feature = "otel")]
    fn span_attr(span: &opentelemetry_sdk::trace::SpanData, key: &str) -> String {
        span.attributes
            .iter()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| kv.value.as_str().into_owned())
            .unwrap_or_default()
    }

    #[cfg(feature = "otel")]
    #[test]
    fn normal_events_map_to_spans_and_critical_never_enters() {
        let dir = TempDir::new("otel-map");
        let clock = ManualClock::new(1_700_000_000_042);
        let mock = crate::otel::MockExporter::new();
        // 先挡住 exporter：第一条进 export 后批次已封口，再把后两条放进容量为 2 的队列。
        let hold = mock.gate.lock().unwrap();
        let (trace, _) = Trace::open_with_mock(
            cfg_with(dir.path(), Some("http://127.0.0.1:9"), 2),
            clock,
            mock.clone(),
        )
        .unwrap();
        assert_eq!(trace.otel_mode(), OtelMode::Online);
        let audit = trace
            .write_critical("audit", json!({ "keep": true }))
            .unwrap();
        trace.write_normal("step", json!({ "i": 0 }));
        spin_until(
            || mock.entered.load(Ordering::SeqCst) >= 1,
            "worker 没有进入 export",
        );
        trace.write_normal("step", json!({ "i": 1 }));
        trace.write_normal("step", json!({ "i": 2 }));
        spin_until(
            || trace.inner.otel_queued.load(Ordering::Relaxed) >= 3,
            "前三笔 normal 没有进 OTel 队列",
        );
        drop(hold);
        spin_until(
            || mock.spans.lock().unwrap().len() >= 3,
            "前三笔 normal 没有导出",
        );
        trace.write_normal("step", json!({ "i": 3 }));
        let report = trace.shutdown().unwrap();
        assert_eq!(report.gap_counter, 0);
        assert_eq!(report.gap_reported, 0);
        assert_eq!(report.normal_persisted, 4);
        assert_eq!(report.otel_counted, 4);
        let spans = mock.spans.lock().unwrap().clone();
        assert_eq!(spans.len(), 4);
        let batches = mock.batches.lock().unwrap().clone();
        assert_eq!(batches, vec![1, 2, 1], "批大小 2，实际 {batches:?}");
        let normals = read_trace_log(&normal_log_path(dir.path())).unwrap();
        assert_eq!(normals.len(), 4);
        let trace_id = spans[0].span_context.trace_id();
        for (span, event) in spans.iter().zip(normals.iter()) {
            assert_eq!(event.level, Level::Normal);
            assert_eq!(span.name.as_ref(), event.kind);
            assert_eq!(span_attr(span, "af.seq"), event.seq.to_string());
            assert_eq!(span_attr(span, "af.level"), "normal");
            assert_eq!(span_attr(span, "af.kind"), "step");
            assert_eq!(span_attr(span, "af.payload"), event.payload.to_string());
            assert_eq!(span_attr(span, "af.ts_ms"), event.ts_ms.to_string());
            assert_eq!(event.ts_ms, 1_700_000_000_042);
            assert_eq!(span.span_context.trace_id(), trace_id);
            assert!(span.span_context.is_valid());
            assert_ne!(span_attr(span, "af.seq"), audit.to_string());
        }
        let crit = read_trace_log(&critical_log_path(dir.path())).unwrap();
        assert!(crit.iter().any(|event| event.seq == audit));
        for event in &crit {
            assert!(
                spans
                    .iter()
                    .all(|span| span_attr(span, "af.seq") != event.seq.to_string()),
                "critical seq {} 进入了 OTel",
                event.seq
            );
        }
    }

    #[cfg(feature = "otel")]
    #[test]
    fn export_failure_only_increments_gap_and_keeps_disk() {
        let dir = TempDir::new("otel-fail");
        let clock = ManualClock::new(9);
        let mock = crate::otel::MockExporter::new();
        mock.fail.store(true, Ordering::SeqCst);
        let (trace, _) =
            Trace::open_with_mock(cfg_with(dir.path(), None, 8), clock, mock.clone()).unwrap();
        let audit = trace.write_critical("audit", json!({ "k": 1 })).unwrap();
        for i in 0..3 {
            trace.write_normal("step", json!({ "i": i }));
        }
        let report = trace.shutdown().unwrap();
        assert_eq!(report.normal_persisted, 3);
        assert_eq!(report.gap_counter, 3);
        assert_eq!(report.gap_reported, 3);
        assert_eq!(report.otel_counted, 0);
        assert!(mock.spans.lock().unwrap().is_empty());
        let attempted = mock.attempted.lock().unwrap().clone();
        assert_eq!(attempted, vec!["step".to_string(); 3]);
        let normals = read_trace_log(&normal_log_path(dir.path())).unwrap();
        assert_eq!(normals.len(), 3);
        let crit = read_trace_log(&critical_log_path(dir.path())).unwrap();
        assert!(crit
            .iter()
            .any(|event| event.seq == audit && event.kind == "audit"));
        assert_eq!(sum_gap_counts(&crit), 3);
        assert!(attempted.iter().all(|name| name != "audit"));
    }

    #[cfg(feature = "otel")]
    #[test]
    fn wedged_export_does_not_block_writes_and_overflow_counts_gap() {
        let dir = TempDir::new("otel-wedge");
        let clock = ManualClock::new(11);
        let mock = crate::otel::MockExporter::new();
        let _hold = mock.gate.lock().unwrap();
        let (trace, _) =
            Trace::open_with_mock(cfg_with(dir.path(), None, 1), clock, mock.clone()).unwrap();
        let audit = trace.write_critical("audit", json!(1)).unwrap();
        for i in 0..3 {
            trace.write_normal("step", json!({ "i": i }));
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while mock.entered.load(Ordering::SeqCst) == 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "worker 没有进入 export"
            );
            std::thread::yield_now();
        }
        let started = std::time::Instant::now();
        trace.write_critical("audit", json!(2)).unwrap();
        trace.write_normal("step", json!({ "i": 3 }));
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "导出被堵住时写入仍应立即返回"
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while trace.gap_counter() == 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "OTel 队列满应立刻记缺口"
            );
            std::thread::yield_now();
        }
        drop(_hold);
        let report = trace.shutdown().unwrap();
        assert_eq!(report.normal_persisted, 4);
        assert_eq!(report.gap_counter + report.otel_counted, 4);
        assert!(report.gap_counter >= 1);
        let spans = mock.spans.lock().unwrap();
        assert_eq!(spans.len() as u64, report.otel_counted);
        assert!(spans.iter().all(|span| span.name.as_ref() == "step"));
        assert!(spans
            .iter()
            .all(|span| span_attr(span, "af.seq") != audit.to_string()));
    }

    #[cfg(feature = "otel")]
    #[test]
    fn missing_collector_does_not_fail_startup() {
        let dir = TempDir::new("otel-missing");
        let clock = ManualClock::new(3);
        let (trace, outcome) =
            Trace::open_with_clock(cfg_with(dir.path(), Some("http://127.0.0.1:9"), 4), clock)
                .unwrap();
        assert_eq!(outcome, OpenOutcome::Created);
        match trace.otel_mode() {
            OtelMode::Online => assert!(trace.otel_offline_reason().is_none()),
            OtelMode::Offline => {
                let reason = trace.otel_offline_reason().expect("离线要有原因");
                assert!(!reason.is_empty(), "{reason}");
            }
        }
        let report = trace.shutdown().unwrap();
        assert_eq!(report.gap_counter, 0);
        assert_eq!(report.otel_counted, 0);
    }

    #[cfg(feature = "otel")]
    #[test]
    fn unbuildable_exporter_degrades_to_pure_count() {
        let dir = TempDir::new("otel-bad-endpoint");
        let clock = ManualClock::new(4);
        let (trace, _) =
            Trace::open_with_clock(cfg_with(dir.path(), Some("http://"), 2), clock).unwrap();
        assert_eq!(
            trace.otel_mode(),
            OtelMode::Offline,
            "reason={:?}",
            trace.otel_offline_reason()
        );
        trace.write_normal("step", json!({ "i": 1 }));
        trace.write_critical("audit", json!(1)).unwrap();
        let report = trace.shutdown().unwrap();
        assert_eq!(report.otel_counted, 1);
        assert_eq!(report.normal_persisted, 1);
        assert_eq!(report.gap_counter, 0);
    }
}
