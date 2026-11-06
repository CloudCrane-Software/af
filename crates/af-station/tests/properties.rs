//! 账本操作序列的属性测试。
//!
//! 单连接序列对照一个影子模型：入账、认领、心跳、完成、回收、推进时刻。
//! 序列中点丢掉连接再打开，锁住「中断后续点可重入」。
//! 序列结束后把时刻推过全部租约，未完成的行回到 queued 并能再次认领，行数不变。
//!
//! 另两组并发：多线程认领/完成是任务集合的一个划分；并发 complete 不删行。
//! 失败不写 `proptest-regressions/`。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use af_station::{Enqueued, Error, ManualClock, State, Station};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

fn config(cases: u32) -> ProptestConfig {
    ProptestConfig {
        cases,
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        ..ProptestConfig::default()
    }
}

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

#[derive(Clone, Debug)]
enum Op {
    Advance(u16),
    Enqueue { id: u8, role: u8, key: Option<u8> },
    Claim { worker: u8, role: u8, lease: u16 },
    Heartbeat { slot: u8, worker: u8, extend: u16 },
    Complete { slot: u8, worker: u8 },
    Reclaim,
}

fn arb_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        2 => (0u16..=3_000).prop_map(Op::Advance),
        3 => (0u8..4, 0u8..2, prop::option::of(0u8..3)).prop_map(|(id, role, key)| Op::Enqueue { id, role, key }),
        3 => (0u8..4, 0u8..2, 0u16..=3_000).prop_map(|(worker, role, lease)| Op::Claim { worker, role, lease }),
        2 => (0u8..4, 0u8..4, 0u16..=3_000).prop_map(|(slot, worker, extend)| Op::Heartbeat { slot, worker, extend }),
        2 => (0u8..4, 0u8..4).prop_map(|(slot, worker)| Op::Complete { slot, worker }),
        2 => Just(Op::Reclaim),
    ]
}

fn tid(id: u8) -> String {
    format!("t{id}")
}

fn rid(role: u8) -> String {
    format!("r{role}")
}

fn wid(worker: u8) -> String {
    format!("w{worker}")
}

fn kid(key: u8) -> String {
    format!("k{key}")
}

#[derive(Clone, Debug)]
struct Row {
    id: String,
    role: String,
    state: State,
    claimant: Option<String>,
    lease_until: Option<i64>,
    key: Option<String>,
}

#[derive(Clone, Debug)]
struct Model {
    rows: Vec<Row>,
    now: i64,
}

impl Model {
    fn count(&self, state: State) -> i64 {
        self.rows.iter().filter(|row| row.state == state).count() as i64
    }

    fn enqueue(&mut self, id: &str, role: &str, key: Option<&str>) -> Result<Enqueued, ()> {
        if let Some(k) = key {
            if let Some(existing) = self.rows.iter().find(|row| row.key.as_deref() == Some(k)) {
                return Ok(Enqueued::Duplicate {
                    existing_id: existing.id.clone(),
                });
            }
        }
        if self.rows.iter().any(|row| row.id == id) {
            return Err(());
        }
        self.rows.push(Row {
            id: id.to_string(),
            role: role.to_string(),
            state: State::Queued,
            claimant: None,
            lease_until: None,
            key: key.map(str::to_string),
        });
        Ok(Enqueued::Inserted { id: id.to_string() })
    }

    fn reclaim_at(&mut self, now: i64) -> usize {
        let mut n = 0usize;
        for row in &mut self.rows {
            let expired = row.lease_until.is_some_and(|lease| lease < now);
            if row.state == State::Claimed && expired {
                row.state = State::Queued;
                row.claimant = None;
                row.lease_until = None;
                n += 1;
            }
        }
        n
    }

    fn claim(&mut self, worker: &str, role: &str, lease_ms: u64) -> Option<String> {
        // 本序列不绑定班次。未绑定不设班次门，认领仍只按角色精确匹配。
        let now = self.now;
        self.reclaim_at(now);
        let lease_until = now + lease_ms as i64;
        let mut seen = 0usize;
        let mut chosen = None;
        for (idx, row) in self.rows.iter().enumerate() {
            if row.state == State::Queued && row.role == role {
                if seen == 64 {
                    break;
                }
                seen += 1;
                if row.claimant.is_none() {
                    chosen = Some(idx);
                    break;
                }
            }
        }
        let idx = chosen?;
        let row = &mut self.rows[idx];
        row.state = State::Claimed;
        row.claimant = Some(worker.to_string());
        row.lease_until = Some(lease_until);
        Some(row.id.clone())
    }

