//! 投放闸账本：首启/重入、声明不符失败、重启后在制与渐放游标原样且不超发。
//!
//! 时刻用 [`ManualClock`] 注入。用例里没有 sleep。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use af_release_gate::{
    AdmitError, CanaryCursor, CanaryPredicate, DurableGate, EvalReport, GateConfig, GateError,
    GateEvent, Item, ManualClock, NightWindow, OpenOutcome, PersistError, SCHEMA_VERSION,
};

struct TmpDb {
    path: PathBuf,
}

impl TmpDb {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "af-release-gate-{label}-{}-{n}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TmpDb {
    fn drop(&mut self) {
        let display = self.path.display().to_string();
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(format!("{display}-wal"));
        let _ = std::fs::remove_file(format!("{display}-shm"));
    }
}

fn cfg(upper: f64, lower: f64, night: Option<NightWindow>) -> GateConfig {
    GateConfig {
        wip_upper_minutes: upper,
        wip_lower_minutes: lower,
        night_window: night,
    }
}

fn item(id: u64, config: &str, weight: f64) -> Item {
    Item {
        id,
        config: config.to_string(),
        review_minutes: weight,
    }
}

fn pred_always() -> CanaryPredicate {
    Box::new(|_report: &EvalReport| true)
}

fn pred_clean() -> CanaryPredicate {
    Box::new(|report: &EvalReport| report.probes_completed >= 1 && report.defects == 0)
}

fn pred_never() -> CanaryPredicate {
    Box::new(|_report: &EvalReport| false)
}

fn assert_books(gate: &DurableGate, minutes: f64, count: usize) {
    assert_eq!(gate.wip_minutes().to_bits(), minutes.to_bits());
    assert_eq!(gate.in_process_count(), count);
    let sum = gate
        .in_process()
        .iter()
        .fold(0.0, |acc, item| acc + item.weight);
    assert_eq!(sum.to_bits(), minutes.to_bits(), "wip drifted from items");
    assert!(
        gate.wip_minutes() <= gate.config().wip_upper_minutes + 1e-9,
        "over-release wip={} upper={}",
        gate.wip_minutes(),
        gate.config().wip_upper_minutes
    );
}

#[test]
fn storage_profile_matches_wal_recipe() {
    let tmp = TmpDb::new("profile");
    let (gate, outcome) = DurableGate::open_with_clock(
        tmp.path(),
        &cfg(10.0, 0.0, None),
        None,
        ManualClock::new(10),
    )
    .unwrap();
    assert_eq!(outcome, OpenOutcome::Created);
    let profile = gate.storage_profile().unwrap();
    assert_eq!(profile.journal_mode, "wal");
    assert_eq!(profile.synchronous, 1, "synchronous=NORMAL");
    assert!(profile.foreign_keys);
    assert_eq!(profile.busy_timeout_ms, 5_000);
    drop(gate);

    let conn = rusqlite::Connection::open(tmp.path()).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    let rejected = conn.execute(
        "INSERT INTO wip_item (seq, gate_id, item_id, config, weight_bits, admitted_at)
         VALUES (0, 99, 1, 'v1', 0, 0)",
        [],
    );
    assert!(
        rejected.is_err(),
        "foreign key must reject a gate_id that is not in gate_meta"
    );
}

#[test]
fn invalid_declaration_does_not_create_a_database() {
    let tmp = TmpDb::new("bad-decl");
    let err = DurableGate::open(tmp.path(), &cfg(10.0, 10.0, None), None).unwrap_err();
    assert!(matches!(err, PersistError::Config(_)));
    assert!(!tmp.path().exists());
}

