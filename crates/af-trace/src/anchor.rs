//! critical 事件的 rekor 锚定。
//!
//! 落盘线程把已 `sync_data` 的 critical 事件交给锚线程。锚线程按 N 条或 T 毫秒封口，
//! T 只和注入的 [`Clock`] 比较，不睡眠。到点观察发生在：事件进入锚线程、[`Trace::poll_anchor`]、
//! shutdown。
//!
//! rekor 不可达时把批次写入 `anchor-queue.jsonl`，`anchor_gap` 记未锚定的事件数，
//! `anchor_alert` 置位。这不增加 `gap_counter`，也不让 `write_critical` 等待 HTTP。
//! 成功的回执追加到 `critical.anchors.jsonl`。

use crate::clock::Clock;
use crate::error::TraceError;
use crate::event::TraceEvent;
use crate::rekor::{AnchorError, AnchorReceipt, RekorClient};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// 锚定参数。`endpoint` 为空则整条锚关闭，critical 落盘与原来一样。
#[derive(Debug, Clone)]
pub struct AnchorSettings {
    /// `http://host:port`，或已经包含 `/api/v2/log/entries` 的 URL。
    pub endpoint: Option<String>,
    /// 检查点 HMAC-SHA256 密钥。验锚必须用同一把。
    pub hmac_key: Vec<u8>,
    /// 检查点 origin。空白按 `rekor-local`。
    pub origin: String,
    /// 封口条数。和 `batch_t_ms` 都是 0 时按 1 条封口。
    /// 只设置了时间窗时，0 表示不按条数封口。
    pub batch_n: u64,
    /// 封口时间窗（毫秒）。0 表示不按时间封口。读注入的钟，不睡眠。
    pub batch_t_ms: u64,
}

impl Default for AnchorSettings {
    fn default() -> Self {
        Self {
            endpoint: None,
            hmac_key: Vec::new(),
            origin: crate::rekor::DEFAULT_ORIGIN.to_string(),
            batch_n: 0,
            batch_t_ms: 0,
        }
    }
}

pub fn anchor_sidecar_path(dir: &Path) -> PathBuf {
    dir.join("critical.anchors.jsonl")
}

pub fn anchor_queue_path(dir: &Path) -> PathBuf {
    dir.join("anchor-queue.jsonl")
}

pub fn anchor_status_path(dir: &Path) -> PathBuf {
    dir.join("anchor-status.json")
}

/// 读侧车回执。文件不在视为还没有锚定。坏行拒绝，不跳过。
pub fn read_anchor_receipts(dir: &Path) -> Result<Vec<AnchorReceipt>, TraceError> {
    read_json_lines(dir, &anchor_sidecar_path(dir), "critical.anchors.jsonl")
}

enum Cmd {
    Event(TraceEvent),
    Poll(u64),
}

pub(crate) struct AnchorHandle {
    tx: Mutex<Option<Sender<Cmd>>>,
    poll_gen: Arc<AtomicU64>,
    poll_done: Arc<AtomicU64>,
    gap: Arc<AtomicU64>,
    alert: Arc<AtomicBool>,
    reason: Arc<Mutex<Option<String>>>,
    seen: Arc<AtomicU64>,
}

impl AnchorHandle {
    pub(crate) fn submit(&self, event: TraceEvent) {
        let guard = self.tx.lock().unwrap();
        if let Some(tx) = guard.as_ref() {
            if tx.send(Cmd::Event(event)).is_err() {
                self.note_orphan();
            }
        } else {
            self.note_orphan();
        }
    }

