//! af-meter 属性测试。语义以本 crate 的 RESULT.md、`budget.rs` 为准，不改已验证行为。
//!
//! 三组：
//! 1. 窗口计量在任意事件序列下总量守恒（含 Min 的两个窗口各扣同一份）。
//! 2. 预算扣减永不透支。成功入账的美分不为负；总额为负时余额的负数来自总额本身。
//! 3. 多消费者记账合并可交换。
//!
//! 生成器以小整数为主，边界值只放 `0` / `-1` / `i64::MIN` / `i64::MAX`。
//! 失败持久化关掉，反例只出现在测试输出里。时刻全部由用例注入。

use std::cmp::Reverse;

use af_meter::{BudgetError, BudgetLedger, Cents, Count, Meter, MeterError, Ticks, Time};
use proptest::prelude::*;
use proptest::test_runner::{FileFailurePersistence, TestCaseError};

fn proptest_cfg() -> ProptestConfig {
    ProptestConfig {
        cases: 256,
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        ..ProptestConfig::default()
    }
}

// --- 组 1：窗口总量守恒 -------------------------------------------------------

#[derive(Debug, Clone)]
struct WindowCase {
    window: Time,
    capacity: Count,
    origin: Time,
    events: Vec<WindowEvent>,
}

#[derive(Debug, Clone)]
enum WindowEvent {
    Observe { now: Time },
    Consume { now: Time, need: Count },
}

#[derive(Debug, Clone)]
struct MinCase {
    window_a: Time,
    capacity_a: Count,
    window_b: Time,
    capacity_b: Count,
    origin: Time,
    events: Vec<WindowEvent>,
}

#[derive(Clone, Copy, Debug)]
struct WindowView {
    window: Time,
    capacity: Count,
    remaining: Count,
    reset_at: Time,
}

/// 观察到的授予与丢弃。剩余和 `reset_at` 每次从计量器读，不在这里影子复制 while 循环。
#[derive(Clone, Debug)]
struct WindowOracle {
    window: Time,
    capacity: Count,
    granted_in_window: u128,
    total_granted: u128,
    discarded: u128,
    opened: u128,
}

fn arb_window_event() -> impl Strategy<Value = WindowEvent> {
    prop_oneof![
        (0u64..=48).prop_map(|now| WindowEvent::Observe { now }),
        (0u64..=48, 0u64..=12).prop_map(|(now, need)| WindowEvent::Consume { now, need }),
    ]
}

fn arb_window_case() -> impl Strategy<Value = WindowCase> {
    (
        0u64..=8,
        0u64..=12,
        0u64..=16,
        proptest::collection::vec(arb_window_event(), 0..=12),
    )
        .prop_map(|(window, capacity, origin, events)| WindowCase {
            window,
            capacity,
            origin,
            events,
        })
}

fn arb_min_case() -> impl Strategy<Value = MinCase> {
    (
        0u64..=8,
        0u64..=12,
        0u64..=8,
        0u64..=12,
        0u64..=16,
        proptest::collection::vec(arb_window_event(), 0..=12),
    )
        .prop_map(
            |(window_a, capacity_a, window_b, capacity_b, origin, events)| MinCase {
                window_a,
                capacity_a,
                window_b,
                capacity_b,
                origin,
                events,
            },
        )
}

fn place(step: usize, event: &WindowEvent) -> String {
    format!("step={step} event={event:?}")
}

fn must_window(meter: &Meter, at: &str) -> Result<WindowView, TestCaseError> {
    match meter {
        Meter::Window {
            window,
            capacity,
            remaining,
            reset_at,
        } => Ok(WindowView {
            window: *window,
            capacity: *capacity,
            remaining: *remaining,
            reset_at: *reset_at,
        }),
        other => Err(TestCaseError::fail(format!("不是窗口 {at}: {other:?}"))),
    }
}

fn must_min2(meter: &Meter, at: &str) -> Result<(WindowView, WindowView), TestCaseError> {
    match meter {
        Meter::Min(kids) if kids.len() == 2 => {
            Ok((must_window(&kids[0], at)?, must_window(&kids[1], at)?))
        }
        other => Err(TestCaseError::fail(format!(
            "不是两个窗口的 Min {at}: {other:?}"
        ))),
    }
}

/// 自 `reset_at` 到严格超过 `now` 要跨过的边界数。`window == 0` 或时刻未到边界时为 0。
/// 闭式：`floor((now - reset_at) / window) + 1`，不走实现里的 while。
fn crossings(reset_at: Time, window: Time, now: Time) -> Result<u64, TestCaseError> {
    if window == 0 || now < reset_at {
        return Ok(0);
    }
    let over = now - reset_at;
    over.checked_div(window)
        .and_then(|quot| quot.checked_add(1))
        .ok_or_else(|| {
            TestCaseError::fail(format!(
                "窗口边界计数溢出 reset_at={reset_at} window={window} now={now}"
            ))
        })
}

