//! 指纹账本的重启恢复。写完、丢掉连接、用同一声明再打开，状态要原样可读。
//! 时刻全部来自 [`ManualClock`]，不睡眠。

use af_stop::{
    Clock, Decision, Error, Event, Fingerprint, FingerprintAction, FingerprintEntry,
    FingerprintLedger, FingerprintStore, LedgerDeclaration, ManualClock, OpenStatus, StopEngine,
    StopPolicy, StopReason,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static SEQ: AtomicU64 = AtomicU64::new(0);

struct TempDb {
    path: PathBuf,
}

impl TempDb {
    fn new() -> Self {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("af-stop-restart-{}-{n}.db", std::process::id()));
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let path = if suffix.is_empty() {
                self.path.clone()
            } else {
                PathBuf::from(format!("{}{suffix}", self.path.display()))
            };
            let _ = std::fs::remove_file(path);
        }
    }
}

fn tool(name: &str, code: &str) -> Event {
    Event::ToolError {
        tool: name.into(),
        error_code: code.into(),
    }
}

fn policy(streak: usize, threshold: usize, store: FingerprintStore) -> StopPolicy {
    StopPolicy {
        fingerprint_streak_limit: streak,
        normal_error_threshold: threshold,
        store,
    }
}

fn decl_of(
    streak: u64,
    threshold: u64,
    window_ms: u64,
    store: FingerprintStore,
) -> LedgerDeclaration {
    LedgerDeclaration::new(streak, threshold, window_ms, store)
}

#[test]
fn reopen_reports_already_declared_and_keeps_the_declaration() {
    let db = TempDb::new();
    let clock = ManualClock::new(0, 1_700_000_000_000);
    let sample = decl_of(3, 5, 1_000, FingerprintStore::new());
    let (ledger, status) =
        FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock.clone()).unwrap();
    assert_eq!(status, OpenStatus::Created);
    assert_eq!(ledger.declaration(), &sample);
    assert_eq!(ledger.normal_error_count(), 0);
    assert_eq!(ledger.current_streak(), None);
    assert!(ledger.records().unwrap().is_empty());
    drop(ledger);

    let (ledger, status) =
        FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock).unwrap();
    assert_eq!(status, OpenStatus::AlreadyDeclared);
    assert_eq!(ledger.declaration(), &sample);
    assert!(ledger.records().unwrap().is_empty());
}

#[test]
fn unknown_declaration_version_does_not_initialize() {
    let db = TempDb::new();
    let clock = ManualClock::new(0, 1_700_000_000_000);
    let mut bad = decl_of(3, 5, 1_000, FingerprintStore::new());
    bad.v = 2;
    let err = FingerprintLedger::open_with_clock(db.path(), bad, clock.clone()).unwrap_err();
    assert!(
        matches!(
            err,
            Error::UnknownVersion {
                found: 2,
                expected: 1
            }
        ),
        "{err}"
    );

    let sample = decl_of(3, 5, 1_000, FingerprintStore::new());
    let (_, status) = FingerprintLedger::open_with_clock(db.path(), sample, clock).unwrap();
    assert_eq!(status, OpenStatus::Created);
}

#[test]
fn declaration_mismatch_is_fail_closed_and_preserves_rows() {
    let db = TempDb::new();
    let clock = ManualClock::new(0, 1_700_000_000_000);
    let sample = decl_of(3, 5, 1_000, FingerprintStore::new());
    let (mut ledger, _) =
        FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock.clone()).unwrap();
    ledger.feed(tool("search", "E_TIMEOUT")).unwrap();
    ledger.feed(tool("search", "E_TIMEOUT")).unwrap();
    drop(ledger);

    for (label, other) in [
        ("streak", decl_of(9, 5, 1_000, FingerprintStore::new())),
        ("threshold", decl_of(3, 9, 1_000, FingerprintStore::new())),
        ("window", decl_of(3, 5, 9, FingerprintStore::new())),
        (
            "store",
            decl_of(3, 5, 1_000, {
                let mut store = FingerprintStore::new();
                store.register(
                    Fingerprint::new("search", "E_TIMEOUT"),
                    FingerprintEntry::default(),
                );
                store
            }),
        ),
    ] {
        let err = FingerprintLedger::open_with_clock(db.path(), other, clock.clone()).unwrap_err();
        assert!(
            matches!(err, Error::DeclarationMismatch { .. }),
            "{label}: {err}"
        );
    }

    let (ledger, status) = FingerprintLedger::open_with_clock(db.path(), sample, clock).unwrap();
    assert_eq!(status, OpenStatus::AlreadyDeclared);
    let row = ledger
        .record(&Fingerprint::new("search", "E_TIMEOUT"))
        .unwrap()
        .unwrap();
    assert_eq!(row.count, 2);
    assert_eq!(row.disposition, Decision::Continue);
}