#[test]
fn unrelated_database_is_rejected() {
    let tmp = TmpDb::new("unrelated");
    let conn = rusqlite::Connection::open(tmp.path()).unwrap();
    conn.execute_batch("CREATE TABLE other (x INTEGER);")
        .unwrap();
    drop(conn);

    let err = DurableGate::open(tmp.path(), &cfg(10.0, 0.0, None), None).unwrap_err();
    assert!(matches!(err, PersistError::Corrupt { .. }), "{err:?}");

    let conn = rusqlite::Connection::open(tmp.path()).unwrap();
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM other", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 0);
    let meta: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'gate_meta'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(meta, 0, "fail-closed must not create gate tables");
}

#[test]
fn open_forks_on_created_then_already_declared() {
    let tmp = TmpDb::new("fork");
    let clock = ManualClock::new(100);
    let declaration = cfg(30.0, 5.0, None);
    let (gate, outcome) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap();
    assert_eq!(outcome, OpenOutcome::Created);
    assert_books(&gate, 0.0, 0);
    assert!(gate.canary_cursor().is_none());
    assert_eq!(gate.updated_wall_ms(), 100);
    drop(gate);

    let (gate, outcome) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock).unwrap();
    assert_eq!(outcome, OpenOutcome::AlreadyDeclared);
    assert_eq!(
        gate.updated_wall_ms(),
        100,
        "reopen must not rewrite the stamp"
    );
    assert_books(&gate, 0.0, 0);
}

#[test]
fn system_clock_open_stamps_unix_wall_time() {
    let tmp = TmpDb::new("sys");
    let (gate, outcome) = DurableGate::open(tmp.path(), &cfg(10.0, 0.0, None), None).unwrap();
    assert_eq!(outcome, OpenOutcome::Created);
    assert!(
        gate.updated_wall_ms() > 1_000_000_000_000,
        "unix ms, got {}",
        gate.updated_wall_ms()
    );
}

#[test]
fn declaration_mismatch_fails_closed_without_rewrite() {
    let tmp = TmpDb::new("mismatch");
    let night = NightWindow {
        start: 22 * 60,
        end: 6 * 60,
        ratio: 0.25,
    };
    let declaration = cfg(30.0, 5.0, Some(night.clone()));
    let clock = ManualClock::new(1);
    let (mut gate, _) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap();
    gate.try_admit(&item(1, "v1", 4.0), 10).unwrap();
    drop(gate);

    let wrong = [
        (cfg(40.0, 5.0, Some(night.clone())), "wip_upper_minutes"),
        (cfg(30.0, 6.0, Some(night.clone())), "wip_lower_minutes"),
        (cfg(30.0, 5.0, None), "night_window"),
        (
            cfg(
                30.0,
                5.0,
                Some(NightWindow {
                    start: 1,
                    end: 6 * 60,
                    ratio: 0.25,
                }),
            ),
            "night_start",
        ),
        (
            cfg(
                30.0,
                5.0,
                Some(NightWindow {
                    start: 22 * 60,
                    end: 7 * 60,
                    ratio: 0.25,
                }),
            ),
            "night_end",
        ),
        (
            cfg(
                30.0,
                5.0,
                Some(NightWindow {
                    start: 22 * 60,
                    end: 6 * 60,
                    ratio: 0.5,
                }),
            ),
            "night_ratio",
        ),
    ];
    for (candidate, field) in wrong {
        let err =
            DurableGate::open_with_clock(tmp.path(), &candidate, None, clock.clone()).unwrap_err();
        match err {
            PersistError::DeclarationMismatch { field: got } => assert_eq!(got, field),
            other => panic!("field {field} got {other:?}"),
        }
    }

    let (gate, outcome) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock).unwrap();
    assert_eq!(outcome, OpenOutcome::AlreadyDeclared);
    assert_books(&gate, 4.0, 1);
    assert_eq!(gate.updated_wall_ms(), 1);
    assert_eq!(gate.in_process()[0].id, 1);
    assert_eq!(gate.in_process()[0].admitted_at, 10);
}