fn advance_reset(reset_at: Time, window: Time, crossings: u64) -> Result<Time, TestCaseError> {
    if crossings == 0 {
        return Ok(reset_at);
    }
    let step = window.checked_mul(crossings).ok_or_else(|| {
        TestCaseError::fail(format!(
            "reset_at 步进溢出 reset_at={reset_at} window={window} crossings={crossings}"
        ))
    })?;
    reset_at.checked_add(step).ok_or_else(|| {
        TestCaseError::fail(format!(
            "reset_at 溢出 reset_at={reset_at} step={step} crossings={crossings}"
        ))
    })
}

fn window_recovers_at(remaining: Count, capacity: Count, reset_at: Time) -> Option<Ticks> {
    if remaining < capacity {
        Some(Ticks(reset_at))
    } else {
        None
    }
}

fn min_recovers_at(children: &[(Count, Option<Ticks>)]) -> Option<Ticks> {
    let min = children
        .iter()
        .map(|(available, _)| *available)
        .min()
        .unwrap();
    let mut forever = false;
    let mut latest: Option<Ticks> = None;
    for (available, recovers_at) in children {
        if *available != min {
            continue;
        }
        match *recovers_at {
            None => forever = true,
            Some(t) => latest = Some(latest.map_or(t, |earliest| earliest.max(t))),
        }
    }
    if forever {
        None
    } else {
        latest
    }
}

fn assert_in_window(
    remaining: Count,
    granted_in_window: u128,
    capacity: Count,
    at: &str,
) -> Result<(), TestCaseError> {
    prop_assert_eq!(
        u128::from(remaining) + granted_in_window,
        u128::from(capacity),
        "窗口内不守恒（剩余+本窗已授予 != 容量） {} remaining={} granted_in_window={} capacity={}",
        at,
        remaining,
        granted_in_window,
        capacity
    );
    prop_assert!(
        remaining <= capacity,
        "剩余超过容量 {} remaining={} capacity={}",
        at,
        remaining,
        capacity
    );
    Ok(())
}

fn assert_global(oracle: &WindowOracle, remaining: Count, at: &str) -> Result<(), TestCaseError> {
    let left = oracle.total_granted + u128::from(remaining) + oracle.discarded;
    let right = u128::from(oracle.capacity) * oracle.opened;
    prop_assert_eq!(
        left,
        right,
        "总量不守恒（已授予+剩余+已丢弃 != 容量×开窗数） {} granted={} remaining={} discarded={} opened={} capacity={} window={}",
        at,
        oracle.total_granted,
        remaining,
        oracle.discarded,
        oracle.opened,
        oracle.capacity,
        oracle.window
    );
    Ok(())
}

fn oracle_for(view: WindowView) -> WindowOracle {
    WindowOracle {
        window: view.window,
        capacity: view.capacity,
        granted_in_window: 0,
        total_granted: 0,
        discarded: 0,
        opened: 1,
    }
}

/// 用闭式跨窗更新丢弃账。调用前本窗守恒必须已经成立。
fn close_windows(oracle: &mut WindowOracle, prev_remaining: Count, crossings: u64) {
    let extra_full = u128::from(crossings - 1);
    let unused = u128::from(prev_remaining) + u128::from(oracle.capacity) * extra_full;
    oracle.discarded += unused;
    oracle.opened += u128::from(crossings);
    oracle.granted_in_window = 0;
}

fn check_window_case(case: &WindowCase) -> Result<(), TestCaseError> {
    let mut meter = Meter::window(case.window, case.capacity, case.origin);
    let view = must_window(&meter, "init")?;
    prop_assert_eq!(
        view.reset_at,
        case.origin.saturating_add(case.window),
        "新建窗口 reset_at 与构造不一致 origin={} window={}",
        case.origin,
        case.window
    );
    prop_assert_eq!(view.remaining, case.capacity, "新建窗口剩余不是容量");
    let mut oracle = oracle_for(view);
    assert_in_window(
        view.remaining,
        oracle.granted_in_window,
        oracle.capacity,
        "init",
    )?;
    assert_global(&oracle, view.remaining, "init")?;
    for (step, event) in case.events.iter().enumerate() {
        apply_window(&mut meter, &mut oracle, event, &place(step, event))?;
    }
    Ok(())
}