    pub(crate) fn poll(&self) -> Option<u64> {
        let token = self
            .poll_gen
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1);
        let guard = self.tx.lock().unwrap();
        let tx = guard.as_ref()?;
        tx.send(Cmd::Poll(token)).ok()?;
        Some(token)
    }

    pub(crate) fn poll_done(&self) -> u64 {
        self.poll_done.load(Ordering::SeqCst)
    }

    pub(crate) fn disconnect(&self) {
        self.tx.lock().unwrap().take();
    }

    pub(crate) fn gap(&self) -> u64 {
        self.gap.load(Ordering::SeqCst)
    }

    pub(crate) fn alert(&self) -> bool {
        self.alert.load(Ordering::SeqCst)
    }

    pub(crate) fn reason(&self) -> Option<String> {
        self.reason.lock().unwrap().clone()
    }

    pub(crate) fn seen(&self) -> u64 {
        self.seen.load(Ordering::SeqCst)
    }

    fn note_orphan(&self) {
        self.gap.fetch_add(1, Ordering::SeqCst);
        self.alert.store(true, Ordering::SeqCst);
        *self.reason.lock().unwrap() = Some("锚线程已停止，事件留在 critical 日志里".into());
    }
}

pub(crate) struct AnchorParts {
    pub handle: AnchorHandle,
    pub join: JoinHandle<()>,
}

pub(crate) fn spawn(
    dir: &Path,
    settings: &AnchorSettings,
    history: &[TraceEvent],
    clock: Arc<dyn Clock>,
) -> Result<Option<AnchorParts>, TraceError> {
    let Some(endpoint) = configured_endpoint(settings) else {
        return Ok(None);
    };
    let client = RekorClient::new(endpoint, &settings.origin, &settings.hmac_key)
        .map_err(|err| TraceError::Open(err.to_string()))?;
    let state = load_state(dir, history)?;
    let gap = Arc::new(AtomicU64::new(0));
    let alert = Arc::new(AtomicBool::new(false));
    let reason = Arc::new(Mutex::new(None));
    let poll_done = Arc::new(AtomicU64::new(0));
    let seen = Arc::new(AtomicU64::new(0));
    let handle = AnchorHandle {
        tx: Mutex::new(None),
        poll_gen: Arc::new(AtomicU64::new(0)),
        poll_done: Arc::clone(&poll_done),
        gap: Arc::clone(&gap),
        alert: Arc::clone(&alert),
        reason: Arc::clone(&reason),
        seen: Arc::clone(&seen),
    };
    let (tx, rx) = mpsc::channel();
    *handle.tx.lock().unwrap() = Some(tx);
    let runtime = Runtime {
        dir: dir.to_path_buf(),
        policy: policy_of(settings),
        client,
        clock,
        open: state.open,
        open_started_ms: state.open_started_ms,
        failed: state.failed,
        covered: state.covered,
        gap,
        alert,
        reason,
        poll_done,
        seen,
    };
    let join = std::thread::Builder::new()
        .name("af-trace-anchor".into())
        .spawn(move || run(runtime, rx))
        .map_err(|err| TraceError::Open(format!("启动锚线程失败: {err}")))?;
    Ok(Some(AnchorParts { handle, join }))
}

struct Policy {
    max_events: Option<u64>,
    max_age_ms: Option<u64>,
}

fn policy_of(settings: &AnchorSettings) -> Policy {
    match (settings.batch_n, settings.batch_t_ms) {
        (0, 0) => Policy {
            max_events: Some(1),
            max_age_ms: None,
        },
        (n, 0) => Policy {
            max_events: Some(n),
            max_age_ms: None,
        },
        (0, age) => Policy {
            max_events: None,
            max_age_ms: Some(age),
        },
        (n, age) => Policy {
            max_events: Some(n),
            max_age_ms: Some(age),
        },
    }
}

fn configured_endpoint(settings: &AnchorSettings) -> Option<&str> {
    settings
        .endpoint
        .as_deref()
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
}

struct Loaded {
    open: Vec<TraceEvent>,
    open_started_ms: Option<u64>,
    failed: Vec<Vec<TraceEvent>>,
    covered: HashSet<u64>,
}