    fn heartbeat(&mut self, id: &str, worker: &str, extend_ms: u64) -> bool {
        let now = self.now;
        let Some(row) = self.rows.iter_mut().find(|row| row.id == id) else {
            return false;
        };
        let live = row.lease_until.is_some_and(|lease| lease > now);
        if row.claimant.as_deref() == Some(worker) && row.state == State::Claimed && live {
            row.lease_until = Some(now + extend_ms as i64);
            true
        } else {
            false
        }
    }

    fn complete(&mut self, id: &str, worker: &str) -> bool {
        let Some(row) = self.rows.iter_mut().find(|row| row.id == id) else {
            return false;
        };
        if row.claimant.as_deref() == Some(worker) && row.state == State::Claimed {
            row.state = State::Done;
            true
        } else {
            false
        }
    }
}

fn assert_ledger(station: &Station, model: &Model, step: usize) {
    let mut ids: Vec<&str> = model.rows.iter().map(|row| row.id.as_str()).collect();
    let total = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), total, "step {step}: duplicate id in model");
    for id in 0..4u8 {
        let key = tid(id);
        let got = station.get(&key).unwrap();
        let expected = model.rows.iter().find(|row| row.id == key);
        match (&got, expected) {
            (None, None) => {}
            (Some(task), Some(row)) => {
                assert_eq!(task.payload_ref, "p", "step {step} {key}");
                assert_eq!(task.required_role, row.role, "step {step} {key}");
                assert_eq!(task.state, row.state, "step {step} {key}");
                assert_eq!(task.claimant, row.claimant, "step {step} {key}");
                assert_eq!(task.lease_until, row.lease_until, "step {step} {key}");
                assert_eq!(task.idempotency_key, row.key, "step {step} {key}");
                assert_eq!(task.deadline, None, "step {step} {key}");
            }
            _ => panic!("step {step} id {key}: station {got:?} model {expected:?}"),
        }
    }
    assert_eq!(
        station.count_by_state(State::Queued).unwrap(),
        model.count(State::Queued),
        "step {step} queued"
    );
    assert_eq!(
        station.count_by_state(State::Claimed).unwrap(),
        model.count(State::Claimed),
        "step {step} claimed"
    );
    assert_eq!(
        station.count_by_state(State::Done).unwrap(),
        model.count(State::Done),
        "step {step} done"
    );
    for row in &model.rows {
        match row.state {
            State::Queued => {
                assert!(row.claimant.is_none(), "step {step} {:?}", row.id);
                assert!(row.lease_until.is_none(), "step {step} {:?}", row.id);
            }
            State::Claimed => {
                assert!(row.claimant.is_some(), "step {step} {:?}", row.id);
                assert!(row.lease_until.is_some(), "step {step} {:?}", row.id);
            }
            State::Done => {
                assert!(row.claimant.is_some(), "step {step} {:?}", row.id);
            }
        }
    }
}