#[test]
fn schema_mismatch_fails_closed() {
    let tmp = TmpDb::new("schema");
    let declaration = cfg(10.0, 0.0, None);
    let (gate, _) = DurableGate::open(tmp.path(), &declaration, None).unwrap();
    drop(gate);

    let conn = rusqlite::Connection::open(tmp.path()).unwrap();
    conn.execute("UPDATE gate_meta SET schema_version = 9 WHERE id = 1", [])
        .unwrap();
    drop(conn);
    let err = DurableGate::open(tmp.path(), &declaration, None).unwrap_err();
    assert!(matches!(
        err,
        PersistError::SchemaMismatch {
            stored: 9,
            expected: SCHEMA_VERSION
        }
    ));

    let conn = rusqlite::Connection::open(tmp.path()).unwrap();
    conn.execute("UPDATE gate_meta SET schema_version = 1 WHERE id = 1", [])
        .unwrap();
    conn.pragma_update(None, "user_version", 7i64).unwrap();
    drop(conn);
    let err = DurableGate::open(tmp.path(), &declaration, None).unwrap_err();
    assert!(
        matches!(
            err,
            PersistError::SchemaMismatch {
                stored: 7,
                expected: SCHEMA_VERSION
            }
        ),
        "{err:?}"
    );
}

#[test]
fn restart_restores_wip_and_never_over_releases() {
    let tmp = TmpDb::new("wip");
    let declaration = cfg(10.0, 0.0, None);
    let clock = ManualClock::new(50);
    let (mut gate, outcome) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap();
    assert_eq!(outcome, OpenOutcome::Created);
    gate.try_admit(&item(1, "v1", 6.0), 1).unwrap();
    gate.try_admit(&item(u64::MAX, "v1", 4.0), 2).unwrap();
    assert_books(&gate, 10.0, 2);
    let rejected = gate.try_admit(&item(3, "v1", 1.0), 3).unwrap_err();
    assert!(matches!(
        rejected,
        PersistError::Admit(AdmitError::WipCap { .. })
    ));
    assert_books(&gate, 10.0, 2);
    drop(gate);

    let (mut gate, outcome) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap();
    assert_eq!(outcome, OpenOutcome::AlreadyDeclared);
    assert_books(&gate, 10.0, 2);
    let items = gate.in_process();
    assert_eq!(items[0].id, 1);
    assert_eq!(items[0].weight.to_bits(), 6.0f64.to_bits());
    assert_eq!(items[0].admitted_at, 1);
    assert_eq!(items[0].config, "v1");
    assert_eq!(items[1].id, u64::MAX);
    assert_eq!(items[1].weight.to_bits(), 4.0f64.to_bits());
    assert_eq!(items[1].admitted_at, 2);
    let rejected = gate.try_admit(&item(3, "v1", 1.0), 4).unwrap_err();
    assert!(matches!(
        rejected,
        PersistError::Admit(AdmitError::WipCap { .. })
    ));
    assert_books(&gate, 10.0, 2);
    let unknown = gate.complete(3, 5, 0).unwrap_err();
    assert!(matches!(
        unknown,
        PersistError::Gate(GateError::UnknownItem(3))
    ));
    assert_books(&gate, 10.0, 2);
    gate.complete(1, 6, 0).unwrap();
    assert_books(&gate, 4.0, 1);
    gate.try_admit(&item(4, "v2", 6.0), 7).unwrap();
    assert_books(&gate, 10.0, 2);
    let rejected = gate.try_admit(&item(5, "v1", 1.0), 8).unwrap_err();
    assert!(matches!(
        rejected,
        PersistError::Admit(AdmitError::WipCap { .. })
    ));
    drop(gate);

    let (gate, _) = DurableGate::open_with_clock(tmp.path(), &declaration, None, clock).unwrap();
    assert_books(&gate, 10.0, 2);
    let ids: Vec<_> = gate.in_process().iter().map(|item| item.id).collect();
    assert_eq!(ids, vec![u64::MAX, 4]);
}