#[test]
fn store_registration_order_matches_on_reentry() {
    let db = TempDb::new();
    let clock = ManualClock::new(0, 1_700_000_000_000);
    let mut first = FingerprintStore::new();
    first.register(
        Fingerprint::new("b", "2"),
        FingerprintEntry {
            streak_limit: Some(0),
            action: FingerprintAction::StopNoInfluence,
        },
    );
    first.register(
        Fingerprint::new("a", "1"),
        FingerprintEntry {
            streak_limit: Some(1),
            action: FingerprintAction::Escalate,
        },
    );
    let mut second = FingerprintStore::new();
    second.register(
        Fingerprint::new("a", "1"),
        FingerprintEntry {
            streak_limit: Some(1),
            action: FingerprintAction::Escalate,
        },
    );
    second.register(
        Fingerprint::new("b", "2"),
        FingerprintEntry {
            streak_limit: Some(0),
            action: FingerprintAction::StopNoInfluence,
        },
    );
    let (ledger, status) =
        FingerprintLedger::open_with_clock(db.path(), decl_of(3, 5, 1_000, first), clock.clone())
            .unwrap();
    assert_eq!(status, OpenStatus::Created);
    drop(ledger);
    let (_, status) =
        FingerprintLedger::open_with_clock(db.path(), decl_of(3, 5, 1_000, second), clock).unwrap();
    assert_eq!(status, OpenStatus::AlreadyDeclared);
}

#[test]
fn fingerprint_row_roundtrips_across_restart() {
    let db = TempDb::new();
    let clock = ManualClock::new(0, 1_700_000_000_000);
    let sample = LedgerDeclaration::from_policy(&StopPolicy::default(), 5_000).unwrap();
    let (mut ledger, status) =
        FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock.clone()).unwrap();
    assert_eq!(status, OpenStatus::Created);
    let decision = ledger.feed(tool("search", "E_TIMEOUT")).unwrap();
    assert_eq!(decision, Decision::Continue);
    let row = ledger
        .record(&Fingerprint::new("search", "E_TIMEOUT"))
        .unwrap()
        .unwrap();
    assert_eq!(row.v, 1);
    assert_eq!(row.count, 1);
    assert_eq!(row.disposition, Decision::Continue);
    assert_eq!(row.window_start_ms, 0);
    assert_eq!(row.window_end_ms, 5_000);
    assert_eq!(row.observed_wall_ms, clock.wall_ms());
    assert!(row.to_json().unwrap().contains("\"v\":1"));
    let bad = row.to_json().unwrap().replacen("\"v\":1", "\"v\":7", 1);
    let err = af_stop::FingerprintRecord::from_json(&bad).unwrap_err();
    assert!(
        matches!(
            err,
            Error::UnknownVersion {
                found: 7,
                expected: 1
            }
        ),
        "{err}"
    );
    drop(ledger);

    let (ledger, status) = FingerprintLedger::open_with_clock(db.path(), sample, clock).unwrap();
    assert_eq!(status, OpenStatus::AlreadyDeclared);
    let again = ledger
        .record(&Fingerprint::new("search", "E_TIMEOUT"))
        .unwrap()
        .unwrap();
    assert_eq!(again, row);
}