fn load_state(dir: &Path, history: &[TraceEvent]) -> Result<Loaded, TraceError> {
    let receipts = read_anchor_receipts(dir)?;
    let covered: HashSet<u64> = receipts
        .iter()
        .flat_map(|receipt| receipt.batch_seqs.iter().copied())
        .collect();
    let queued = read_queue(dir)?;
    let mut failed = Vec::new();
    let mut failed_seqs = HashSet::new();
    for batch in queued {
        if batch.is_empty() || batch.iter().all(|event| covered.contains(&event.seq)) {
            continue;
        }
        if batch.iter().any(|event| covered.contains(&event.seq)) {
            // 批次被回执盖住了一部分：没盖住的留在 critical 日志里，下面按开放批次重封。
            continue;
        }
        for event in &batch {
            failed_seqs.insert(event.seq);
        }
        failed.push(batch);
    }
    let open: Vec<TraceEvent> = history
        .iter()
        .filter(|event| !covered.contains(&event.seq) && !failed_seqs.contains(&event.seq))
        .cloned()
        .collect();
    let open_started_ms = open.first().map(|event| event.ts_ms);
    Ok(Loaded {
        open,
        open_started_ms,
        failed,
        covered,
    })
}

struct Runtime {
    dir: PathBuf,
    policy: Policy,
    client: RekorClient,
    clock: Arc<dyn Clock>,
    open: Vec<TraceEvent>,
    open_started_ms: Option<u64>,
    failed: Vec<Vec<TraceEvent>>,
    covered: HashSet<u64>,
    gap: Arc<AtomicU64>,
    alert: Arc<AtomicBool>,
    reason: Arc<Mutex<Option<String>>>,
    poll_done: Arc<AtomicU64>,
    seen: Arc<AtomicU64>,
}

fn run(mut rt: Runtime, rx: Receiver<Cmd>) {
    rt.maybe_seal(false);
    loop {
        match rx.recv() {
            Ok(Cmd::Event(event)) => {
                rt.on_event(event);
                rt.seen.fetch_add(1, Ordering::SeqCst);
            }
            Ok(Cmd::Poll(token)) => {
                rt.maybe_seal(false);
                rt.poll_done.store(token, Ordering::SeqCst);
            }
            Err(_) => {
                rt.maybe_seal(true);
                break;
            }
        }
    }
}

impl Runtime {
    fn on_event(&mut self, event: TraceEvent) {
        if self.known(event.seq) {
            return;
        }
        if self.open.is_empty() {
            self.open_started_ms = Some(self.clock.wall_ms());
        }
        self.open.push(event);
        self.maybe_seal(false);
    }

    fn known(&self, seq: u64) -> bool {
        self.covered.contains(&seq)
            || self.open.iter().any(|event| event.seq == seq)
            || self
                .failed
                .iter()
                .any(|batch| batch.iter().any(|event| event.seq == seq))
    }

    fn maybe_seal(&mut self, force: bool) {
        self.try_retry();
        if self.open.is_empty() {
            return;
        }
        // 时间窗或 shutdown 把当前开放批次整批封口。条数只切下满 N 的前缀，避免恢复时把多批并成一条。
        if force || self.time_due() {
            let batch = std::mem::take(&mut self.open);
            self.open_started_ms = None;
            self.submit_or_fail(batch);
            return;
        }
        while self.count_due() {
            let Some(limit) = self.policy.max_events else {
                break;
            };
            let take = usize::try_from(limit).unwrap_or(self.open.len());
            let take = take.min(self.open.len());
            if take == 0 {
                break;
            }
            let batch: Vec<_> = self.open.drain(..take).collect();
            if self.open.is_empty() {
                self.open_started_ms = None;
            } else {
                self.open_started_ms = Some(self.clock.wall_ms());
            }
            self.submit_or_fail(batch);
            if !self.failed.is_empty() {
                break;
            }
        }
    }

    fn count_due(&self) -> bool {
        self.policy
            .max_events
            .is_some_and(|limit| self.open.len() as u64 >= limit)
    }

    fn time_due(&self) -> bool {
        match (self.policy.max_age_ms, self.open_started_ms) {
            (Some(window), Some(start)) => self.clock.wall_ms().saturating_sub(start) >= window,
            _ => false,
        }
    }

