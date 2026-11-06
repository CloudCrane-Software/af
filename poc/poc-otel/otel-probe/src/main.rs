//! poc-otel probe：af-trace → OpenTelemetry 出口实测
//!
//! 子命令：
//!   raw  — 裸导出器往返时延（gRPC vs HTTP，绕过 BatchSpanProcessor）
//!   pipe — 全链路（BatchSpanProcessor→collector→file）吞吐 + 端到端时延
//!   down — collector 挂掉时的行为（recover / shutdown_down）
//!
//! 事件结构模拟 af-trace（crates/af-trace/src/event.rs）：
//!   level ∈ {critical, degraded, gap}；gap 事件带
//!   af.gap.count / af.gap.total_dropped / af.gap.first_seq / af.gap.last_seq
//!   且所有事件带 af.gap_counter（累计丢弃计数）。

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use opentelemetry::trace::{
    Span as _, SpanContext, SpanId, SpanKind, Status, TraceFlags, TraceId, TraceState, Tracer,
    TracerProvider as _,
};
use opentelemetry::{InstrumentationScope, KeyValue};
use opentelemetry_otlp::{Protocol, WithExportConfig};
use opentelemetry_sdk::trace::SpanExporter as _;
use opentelemetry_sdk::trace::{
    BatchConfigBuilder, BatchSpanProcessor, SdkTracerProvider, SpanData, SpanEvents,
    SpanLinks,
};
use opentelemetry_sdk::Resource;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------- 极简参数解析 ----------

struct Args {
    map: HashMap<String, String>,
}

impl Args {
    fn parse() -> Self {
        let mut map = HashMap::new();
        let mut it = std::env::args().skip(1);
        if let Some(cmd) = it.next() {
            map.insert("_cmd".into(), cmd);
        }
        while let Some(k) = it.next() {
            let k = k.trim_start_matches("--").to_string();
            let v = it.next().unwrap_or_default();
            map.insert(k, v);
        }
        Args { map }
    }
    fn get(&self, k: &str, d: &str) -> String {
        self.map.get(k).cloned().unwrap_or_else(|| d.to_string())
    }
    fn num(&self, k: &str, d: u64) -> u64 {
        self.map.get(k).and_then(|v| v.parse().ok()).unwrap_or(d)
    }
}

fn build_exporter(
    transport: &str,
    endpoint: &str,
    timeout_ms: u64,
) -> Result<opentelemetry_otlp::SpanExporter, Box<dyn std::error::Error>> {
    let exporter = match transport {
        "grpc" => opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .with_timeout(Duration::from_millis(timeout_ms))
            .build()?,
        "http" => opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_protocol(Protocol::HttpBinary)
            .with_endpoint(endpoint) // 编程式 endpoint 按原样使用，须含 /v1/traces
            .with_timeout(Duration::from_millis(timeout_ms))
            .build()?,
        other => return Err(format!("unknown transport: {other}").into()),
    };
    Ok(exporter)
}

fn payload_string(i: u64, bytes: u64) -> String {
    let fill = bytes.saturating_sub(40).max(0);
    format!("{{\"seq\":{i},\"payload\":\"{}\"}}", "x".repeat(fill as usize))
}