fn apply_window(
    meter: &mut Meter,
    oracle: &mut WindowOracle,
    event: &WindowEvent,
    at: &str,
) -> Result<(), TestCaseError> {
    let prev = must_window(meter, at)?;
    prop_assert_eq!(prev.window, oracle.window, "window 被改写 {}", at);
    prop_assert_eq!(prev.capacity, oracle.capacity, "capacity 被改写 {}", at);
    let now = match event {
        WindowEvent::Observe { now } | WindowEvent::Consume { now, .. } => *now,
    };
    let k = crossings(prev.reset_at, oracle.window, now)?;
    let predicted_reset = advance_reset(prev.reset_at, oracle.window, k)?;
    assert_in_window(
        prev.remaining,
        oracle.granted_in_window,
        oracle.capacity,
        at,
    )?;
    if k >= 1 {
        close_windows(oracle, prev.remaining, k);
    }
    let refreshed = if k >= 1 {
        oracle.capacity
    } else {
        prev.remaining
    };
    let granted = drive_window(
        meter,
        event,
        refreshed,
        predicted_reset,
        oracle.capacity,
        at,
    )?;
    if let Some(need) = granted {
        oracle.granted_in_window += u128::from(need);
        oracle.total_granted += u128::from(need);
    }
    let after = must_window(meter, at)?;
    prop_assert!(
        after.reset_at >= prev.reset_at,
        "reset_at 倒退 {} prev={} after={}",
        at,
        prev.reset_at,
        after.reset_at
    );
    prop_assert_eq!(
        after.reset_at,
        predicted_reset,
        "reset_at 与闭式跨窗不一致 {} prev_reset={} now={} crossings={}",
        at,
        prev.reset_at,
        now,
        k
    );
    match granted {
        Some(need) => {
            prop_assert!(
                refreshed >= need,
                "接受了超出刷新后剩余的消耗 {} refreshed={} need={}",
                at,
                refreshed,
                need
            );
            prop_assert_eq!(
                after.remaining,
                refreshed - need,
                "扣减量不等于请求 {} refreshed={} need={} remaining={}",
                at,
                refreshed,
                need,
                after.remaining
            );
        }
        None => prop_assert_eq!(
            after.remaining,
            refreshed,
            "未授予时剩余偏离刷新后的值 {} refreshed={} remaining={}",
            at,
            refreshed,
            after.remaining
        ),
    }
    if k >= 1 {
        let refilled = match granted {
            Some(need) => after.remaining + need,
            None => after.remaining,
        };
        prop_assert_eq!(
            refilled,
            oracle.capacity,
            "跨过 {} 个窗口边界后回填 {}，不是容量 {}（错过的窗口不能累积） {}",
            k,
            refilled,
            oracle.capacity,
            at
        );
    }
    assert_in_window(
        after.remaining,
        oracle.granted_in_window,
        oracle.capacity,
        at,
    )?;
    assert_global(oracle, after.remaining, at)?;
    Ok(())
}

/// 驱动计量器。返回本次实际授予量；观察或拒绝时为 `None`。
fn drive_window(
    meter: &mut Meter,
    event: &WindowEvent,
    refreshed: Count,
    predicted_reset: Time,
    capacity: Count,
    at: &str,
) -> Result<Option<Count>, TestCaseError> {
    match event {
        WindowEvent::Observe { now } => {
            let reading = meter.given(*now);
            prop_assert_eq!(
                reading.available,
                refreshed,
                "观察读数与刷新后剩余不一致 {}",
                at
            );
            prop_assert_eq!(
                reading.recovers_at,
                window_recovers_at(refreshed, capacity, predicted_reset),
                "观察的 recovers_at 不一致 {}",
                at
            );
            Ok(None)
        }
        WindowEvent::Consume { now, need } => match meter.try_consume(*need, *now) {
            Ok(()) => Ok(Some(*need)),
            Err(MeterError::Insufficient {
                need: got,
                available,
                recovers_at,
            }) => {
                prop_assert_eq!(got, *need, "Insufficient.need 不一致 {}", at);
                prop_assert_eq!(
                    available,
                    refreshed,
                    "Insufficient.available 不是刷新后剩余 {}",
                    at
                );
                prop_assert!(
                    refreshed < *need,
                    "拒绝了刷新后仍然覆盖得了的消耗 {} refreshed={} need={}",
                    at,
                    refreshed,
                    need
                );
                prop_assert_eq!(
                    recovers_at,
                    window_recovers_at(refreshed, capacity, predicted_reset),
                    "Insufficient.recovers_at 不一致 {}",
                    at
                );
                Ok(None)
            }
            Err(MeterError::Unsupported(op)) => Err(TestCaseError::fail(format!(
                "窗口消耗返回 Unsupported({op}) {at}"
            ))),
        },
    }
}