    fn try_retry(&mut self) {
        let index = 0;
        while index < self.failed.len() {
            let batch = self.failed[index].clone();
            match self.client.submit_batch(&batch, self.clock.wall_ms()) {
                Ok(receipt) => {
                    if let Err(err) = append_receipt(&self.dir, &receipt) {
                        self.refresh_gap(Some(format!("回执落盘失败: {err}")));
                        break;
                    }
                    for seq in receipt.batch_seqs {
                        self.covered.insert(seq);
                    }
                    self.failed.remove(index);
                    if let Err(err) = self.persist_queue() {
                        self.refresh_gap(Some(format!("队列落盘失败: {err}")));
                        break;
                    }
                    self.refresh_gap(None);
                }
                Err(err) => {
                    self.refresh_gap(Some(anchor_failure(&err)));
                    break;
                }
            }
        }
    }

    fn submit_or_fail(&mut self, batch: Vec<TraceEvent>) {
        if batch.is_empty() {
            return;
        }
        match self.client.submit_batch(&batch, self.clock.wall_ms()) {
            Ok(receipt) => {
                if let Err(err) = append_receipt(&self.dir, &receipt) {
                    self.fail_batch(batch, format!("回执落盘失败: {err}"));
                    return;
                }
                for seq in receipt.batch_seqs {
                    self.covered.insert(seq);
                }
                if let Err(err) = self.persist_queue() {
                    self.refresh_gap(Some(format!("队列落盘失败: {err}")));
                    return;
                }
                self.refresh_gap(None);
            }
            Err(err) => self.fail_batch(batch, anchor_failure(&err)),
        }
    }

    fn fail_batch(&mut self, batch: Vec<TraceEvent>, reason: String) {
        self.failed.push(batch);
        let reason = match self.persist_queue() {
            Ok(()) => reason,
            Err(err) => format!("{reason}; 队列落盘失败: {err}"),
        };
        self.refresh_gap(Some(reason));
    }

    fn refresh_gap(&self, failure: Option<String>) {
        let gap = self
            .failed
            .iter()
            .map(|batch| u64::try_from(batch.len()).unwrap_or(u64::MAX))
            .fold(0u64, u64::saturating_add);
        let alert = gap > 0;
        let reason = if alert {
            Some(failure.unwrap_or_else(|| "锚定队列待重试".into()))
        } else {
            None
        };
        // 先落盘再发布计数，读到新计数时状态文件已经是同一次更新。
        let _ = write_status(&self.dir, gap, alert, reason.clone());
        *self.reason.lock().unwrap() = reason;
        self.alert.store(alert, Ordering::SeqCst);
        self.gap.store(gap, Ordering::SeqCst);
    }

    fn persist_queue(&self) -> Result<(), String> {
        let path = anchor_queue_path(&self.dir);
        if self.failed.is_empty() {
            if path.exists() {
                fs::remove_file(&path).map_err(|err| err.to_string())?;
            }
            return Ok(());
        }
        let mut body = String::new();
        for batch in &self.failed {
            let line = serde_json::to_string(&QueuedBatch { events: batch })
                .map_err(|err| err.to_string())?;
            body.push_str(&line);
            body.push('\n');
        }
        atomic_write(&path, body.as_bytes())
    }
}

fn anchor_failure(err: &AnchorError) -> String {
    match err {
        AnchorError::Unreachable(detail) => format!("rekor 不可达: {detail}"),
        other => other.to_string(),
    }
}

#[derive(Serialize)]
struct QueuedBatch<'a> {
    events: &'a [TraceEvent],
}

#[derive(Deserialize)]
struct OwnedBatch {
    events: Vec<TraceEvent>,
}

#[derive(Serialize, Deserialize)]
struct StatusFile {
    anchor_gap: u64,
    alert: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    alert_reason: Option<String>,
}