fn apply(station: &Station, clock: &ManualClock, model: &mut Model, op: &Op) {
    match op {
        Op::Advance(delta) => {
            clock.advance(u64::from(*delta));
            model.now = clock.now_ms();
        }
        Op::Enqueue { id, role, key } => {
            let id = tid(*id);
            let role = rid(*role);
            let key = key.map(kid);
            let from_station = station.new_task(&id, "p", &role, key.as_deref());
            let from_model = model.enqueue(&id, &role, key.as_deref());
            match (from_station, from_model) {
                (Ok(left), Ok(right)) => assert_eq!(left, right),
                (Err(Error::Sqlite(err)), Err(())) => {
                    let msg = err.to_string();
                    assert!(
                        msg.contains("UNIQUE") || msg.contains("constraint"),
                        "pk conflict text: {msg}"
                    );
                }
                (left, right) => panic!("enqueue diverge: station {left:?} model {right:?}"),
            }
        }
        Op::Claim {
            worker,
            role,
            lease,
        } => {
            let worker = wid(*worker);
            let role = rid(*role);
            let from_station = station.claim(&worker, &role, u64::from(*lease)).unwrap();
            let from_model = model.claim(&worker, &role, u64::from(*lease));
            assert_eq!(
                from_station.as_ref().map(|task| task.id.as_str()),
                from_model.as_deref()
            );
        }
        Op::Heartbeat {
            slot,
            worker,
            extend,
        } => {
            let id = tid(*slot);
            let worker = wid(*worker);
            let from_station = station.heartbeat(&id, &worker, u64::from(*extend)).unwrap();
            let from_model = model.heartbeat(&id, &worker, u64::from(*extend));
            assert_eq!(from_station, from_model);
        }
        Op::Complete { slot, worker } => {
            let id = tid(*slot);
            let worker = wid(*worker);
            let from_station = station.complete(&id, &worker).unwrap();
            let from_model = model.complete(&id, &worker);
            assert_eq!(from_station, from_model);
        }
        Op::Reclaim => {
            let from_station = station.reclaim_expired().unwrap();
            let from_model = model.reclaim_at(model.now);
            assert_eq!(from_station, from_model);
        }
    }
}

fn reopen(path: &PathBuf, clock: &ManualClock) -> Station {
    Station::open_with_clock(path, clock.clone())
        .unwrap()
        .into_station()
}

/// 时刻跳过全部租约，丢掉连接再打开。未完成的行回到 queued，同一批 id 可再认领。
fn resume_after_clock_jump(path: &PathBuf, clock: &ManualClock, model: &mut Model) {
    let before = model.rows.len();
    clock.set(model.now.saturating_add(100_000_000));
    model.now = clock.now_ms();
    let station = reopen(path, clock);
    let from_station = station.reclaim_expired().unwrap();
    let from_model = model.reclaim_at(model.now);
    assert_eq!(from_station, from_model);
    assert_eq!(model.rows.len(), before, "reclaim must not drop rows");
    assert_ledger(&station, model, usize::MAX);
    for role in [0u8, 1] {
        loop {
            let worker = "w-resume";
            let role_name = rid(role);
            let from_station = station.claim(worker, &role_name, 1_000).unwrap();
            let from_model = model.claim(worker, &role_name, 1_000);
            match (from_station, from_model) {
                (None, None) => break,
                (Some(task), Some(id)) => {
                    assert_eq!(task.id, id);
                    assert!(station.complete(&task.id, worker).unwrap());
                    assert!(model.complete(&id, worker));
                }
                (station_task, model_id) => {
                    panic!("resume claim diverge: station {station_task:?} model {model_id:?}")
                }
            }
        }
    }
    assert_eq!(
        model.rows.len(),
        before,
        "reclaim+reclaim must not drop rows"
    );
    assert_ledger(&station, model, usize::MAX);
    for row in &model.rows {
        assert_eq!(row.state, State::Done, "every row reaches done on reentry");
    }
}

fn run_sequence(ops: &[Op]) {
    let tmp = TmpDb::new("prop-seq");
    let path = tmp.path();
    let clock = ManualClock::new(1_000_000_000_000);
    let mut model = Model {
        rows: Vec::new(),
        now: clock.now_ms(),
    };
    let mut station = reopen(&path, &clock);
    assert_ledger(&station, &model, 0);
    let reopen_at = ops.len() / 2;
    for (index, op) in ops.iter().enumerate() {
        if index == reopen_at {
            drop(station);
            station = reopen(&path, &clock);
            assert_ledger(&station, &model, index);
        }
        apply(&station, &clock, &mut model, op);
        assert_ledger(&station, &model, index + 1);
    }
    drop(station);
    resume_after_clock_jump(&path, &clock, &mut model);
}

proptest! {
    #![proptest_config(config(96))]
    #[test]
    fn op_sequence_matches_model_across_reopen(ops in prop::collection::vec(arb_op(), 1..=16)) {
        run_sequence(&ops);
    }
}

