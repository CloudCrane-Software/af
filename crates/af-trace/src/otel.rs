//! 非关键 OTel lane。
//!
//! normal 事件在 `normal.jsonl` 写成功之后才进入本 lane。critical 事件（含 gap span）
//! 不进入。feature `otel` 关闭、端点为空，或 OTLP exporter 建不起来时，lane 离线：
//! 只把落盘成功的 normal 事件计入 `Trace::otel_counted`，`Trace::open` 不因此失败。
//! 在线时按 `TraceConfig::otel_batch_size` 分批交给 `SpanExporter`。导出失败或
//! 队列满只走既有 `record_gap`，不阻塞 `write_normal`。
//!
//! 真实 collector 的连通不在 `cargo test` 里，留给 orchestration 手册。

#[cfg(feature = "otel")]
use crate::event::Level;
use crate::event::TraceEvent;
use std::sync::atomic::AtomicU64;
#[cfg(feature = "otel")]
use std::sync::atomic::Ordering;
use std::sync::mpsc::SyncSender;
#[cfg(feature = "otel")]
use std::sync::mpsc::{sync_channel, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;

/// 非关键 lane 的运行形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtelMode {
    /// 不导出。落盘成功的 normal 事件只计数。
    Offline,
    /// 已装上 exporter（OTLP 或测试 mock）。失败记缺口。
    Online,
}

/// 批大小。`0` 当成配方没写，用 OTel BSP 的缺省 512。至少为 1。
/// feature 关闭时库路径不调用它，单测仍覆盖这条归一化。
#[cfg_attr(not(feature = "otel"), allow(dead_code))]
pub(crate) fn normalize_batch_size(size: usize) -> usize {
    if size == 0 {
        512
    } else {
        size
    }
}

pub(crate) type FailHook = Arc<dyn Fn(u64) + Send + Sync>;
pub(crate) type SpawnWorker = Box<dyn FnOnce(Arc<AtomicU64>, FailHook) -> JoinHandle<()> + Send>;

pub(crate) struct Prepared {
    pub mode: OtelMode,
    pub reason: Option<String>,
    pub tx: Option<SyncSender<TraceEvent>>,
    pub spawn: Option<SpawnWorker>,
}

#[cfg(not(feature = "otel"))]
pub(crate) fn prepare(_endpoint: Option<&str>, _batch_size: usize) -> Prepared {
    Prepared {
        mode: OtelMode::Offline,
        reason: Some("feature otel 未启用，非关键 lane 只计数".into()),
        tx: None,
        spawn: None,
    }
}

#[cfg(feature = "otel")]
pub(crate) fn prepare(endpoint: Option<&str>, batch_size: usize) -> Prepared {
    let Some(endpoint) = endpoint else {
        return offline("未配置 OTLP 端点，非关键 lane 只计数");
    };
    match build_otlp(endpoint) {
        Ok(exporter) => online(batch_size, ExportTarget::Otlp(Box::new(exporter))),
        Err(reason) => offline(&format!(
            "OTLP exporter 建不起来，非关键 lane 只计数: {reason}"
        )),
    }
}

#[cfg(all(test, feature = "otel"))]
pub(crate) fn prepare_with_mock(mock: MockExporter, batch_size: usize) -> Prepared {
    online(batch_size, ExportTarget::Mock(mock))
}

#[cfg(feature = "otel")]
fn offline(reason: &str) -> Prepared {
    Prepared {
        mode: OtelMode::Offline,
        reason: Some(reason.to_string()),
        tx: None,
        spawn: None,
    }
}

#[cfg(feature = "otel")]
fn online(batch_size: usize, target: ExportTarget) -> Prepared {
    let batch_size = normalize_batch_size(batch_size);
    let (tx, rx) = sync_channel(batch_size);
    Prepared {
        mode: OtelMode::Online,
        reason: None,
        tx: Some(tx),
        spawn: Some(Box::new(move |counted, on_fail| {
            spawn_worker(rx, target, batch_size, counted, on_fail)
        })),
    }
}

#[cfg(feature = "otel")]
fn spawn_worker(
    rx: Receiver<TraceEvent>,
    target: ExportTarget,
    batch_size: usize,
    counted: Arc<AtomicU64>,
    on_fail: FailHook,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("af-trace-otel".into())
        .spawn(move || worker_loop(rx, target, batch_size, counted, on_fail))
        .expect("spawn af-trace otel thread")
}

#[cfg(feature = "otel")]
fn worker_loop(
    rx: Receiver<TraceEvent>,
    target: ExportTarget,
    batch_size: usize,
    counted: Arc<AtomicU64>,
    on_fail: FailHook,
) {
    use opentelemetry_sdk::trace::{IdGenerator, RandomIdGenerator};

    let ids = RandomIdGenerator::default();
    let trace_id = ids.new_trace_id();
    let scope = opentelemetry::InstrumentationScope::builder("af-trace")
        .with_version(env!("CARGO_PKG_VERSION"))
        .build();
    while let Ok(first) = rx.recv() {
        let mut events = vec![first];
        while events.len() < batch_size {
            match rx.try_recv() {
                Ok(event) => events.push(event),
                Err(_) => break,
            }
        }
        events.retain(|event| event.level == Level::Normal);
        if events.is_empty() {
            continue;
        }
        let seqs: Vec<u64> = events.iter().map(|event| event.seq).collect();
        let spans = events
            .iter()
            .map(|event| event_to_span(event, trace_id, &ids, &scope))
            .collect();
        match export_blocking(&target, spans) {
            Ok(()) => {
                counted.fetch_add(
                    u64::try_from(seqs.len()).unwrap_or(u64::MAX),
                    Ordering::Relaxed,
                );
            }
            Err(_) => {
                for seq in seqs {
                    on_fail(seq);
                }
            }
        }
    }
}

