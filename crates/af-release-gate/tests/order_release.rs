//! 工单 Created→Released 过投放闸。
//!
//! 顺序：工单状态机与 release 门控 → 权重 → 渐放 → 夜间窗口 → CONWIP。
//! 门控失败时 `try_admit` 不会被调用（夜间观察保持原值，信用不扣）。

use af_release_gate::{
    EvalReport, GateConfig, GateEvent, NightWindow, RampPhase, Reject, ReleaseGate,
};
use af_workorder::{
    BudgetVoucher, GateError, GraphVersion, HistoryRecord, LineBinding, Missing, OrderState,
    Priority, Sla, Transition, WorkOrder,
};

fn cfg(upper: f64, lower: f64, night: Option<NightWindow>) -> GateConfig {
    GateConfig {
        wip_upper_minutes: upper,
        wip_lower_minutes: lower,
        night_window: night,
    }
}

fn sla() -> Sla {
    Sla::new(None, Priority::Normal)
}

fn ready(id: &str, version: &str) -> WorkOrder {
    let mut order = WorkOrder::new(id, GraphVersion::new(version), sla());
    order
        .bind_line(LineBinding {
            line_id: "line-A".into(),
            line_version: "lv-1".into(),
        })
        .unwrap();
    order
        .attach_budget_voucher(BudgetVoucher {
            voucher_id: format!("bv-{id}"),
            amount: 100,
            signed: true,
        })
        .unwrap();
    order
}

fn assert_capacity(err: Reject, wip: f64, weight: f64, cap: f64) {
    match err {
        Reject::AtCapacity {
            wip: got_wip,
            weight: got_weight,
            cap: got_cap,
        } => {
            assert!((got_wip - wip).abs() < 1e-9, "wip {got_wip}");
            assert!((got_weight - weight).abs() < 1e-9, "weight {got_weight}");
            assert!((got_cap - cap).abs() < 1e-9, "cap {got_cap}");
        }
        other => panic!("expected AtCapacity, got {other:?}"),
    }
}

#[test]
fn full_gate_rejects_release_until_capacity_returns() {
    let mut gate = ReleaseGate::new(cfg(10.0, 1.0, None)).unwrap();
    let mut first = ready("wo-1", "g-1");
    let mut second = ready("wo-2", "g-1");
    let first_permit = gate.admit(&mut first, 6.0, 100).unwrap();
    let second_permit = gate.admit(&mut second, 4.0, 101).unwrap();
    assert_eq!(first.state(), OrderState::Released);
    assert_eq!(second.state(), OrderState::Released);
    assert_eq!(first_permit.ramp, RampPhase::Ungated);
    assert_eq!(second_permit.ramp, RampPhase::Ungated);
    assert_eq!(second_permit.order_id, "wo-2");
    assert_eq!(second_permit.ticket.config, "g-1");
    assert_eq!(second_permit.ticket.admitted_at, 101);
    assert!((gate.wip_minutes() - 10.0).abs() < 1e-9);

    let mut third = ready("wo-3", "g-1");
    let err = gate.admit(&mut third, 1.0, 102).unwrap_err();
    assert_capacity(err, 10.0, 1.0, 10.0);
    assert_eq!(third.state(), OrderState::Created);
    assert!(third.history().is_empty());
    assert_eq!(gate.in_process_count(), 2);
    assert!((gate.wip_minutes() - 10.0).abs() < 1e-9);

    // 放掉恰好占满余量的那一件后，同一张单可以按精确等号进入。
    gate.complete(second_permit.ticket.item_id, 103, 0).unwrap();
    let permit = gate.admit(&mut third, 4.0, 104).unwrap();
    assert_eq!(third.state(), OrderState::Released);
    assert_eq!(
        third.history(),
        &[HistoryRecord {
            from: OrderState::Created,
            to: OrderState::Released,
        }]
    );
    assert_eq!(permit.ramp, RampPhase::Ungated);
    assert!((gate.wip_minutes() - 10.0).abs() < 1e-9);
    assert_eq!(gate.in_process_count(), 2);
}

