//! af-release-gate 属性测试。语义以本 crate 的 RESULT.md 为准，只观察闸门、不改规则。
//!
//! 三组：
//! 1. CONWIP：任意操作序列下已准入 WIP 不超过上限，精确等号可以入队。
//! 2. 渐放：同一批次内探针计数只增不减且不超过封顶；晋升之后本批次不再回到 gated。
//! 3. 拒绝：失败的准入、完成、重复开启不改写已经提交的准入状态。
//!
//! `seed_wip` 按 RESULT 不走准入、不衡上限，不进入这三组的操作序列。
//! 夜间信用在 WIP 判定之前扣减（RESULT 反馈 4）。拒绝断言锁住准入提交，
//! 并按 canary → 夜间 → WIP 的顺序核对夜间计数。
//!
//! 时刻全部由用例注入。失败时 proptest 打印缩到最小的输入。不写回归文件。

use af_release_gate::{
    AdmitError, CanaryStatus, EvalReport, GateConfig, GateError, GateEvent, Item, MinuteOfDay,
    NightWindow, ReleaseGate,
};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

fn proptest_cfg() -> ProptestConfig {
    ProptestConfig {
        cases: 256,
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        ..ProptestConfig::default()
    }
}

#[derive(Clone, Copy, Debug)]
enum Pred {
    Never,
    Always,
    CleanAfter(u8),
    AnyAfter(u8),
}

#[derive(Clone, Copy, Debug)]
struct NightSpec {
    start: u16,
    end: u16,
    /// 放行比例 = eighths / 8，取值 1..=8，二进分数，f64 可精确表示。
    eighths: u8,
}

impl NightSpec {
    fn ratio(self) -> f64 {
        f64::from(self.eighths) / 8.0
    }

    fn window(self) -> NightWindow {
        NightWindow {
            start: u32::from(self.start),
            end: u32::from(self.end),
            ratio: self.ratio(),
        }
    }
}

#[derive(Clone, Debug)]
struct GateSpec {
    upper: u16,
    lower: u16,
    night: Option<NightSpec>,
}

impl GateSpec {
    fn config(&self) -> GateConfig {
        GateConfig {
            wip_upper_minutes: f64::from(self.upper),
            wip_lower_minutes: f64::from(self.lower),
            night_window: self.night.map(NightSpec::window),
        }
    }

    fn open(&self) -> ReleaseGate {
        ReleaseGate::new(self.config()).expect("generator only builds a valid gate")
    }
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Admit {
        config: &'static str,
        weight: u16,
        minute: u16,
    },
    Complete {
        known: bool,
        slot: u8,
        defects: u8,
        minute: u16,
    },
    BeginCanary {
        config: &'static str,
        max_probes: u8,
        pred: Pred,
    },
}

#[derive(Clone, Debug)]
struct Case {
    gate: GateSpec,
    anchor: u16,
    ops: Vec<Op>,
}

#[derive(Clone, Copy, Debug)]
struct Held {
    id: u64,
    config: &'static str,
    weight: u16,
}

fn arb_config() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("v1"), Just("v2")]
}

fn arb_pred() -> impl Strategy<Value = Pred> {
    prop_oneof![
        Just(Pred::Never),
        Just(Pred::Always),
        (1u8..=4).prop_map(Pred::CleanAfter),
        (1u8..=4).prop_map(Pred::AnyAfter),
    ]
}

fn arb_night() -> impl Strategy<Value = NightSpec> {
    (0u16..1440, 0u16..1440, 1u8..=8).prop_map(|(start, end, eighths)| NightSpec {
        start,
        end,
        eighths,
    })
}

fn arb_gate() -> impl Strategy<Value = GateSpec> {
    (1u16..=40)
        .prop_flat_map(|upper| (Just(upper), 0..upper, prop::option::of(arb_night())))
        .prop_map(|(upper, lower, night)| GateSpec {
            upper,
            lower,
            night,
        })
}

fn arb_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (arb_config(), 1u16..=50, 0u16..1440).prop_map(|(config, weight, minute)| {
            Op::Admit {
                config,
                weight,
                minute,
            }
        }),
        3 => (any::<bool>(), any::<u8>(), 0u8..=4, 0u16..1440).prop_map(
            |(known, slot, defects, minute)| Op::Complete {
                known,
                slot,
                defects,
                minute,
            }
        ),
        2 => (arb_config(), 0u8..=5, arb_pred()).prop_map(|(config, max_probes, pred)| {
            Op::BeginCanary {
                config,
                max_probes,
                pred,
            }
        }),
    ]
}

fn arb_case() -> impl Strategy<Value = Case> {
    (
        arb_gate(),
        0u16..1440,
        prop::collection::vec(arb_op(), 0..=28),
    )
        .prop_map(|(gate, anchor, ops)| Case { gate, anchor, ops })
}

fn item(id: u64, config: &str, weight: f64) -> Item {
    Item {
        id,
        config: config.to_string(),
        review_minutes: weight,
    }
}

fn predicate(pred: Pred) -> Box<dyn Fn(&EvalReport) -> bool + Send> {
    Box::new(move |report: &EvalReport| match pred {
        Pred::Never => false,
        Pred::Always => true,
        Pred::CleanAfter(n) => report.probes_completed >= usize::from(n) && report.defects == 0,
        Pred::AnyAfter(n) => report.probes_completed >= usize::from(n),
    })
}

fn held_minutes(held: &[Held]) -> f64 {
    held.iter().fold(0.0, |acc, h| acc + f64::from(h.weight))
}