fn concurrent_partition(n_tasks: usize, n_threads: usize) {
    let tmp = TmpDb::new("prop-part");
    let path = tmp.path();
    let clock = ManualClock::new(9_000_000);
    {
        let station = Station::open_with_clock(&path, clock.clone()).unwrap();
        for i in 0..n_tasks {
            station
                .new_task(&format!("t{i}"), "p", "coder", None)
                .unwrap();
        }
    }
    let barrier = Arc::new(Barrier::new(n_threads));
    let mut handles = Vec::with_capacity(n_threads);
    for worker_idx in 0..n_threads {
        let barrier = Arc::clone(&barrier);
        let path = path.clone();
        let clock = clock.clone();
        handles.push(thread::spawn(move || {
            let station = Station::open_with_clock(&path, clock).unwrap();
            let worker = format!("w{worker_idx}");
            let mut done: Vec<String> = Vec::new();
            let mut held: Option<String> = None;
            barrier.wait();
            loop {
                if let Some(prev) = held.take() {
                    assert!(station.complete(&prev, &worker).unwrap());
                    done.push(prev);
                }
                match station.claim(&worker, "coder", 60_000).unwrap() {
                    Some(task) => held = Some(task.id),
                    None => break,
                }
            }
            if let Some(id) = held {
                assert!(station.complete(&id, &worker).unwrap());
                done.push(id);
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
    assert_eq!(total, n_tasks);
    assert_eq!(all.len(), n_tasks);
    let station = Station::open_with_clock(&path, clock).unwrap();
    assert_eq!(station.count_by_state(State::Done).unwrap(), n_tasks as i64);
    assert_eq!(station.count_by_state(State::Queued).unwrap(), 0);
    assert_eq!(station.count_by_state(State::Claimed).unwrap(), 0);
    for i in 0..n_tasks {
        let task = station.get(&format!("t{i}")).unwrap().expect("row missing");
        assert_eq!(task.state, State::Done);
        assert!(task.claimant.is_some());
    }
}

fn concurrent_complete(n_tasks: usize, n_threads: usize) {
    let tmp = TmpDb::new("prop-done");
    let path = tmp.path();
    let clock = ManualClock::new(7_000_000);
    let mut owners = Vec::with_capacity(n_tasks);
    {
        let station = Station::open_with_clock(&path, clock.clone()).unwrap();
        for i in 0..n_tasks {
            let id = format!("t{i}");
            station.new_task(&id, "p", "coder", None).unwrap();
            let task = station
                .claim(&format!("w{}", i % 3), "coder", 60_000)
                .unwrap()
                .unwrap();
            owners.push((task.id, task.claimant.unwrap()));
        }
    }
    let owners = Arc::new(owners);
    let barrier = Arc::new(Barrier::new(n_threads));
    let oks = Arc::new(Mutex::new(0usize));
    let mut handles = Vec::with_capacity(n_threads);
    for _ in 0..n_threads {
        let owners = Arc::clone(&owners);
        let barrier = Arc::clone(&barrier);
        let oks = Arc::clone(&oks);
        let path = path.clone();
        let clock = clock.clone();
        handles.push(thread::spawn(move || {
            let station = Station::open_with_clock(&path, clock).unwrap();
            barrier.wait();
            let mut local = 0usize;
            for (id, claimant) in owners.iter() {
                if station.complete(id, claimant).unwrap() {
                    local += 1;
                }
            }
            *oks.lock().unwrap() += local;
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(*oks.lock().unwrap(), n_tasks);
    let station = Station::open_with_clock(&path, clock).unwrap();
    assert_eq!(station.count_by_state(State::Done).unwrap(), n_tasks as i64);
    assert_eq!(station.count_by_state(State::Claimed).unwrap(), 0);
    assert_eq!(station.count_by_state(State::Queued).unwrap(), 0);
    for (id, claimant) in owners.iter() {
        let task = station.get(id).unwrap().expect("row missing");
        assert_eq!(task.state, State::Done);
        assert_eq!(task.claimant.as_deref(), Some(claimant.as_str()));
    }
}

proptest! {
    #![proptest_config(config(24))]
    #[test]
    fn concurrent_claim_complete_is_a_partition(
        n_tasks in 1usize..=12,
        n_threads in 2usize..=6,
    ) {
        concurrent_partition(n_tasks, n_threads);
    }

    #[test]
    fn concurrent_complete_keeps_every_row(
        n_tasks in 1usize..=12,
        n_threads in 2usize..=6,
    ) {
        concurrent_complete(n_tasks, n_threads);
    }
}