#[test]
fn engine_counters_roundtrip_across_restart() {
    let db = TempDb::new();
    let clock = ManualClock::new(0, 1_700_000_000_000);
    let sample = decl_of(3, 4, 60_000, FingerprintStore::new());
    let (mut ledger, _) =
        FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock.clone()).unwrap();
    assert_eq!(
        ledger.feed(tool("search", "E_TIMEOUT")).unwrap(),
        Decision::Continue
    );
    assert_eq!(
        ledger.feed(tool("search", "E_TIMEOUT")).unwrap(),
        Decision::Continue
    );
    assert_eq!(
        ledger
            .feed(Event::ToolOk {
                tool: "search".into(),
            })
            .unwrap(),
        Decision::Continue
    );
    assert_eq!(
        ledger
            .feed(Event::GenericError {
                source: "llm".into(),
                error_code: "E_5XX".into(),
            })
            .unwrap(),
        Decision::Continue
    );
    assert_eq!(ledger.current_streak(), None);
    assert_eq!(ledger.normal_error_count(), 3);
    drop(ledger);

    let (mut ledger, status) =
        FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock.clone()).unwrap();
    assert_eq!(status, OpenStatus::AlreadyDeclared);
    assert_eq!(ledger.current_streak(), None);
    assert_eq!(ledger.normal_error_count(), 3);
    assert_eq!(
        ledger
            .record(&Fingerprint::new("search", "E_TIMEOUT"))
            .unwrap()
            .unwrap()
            .count,
        2
    );
    assert_eq!(
        ledger
            .feed(Event::CheckFailed {
                node: "schema_check".into(),
            })
            .unwrap(),
        Decision::Stop {
            reason: StopReason::CheckFirstFail,
        }
    );
    assert_eq!(ledger.normal_error_count(), 3);
    assert_eq!(ledger.current_streak(), None);
    drop(ledger);

    let (mut ledger, _) = FingerprintLedger::open_with_clock(db.path(), sample, clock).unwrap();
    assert_eq!(ledger.normal_error_count(), 3);
    assert_eq!(ledger.current_streak(), None);
    // 连击从 1 重新数，普通错误到 4，规则③升级。规则①不抢这次裁决。
    assert_eq!(
        ledger.feed(tool("search", "E_TIMEOUT")).unwrap(),
        Decision::Escalate {
            reason: af_stop::EscalateReason::NormalErrorThreshold,
        }
    );
    assert_eq!(
        ledger
            .record(&Fingerprint::new("search", "E_TIMEOUT"))
            .unwrap()
            .unwrap()
            .count,
        3
    );
}

#[test]
fn fingerprint_stop_still_beats_threshold_after_restart() {
    let db = TempDb::new();
    let clock = ManualClock::new(0, 1_700_000_000_000);
    let sample = decl_of(3, 3, 60_000, FingerprintStore::new());
    let (mut ledger, _) =
        FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock.clone()).unwrap();
    assert_eq!(
        ledger.feed(tool("db", "E_LOCK")).unwrap(),
        Decision::Continue
    );
    assert_eq!(
        ledger.feed(tool("db", "E_LOCK")).unwrap(),
        Decision::Continue
    );
    drop(ledger);
    let (mut ledger, _) = FingerprintLedger::open_with_clock(db.path(), sample, clock).unwrap();
    assert_eq!(
        ledger.feed(tool("db", "E_LOCK")).unwrap(),
        Decision::Stop {
            reason: StopReason::NoInfluence,
        }
    );
    assert_eq!(
        ledger
            .record(&Fingerprint::new("db", "E_LOCK"))
            .unwrap()
            .unwrap()
            .count,
        3
    );
}

