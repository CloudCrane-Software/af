//! 守恒不变式（PoC-B 验收硬指标，v0.3 §二 阶段1 B）：
//!
//! 1. 件数守恒：opening+admitted == closed+cancelled+wip；
//! 2. 权重守恒：同式（f64，容差 1e-9 标准审阅分钟）；
//! 3. 0 ≤ wip ≤ upper（正式准入路径；影子记账路径 `try_admit_uncapped` 明确豁免，
//!    只在影子对照期入账已发生的事实）；
//! 4. 同一 order 最多一张有效 Permit（在制期间重复投放被拒，且拒绝零副作用）；
//! 5. settle 幂等：重复 complete/cancel 不重复释容；
//! 6. 拒绝不半提交：admit 失败时 wip/守恒计数/夜间信用/渐放游标零变化；
//! 7. 重启不超放：重开账本后 wip 原样读回，继续准入仍守上限；
//! 8. item_id 单调（缺陷回归）：旧法"队列现存 max+1"会在旧件出队后把同一 id 发给
//!    新单，迟到的旧 complete 误释新单；现改为持久化单调 next_item_id。
//!
//! 全部用例跑在 [`DurableGate`]（SQLite 账本）上，重启路径真实落盘重开。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use af_release_gate::{
    AdmitError, Conservation, DurableGate, GateConfig, GateError, Item, OpenOutcome, PersistError,
    SCHEMA_VERSION,
};
use proptest::prelude::*;

struct TmpDb {
    path: PathBuf,
}