fn plain_gate(upper: f64) -> ReleaseGate {
    ReleaseGate::new(GateConfig {
        wip_upper_minutes: upper,
        wip_lower_minutes: 0.0,
        night_window: None,
    })
    .expect("plain gate is valid")
}

fn count_promos(events: &[GateEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, GateEvent::CanaryPromoted { .. }))
        .count()
}

// --- 组 1：CONWIP 不超发 ----------------------------------------------------

fn assert_books(
    gate: &ReleaseGate,
    held: &[Held],
    upper: u16,
    step: usize,
) -> Result<(), TestCaseError> {
    let wip = gate.wip_minutes();
    let expect = held_minutes(held);
    prop_assert_eq!(
        wip.to_bits(),
        expect.to_bits(),
        "step={} wip={} held={}",
        step,
        wip,
        expect
    );
    prop_assert_eq!(gate.in_process_count(), held.len(), "step={}", step);
    prop_assert!(
        wip <= f64::from(upper) + 1e-9,
        "step={} over-release wip={} upper={}",
        step,
        wip,
        upper
    );
    Ok(())
}

/// 精确等号可入；再加 1 分钟被拒且 WIP 不动。超重件单独也进不了空闸。
fn script_exact_fit(upper: u16) -> Result<(), TestCaseError> {
    let mut heavy = plain_gate(f64::from(upper));
    let rejected = heavy.try_admit(&item(1, "v1", f64::from(upper) + 1.0), 0);
    prop_assert!(
        matches!(rejected, Err(AdmitError::WipCap { .. })),
        "overweight on an empty gate must be WipCap, got {:?}",
        rejected
    );
    prop_assert_eq!(heavy.wip_minutes().to_bits(), 0.0f64.to_bits());
    prop_assert_eq!(heavy.in_process_count(), 0);

    let mut gate = plain_gate(f64::from(upper));
    let fit = gate.try_admit(&item(1, "v1", f64::from(upper)), 0);
    prop_assert!(
        fit.is_ok(),
        "exact fit must admit, upper={} got {:?}",
        upper,
        fit.err()
    );
    prop_assert_eq!(gate.wip_minutes().to_bits(), f64::from(upper).to_bits());
    let past = gate.try_admit(&item(2, "v1", 1.0), 1);
    prop_assert!(
        matches!(past, Err(AdmitError::WipCap { .. })),
        "one minute past the cap must be WipCap, got {:?}",
        past
    );
    prop_assert_eq!(gate.wip_minutes().to_bits(), f64::from(upper).to_bits());
    prop_assert_eq!(gate.in_process_count(), 1);
    gate.complete(1, 2, 0).expect("admitted item completes");
    prop_assert_eq!(gate.wip_minutes().to_bits(), 0.0f64.to_bits());
    prop_assert_eq!(gate.in_process_count(), 0);

    if upper > 1 {
        gate.try_admit(&item(3, "v1", f64::from(upper - 1)), 3)
            .expect("upper-1 fits");
        gate.try_admit(&item(4, "v1", 1.0), 4)
            .expect("the last minute fits exactly");
        prop_assert_eq!(gate.wip_minutes().to_bits(), f64::from(upper).to_bits());
        let past = gate.try_admit(&item(5, "v1", 1.0), 5);
        prop_assert!(
            matches!(past, Err(AdmitError::WipCap { .. })),
            "split exact fit still caps the next item, got {:?}",
            past
        );
        prop_assert_eq!(gate.in_process_count(), 2);
        prop_assert_eq!(gate.wip_minutes().to_bits(), f64::from(upper).to_bits());
    }
    Ok(())
}

fn pick_id(held: &[Held], known: bool, slot: u8, next_id: &mut u64) -> u64 {
    if known && !held.is_empty() {
        held[usize::from(slot) % held.len()].id
    } else {
        let id = *next_id;
        *next_id += 1;
        id
    }
}

fn walk_conwip(case: &Case) -> Result<(), TestCaseError> {
    let mut gate = case.gate.open();
    let mut held: Vec<Held> = Vec::new();
    let mut next_id = 1u64;
    let upper = case.gate.upper;

    // 空闸上重量 1 一定放得下。夜间窗口的第一次请求吃初始信用 1.0，不会被限流。
    let anchor = item(next_id, "v1", 1.0);
    next_id += 1;
    let admitted = gate.try_admit(&anchor, u32::from(case.anchor));
    prop_assert!(
        admitted.is_ok(),
        "anchor weight 1 must fit, upper={} minute={} got {:?}",
        upper,
        case.anchor,
        admitted.err()
    );
    held.push(Held {
        id: anchor.id,
        config: "v1",
        weight: 1,
    });
    assert_books(&gate, &held, upper, 0)?;

    for (offset, op) in case.ops.iter().enumerate() {
        let step = offset + 1;
        match *op {
            Op::Admit {
                config,
                weight,
                minute,
            } => {
                let id = next_id;
                next_id += 1;
                let before = gate.wip_minutes();
                match gate.try_admit(&item(id, config, f64::from(weight)), u32::from(minute)) {
                    Ok(_) => {
                        let after = gate.wip_minutes();
                        prop_assert_eq!(
                            after.to_bits(),
                            (before + f64::from(weight)).to_bits(),
                            "step={} admit delta",
                            step
                        );
                        prop_assert!(
                            after <= f64::from(upper) + 1e-9,
                            "step={} admitted over the cap wip={} upper={}",
                            step,
                            after,
                            upper
                        );
                        held.push(Held { id, config, weight });
                    }
                    Err(_) => {
                        prop_assert_eq!(
                            gate.wip_minutes().to_bits(),
                            before.to_bits(),
                            "step={} rejection moved wip",
                            step
                        );
                    }
                }
            }
            Op::Complete {
                known,
                slot,
                defects,
                minute,
            } => {
                let id = pick_id(&held, known, slot, &mut next_id);
                let before = gate.wip_minutes();
                match gate.complete(id, u32::from(minute), u64::from(defects)) {
                    Ok(done) => {
                        let weight = done.ticket.weight;
                        prop_assert_eq!(
                            gate.wip_minutes().to_bits(),
                            (before - weight).to_bits(),
                            "step={} complete delta",
                            step
                        );
                        let idx = held.iter().position(|h| h.id == id);
                        prop_assert!(idx.is_some(), "step={} completed unknown id {}", step, id);
                        if let Some(idx) = idx {
                            held.remove(idx);
                        }
                    }
                    Err(GateError::UnknownItem(_)) => {
                        prop_assert_eq!(
                            gate.wip_minutes().to_bits(),
                            before.to_bits(),
                            "step={} unknown complete moved wip",
                            step
                        );
                    }
                    Err(err) => {
                        return Err(TestCaseError::fail(format!(
                            "step={step} unexpected complete error {err:?}"
                        )));
                    }
                }
            }
            Op::BeginCanary {
                config,
                max_probes,
                pred,
            } => {
                let before = gate.wip_minutes();
                let _ = gate.begin_canary(config, usize::from(max_probes), predicate(pred));
                prop_assert_eq!(
                    gate.wip_minutes().to_bits(),
                    before.to_bits(),
                    "step={} begin_canary moved wip",
                    step
                );
            }
        }
        assert_books(&gate, &held, upper, step)?;
    }
    Ok(())
}

