//! af-trace 路径基准。只由 `cargo bench` 执行，不进入 `cargo test`。
//!
//! 计时窗口只包住被测路径；`Trace::spawn` 与 `shutdown` 在窗外。
//! 洪峰背压不使用 sleep：`hold_consumers` 挡住落盘线程，有界队列保持满，
//! 后续 `write_normal` 走丢弃并记缺口的路径。
//! 事件夹具的 `ts_ms` 是注入值；基准计时使用 `Instant`。

use af_trace::{GapInfo, Level, Trace, TraceConfig, TraceEvent};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use serde_json::{json, Value};
use std::hint::black_box;
use std::time::{Duration, Instant};

/// 小于 normal 容量，队列装得下整批，测的是 try_send 成功的入队路径。
const ENQUEUE_BATCH: usize = 1024;
const ENQUEUE_CAPACITY: usize = 8192;
/// 与洪峰集成测试相同的队列容量：normal 512、critical 8192。
const FLOOD_EVENTS: usize = 100_000;
const FLOOD_NORMAL_CAPACITY: usize = 512;
const FLOOD_CRITICAL_CAPACITY: usize = 8192;
/// 落盘线程在阻塞于 sink 锁之前最多取走一条；再留一条余量。
const MAX_DRAIN_RACE: u64 = 2;
/// 连续这么多次丢弃，说明队列已满且没有被排空。
const SATURATED_STREAK: usize = 32;
/// 缺口扫描的事件条数，与洪峰规模同量级。
const SINK_EVENTS: usize = 100_000;

fn elements(n: usize) -> u64 {
    u64::try_from(n).expect("event count fits u64")
}

fn steady_cfg() -> TraceConfig {
    TraceConfig {
        critical_capacity: 1024,
        normal_capacity: ENQUEUE_CAPACITY,
        critical_consume_delay_us: 0,
        normal_consume_delay_us: 0,
    }
}

fn flood_cfg() -> TraceConfig {
    TraceConfig {
        critical_capacity: FLOOD_CRITICAL_CAPACITY,
        normal_capacity: FLOOD_NORMAL_CAPACITY,
        critical_consume_delay_us: 0,
        normal_consume_delay_us: 0,
    }
}

fn steady_enqueue(c: &mut Criterion) {
    const {
        assert!(
            ENQUEUE_BATCH < ENQUEUE_CAPACITY,
            "常载批次必须小于队列容量，否则测到的不是入队路径"
        );
    }
    let mut group = c.benchmark_group("steady_enqueue");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(elements(ENQUEUE_BATCH)));
    group.bench_function("write_normal_under_capacity", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let trace = Trace::spawn(steady_cfg());
                let payload = json!({ "i": 0 });
                let start = Instant::now();
                for _ in 0..ENQUEUE_BATCH {
                    trace.write_normal("agent.step", payload.clone());
                }
                let gap = trace.gap_counter();
                black_box(gap);
                total += start.elapsed();
                assert_eq!(gap, 0, "常载批次必须全部入队");
                trace.shutdown();
            }
            total
        });
    });
    group.finish();
}

fn flood_backpressure(c: &mut Criterion) {
    let flood = elements(FLOOD_EVENTS);
    let mut group = c.benchmark_group("flood_backpressure");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Elements(flood));
    group.bench_function("write_normal_100k_full_queue", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let trace = Trace::spawn(flood_cfg());
                let (elapsed, dropped) = time_flood(&trace);
                total += elapsed;
                let report = trace.shutdown();
                assert_eq!(
                    report.gap_reported, report.gap_counter,
                    "shutdown 补发后 reported 必须等于 counter"
                );
                assert!(
                    dropped + MAX_DRAIN_RACE >= flood,
                    "洪峰应走背压丢弃路径，实际丢弃 {dropped}"
                );
            }
            total
        });
    });
    group.finish();
}

/// 持有消费者锁把 normal 队列打满，再计时 10 万次 `write_normal`。
/// 锁在返回前 drop，调用方随后才能 `shutdown`。
fn time_flood(trace: &Trace) -> (Duration, u64) {
    let _hold = trace.hold_consumers();
    saturate_normal(trace, FLOOD_NORMAL_CAPACITY);
    let payload = json!({ "i": 0 });
    let before = trace.gap_counter();
    let start = Instant::now();
    for _ in 0..FLOOD_EVENTS {
        trace.write_normal("agent.step", payload.clone());
    }
    let dropped = trace.gap_counter() - before;
    black_box(dropped);
    (start.elapsed(), dropped)
}

fn saturate_normal(trace: &Trace, capacity: usize) {
    let payload = Value::Null;
    let mut streak = 0usize;
    let limit = capacity.saturating_mul(8).saturating_add(SATURATED_STREAK);
    for _ in 0..limit {
        let before = trace.gap_counter();
        trace.write_normal("saturate", payload.clone());
        if trace.gap_counter() > before {
            streak += 1;
            if streak >= SATURATED_STREAK {
                return;
            }
        } else {
            streak = 0;
        }
    }
    panic!("normal 队列未能打满，背压基准无法覆盖丢弃路径");
}

fn gap_count_scan(c: &mut Criterion) {
    let (events, expected) = gap_sink(SINK_EVENTS);
    assert_eq!(
        sum_gap_counts(&events),
        expected,
        "缺口扫描夹具：Σ(count) 必须等于注入的丢弃数"
    );
    let mut group = c.benchmark_group("gap_count_scan");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));
    group.throughput(Throughput::Elements(elements(SINK_EVENTS)));
    group.bench_function("sum_gap_span_counts", |b| {
        b.iter(|| black_box(sum_gap_counts(black_box(&events))));
    });
    group.finish();
}

/// 与集成测试同一条对账式：把 gap span 的 `count` 加总，而不是数 span 条数。
fn sum_gap_counts(events: &[TraceEvent]) -> u64 {
    events
        .iter()
        .filter_map(|event| event.gap.as_ref())
        .map(|gap| gap.count)
        .sum()
}

fn gap_sink(n: usize) -> (Vec<TraceEvent>, u64) {
    let mut events = Vec::with_capacity(n);
    let mut expected = 0u64;
    for i in 0..n {
        let seq = elements(i);
        let gap = if i % 5 == 0 {
            let count = (seq % 7) + 1;
            expected += count;
            Some(GapInfo {
                total_dropped: expected,
                count,
                first_seq: seq,
                last_seq: seq + count - 1,
            })
        } else {
            None
        };
        let is_gap = gap.is_some();
        events.push(TraceEvent {
            seq,
            level: if is_gap {
                Level::Critical
            } else {
                Level::Normal
            },
            kind: if is_gap {
                "gap".to_string()
            } else {
                "agent.step".to_string()
            },
            payload: Value::Null,
            // 注入的时刻，不读系统时钟。
            ts_ms: 1_700_000_000_000 + seq,
            gap,
        });
    }
    (events, expected)
}

criterion_group!(
    trace_benches,
    steady_enqueue,
    flood_backpressure,
    gap_count_scan
);
criterion_main!(trace_benches);
