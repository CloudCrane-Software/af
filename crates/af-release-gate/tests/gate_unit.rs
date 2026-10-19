//! 闸门语义单元测试:每条 CONWIP/夜间/渐放规则一个用例。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use af_release_gate::{
    AdmitError, EvalReport, GateConfig, GateEvent, Item, NightWindow, ReleaseGate, SharedGate,
};

fn cfg(upper: f64, lower: f64, night: Option<NightWindow>) -> GateConfig {
    GateConfig {
        wip_upper_minutes: upper,
        wip_lower_minutes: lower,
        night_window: night,
    }
}

fn item(id: u64, weight: f64) -> Item {
    Item {
        id,
        config: "v1".into(),
        review_minutes: weight,
    }
}

fn err_of(r: Result<af_release_gate::Ticket, AdmitError>) -> AdmitError {
    r.expect_err("expected rejection")
}

#[test]
fn wip_cap_blocks_admit_and_complete_releases_capacity() {
    let mut g = ReleaseGate::new(cfg(30.0, 10.0, None)).unwrap();
    g.try_admit(&item(1, 20.0), 0).unwrap();
    let e = err_of(g.try_admit(&item(2, 15.0), 1));
    assert!(
        matches!(e, AdmitError::WipCap { wip, weight, cap } if (wip - 20.0).abs() < 1e-9 && weight == 15.0 && cap == 30.0)
    );
    // 完成一件释放容量后,同一件可准入。
    g.complete(1, 2, 0).unwrap();
    g.try_admit(&item(2, 15.0), 3).unwrap();
    assert!((g.wip_minutes() - 15.0).abs() < 1e-9);
}

#[test]
fn wip_never_exceeds_cap_even_on_exact_fit() {
    let mut g = ReleaseGate::new(cfg(30.0, 10.0, None)).unwrap();
    g.try_admit(&item(1, 30.0), 0).unwrap();
    // 恰好等于上限可以,再小一点都不行。
    assert!(g.try_admit(&item(2, 1.0), 1).is_err());
    g.complete(1, 2, 0).unwrap();
    assert!(g.try_admit(&item(2, 1.0), 3).is_ok());
}

#[test]
fn item_heavier_than_cap_never_admits() {
    let mut g = ReleaseGate::new(cfg(10.0, 2.0, None)).unwrap();
    assert!(matches!(
        err_of(g.try_admit(&item(1, 99.0), 0)),
        AdmitError::WipCap { .. }
    ));
}

#[test]
fn config_validation_rejects_lower_ge_upper() {
    assert!(ReleaseGate::new(cfg(10.0, 10.0, None)).is_err());
    assert!(ReleaseGate::new(cfg(10.0, 12.0, None)).is_err());
}

#[test]
fn refill_signal_fires_below_lower_and_clears_above() {
    let mut g = ReleaseGate::new(cfg(100.0, 50.0, None)).unwrap();
    g.try_admit(&item(1, 60.0), 0).unwrap();
    assert!(!g.refill_needed());
    g.complete(1, 1, 0).unwrap();
    // wip=0 < 50 → 预警触发一次。
    assert!(g.refill_needed());
    g.try_admit(&item(2, 40.0), 2).unwrap();
    // wip=40 仍 < 50。
    assert!(g.refill_needed());
    g.try_admit(&item(3, 20.0), 3).unwrap();
    assert!(!g.refill_needed());
    let evs = g.drain_events();
    assert_eq!(
        evs.iter()
            .filter(|e| matches!(e, GateEvent::NeedRefill { .. }))
            .count(),
        1,
        "NeedRefill 边沿触发仅一次: {evs:?}"
    );
    assert!(evs.contains(&GateEvent::RefillSatisfied { at: 3, wip: 60.0 }));
}

#[test]
fn night_window_contains_handles_wrap() {
    let w = NightWindow {
        start: 22 * 60,
        end: 6 * 60,
        ratio: 0.3,
    }; // 22:00→06:00
    assert!(w.contains(0));
    assert!(w.contains(359));
    assert!(!w.contains(360));
    assert!(!w.contains(1319));
    assert!(w.contains(1320));
    let day = NightWindow {
        start: 100,
        end: 200,
        ratio: 1.0,
    };
    assert!(!day.contains(99) && day.contains(100) && !day.contains(200));
}