// --- 组 2：渐放批次单调、晋升不回头 ----------------------------------------

#[derive(Clone, Debug)]
struct Batch {
    config: String,
    max_probes: usize,
    released: usize,
    open: bool,
}

fn note_batch(
    batch: &mut Option<Batch>,
    gate: &ReleaseGate,
    events: &[GateEvent],
    released_delta: usize,
    may_promote: bool,
    step: usize,
) -> Result<(), TestCaseError> {
    let Some(batch) = batch.as_mut() else {
        prop_assert!(
            gate.canary_status().is_none(),
            "step={} canary appeared without begin_canary",
            step
        );
        prop_assert_eq!(count_promos(events), 0, "step={}", step);
        return Ok(());
    };
    prop_assert!(
        gate.canary_status().is_some(),
        "step={} canary disappeared",
        step
    );
    let st = gate.canary_status().expect("checked above");
    prop_assert_eq!(st.config.as_str(), batch.config.as_str(), "step={}", step);
    prop_assert_eq!(st.max_probes, batch.max_probes, "step={}", step);
    prop_assert!(
        st.released <= st.max_probes,
        "step={} released={} max={}",
        step,
        st.released,
        st.max_probes
    );
    prop_assert_eq!(
        st.released,
        batch.released + released_delta,
        "step={} released moved by more than this op",
        step
    );
    if batch.open {
        prop_assert!(st.open, "step={} promoted batch reverted to gated", step);
        prop_assert_eq!(
            released_delta,
            0,
            "step={} released grew after promotion",
            step
        );
        prop_assert!(!may_promote, "step={} promotion re-armed", step);
    }
    let promos = count_promos(events);
    if st.open && !batch.open {
        prop_assert!(
            may_promote,
            "step={} open flipped outside a matching complete",
            step
        );
        prop_assert_eq!(
            released_delta,
            0,
            "step={} promotion changed released",
            step
        );
        prop_assert_eq!(promos, 1, "step={} promotion events", step);
        let event = events
            .iter()
            .find(|event| matches!(event, GateEvent::CanaryPromoted { .. }));
        prop_assert!(event.is_some(), "step={}", step);
        if let Some(GateEvent::CanaryPromoted {
            config,
            probes_released,
            ..
        }) = event
        {
            prop_assert_eq!(config, &batch.config, "step={}", step);
            prop_assert_eq!(*probes_released, st.released, "step={}", step);
        }
    } else {
        prop_assert_eq!(promos, 0, "step={}", step);
        if !may_promote {
            prop_assert_eq!(st.open, batch.open, "step={} open changed", step);
        }
    }
    batch.open = st.open;
    batch.released = st.released;
    Ok(())
}