/// 模拟 af-trace 事件属性：三级 + gap_counter
fn af_attributes(i: u64, payload_bytes: u64) -> Vec<KeyValue> {
    let (level, kind) = if i % 10 == 9 {
        ("gap", "gap")
    } else if i % 3 == 0 {
        ("critical", "checkpoint")
    } else {
        ("degraded", "span.start")
    };
    let mut attrs = vec![
        KeyValue::new("af.seq", i as i64),
        KeyValue::new("af.level", level),
        KeyValue::new("af.kind", kind),
        KeyValue::new("af.ts_ms", now_ms() as i64),
        KeyValue::new("af.gap_counter", (i / 10) as i64), // 累计丢弃计数（模拟）
        KeyValue::new("payload", payload_string(i, payload_bytes)),
    ];
    if level == "gap" {
        attrs.push(KeyValue::new("af.gap.count", 1 + (i % 7) as i64));
        attrs.push(KeyValue::new("af.gap.total_dropped", (i / 10) as i64));
        attrs.push(KeyValue::new("af.gap.first_seq", (i.saturating_sub(9)) as i64));
        attrs.push(KeyValue::new("af.gap.last_seq", i as i64));
    }
    attrs
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn stats(v: &[u64]) -> serde_json::Value {
    let mut s: Vec<u64> = v.to_vec();
    s.sort_unstable();
    let avg = if s.is_empty() {
        0.0
    } else {
        s.iter().map(|&x| x as f64).sum::<f64>() / s.len() as f64
    };
    serde_json::json!({
        "n": s.len(),
        "avg": avg as u64,
        "p50": percentile(&s, 0.50),
        "p95": percentile(&s, 0.95),
        "max": s.last().copied().unwrap_or(0),
    })
}

fn write_json(out: &str, v: &serde_json::Value) {
    if out.is_empty() {
        return;
    }
    let mut f = std::fs::File::create(out).expect("create out file");
    f.write_all(serde_json::to_string_pretty(v).unwrap().as_bytes())
        .expect("write out file");
}

// ---------- tail 线程：从 collector file 输出统计到达 ----------

struct TailState {
    arrived: usize,
    lat_ms: Vec<u64>,
}

fn spawn_tail(
    path: String,
    end_map: Arc<Mutex<HashMap<String, u64>>>,
    target: usize,
) -> (Arc<AtomicUsize>, std::thread::JoinHandle<TailState>) {
    let arrived_ctr = Arc::new(AtomicUsize::new(0));
    let ctr = arrived_ctr.clone();
    let handle = std::thread::spawn(move || {
        let mut st = TailState {
            arrived: 0,
            lat_ms: Vec::new(),
        };
        let mut seen: std::collections::HashSet<String> = Default::default();
        let mut pos: u64 = 0;
        let mut buf = String::new();
        let mut f = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => std::fs::File::create(&path).expect("create tail file"),
        };
        let deadline = Instant::now() + Duration::from_secs(180);
        while st.arrived < target && Instant::now() < deadline {
            if let Ok(meta) = f.metadata() {
                let len = meta.len();
                if len > pos {
                    if f.seek(SeekFrom::Start(pos)).is_ok() {
                        let mut chunk = vec![0u8; (len - pos) as usize];
                        if f.read_exact(&mut chunk).is_ok() {
                            buf.push_str(&String::from_utf8_lossy(&chunk));
                            pos = len;
                            // 逐行解析，保留最后半行
                            let mut last_complete = 0usize;
                            for (idx, _) in buf.match_indices('\n') {
                                last_complete = idx + 1;
                            }
                            let (complete, rest) = buf.split_at(last_complete);
                            for line in complete.lines() {
                                if line.trim().is_empty() {
                                    continue;
                                }
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                                    let mut ids = Vec::new();
                                    collect_span_ids(&v, &mut ids);
                                    let map = end_map.lock().unwrap();
                                    for id in ids {
                                        if !seen.contains(&id) {
                                            if let Some(&end) = map.get(&id) {
                                                seen.insert(id);
                                                st.arrived += 1;
                                                st.lat_ms.push(now_ms().saturating_sub(end));
                                            }
                                        }
                                    }
                                }
                            }
                            buf = rest.to_string();
                        }
                    }
                }
            }
            ctr.store(st.arrived, Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(5));
        }
        ctr.store(st.arrived, Ordering::Relaxed);
        st
    });
    (arrived_ctr, handle)
}

fn collect_span_ids(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(m) => {
            if let Some(serde_json::Value::String(id)) = m.get("spanId") {
                out.push(id.clone());
            }
            for val in m.values() {
                collect_span_ids(val, out);
            }
        }
        serde_json::Value::Array(a) => {
            for val in a {
                collect_span_ids(val, out);
            }
        }
        _ => {}
    }
}

// ---------- raw：裸导出器往返 ----------