#[test]
fn ramp_phase_blocks_until_promotion_then_admits() {
    let mut gate = ReleaseGate::new(cfg(100.0, 1.0, None)).unwrap();
    gate.begin_canary(
        "g-2",
        1,
        Box::new(|report: &EvalReport| report.defects == 0 && report.probes_completed >= 1),
    )
    .unwrap();

    let mut probe = ready("probe", "g-2");
    let admitted = gate.admit(&mut probe, 3.0, 10).unwrap();
    assert_eq!(probe.state(), OrderState::Released);
    assert_eq!(
        admitted.ramp,
        RampPhase::Probe {
            index: 1,
            max_probes: 1,
        }
    );
    assert_eq!(admitted.ticket.config, "g-2");

    let mut blocked = ready("next", "g-2");
    let err = gate.admit(&mut blocked, 3.0, 11).unwrap_err();
    assert_eq!(
        err,
        Reject::RampPhaseNotOpen {
            config: "g-2".into(),
            released: 1,
            cap: 1,
        }
    );
    assert_eq!(blocked.state(), OrderState::Created);
    assert!(blocked.history().is_empty());
    let status = gate.canary_status().unwrap();
    assert_eq!(status.released, 1);
    assert!(!status.open);
    assert!((gate.wip_minutes() - 3.0).abs() < 1e-9);

    // 渐放只封自己的图谱版本。
    let mut other = ready("other", "g-1");
    let other_permit = gate.admit(&mut other, 3.0, 12).unwrap();
    assert_eq!(other.state(), OrderState::Released);
    assert_eq!(other_permit.ramp, RampPhase::Ungated);

    gate.complete(admitted.ticket.item_id, 13, 0).unwrap();
    assert!(gate.canary_status().unwrap().open);
    assert_eq!(
        gate.drain_events(),
        vec![GateEvent::CanaryPromoted {
            config: "g-2".into(),
            probes_released: 1,
            at: 13,
        }]
    );

    let permit = gate.admit(&mut blocked, 3.0, 14).unwrap();
    assert_eq!(blocked.state(), OrderState::Released);
    assert_eq!(permit.ramp, RampPhase::Open { probes_released: 1 });
    assert_eq!(gate.canary_status().unwrap().released, 1);
}

#[test]
fn missing_preconditions_win_over_full_gate() {
    let mut gate = ReleaseGate::new(cfg(5.0, 1.0, None)).unwrap();
    let mut held = ready("held", "g-1");
    gate.admit(&mut held, 5.0, 0).unwrap();
    gate.drain_events();
    let wip = gate.wip_minutes();
    let count = gate.in_process_count();
    let (requests, admitted) = gate.night_stats();

    let mut bare = WorkOrder::new("bare", GraphVersion::new("g-1"), sla());
    let err = gate.admit(&mut bare, 5.0, 1).unwrap_err();
    assert_eq!(
        err,
        Reject::Order(GateError::PreconditionsNotMet {
            transition: Transition::Release,
            missing: vec![
                Missing::LineVersionNotBound,
                Missing::BudgetVoucherNotIssued,
            ],
        })
    );
    assert_eq!(bare.state(), OrderState::Created);
    assert!(bare.history().is_empty());
    assert!((gate.wip_minutes() - wip).abs() < 1e-9);
    assert_eq!(gate.in_process_count(), count);
    assert_eq!(gate.night_stats(), (requests, admitted));
    assert!(gate.drain_events().is_empty());
}

#[test]
fn missing_preconditions_win_over_closed_ramp() {
    let mut gate = ReleaseGate::new(cfg(100.0, 1.0, None)).unwrap();
    // 探针名额为 0：只要咨询了渐放，g-2 会得到 RampPhaseNotOpen。
    gate.begin_canary("g-2", 0, Box::new(|_: &EvalReport| false))
        .unwrap();

    let mut order = WorkOrder::new("bare", GraphVersion::new("g-2"), sla());
    order
        .bind_line(LineBinding {
            line_id: "line-A".into(),
            line_version: "lv-1".into(),
        })
        .unwrap();
    order
        .attach_budget_voucher(BudgetVoucher {
            voucher_id: "bv-x".into(),
            amount: 1,
            signed: false,
        })
        .unwrap();

    let err = gate.admit(&mut order, 1.0, 20).unwrap_err();
    assert_eq!(
        err,
        Reject::Order(GateError::PreconditionsNotMet {
            transition: Transition::Release,
            missing: vec![Missing::BudgetVoucherNotSigned {
                voucher_id: "bv-x".into(),
            }],
        })
    );
    assert_eq!(order.state(), OrderState::Created);
    let status = gate.canary_status().unwrap();
    assert_eq!(status.released, 0);
    assert!(!status.open);
    assert!(gate.wip_minutes().abs() < 1e-9);
    // try_admit 第一步就会记下窗口观察。还是 None 说明投放闸没被咨询。
    assert!(gate.night_view().prev_in_window.is_none());
}

