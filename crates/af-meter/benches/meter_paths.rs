//! af-meter 路径基准。只由 `cargo bench` 执行，不进入 `cargo test`。
//!
//! 时钟一律是调用方注入的整数秒，不读墙钟，也不用 sleep。
//! 计时窗口不含夹具构造。
//!
//! - `per_event_accounting`：同一注入时刻上，对 `Min(周窗, 日窗)` 逐事件 `try_consume(1)`。
//!   批大小小于两个窗口的容量，且不跨 `reset_at`，测的是记账扣减。
//!   `Min` 对每个子计量各扣一份（已验证语义）。
//! - `window_roll`：先扣空当前日窗，再 `given(now)` 跨过大量错过的日窗与周窗。
//!   测的是 `refresh` 的滚转循环。滚完剩余量回到容量，不累积错过的窗口。

use af_meter::{Count, Meter, Reading, Time};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::hint::black_box;
use std::time::{Duration, Instant};

const DAY: Time = 86_400;
const WEEK: Time = 604_800;
/// 非零注入时刻。计量器不读系统时钟。
const T0: Time = 1_700_000_000;
const EVENTS: usize = 4_096;
const DAILY_CAP: Count = 8_192;
const WEEKLY_CAP: Count = 65_536;
/// 一次 `given` 跨过的日窗口数。周窗在同一次调用里按周界滚转。
const MISSED_DAYS: Time = 100_000;
const ROLL_NOW: Time = MISSED_DAYS * DAY;

const _: () = {
    assert!(WEEK == DAY * 7);
    assert!(MISSED_DAYS > 7, "滚转至少跨过一周");
    assert!(T0 > 0);
    assert!(DAILY_CAP > 4_096, "整批必须落在同一个日窗里");
    assert!(WEEKLY_CAP > 4_096, "整批必须落在同一个周窗里");
    assert!(EVENTS == 4_096);
};

fn elements(n: usize) -> u64 {
    u64::try_from(n).expect("count fits u64")
}

fn subscription(now: Time) -> Meter {
    Meter::min(vec![
        Meter::window(WEEK, WEEKLY_CAP, now),
        Meter::window(DAY, DAILY_CAP, now),
    ])
}

fn window_fields(meter: &Meter) -> (Count, Time) {
    match meter {
        Meter::Window {
            remaining,
            reset_at,
            ..
        } => (*remaining, *reset_at),
        _ => panic!("子计量应是窗口"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AccountSnap {
    available: Count,
    recovers_at: Option<Time>,
    weekly_remaining: Count,
    daily_remaining: Count,
}

fn account_snap(meter: &mut Meter) -> AccountSnap {
    let reading = meter.given(T0);
    let Meter::Min(kids) = meter else {
        panic!("订阅计量应是 Min(周窗, 日窗)");
    };
    let (weekly_remaining, _) = window_fields(&kids[0]);
    let (daily_remaining, _) = window_fields(&kids[1]);
    AccountSnap {
        available: reading.available,
        recovers_at: reading.recovers_at,
        weekly_remaining,
        daily_remaining,
    }
}

fn assert_accounted(snap: AccountSnap) {
    let spent = elements(EVENTS);
    assert_eq!(
        snap.available,
        DAILY_CAP - spent,
        "日窗剩余是可用量，且没有滚转"
    );
    assert_eq!(snap.recovers_at, Some(T0 + DAY));
    assert_eq!(snap.daily_remaining, DAILY_CAP - spent);
    // Min 扣每一个子计量，不是只扣被读出来的那个。
    assert_eq!(snap.weekly_remaining, WEEKLY_CAP - spent);
}

fn per_event_accounting(c: &mut Criterion) {
    let mut probe = subscription(T0);
    for _ in 0..EVENTS {
        probe
            .try_consume(1, T0)
            .expect("探针：窗口内配额覆盖本批事件");
    }
    assert_accounted(account_snap(&mut probe));

    let mut group = c.benchmark_group("per_event_accounting");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(elements(EVENTS)));
    group.bench_function("try_consume_in_open_window", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let mut meter = subscription(T0);
                let start = Instant::now();
                for _ in 0..EVENTS {
                    meter
                        .try_consume(black_box(1), black_box(T0))
                        .expect("窗口内配额覆盖本批事件");
                    black_box(&meter);
                }
                total += start.elapsed();
                assert_accounted(account_snap(&mut meter));
            }
            total
        });
    });
    group.finish();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RollSnap {
    available: Count,
    recovers_at: Option<Time>,
    weekly_remaining: Count,
    weekly_reset: Time,
    daily_remaining: Count,
    daily_reset: Time,
}

fn roll_snap(meter: &Meter, reading: Reading) -> RollSnap {
    let Meter::Min(kids) = meter else {
        panic!("订阅计量应是 Min(周窗, 日窗)");
    };
    let (weekly_remaining, weekly_reset) = window_fields(&kids[0]);
    let (daily_remaining, daily_reset) = window_fields(&kids[1]);
    RollSnap {
        available: reading.available,
        recovers_at: reading.recovers_at,
        weekly_remaining,
        weekly_reset,
        daily_remaining,
        daily_reset,
    }
}

/// 扣空创建于 0 的当前日窗。滚转必须把剩余量写回容量，而不是在满额上空转。
fn depleted_daily() -> Meter {
    let mut meter = subscription(0);
    meter.try_consume(DAILY_CAP, 0).expect("扣空当前日窗");
    meter
}

fn assert_roll_semantics(snap: RollSnap) {
    assert_eq!(
        snap.available, DAILY_CAP,
        "滚转后可用量回到日窗容量，错过的窗口不累积"
    );
    assert_eq!(snap.recovers_at, None, "窗口已满，没有下一次恢复");
    assert_eq!(snap.daily_remaining, DAILY_CAP);
    assert_eq!(snap.weekly_remaining, WEEKLY_CAP);
    assert_eq!(snap.daily_reset, ROLL_NOW + DAY, "日窗跨过每一个错过的日界");
    assert!(snap.weekly_reset > ROLL_NOW, "周窗滚过注入时刻");
    assert!(
        snap.weekly_reset - WEEK <= ROLL_NOW,
        "周窗停在注入时刻之后的下一周界"
    );
}

fn window_roll(c: &mut Criterion) {
    let mut probe = depleted_daily();
    let reading = probe.given(ROLL_NOW);
    let expected = roll_snap(&probe, reading);
    assert_roll_semantics(expected);

    let mut group = c.benchmark_group("window_roll");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(MISSED_DAYS));
    group.bench_function("given_across_missed_windows", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let mut meter = depleted_daily();
                let start = Instant::now();
                // black_box 挡住把滚转循环折成闭式的优化，循环次数仍是注入的 MISSED_DAYS。
                let reading = meter.given(black_box(ROLL_NOW));
                black_box(reading);
                total += start.elapsed();
                assert_eq!(roll_snap(&meter, reading), expected);
            }
            total
        });
    });
    group.finish();
}

criterion_group!(meter_benches, per_event_accounting, window_roll);
criterion_main!(meter_benches);