#[test]
fn canary_cursor_restores_and_stays_capped() {
    let tmp = TmpDb::new("canary");
    let declaration = cfg(100.0, 0.0, None);
    let clock = ManualClock::new(7);
    let (mut gate, _) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap();
    gate.begin_canary("v2", 2, pred_clean()).unwrap();
    gate.try_admit(&item(1, "v2", 5.0), 3).unwrap();
    gate.try_admit(&item(2, "v2", 7.0), 4).unwrap();
    gate.complete(1, 5, 1).unwrap();
    let cursor = CanaryCursor {
        config: "v2".into(),
        open: false,
        released: 2,
        max_probes: 2,
        completed: 1,
        defects: 1,
    };
    assert_eq!(gate.canary_cursor().as_ref(), Some(&cursor));
    assert_books(&gate, 7.0, 1);
    drop(gate);

    let missing =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap_err();
    assert!(matches!(missing, PersistError::CanaryPredicateRequired));

    let (mut gate, outcome) =
        DurableGate::open_with_clock(tmp.path(), &declaration, Some(pred_clean()), clock.clone())
            .unwrap();
    assert_eq!(outcome, OpenOutcome::AlreadyDeclared);
    assert_eq!(gate.canary_cursor().as_ref(), Some(&cursor));
    assert_books(&gate, 7.0, 1);
    let stamp = gate.updated_wall_ms();
    clock.advance(50);
    let replaced = gate.begin_canary("v3", 4, pred_always()).unwrap_err();
    assert!(matches!(
        replaced,
        PersistError::Gate(GateError::CanaryAlreadyActive(_))
    ));
    assert_eq!(gate.updated_wall_ms(), stamp, "failed begin must not write");
    assert_eq!(gate.canary_cursor().as_ref(), Some(&cursor));
    let rejected = gate.try_admit(&item(3, "v2", 1.0), 6).unwrap_err();
    assert!(matches!(
        rejected,
        PersistError::Admit(AdmitError::CanaryProbeCap { cap: 2, .. })
    ));
    assert_eq!(gate.canary_cursor().unwrap().released, 2);
    assert_books(&gate, 7.0, 1);
    gate.complete(2, 7, 0).unwrap();
    let cursor = gate.canary_cursor().unwrap();
    assert!(!cursor.open, "cumulative defects keep the batch gated");
    assert_eq!(cursor.completed, 2);
    assert_eq!(cursor.defects, 1);
    assert_eq!(cursor.released, 2);
    gate.try_admit(&item(4, "v1", 1.0), 8).unwrap();
    assert_eq!(gate.canary_cursor().unwrap().released, 2);
    assert_books(&gate, 1.0, 1);
    drop(gate);

    let (gate, _) =
        DurableGate::open_with_clock(tmp.path(), &declaration, Some(pred_clean()), clock).unwrap();
    let cursor = gate.canary_cursor().unwrap();
    assert_eq!(cursor.config, "v2");
    assert!(!cursor.open);
    assert_eq!(cursor.released, 2);
    assert_eq!(cursor.completed, 2);
    assert_eq!(cursor.defects, 1);
    assert_books(&gate, 1.0, 1);
}

#[test]
fn promoted_canary_stays_open_across_restart() {
    let tmp = TmpDb::new("promoted");
    let declaration = cfg(100.0, 0.0, None);
    let clock = ManualClock::new(9);
    let (mut gate, _) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap();
    gate.begin_canary("v2", 1, pred_always()).unwrap();
    gate.try_admit(&item(1, "v2", 3.0), 1).unwrap();
    gate.complete(1, 2, 4).unwrap();
    let events = gate.drain_events();
    assert!(events.iter().any(|event| matches!(
        event,
        GateEvent::CanaryPromoted {
            probes_released: 1,
            ..
        }
    )));
    assert!(gate.canary_cursor().unwrap().open);
    assert_eq!(gate.canary_cursor().unwrap().released, 1);
    drop(gate);

    let (mut gate, _) =
        DurableGate::open_with_clock(tmp.path(), &declaration, Some(pred_always()), clock).unwrap();
    assert!(
        gate.drain_events().is_empty(),
        "restart does not replay promotion"
    );
    let cursor = gate.canary_cursor().unwrap();
    assert!(cursor.open);
    assert_eq!(cursor.released, 1);
    assert_eq!(cursor.completed, 1);
    assert_eq!(cursor.defects, 4);
    assert_eq!(cursor.max_probes, 1);
    gate.try_admit(&item(2, "v2", 3.0), 3).unwrap();
    let cursor = gate.canary_cursor().unwrap();
    assert!(cursor.open);
    assert_eq!(cursor.released, 1, "promotion freezes the probe count");
    assert_books(&gate, 3.0, 1);
}

