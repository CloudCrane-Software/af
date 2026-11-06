//! 班次窗口路由。
//!
//! 角色精确匹配之外，工位/角色的可派单时段由 af-calendar 决定。
//! 非班次任务保持 queued。时刻全部来自 ManualClock，不睡眠。
//!
//! 跨午夜按 af-calendar 已验证的「两天两窗口」建模：当天 [22:00, 24:00) 用
//! 23:59:59.999999999 逼近 24:00，次日 [00:00, 06:00)。午夜接缝的 1ns 低于
//! 本账本的毫秒时钟，毫秒网格上看不到空档。
//!
//! DST 与 af-calendar 的裁决一致：窗口按本地钟面，`is_working_instant` 不消歧。
//! 春跳缺失的那一小时没有可注入的时刻；秋拨重复的那一小时两次出现都按钟面派单。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use af_calendar::{Calendar, Shift};
use af_station::{Enqueued, Error, ManualClock, NewTask, OpenStatus, State, Station};
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Asia::Shanghai;
use chrono_tz::Europe::Berlin;
use chrono_tz::Tz;

struct Tmp {
    dir: PathBuf,
}

impl Tmp {
    fn new(tag: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("af-station-shift-{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    fn path(&self) -> PathBuf {
        self.dir.join("station.db")
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn sh(y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Tz> {
    Shanghai
        .with_ymd_and_hms(y, m, d, h, mi, s)
        .single()
        .expect("上海无 DST，本地时刻唯一")
}

fn berlin(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<Tz> {
    Berlin
        .with_ymd_and_hms(y, m, d, h, mi, 0)
        .single()
        .expect("该柏林时刻唯一存在")
}

fn clock_at(at: DateTime<impl TimeZone>) -> ManualClock {
    ManualClock::new(at.with_timezone(&Utc).timestamp_millis())
}

fn office() -> Calendar {
    let cal = Calendar::weekday_8x5_shanghai();
    cal.validate().unwrap();
    cal
}

fn end_of_day() -> NaiveTime {
    NaiveTime::from_hms_nano_opt(23, 59, 59, 999_999_999).unwrap()
}

fn weekly_shift(tz: Tz, shift: Shift) -> Calendar {
    let mut weekly: [Vec<Shift>; 7] = Default::default();
    for slot in &mut weekly {
        *slot = vec![shift];
    }
    let cal = Calendar::from_weekly(tz, weekly);
    cal.validate().unwrap();
    cal
}

/// 全周夜班：凌晨 [00:00, 06:00) + 晚间 [22:00, 24:00)。
fn night_calendar(tz: Tz) -> Calendar {
    let shifts = vec![
        Shift::new(NaiveTime::MIN, NaiveTime::from_hms_opt(6, 0, 0).unwrap()).unwrap(),
        Shift::new(NaiveTime::from_hms_opt(22, 0, 0).unwrap(), end_of_day()).unwrap(),
    ];
    let mut weekly: [Vec<Shift>; 7] = Default::default();
    for slot in &mut weekly {
        *slot = shifts.clone();
    }
    let cal = Calendar::from_weekly(tz, weekly);
    cal.validate().unwrap();
    cal
}

fn boot(path: &Path, clock: &ManualClock) -> Station {
    let opened = Station::open_with_clock(path, clock.clone()).unwrap();
    assert_eq!(opened.status(), OpenStatus::Created);
    opened.into_station()
}

fn reopen(path: &Path, clock: &ManualClock) -> Station {
    let opened = Station::open_with_clock(path, clock.clone()).unwrap();
    assert_eq!(opened.status(), OpenStatus::AlreadyDeclared);
    opened.into_station()
}

fn put(s: &Station, id: &str, role: &str, deadline: Option<DateTime<Utc>>) {
    assert_eq!(
        s.enqueue(NewTask {
            id,
            payload_ref: "p",
            required_role: role,
            idempotency_key: None,
            deadline,
        })
        .unwrap(),
        Enqueued::Inserted { id: id.into() }
    );
}

fn assert_queued(s: &Station, id: &str) {
    let task = s.get(id).unwrap().unwrap();
    assert_eq!(task.state, State::Queued);
    assert_eq!(task.claimant, None);
    assert_eq!(task.lease_until, None);
}

#[test]
fn unbound_shift_keeps_role_only_routing() {
    // 周日下午：若误把「没绑定」当成停工，这条会失败。
    let at = sh(2026, 10, 25, 15, 0, 0);
    assert_eq!(at.weekday().to_string(), "Sun");
    let clock = clock_at(at);
    let s = Station::open_in_memory_with_clock(clock).unwrap();
    put(&s, "t1", "reviewer", None);
    assert!(s.claim("coder", "coder", 1_000).unwrap().is_none());
    assert_queued(&s, "t1");
    let task = s.claim("w", "reviewer", 1_000).unwrap().unwrap();
    assert_eq!(task.id, "t1");
    assert_eq!(task.state, State::Claimed);
}

#[test]
fn empty_calendar_never_dispatches() {
    let at = sh(2026, 10, 19, 10, 0, 0);
    assert_eq!(at.weekday().to_string(), "Mon");
    let clock = clock_at(at);
    let s = Station::open_in_memory_with_clock(clock).unwrap();
    let cal = Calendar::new_empty(office().tz);
    cal.validate().unwrap();
    s.bind_role_shift("reviewer", &cal).unwrap();
    put(&s, "t1", "reviewer", None);
    assert!(s.claim("w", "reviewer", 1_000).unwrap().is_none());
    assert_queued(&s, "t1");
}

#[test]
fn off_shift_stays_queued_until_window_opens() {
    let clock = clock_at(sh(2026, 10, 18, 10, 0, 0));
    assert_eq!(
        sh(2026, 10, 18, 10, 0, 0).weekday().to_string(),
        "Sun",
        "夹具护栏"
    );
    let s = Station::open_in_memory_with_clock(clock.clone()).unwrap();
    s.bind_role_shift("reviewer", &office()).unwrap();
    put(&s, "t1", "reviewer", None);
    assert!(s.claim("w", "reviewer", 60_000).unwrap().is_none());
    assert_queued(&s, "t1");

    clock.set(
        sh(2026, 10, 19, 10, 0, 0)
            .with_timezone(&Utc)
            .timestamp_millis(),
    );
    let task = s.claim("w", "reviewer", 60_000).unwrap().unwrap();
    assert_eq!(task.id, "t1");
    assert_eq!(task.claimant.as_deref(), Some("w"));
    assert!(office().is_working_instant(s.now_utc().unwrap()));
}

#[test]
fn shift_window_is_half_open() {
    let clock = clock_at(sh(2026, 10, 19, 7, 59, 59));
    let s = Station::open_in_memory_with_clock(clock.clone()).unwrap();
    s.bind_role_shift("reviewer", &office()).unwrap();
    put(&s, "before", "reviewer", None);
    put(&s, "start", "reviewer", None);
    put(&s, "last", "reviewer", None);
    put(&s, "end", "reviewer", None);

    assert!(s.claim("w", "reviewer", 1_000).unwrap().is_none());
    assert_queued(&s, "before");

    clock.set(
        sh(2026, 10, 19, 8, 0, 0)
            .with_timezone(&Utc)
            .timestamp_millis(),
    );
    assert_eq!(
        s.claim("w", "reviewer", 1_000).unwrap().unwrap().id,
        "before"
    );
    assert!(s.complete("before", "w").unwrap());

    clock.set(
        sh(2026, 10, 19, 16, 59, 59)
            .with_timezone(&Utc)
            .timestamp_millis(),
    );
    assert_eq!(
        s.claim("w", "reviewer", 1_000).unwrap().unwrap().id,
        "start"
    );
    assert!(s.complete("start", "w").unwrap());

    clock.set(
        sh(2026, 10, 19, 17, 0, 0)
            .with_timezone(&Utc)
            .timestamp_millis(),
    );
    assert!(s.claim("w", "reviewer", 1_000).unwrap().is_none());
    assert_queued(&s, "last");
    assert_queued(&s, "end");
}

#[test]
fn role_exact_match_still_required_on_shift() {
    let clock = clock_at(sh(2026, 10, 19, 10, 0, 0));
    let s = Station::open_in_memory_with_clock(clock).unwrap();
    s.bind_role_shift("reviewer", &office()).unwrap();
    s.bind_role_shift("coder", &office()).unwrap();
    put(&s, "t1", "reviewer", None);
    assert!(s.claim("w-coder", "coder", 1_000).unwrap().is_none());
    assert_queued(&s, "t1");
    assert_eq!(
        s.claim("w-reviewer", "reviewer", 1_000)
            .unwrap()
            .unwrap()
            .id,
        "t1"
    );
}

#[test]
fn holiday_blocks_and_makeup_saturday_opens() {
    let monday = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
    let saturday = NaiveDate::from_ymd_opt(2026, 10, 24).unwrap();
    let cal = office()
        .with_holidays([monday])
        .with_day_override(saturday, vec![Shift::hm(9, 0, 12, 0).unwrap()]);
    cal.validate().unwrap();

    let clock = clock_at(sh(2026, 10, 19, 10, 0, 0));
    let s = Station::open_in_memory_with_clock(clock.clone()).unwrap();
    s.bind_role_shift("reviewer", &cal).unwrap();
    put(&s, "mon", "reviewer", None);
    assert!(
        !cal.is_working_instant(sh(2026, 10, 19, 10, 0, 0)),
        "假日整日不开工"
    );
    assert!(s.claim("w", "reviewer", 1_000).unwrap().is_none());
    assert_queued(&s, "mon");

    clock.set(
        sh(2026, 10, 20, 10, 0, 0)
            .with_timezone(&Utc)
            .timestamp_millis(),
    );
    assert_eq!(s.claim("w", "reviewer", 1_000).unwrap().unwrap().id, "mon");
    assert!(s.complete("mon", "w").unwrap());

    put(&s, "sat-early", "reviewer", None);
    clock.set(
        sh(2026, 10, 24, 8, 30, 0)
            .with_timezone(&Utc)
            .timestamp_millis(),
    );
    assert_eq!(sh(2026, 10, 24, 8, 30, 0).weekday().to_string(), "Sat");
    assert!(s.claim("w", "reviewer", 1_000).unwrap().is_none());
    assert_queued(&s, "sat-early");

    clock.set(
        sh(2026, 10, 24, 10, 0, 0)
            .with_timezone(&Utc)
            .timestamp_millis(),
    );
    assert_eq!(
        s.claim("w", "reviewer", 1_000).unwrap().unwrap().id,
        "sat-early"
    );
    assert!(s.complete("sat-early", "w").unwrap());

    put(&s, "sun", "reviewer", None);
    clock.set(
        sh(2026, 10, 25, 10, 0, 0)
            .with_timezone(&Utc)
            .timestamp_millis(),
    );
    assert!(s.claim("w", "reviewer", 1_000).unwrap().is_none());
    assert_queued(&s, "sun");
}

#[test]
fn worker_shift_intersects_role_shift() {
    let narrow = Calendar::weekday_single_shift(Shanghai, Shift::hm(12, 0, 13, 0).unwrap());
    narrow.validate().unwrap();
    let clock = clock_at(sh(2026, 10, 19, 9, 0, 0));
    let s = Station::open_in_memory_with_clock(clock.clone()).unwrap();
    s.bind_role_shift("reviewer", &office()).unwrap();
    s.bind_worker_shift("alice", &narrow).unwrap();
    put(&s, "t9", "reviewer", None);
    put(&s, "t12", "reviewer", None);
    put(&s, "t13", "reviewer", None);

    assert!(s.claim("alice", "reviewer", 1_000).unwrap().is_none());
    assert_eq!(
        s.claim("bob", "reviewer", 1_000).unwrap().unwrap().id,
        "t9",
        "未绑定工位日历的 bob 只受角色班次约束"
    );
    assert!(s.complete("t9", "bob").unwrap());

    clock.set(
        sh(2026, 10, 19, 12, 30, 0)
            .with_timezone(&Utc)
            .timestamp_millis(),
    );
    assert_eq!(
        s.claim("alice", "reviewer", 1_000).unwrap().unwrap().id,
        "t12"
    );
    assert!(s.complete("t12", "alice").unwrap());

    clock.set(
        sh(2026, 10, 19, 13, 0, 0)
            .with_timezone(&Utc)
            .timestamp_millis(),
    );
    assert!(s.claim("alice", "reviewer", 1_000).unwrap().is_none());
    assert_queued(&s, "t13");
    assert_eq!(
        s.claim("bob", "reviewer", 1_000).unwrap().unwrap().id,
        "t13"
    );
}

#[test]
fn cross_midnight_two_windows_dispatch() {
    let cal = night_calendar(Shanghai);
    let fri_before = sh(2026, 6, 5, 21, 59, 59);
    assert_eq!(fri_before.weekday().to_string(), "Fri");
    let clock = clock_at(fri_before);
    let s = Station::open_in_memory_with_clock(clock.clone()).unwrap();
    s.bind_role_shift("night", &cal).unwrap();

    // 非班次样本留下的 queued 行会在下一班次被认领，所以同一时刻只留一条未完成任务。
    let samples = [
        ("before", sh(2026, 6, 5, 21, 59, 59), false),
        ("open", sh(2026, 6, 5, 22, 0, 0), true),
        ("late", sh(2026, 6, 5, 23, 59, 59), true),
        ("midnight", sh(2026, 6, 6, 0, 0, 0), true),
        ("last", sh(2026, 6, 6, 5, 59, 59), true),
        ("end", sh(2026, 6, 6, 6, 0, 0), false),
    ];
    let mut seq = 0u32;
    let mut pending: Option<String> = None;
    for (label, at, working) in samples {
        assert_eq!(
            cal.is_working_instant(at),
            working,
            "{label} 的钟面判定应与日历一致"
        );
        if pending.is_none() {
            let id = format!("t{seq}");
            seq += 1;
            put(&s, &id, "night", None);
            pending = Some(id);
        }
        clock.set(at.with_timezone(&Utc).timestamp_millis());
        let claimed = s.claim("w", "night", 1_000).unwrap();
        if working {
            let id = pending.take().unwrap();
            assert_eq!(claimed.unwrap().id, id, "{label}");
            assert!(s.complete(&id, "w").unwrap());
        } else {
            assert!(claimed.is_none(), "{label}");
            assert_queued(&s, pending.as_deref().unwrap());
        }
    }
}

#[test]
fn dst_spring_gap_window_does_not_dispatch() {
    // Berlin 2026-03-29 02:00→03:00。窗口 [02:00, 03:00) 整段落在缺失小时里。
    let cal = weekly_shift(Berlin, Shift::hm(2, 0, 3, 0).unwrap());
    assert!(
        Berlin
            .with_ymd_and_hms(2026, 3, 29, 2, 30, 0)
            .single()
            .is_none(),
        "春跳没有 02:30 这个时刻"
    );
    let day_before = berlin(2026, 3, 28, 2, 30);
    assert!(cal.is_working_instant(day_before));

    let clock = clock_at(day_before);
    let s = Station::open_in_memory_with_clock(clock.clone()).unwrap();
    s.bind_role_shift("reviewer", &cal).unwrap();
    put(&s, "prev", "reviewer", None);
    assert_eq!(s.claim("w", "reviewer", 1_000).unwrap().unwrap().id, "prev");
    assert!(s.complete("prev", "w").unwrap());

    for (id, at) in [
        ("before-gap", berlin(2026, 3, 29, 1, 30)),
        ("after-gap", berlin(2026, 3, 29, 3, 0)),
    ] {
        assert!(!cal.is_working_instant(at), "{id}");
        put(&s, id, "reviewer", None);
        clock.set(at.with_timezone(&Utc).timestamp_millis());
        assert!(s.claim("w", "reviewer", 1_000).unwrap().is_none());
        assert_queued(&s, id);
    }
}

#[test]
fn dst_spring_wide_window_covers_both_sides_of_gap() {
    let cal = weekly_shift(
        Berlin,
        Shift::new(NaiveTime::MIN, NaiveTime::from_hms_opt(6, 0, 0).unwrap()).unwrap(),
    );
    let clock = clock_at(berlin(2026, 3, 29, 1, 30));
    let s = Station::open_in_memory_with_clock(clock.clone()).unwrap();
    s.bind_role_shift("reviewer", &cal).unwrap();
    for (id, at, working) in [
        ("early", berlin(2026, 3, 29, 1, 30), true),
        ("later", berlin(2026, 3, 29, 3, 30), true),
        ("end", berlin(2026, 3, 29, 6, 0), false),
    ] {
        assert_eq!(cal.is_working_instant(at), working, "{id}");
        put(&s, id, "reviewer", None);
        clock.set(at.with_timezone(&Utc).timestamp_millis());
        let claimed = s.claim("w", "reviewer", 1_000).unwrap();
        if working {
            assert_eq!(claimed.unwrap().id, id);
            assert!(s.complete(id, "w").unwrap());
        } else {
            assert!(claimed.is_none());
            assert_queued(&s, id);
        }
    }
}

#[test]
fn dst_fall_back_dispatches_both_clock_faces() {
    // Berlin 2026-10-25 03:00→02:00。02:30 出现两次，钟面都落在 [02:00, 03:00)。
    let cal = weekly_shift(Berlin, Shift::hm(2, 0, 3, 0).unwrap());
    let naive = NaiveDate::from_ymd_opt(2026, 10, 25)
        .unwrap()
        .and_hms_opt(2, 30, 0)
        .unwrap();
    let first = Berlin.from_local_datetime(&naive).earliest().unwrap();
    let second = Berlin.from_local_datetime(&naive).latest().unwrap();
    assert_ne!(first.timestamp(), second.timestamp());
    assert!(cal.is_working_instant(first));
    assert!(cal.is_working_instant(second));

    let before = berlin(2026, 10, 25, 1, 30);
    let after = berlin(2026, 10, 25, 3, 0);
    assert!(!cal.is_working_instant(before));
    assert!(!cal.is_working_instant(after));

    let clock = clock_at(before);
    let s = Station::open_in_memory_with_clock(clock.clone()).unwrap();
    s.bind_role_shift("reviewer", &cal).unwrap();
    put(&s, "fold", "reviewer", None);
    assert!(s.claim("w", "reviewer", 1_000).unwrap().is_none());
    assert_queued(&s, "fold");

    // 01:30 留下的那一行到第一次 02:30 才被派出去。
    clock.set(first.with_timezone(&Utc).timestamp_millis());
    assert_eq!(s.claim("w", "reviewer", 1_000).unwrap().unwrap().id, "fold");
    assert!(s.complete("fold", "w").unwrap());

    put(&s, "second", "reviewer", None);
    clock.set(second.with_timezone(&Utc).timestamp_millis());
    assert_eq!(
        s.claim("w", "reviewer", 1_000).unwrap().unwrap().id,
        "second"
    );
    assert!(s.complete("second", "w").unwrap());

    put(&s, "after", "reviewer", None);
    clock.set(after.with_timezone(&Utc).timestamp_millis());
    assert!(s.claim("w", "reviewer", 1_000).unwrap().is_none());
    assert_queued(&s, "after");
}

#[test]
fn claimed_on_shift_survives_off_shift_until_lease_ends() {
    let clock = clock_at(sh(2026, 10, 19, 16, 50, 0));
    let s = Station::open_in_memory_with_clock(clock.clone()).unwrap();
    s.bind_role_shift("reviewer", &office()).unwrap();
    put(&s, "t1", "reviewer", None);
    let task = s
        .claim("alice", "reviewer", 2 * 60 * 60 * 1_000)
        .unwrap()
        .unwrap();
    let lease = task.lease_until.unwrap();
    clock.set(
        sh(2026, 10, 19, 17, 30, 0)
            .with_timezone(&Utc)
            .timestamp_millis(),
    );
    assert!(lease > s.now_utc().unwrap().timestamp_millis());
    assert!(s.claim("bob", "reviewer", 1_000).unwrap().is_none());
    let held = s.get("t1").unwrap().unwrap();
    assert_eq!(held.state, State::Claimed);
    assert_eq!(held.claimant.as_deref(), Some("alice"));
    assert_eq!(held.lease_until, Some(lease));
    assert!(s.heartbeat("t1", "alice", 1_000).unwrap());
    assert!(s.complete("t1", "alice").unwrap());
    assert_eq!(s.get("t1").unwrap().unwrap().state, State::Done);
}

#[test]
fn off_shift_claim_reclaims_expired_lease_without_dispatching() {
    let clock = clock_at(sh(2026, 10, 19, 16, 50, 0));
    let s = Station::open_in_memory_with_clock(clock.clone()).unwrap();
    s.bind_role_shift("reviewer", &office()).unwrap();
    put(&s, "t1", "reviewer", None);
    s.claim("alice", "reviewer", 20 * 60 * 1_000)
        .unwrap()
        .unwrap();
    clock.set(
        sh(2026, 10, 19, 17, 30, 0)
            .with_timezone(&Utc)
            .timestamp_millis(),
    );
    assert!(s.claim("bob", "reviewer", 1_000).unwrap().is_none());
    assert_queued(&s, "t1");
    clock.set(
        sh(2026, 10, 20, 9, 0, 0)
            .with_timezone(&Utc)
            .timestamp_millis(),
    );
    assert_eq!(
        s.claim("bob", "reviewer", 1_000)
            .unwrap()
            .unwrap()
            .claimant
            .as_deref(),
        Some("bob")
    );
}

#[test]
fn overdue_deadline_does_not_gate_dispatch() {
    let past = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    let clock = clock_at(sh(2026, 10, 18, 10, 0, 0));
    let s = Station::open_in_memory_with_clock(clock.clone()).unwrap();
    s.bind_role_shift("reviewer", &office()).unwrap();
    put(&s, "t1", "reviewer", Some(past));
    let task = s.get("t1").unwrap().unwrap();
    assert!(task.is_overdue(s.now_utc().unwrap()));
    assert!(s.claim("w", "reviewer", 1_000).unwrap().is_none());
    assert_queued(&s, "t1");

    clock.set(
        sh(2026, 10, 19, 10, 0, 0)
            .with_timezone(&Utc)
            .timestamp_millis(),
    );
    assert!(s
        .get("t1")
        .unwrap()
        .unwrap()
        .is_overdue(s.now_utc().unwrap()));
    assert_eq!(s.claim("w", "reviewer", 1_000).unwrap().unwrap().id, "t1");
}

#[test]
fn submillisecond_deadline_is_rejected() {
    let s = Station::open_in_memory().unwrap();
    let aligned = Utc.with_ymd_and_hms(2026, 10, 26, 3, 0, 0).unwrap();
    let nudged = aligned + Duration::microseconds(500);
    let err = s
        .enqueue(NewTask {
            id: "t1",
            payload_ref: "p",
            required_role: "reviewer",
            idempotency_key: None,
            deadline: Some(nudged),
        })
        .unwrap_err();
    assert!(matches!(err, Error::DeadlineNotMillis(_)), "{err}");
    assert!(s.get("t1").unwrap().is_none());
}

#[test]
fn sla_deadline_is_utc_and_duplicate_key_keeps_it() {
    let placed = sh(2026, 10, 23, 16, 0, 0);
    assert_eq!(placed.weekday().to_string(), "Fri");
    let deadline = af_station::sla_deadline_utc(&office(), placed, Duration::hours(4)).unwrap();
    assert_eq!(
        deadline,
        Utc.with_ymd_and_hms(2026, 10, 26, 3, 0, 0).unwrap(),
        "周五 16:00+08 承诺 4 工时 → 下周一 03:00Z"
    );
    let later = Utc.with_ymd_and_hms(2026, 10, 27, 3, 0, 0).unwrap();
    let trap = DateTime::parse_from_rfc3339("2026-10-26T10:59:00+08:00")
        .unwrap()
        .with_timezone(&Utc);
    assert!(trap < deadline);
    assert!(
        "2026-10-26T03:00:00Z" < "2026-10-26T10:59:00+08:00",
        "护栏：旧字符串比较会把这个时刻判成已逾期"
    );

    let tmp = Tmp::new("sla");
    let clock = clock_at(placed);
    {
        let s = boot(tmp.path().as_path(), &clock);
        assert_eq!(
            s.enqueue(NewTask {
                id: "t1",
                payload_ref: "payload://order",
                required_role: "reviewer",
                idempotency_key: Some("k1"),
                deadline: Some(deadline),
            })
            .unwrap(),
            Enqueued::Inserted { id: "t1".into() }
        );
        assert_eq!(
            s.enqueue(NewTask {
                id: "t9",
                payload_ref: "other",
                required_role: "coder",
                idempotency_key: Some("k1"),
                deadline: Some(later),
            })
            .unwrap(),
            Enqueued::Duplicate {
                existing_id: "t1".into()
            }
        );
        let task = s.get("t1").unwrap().unwrap();
        assert_eq!(task.deadline, Some(deadline));
        assert!(!task.is_overdue(deadline));
        assert!(!task.is_overdue(trap));
        assert!(task.is_overdue(deadline + Duration::seconds(1)));
    }
    let s = reopen(tmp.path().as_path(), &clock);
    let task = s.get("t1").unwrap().unwrap();
    assert_eq!(task.deadline, Some(deadline));
    assert_eq!(task.required_role, "reviewer");
    assert_eq!(task.idempotency_key.as_deref(), Some("k1"));
    assert!(!task.is_overdue(trap));
    assert_eq!(s.count_by_state(State::Queued).unwrap(), 1);
}

#[test]
fn restart_new_task_roundtrip() {
    let tmp = Tmp::new("new");
    let clock = ManualClock::new(1_700_000_000_000);
    {
        let s = boot(tmp.path().as_path(), &clock);
        s.new_task("t1", "payload://a", "reviewer", Some("k1"))
            .unwrap();
    }
    let s = reopen(tmp.path().as_path(), &clock);
    let task = s.get("t1").unwrap().unwrap();
    assert_eq!(task.payload_ref, "payload://a");
    assert_eq!(task.required_role, "reviewer");
    assert_eq!(task.state, State::Queued);
    assert_eq!(task.claimant, None);
    assert_eq!(task.lease_until, None);
    assert_eq!(task.idempotency_key.as_deref(), Some("k1"));
    assert_eq!(task.deadline, None);
}

#[test]
fn restart_idempotency_key() {
    let tmp = Tmp::new("idem");
    let clock = ManualClock::new(1_700_000_000_000);
    {
        let s = boot(tmp.path().as_path(), &clock);
        assert_eq!(
            s.new_task("t1", "p", "reviewer", Some("k1")).unwrap(),
            Enqueued::Inserted { id: "t1".into() }
        );
        assert_eq!(
            s.new_task("t9", "p", "coder", Some("k1")).unwrap(),
            Enqueued::Duplicate {
                existing_id: "t1".into()
            }
        );
    }
    let s = reopen(tmp.path().as_path(), &clock);
    assert_eq!(
        s.new_task("t8", "p", "reviewer", Some("k1")).unwrap(),
        Enqueued::Duplicate {
            existing_id: "t1".into()
        }
    );
    assert_eq!(s.count_by_state(State::Queued).unwrap(), 1);
    assert!(s.get("t9").unwrap().is_none());
}

#[test]
fn restart_role_shift_binding() {
    let tmp = Tmp::new("role");
    let clock = clock_at(sh(2026, 10, 18, 10, 0, 0));
    {
        let s = boot(tmp.path().as_path(), &clock);
        s.bind_role_shift("reviewer", &office()).unwrap();
        put(&s, "t1", "reviewer", None);
    }
    let s = reopen(tmp.path().as_path(), &clock);
    let loaded = s.role_shift("reviewer").unwrap().unwrap();
    assert_eq!(
        serde_json::to_value(office()).unwrap(),
        serde_json::to_value(loaded).unwrap()
    );
    assert!(s.claim("w", "reviewer", 1_000).unwrap().is_none());
    assert_queued(&s, "t1");
    clock.set(
        sh(2026, 10, 19, 10, 0, 0)
            .with_timezone(&Utc)
            .timestamp_millis(),
    );
    assert_eq!(s.claim("w", "reviewer", 1_000).unwrap().unwrap().id, "t1");
}

#[test]
fn restart_worker_shift_binding() {
    let narrow = Calendar::weekday_single_shift(Shanghai, Shift::hm(12, 0, 13, 0).unwrap());
    narrow.validate().unwrap();
    let tmp = Tmp::new("worker");
    let clock = clock_at(sh(2026, 10, 19, 9, 0, 0));
    {
        let s = boot(tmp.path().as_path(), &clock);
        s.bind_role_shift("reviewer", &office()).unwrap();
        s.bind_worker_shift("alice", &narrow).unwrap();
        put(&s, "t1", "reviewer", None);
    }
    let s = reopen(tmp.path().as_path(), &clock);
    assert!(s.worker_shift("alice").unwrap().is_some());
    assert!(s.claim("alice", "reviewer", 1_000).unwrap().is_none());
    assert_queued(&s, "t1");
    assert_eq!(s.claim("bob", "reviewer", 1_000).unwrap().unwrap().id, "t1");
}

#[test]
fn restart_rebind_replaces_shift() {
    let tmp = Tmp::new("rebind");
    let clock = clock_at(sh(2026, 10, 19, 10, 0, 0));
    let empty = Calendar::new_empty(office().tz);
    empty.validate().unwrap();
    {
        let s = boot(tmp.path().as_path(), &clock);
        s.bind_role_shift("reviewer", &office()).unwrap();
        s.bind_role_shift("reviewer", &empty).unwrap();
        put(&s, "t1", "reviewer", None);
    }
    let s = reopen(tmp.path().as_path(), &clock);
    assert!(s.claim("w", "reviewer", 1_000).unwrap().is_none());
    assert_queued(&s, "t1");
}

#[test]
fn restart_claim() {
    let tmp = Tmp::new("claim");
    let clock = ManualClock::new(5_000_000);
    {
        let s = boot(tmp.path().as_path(), &clock);
        s.new_task("t1", "p", "reviewer", None).unwrap();
        let task = s.claim("alice", "reviewer", 150).unwrap().unwrap();
        assert_eq!(task.lease_until, Some(5_000_150));
    }
    let s = reopen(tmp.path().as_path(), &clock);
    let task = s.get("t1").unwrap().unwrap();
    assert_eq!(task.state, State::Claimed);
    assert_eq!(task.claimant.as_deref(), Some("alice"));
    assert_eq!(task.lease_until, Some(5_000_150));
    assert!(s.claim("bob", "reviewer", 150).unwrap().is_none());
    assert!(s.complete("t1", "alice").unwrap());
}

#[test]
fn restart_heartbeat() {
    let tmp = Tmp::new("beat");
    let clock = ManualClock::new(1_700_000_000_000);
    let after = {
        let s = boot(tmp.path().as_path(), &clock);
        s.new_task("t1", "p", "reviewer", None).unwrap();
        let before = s
            .claim("alice", "reviewer", 1_000)
            .unwrap()
            .unwrap()
            .lease_until
            .unwrap();
        assert_eq!(before, 1_700_000_000_000 + 1_000);
        clock.advance(60);
        assert!(s.heartbeat("t1", "alice", 1_000).unwrap());
        let after = s.get("t1").unwrap().unwrap().lease_until.unwrap();
        assert_eq!(after, before + 60);
        after
    };
    let s = reopen(tmp.path().as_path(), &clock);
    assert_eq!(s.get("t1").unwrap().unwrap().lease_until, Some(after));
    assert_eq!(s.get("t1").unwrap().unwrap().state, State::Claimed);
}

#[test]
fn restart_reclaim() {
    let tmp = Tmp::new("reclaim");
    let clock = ManualClock::new(5_000_000);
    {
        let s = boot(tmp.path().as_path(), &clock);
        s.new_task("t1", "p", "reviewer", None).unwrap();
        s.claim("alice", "reviewer", 150).unwrap().unwrap();
        clock.advance(350);
        assert_eq!(s.reclaim_expired().unwrap(), 1);
        assert_queued(&s, "t1");
    }
    let s = reopen(tmp.path().as_path(), &clock);
    assert_queued(&s, "t1");
    assert_eq!(
        s.claim("bob", "reviewer", 1_000)
            .unwrap()
            .unwrap()
            .claimant
            .as_deref(),
        Some("bob")
    );
}

#[test]
fn restart_complete() {
    let tmp = Tmp::new("done");
    let clock = ManualClock::new(5_000_000);
    {
        let s = boot(tmp.path().as_path(), &clock);
        s.new_task("t1", "p", "reviewer", None).unwrap();
        s.claim("alice", "reviewer", 1_000).unwrap().unwrap();
        assert!(s.complete("t1", "alice").unwrap());
    }
    let s = reopen(tmp.path().as_path(), &clock);
    let task = s.get("t1").unwrap().unwrap();
    assert_eq!(task.state, State::Done);
    assert_eq!(task.claimant.as_deref(), Some("alice"));
    assert!(!s.complete("t1", "alice").unwrap());
    assert_eq!(s.count_by_state(State::Done).unwrap(), 1);
    assert_eq!(s.count_by_state(State::Queued).unwrap(), 0);
}