async fn cmd_raw(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let transport = args.get("transport", "grpc");
    let endpoint = args.get("endpoint", "http://127.0.0.1:4317");
    let batch = args.num("batch", 512).max(1) as usize;
    let total = args.num("total", 20000) as usize;
    let payload_bytes = args.num("payload-bytes", 1024);
    let out = args.get("out", "");

    let exporter = build_exporter(&transport, &endpoint, 10_000)?;

    // reqwest::blocking::Client::execute 不能在 tokio worker 上调用（自旋卡死）：
    // http 走独立 std::thread + futures-executor::block_on；grpc 留在当前 runtime。
    if transport == "http" {
        let handle = std::thread::spawn(move || {
            futures_executor::block_on(raw_loop(exporter, batch, total, payload_bytes))
        });
        let (call_lat_us, errors, exported, wall_ms) = handle.join().unwrap();
        let res = serde_json::json!({
            "mode": "raw", "transport": transport, "endpoint": endpoint,
            "batch": batch, "total_spans": exported, "payload_bytes": payload_bytes,
            "calls": call_lat_us.len(), "export_errors": errors,
            "wall_ms": wall_ms,
            "throughput_spans_per_s": if wall_ms > 0 { exported as f64 / (wall_ms as f64 / 1000.0) } else { 0.0 },
            "call_latency_us": stats(&call_lat_us),
            "driver": "std-thread+futures-executor",
        });
        println!("{}", serde_json::to_string_pretty(&res)?);
        write_json(&out, &res);
        return Ok(());
    }

    let (call_lat_us, errors, exported, wall_ms) = raw_loop(exporter, batch, total, payload_bytes).await;
    let res = serde_json::json!({
        "mode": "raw", "transport": transport, "endpoint": endpoint,
        "batch": batch, "total_spans": exported, "payload_bytes": payload_bytes,
        "calls": call_lat_us.len(), "export_errors": errors,
        "wall_ms": wall_ms,
        "throughput_spans_per_s": if wall_ms > 0 { exported as f64 / (wall_ms as f64 / 1000.0) } else { 0.0 },
        "call_latency_us": stats(&call_lat_us),
        "driver": "tokio",
    });
    println!("{}", serde_json::to_string_pretty(&res)?);
    write_json(&out, &res);
    Ok(())
}

async fn raw_loop(
    exporter: opentelemetry_otlp::SpanExporter,
    batch: usize,
    total: usize,
    payload_bytes: u64,
) -> (Vec<u64>, usize, usize, u64) {
    let scope = InstrumentationScope::builder("poc-otel").build();
    let mut call_lat_us = Vec::new();
    let mut errors = 0usize;
    let mut exported = 0usize;
    let t0 = Instant::now();
    let mut start = 0u64;
    while exported < total {
        let n = batch.min(total - exported);
        let mut batch_vec = Vec::with_capacity(n);
        for k in 0..n {
            let id = start + k as u64 + 1;
            batch_vec.push(SpanData {
                span_context: SpanContext::new(
                    TraceId::from(id as u128),
                    SpanId::from(id),
                    TraceFlags::SAMPLED,
                    false,
                    TraceState::default(),
                ),
                parent_span_id: SpanId::INVALID,
                parent_span_is_remote: false,
                span_kind: SpanKind::Internal,
                name: "af-trace-event".into(),
                start_time: SystemTime::now(),
                end_time: SystemTime::now(),
                attributes: af_attributes(id, payload_bytes),
                dropped_attributes_count: 0,
                events: SpanEvents::default(),
                links: SpanLinks::default(),
                status: Status::Ok,
                instrumentation_scope: scope.clone(),
            });
        }
        start += n as u64;
        let t = Instant::now();
        match exporter.export(batch_vec).await {
            Ok(()) => {}
            Err(_) => errors += 1,
        }
        call_lat_us.push(t.elapsed().as_micros() as u64);
        exported += n;
    }
    let wall_ms = t0.elapsed().as_millis() as u64;
    let _ = exporter.shutdown();
    (call_lat_us, errors, exported, wall_ms)
}

// ---------- pipe：全链路 ----------