#[test]
fn night_throttle_survives_restart() {
    let tmp = TmpDb::new("night");
    let declaration = cfg(
        10.0,
        0.0,
        Some(NightWindow {
            start: 0,
            end: 100,
            ratio: 0.25,
        }),
    );
    let clock = ManualClock::new(3);
    let (mut gate, _) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap();
    gate.try_admit(&item(1, "v1", 1.0), 0).unwrap();
    let throttled = gate.try_admit(&item(2, "v1", 1.0), 0).unwrap_err();
    assert!(matches!(
        throttled,
        PersistError::Admit(AdmitError::NightThrottled { .. })
    ));
    assert_eq!(gate.night_stats(), (2, 1));
    assert_eq!(gate.night_view().credit.to_bits(), 0.5f64.to_bits());
    assert_books(&gate, 1.0, 1);
    drop(gate);

    let (mut gate, _) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock).unwrap();
    assert_eq!(gate.night_view().credit.to_bits(), 0.5f64.to_bits());
    assert_eq!(gate.night_view().prev_in_window, Some(true));
    assert_eq!(gate.night_stats(), (2, 1));
    let throttled = gate.try_admit(&item(3, "v1", 1.0), 0).unwrap_err();
    assert!(matches!(
        throttled,
        PersistError::Admit(AdmitError::NightThrottled { .. })
    ));
    assert_books(&gate, 1.0, 1);
    assert_eq!(gate.night_stats(), (3, 1));
    assert!(gate
        .in_process()
        .iter()
        .all(|item| item.id != 2 && item.id != 3));
}

#[test]
fn night_wip_cap_persists_spent_credit_without_the_item() {
    let tmp = TmpDb::new("night-wip");
    let declaration = cfg(
        10.0,
        0.0,
        Some(NightWindow {
            start: 0,
            end: 60,
            ratio: 0.5,
        }),
    );
    let clock = ManualClock::new(11);
    let (mut gate, _) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap();
    let rejected = gate.try_admit(&item(1, "v1", 11.0), 0).unwrap_err();
    assert!(matches!(
        rejected,
        PersistError::Admit(AdmitError::WipCap { .. })
    ));
    assert_eq!(gate.in_process_count(), 0);
    assert_eq!(gate.night_stats(), (1, 1));
    assert_eq!(gate.night_view().credit.to_bits(), 0.5f64.to_bits());
    drop(gate);

    let (mut gate, _) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock).unwrap();
    assert_eq!(gate.in_process_count(), 0);
    assert_eq!(gate.night_stats(), (1, 1));
    assert_eq!(gate.night_view().credit.to_bits(), 0.5f64.to_bits());
    gate.try_admit(&item(2, "v1", 1.0), 0).unwrap();
    let throttled = gate.try_admit(&item(3, "v1", 1.0), 0).unwrap_err();
    assert!(matches!(
        throttled,
        PersistError::Admit(AdmitError::NightThrottled { .. })
    ));
    assert_books(&gate, 1.0, 1);
}