#[cfg(feature = "otel")]
fn event_to_span(
    event: &TraceEvent,
    trace_id: opentelemetry::trace::TraceId,
    ids: &opentelemetry_sdk::trace::RandomIdGenerator,
    scope: &opentelemetry::InstrumentationScope,
) -> opentelemetry_sdk::trace::SpanData {
    use opentelemetry::trace::{SpanContext, SpanId, SpanKind, Status, TraceFlags, TraceState};
    use opentelemetry::KeyValue;
    use opentelemetry_sdk::trace::{IdGenerator, SpanData, SpanEvents, SpanLinks};
    use std::time::{Duration, UNIX_EPOCH};

    let span_context = SpanContext::new(
        trace_id,
        ids.new_span_id(),
        TraceFlags::SAMPLED,
        false,
        TraceState::default(),
    );
    let when = UNIX_EPOCH
        .checked_add(Duration::from_millis(event.ts_ms))
        .unwrap_or(UNIX_EPOCH);
    SpanData {
        span_context,
        parent_span_id: SpanId::INVALID,
        parent_span_is_remote: false,
        span_kind: SpanKind::Internal,
        name: event.kind.clone().into(),
        start_time: when,
        end_time: when,
        attributes: vec![
            KeyValue::new("af.seq", event.seq.to_string()),
            KeyValue::new("af.level", "normal"),
            KeyValue::new("af.kind", event.kind.clone()),
            KeyValue::new("af.payload", event.payload.to_string()),
            KeyValue::new("af.ts_ms", event.ts_ms.to_string()),
        ],
        dropped_attributes_count: 0,
        events: SpanEvents::default(),
        links: SpanLinks::default(),
        status: Status::Unset,
        instrumentation_scope: scope.clone(),
    }
}

#[cfg(feature = "otel")]
fn export_blocking(
    target: &ExportTarget,
    batch: Vec<opentelemetry_sdk::trace::SpanData>,
) -> Result<(), String> {
    use opentelemetry_sdk::trace::SpanExporter;

    let result = match target {
        ExportTarget::Otlp(exporter) => block_on(exporter.export(batch)),
        #[cfg(test)]
        ExportTarget::Mock(exporter) => block_on(exporter.export(batch)),
    };
    result.map_err(|err| err.to_string())
}

#[cfg(feature = "otel")]
fn build_otlp(endpoint: &str) -> Result<opentelemetry_otlp::SpanExporter, String> {
    use opentelemetry_otlp::WithExportConfig;
    use std::time::Duration;

    opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .with_protocol(opentelemetry_otlp::Protocol::HttpBinary)
        .with_timeout(Duration::from_secs(10))
        .build()
        .map_err(|err| err.to_string())
}

/// 把 [`SpanExporter::export`] 的 future 在当前线程跑完。
/// 不引入 `futures-executor`：OTLP 的 HTTP future 靠 waker 唤醒本线程。
#[cfg(feature = "otel")]
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::task::{Context, Poll, Wake, Waker};
    use std::thread;

    struct ThreadWaker(thread::Thread);

    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => thread::park(),
        }
    }
}

#[cfg(feature = "otel")]
enum ExportTarget {
    Otlp(Box<opentelemetry_otlp::SpanExporter>),
    #[cfg(test)]
    Mock(MockExporter),
}

#[cfg(all(test, feature = "otel"))]
#[derive(Clone, Debug)]
pub(crate) struct MockExporter {
    pub spans: Arc<std::sync::Mutex<Vec<opentelemetry_sdk::trace::SpanData>>>,
    pub batches: Arc<std::sync::Mutex<Vec<usize>>>,
    pub attempted: Arc<std::sync::Mutex<Vec<String>>>,
    pub fail: Arc<std::sync::atomic::AtomicBool>,
    pub entered: Arc<AtomicU64>,
    pub gate: Arc<std::sync::Mutex<()>>,
}

#[cfg(all(test, feature = "otel"))]
impl MockExporter {
    pub(crate) fn new() -> Self {
        Self {
            spans: Arc::new(std::sync::Mutex::new(Vec::new())),
            batches: Arc::new(std::sync::Mutex::new(Vec::new())),
            attempted: Arc::new(std::sync::Mutex::new(Vec::new())),
            fail: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            entered: Arc::new(AtomicU64::new(0)),
            gate: Arc::new(std::sync::Mutex::new(())),
        }
    }
}

#[cfg(all(test, feature = "otel"))]
impl opentelemetry_sdk::trace::SpanExporter for MockExporter {
    fn export(
        &self,
        batch: Vec<opentelemetry_sdk::trace::SpanData>,
    ) -> impl std::future::Future<Output = opentelemetry_sdk::error::OTelSdkResult> + Send {
        self.entered.fetch_add(1, Ordering::SeqCst);
        let gate = self.gate.lock().unwrap();
        self.attempted
            .lock()
            .unwrap()
            .extend(batch.iter().map(|span| span.name.as_ref().to_string()));
        let result = if self.fail.load(Ordering::SeqCst) {
            Err(opentelemetry_sdk::error::OTelSdkError::InternalFailure(
                "mock otel export failed".into(),
            ))
        } else {
            self.batches.lock().unwrap().push(batch.len());
            self.spans.lock().unwrap().extend(batch);
            Ok(())
        };
        drop(gate);
        std::future::ready(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_batch_size_uses_the_bsp_default() {
        assert_eq!(normalize_batch_size(0), 512);
        assert_eq!(normalize_batch_size(1), 1);
        assert_eq!(normalize_batch_size(7), 7);
    }
}