fn check_min_case(case: &MinCase) -> Result<(), TestCaseError> {
    let mut meter = Meter::min(vec![
        Meter::window(case.window_a, case.capacity_a, case.origin),
        Meter::window(case.window_b, case.capacity_b, case.origin),
    ]);
    let (left, right) = must_min2(&meter, "init")?;
    prop_assert_eq!(left.remaining, case.capacity_a, "左窗初始剩余不是容量");
    prop_assert_eq!(right.remaining, case.capacity_b, "右窗初始剩余不是容量");
    let mut oracle_a = oracle_for(left);
    let mut oracle_b = oracle_for(right);
    assert_global(&oracle_a, left.remaining, "init-a")?;
    assert_global(&oracle_b, right.remaining, "init-b")?;
    for (step, event) in case.events.iter().enumerate() {
        apply_min(
            &mut meter,
            &mut oracle_a,
            &mut oracle_b,
            event,
            &place(step, event),
        )?;
    }
    Ok(())
}

fn apply_min(
    meter: &mut Meter,
    oracle_a: &mut WindowOracle,
    oracle_b: &mut WindowOracle,
    event: &WindowEvent,
    at: &str,
) -> Result<(), TestCaseError> {
    let (prev_a, prev_b) = must_min2(meter, at)?;
    let now = match event {
        WindowEvent::Observe { now } | WindowEvent::Consume { now, .. } => *now,
    };
    let k_a = crossings(prev_a.reset_at, oracle_a.window, now)?;
    let k_b = crossings(prev_b.reset_at, oracle_b.window, now)?;
    let reset_a = advance_reset(prev_a.reset_at, oracle_a.window, k_a)?;
    let reset_b = advance_reset(prev_b.reset_at, oracle_b.window, k_b)?;
    assert_in_window(
        prev_a.remaining,
        oracle_a.granted_in_window,
        oracle_a.capacity,
        at,
    )?;
    assert_in_window(
        prev_b.remaining,
        oracle_b.granted_in_window,
        oracle_b.capacity,
        at,
    )?;
    if k_a >= 1 {
        close_windows(oracle_a, prev_a.remaining, k_a);
    }
    if k_b >= 1 {
        close_windows(oracle_b, prev_b.remaining, k_b);
    }
    let refreshed_a = if k_a >= 1 {
        oracle_a.capacity
    } else {
        prev_a.remaining
    };
    let refreshed_b = if k_b >= 1 {
        oracle_b.capacity
    } else {
        prev_b.remaining
    };
    let avail = refreshed_a.min(refreshed_b);
    let recovers = min_recovers_at(&[
        (
            refreshed_a,
            window_recovers_at(refreshed_a, oracle_a.capacity, reset_a),
        ),
        (
            refreshed_b,
            window_recovers_at(refreshed_b, oracle_b.capacity, reset_b),
        ),
    ]);
    let granted = match event {
        WindowEvent::Observe { now } => {
            let reading = meter.given(*now);
            prop_assert_eq!(
                reading.available,
                avail,
                "Min 观察可用量不是两个窗口剩余的较小值 {} a={} b={}",
                at,
                refreshed_a,
                refreshed_b
            );
            prop_assert_eq!(
                reading.recovers_at,
                recovers,
                "Min 观察 recovers_at 不一致 {}",
                at
            );
            None
        }
        WindowEvent::Consume { now, need } => match meter.try_consume(*need, *now) {
            Ok(()) => {
                prop_assert!(
                    avail >= *need,
                    "Min 接受了超出较小剩余的消耗 {} avail={} need={}",
                    at,
                    avail,
                    need
                );
                Some(*need)
            }
            Err(MeterError::Insufficient {
                need: got,
                available,
                recovers_at,
            }) => {
                prop_assert_eq!(got, *need, "Min Insufficient.need 不一致 {}", at);
                prop_assert_eq!(available, avail, "Min Insufficient.available 不一致 {}", at);
                prop_assert!(
                    avail < *need,
                    "Min 拒绝了两个窗口都覆盖得了的消耗 {} avail={} need={}",
                    at,
                    avail,
                    need
                );
                prop_assert_eq!(
                    recovers_at,
                    recovers,
                    "Min Insufficient.recovers_at 不一致 {}",
                    at
                );
                None
            }
            Err(MeterError::Unsupported(op)) => {
                return Err(TestCaseError::fail(format!(
                    "Min 窗口消耗返回 Unsupported({op}) {at}"
                )));
            }
        },
    };
    if let Some(need) = granted {
        for oracle in [&mut *oracle_a, &mut *oracle_b] {
            oracle.granted_in_window += u128::from(need);
            oracle.total_granted += u128::from(need);
        }
    }
    let (after_a, after_b) = must_min2(meter, at)?;
    prop_assert_eq!(
        oracle_a.total_granted,
        oracle_b.total_granted,
        "Min 两边授予总量不一致 {}",
        at
    );
    match granted {
        Some(need) => {
            prop_assert_eq!(
                after_a.remaining,
                refreshed_a - need,
                "Min 左窗扣减不等于请求 {} refreshed={} need={}",
                at,
                refreshed_a,
                need
            );
            prop_assert_eq!(
                after_b.remaining,
                refreshed_b - need,
                "Min 右窗扣减不等于请求 {} refreshed={} need={}",
                at,
                refreshed_b,
                need
            );
        }
        None => {
            prop_assert_eq!(
                after_a.remaining,
                refreshed_a,
                "Min 未授予时左窗剩余变了 {}",
                at
            );
            prop_assert_eq!(
                after_b.remaining,
                refreshed_b,
                "Min 未授予时右窗剩余变了 {}",
                at
            );
        }
    }
    prop_assert_eq!(
        after_a.reset_at,
        reset_a,
        "Min 左窗 reset_at 与闭式不一致 {}",
        at
    );
    prop_assert_eq!(
        after_b.reset_at,
        reset_b,
        "Min 右窗 reset_at 与闭式不一致 {}",
        at
    );
    prop_assert!(
        after_a.reset_at >= prev_a.reset_at,
        "Min 左窗 reset_at 倒退 {}",
        at
    );
    prop_assert!(
        after_b.reset_at >= prev_b.reset_at,
        "Min 右窗 reset_at 倒退 {}",
        at
    );
    if k_a >= 1 {
        let refilled = match granted {
            Some(need) => after_a.remaining + need,
            None => after_a.remaining,
        };
        prop_assert_eq!(
            refilled,
            oracle_a.capacity,
            "Min 左窗跨过 {} 个边界后回填 {}，不是容量 {} {}",
            k_a,
            refilled,
            oracle_a.capacity,
            at
        );
    }
    if k_b >= 1 {
        let refilled = match granted {
            Some(need) => after_b.remaining + need,
            None => after_b.remaining,
        };
        prop_assert_eq!(
            refilled,
            oracle_b.capacity,
            "Min 右窗跨过 {} 个边界后回填 {}，不是容量 {} {}",
            k_b,
            refilled,
            oracle_b.capacity,
            at
        );
    }
    assert_in_window(
        after_a.remaining,
        oracle_a.granted_in_window,
        oracle_a.capacity,
        at,
    )?;
    assert_in_window(
        after_b.remaining,
        oracle_b.granted_in_window,
        oracle_b.capacity,
        at,
    )?;
    assert_global(oracle_a, after_a.remaining, at)?;
    assert_global(oracle_b, after_b.remaining, at)?;
    Ok(())
}