async fn cmd_pipe(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let transport = args.get("transport", "grpc");
    let endpoint = args.get("endpoint", "http://127.0.0.1:4317");
    let total = args.num("total", 20000) as usize;
    let rate = args.num("rate", 0); // 0 = 最快
    let queue = args.num("queue", 65536) as usize;
    let export_batch = args.num("export-batch", 512) as usize;
    let delay_ms = args.num("delay-ms", 500);
    let payload_bytes = args.num("payload-bytes", 1024);
    let tail_file = args.get("tail-file", "");
    let out = args.get("out", "");

    let exporter = build_exporter(&transport, &endpoint, 10_000)?;
    let bsp = BatchSpanProcessor::builder(exporter)
        .with_batch_config(
            BatchConfigBuilder::default()
                .with_max_queue_size(queue)
                .with_max_export_batch_size(export_batch)
                .with_scheduled_delay(Duration::from_millis(delay_ms))
                .build(),
        )
        .build();
    let provider = SdkTracerProvider::builder()
        .with_span_processor(bsp)
        .with_resource(
            Resource::builder()
                .with_service_name("af-trace-poc")
                .build(),
        )
        .build();
    let tracer = provider.tracer("poc-otel");

    let end_map: Arc<Mutex<HashMap<String, u64>>> = Arc::new(Mutex::new(HashMap::new()));
    let (arrived_ctr, tail_handle) = if tail_file.is_empty() {
        let ctr = Arc::new(AtomicUsize::new(0));
        (ctr, None)
    } else {
        // 清空输出文件，tail 从头统计
        std::fs::File::create(&tail_file)?;
        let (ctr, h) = spawn_tail(tail_file.clone(), end_map.clone(), total);
        (ctr, Some(h))
    };

    let t0 = Instant::now();
    let interval = if rate > 0 {
        Duration::from_nanos(1_000_000_000 / rate)
    } else {
        Duration::ZERO
    };
    for i in 0..total {
        let mut span = tracer
            .span_builder("af-trace-event")
            .with_kind(SpanKind::Internal)
            .with_attributes(af_attributes(i as u64, payload_bytes))
            .start(&tracer);
        let sid = span.span_context().span_id().to_string();
        end_map.lock().unwrap().insert(sid, now_ms());
        span.end();
        if !interval.is_zero() {
            std::thread::sleep(interval);
        }
    }
    let sent_ms = t0.elapsed().as_millis() as u64;

    let ts0 = Instant::now();
    let shutdown_res = provider.shutdown();
    let shutdown_ms = ts0.elapsed().as_millis() as u64;
    let wall_ms = t0.elapsed().as_millis() as u64;

    // 给 tail 一点余量把 shutdown 后落盘的数据统计进来
    let wait_until = Instant::now() + Duration::from_millis(3000);
    while arrived_ctr.load(Ordering::Relaxed) < total && Instant::now() < wait_until {
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(300));
    let arrived = arrived_ctr.load(Ordering::Relaxed);
    let mut lat = serde_json::Value::Null;
    if let Some(h) = tail_handle {
        if h.is_finished() {
            let st = h.join().unwrap();
            lat = stats(&st.lat_ms);
        }
    }
    let res = serde_json::json!({
        "mode": "pipe", "transport": transport, "endpoint": endpoint,
        "total": total, "rate_per_s": rate, "queue": queue,
        "export_batch": export_batch, "delay_ms": delay_ms,
        "payload_bytes": payload_bytes,
        "send_ms": sent_ms, "shutdown_ms": shutdown_ms, "wall_ms": wall_ms,
        "arrived": arrived,
        "send_throughput_spans_per_s": if sent_ms > 0 { total as f64 / (sent_ms as f64/1000.0) } else { 0.0 },
        "e2e_throughput_spans_per_s": if wall_ms > 0 { total as f64 / (wall_ms as f64/1000.0) } else { 0.0 },
        "e2e_latency_ms": lat,
        "shutdown_ok": shutdown_res.is_ok(),
        "shutdown_err": shutdown_res.err().map(|e| e.to_string()),
    });
    println!("{}", serde_json::to_string_pretty(&res)?);
    write_json(&out, &res);
    Ok(())
}

// ---------- down：collector 故障行为 ----------