#[test]
fn night_window_edge_survives_restart() {
    let tmp = TmpDb::new("edge");
    let declaration = cfg(
        10.0,
        0.0,
        Some(NightWindow {
            start: 0,
            end: 30,
            ratio: 0.5,
        }),
    );
    let clock = ManualClock::new(4);
    let (mut gate, _) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap();
    gate.try_admit(&item(1, "v1", 1.0), 0).unwrap();
    assert!(
        gate.drain_events().is_empty(),
        "first observation is silent"
    );
    drop(gate);

    let (mut gate, _) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock).unwrap();
    assert!(gate.drain_events().is_empty());
    gate.try_admit(&item(2, "v1", 1.0), 30).unwrap();
    let events = gate.drain_events();
    assert!(events
        .iter()
        .any(|event| matches!(event, GateEvent::NightWindowExit { at: 30 })));
    assert_eq!(
        gate.night_stats(),
        (0, 0),
        "leaving the window resets the counters"
    );
    assert_eq!(gate.night_view().prev_in_window, Some(false));
    assert_books(&gate, 2.0, 2);
}

#[test]
fn refill_latch_survives_restart() {
    let tmp = TmpDb::new("refill");
    let declaration = cfg(30.0, 10.0, None);
    let clock = ManualClock::new(8);
    let (mut gate, _) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap();
    gate.try_admit(&item(1, "v1", 15.0), 0).unwrap();
    gate.complete(1, 1, 0).unwrap();
    assert!(gate.refill_needed());
    let events = gate.drain_events();
    assert!(events
        .iter()
        .any(|event| matches!(event, GateEvent::NeedRefill { .. })));
    drop(gate);

    let (mut gate, _) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock).unwrap();
    assert!(gate.drain_events().is_empty());
    assert!(gate.refill_needed());
    assert_books(&gate, 0.0, 0);
    gate.try_admit(&item(2, "v1", 10.0), 2).unwrap();
    let events = gate.drain_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GateEvent::RefillSatisfied { .. })),
        "lost latch would skip RefillSatisfied, events={events:?}"
    );
    assert!(!gate.refill_needed());
    assert_books(&gate, 10.0, 1);
}

#[test]
fn predicate_presence_must_match_persisted_canary() {
    let tmp = TmpDb::new("pred");
    let declaration = cfg(10.0, 0.0, None);
    let clock = ManualClock::new(1);
    let err =
        DurableGate::open_with_clock(tmp.path(), &declaration, Some(pred_never()), clock.clone())
            .unwrap_err();
    assert!(matches!(err, PersistError::CanaryPredicateUnexpected));

    let (gate, outcome) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap();
    assert_eq!(outcome, OpenOutcome::Created);
    drop(gate);

    let err =
        DurableGate::open_with_clock(tmp.path(), &declaration, Some(pred_never()), clock.clone())
            .unwrap_err();
    assert!(matches!(err, PersistError::CanaryPredicateUnexpected));
    let (mut gate, outcome) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap();
    assert_eq!(outcome, OpenOutcome::AlreadyDeclared);
    gate.begin_canary("v2", 1, pred_never()).unwrap();
    drop(gate);

    let err =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap_err();
    assert!(matches!(err, PersistError::CanaryPredicateRequired));
    let (gate, outcome) =
        DurableGate::open_with_clock(tmp.path(), &declaration, Some(pred_never()), clock).unwrap();
    assert_eq!(outcome, OpenOutcome::AlreadyDeclared);
    let cursor = gate.canary_cursor().unwrap();
    assert_eq!(cursor.config, "v2");
    assert_eq!(cursor.max_probes, 1);
    assert!(!cursor.open);
    assert_eq!(cursor.released, 0);
}