fn script_promotion_does_not_revert() -> Result<(), TestCaseError> {
    let mut gate = plain_gate(1_000.0);
    gate.begin_canary("v2", 3, predicate(Pred::Always))
        .expect("first canary");
    for id in 1..=3u64 {
        gate.try_admit(&item(id, "v2", 1.0), 0).expect("probe fits");
        let st = gate.canary_status().expect("canary");
        prop_assert_eq!(st.released, usize::try_from(id).unwrap());
        prop_assert!(!st.open, "admit must not promote");
        prop_assert_eq!(st.max_probes, 3);
    }
    let capped = gate.try_admit(&item(4, "v2", 1.0), 0);
    prop_assert!(
        matches!(capped, Err(AdmitError::CanaryProbeCap { cap: 3, .. })),
        "4th probe while gated, got {:?}",
        capped
    );
    let st = gate.canary_status().expect("canary");
    prop_assert_eq!(st.released, 3);
    prop_assert!(!st.open);
    prop_assert_eq!(gate.wip_minutes().to_bits(), 3.0f64.to_bits());

    gate.complete(1, 1, 4).expect("probe completes");
    let st = gate.canary_status().expect("canary");
    prop_assert!(st.open, "Always promotes on the first probe completion");
    prop_assert_eq!(st.released, 3, "promotion freezes the probe count");
    let events = gate.drain_events();
    prop_assert_eq!(count_promos(&events), 1);
    let frozen = events.iter().any(|event| {
        matches!(
            event,
            GateEvent::CanaryPromoted {
                probes_released: 3,
                ..
            }
        )
    });
    prop_assert!(frozen, "promotion event keeps the gated probe count");

    gate.try_admit(&item(5, "v2", 1.0), 2)
        .expect("promoted config is uncapped");
    let st = gate.canary_status().expect("canary");
    prop_assert!(st.open);
    prop_assert_eq!(st.released, 3);
    gate.complete(5, 3, 9).expect("later complete");
    let st = gate.canary_status().expect("canary");
    prop_assert!(st.open, "defects after promotion do not re-gate");
    prop_assert_eq!(st.released, 3);
    prop_assert_eq!(count_promos(&gate.drain_events()), 0);

    gate.begin_canary("v2", 1, predicate(Pred::Always))
        .expect("a new batch starts only after promotion");
    let st = gate.canary_status().expect("canary");
    prop_assert!(!st.open);
    prop_assert_eq!(st.released, 0);
    prop_assert_eq!(st.max_probes, 1);
    let again = gate.begin_canary("v3", 2, predicate(Pred::Never));
    prop_assert!(
        matches!(again, Err(GateError::CanaryAlreadyActive(_))),
        "a gated batch cannot be replaced, got {:?}",
        again
    );
    let st = gate.canary_status().expect("canary");
    prop_assert_eq!(st.config.as_str(), "v2");
    prop_assert!(!st.open);
    prop_assert_eq!(st.released, 0);
    prop_assert_eq!(st.max_probes, 1);
    Ok(())
}

fn script_defects_keep_batch_gated() -> Result<(), TestCaseError> {
    let mut gate = plain_gate(1_000.0);
    gate.begin_canary("v2", 2, predicate(Pred::CleanAfter(1)))
        .expect("canary");
    gate.try_admit(&item(1, "v2", 1.0), 0).expect("probe");
    gate.try_admit(&item(2, "v2", 1.0), 0).expect("probe");
    gate.complete(1, 1, 1).expect("defective probe");
    let st = gate.canary_status().expect("canary");
    prop_assert!(!st.open);
    prop_assert_eq!(st.released, 2);
    gate.complete(2, 2, 0).expect("later clean probe");
    let st = gate.canary_status().expect("canary");
    prop_assert!(!st.open, "cumulative defects keep the batch gated");
    prop_assert_eq!(st.released, 2);
    let capped = gate.try_admit(&item(3, "v2", 1.0), 3);
    let probe_capped = matches!(capped, Err(AdmitError::CanaryProbeCap { cap: 2, .. }));
    prop_assert!(probe_capped, "probe cap stays at 2, got {:?}", capped);
    let st = gate.canary_status().expect("canary");
    prop_assert!(!st.open);
    prop_assert_eq!(st.released, 2);
    gate.try_admit(&item(4, "v1", 1.0), 3)
        .expect("other configs stay outside the cap");
    prop_assert_eq!(gate.canary_status().expect("canary").released, 2);
    prop_assert!(!gate.canary_status().expect("canary").open);
    Ok(())
}

fn walk_canary(case: &Case) -> Result<(), TestCaseError> {
    let mut gate = case.gate.open();
    let mut batch: Option<Batch> = None;
    let mut held: Vec<Held> = Vec::new();
    let mut next_id = 1u64;

    for (step, op) in case.ops.iter().enumerate() {
        match *op {
            Op::BeginCanary {
                config,
                max_probes,
                pred,
            } => {
                let before = gate.canary_status();
                match gate.begin_canary(config, usize::from(max_probes), predicate(pred)) {
                    Ok(()) => {
                        if let Some(prev) = &batch {
                            prop_assert!(
                                prev.open,
                                "step={} new batch started while {} was still gated",
                                step,
                                prev.config
                            );
                        }
                        batch = Some(Batch {
                            config: config.to_string(),
                            max_probes: usize::from(max_probes),
                            released: 0,
                            open: false,
                        });
                        let st = gate.canary_status().expect("begin installs a canary");
                        prop_assert_eq!(st.config.as_str(), config, "step={}", step);
                        prop_assert!(!st.open, "step={}", step);
                        prop_assert_eq!(st.released, 0, "step={}", step);
                        prop_assert_eq!(st.max_probes, usize::from(max_probes), "step={}", step);
                        prop_assert_eq!(count_promos(&gate.drain_events()), 0, "step={}", step);
                    }
                    Err(GateError::CanaryAlreadyActive(name)) => {
                        prop_assert!(
                            batch.as_ref().is_some_and(|b| !b.open && b.config == name),
                            "step={} unexpected CanaryAlreadyActive {}",
                            step,
                            name
                        );
                        prop_assert_eq!(gate.canary_status(), before, "step={}", step);
                        prop_assert_eq!(count_promos(&gate.drain_events()), 0, "step={}", step);
                    }
                    Err(err) => {
                        return Err(TestCaseError::fail(format!(
                            "step={step} unexpected begin error {err:?}"
                        )));
                    }
                }
            }
            Op::Admit {
                config,
                weight,
                minute,
            } => {
                let id = next_id;
                next_id += 1;
                let before = batch.clone();
                let result =
                    gate.try_admit(&item(id, config, f64::from(weight)), u32::from(minute));
                if let Err(AdmitError::CanaryProbeCap {
                    config: capped,
                    cap,
                }) = &result
                {
                    prop_assert!(
                        before.as_ref().is_some_and(|b| {
                            !b.open
                                && b.config == *capped
                                && b.max_probes == *cap
                                && b.config == config
                                && b.released >= b.max_probes
                        }),
                        "step={} CanaryProbeCap {:?} batch={:?}",
                        step,
                        result,
                        before
                    );
                }
                let events = gate.drain_events();
                let delta = match (&result, &before) {
                    (Ok(_), Some(b)) if !b.open && b.config == config => 1,
                    _ => 0,
                };
                note_batch(&mut batch, &gate, &events, delta, false, step)?;
                if result.is_ok() {
                    held.push(Held { id, config, weight });
                }
                prop_assert!(
                    batch.as_ref().is_none_or(|b| !b.open)
                        || !matches!(result, Err(AdmitError::CanaryProbeCap { .. })),
                    "step={} probe cap bound a promoted batch",
                    step
                );
            }
            Op::Complete {
                known,
                slot,
                defects,
                minute,
            } => {
                let id = pick_id(&held, known, slot, &mut next_id);
                let completed = held.iter().find(|h| h.id == id).copied();
                let may_promote = completed.is_some_and(|h| {
                    batch
                        .as_ref()
                        .is_some_and(|b| !b.open && b.config == h.config)
                });
                let result = gate.complete(id, u32::from(minute), u64::from(defects));
                let events = gate.drain_events();
                let completed_ok = result.is_ok();
                note_batch(
                    &mut batch,
                    &gate,
                    &events,
                    0,
                    may_promote && completed_ok,
                    step,
                )?;
                if completed_ok {
                    if let Some(pos) = held.iter().position(|h| h.id == id) {
                        held.remove(pos);
                    }
                }
            }
        }
    }
    Ok(())
}