#[test]
fn night_ratio_is_deterministic_credit_scheme() {
    // 整天处于窗口内:信用 = 1(初始) + n×ratio,放行数 = floor(1 + n×ratio)。
    // 50 件请求 ratio=0.4 → floor(1+20) = 21 件。
    let mut g = ReleaseGate::new(cfg(
        1e9,
        0.0,
        Some(NightWindow {
            start: 0,
            end: 1,
            ratio: 0.4,
        }),
    ))
    .unwrap();
    let mut admitted = 0;
    for i in 0..50u64 {
        // now 在 [0,1) 窗口内恒为 0。
        if g.try_admit(&item(i, 1.0), 0).is_ok() {
            admitted += 1;
        }
    }
    assert_eq!(admitted, 21);
    let (req, adm) = g.night_stats();
    assert_eq!((req, adm), (50, 21));
}

#[test]
fn night_ratio_one_admits_everything() {
    let mut g = ReleaseGate::new(cfg(
        1e9,
        0.0,
        Some(NightWindow {
            start: 0,
            end: 1,
            ratio: 1.0,
        }),
    ))
    .unwrap();
    for i in 0..10u64 {
        assert!(g.try_admit(&item(i, 1.0), 0).is_ok());
    }
}

#[test]
fn night_window_exit_resets_credit() {
    // 跨午夜窗口 [1350, 30),ratio 0.5:窗口内放行数 = floor(1 + n×0.5)。
    let mut g = ReleaseGate::new(cfg(
        1e9,
        0.0,
        Some(NightWindow {
            start: 1350,
            end: 30,
            ratio: 0.5,
        }),
    ))
    .unwrap();
    // t=0 在窗口内:信用 1.0 起,#1、#2 放行,#3 被限。
    assert!(g.try_admit(&item(1, 1.0), 0).is_ok());
    assert!(g.try_admit(&item(2, 1.0), 0).is_ok());
    assert!(matches!(
        err_of(g.try_admit(&item(3, 1.0), 0)),
        AdmitError::NightThrottled { .. }
    ));
    // 出窗(30)后不受限。
    assert!(g.try_admit(&item(4, 1.0), 30).is_ok());
    assert!(g.try_admit(&item(5, 1.0), 31).is_ok());
    // 1350 重进窗口:信用重置,头两件放行、第三件被限。
    assert!(g.try_admit(&item(6, 1.0), 1350).is_ok());
    assert!(g.try_admit(&item(7, 1.0), 1350).is_ok());
    assert!(matches!(
        err_of(g.try_admit(&item(8, 1.0), 1350)),
        AdmitError::NightThrottled { .. }
    ));
    let evs = g.drain_events();
    assert!(evs.contains(&GateEvent::NightWindowExit { at: 30 }));
    assert!(evs.contains(&GateEvent::NightWindowEnter { at: 1350 }));
}

#[test]
fn canary_caps_probes_until_predicate_passes() {
    // 评估谓词:至少 1 件探针干净完成(累计缺陷为 0)即晋升。
    let mut g = ReleaseGate::new(cfg(1e9, 0.0, None)).unwrap();
    g.begin_canary(
        "v2",
        3,
        Box::new(|r: &EvalReport| r.probes_completed >= 1 && r.defects == 0),
    )
    .unwrap();
    // 前 3 件探针放行,第 4 件被渐放封顶。
    for i in 0..3u64 {
        g.try_admit(
            &Item {
                id: i,
                config: "v2".into(),
                review_minutes: 5.0,
            },
            0,
        )
        .unwrap();
    }
    assert!(matches!(
        err_of(g.try_admit(&Item { id: 9, config: "v2".into(), review_minutes: 5.0 }, 1)),
        AdmitError::CanaryProbeCap { config, cap } if config == "v2" && cap == 3
    ));
    // v1 不受影响。
    assert!(g.try_admit(&item(20, 5.0), 1).is_ok());
    // 干净完成 → 谓词通过 → 晋升,第 4 件放行。
    g.complete(0, 2, 0).unwrap();
    let st = g.canary_status().unwrap();
    assert!(st.open);
    g.try_admit(
        &Item {
            id: 9,
            config: "v2".into(),
            review_minutes: 5.0,
        },
        4,
    )
    .unwrap();
    let evs = g.drain_events();
    assert!(evs.contains(&GateEvent::CanaryPromoted {
        config: "v2".into(),
        probes_released: 3,
        at: 2
    }));
}