fn append_receipt(dir: &Path, receipt: &AnchorReceipt) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(anchor_sidecar_path(dir))
        .map_err(|err| err.to_string())?;
    let line = serde_json::to_string(receipt).map_err(|err| err.to_string())?;
    file.write_all(line.as_bytes())
        .map_err(|err| err.to_string())?;
    file.write_all(b"\n").map_err(|err| err.to_string())?;
    file.flush().map_err(|err| err.to_string())?;
    file.sync_data().map_err(|err| err.to_string())?;
    Ok(())
}

fn write_status(
    dir: &Path,
    anchor_gap: u64,
    alert: bool,
    alert_reason: Option<String>,
) -> Result<(), String> {
    let body = serde_json::to_vec(&StatusFile {
        anchor_gap,
        alert,
        alert_reason,
    })
    .map_err(|err| err.to_string())?;
    atomic_write(&anchor_status_path(dir), &body)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .map_err(|err| err.to_string())?;
        file.write_all(bytes).map_err(|err| err.to_string())?;
        file.flush().map_err(|err| err.to_string())?;
        file.sync_data().map_err(|err| err.to_string())?;
    }
    fs::rename(&tmp, path).map_err(|err| err.to_string())
}

fn read_queue(dir: &Path) -> Result<Vec<Vec<TraceEvent>>, TraceError> {
    let batches: Vec<OwnedBatch> =
        read_json_lines(dir, &anchor_queue_path(dir), "anchor-queue.jsonl")?;
    Ok(batches.into_iter().map(|batch| batch.events).collect())
}