#[test]
fn window_roll_keeps_count_and_survives_restart() {
    let db = TempDb::new();
    let clock = ManualClock::new(0, 1_700_000_000_000);
    let sample = decl_of(3, 100, 1_000, FingerprintStore::new());
    let (mut ledger, _) =
        FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock.clone()).unwrap();
    ledger.feed(tool("search", "E_TIMEOUT")).unwrap();
    let first = ledger
        .record(&Fingerprint::new("search", "E_TIMEOUT"))
        .unwrap()
        .unwrap();
    assert_eq!(
        (first.window_start_ms, first.window_end_ms, first.count),
        (0, 1_000, 1)
    );

    clock.advance(Duration::from_millis(999));
    ledger.feed(tool("search", "E_TIMEOUT")).unwrap();
    let inside = ledger
        .record(&Fingerprint::new("search", "E_TIMEOUT"))
        .unwrap()
        .unwrap();
    assert_eq!(
        (inside.window_start_ms, inside.window_end_ms, inside.count),
        (0, 1_000, 2)
    );

    clock.advance(Duration::from_millis(1));
    ledger.feed(tool("search", "E_TIMEOUT")).unwrap();
    let rolled = ledger
        .record(&Fingerprint::new("search", "E_TIMEOUT"))
        .unwrap()
        .unwrap();
    assert_eq!(
        (rolled.window_start_ms, rolled.window_end_ms, rolled.count),
        (1_000, 2_000, 3)
    );
    assert!(rolled.count > inside.count);
    drop(ledger);

    let (mut ledger, status) =
        FingerprintLedger::open_with_clock(db.path(), sample, clock.clone()).unwrap();
    assert_eq!(status, OpenStatus::AlreadyDeclared);
    let restored = ledger
        .record(&Fingerprint::new("search", "E_TIMEOUT"))
        .unwrap()
        .unwrap();
    assert_eq!(restored, rolled);
    assert_eq!(ledger.logical_now(), 1_000);

    clock.advance(Duration::from_millis(1_000));
    ledger.feed(tool("search", "E_TIMEOUT")).unwrap();
    let next = ledger
        .record(&Fingerprint::new("search", "E_TIMEOUT"))
        .unwrap()
        .unwrap();
    assert_eq!(
        (next.window_start_ms, next.window_end_ms, next.count),
        (2_000, 3_000, 4)
    );
}

#[test]
fn wall_clock_does_not_drive_expiry_or_drop_the_count() {
    let db = TempDb::new();
    let clock = ManualClock::new(0, 5_000);
    let sample = decl_of(3, 100, 1_000, FingerprintStore::new());
    let (mut ledger, _) =
        FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock.clone()).unwrap();
    ledger.feed(tool("search", "E_TIMEOUT")).unwrap();
    clock.shift_wall_ms(-4_000);
    ledger.feed(tool("search", "E_TIMEOUT")).unwrap();
    let row = ledger
        .record(&Fingerprint::new("search", "E_TIMEOUT"))
        .unwrap()
        .unwrap();
    assert_eq!(row.count, 2);
    assert_eq!(row.window_start_ms, 0);
    assert_eq!(row.window_end_ms, 1_000);
    assert_eq!(row.observed_wall_ms, 1_000);
    assert_eq!(Clock::monotonic_ms(&clock), 0);
    drop(ledger);

    let (ledger, _) = FingerprintLedger::open_with_clock(db.path(), sample, clock).unwrap();
    let again = ledger
        .record(&Fingerprint::new("search", "E_TIMEOUT"))
        .unwrap()
        .unwrap();
    assert_eq!(again, row);
}

#[test]
fn advance_while_closed_does_not_expire_the_window_or_drop_the_count() {
    let db = TempDb::new();
    let clock = ManualClock::new(0, 1_700_000_000_000);
    let sample = decl_of(3, 100, 1_000, FingerprintStore::new());
    let (mut ledger, _) =
        FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock.clone()).unwrap();
    ledger.feed(tool("search", "E_TIMEOUT")).unwrap();
    let before = ledger
        .record(&Fingerprint::new("search", "E_TIMEOUT"))
        .unwrap()
        .unwrap();
    drop(ledger);

    clock.advance(Duration::from_millis(10_000));
    let (ledger, status) = FingerprintLedger::open_with_clock(db.path(), sample, clock).unwrap();
    assert_eq!(status, OpenStatus::AlreadyDeclared);
    let after = ledger
        .record(&Fingerprint::new("search", "E_TIMEOUT"))
        .unwrap()
        .unwrap();
    assert_eq!(after, before);
    assert_eq!(after.count, 1);
    assert_eq!(ledger.logical_now(), 0);
}