// --- 组 2：预算扣减永不透支 ---------------------------------------------------

#[derive(Debug, Clone)]
struct SpendOp {
    spend_cents: Cents,
    spend_cap_cents: Cents,
}

#[derive(Debug, Clone)]
struct BudgetCase {
    budget_cents: Cents,
    ops: Vec<SpendOp>,
}

fn arb_cents() -> impl Strategy<Value = Cents> {
    prop_oneof![
        8 => -12i64..=24,
        1 => Just(0),
        1 => Just(-1),
        1 => Just(i64::MIN),
        1 => Just(i64::MAX),
    ]
}

fn arb_spend_op() -> impl Strategy<Value = SpendOp> {
    (arb_cents(), arb_cents()).prop_map(|(spend_cents, spend_cap_cents)| SpendOp {
        spend_cents,
        spend_cap_cents,
    })
}

fn arb_budget_case() -> impl Strategy<Value = BudgetCase> {
    (
        arb_cents(),
        proptest::collection::vec(arb_spend_op(), 0..=8),
    )
        .prop_map(|(budget_cents, ops)| BudgetCase { budget_cents, ops })
}

fn assert_not_overdrawn(ledger: &BudgetLedger, at: &str) -> Result<(), TestCaseError> {
    prop_assert!(
        ledger.spent_cents >= 0,
        "已花为负 {} ledger={:?}",
        at,
        ledger
    );
    prop_assert_eq!(
        ledger.remaining_cents(),
        ledger.budget_cents.saturating_sub(ledger.spent_cents),
        "余额偏离 saturating_sub {} ledger={:?}",
        at,
        ledger
    );
    if ledger.budget_cents >= 0 {
        prop_assert!(
            ledger.spent_cents <= ledger.budget_cents,
            "扣减透支 {} ledger={:?}",
            at,
            ledger
        );
        prop_assert!(
            ledger.remaining_cents() >= 0,
            "非负总额下余额为负 {} ledger={:?}",
            at,
            ledger
        );
        prop_assert_eq!(
            ledger.remaining_cents() + ledger.spent_cents,
            ledger.budget_cents,
            "余额+已花不等于总额 {} ledger={:?}",
            at,
            ledger
        );
    } else {
        // 负总额是签发值。没有任何成功扣减，负数余额不是花出来的。
        prop_assert_eq!(
            ledger.spent_cents,
            0,
            "负总额仍发生了扣减 {} ledger={:?}",
            at,
            ledger
        );
        prop_assert_eq!(
            ledger.remaining_cents(),
            ledger.budget_cents,
            "负总额的余额应等于总额本身 {} ledger={:?}",
            at,
            ledger
        );
    }
    Ok(())
}