impl TmpDb {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "af-release-gate-conservation-{label}-{}-{n}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        Self { path }
    }

    fn path(&self) -> &std::path::Path {
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

fn cfg(upper: f64, lower: f64) -> GateConfig {
    GateConfig {
        wip_upper_minutes: upper,
        wip_lower_minutes: lower,
        night_window: None,
    }
}

fn order_item(id: u64, order: &str, weight: f64) -> Item {
    Item {
        id,
        order_id: order.to_string(),
        config: "v1".to_string(),
        review_minutes: weight,
    }
}

/// 守恒断言：件数精确、权重容差、wip 在 [0, upper]。
fn assert_conservation(gate: &DurableGate, upper: f64) {
    let c: Conservation = gate.conservation();
    assert!(c.count_balanced(), "count conservation broken: {c:?}");
    assert!(c.weight_balanced(), "weight conservation broken: {c:?}");
    let wip = gate.wip_minutes();
    assert!(wip >= 0.0, "negative wip {wip}");
    assert!(
        wip <= upper + 1e-9,
        "wip {wip} over upper {upper} (conservation {c:?})"
    );
}

// ---------- 3/6/7：确定性骨架 ----------

#[test]
fn admit_respects_cap_and_never_partial_commits() {
    let tmp = TmpDb::new("cap");
    let declaration = cfg(8.0, 2.0);
    let (mut gate, outcome) = DurableGate::open(tmp.path(), &declaration, None).unwrap();
    assert_eq!(outcome, OpenOutcome::Created);

    for i in 1..=8u64 {
        let t = gate
            .try_admit(&order_item(i, &format!("A{i:03}"), 1.0), i as u32)
            .unwrap();
        assert_eq!(t.item_id, i);
        assert_conservation(&gate, 8.0);
    }

    // 满了：拒绝，且零变化（wip/计数/游标）。
    let before = gate.conservation();
    let before_next = gate.next_item_id();
    let rejected = gate.try_admit(&order_item(9, "A009", 1.0), 9).unwrap_err();
    assert!(matches!(
        rejected,
        PersistError::Admit(AdmitError::WipCap { .. })
    ));
    assert_eq!(
        gate.conservation(),
        before,
        "reject must not move the books"
    );
    assert_eq!(
        gate.next_item_id(),
        before_next,
        "reject must not spend an id"
    );

    // 一件出队后仍放不下（wip=8 而非 9 被拒过）：先 complete 一件再放行。
    gate.complete(3, 20, 0).unwrap();
    assert_conservation(&gate, 8.0);
    gate.try_admit(&order_item(10, "A010", 1.0), 21).unwrap();
    assert_conservation(&gate, 8.0);
    let rejected = gate
        .try_admit(&order_item(11, "A011", 1.0), 22)
        .unwrap_err();
    assert!(matches!(
        rejected,
        PersistError::Admit(AdmitError::WipCap { .. })
    ));
}

#[test]
fn duplicate_order_gets_at_most_one_permit() {
    let tmp = TmpDb::new("dup");
    let (mut gate, _) = DurableGate::open(tmp.path(), &cfg(8.0, 2.0), None).unwrap();
    gate.try_admit(&order_item(1, "A001", 1.0), 1).unwrap();

    let before = gate.conservation();
    let dup = gate.try_admit(&order_item(2, "A001", 1.0), 2).unwrap_err();
    match dup {
        PersistError::Admit(AdmitError::DuplicateOrder { order_id, item_id }) => {
            assert_eq!(order_id, "A001");
            assert_eq!(item_id, 1);
        }
        other => panic!("expected DuplicateOrder, got {other:?}"),
    }
    assert_eq!(gate.conservation(), before, "duplicate reject is a no-op");
    assert_eq!(gate.in_process().len(), 1);

    // settle 后同一 order 可以再投（新一张 Permit，新 id）。
    gate.complete(1, 5, 0).unwrap();
    let again = gate.try_admit(&order_item(3, "A001", 1.0), 6).unwrap();
    assert_eq!(again.item_id, 3);
    assert_conservation(&gate, 8.0);
}

#[test]
fn settle_is_idempotent_across_restart() {
    let tmp = TmpDb::new("idem");
    let declaration = cfg(8.0, 2.0);
    let (mut gate, _) = DurableGate::open(tmp.path(), &declaration, None).unwrap();
    for i in 1..=4u64 {
        gate.try_admit(&order_item(i, &format!("A{i:03}"), 2.0), i as u32)
            .unwrap();
    }
    gate.complete(2, 30, 1).unwrap();
    let after_first = gate.conservation();

    // 同一 id 重复 settle：UnknownItem（已出队），账面零变化 → 不重复释容。
    let repeat = gate.complete(2, 31, 0).unwrap_err();
    assert!(matches!(
        repeat,
        PersistError::Gate(GateError::UnknownItem(2))
    ));
    assert_eq!(gate.conservation(), after_first);

    // 未知 id（从未准入）同样不释容。
    let unknown = gate.complete(99, 32, 0).unwrap_err();
    assert!(matches!(
        unknown,
        PersistError::Gate(GateError::UnknownItem(99))
    ));
    assert_eq!(gate.conservation(), after_first);

    // 重启后重复同一 settle：还是不释容。
    drop(gate);
    let (mut gate, outcome) = DurableGate::open(tmp.path(), &declaration, None).unwrap();
    assert_eq!(outcome, OpenOutcome::AlreadyDeclared);
    let repeat = gate.complete(2, 33, 0).unwrap_err();
    assert!(matches!(
        repeat,
        PersistError::Gate(GateError::UnknownItem(2))
    ));
    assert_eq!(gate.conservation(), after_first);
    assert_conservation(&gate, 8.0);
}

#[test]
fn cancel_releases_capacity_into_its_own_account() {
    let tmp = TmpDb::new("cancel");
    let (mut gate, _) = DurableGate::open(tmp.path(), &cfg(4.0, 1.0), None).unwrap();
    gate.try_admit(&order_item(1, "A001", 2.0), 1).unwrap();
    gate.try_admit(&order_item(2, "A002", 2.0), 2).unwrap();
    let rejected = gate.try_admit(&order_item(3, "A003", 1.0), 3).unwrap_err();
    assert!(matches!(
        rejected,
        PersistError::Admit(AdmitError::WipCap { .. })
    ));

    let cancelled = gate.cancel(1, 10).unwrap();
    assert_eq!(cancelled.ticket.order_id, "A001");
    let c = gate.conservation();
    assert_eq!(c.cancelled_count, 1);
    assert_eq!(c.cancelled_weight, 2.0);
    assert_eq!(c.completed_count, 0, "cancel is not a completion");
    assert_conservation(&gate, 4.0);

    // 释出的容量可再投放。
    gate.try_admit(&order_item(4, "A004", 2.0), 11).unwrap();
    assert_conservation(&gate, 4.0);
}

#[test]
fn restart_reads_wip_back_and_never_over_releases() {
    let tmp = TmpDb::new("restart");
    let declaration = cfg(6.0, 1.0);
    let (mut gate, _) = DurableGate::open(tmp.path(), &declaration, None).unwrap();
    gate.try_admit(&order_item(1, "A001", 2.5), 1).unwrap();
    gate.try_admit(&order_item(2, "A002", 2.5), 2).unwrap();
    let snapshot = gate.conservation();
    drop(gate);

    let (mut gate, outcome) = DurableGate::open(tmp.path(), &declaration, None).unwrap();
    assert_eq!(outcome, OpenOutcome::AlreadyDeclared);
    // 重启后 wip 与守恒计数原样读回（不重置、不超放）。
    assert_eq!(gate.conservation(), snapshot);
    assert_eq!(gate.in_process()[0].order_id, "A001");
    assert_eq!(gate.in_process()[1].order_id, "A002");
    let rejected = gate.try_admit(&order_item(3, "A003", 1.5), 10).unwrap_err();
    assert!(matches!(
        rejected,
        PersistError::Admit(AdmitError::WipCap { .. })
    ));
    // 精确等号可入：5.0+1.0=6.0 不算超。
    gate.try_admit(&order_item(3, "A003", 1.0), 11).unwrap();
    assert_conservation(&gate, 6.0);
    let rejected = gate.try_admit(&order_item(4, "A004", 0.5), 12).unwrap_err();
    assert!(matches!(
        rejected,
        PersistError::Admit(AdmitError::WipCap { .. })
    ));
    // settle 一件腾出容量后再放行，仍守上限。
    gate.complete(1, 13, 0).unwrap();
    gate.try_admit(&order_item(5, "A005", 2.0), 14).unwrap();
    assert_conservation(&gate, 6.0);
}

// ---------- 8：item_id 单调（缺陷回归） ----------

/// 旧缺陷复现（工单路径）：item_id 按"队列现存 max+1"分配——旧件出队后，
/// 同一 id 会被发给新单；迟到的旧 complete 会把新单误释。
/// 现在：持久化单调 next_item_id，旧 id 永不复用。
#[test]
fn item_ids_never_reuse_after_completion() {
    let tmp = TmpDb::new("monotonic");
    let declaration = cfg(8.0, 2.0);
    let (mut gate, _) = DurableGate::open(tmp.path(), &declaration, None).unwrap();

    let t1 = gate.try_admit(&order_item(1, "A001", 1.0), 1).unwrap();
    assert_eq!(t1.item_id, 1);
    gate.complete(1, 5, 0).unwrap(); // 旧件出队，队列 max 归零

    let t2 = gate.try_admit(&order_item(2, "A002", 1.0), 6).unwrap();
    assert_ne!(
        t2.item_id, t1.item_id,
        "item_id must never be reused (queue was drained)"
    );
    // 显式 id 也把单调游标推过去。
    let t3 = gate.try_admit(&order_item(50, "A003", 1.0), 7).unwrap();
    let t4 = gate
        .try_admit(&order_item(gate.next_item_id(), "A004", 1.0), 8)
        .unwrap();
    assert!(t4.item_id > t3.item_id, "cursor must be past explicit ids");
    assert_conservation(&gate, 8.0);

    // 迟到的旧 complete 打不到新单：按旧缺陷它会释掉 t2。
    let stale = gate.complete(t1.item_id, 9, 0).unwrap_err();
    assert!(matches!(
        stale,
        PersistError::Gate(GateError::UnknownItem(1))
    ));
    assert_conservation(&gate, 8.0);
    let c = gate.conservation();
    assert_eq!(c.wip_count, 3, "stale settle must not release a live item");
}

/// 游标跨重启持久：重启后发的 id 严格大于重启前发过的所有 id。
#[test]
fn item_id_cursor_survives_restart() {
    let tmp = TmpDb::new("cursor-restart");
    let declaration = cfg(8.0, 2.0);
    let (mut gate, _) = DurableGate::open(tmp.path(), &declaration, None).unwrap();
    let t1 = gate.try_admit(&order_item(1, "A001", 1.0), 1).unwrap();
    let t2 = gate.try_admit(&order_item(2, "A002", 1.0), 2).unwrap();
    gate.complete(t1.item_id, 3, 0).unwrap();
    drop(gate);

    let (mut gate, _) = DurableGate::open(tmp.path(), &declaration, None).unwrap();
    let t3 = gate
        .try_admit(&order_item(gate.next_item_id(), "A003", 1.0), 4)
        .unwrap();
    assert!(
        t3.item_id > t2.item_id,
        "post-restart id must exceed pre-restart ids"
    );
    assert_conservation(&gate, 8.0);
}

// ---------- 1/2/3：性质化随机操作序列 ----------

#[derive(Debug, Clone)]
enum Op {
    Admit { order: u16, weight: u16 },
    Complete { slot: u8 },
    Cancel { slot: u8 },
    Reopen,
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (any::<u16>(), 1u16..40u16).prop_map(|(order, weight)| Op::Admit { order, weight }),
        any::<u8>().prop_map(|slot| Op::Complete { slot }),
        any::<u8>().prop_map(|slot| Op::Cancel { slot }),
        Just(Op::Reopen),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// 随机操作序列（准入/完成/取消/重启）下，六条硬不变式每步都成立。
    /// settle 目标从"当前在制"里按 slot 挑——重复 settle 自然出现（slot 重复），
    /// 幂等由断言覆盖（账面只动第一次）。
    #[test]
    fn conservation_holds_over_random_operation_sequences(ops in prop::collection::vec(op_strategy(), 1..120)) {
        let tmp = TmpDb::new("prop");
        let declaration = cfg(20.0, 4.0);
        let (mut gate, _) = DurableGate::open(tmp.path(), &declaration, None).unwrap();
        // 在制单索引（按准入顺序），settle 按 slot 取模挑一个。
        let mut live: Vec<(u64, String)> = Vec::new();
        let mut id = 1u64;

        for (step, op) in ops.iter().enumerate() {
            match op {
                Op::Admit { order, weight } => {
                    let oid = format!("A{order:05}");
                    let item = order_item(id, &oid, f64::from(*weight));
                    let result = gate.try_admit(&item, (step % 1440) as u32);
                    match result {
                        Ok(t) => {
                            assert_eq!(t.order_id, oid);
                            live.push((t.item_id, oid));
                            id += 1;
                        }
                        Err(PersistError::Admit(AdmitError::WipCap { .. })) => {}
                        Err(PersistError::Admit(AdmitError::DuplicateOrder { order_id, .. })) => {
                            // 只可能是已 settles 的同号单还在制——查在制名单核实。
                            assert!(
                                live.iter().any(|(_, o)| *o == order_id),
                                "duplicate reject for an order not in process"
                            );
                        }
                        Err(other) => panic!("step {step}: unexpected admit error {other:?}"),
                    }
                }
                Op::Complete { slot } | Op::Cancel { slot } => {
                    if live.is_empty() {
                        continue;
                    }
                    let pick = (*slot as usize) % live.len();
                    let (item_id, _) = live[pick];
                    let before = gate.conservation();
                    let result = if matches!(op, Op::Complete { .. }) {
                        gate.complete(item_id, (step % 1440) as u32, u64::from(slot % 3))
                    } else {
                        gate.cancel(item_id, (step % 1440) as u32)
                    };
                    match result {
                        Ok(done) => {
                            live.remove(pick);
                            // 释容恰好这一件的权重，方向对账。
                            let after = gate.conservation();
                            if matches!(op, Op::Complete { .. }) {
                                assert_eq!(after.completed_weight - before.completed_weight, done.ticket.weight, "step {step}");
                            } else {
                                assert_eq!(after.cancelled_weight - before.cancelled_weight, done.ticket.weight, "step {step}");
                            }
                        }
                        // 同 slot 重复 settle（live 未及时清理的场景不会出现——
                        // 这里 live 与账本同步；仍防御性地确认零释容）。
                        Err(PersistError::Gate(GateError::UnknownItem(_))) => {
                            assert_eq!(gate.conservation(), before, "step {step}: unknown settle moved books");
                        }
                        Err(other) => panic!("step {step}: unexpected settle error {other:?}"),
                    }
                }
                Op::Reopen => {
                    drop(gate);
                    let (reopened, outcome) = DurableGate::open(tmp.path(), &declaration, None).unwrap();
                    assert_eq!(outcome, OpenOutcome::AlreadyDeclared);
                    gate = reopened;
                }
            }
            assert_conservation(&gate, 20.0);
        }
    }
}

// ---------- v1→v2 迁移 ----------

/// 手工造一个 v1 形状的账本（无 order_id/游标/守恒列），开库应原地迁移：
/// 在制读回、游标= max(item_id)+1、守恒按迁移时刻重定基后成立。
#[test]
fn v1_ledger_migrates_in_place_and_keeps_wip() {
    let tmp = TmpDb::new("mig");
    {
        let conn = rusqlite::Connection::open(tmp.path()).unwrap();
        let upper_bits = (8.0f64.to_bits()) as i64;
        let lower_bits = (2.0f64.to_bits()) as i64;
        let wip_bits = (3.0f64.to_bits()) as i64;
        let credit_bits = (1.0f64.to_bits()) as i64;
        let w1 = (1.0f64.to_bits()) as i64;
        let w2 = (2.0f64.to_bits()) as i64;
        conn.execute_batch(
            format!(
                "CREATE TABLE gate_meta (
                    id INTEGER PRIMARY KEY CHECK (id = 1), schema_version INTEGER NOT NULL,
                    wip_upper_bits INTEGER NOT NULL, wip_lower_bits INTEGER NOT NULL,
                    night_start INTEGER, night_end INTEGER, night_ratio_bits INTEGER,
                    wip_bits INTEGER NOT NULL, in_process_count INTEGER NOT NULL,
                    refill_active INTEGER NOT NULL CHECK (refill_active IN (0,1)),
                    night_credit_bits INTEGER NOT NULL, night_requests INTEGER NOT NULL,
                    night_admitted INTEGER NOT NULL,
                    prev_in_window INTEGER CHECK (prev_in_window IS NULL OR prev_in_window IN (0,1)),
                    updated_wall_ms INTEGER NOT NULL
                 );
                 CREATE TABLE wip_item (
                    seq INTEGER PRIMARY KEY, gate_id INTEGER NOT NULL REFERENCES gate_meta(id),
                    item_id INTEGER NOT NULL, config TEXT NOT NULL,
                    weight_bits INTEGER NOT NULL, admitted_at INTEGER NOT NULL
                 );
                 CREATE TABLE canary (
                    id INTEGER PRIMARY KEY CHECK (id = 1), gate_id INTEGER NOT NULL REFERENCES gate_meta(id),
                    config TEXT NOT NULL, max_probes INTEGER NOT NULL, released INTEGER NOT NULL,
                    completed INTEGER NOT NULL, defects INTEGER NOT NULL,
                    open INTEGER NOT NULL CHECK (open IN (0,1)), updated_wall_ms INTEGER NOT NULL
                 );
                 PRAGMA user_version = 1;
                 INSERT INTO gate_meta VALUES (
                    1, 1, {upper_bits}, {lower_bits},
                    NULL, NULL, NULL,
                    {wip_bits}, 2, 0,
                    {credit_bits}, 0, 0, NULL, 111);
                 INSERT INTO wip_item (seq, gate_id, item_id, config, weight_bits, admitted_at)
                    VALUES (0, 1, 4, 'v1', {w1}, 10),
                           (1, 1, 7, 'v1', {w2}, 20);"
            )
            .as_str(),
        )
        .unwrap();
    }

    let (mut gate, outcome) = DurableGate::open(tmp.path(), &cfg(8.0, 2.0), None).unwrap();
    assert_eq!(outcome, OpenOutcome::AlreadyDeclared);
    // 迁移后读回：在制原样（order_id 空串）、游标越过 max id、守恒重定基成立。
    assert_eq!(gate.in_process_count(), 2);
    assert_eq!(gate.wip_minutes(), 3.0);
    assert_eq!(gate.next_item_id(), 8, "cursor = max(item_id)+1");
    assert!(gate.in_process().iter().all(|i| i.order_id.is_empty()));
    assert_conservation(&gate, 8.0);
    // 迁移不超放：腾一件后仍守上限。
    gate.complete(7, 30, 0).unwrap(); // wip 3.0 → 1.0
    let t = gate.try_admit(&order_item(8, "A001", 7.0), 31).unwrap(); // 1.0+7.0=8.0 到顶
    assert_eq!(t.item_id, 8);
    assert!(matches!(
        gate.try_admit(&order_item(9, "A002", 0.5), 32),
        Err(PersistError::Admit(AdmitError::WipCap { .. }))
    ));
    assert_conservation(&gate, 8.0);

    // user_version 已钉到 2。
    let conn = rusqlite::Connection::open(tmp.path()).unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
}