#[test]
fn missing_preconditions_do_not_spend_night_credit() {
    let night = NightWindow {
        start: 0,
        end: 60,
        ratio: 0.25,
    };
    let mut gate = ReleaseGate::new(cfg(50.0, 1.0, Some(night))).unwrap();
    let mut bare = WorkOrder::new("bare", GraphVersion::new("g-1"), sla());
    let err = gate.admit(&mut bare, 1.0, 10).unwrap_err();
    assert!(matches!(
        err,
        Reject::Order(GateError::PreconditionsNotMet { .. })
    ));
    assert_eq!(bare.state(), OrderState::Created);
    assert_eq!(gate.night_stats(), (0, 0));
    assert!(gate.night_view().prev_in_window.is_none());

    // 初始信用 1.0。若上面那次已经扣过信用，这一次会因 0.25 不够 1 而被夜间拒绝。
    let mut ready_order = ready("ok", "g-1");
    let permit = gate.admit(&mut ready_order, 1.0, 10).unwrap();
    assert_eq!(ready_order.state(), OrderState::Released);
    assert_eq!(permit.ramp, RampPhase::Ungated);
    assert_eq!(gate.night_stats(), (1, 1));
}

#[test]
fn admit_rejects_when_release_edge_is_absent() {
    let mut gate = ReleaseGate::new(cfg(10.0, 1.0, None)).unwrap();

    let mut bypassed = ready("bypass", "g-1");
    bypassed.release().unwrap();
    let err = gate.admit(&mut bypassed, 4.0, 5).unwrap_err();
    assert_eq!(
        err,
        Reject::Order(GateError::IllegalTransition {
            from: OrderState::Released,
            to: OrderState::Released,
            allowed: vec![Transition::Start, Transition::Cancel],
        })
    );
    assert_eq!(bypassed.state(), OrderState::Released);
    assert_eq!(gate.in_process_count(), 0);
    assert!(gate.wip_minutes().abs() < 1e-9);
    assert!(gate.night_view().prev_in_window.is_none());

    let mut order = ready("wo", "g-1");
    gate.admit(&mut order, 4.0, 6).unwrap();
    let err = gate.admit(&mut order, 4.0, 7).unwrap_err();
    assert_eq!(
        err,
        Reject::Order(GateError::IllegalTransition {
            from: OrderState::Released,
            to: OrderState::Released,
            allowed: vec![Transition::Start, Transition::Cancel],
        })
    );
    assert!((gate.wip_minutes() - 4.0).abs() < 1e-9);
    assert_eq!(gate.in_process_count(), 1);
    assert_eq!(order.history().len(), 1);
}

#[test]
fn non_positive_weight_does_not_enter_the_gate() {
    let mut gate = ReleaseGate::new(cfg(10.0, 1.0, None)).unwrap();
    gate.begin_canary("g-1", 0, Box::new(|_: &EvalReport| false))
        .unwrap();
    let mut order = ready("wo", "g-1");

    for weight in [0.0, -1.0] {
        let err = gate.admit(&mut order, weight, 3).unwrap_err();
        match err {
            Reject::BadWeight { weight: got } => assert!((got - weight).abs() < 1e-9),
            other => panic!("expected BadWeight, got {other:?}"),
        }
        assert_eq!(order.state(), OrderState::Created);
    }
    for weight in [f64::INFINITY, f64::NEG_INFINITY] {
        let err = gate.admit(&mut order, weight, 3).unwrap_err();
        match err {
            Reject::BadWeight { weight: got } => assert!(got.is_infinite()),
            other => panic!("expected BadWeight, got {other:?}"),
        }
    }
    match gate.admit(&mut order, f64::NAN, 3).unwrap_err() {
        Reject::BadWeight { weight } => assert!(weight.is_nan()),
        other => panic!("expected BadWeight, got {other:?}"),
    }
    assert_eq!(order.state(), OrderState::Created);
    assert_eq!(gate.canary_status().unwrap().released, 0);
    assert!(gate.night_view().prev_in_window.is_none());

    let mut bare = WorkOrder::new("bare", GraphVersion::new("g-1"), sla());
    let err = gate.admit(&mut bare, f64::NAN, 4).unwrap_err();
    assert!(matches!(
        err,
        Reject::Order(GateError::PreconditionsNotMet { .. })
    ));
    assert!(gate.night_view().prev_in_window.is_none());

    // 权重合法之后才看见渐放关闭，说明非法权重没有先把相位判掉。
    let err = gate.admit(&mut order, 1.0, 5).unwrap_err();
    assert_eq!(
        err,
        Reject::RampPhaseNotOpen {
            config: "g-1".into(),
            released: 0,
            cap: 0,
        }
    );
    assert_eq!(order.state(), OrderState::Created);
}