async fn cmd_down(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let transport = args.get("transport", "grpc");
    let endpoint = args.get("endpoint", "http://127.0.0.1:4317");
    let mode = args.get("mode", "recover");
    let baseline = args.num("baseline", 500) as usize;
    let burst = args.num("burst", 4000) as usize;
    let queue = args.num("queue", 8192) as usize;
    let export_batch = args.num("export-batch", 512) as usize;
    let delay_ms = args.num("delay-ms", 200);
    let payload_bytes = args.num("payload-bytes", 1024);
    let outage_secs = args.num("outage-secs", 10);
    let tail_file = args.get("tail-file", "");
    let collector_bin = args.get("collector-bin", "");
    let collector_cfg = args.get("collector-config", "");
    let out = args.get("out", "");

    let total = baseline + burst;
    let exporter = build_exporter(&transport, &endpoint, 10_000)?;
    let bsp = BatchSpanProcessor::builder(exporter)
        .with_batch_config(
            BatchConfigBuilder::default()
                .with_max_queue_size(queue)
                .with_max_export_batch_size(export_batch)
                .with_scheduled_delay(Duration::from_millis(delay_ms))
                .build(),
        )
        .build();
    let provider = SdkTracerProvider::builder()
        .with_span_processor(bsp)
        .with_resource(Resource::builder().with_service_name("af-trace-poc").build())
        .build();
    let tracer = provider.tracer("poc-otel");

    let end_map: Arc<Mutex<HashMap<String, u64>>> = Arc::new(Mutex::new(HashMap::new()));
    std::fs::File::create(&tail_file)?;
    let (arrived_ctr, _tail_handle) = spawn_tail(tail_file.clone(), end_map.clone(), total);

    // 把配置模板里的 __OUT__ 替换为实际输出文件后写到 run 目录
    let tmpl = std::fs::read_to_string(&collector_cfg)?;
    let inst_cfg = format!(
        "{}.down-{}.yaml",
        collector_cfg.trim_end_matches(".yaml"),
        std::process::id()
    );
    std::fs::write(
        &inst_cfg,
        tmpl.replace("__OUT__", std::path::Path::new(&tail_file).to_str().unwrap_or("out.json")),
    )?;
    let collector_cfg = inst_cfg;

    let mut child: Option<std::process::Child> = None;
    let collector_up = |up: bool,
                        child: &mut Option<std::process::Child>,
                        bin: &str,
                        cfg: &str|
     -> Result<(), Box<dyn std::error::Error>> {
        if up {
            let c = std::process::Command::new(bin)
                .args(["--config", cfg])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?;
            *child = Some(c);
            std::thread::sleep(Duration::from_millis(2000));
        } else if let Some(mut c) = child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        Ok(())
    };

    let send_spans = |from: usize, n: usize| {
        for i in from..from + n {
            let mut span = tracer
                .span_builder("af-trace-event")
                .with_kind(SpanKind::Internal)
                .with_attributes(af_attributes(i as u64, payload_bytes))
                .start(&tracer);
            let sid = span.span_context().span_id().to_string();
            end_map.lock().unwrap().insert(sid, now_ms());
            span.end();
        }
    };

    // 阶段 0：collector 在线，发 baseline
    collector_up(true, &mut child, &collector_bin, &collector_cfg)?;
    send_spans(0, baseline);
    std::thread::sleep(Duration::from_millis(2500));
    let after_baseline = arrived_ctr.load(Ordering::Relaxed);

    // 阶段 1：杀 collector，立即洪峰 burst
    collector_up(false, &mut child, &collector_bin, &collector_cfg)?;
    let t_kill = now_ms();
    send_spans(baseline, burst);
    let burst_ms = now_ms() - t_kill;
    let just_after_burst = arrived_ctr.load(Ordering::Relaxed);

    let shutdown_ms;
    let shutdown_ok;
    let shutdown_err;
    if mode == "recover" {
        // 阶段 2：停机窗口后再拉起，shutdown 冲刷
        std::thread::sleep(Duration::from_secs(outage_secs));
        collector_up(true, &mut child, &collector_bin, &collector_cfg)?;
        let ts0 = Instant::now();
        let r = provider.shutdown();
        shutdown_ms = ts0.elapsed().as_millis() as u64;
        shutdown_ok = r.is_ok();
        shutdown_err = r.err().map(|e| e.to_string());
        std::thread::sleep(Duration::from_secs(5));
    } else {
        // shutdown_down：collector 保持下线时直接 shutdown
        let ts0 = Instant::now();
        let r = provider.shutdown();
        shutdown_ms = ts0.elapsed().as_millis() as u64;
        shutdown_ok = r.is_ok();
        shutdown_err = r.err().map(|e| e.to_string());
        std::thread::sleep(Duration::from_secs(3));
    }

    let final_arrived = arrived_ctr.load(Ordering::Relaxed);
    if let Some(mut c) = child.take() {
        let _ = c.kill();
        let _ = c.wait();
    }

    let res = serde_json::json!({
        "mode": format!("down/{mode}"), "transport": transport, "endpoint": endpoint,
        "baseline": baseline, "burst": burst, "queue": queue,
        "export_batch": export_batch, "delay_ms": delay_ms,
        "outage_secs": outage_secs,
        "after_baseline": after_baseline,
        "just_after_burst": just_after_burst,
        "burst_send_ms": burst_ms,
        "shutdown_ms": shutdown_ms, "shutdown_ok": shutdown_ok, "shutdown_err": shutdown_err,
        "final_arrived": final_arrived,
        "lost": total.saturating_sub(final_arrived),
    });
    println!("{}", serde_json::to_string_pretty(&res)?);
    write_json(&out, &res);
    Ok(())
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let r = match args.get("_cmd", "").as_str() {
        "raw" => cmd_raw(args).await,
        "pipe" => cmd_pipe(args).await,
        "down" => cmd_down(args).await,
        c => Err(format!("unknown subcommand: {c}").into()),
    };
    if let Err(e) = r {
        eprintln!("probe error: {e}");
        std::process::exit(1);
    }
}