/// 影子记账路径：豁免 WIP 上限与渐放封顶，重复单照常拒绝；守恒式仍然成立。
#[test]
fn uncapped_shadow_bookkeeping_keeps_conservation() {
    let tmp = TmpDb::new("shadow");
    let (mut gate, _) = DurableGate::open(tmp.path(), &cfg(2.0, 1.0), None).unwrap();
    gate.try_admit(&order_item(1, "A001", 1.0), 1).unwrap();
    gate.try_admit(&order_item(2, "A002", 1.0), 2).unwrap();
    // 正式路径已满。
    assert!(matches!(
        gate.try_admit(&order_item(3, "A003", 1.0), 3),
        Err(PersistError::Admit(AdmitError::WipCap { .. }))
    ));
    // 影子记账：照实入账（wip 可超上限，由调用方标注差异），守恒式仍成立。
    let t = gate
        .try_admit_shadow_record(&order_item(3, "A003", 1.0), 4)
        .unwrap();
    assert_eq!(t.item_id, 3);
    let c = gate.conservation();
    assert!(c.count_balanced() && c.weight_balanced(), "{c:?}");
    // 重复单在影子路径同样只有一张 Permit。
    assert!(matches!(
        gate.try_admit_shadow_record(&order_item(4, "A003", 1.0), 5),
        Err(PersistError::Admit(AdmitError::DuplicateOrder { .. }))
    ));
    // settle 后回到上限内。
    gate.complete(3, 6, 0).unwrap();
    assert_conservation(&gate, 2.0);
}
