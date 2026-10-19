//! 账本：钟面窗口、幂等扣减、outbox、重启、多连接守恒。
//!
//! 时刻全部注入。测试不睡眠。

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};

use chrono::{Datelike, Days, NaiveDate, NaiveTime, TimeZone, Weekday};
use chrono_tz::Tz;

use af_meter::{
    window_bounds, ConsumeResult, Declaration, Ledger, LedgerError, Opened, WindowKind,
    SCHEMA_VERSION,
};

fn sample(capacity: u64) -> Declaration {
    Declaration {
        meter_id: "subscription".into(),
        capacity,
        timezone: "Europe/Berlin".into(),
        kind: WindowKind::Day,
    }
}

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("af-meter-{label}-{}-{n}.db", std::process::id()));
        Self { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let mut name = self.path.clone().into_os_string();
            name.push(suffix);
            let _ = std::fs::remove_file(name);
        }
    }
}

fn at_civil(tz_name: &str, y: i32, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> i64 {
    let tz: Tz = tz_name.parse().unwrap();
    let naive = NaiveDate::from_ymd_opt(y, m, d)
        .unwrap()
        .and_hms_opt(hh, mm, ss)
        .unwrap();
    tz.from_local_datetime(&naive)
        .earliest()
        .unwrap_or_else(|| {
            panic!("civil {y}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02} missing in {tz_name}")
        })
        .timestamp_millis()
}

fn assert_civil_midnight(tz_name: &str, ms: i64, y: i32, m: u32, d: u32) {
    let tz: Tz = tz_name.parse().unwrap();
    let dt = tz
        .timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(|| panic!("{ms} is not a single instant in {tz_name}"));
    assert_eq!(
        dt.date_naive(),
        NaiveDate::from_ymd_opt(y, m, d).unwrap(),
        "{tz_name} {ms}"
    );
    assert_eq!(dt.time(), NaiveTime::from_hms_opt(0, 0, 0).unwrap());
}

fn open_created(path: &std::path::Path, decl: &Declaration) -> Ledger {
    match Ledger::open(path, decl).unwrap() {
        Opened::Created(ledger) => ledger,
        Opened::AlreadyDeclared(_) => panic!("expected Created"),
    }
}

fn open_again(path: &std::path::Path, decl: &Declaration) -> Ledger {
    match Ledger::open(path, decl).unwrap() {
        Opened::AlreadyDeclared(ledger) => ledger,
        Opened::Created(_) => panic!("expected AlreadyDeclared"),
    }
}

#[test]
fn spring_forward_day_ends_at_next_civil_midnight() {
    let tz = "Europe/Berlin";
    let noon = at_civil(tz, 2026, 3, 29, 12, 0, 0);
    let (start, end) = window_bounds(noon, tz, WindowKind::Day).unwrap();
    assert_civil_midnight(tz, start, 2026, 3, 29);
    assert_civil_midnight(tz, end, 2026, 3, 30);
    assert!(
        end - start < 86_400_000,
        "spring-forward civil day is shorter than 86400000 ms, delta={}",
        end - start
    );
    let zone: Tz = tz.parse().unwrap();
    let start_date = zone
        .timestamp_millis_opt(start)
        .single()
        .unwrap()
        .date_naive();
    let end_date = zone
        .timestamp_millis_opt(end)
        .single()
        .unwrap()
        .date_naive();
    assert_eq!(end_date, start_date.checked_add_days(Days::new(1)).unwrap());
}

#[test]
fn fall_back_day_ends_at_next_civil_midnight() {
    let tz = "Europe/Berlin";
    let noon = at_civil(tz, 2026, 10, 25, 12, 0, 0);
    let (start, end) = window_bounds(noon, tz, WindowKind::Day).unwrap();
    assert_civil_midnight(tz, start, 2026, 10, 25);
    assert_civil_midnight(tz, end, 2026, 10, 26);
    assert!(
        end - start > 86_400_000,
        "fall-back civil day is longer than 86400000 ms, delta={}",
        end - start
    );
}

#[test]
fn week_crossing_spring_forward_uses_civil_mondays() {
    let tz = "Europe/Berlin";
    let sunday = at_civil(tz, 2026, 3, 29, 12, 0, 0);
    let (start, end) = window_bounds(sunday, tz, WindowKind::Week).unwrap();
    assert_civil_midnight(tz, start, 2026, 3, 23);
    assert_civil_midnight(tz, end, 2026, 3, 30);
    assert_ne!(end - start, 7 * 86_400_000);
    let zone: Tz = tz.parse().unwrap();
    let start_dt = zone.timestamp_millis_opt(start).single().unwrap();
    let end_dt = zone.timestamp_millis_opt(end).single().unwrap();
    assert_eq!(start_dt.weekday(), Weekday::Mon);
    assert_eq!(end_dt.weekday(), Weekday::Mon);
    assert_eq!(
        end_dt.date_naive(),
        start_dt
            .date_naive()
            .checked_add_days(Days::new(7))
            .unwrap()
    );
}

#[test]
fn ordinary_day_boundary_is_the_next_civil_midnight() {
    let tz = "Europe/Berlin";
    let noon = at_civil(tz, 2026, 6, 15, 12, 0, 0);
    let (start, end) = window_bounds(noon, tz, WindowKind::Day).unwrap();
    assert_civil_midnight(tz, start, 2026, 6, 15);
    assert_civil_midnight(tz, end, 2026, 6, 16);
    assert_eq!(end - start, 86_400_000);
    let (next_start, _) = window_bounds(end, tz, WindowKind::Day).unwrap();
    assert_eq!(next_start, end);
    let (still, _) = window_bounds(end - 1, tz, WindowKind::Day).unwrap();
    assert_eq!(still, start);
}

#[test]
fn same_instant_falls_on_different_civil_days() {
    let instant = at_civil("UTC", 2026, 6, 15, 23, 30, 0);
    let (utc_start, _) = window_bounds(instant, "UTC", WindowKind::Day).unwrap();
    let (shanghai_start, _) = window_bounds(instant, "Asia/Shanghai", WindowKind::Day).unwrap();
    assert_civil_midnight("UTC", utc_start, 2026, 6, 15);
    assert_civil_midnight("Asia/Shanghai", shanghai_start, 2026, 6, 16);
    assert_ne!(utc_start, shanghai_start);
}

#[test]
fn bad_declaration_is_rejected_before_create() {
    let scratch = Scratch::new("bad-decl");
    let mut decl = sample(10);
    decl.timezone = "Not/AZone".into();
    let err = Ledger::open(&scratch.path, &decl).unwrap_err();
    assert!(matches!(err, LedgerError::BadDeclaration("timezone")));
    assert!(!scratch.path.exists());

    decl.timezone = "Europe/Berlin".into();
    decl.meter_id.clear();
    let err = Ledger::open(&scratch.path, &decl).unwrap_err();
    assert!(matches!(err, LedgerError::BadDeclaration("meter_id")));

    decl.meter_id = "subscription".into();
    decl.capacity = (i64::MAX as u64).saturating_add(1);
    let err = Ledger::open(&scratch.path, &decl).unwrap_err();
    assert!(matches!(err, LedgerError::BadDeclaration("capacity")));
    assert!(!scratch.path.exists());
}

#[test]
fn open_created_then_reopen_checks_recipe_and_declaration() {
    let scratch = Scratch::new("open");
    let decl = sample(10);
    let ledger = open_created(&scratch.path, &decl);
    let recipe = ledger.recipe().unwrap();
    assert_eq!(recipe.journal_mode, "wal");
    assert_eq!(recipe.synchronous, 1);
    assert_eq!(recipe.foreign_keys, 1);
    assert_eq!(recipe.busy_timeout_ms, 5_000);
    assert_eq!(recipe.user_version, SCHEMA_VERSION);
    assert_eq!(ledger.declaration(), &decl);
    drop(ledger);

    let raw = rusqlite::Connection::open(&scratch.path).unwrap();
    let mode: String = raw
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    let version: i64 = raw
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(mode.to_ascii_lowercase(), "wal");
    assert_eq!(version, SCHEMA_VERSION);
    drop(raw);

    let ledger = open_again(&scratch.path, &decl);
    assert_eq!(ledger.declaration(), &decl);
    assert_eq!(ledger.recipe().unwrap().foreign_keys, 1);
}

#[test]
fn declaration_mismatch_fails_closed_and_keeps_committed_rows() {
    let scratch = Scratch::new("mismatch");
    let decl = sample(10);
    let now = at_civil("Europe/Berlin", 2026, 6, 15, 9, 0, 0);
    let mut ledger = open_created(&scratch.path, &decl);
    assert!(!ledger.consume("keep", 4, now).unwrap().is_duplicate());

    let mut wrong = decl.clone();
    wrong.capacity = 11;
    let err = Ledger::open(&scratch.path, &wrong).unwrap_err();
    assert!(matches!(
        err,
        LedgerError::DeclarationMismatch {
            field: "capacity",
            ..
        }
    ));
    wrong.capacity = decl.capacity;
    wrong.timezone = "Europe/Paris".into();
    let err = Ledger::open(&scratch.path, &wrong).unwrap_err();
    assert!(matches!(
        err,
        LedgerError::DeclarationMismatch {
            field: "timezone",
            ..
        }
    ));
    wrong.timezone = decl.timezone.clone();
    wrong.kind = WindowKind::Week;
    let err = Ledger::open(&scratch.path, &wrong).unwrap_err();
    assert!(matches!(
        err,
        LedgerError::DeclarationMismatch {
            field: "window_kind",
            ..
        }
    ));
    wrong.kind = decl.kind;
    wrong.meter_id = "other".into();
    let err = Ledger::open(&scratch.path, &wrong).unwrap_err();
    assert!(matches!(
        err,
        LedgerError::DeclarationMismatch {
            field: "meter_id",
            ..
        }
    ));

    assert_eq!(ledger.totals().unwrap().event_amount, 4);
    drop(ledger);
    let ledger = open_again(&scratch.path, &decl);
    assert_eq!(ledger.event("keep").unwrap().unwrap().amount, 4);
    assert_eq!(ledger.totals().unwrap().window_consumed, 4);
}

#[test]
fn user_version_mismatch_and_torn_init_fail_closed() {
    let scratch = Scratch::new("version");
    let decl = sample(10);
    let ledger = open_created(&scratch.path, &decl);
    drop(ledger);

    let conn = rusqlite::Connection::open(&scratch.path).unwrap();
    conn.pragma_update(None, "user_version", 2).unwrap();
    drop(conn);
    let err = Ledger::open(&scratch.path, &decl).unwrap_err();
    assert!(matches!(
        err,
        LedgerError::SchemaMismatch {
            found: 2,
            expected: SCHEMA_VERSION,
        }
    ));

    let conn = rusqlite::Connection::open(&scratch.path).unwrap();
    conn.pragma_update(None, "user_version", 0).unwrap();
    drop(conn);
    let err = Ledger::open(&scratch.path, &decl).unwrap_err();
    assert!(matches!(err, LedgerError::TornInitialization));
}

#[test]
fn extra_table_fails_closed() {
    let scratch = Scratch::new("extra");
    let decl = sample(10);
    drop(open_created(&scratch.path, &decl));
    let conn = rusqlite::Connection::open(&scratch.path).unwrap();
    conn.execute_batch("CREATE TABLE extra (id INTEGER);")
        .unwrap();
    drop(conn);
    let err = Ledger::open(&scratch.path, &decl).unwrap_err();
    assert!(matches!(err, LedgerError::SchemaDrift { .. }), "{err}");
}

#[test]
fn consume_is_idempotent_and_conflict_does_not_charge() {
    let scratch = Scratch::new("idem");
    let decl = sample(10);
    let now = at_civil("Europe/Berlin", 2026, 6, 15, 9, 0, 0);
    let mut ledger = open_created(&scratch.path, &decl);
    let err = ledger.consume("", 1, now).unwrap_err();
    assert!(matches!(err, LedgerError::EmptyIdempotencyKey));
    let err = ledger.consume("zero", 0, now).unwrap_err();
    assert!(matches!(err, LedgerError::BadAmount { amount: 0 }));
    let err = ledger.consume("huge", u64::MAX, now).unwrap_err();
    assert!(matches!(err, LedgerError::BadAmount { amount: u64::MAX }));

    let applied = ledger.consume("k", 4, now).unwrap();
    assert!(!applied.is_duplicate());
    assert_eq!(applied.reading().available, 6);
    let snap = ledger.window_at(now).unwrap();
    assert_eq!(
        applied.reading().recovers_at,
        Some(u64::try_from(snap.end_ms).unwrap())
    );
    assert!(snap.persisted);
    assert_eq!(snap.consumed, 4);

    let again = ledger.consume("k", 4, now).unwrap();
    assert!(again.is_duplicate());
    assert_eq!(again.reading().available, 6);
    assert_eq!(ledger.totals().unwrap().event_count, 1);
    assert_eq!(ledger.totals().unwrap().outbox_count, 1);

    let err = ledger.consume("k", 5, now).unwrap_err();
    assert!(matches!(
        err,
        LedgerError::IdempotencyConflict {
            stored: 4,
            incoming: 5,
            ..
        }
    ));
    assert_eq!(ledger.totals().unwrap().window_consumed, 4);
    assert_eq!(ledger.event("k").unwrap().unwrap().at_ms, now);
}

#[test]
fn insufficient_rolls_back_and_restart_keeps_the_committed_charge() {
    let scratch = Scratch::new("restart");
    let decl = sample(10);
    let now = at_civil("Europe/Berlin", 2026, 6, 15, 9, 0, 0);
    let mut ledger = open_created(&scratch.path, &decl);
    let err = ledger.consume("too-big", 11, now).unwrap_err();
    let snap = ledger.window_at(now).unwrap();
    match err {
        LedgerError::Insufficient {
            need,
            available,
            recovers_at,
        } => {
            assert_eq!(need, 11);
            assert_eq!(available, 10);
            assert_eq!(recovers_at, u64::try_from(snap.end_ms).unwrap());
        }
        other => panic!("{other:?}"),
    }
    assert!(!ledger.window_at(now).unwrap().persisted);
    assert_eq!(ledger.totals().unwrap().event_count, 0);

    assert!(!ledger.consume("kept", 3, now).unwrap().is_duplicate());
    let payload = ledger.outbox().unwrap();
    assert_eq!(payload.len(), 1);
    assert!(payload[0].sent_at_ms.is_none());
    let body: serde_json::Value = serde_json::from_str(&payload[0].payload).unwrap();
    assert_eq!(body["idempotency_key"], "kept");
    assert_eq!(body["amount"], 3);
    assert_eq!(body["meter_id"], "subscription");
    assert_eq!(body["at_ms"], now);
    drop(ledger);

    let ledger = open_again(&scratch.path, &decl);
    let event = ledger.event("kept").unwrap().unwrap();
    assert_eq!(event.amount, 3);
    assert_eq!(event.at_ms, now);
    assert_eq!(ledger.reading(now).unwrap().available, 7);
    let totals = ledger.totals().unwrap();
    assert_eq!(totals.event_amount, totals.window_consumed);
    assert_eq!(totals.event_count, totals.outbox_count);
    assert_eq!(totals.outbox_unsent, 1);
    assert_eq!(ledger.outbox().unwrap()[0].sent_at_ms, None);
}

#[test]
fn lazy_reset_opens_the_next_civil_window() {
    let scratch = Scratch::new("lazy");
    let decl = sample(10);
    let noon = at_civil("Europe/Berlin", 2026, 3, 29, 12, 0, 0);
    let mut ledger = open_created(&scratch.path, &decl);
    let applied = ledger.consume("fill", 10, noon).unwrap();
    let snap = ledger.window_at(noon).unwrap();
    assert_ne!(snap.end_ms - snap.start_ms, 86_400_000);
    assert_eq!(
        applied.reading().recovers_at,
        Some(u64::try_from(snap.end_ms).unwrap())
    );
    assert_eq!(ledger.reading(snap.end_ms - 1).unwrap().available, 0);
    assert!(!ledger.window_at(snap.end_ms).unwrap().persisted);
    assert_eq!(ledger.reading(snap.end_ms).unwrap().available, 10);

    let next = ledger.consume("next", 3, snap.end_ms).unwrap();
    assert_eq!(next.reading().available, 7);
    assert_eq!(ledger.window_at(snap.end_ms).unwrap().consumed, 3);
    assert_eq!(ledger.window_at(noon).unwrap().consumed, 10);

    let replay = ledger.consume("fill", 10, snap.end_ms).unwrap();
    assert!(replay.is_duplicate());
    assert_eq!(ledger.window_at(snap.end_ms).unwrap().consumed, 3);
    assert_eq!(ledger.totals().unwrap().event_amount, 13);
    assert_eq!(ledger.totals().unwrap().window_consumed, 13);
}

#[test]
fn recover_persists_the_sent_mark_across_reopen() {
    let scratch = Scratch::new("recover");
    let decl = sample(10);
    let now = at_civil("Europe/Berlin", 2026, 6, 15, 9, 0, 0);
    let mut ledger = open_created(&scratch.path, &decl);
    assert!(!ledger.consume("a", 1, now).unwrap().is_duplicate());
    assert!(!ledger.consume("b", 2, now).unwrap().is_duplicate());
    drop(ledger);

    let mut ledger = open_again(&scratch.path, &decl);
    let mut calls = 0;
    let err = ledger
        .recover(now + 10, |_| {
            calls += 1;
            if calls == 2 {
                Err("down")
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    assert!(matches!(err, LedgerError::Dispatch(_)));
    assert_eq!(calls, 2);
    assert_eq!(ledger.totals().unwrap().outbox_unsent, 1);
    drop(ledger);

    let mut ledger = open_again(&scratch.path, &decl);
    assert_eq!(ledger.totals().unwrap().outbox_unsent, 1);
    let marked = ledger.recover(now + 25, |_| Ok::<(), &str>(())).unwrap();
    assert_eq!(marked, 1);
    assert_eq!(ledger.totals().unwrap().outbox_unsent, 0);
    let sent_at: Vec<_> = ledger
        .outbox()
        .unwrap()
        .into_iter()
        .map(|row| row.sent_at_ms)
        .collect();
    assert!(sent_at.contains(&Some(now + 10)));
    assert!(sent_at.contains(&Some(now + 25)));
    drop(ledger);

    let mut ledger = open_again(&scratch.path, &decl);
    let marked = ledger
        .recover(now + 40, |_| -> Result<(), &str> {
            panic!("unsent queue is empty")
        })
        .unwrap();
    assert_eq!(marked, 0);
    assert_eq!(ledger.outbox().unwrap().len(), 2);
}

#[test]
fn recover_redelivers_the_same_idempotency_key() {
    let scratch = Scratch::new("once");
    let decl = sample(10);
    let now = at_civil("Europe/Berlin", 2026, 6, 15, 9, 0, 0);
    let mut ledger = open_created(&scratch.path, &decl);
    assert!(!ledger.consume("only", 6, now).unwrap().is_duplicate());
    let mut seen = HashSet::new();
    ledger
        .recover(now + 1, |row| {
            let body: serde_json::Value = serde_json::from_str(&row.payload).unwrap();
            assert_eq!(body["idempotency_key"], row.idempotency_key);
            seen.insert(row.idempotency_key.clone());
            Ok::<(), &str>(())
        })
        .unwrap();
    assert_eq!(seen.len(), 1);

    let conn = rusqlite::Connection::open(&scratch.path).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    let orphan = conn.execute(
        "INSERT INTO outbox (idempotency_key, payload, created_at_ms, sent_at_ms)
         VALUES ('missing', '{}', 0, NULL)",
        [],
    );
    assert!(
        orphan.is_err(),
        "orphan outbox row must fail the foreign key"
    );
    conn.execute("UPDATE outbox SET sent_at_ms = NULL", [])
        .unwrap();
    drop(conn);

    ledger
        .recover(now + 2, |row| {
            seen.insert(row.idempotency_key.clone());
            Ok::<(), &str>(())
        })
        .unwrap();
    assert_eq!(seen.len(), 1, "redelivery carries the same key");
    assert_eq!(ledger.totals().unwrap().event_count, 1);
    assert_eq!(ledger.totals().unwrap().window_consumed, 6);
}

#[test]
fn concurrent_consumers_conserve_the_window() {
    let scratch = Scratch::new("conserve");
    let decl = sample(50);
    let now = at_civil("Europe/Berlin", 2026, 6, 15, 12, 0, 0);
    drop(open_created(&scratch.path, &decl));

    let threads = 4;
    let each = 30;
    let barrier = Arc::new(Barrier::new(threads));
    let (applied, rejected) = std::thread::scope(|scope| {
        let mut guards = Vec::new();
        for t in 0..threads {
            let path = scratch.path.clone();
            let decl = decl.clone();
            let barrier = Arc::clone(&barrier);
            guards.push(scope.spawn(move || {
                let mut ledger = open_again(&path, &decl);
                barrier.wait();
                let mut applied = 0u64;
                let mut rejected = 0u64;
                for i in 0..each {
                    let key = format!("t{t}-{i}");
                    match ledger.consume(&key, 1, now) {
                        Ok(ConsumeResult::Applied(_)) => applied += 1,
                        Err(LedgerError::Insufficient { .. }) => rejected += 1,
                        other => panic!("{other:?}"),
                    }
                }
                (applied, rejected)
            }));
        }
        guards
            .into_iter()
            .map(|guard| guard.join().unwrap())
            .fold((0u64, 0u64), |(a, r), (na, nr)| (a + na, r + nr))
    });
    assert_eq!(applied, 50);
    assert_eq!(rejected, u64::try_from(threads * each).unwrap() - 50);

    let ledger = open_again(&scratch.path, &decl);
    let totals = ledger.totals().unwrap();
    assert_eq!(totals.event_count, 50);
    assert_eq!(totals.event_amount, 50);
    assert_eq!(totals.window_consumed, 50);
    assert_eq!(totals.outbox_count, 50);
    assert_eq!(totals.outbox_unsent, 50);
    assert_eq!(ledger.reading(now).unwrap().available, 0);
    let snap = ledger.window_at(now).unwrap();
    assert_eq!(
        ledger.reading(now).unwrap().recovers_at,
        Some(u64::try_from(snap.end_ms).unwrap())
    );
}

#[test]
fn concurrent_same_key_is_charged_once() {
    let scratch = Scratch::new("race-key");
    let decl = sample(10);
    let now = at_civil("Europe/Berlin", 2026, 6, 15, 12, 0, 0);
    drop(open_created(&scratch.path, &decl));

    let threads = 8;
    let barrier = Arc::new(Barrier::new(threads));
    let (applied, duplicate) = std::thread::scope(|scope| {
        let mut guards = Vec::new();
        for _ in 0..threads {
            let path = scratch.path.clone();
            let decl = decl.clone();
            let barrier = Arc::clone(&barrier);
            guards.push(scope.spawn(move || {
                let mut ledger = open_again(&path, &decl);
                barrier.wait();
                ledger.consume("same-key", 1, now).unwrap()
            }));
        }
        guards.into_iter().map(|guard| guard.join().unwrap()).fold(
            (0u32, 0u32),
            |(applied, duplicate), result| match result {
                ConsumeResult::Applied(_) => (applied + 1, duplicate),
                ConsumeResult::AlreadyApplied(_) => (applied, duplicate + 1),
            },
        )
    });
    assert_eq!(applied, 1);
    assert_eq!(duplicate, u32::try_from(threads).unwrap() - 1);

    let ledger = open_again(&scratch.path, &decl);
    let totals = ledger.totals().unwrap();
    assert_eq!(totals.event_count, 1);
    assert_eq!(totals.event_amount, 1);
    assert_eq!(totals.window_consumed, 1);
    assert_eq!(totals.outbox_count, 1);
    assert_eq!(ledger.event("same-key").unwrap().unwrap().amount, 1);
}