fn check_budget(case: &BudgetCase) -> Result<(), TestCaseError> {
    let mut ledger = BudgetLedger::new(case.budget_cents);
    assert_not_overdrawn(&ledger, "init")?;
    let mut accepted: i128 = 0;
    for (step, op) in case.ops.iter().enumerate() {
        let at = format!("step={step} op={op:?}");
        let before = ledger.clone();
        match ledger.try_spend(op.spend_cents, op.spend_cap_cents) {
            Ok(()) => {
                prop_assert!(
                    op.spend_cents >= 0,
                    "负数消耗被记入 {} before={:?}",
                    at,
                    before
                );
                prop_assert!(
                    op.spend_cents <= op.spend_cap_cents,
                    "超过 per-call cap 仍被接受 {}",
                    at
                );
                let Some(next) = before.spent_cents.checked_add(op.spend_cents) else {
                    return Err(TestCaseError::fail(format!(
                        "成功路径加法溢出 {at} before={before:?}"
                    )));
                };
                prop_assert_eq!(
                    ledger.spent_cents,
                    next,
                    "已花增量不等于本次消耗 {} before={:?}",
                    at,
                    before
                );
                prop_assert!(
                    ledger.spent_cents <= ledger.budget_cents,
                    "扣减后透支 {} ledger={:?}",
                    at,
                    ledger
                );
                prop_assert_eq!(
                    ledger.budget_cents,
                    before.budget_cents,
                    "扣减改写了总额 {}",
                    at
                );
                accepted += i128::from(op.spend_cents);
            }
            Err(err) => {
                prop_assert!(
                    ledger.budget_cents == before.budget_cents
                        && ledger.spent_cents == before.spent_cents,
                    "拒绝后账本被改写 {} before={:?} after={:?} err={:?}",
                    at,
                    before,
                    ledger,
                    err
                );
                expect_spend_rejection(&before, op, err, &at)?;
            }
        }
        prop_assert_eq!(
            i128::from(ledger.spent_cents),
            accepted,
            "已花不等于成功消耗之和 {} accepted={}",
            at,
            accepted
        );
        assert_not_overdrawn(&ledger, &at)?;
    }
    Ok(())
}

fn expect_spend_rejection(
    before: &BudgetLedger,
    op: &SpendOp,
    err: BudgetError,
    at: &str,
) -> Result<(), TestCaseError> {
    if op.spend_cents < 0 {
        prop_assert_eq!(
            err,
            BudgetError::NegativeSpend {
                spend_cents: op.spend_cents,
            },
            "负数消耗应报 NegativeSpend {}",
            at
        );
        return Ok(());
    }
    if op.spend_cents > op.spend_cap_cents {
        prop_assert_eq!(
            err,
            BudgetError::OverCap {
                spend_cents: op.spend_cents,
                spend_cap_cents: op.spend_cap_cents,
            },
            "超过 cap 应报 OverCap {}",
            at
        );
        return Ok(());
    }
    let overflows_or_exceeds = match before.spent_cents.checked_add(op.spend_cents) {
        None => true,
        Some(next) => next > before.budget_cents,
    };
    prop_assert!(
        overflows_or_exceeds,
        "拒绝了本可入账的消耗 {} before={:?} err={:?}",
        at,
        before,
        err
    );
    prop_assert_eq!(
        err,
        BudgetError::OverBudget {
            spend_cents: op.spend_cents,
            remaining_cents: before.remaining_cents(),
        },
        "超预算应报 OverBudget {}",
        at
    );
    Ok(())
}

// --- 组 3：多消费者记账合并可交换 --------------------------------------------

#[derive(Debug, Clone)]
struct ConsumerRun {
    budget_cents: Cents,
    ops: Vec<SpendOp>,
}

#[derive(Debug, Clone)]
struct RawLedger {
    budget_cents: Cents,
    spent_cents: Cents,
}