#[test]
fn canary_defect_keeps_gate_gated() {
    // 评估报告按累计口径:首件带缺陷时,零缺陷谓词不通过 → 继续封顶。
    let mut g = ReleaseGate::new(cfg(1e9, 0.0, None)).unwrap();
    g.begin_canary(
        "v2",
        3,
        Box::new(|r: &EvalReport| r.probes_completed >= 1 && r.defects == 0),
    )
    .unwrap();
    for i in 0..3u64 {
        g.try_admit(
            &Item {
                id: i,
                config: "v2".into(),
                review_minutes: 5.0,
            },
            0,
        )
        .unwrap();
    }
    g.complete(0, 1, 3).unwrap(); // 带缺陷完成
    let st = g.canary_status().unwrap();
    assert!(!st.open, "累计缺陷 > 0 时不得晋升");
    assert!(matches!(
        err_of(g.try_admit(
            &Item {
                id: 9,
                config: "v2".into(),
                review_minutes: 5.0
            },
            2
        )),
        AdmitError::CanaryProbeCap { .. }
    ));
}

#[test]
fn blocking_admit_wakes_on_completion() {
    let gate = SharedGate::new(ReleaseGate::new(cfg(30.0, 10.0, None)).unwrap());
    gate.try_admit(&item(1, 20.0), 0).unwrap();
    gate.try_admit(&item(2, 10.0), 0).unwrap(); // wip=30 = 上限

    let admitted = Arc::new(AtomicBool::new(false));
    let (admitted_flag, gate2) = (Arc::clone(&admitted), gate.clone());
    let big = item(3, 20.0); // 需要空闲 ≤10 分钟:必须先完成件 1(20 分钟)才放得下
    let producer = thread::spawn(move || {
        gate2
            .admit_blocking(&big, 1, Duration::from_secs(5))
            .expect("producer should admit");
        admitted_flag.store(true, Ordering::SeqCst);
    });

    thread::sleep(Duration::from_millis(150));
    assert!(!admitted.load(Ordering::SeqCst), "在上限时必须阻塞");

    gate.complete(1, 2, 0).unwrap(); // 释放 20 分钟容量,wip=10
    producer.join().unwrap();
    assert!(
        admitted.load(Ordering::SeqCst),
        "完成一件后阻塞的 admit 应被唤醒"
    );
    assert!((gate.with(|g| g.wip_minutes()) - 30.0).abs() < 1e-9); // 10 + 20
}

#[test]
fn blocking_admit_times_out_when_no_capacity_freed() {
    let gate = SharedGate::new(ReleaseGate::new(cfg(30.0, 10.0, None)).unwrap());
    gate.try_admit(&item(1, 30.0), 0).unwrap();
    let r = gate.admit_blocking(&item(2, 5.0), 1, Duration::from_millis(120));
    assert!(matches!(r, Err(AdmitError::TimedOut)));
}

#[test]
fn gate_config_roundtrips_through_json() {
    // 参数注入:配置可从 JSON 反序列化(serde)。
    let json = r#"{
        "wip_upper_minutes": 400.0,
        "wip_lower_minutes": 120.0,
        "night_window": { "start": 1320, "end": 360, "ratio": 0.3 }
    }"#;
    let c: GateConfig = serde_json::from_str(json).unwrap();
    assert_eq!(c.wip_upper_minutes, 400.0);
    let w = c.night_window.clone().unwrap();
    assert_eq!((w.start, w.end, w.ratio), (1320, 360, 0.3));
    let _ = ReleaseGate::new(c).unwrap();
}
