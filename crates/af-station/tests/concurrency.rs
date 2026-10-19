//! 人工工位账本的并发竞争。
//!
//! 三条不变量：
//! 1. 同一任务只能被一个工位认领。
//! 2. 并发 complete（成功或失败）不删账本行。
//! 3. 连接中断后，续点状态可重入：租约未过期则原主继续；过期回收后回到 queued，同一行可再认领。
//!
//! 线程用 [`std::sync::Barrier`] 对齐起跑，租约用 [`ManualClock`] 注入时刻，不睡眠。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use af_station::{Enqueued, ManualClock, State, Station, Task};

struct TmpDb {
    dir: PathBuf,
}

impl TmpDb {
    fn new(tag: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("af-station-{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    fn path(&self) -> PathBuf {
        self.dir.join("station.db")
    }
}

impl Drop for TmpDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn assert_one_row(s: &Station, id: &str, state: State) -> Task {
    let task = s
        .get(id)
        .unwrap()
        .unwrap_or_else(|| panic!("missing row {id}"));
    assert_eq!(task.state, state);
    assert_eq!(
        s.count_by_state(State::Queued).unwrap()
            + s.count_by_state(State::Claimed).unwrap()
            + s.count_by_state(State::Done).unwrap(),
        1,
        "ledger must keep exactly one row"
    );
    task
}

/// 8 个工位同时认领并立刻完成同一条任务：恰好一次认领、恰好一次完成。
#[test]
fn eight_threads_claim_and_complete_one_task_exactly_once() {
    let tmp = TmpDb::new("race8");
    let db = tmp.path();
    {
        let s = Station::open(&db).unwrap();
        s.new_task("t1", "payload://hot", "reviewer", Some("hot"))
            .unwrap();
    }
    const N: usize = 8;
    let barrier = Arc::new(Barrier::new(N));
    let mut handles = Vec::with_capacity(N);
    for i in 0..N {
        let barrier = Arc::clone(&barrier);
        let db = db.clone();
        handles.push(thread::spawn(move || {
            let s = Station::open(&db).unwrap();
            let worker = format!("w{i}");
            barrier.wait();
            let claimed = s.claim(&worker, "reviewer", 60_000).unwrap().is_some();
            let completed = s.complete("t1", &worker).unwrap();
            (claimed, completed)
        }));
    }
    let results: Vec<(bool, bool)> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let claims = results.iter().filter(|(claimed, _)| *claimed).count();
    let completes = results.iter().filter(|(_, completed)| *completed).count();
    assert_eq!(claims, 1, "同一任务只能被一个工位认领: {results:?}");
    assert_eq!(completes, 1, "完成必须恰好一次: {results:?}");
    assert!(
        results
            .iter()
            .all(|(claimed, completed)| *completed == *claimed),
        "只有认领者能完成: {results:?}"
    );
    let s = Station::open(&db).unwrap();
    let task = assert_one_row(&s, "t1", State::Done);
    let claimant = task.claimant.as_deref().unwrap();
    assert!(!s.complete("t1", claimant).unwrap());
    assert!(s.get("t1").unwrap().is_some());
}

/// 每条任务被 8 个连接以正确 claimant 同时 complete：成功次数等于行数，行不丢。
#[test]
fn concurrent_complete_does_not_drop_ledger_rows() {
    const N: usize = 32;
    const THREADS: usize = 8;
    let tmp = TmpDb::new("complete");
    let db = tmp.path();
    let mut owners = Vec::with_capacity(N);
    {
        let s = Station::open(&db).unwrap();
        for i in 0..N {
            s.new_task(&format!("t{i:02}"), "p", "coder", None).unwrap();
        }
        for i in 0..N {
            let task = s
                .claim(&format!("w{}", i % 4), "coder", 60_000)
                .unwrap()
                .unwrap();
            owners.push((task.id, task.claimant.clone().unwrap()));
        }
        assert_eq!(s.count_by_state(State::Claimed).unwrap(), N as i64);
    }
    let owners = Arc::new(owners);
    let barrier = Arc::new(Barrier::new(THREADS));
    let oks = Arc::new(Mutex::new(0usize));
    let mut handles = Vec::with_capacity(THREADS);
    for _ in 0..THREADS {
        let owners = Arc::clone(&owners);
        let barrier = Arc::clone(&barrier);
        let oks = Arc::clone(&oks);
        let db = db.clone();
        handles.push(thread::spawn(move || {
            let s = Station::open(&db).unwrap();
            barrier.wait();
            let mut local = 0usize;
            for (id, claimant) in owners.iter() {
                if s.complete(id, claimant).unwrap() {
                    local += 1;
                }
            }
            *oks.lock().unwrap() += local;
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(*oks.lock().unwrap(), N);
    let s = Station::open(&db).unwrap();
    assert_eq!(s.count_by_state(State::Done).unwrap(), N as i64);
    assert_eq!(s.count_by_state(State::Claimed).unwrap(), 0);
    assert_eq!(s.count_by_state(State::Queued).unwrap(), 0);
    for (id, claimant) in owners.iter() {
        let task = s
            .get(id)
            .unwrap()
            .unwrap_or_else(|| panic!("missing row {id}"));
        assert_eq!(task.state, State::Done);
        assert_eq!(task.claimant.as_deref(), Some(claimant.as_str()));
        assert!(!s.complete(id, claimant).unwrap());
        assert!(s.get(id).unwrap().is_some(), "repeat complete dropped {id}");
    }
}

/// 非主人并发 complete 全部失败，已认领的行保持原 claimant。
#[test]
fn foreign_complete_storm_leaves_every_claimed_row() {
    const N: usize = 16;
    const THREADS: usize = 8;
    let tmp = TmpDb::new("foreign");
    let db = tmp.path();
    let mut owners = Vec::with_capacity(N);
    {
        let s = Station::open(&db).unwrap();
        for i in 0..N {
            s.new_task(&format!("t{i:02}"), "p", "coder", None).unwrap();
            let task = s.claim("owner", "coder", 60_000).unwrap().unwrap();
            owners.push(task.id);
        }
    }
    let owners = Arc::new(owners);
    let barrier = Arc::new(Barrier::new(THREADS));
    let oks = Arc::new(Mutex::new(0usize));
    let mut handles = Vec::with_capacity(THREADS);
    for i in 0..THREADS {
        let owners = Arc::clone(&owners);
        let barrier = Arc::clone(&barrier);
        let oks = Arc::clone(&oks);
        let db = db.clone();
        handles.push(thread::spawn(move || {
            let s = Station::open(&db).unwrap();
            let intruder = format!("intruder-{i}");
            barrier.wait();
            let mut local = 0usize;
            for id in owners.iter() {
                if s.complete(id, &intruder).unwrap() {
                    local += 1;
                }
            }
            *oks.lock().unwrap() += local;
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(*oks.lock().unwrap(), 0);
    let s = Station::open(&db).unwrap();
    assert_eq!(s.count_by_state(State::Claimed).unwrap(), N as i64);
    assert_eq!(s.count_by_state(State::Done).unwrap(), 0);
    for id in owners.iter() {
        let task = s
            .get(id)
            .unwrap()
            .unwrap_or_else(|| panic!("missing row {id}"));
        assert_eq!(task.state, State::Claimed);
        assert_eq!(task.claimant.as_deref(), Some("owner"));
    }
}

/// 每个线程先完成手里的上一单，再去认领下一单。认领集合互斥，行全部变成 done。
#[test]
fn interleaved_claim_and_complete_partition_without_losing_rows() {
    const N: usize = 40;
    const THREADS: usize = 4;
    let tmp = TmpDb::new("pipeline");
    let db = tmp.path();
    {
        let s = Station::open(&db).unwrap();
        for i in 0..N {
            s.new_task(&format!("t{i:02}"), "p", "coder", None).unwrap();
        }
    }
    let barrier = Arc::new(Barrier::new(THREADS));
    let mut handles = Vec::with_capacity(THREADS);
    for w in 0..THREADS {
        let barrier = Arc::clone(&barrier);
        let db = db.clone();
        handles.push(thread::spawn(move || {
            let s = Station::open(&db).unwrap();
            let worker = format!("w{w}");
            let mut done = Vec::new();
            let mut held: Option<Task> = None;
            barrier.wait();
            loop {
                if let Some(prev) = held.take() {
                    assert!(
                        s.complete(&prev.id, &worker).unwrap(),
                        "owner complete failed for {}",
                        prev.id
                    );
                    done.push(prev.id);
                }
                match s.claim(&worker, "coder", 60_000).unwrap() {
                    Some(task) => held = Some(task),
                    None => break,
                }
            }
            if let Some(task) = held {
                assert!(s.complete(&task.id, &worker).unwrap());
                done.push(task.id);
            }
            done
        }));
    }
    let mut all = Vec::new();
    for handle in handles {
        all.extend(handle.join().unwrap());
    }
    all.sort();
    let total = all.len();
    all.dedup();
    assert_eq!(total, N, "每个任务恰好被认领并完成一次");
    assert_eq!(all.len(), N, "认领集合必须互斥");
    let s = Station::open(&db).unwrap();
    assert_eq!(s.count_by_state(State::Done).unwrap(), N as i64);
    assert_eq!(s.count_by_state(State::Queued).unwrap(), 0);
    assert_eq!(s.count_by_state(State::Claimed).unwrap(), 0);
    for id in &all {
        let task = s
            .get(id)
            .unwrap()
            .unwrap_or_else(|| panic!("missing row {id}"));
        assert_eq!(task.state, State::Done);
    }
}

/// 租约仍有效时丢掉连接：新连接看到同一行，原主可以心跳并完成，已完成的单不会复活。
#[test]
fn resume_while_lease_holds_is_reentrant() {
    let clock = ManualClock::new(1_000_000);
    let tmp = TmpDb::new("resume-live");
    let db = tmp.path();
    {
        let s = Station::open_with_clock(&db, clock.clone()).unwrap();
        s.new_task("keep", "p", "reviewer", Some("k-keep")).unwrap();
        s.new_task("open", "p", "reviewer", Some("k-open")).unwrap();
        let first = s.claim("wA", "reviewer", 5_000).unwrap().unwrap();
        assert_eq!(first.id, "keep");
        assert!(s.complete("keep", "wA").unwrap());
        let second = s.claim("wA", "reviewer", 5_000).unwrap().unwrap();
        assert_eq!(second.id, "open");
        assert_eq!(second.lease_until, Some(1_005_000));
    }
    let s = Station::open_with_clock(&db, clock.clone()).unwrap();
    assert_eq!(s.count_by_state(State::Done).unwrap(), 1);
    assert_eq!(s.count_by_state(State::Claimed).unwrap(), 1);
    assert_eq!(s.count_by_state(State::Queued).unwrap(), 0);
    assert!(!s.complete("keep", "wA").unwrap());
    assert!(s.claim("wB", "reviewer", 5_000).unwrap().is_none());
    let open = s.get("open").unwrap().unwrap();
    assert_eq!(open.state, State::Claimed);
    assert_eq!(open.claimant.as_deref(), Some("wA"));
    assert_eq!(open.lease_until, Some(1_005_000));
    clock.advance(10);
    assert!(s.heartbeat("open", "wA", 5_000).unwrap());
    assert_eq!(s.get("open").unwrap().unwrap().lease_until, Some(1_005_010));
    assert!(s.complete("open", "wA").unwrap());
    assert_eq!(s.get("open").unwrap().unwrap().state, State::Done);
    assert_eq!(s.get("keep").unwrap().unwrap().state, State::Done);
    assert_eq!(
        s.new_task("open-dup", "p", "reviewer", Some("k-open"))
            .unwrap(),
        Enqueued::Duplicate {
            existing_id: "open".into()
        }
    );
    assert_eq!(s.count_by_state(State::Done).unwrap(), 2);
}

/// 过期后的续点是 queued。相等的截止点不算过期；越过 1 毫秒后同一行可再认领。
#[test]
fn expired_interrupt_reenters_queued_resume_point() {
    let clock = ManualClock::new(2_000_000);
    let tmp = TmpDb::new("reenter");
    let db = tmp.path();
    {
        let s = Station::open_with_clock(&db, clock.clone()).unwrap();
        s.new_task("t1", "p", "reviewer", Some("k1")).unwrap();
        let task = s.claim("wA", "reviewer", 100).unwrap().unwrap();
        assert_eq!(task.lease_until, Some(2_000_100));
    }
    clock.set(2_000_100);
    {
        let s = Station::open_with_clock(&db, clock.clone()).unwrap();
        assert_eq!(s.reclaim_expired().unwrap(), 0);
        assert_eq!(s.get("t1").unwrap().unwrap().state, State::Claimed);
    }
    clock.set(2_000_101);
    let s = Station::open_with_clock(&db, clock.clone()).unwrap();
    assert!(!s.heartbeat("t1", "wA", 100).unwrap());
    assert_eq!(s.reclaim_expired().unwrap(), 1);
    let queued = s.get("t1").unwrap().unwrap();
    assert_eq!(queued.state, State::Queued);
    assert_eq!(queued.claimant, None);
    assert_eq!(queued.lease_until, None);
    assert_eq!(queued.idempotency_key.as_deref(), Some("k1"));
    let again = s.claim("wA", "reviewer", 100).unwrap().unwrap();
    assert_eq!(again.id, "t1");
    assert_eq!(again.claimant.as_deref(), Some("wA"));
    assert_eq!(again.lease_until, Some(2_000_201));
    assert_eq!(s.count_by_state(State::Claimed).unwrap(), 1);
    assert_eq!(s.count_by_state(State::Queued).unwrap(), 0);
    assert_eq!(
        s.new_task("t1-dup", "p", "reviewer", Some("k1")).unwrap(),
        Enqueued::Duplicate {
            existing_id: "t1".into()
        }
    );
    assert!(s.complete("t1", "wA").unwrap());
    assert!(!s.complete("t1", "wA").unwrap());
    clock.advance(10_000);
    assert_eq!(s.reclaim_expired().unwrap(), 0);
    let done = s.get("t1").unwrap().unwrap();
    assert_eq!(done.state, State::Done);
    assert_eq!(done.claimant.as_deref(), Some("wA"));
}

/// 完成不看租约。回收前过期原主仍能完成；先回收则原主失败、新主认领同一行。
#[test]
fn expired_complete_before_reclaim_and_reclaim_before_complete() {
    let clock = ManualClock::new(3_000_000);
    let tmp = TmpDb::new("order");
    let db = tmp.path();
    {
        let s = Station::open_with_clock(&db, clock.clone()).unwrap();
        s.new_task("finish", "p", "reviewer", Some("k-finish"))
            .unwrap();
        s.new_task("hand", "p", "reviewer", Some("k-hand")).unwrap();
        assert_eq!(s.claim("wA", "reviewer", 10).unwrap().unwrap().id, "finish");
        assert_eq!(s.claim("wA", "reviewer", 10).unwrap().unwrap().id, "hand");
    }
    clock.set(3_000_011);
    let s = Station::open_with_clock(&db, clock.clone()).unwrap();
    assert!(s.complete("finish", "wA").unwrap());
    let finish = s.get("finish").unwrap().unwrap();
    assert_eq!(finish.state, State::Done);
    assert_eq!(finish.claimant.as_deref(), Some("wA"));

    assert_eq!(s.reclaim_expired().unwrap(), 1);
    assert!(!s.complete("hand", "wA").unwrap());
    let hand = s.get("hand").unwrap().unwrap();
    assert_eq!(hand.state, State::Queued);
    assert_eq!(hand.claimant, None);
    let taken = s.claim("wB", "reviewer", 10).unwrap().unwrap();
    assert_eq!(taken.id, "hand");
    assert_eq!(taken.claimant.as_deref(), Some("wB"));
    assert!(!s.complete("hand", "wA").unwrap());
    assert!(s.complete("hand", "wB").unwrap());

    assert_eq!(s.count_by_state(State::Done).unwrap(), 2);
    assert_eq!(s.count_by_state(State::Queued).unwrap(), 0);
    assert_eq!(s.count_by_state(State::Claimed).unwrap(), 0);
    assert!(s.get("finish").unwrap().is_some());
    assert!(s.get("hand").unwrap().is_some());
}

/// 过期后，原主 complete 与新主 claim 一起跑。两种交错都只留一行，且只有一方成功。
#[test]
fn expired_owner_complete_races_takeover_keeps_one_row() {
    let clock = ManualClock::new(8_000_000);
    let tmp = TmpDb::new("takeover-race");
    let db = tmp.path();
    {
        let s = Station::open_with_clock(&db, clock.clone()).unwrap();
        s.new_task("t1", "p", "reviewer", Some("k1")).unwrap();
        let task = s.claim("wA", "reviewer", 50).unwrap().unwrap();
        assert_eq!(task.lease_until, Some(8_000_050));
    }
    clock.set(8_000_051);
    let barrier = Arc::new(Barrier::new(2));
    let db_complete = db.clone();
    let db_claim = db.clone();
    let barrier_complete = Arc::clone(&barrier);
    let barrier_claim = Arc::clone(&barrier);
    let clock_complete = clock.clone();
    let clock_claim = clock.clone();
    let complete_thread = thread::spawn(move || {
        let s = Station::open_with_clock(&db_complete, clock_complete).unwrap();
        barrier_complete.wait();
        s.complete("t1", "wA").unwrap()
    });
    let claim_thread = thread::spawn(move || {
        let s = Station::open_with_clock(&db_claim, clock_claim).unwrap();
        barrier_claim.wait();
        s.claim("wB", "reviewer", 50)
            .unwrap()
            .map(|task| task.claimant.unwrap())
    });
    let completed = complete_thread.join().unwrap();
    let claimed_by = claim_thread.join().unwrap();

    let s = Station::open_with_clock(&db, clock.clone()).unwrap();
    let task = s.get("t1").unwrap().expect("ledger row missing");
    assert_eq!(
        s.count_by_state(State::Queued).unwrap()
            + s.count_by_state(State::Claimed).unwrap()
            + s.count_by_state(State::Done).unwrap(),
        1
    );
    match (completed, claimed_by.as_deref()) {
        (true, None) => {
            assert_eq!(task.state, State::Done);
            assert_eq!(task.claimant.as_deref(), Some("wA"));
        }
        (false, Some("wB")) => {
            assert_eq!(task.state, State::Claimed);
            assert_eq!(task.claimant.as_deref(), Some("wB"));
            assert!(!s.complete("t1", "wA").unwrap());
            assert!(s.complete("t1", "wB").unwrap());
        }
        other => panic!("incoherent resume race: {other:?}"),
    }
    clock.advance(10_000);
    assert_eq!(s.reclaim_expired().unwrap(), 0);
    let done = s.get("t1").unwrap().expect("row missing after reentry");
    assert_eq!(done.state, State::Done);
    assert_eq!(
        s.new_task("t1-dup", "p", "reviewer", Some("k1")).unwrap(),
        Enqueued::Duplicate {
            existing_id: "t1".into()
        }
    );
}