#[derive(Debug, Clone)]
enum MergeInput {
    /// 同一总额，各自 `try_spend`。和可以超过总额。
    Shared {
        budget_cents: Cents,
        consumers: Vec<Vec<SpendOp>>,
    },
    /// 总额各自独立。
    Separate(Vec<ConsumerRun>),
    /// 直接写字段，覆盖负数已花。
    Raw(Vec<RawLedger>),
}

fn arb_merge_input() -> impl Strategy<Value = MergeInput> {
    prop_oneof![
        4 => (
            arb_cents(),
            proptest::collection::vec(proptest::collection::vec(arb_spend_op(), 0..=5), 0..=4),
        )
            .prop_map(|(budget_cents, consumers)| MergeInput::Shared {
                budget_cents,
                consumers,
            }),
        2 => proptest::collection::vec(
            (arb_cents(), proptest::collection::vec(arb_spend_op(), 0..=4)).prop_map(
                |(budget_cents, ops)| ConsumerRun { budget_cents, ops },
            ),
            0..=4,
        )
        .prop_map(MergeInput::Separate),
        2 => proptest::collection::vec(
            (arb_cents(), arb_cents()).prop_map(|(budget_cents, spent_cents)| RawLedger {
                budget_cents,
                spent_cents,
            }),
            0..=4,
        )
        .prop_map(MergeInput::Raw),
    ]
}

fn realize(input: &MergeInput) -> Vec<BudgetLedger> {
    match input {
        MergeInput::Shared {
            budget_cents,
            consumers,
        } => consumers
            .iter()
            .map(|ops| run_consumer(*budget_cents, ops))
            .collect(),
        MergeInput::Separate(consumers) => consumers
            .iter()
            .map(|consumer| run_consumer(consumer.budget_cents, &consumer.ops))
            .collect(),
        MergeInput::Raw(raw) => raw
            .iter()
            .map(|row| BudgetLedger {
                budget_cents: row.budget_cents,
                spent_cents: row.spent_cents,
            })
            .collect(),
    }
}

fn run_consumer(budget_cents: Cents, ops: &[SpendOp]) -> BudgetLedger {
    let mut ledger = BudgetLedger::new(budget_cents);
    for op in ops {
        let _ = ledger.try_spend(op.spend_cents, op.spend_cap_cents);
    }
    ledger
}

fn reorderings(ledgers: &[BudgetLedger]) -> Vec<(&'static str, Vec<BudgetLedger>)> {
    let mut out = vec![("identity", ledgers.to_vec())];
    let mut reversed = ledgers.to_vec();
    reversed.reverse();
    out.push(("reverse", reversed));
    if !ledgers.is_empty() {
        let mut rotated = ledgers.to_vec();
        rotated.rotate_left(1);
        out.push(("rotate-left-1", rotated));
        let mut half = ledgers.to_vec();
        half.rotate_left(ledgers.len() / 2);
        out.push(("rotate-half", half));
    }
    let mut by_spent = ledgers.to_vec();
    by_spent.sort_by_key(|ledger| (ledger.spent_cents, ledger.budget_cents));
    out.push(("sort-by-spent", by_spent));
    let mut by_spent_desc = ledgers.to_vec();
    by_spent_desc.sort_by_key(|ledger| Reverse((ledger.spent_cents, ledger.budget_cents)));
    out.push(("sort-by-spent-desc", by_spent_desc));
    out
}

/// 与实现独立的合并规格。顺序：总额是否一致，再看负数已花，再看和是否装得下。
fn spec_merge(ledgers: &[BudgetLedger]) -> Result<BudgetLedger, BudgetError> {
    let Some(first) = ledgers.first() else {
        return Ok(BudgetLedger::new(0));
    };
    let low = ledgers
        .iter()
        .map(|ledger| ledger.budget_cents)
        .min()
        .unwrap();
    let high = ledgers
        .iter()
        .map(|ledger| ledger.budget_cents)
        .max()
        .unwrap();
    if low != high {
        return Err(BudgetError::BudgetMismatch { low, high });
    }
    let min_spent = ledgers
        .iter()
        .map(|ledger| ledger.spent_cents)
        .min()
        .unwrap();
    if min_spent < 0 {
        return Err(BudgetError::NegativeSpend {
            spend_cents: min_spent,
        });
    }
    let sum: i128 = ledgers
        .iter()
        .map(|ledger| i128::from(ledger.spent_cents))
        .sum();
    if sum != 0 && sum > i128::from(first.budget_cents) {
        return Err(BudgetError::OverBudget {
            spend_cents: i64::try_from(sum).unwrap_or(i64::MAX),
            remaining_cents: first.budget_cents,
        });
    }
    let spent_cents = i64::try_from(sum).unwrap_or(i64::MAX);
    Ok(BudgetLedger {
        budget_cents: first.budget_cents,
        spent_cents,
    })
}