// --- 组 3：拒绝不产生部分准入提交 ------------------------------------------

#[derive(Clone, Copy, Debug)]
struct NightLedger {
    credit: f64,
    requests: u64,
    admitted: u64,
    seen: Option<bool>,
}

impl NightLedger {
    fn fresh() -> Self {
        Self {
            credit: 1.0,
            requests: 0,
            admitted: 0,
            seen: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Admission {
    wip_bits: u64,
    count: usize,
    refill: bool,
    canary: Option<CanaryStatus>,
}

fn admission(gate: &ReleaseGate) -> Admission {
    Admission {
        wip_bits: gate.wip_minutes().to_bits(),
        count: gate.in_process_count(),
        refill: gate.refill_needed(),
        canary: gate.canary_status(),
    }
}

#[derive(Clone, Copy, Debug)]
enum ForecastKind {
    Admit,
    Canary,
    Night,
    Wip,
}

struct Forecast {
    kind: ForecastKind,
    ledger: NightLedger,
    /// `Some(true)` 进窗，`Some(false)` 出窗。首次观察不产生事件。
    window_event: Option<bool>,
}

fn forecast_admit(
    ledger: &NightLedger,
    gate: &ReleaseGate,
    item: &Item,
    now: MinuteOfDay,
) -> Forecast {
    let in_window = gate.in_night_window(now);
    let mut next = *ledger;
    let mut window_event = None;
    match ledger.seen {
        None => next.seen = Some(in_window),
        Some(prev) if prev != in_window => {
            next.seen = Some(in_window);
            next.credit = 1.0;
            next.requests = 0;
            next.admitted = 0;
            window_event = Some(in_window);
        }
        Some(_) => {}
    }
    if let Some(canary) = gate.canary_status() {
        if !canary.open && canary.config == item.config && canary.released >= canary.max_probes {
            return Forecast {
                kind: ForecastKind::Canary,
                ledger: next,
                window_event,
            };
        }
    }
    if in_window {
        let ratio = gate
            .config()
            .night_window
            .as_ref()
            .expect("in-window implies a night config")
            .ratio;
        next.requests += 1;
        next.credit += ratio;
        if next.credit >= 1.0 {
            next.credit -= 1.0;
            next.admitted += 1;
        } else {
            return Forecast {
                kind: ForecastKind::Night,
                ledger: next,
                window_event,
            };
        }
    }
    let upper = gate.config().wip_upper_minutes;
    if gate.wip_minutes() + item.review_minutes > upper + 1e-9 {
        return Forecast {
            kind: ForecastKind::Wip,
            ledger: next,
            window_event,
        };
    }
    Forecast {
        kind: ForecastKind::Admit,
        ledger: next,
        window_event,
    }
}

struct World {
    gate: ReleaseGate,
    ledger: NightLedger,
    held: Vec<Held>,
    next_id: u64,
}

impl World {
    fn open(spec: &GateSpec) -> Self {
        Self {
            gate: spec.open(),
            ledger: NightLedger::fresh(),
            held: Vec::new(),
            next_id: 1,
        }
    }

    fn check_night(&self, step: usize) -> Result<(), TestCaseError> {
        let (requests, admitted) = self.gate.night_stats();
        prop_assert_eq!(requests, self.ledger.requests, "step={} requests", step);
        prop_assert_eq!(admitted, self.ledger.admitted, "step={} admitted", step);
        Ok(())
    }

    fn check_books(&self, step: usize) -> Result<(), TestCaseError> {
        let wip = self.gate.wip_minutes();
        prop_assert_eq!(
            wip.to_bits(),
            held_minutes(&self.held).to_bits(),
            "step={} wip={} held={}",
            step,
            wip,
            held_minutes(&self.held)
        );
        prop_assert_eq!(
            self.gate.in_process_count(),
            self.held.len(),
            "step={}",
            step
        );
        prop_assert!(
            wip <= self.gate.config().wip_upper_minutes + 1e-9,
            "step={} wip={} upper={}",
            step,
            wip,
            self.gate.config().wip_upper_minutes
        );
        Ok(())
    }

    fn take_admit_events(
        &mut self,
        forecast: &Forecast,
        rejected: bool,
        now: MinuteOfDay,
        step: usize,
    ) -> Result<(), TestCaseError> {
        let mut events = self.gate.drain_events();
        if let Some(entering) = forecast.window_event {
            prop_assert!(!events.is_empty(), "step={} missing window event", step);
            let event = events.remove(0);
            if entering {
                prop_assert!(
                    matches!(event, GateEvent::NightWindowEnter { at } if at == now),
                    "step={} event={:?}",
                    step,
                    event
                );
            } else {
                prop_assert!(
                    matches!(event, GateEvent::NightWindowExit { at } if at == now),
                    "step={} event={:?}",
                    step,
                    event
                );
            }
        }
        if rejected {
            prop_assert!(
                events.is_empty(),
                "step={} rejection emitted {:?}",
                step,
                events
            );
        } else {
            prop_assert!(
                events
                    .iter()
                    .all(|event| matches!(event, GateEvent::RefillSatisfied { .. })),
                "step={} unexpected admit events {:?}",
                step,
                events
            );
            prop_assert!(events.len() <= 1, "step={} {:?}", step, events);
        }
        Ok(())
    }

    fn admit(
        &mut self,
        config: &'static str,
        weight: u16,
        minute: u16,
        step: usize,
    ) -> Result<ForecastKind, TestCaseError> {
        let now = u32::from(minute);
        let weight_f = f64::from(weight);
        let pending = item(self.next_id, config, weight_f);
        let before = admission(&self.gate);
        let forecast = forecast_admit(&self.ledger, &self.gate, &pending, now);
        let result = self.gate.try_admit(&pending, now);
        let after = admission(&self.gate);
        let kind_ok = matches!(
            (forecast.kind, &result),
            (ForecastKind::Admit, Ok(_))
                | (ForecastKind::Canary, Err(AdmitError::CanaryProbeCap { .. }))
                | (ForecastKind::Night, Err(AdmitError::NightThrottled { .. }))
                | (ForecastKind::Wip, Err(AdmitError::WipCap { .. }))
        );
        prop_assert!(
            kind_ok,
            "step={} forecast={:?} result={:?}",
            step,
            forecast.kind,
            result
        );
        self.ledger = forecast.ledger;
        self.check_night(step)?;
        match &result {
            Ok(ticket) => {
                prop_assert_eq!(ticket.item_id, pending.id, "step={}", step);
                prop_assert_eq!(ticket.weight.to_bits(), weight_f.to_bits(), "step={}", step);
                prop_assert_eq!(ticket.config.as_str(), config, "step={}", step);
                prop_assert_eq!(after.count, before.count + 1, "step={}", step);
                prop_assert_eq!(
                    after.wip_bits,
                    (f64::from_bits(before.wip_bits) + weight_f).to_bits(),
                    "step={}",
                    step
                );
                match (&before.canary, &after.canary) {
                    (None, None) => {}
                    (Some(prev), Some(next)) => {
                        prop_assert_eq!(
                            next.config.as_str(),
                            prev.config.as_str(),
                            "step={}",
                            step
                        );
                        prop_assert_eq!(next.max_probes, prev.max_probes, "step={}", step);
                        prop_assert_eq!(next.open, prev.open, "step={} admit flipped open", step);
                        if !prev.open && prev.config == config {
                            prop_assert_eq!(next.released, prev.released + 1, "step={}", step);
                        } else {
                            prop_assert_eq!(next.released, prev.released, "step={}", step);
                        }
                    }
                    _ => {
                        return Err(TestCaseError::fail(format!(
                            "step={step} canary presence changed on admit"
                        )));
                    }
                }
                self.held.push(Held {
                    id: pending.id,
                    config,
                    weight,
                });
                self.next_id += 1;
            }
            Err(AdmitError::CanaryProbeCap {
                config: capped,
                cap,
            }) => {
                prop_assert_eq!(&after, &before, "step={} partial canary reject", step);
                prop_assert_eq!(capped, config, "step={}", step);
                prop_assert_eq!(
                    before.canary.as_ref().map(|c| c.max_probes),
                    Some(*cap),
                    "step={}",
                    step
                );
            }
            Err(AdmitError::NightThrottled { ratio }) => {
                prop_assert_eq!(&after, &before, "step={} partial night reject", step);
                let expect = self
                    .gate
                    .config()
                    .night_window
                    .as_ref()
                    .expect("throttle implies a window")
                    .ratio;
                prop_assert!(
                    (ratio - expect).abs() <= 1e-12,
                    "step={} ratio={}",
                    step,
                    ratio
                );
            }
            Err(AdmitError::WipCap {
                wip,
                weight: got,
                cap,
            }) => {
                prop_assert_eq!(&after, &before, "step={} partial wip reject", step);
                prop_assert_eq!(wip.to_bits(), before.wip_bits, "step={}", step);
                prop_assert_eq!(got.to_bits(), weight_f.to_bits(), "step={}", step);
                prop_assert_eq!(
                    cap.to_bits(),
                    self.gate.config().wip_upper_minutes.to_bits(),
                    "step={}",
                    step
                );
            }
            Err(AdmitError::TimedOut) => {
                return Err(TestCaseError::fail(format!(
                    "step={step} try_admit does not time out"
                )));
            }
        }
        let rejected = result.is_err();
        self.take_admit_events(&forecast, rejected, now, step)?;
        self.check_books(step)?;
        Ok(forecast.kind)
    }

    fn complete_op(
        &mut self,
        known: bool,
        slot: u8,
        defects: u8,
        minute: u16,
        step: usize,
    ) -> Result<(), TestCaseError> {
        let id = pick_id(&self.held, known, slot, &mut self.next_id);
        let known_item = self.held.iter().find(|h| h.id == id).copied();
        let before = admission(&self.gate);
        let result = self
            .gate
            .complete(id, u32::from(minute), u64::from(defects));
        let after = admission(&self.gate);
        let events = self.gate.drain_events();
        match (known_item, result) {
            (None, Err(GateError::UnknownItem(got))) => {
                prop_assert_eq!(got, id, "step={}", step);
                prop_assert_eq!(
                    &after,
                    &before,
                    "step={} unknown complete mutated state",
                    step
                );
                prop_assert!(events.is_empty(), "step={} {:?}", step, events);
            }
            (Some(held), Ok(done)) => {
                prop_assert_eq!(done.ticket.item_id, held.id, "step={}", step);
                prop_assert_eq!(
                    done.ticket.weight.to_bits(),
                    f64::from(held.weight).to_bits(),
                    "step={}",
                    step
                );
                prop_assert_eq!(after.count, before.count - 1, "step={}", step);
                prop_assert_eq!(
                    after.wip_bits,
                    (f64::from_bits(before.wip_bits) - f64::from(held.weight)).to_bits(),
                    "step={}",
                    step
                );
                prop_assert_eq!(after.refill, self.gate.refill_needed(), "step={}", step);
                match (&before.canary, &after.canary) {
                    (None, None) => {
                        prop_assert_eq!(count_promos(&events), 0, "step={}", step);
                    }
                    (Some(prev), Some(next)) => {
                        prop_assert_eq!(next.released, prev.released, "step={}", step);
                        prop_assert_eq!(next.max_probes, prev.max_probes, "step={}", step);
                        prop_assert_eq!(
                            next.config.as_str(),
                            prev.config.as_str(),
                            "step={}",
                            step
                        );
                        if prev.open {
                            prop_assert!(next.open, "step={} promotion reverted", step);
                        }
                        let matching = !prev.open && prev.config == held.config;
                        if next.open && !prev.open {
                            prop_assert!(matching, "step={} promoted on a non-probe", step);
                            prop_assert_eq!(count_promos(&events), 1, "step={}", step);
                        } else {
                            prop_assert_eq!(next.open, prev.open, "step={}", step);
                            prop_assert_eq!(count_promos(&events), 0, "step={}", step);
                        }
                    }
                    _ => {
                        return Err(TestCaseError::fail(format!(
                            "step={step} canary presence changed on complete"
                        )));
                    }
                }
                prop_assert!(
                    events.iter().all(|event| {
                        matches!(
                            event,
                            GateEvent::CanaryPromoted { .. } | GateEvent::NeedRefill { .. }
                        )
                    }),
                    "step={} {:?}",
                    step,
                    events
                );
                let pos = self.held.iter().position(|h| h.id == held.id);
                prop_assert!(pos.is_some(), "step={}", step);
                if let Some(pos) = pos {
                    self.held.remove(pos);
                }
            }
            (Some(_), Err(err)) => {
                return Err(TestCaseError::fail(format!(
                    "step={step} known item rejected {err:?}"
                )));
            }
            (None, Ok(_)) => {
                return Err(TestCaseError::fail(format!(
                    "step={step} unknown item was completed"
                )));
            }
            (None, Err(err)) => {
                return Err(TestCaseError::fail(format!(
                    "step={step} unexpected complete error {err:?}"
                )));
            }
        }
        self.check_night(step)?;
        self.check_books(step)?;
        Ok(())
    }

    fn begin(
        &mut self,
        config: &'static str,
        max_probes: u8,
        pred: Pred,
        step: usize,
    ) -> Result<(), TestCaseError> {
        let before = admission(&self.gate);
        let result = self
            .gate
            .begin_canary(config, usize::from(max_probes), predicate(pred));
        let after = admission(&self.gate);
        let events = self.gate.drain_events();
        match result {
            Err(GateError::CanaryAlreadyActive(name)) => {
                prop_assert_eq!(&after, &before, "step={} failed begin mutated state", step);
                prop_assert!(
                    before
                        .canary
                        .as_ref()
                        .is_some_and(|c| !c.open && c.config == name),
                    "step={}",
                    step
                );
                prop_assert!(events.is_empty(), "step={} {:?}", step, events);
            }
            Ok(()) => {
                prop_assert_eq!(after.wip_bits, before.wip_bits, "step={}", step);
                prop_assert_eq!(after.count, before.count, "step={}", step);
                prop_assert_eq!(after.refill, before.refill, "step={}", step);
                let st = after.canary.expect("begin installs a canary");
                prop_assert_eq!(st.config.as_str(), config, "step={}", step);
                prop_assert!(!st.open, "step={}", step);
                prop_assert_eq!(st.released, 0, "step={}", step);
                prop_assert_eq!(st.max_probes, usize::from(max_probes), "step={}", step);
                prop_assert!(
                    before.canary.as_ref().is_none_or(|c| c.open),
                    "step={} replaced a gated canary",
                    step
                );
                prop_assert!(events.is_empty(), "step={} {:?}", step, events);
            }
            Err(err) => {
                return Err(TestCaseError::fail(format!(
                    "step={step} unexpected begin error {err:?}"
                )));
            }
        }
        self.check_night(step)?;
        self.check_books(step)?;
        Ok(())
    }
}

/// 固定序列：窗内超重件只烧信用、下一次比例不足整单拒绝、出窗重置后 WIP 仍在。
fn script_night_rejection_is_atomic() -> Result<(), TestCaseError> {
    let spec = GateSpec {
        upper: 10,
        lower: 0,
        night: Some(NightSpec {
            start: 0,
            end: 30,
            eighths: 4,
        }),
    };
    let mut world = World::open(&spec);
    let kind = world.admit("v1", 11, 0, 1)?;
    prop_assert!(
        matches!(kind, ForecastKind::Wip),
        "heavy item inside the window is WipCap, got {:?}",
        kind
    );
    prop_assert_eq!(world.gate.wip_minutes().to_bits(), 0.0f64.to_bits());
    prop_assert_eq!(world.gate.night_stats(), (1, 1));
    prop_assert!(world.held.is_empty());

    let kind = world.admit("v1", 1, 0, 2)?;
    prop_assert!(
        matches!(kind, ForecastKind::Admit),
        "second request of ratio 0.5 still passes credit, got {:?}",
        kind
    );
    prop_assert_eq!(world.gate.night_stats(), (2, 2));
    prop_assert_eq!(world.held.len(), 1);
    prop_assert_eq!(world.gate.wip_minutes().to_bits(), 1.0f64.to_bits());

    let kind = world.admit("v1", 1, 0, 3)?;
    prop_assert!(
        matches!(kind, ForecastKind::Night),
        "third request of ratio 0.5 is throttled, got {:?}",
        kind
    );
    prop_assert_eq!(world.gate.night_stats(), (3, 2));
    prop_assert_eq!(world.held.len(), 1);
    prop_assert_eq!(world.gate.wip_minutes().to_bits(), 1.0f64.to_bits());

    let kind = world.admit("v1", 100, 30, 4)?;
    prop_assert!(
        matches!(kind, ForecastKind::Wip),
        "leaving the window then missing capacity is WipCap, got {:?}",
        kind
    );
    prop_assert_eq!(world.gate.night_stats(), (0, 0));
    prop_assert_eq!(world.gate.wip_minutes().to_bits(), 1.0f64.to_bits());
    prop_assert_eq!(world.held.len(), 1);
    Ok(())
}

fn script_canary_rejection_is_atomic() -> Result<(), TestCaseError> {
    let spec = GateSpec {
        upper: 100,
        lower: 0,
        night: None,
    };
    let mut world = World::open(&spec);
    world.begin("v2", 1, Pred::Never, 1)?;
    let kind = world.admit("v2", 1, 0, 2)?;
    prop_assert!(matches!(kind, ForecastKind::Admit), "got {:?}", kind);
    prop_assert_eq!(world.gate.canary_status().expect("canary").released, 1);
    let before = admission(&world.gate);
    let kind = world.admit("v2", 1, 0, 3)?;
    prop_assert!(
        matches!(kind, ForecastKind::Canary),
        "second probe is capped, got {:?}",
        kind
    );
    prop_assert_eq!(admission(&world.gate), before);
    prop_assert_eq!(world.gate.night_stats(), (0, 0));
    let kind = world.admit("v1", 1, 0, 4)?;
    prop_assert!(
        matches!(kind, ForecastKind::Admit),
        "other config bypasses the probe cap, got {:?}",
        kind
    );
    prop_assert_eq!(world.gate.canary_status().expect("canary").released, 1);
    world.begin("v3", 1, Pred::Always, 5)?;
    let st = world.gate.canary_status().expect("canary");
    prop_assert_eq!(st.config.as_str(), "v2");
    prop_assert!(!st.open);
    prop_assert_eq!(st.released, 1);
    world.complete_op(false, 0, 0, 6, 6)?;
    prop_assert_eq!(world.gate.wip_minutes().to_bits(), 2.0f64.to_bits());
    Ok(())
}

fn walk_rejection(case: &Case) -> Result<(), TestCaseError> {
    let mut world = World::open(&case.gate);
    for (step, op) in case.ops.iter().enumerate() {
        match *op {
            Op::Admit {
                config,
                weight,
                minute,
            } => {
                world.admit(config, weight, minute, step)?;
            }
            Op::Complete {
                known,
                slot,
                defects,
                minute,
            } => {
                world.complete_op(known, slot, defects, minute, step)?;
            }
            Op::BeginCanary {
                config,
                max_probes,
                pred,
            } => {
                world.begin(config, max_probes, pred, step)?;
            }
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(proptest_cfg())]

    /// 组 1。任意准入 / 完成 / 开启渐放序列：WIP 等于在制权重和，且不超过上限。
    #[test]
    fn conwip_capacity_never_over_releases(case in arb_case()) {
        script_exact_fit(case.gate.upper)?;
        walk_conwip(&case)?;
    }

    /// 组 2。同一渐放批次的 released 只增不减且不超过封顶；open 只从 false 到 true。
    #[test]
    fn canary_batch_is_monotonic_and_never_reverts(case in arb_case()) {
        script_promotion_does_not_revert()?;
        script_defects_keep_batch_gated()?;
        walk_canary(&case)?;
    }

    /// 组 3。拒绝路径不改 WIP、在制件数、渐放计数、下限标志；夜间计数只按已验证顺序变化。
    #[test]
    fn rejection_does_not_partially_commit(case in arb_case()) {
        script_night_rejection_is_atomic()?;
        script_canary_rejection_is_atomic()?;
        walk_rejection(&case)?;
    }
}