#[test]
fn ledger_feed_matches_the_pure_engine_across_restart() {
    let mut store = FingerprintStore::new();
    store.register(
        Fingerprint::new("deploy", "E_LOCKED"),
        FingerprintEntry {
            streak_limit: Some(0),
            ..FingerprintEntry::default()
        },
    );
    let rules = policy(3, 100, store);
    let sample = LedgerDeclaration::from_policy(&rules, 60_000).unwrap();
    let db = TempDb::new();
    let clock = ManualClock::new(0, 1_700_000_000_000);
    let (mut ledger, _) =
        FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock.clone()).unwrap();
    let mut control = StopEngine::new(rules);
    let events = [
        tool("deploy", "E_LOCKED"),
        tool("deploy", "E_LOCKED"),
        tool("search", "E_TIMEOUT"),
        tool("search", "E_TIMEOUT"),
        Event::ToolOk {
            tool: "search".into(),
        },
    ];
    for event in &events {
        assert_eq!(
            ledger.feed(event.clone()).unwrap(),
            control.feed(event.clone())
        );
    }
    let mid_deploy = ledger
        .record(&Fingerprint::new("deploy", "E_LOCKED"))
        .unwrap()
        .unwrap()
        .count;
    let mid_search = ledger
        .record(&Fingerprint::new("search", "E_TIMEOUT"))
        .unwrap()
        .unwrap()
        .count;
    assert_eq!((mid_deploy, mid_search), (2, 2));
    drop(ledger);

    let (mut ledger, status) =
        FingerprintLedger::open_with_clock(db.path(), sample, clock).unwrap();
    assert_eq!(status, OpenStatus::AlreadyDeclared);
    assert_eq!(
        ledger
            .record(&Fingerprint::new("deploy", "E_LOCKED"))
            .unwrap()
            .unwrap()
            .count,
        mid_deploy
    );
    let rest = [
        tool("search", "E_TIMEOUT"),
        tool("search", "E_TIMEOUT"),
        tool("search", "E_TIMEOUT"),
        tool("deploy", "E_LOCKED"),
    ];
    for event in &rest {
        assert_eq!(
            ledger.feed(event.clone()).unwrap(),
            control.feed(event.clone())
        );
    }
    assert_eq!(
        ledger
            .record(&Fingerprint::new("deploy", "E_LOCKED"))
            .unwrap()
            .unwrap()
            .count,
        3
    );
    assert_eq!(
        ledger
            .record(&Fingerprint::new("search", "E_TIMEOUT"))
            .unwrap()
            .unwrap()
            .count,
        5
    );
    assert!(
        ledger
            .record(&Fingerprint::new("deploy", "E_LOCKED"))
            .unwrap()
            .unwrap()
            .count
            >= mid_deploy
    );
    assert!(
        ledger
            .record(&Fingerprint::new("search", "E_TIMEOUT"))
            .unwrap()
            .unwrap()
            .count
            >= mid_search
    );
}

#[test]
fn whitelisted_fingerprint_stays_continue_after_restart() {
    let mut store = FingerprintStore::new();
    store.register(
        Fingerprint::new("deploy", "E_LOCKED"),
        FingerprintEntry {
            streak_limit: Some(0),
            ..FingerprintEntry::default()
        },
    );
    let rules = policy(3, 100, store);
    let sample = LedgerDeclaration::from_policy(&rules, 60_000).unwrap();
    let db = TempDb::new();
    let clock = ManualClock::new(0, 1_700_000_000_000);
    let (mut ledger, _) =
        FingerprintLedger::open_with_clock(db.path(), sample.clone(), clock.clone()).unwrap();
    let mut control = StopEngine::new(rules);
    for _ in 0..2 {
        let event = tool("deploy", "E_LOCKED");
        assert_eq!(ledger.feed(event.clone()).unwrap(), control.feed(event));
    }
    drop(ledger);
    let (mut ledger, _) = FingerprintLedger::open_with_clock(db.path(), sample, clock).unwrap();
    for _ in 0..3 {
        let event = tool("deploy", "E_LOCKED");
        let decision = ledger.feed(event.clone()).unwrap();
        assert_eq!(decision, control.feed(event));
        assert_eq!(decision, Decision::Continue);
    }
    assert_eq!(
        ledger
            .record(&Fingerprint::new("deploy", "E_LOCKED"))
            .unwrap()
            .unwrap()
            .count,
        5
    );
}