fn check_merge(input: &MergeInput) -> Result<(), TestCaseError> {
    let ledgers = realize(input);
    if !matches!(input, MergeInput::Raw(_)) {
        for (index, ledger) in ledgers.iter().enumerate() {
            let at = format!("consumer={index}");
            assert_not_overdrawn(ledger, &at)?;
        }
    }
    let expected = spec_merge(&ledgers);
    for (name, perm) in reorderings(&ledgers) {
        let got = BudgetLedger::merge_consumers(&perm);
        prop_assert_eq!(
            got,
            expected.clone(),
            "合并随顺序变化 order={} input={:?} ledgers={:?}",
            name,
            input,
            ledgers
        );
    }
    match &expected {
        Ok(merged) => {
            let sum: i128 = ledgers
                .iter()
                .map(|ledger| i128::from(ledger.spent_cents))
                .sum();
            prop_assert_eq!(
                i128::from(merged.spent_cents),
                sum,
                "合并已花不是各消费者之和 ledgers={:?}",
                ledgers
            );
            prop_assert!(
                merged.spent_cents >= 0,
                "合并已花为负 ledgers={:?}",
                ledgers
            );
            prop_assert!(
                sum == 0 || sum <= i128::from(merged.budget_cents),
                "合并装进了超过总额的已花 sum={} ledger={:?}",
                sum,
                merged
            );
            assert_not_overdrawn(merged, "merged")?;
            prop_assert_eq!(
                BudgetLedger::merge_consumers(std::slice::from_ref(merged)),
                Ok(merged.clone()),
                "单个已合并账本再合并不是恒等 merged={:?}",
                merged
            );
        }
        Err(BudgetError::OverBudget {
            spend_cents,
            remaining_cents,
        }) => {
            let sum: i128 = ledgers
                .iter()
                .map(|ledger| i128::from(ledger.spent_cents))
                .sum();
            prop_assert!(
                sum != 0 && sum > i128::from(*remaining_cents),
                "OverBudget 但和仍然装得下 sum={} remaining={} spend_cents={}",
                sum,
                remaining_cents,
                spend_cents
            );
            prop_assert_eq!(
                *spend_cents,
                i64::try_from(sum).unwrap_or(i64::MAX),
                "OverBudget.spend_cents 不是各消费者已花之和 sum={}",
                sum
            );
        }
        Err(BudgetError::BudgetMismatch { low, high }) => {
            let actual_low = ledgers
                .iter()
                .map(|ledger| ledger.budget_cents)
                .min()
                .unwrap();
            let actual_high = ledgers
                .iter()
                .map(|ledger| ledger.budget_cents)
                .max()
                .unwrap();
            prop_assert_eq!(*low, actual_low, "BudgetMismatch.low 不是最小总额");
            prop_assert_eq!(*high, actual_high, "BudgetMismatch.high 不是最大总额");
            prop_assert!(low < high, "总额其实一致 low={} high={}", low, high);
        }
        Err(BudgetError::NegativeSpend { spend_cents }) => {
            let most_negative = ledgers
                .iter()
                .map(|ledger| ledger.spent_cents)
                .min()
                .unwrap();
            prop_assert_eq!(
                *spend_cents,
                most_negative,
                "NegativeSpend 不是最小的负数已花"
            );
            prop_assert!(*spend_cents < 0, "NegativeSpend 的金额不是负数");
        }
        Err(BudgetError::OverCap { .. }) => {
            return Err(TestCaseError::fail(
                "合并不会因 per-call cap 失败".to_string(),
            ));
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(proptest_cfg())]

    /// 组 1：任意时刻、任意消耗序列下，窗口配额守恒。
    /// 已授予 + 剩余 + 已丢弃 = 容量 × 开窗数。错过的窗口不累积。时钟可以倒退。
    #[test]
    fn window_quota_conserved_over_any_event_sequence(case in arb_window_case()) {
        check_window_case(&case)?;
    }

    /// 组 1：Min(窗口, 窗口) 的每个子窗口各自守恒，成功时两边扣同一份。
    #[test]
    fn min_windows_conserve_and_grant_the_same_amount(case in arb_min_case()) {
        check_min_case(&case)?;
    }

    /// 组 2：任意扣减序列都不透支。拒绝不改账本。负数消耗永不入账。
    /// 总额为负时余额可以为负，那是总额本身，不是扣出来的。
    #[test]
    fn budget_spend_never_overdrafts(case in arb_budget_case()) {
        check_budget(&case)?;
    }

    /// 组 3：多个消费者的账本合并与顺序无关，且不会合并出透支。
    #[test]
    fn consumer_ledger_merge_is_commutative(input in arb_merge_input()) {
        check_merge(&input)?;
    }
}