/// F1：admit 0.1 与 0.2，再 complete 掉 0.1。增量累加的 wip 与在制重求和差 1 ULP，重开被判损坏。
#[test]
fn fractional_wip_survives_reopen_after_partial_complete() {
    let tmp = TmpDb::new("frac-wip");
    let declaration = cfg(10.0, 0.0, None);
    let clock = ManualClock::new(50);
    let (mut gate, outcome) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap();
    assert_eq!(outcome, OpenOutcome::Created);
    gate.try_admit(&item(1, "v1", 0.1), 1).unwrap();
    gate.try_admit(&item(2, "v1", 0.2), 2).unwrap();
    gate.complete(1, 3, 0).unwrap();
    let before_bits = gate.wip_minutes().to_bits();
    let remaining = gate.in_process();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, 2);
    let stored_weight_bits = remaining[0].weight.to_bits();
    assert_eq!(stored_weight_bits, 0.2f64.to_bits());
    drop(gate);

    let (gate, outcome) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock).unwrap();
    assert_eq!(outcome, OpenOutcome::AlreadyDeclared);
    assert_eq!(
        gate.wip_minutes().to_bits(),
        before_bits,
        "reopen changed wip"
    );
    assert_eq!(
        gate.wip_minutes().to_bits(),
        stored_weight_bits,
        "wip is not the remaining item"
    );
    assert_books(&gate, 0.2, 1);
    assert_eq!(gate.in_process()[0].id, 2);
    assert_eq!(gate.in_process()[0].admitted_at, 2);
}

#[test]
fn corrupt_rows_fail_closed() {
    let declaration = cfg(20.0, 0.0, None);

    let tmp = TmpDb::new("corrupt-wip");
    let (mut gate, _) = DurableGate::open(tmp.path(), &declaration, None).unwrap();
    gate.try_admit(&item(1, "v1", 5.0), 0).unwrap();
    drop(gate);
    let conn = rusqlite::Connection::open(tmp.path()).unwrap();
    conn.execute("UPDATE gate_meta SET wip_bits = 0 WHERE id = 1", [])
        .unwrap();
    drop(conn);
    let err = DurableGate::open(tmp.path(), &declaration, None).unwrap_err();
    assert!(matches!(err, PersistError::Corrupt { .. }), "{err:?}");

    let tmp = TmpDb::new("corrupt-canary");
    let (mut gate, _) = DurableGate::open(tmp.path(), &declaration, None).unwrap();
    gate.begin_canary("v2", 1, pred_never()).unwrap();
    drop(gate);
    let conn = rusqlite::Connection::open(tmp.path()).unwrap();
    conn.execute("UPDATE canary SET released = 9 WHERE id = 1", [])
        .unwrap();
    drop(conn);
    let err = DurableGate::open(tmp.path(), &declaration, Some(pred_never())).unwrap_err();
    assert!(matches!(err, PersistError::Corrupt { .. }), "{err:?}");
}

#[test]
fn wall_stamp_follows_injected_clock_and_reopen_does_not_rewrite() {
    let tmp = TmpDb::new("stamp");
    let clock = ManualClock::new(5_000);
    let declaration = cfg(10.0, 0.0, None);
    let (mut gate, _) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap();
    assert_eq!(gate.updated_wall_ms(), 5_000);
    clock.advance(10);
    let err = gate.complete(99, 1, 0).unwrap_err();
    assert!(matches!(
        err,
        PersistError::Gate(GateError::UnknownItem(99))
    ));
    assert_eq!(gate.updated_wall_ms(), 5_000);
    drop(gate);

    let (mut gate, _) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, clock.clone()).unwrap();
    assert_eq!(gate.updated_wall_ms(), 5_000);
    clock.advance(2_990);
    assert_eq!(clock.wall_ms(), 8_000);
    gate.try_admit(&item(1, "v1", 1.0), 4).unwrap();
    assert_eq!(gate.updated_wall_ms(), 8_000);
    drop(gate);

    let (gate, _) =
        DurableGate::open_with_clock(tmp.path(), &declaration, None, ManualClock::new(1)).unwrap();
    assert_eq!(gate.updated_wall_ms(), 8_000);
    assert_eq!(gate.in_process()[0].admitted_at, 4);
    assert_books(&gate, 1.0, 1);
}