fn read_json_lines<T: for<'de> Deserialize<'de>>(
    dir: &Path,
    path: &Path,
    label: &str,
) -> Result<Vec<T>, TraceError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)
        .map_err(|err| TraceError::Open(format!("读取 {}/{}: {err}", dir.display(), label)))?;
    let mut rows = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str(line) {
            Ok(row) => rows.push(row),
            Err(err) => {
                return Err(TraceError::Inconsistent(format!(
                    "{label} 第 {} 行无法解析: {err}",
                    idx + 1
                )));
            }
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;
    use crate::rekor::stub::RekorStub;
    use crate::rekor::verify_anchor;
    use crate::{critical_log_path, read_trace_log, Level, OpenOutcome, Trace, TraceConfig};
    use serde_json::json;
    use std::time::{Duration, Instant};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "af-trace-anchor-{label}-{}-{n}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn settings(endpoint: &str, key: &[u8], batch_n: u64, batch_t_ms: u64) -> AnchorSettings {
        AnchorSettings {
            endpoint: Some(endpoint.to_string()),
            hmac_key: key.to_vec(),
            origin: "rekor-local".into(),
            batch_n,
            batch_t_ms,
        }
    }

    fn open_trace(dir: &Path, clock: &ManualClock, anchor: AnchorSettings) -> Arc<Trace> {
        let cfg = TraceConfig {
            critical_capacity: 64,
            normal_capacity: 64,
            critical_consume_delay_us: 0,
            normal_consume_delay_us: 0,
            dir: dir.to_path_buf(),
            otel_endpoint: None,
            otel_batch_size: 512,
            anchor,
        };
        let (trace, outcome) = Trace::open_with_clock(cfg, clock.clone()).unwrap();
        assert_eq!(outcome, OpenOutcome::Created);
        trace
    }

    fn wait_until(mut ready: impl FnMut() -> bool, what: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready() {
            assert!(Instant::now() < deadline, "{what}");
            std::thread::yield_now();
        }
    }

    fn verify_dir(dir: &Path, key: &[u8]) {
        let events = read_trace_log(&critical_log_path(dir)).unwrap();
        let receipts = read_anchor_receipts(dir).unwrap();
        assert!(!receipts.is_empty());
        for receipt in &receipts {
            let batch: Vec<TraceEvent> = receipt
                .batch_seqs
                .iter()
                .map(|seq| {
                    events
                        .iter()
                        .find(|event| event.seq == *seq)
                        .unwrap_or_else(|| panic!("回执 seq {seq} 不在 critical 日志"))
                        .clone()
                })
                .collect();
            verify_anchor(&batch, receipt, key, "rekor-local").unwrap();
        }
    }

    #[test]
    fn disabled_anchor_does_not_write_sidecar_or_gap() {
        let dir = TempDir::new("off");
        let clock = ManualClock::new(10);
        let trace = open_trace(dir.path(), &clock, AnchorSettings::default());
        trace.write_critical("audit", json!({ "i": 1 })).unwrap();
        trace.write_normal("step", json!(1));
        let report = trace.shutdown().unwrap();
        assert_eq!(report.anchor_gap, 0);
        assert!(!report.anchor_alert);
        assert!(report.anchor_alert_reason.is_none());
        assert_eq!(report.gap_counter, 0);
        assert!(!anchor_sidecar_path(dir.path()).exists());
        assert!(!anchor_queue_path(dir.path()).exists());
        assert_eq!(report.critical_persisted, 1);
    }

    #[test]
    fn batch_n_submits_and_shutdown_flushes_the_remainder() {
        let key = b"batch-n-key";
        let stub = RekorStub::spawn("rekor-local", key);
        let dir = TempDir::new("n");
        let clock = ManualClock::new(20);
        let trace = open_trace(dir.path(), &clock, settings(&stub.endpoint(), key, 2, 0));
        let first = trace.write_critical("audit", json!({ "i": 1 })).unwrap();
        let second = trace.write_critical("audit", json!({ "i": 2 })).unwrap();
        wait_until(
            || stub.accepted.load(Ordering::SeqCst) == 1,
            "两条 critical 应封成一批",
        );
        assert_eq!(trace.anchor_gap(), 0);
        assert!(!trace.anchor_alert());
        assert_eq!(trace.gap_counter(), 0);
        let third = trace.write_critical("audit", json!({ "i": 3 })).unwrap();
        wait_until(|| trace.anchor_seen() >= 3, "第三条应已进入锚线程");
        assert_eq!(stub.accepted.load(Ordering::SeqCst), 1, "未满 N 不应提交");
        let report = trace.shutdown().unwrap();
        assert_eq!(report.anchor_gap, 0);
        assert!(!report.anchor_alert);
        assert_eq!(report.gap_counter, 0);
        assert_eq!(stub.accepted.load(Ordering::SeqCst), 2);
        let receipts = read_anchor_receipts(dir.path()).unwrap();
        assert_eq!(receipts[0].batch_seqs, vec![first, second]);
        assert_eq!(receipts[1].batch_seqs, vec![third]);
        assert_eq!(receipts[0].log_index, 0);
        assert_eq!(receipts[1].log_index, 1);
        assert!(!receipts[0].signature_b64.is_empty());
        verify_dir(dir.path(), key);
        let crit = read_trace_log(&critical_log_path(dir.path())).unwrap();
        assert!(crit.iter().all(|event| event.level == Level::Critical));
        assert_eq!(crit.len(), 3);
    }

    #[test]
    fn batch_window_uses_the_injected_clock() {
        let key = b"batch-t-key";
        let stub = RekorStub::spawn("rekor-local", key);
        let dir = TempDir::new("t");
        let clock = ManualClock::new(10_000);
        let trace = open_trace(
            dir.path(),
            &clock,
            settings(&stub.endpoint(), key, 0, 5_000),
        );
        trace.write_critical("audit", json!({ "i": 1 })).unwrap();
        trace.write_critical("audit", json!({ "i": 2 })).unwrap();
        wait_until(|| trace.anchor_seen() >= 2, "两条应已进入锚线程");
        trace.poll_anchor();
        assert_eq!(stub.accepted.load(Ordering::SeqCst), 0);
        assert!(read_anchor_receipts(dir.path()).unwrap().is_empty());
        clock.advance(4_999);
        trace.poll_anchor();
        assert_eq!(stub.accepted.load(Ordering::SeqCst), 0, "差 1 毫秒不应封口");
        clock.advance(1);
        trace.poll_anchor();
        assert_eq!(stub.accepted.load(Ordering::SeqCst), 1);
        let report = trace.shutdown().unwrap();
        assert_eq!(report.anchor_gap, 0);
        assert!(!report.anchor_alert);
        assert_eq!(report.gap_counter, 0);
        let receipts = read_anchor_receipts(dir.path()).unwrap();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].batch_seqs.len(), 2);
        assert_eq!(receipts[0].integrated_time_ms, 15_000);
        verify_dir(dir.path(), key);
    }

    #[test]
    fn unreachable_queues_for_retry_without_blocking_critical() {
        let key = b"retry-key";
        let stub = RekorStub::spawn("rekor-local", key);
        stub.fail_remaining.store(1_000, Ordering::SeqCst);
        let dir = TempDir::new("retry");
        let clock = ManualClock::new(7);
        let trace = open_trace(dir.path(), &clock, settings(&stub.endpoint(), key, 1, 0));
        let started = Instant::now();
        let mut seqs = Vec::new();
        for i in 0..2 {
            seqs.push(trace.write_critical("audit", json!({ "i": i })).unwrap());
        }
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "rekor 断连时 write_critical 仍应立刻返回"
        );
        wait_until(
            || trace.anchor_gap() == 2 && trace.anchor_alert(),
            "两次提交失败应记下 2 条锚定缺口和告警",
        );
        assert_eq!(trace.gap_counter(), 0, "锚定缺口不得计入 gap_counter");
        let reason = trace.anchor_alert_reason().expect("告警要有原因");
        assert!(reason.contains("不可达"), "{reason}");
        let queued = fs::read_to_string(anchor_queue_path(dir.path())).unwrap();
        assert!(queued.contains(&seqs[0].to_string()));
        let status: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(anchor_status_path(dir.path())).unwrap())
                .unwrap();
        assert_eq!(status["alert"], true);
        assert_eq!(status["anchor_gap"], 2);
        let on_disk = read_trace_log(&critical_log_path(dir.path())).unwrap();
        assert_eq!(on_disk.len(), 2);
        assert!(read_anchor_receipts(dir.path()).unwrap().is_empty());

        stub.fail_remaining.store(0, Ordering::SeqCst);
        trace.poll_anchor();
        assert_eq!(trace.anchor_gap(), 0);
        assert!(!trace.anchor_alert());
        assert!(trace.anchor_alert_reason().is_none());
        let report = trace.shutdown().unwrap();
        assert_eq!(report.anchor_gap, 0);
        assert!(!report.anchor_alert);
        assert_eq!(report.gap_counter, 0);
        assert_eq!(report.gap_reported, 0);
        verify_dir(dir.path(), key);
        assert!(!anchor_queue_path(dir.path()).exists());
    }

    #[test]
    fn wedged_rekor_does_not_block_the_next_critical_write() {
        let key = b"wedge-key";
        let stub = RekorStub::spawn("rekor-local", key);
        let _hold = stub.gate.lock().unwrap();
        let dir = TempDir::new("wedge");
        let clock = ManualClock::new(3);
        let trace = open_trace(dir.path(), &clock, settings(&stub.endpoint(), key, 1, 0));
        trace.write_critical("audit", json!({ "i": 1 })).unwrap();
        wait_until(
            || stub.entered.load(Ordering::SeqCst) >= 1,
            "第一条应已进入 stub",
        );
        let started = Instant::now();
        trace.write_critical("audit", json!({ "i": 2 })).unwrap();
        trace.write_normal("step", json!(1));
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "锚线程堵在 HTTP 时，critical 与 normal 写入仍应立刻返回"
        );
        assert_eq!(trace.gap_counter(), 0);
        drop(_hold);
        let report = trace.shutdown().unwrap();
        assert_eq!(report.critical_persisted, 2);
        assert_eq!(report.normal_persisted, 1);
        assert_eq!(report.anchor_gap, 0);
        assert_eq!(report.gap_counter, 0);
        let receipts = read_anchor_receipts(dir.path()).unwrap();
        assert_eq!(receipts.len(), 2);
        verify_dir(dir.path(), key);
        let normals = crate::read_trace_log(&crate::normal_log_path(dir.path())).unwrap();
        assert!(normals.iter().all(|event| event.level == Level::Normal));
        assert!(receipts.iter().all(|receipt| receipt
            .batch_seqs
            .iter()
            .all(|seq| normals.iter().all(|event| event.seq != *seq))));
    }

    #[test]
    fn restart_retries_the_persisted_queue() {
        let key = b"restart-key";
        let stub = RekorStub::spawn("rekor-local", key);
        stub.fail_remaining.store(1_000, Ordering::SeqCst);
        let dir = TempDir::new("restart");
        let clock = ManualClock::new(11);
        let anchor = settings(&stub.endpoint(), key, 1, 0);
        let trace = open_trace(dir.path(), &clock, anchor.clone());
        let seq = trace
            .write_critical("audit", json!({ "keep": true }))
            .unwrap();
        wait_until(|| trace.anchor_gap() == 1, "首启提交失败应入队");
        let report = trace.shutdown().unwrap();
        assert_eq!(report.anchor_gap, 1);
        assert!(report.anchor_alert);
        assert!(anchor_queue_path(dir.path()).exists());
        assert!(read_anchor_receipts(dir.path()).unwrap().is_empty());
        drop(trace);

        stub.fail_remaining.store(1_000, Ordering::SeqCst);
        let cfg = TraceConfig {
            critical_capacity: 64,
            normal_capacity: 64,
            critical_consume_delay_us: 0,
            normal_consume_delay_us: 0,
            dir: dir.path().to_path_buf(),
            otel_endpoint: None,
            otel_batch_size: 512,
            anchor,
        };
        let (again, outcome) = Trace::open_with_clock(cfg, clock.clone()).unwrap();
        assert_eq!(outcome, OpenOutcome::AlreadyDeclared);
        again.poll_anchor();
        assert_eq!(again.anchor_gap(), 1, "rekor 仍不可达时队列应留着");
        assert!(again.anchor_alert());
        stub.fail_remaining.store(0, Ordering::SeqCst);
        again.poll_anchor();
        assert_eq!(again.anchor_gap(), 0);
        assert!(!again.anchor_alert());
        let report = again.shutdown().unwrap();
        assert_eq!(report.gap_counter, 0);
        assert_eq!(report.anchor_gap, 0);
        let receipts = read_anchor_receipts(dir.path()).unwrap();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].batch_seqs, vec![seq]);
        verify_dir(dir.path(), key);
    }

    #[test]
    fn refused_endpoint_does_not_fail_open_and_still_persists_critical() {
        let dir = TempDir::new("refused");
        let clock = ManualClock::new(4);
        let trace = open_trace(
            dir.path(),
            &clock,
            settings("http://127.0.0.1:1", b"k", 1, 0),
        );
        let seq = trace.write_critical("audit", json!(1)).unwrap();
        wait_until(
            || trace.anchor_gap() == 1 && trace.anchor_alert(),
            "拒连应入队并告警",
        );
        assert_eq!(trace.gap_counter(), 0);
        let report = trace.shutdown().unwrap();
        assert_eq!(report.critical_persisted, 1);
        assert_eq!(report.anchor_gap, 1);
        assert!(report.anchor_alert);
        let events = read_trace_log(&critical_log_path(dir.path())).unwrap();
        assert_eq!(events[0].seq, seq);
        assert!(anchor_queue_path(dir.path()).exists());
    }

    #[test]
    fn default_batch_seals_each_event() {
        let key = b"default-batch";
        let stub = RekorStub::spawn("rekor-local", key);
        let dir = TempDir::new("each");
        let clock = ManualClock::new(1);
        let trace = open_trace(dir.path(), &clock, settings(&stub.endpoint(), key, 0, 0));
        trace.write_critical("audit", json!(1)).unwrap();
        wait_until(
            || stub.accepted.load(Ordering::SeqCst) == 1,
            "N 与 T 都是 0 时应逐条封口",
        );
        let _ = trace.shutdown().unwrap();
        verify_dir(dir.path(), key);
    }
}
