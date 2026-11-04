//! 风险#5 故障注入验收：steward 闭环跨库（裁决文书/lease 库/受理账/WIP 面）
//! prepared/committed 补偿——三个崩溃窗 × 「检出 / 收敛 / 重放幂等」。
//!
//! 注入纪律：**零生产注入钩子**——崩溃全部由测试在真实 pub 调用之间停机
//! 编排（镜像 run_steward_loop 首走步骤），时钟用 af-station ManualClock
//! 推进租约到期（不睡眠）。引擎走真子进程，队列走真 af-station。
//!
//! 崩溃窗对照（意图账相位 → 后果 → 验收断言）：
//! - W1 admit 落账 ↔ committed 线：receipt 在账、分支/complete 未落
//!   → reconcile 检出+分支幂等结算；重放臂收敛且不二次受理（T2）；
//! - W2 committed 线 ↔ complete：receipt+分支已落、任务未 done
//!   → 重放幂等臂收敛，admissions 恒 1 笔（T1，改前此窗双写，见 poc 改前读数）；
//! - W3 complete ↔ completed 线：任务已 done、收口线未落
//!   → reconcile 补线收口（T4）；
//! - W0 prepared ↔ admit：意图在账、提交点未到
//!   → reconcile 报 PreparedResidue 留检；后续认领按首走重试，仍 1 笔（T3）；
//! - Reject/abort 臂：已 Cancelled 源单的重复 abort 幂等收敛（T5）。

use std::fs;
use std::path::{Path, PathBuf};

use af_execline::EngineClient;
use af_order::adjudicate::{
    generate_adjudication_doc, loop_gate_list, loop_resource_profile, reconcile_adjudications,
    run_steward_loop, template_body_sha256, AdjudicationJournal, AdjudicationSeed,
    CommittedFact, LoopFacilities, PreparedIntent, ReconcileStatus, JOURNAL_SUFFIX, STEWARD_ROLE,
};
use af_order::admit::{
    admit, decision_digest, idempotency_key, load_receipts, AdmissionRequest, AdjudicationTemplate,
};
use af_order::{cli, StateDir};
use af_station::{State as TaskState, Station};
use af_workorder::{
    Decision, DecisionArtifact, DecisionAttributes, Ed25519Keypair, Role, TrustedActor,
    TrustedActorRegistry, WorkOrder,
};

const NOW: &str = "2026-11-03T00:00:00Z";
const KEY_EXPIRY: &str = "2027-01-01T00:00:00Z";
const DECISION_EXPIRY: &str = "2026-12-01T00:00:00Z";
const STEWARD_SEED: [u8; 32] = [0x61; 32];
/// 崩溃窗注入用租约（ManualClock 推进 2s 即过期）。
const CRASH_LEASE_MS: u64 = 1_000;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace 根可解析")
}

fn engine_client() -> EngineClient {
    let workspace = workspace_root();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target"));
    EngineClient::locate(&workspace, &target).expect("定位/构建真 af-engine")
}

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时钟")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "af-order-poc-after-{}-{tag}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("创建测试 scratch 目录");
    dir
}

fn time(text: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&chrono::Utc)
}

fn submit_stub_order(root: &Path, order_id: &str) {
    let spec_path = root.join(format!("{order_id}-spec.json"));
    fs::write(
        &spec_path,
        format!(
            r#"{{"order_id":"{order_id}","line":{{"line_id":"line-poc","line_version":"lv-1"}},"budget":100,"note":"风险#5 故障注入单"}}"#
        ),
    )
    .unwrap();
    let workspace = workspace_root();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target"));
    cli::run(&[
        "submit".into(),
        "--spec".into(),
        spec_path.display().to_string(),
        "--state".into(),
        root.display().to_string(),
        "--unit".into(),
        "stub".into(),
        "--workspace".into(),
        workspace.display().to_string(),
        "--target-dir".into(),
        target.display().to_string(),
    ])
    .expect("submit 全生命周期应走通");
}

fn registry(role: Role) -> TrustedActorRegistry {
    TrustedActorRegistry::new(vec![TrustedActor::new(
        "steward-a",
        role,
        &Ed25519Keypair::from_seed(&STEWARD_SEED).public_key(),
        time(KEY_EXPIRY),
    )])
    .unwrap()
}

/// 手工镜像 run_steward_loop 首走到指定停机点（故障注入面）。
/// 停机点（按相位序）：`PreparedWritten`（W0 窗）/ `Admitted`（W1 窗，含
/// prepared+admit）/ `CommittedWritten`（W2 窗）/ `TaskCompleted`（W3 窗）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CrashAt {
    PreparedWritten,
    Admitted,
    CommittedWritten,
    TaskCompleted,
}

/// 首走镜像产物（供断言与重试）。
struct WalkedFirstLeg {
    adjudication_id: String,
    task_id: String,
    receipt_seq: u64,
}

/// 镜像首走：生成 → 落盘 → 入队 → 认领 → 复算 → 真签 → prepared → admit
/// →（视停机点）分支结算 + committed → complete。返回停机时已落的事实。
/// decision/next_orders/role/签钥/worker 全部取自 seed（与 run_steward_loop
/// 同源）。
fn walk_first_leg_to_crash(
    root: &Path,
    seed: &AdjudicationSeed<'_>,
    station: &Station,
    client: &EngineClient,
    crash_at: CrashAt,
) -> WalkedFirstLeg {
    let order_id = seed.order_id;
    let decision = seed.decision;
    let role = seed.actor_role;
    let state = StateDir::new(root);
    let keypair = Ed25519Keypair::from_seed(&STEWARD_SEED);
    let (doc, source) = generate_adjudication_doc(
        &state,
        order_id,
        seed.next_orders.clone(),
        time(DECISION_EXPIRY),
    )
    .unwrap();
    let doc_path = doc.persist(&state).unwrap();
    let station_key = format!("{}/{}", doc.source_stable_id, doc.adjudication_id);
    station
        .enqueue(af_station::NewTask {
            id: &doc.adjudication_id,
            payload_ref: doc_path.to_string_lossy().as_ref(),
            required_role: STEWARD_ROLE,
            idempotency_key: Some(&station_key),
            deadline: None,
        })
        .unwrap();
    let task = station
        .claim(seed.worker_id, STEWARD_ROLE, CRASH_LEASE_MS)
        .unwrap()
        .unwrap();
    assert_eq!(task.id, doc.adjudication_id);
    let seq = load_receipts(&state, order_id)
        .unwrap()
        .iter()
        .map(|r| r.decision_seq)
        .max()
        .unwrap_or(0)
        + 1;
    let payload = source.admission_decision_payload();
    let attributes = DecisionAttributes::new(
        seq,
        doc.basis.clone(),
        doc.next_orders_digest.clone(),
        time(DECISION_EXPIRY),
    );
    let signed = DecisionArtifact::seal(
        "steward-a",
        role,
        decision,
        &payload,
        &keypair,
        attributes,
    );
    let digest = decision_digest(&signed);
    let stable_id = source.stable_id().as_str().to_string();
    let receipt_key = idempotency_key(&stable_id, seq, &digest);
    let mut journal =
        AdjudicationJournal::open(&state, order_id, &doc.adjudication_id).unwrap();
    journal
        .append_prepared(PreparedIntent {
            adjudication_id: doc.adjudication_id.clone(),
            task_id: task.id.clone(),
            source_order_id: order_id.to_string(),
            source_stable_id: stable_id,
            decision,
            decision_seq: seq,
            decision_digest: digest,
            receipt_idempotency_key: receipt_key,
            next_orders: doc.next_orders.clone(),
            ts_ms: chrono::Utc::now().timestamp_millis(),
        })
        .unwrap();
    if crash_at == CrashAt::PreparedWritten {
        return WalkedFirstLeg {
            adjudication_id: doc.adjudication_id,
            task_id: task.id,
            receipt_seq: seq,
        };
    }
    let held_voucher = source.budget_voucher().unwrap().voucher_id.clone();
    let receipt = admit(
        &AdmissionRequest {
            order_id: order_id.to_string(),
            decision: signed,
            actor_registry: registry(role),
            template: AdjudicationTemplate {
                template_id: doc.template_id.clone(),
                template_version: doc.template_version.clone(),
                doc_kind: doc.doc_kind.clone(),
                body_sha256: template_body_sha256(),
            },
            resource_profile: loop_resource_profile(),
            gate_list: loop_gate_list(),
            next_orders: doc.next_orders.clone(),
            required_credentials: vec![held_voucher],
            validated_at: Some(time(NOW)),
        },
        &state,
        client,
        time(NOW),
    )
    .expect("崩溃前 admit 应已提交");
    assert_eq!(receipt.decision_seq, seq);
    if crash_at == CrashAt::Admitted {
        return WalkedFirstLeg {
            adjudication_id: doc.adjudication_id,
            task_id: task.id,
            receipt_seq: seq,
        };
    }
    // 分支结算 + committed 线（与 run_steward_loop 首走同序）。
    let mut source = source;
    let (rework_order_ids, aborted) = match decision {
        Decision::Revise => {
            let mut ids = Vec::new();
            for rework_id in &doc.next_orders {
                materialize_rework(&state, &source, rework_id);
                ids.push(rework_id.clone());
            }
            (ids, false)
        }
        Decision::Reject => {
            cancel_source(&state, &mut source);
            (Vec::new(), true)
        }
        _ => (Vec::new(), false),
    };
    journal
        .append_committed(CommittedFact {
            receipt_idempotency_key: receipt.idempotency_key.clone(),
            rework_order_ids,
            aborted,
            ts_ms: chrono::Utc::now().timestamp_millis(),
        })
        .unwrap();
    if crash_at == CrashAt::CommittedWritten {
        return WalkedFirstLeg {
            adjudication_id: doc.adjudication_id,
            task_id: task.id,
            receipt_seq: seq,
        };
    }
    assert!(station.complete(&task.id, "steward-1").unwrap());
    WalkedFirstLeg {
        adjudication_id: doc.adjudication_id,
        task_id: task.id,
        receipt_seq: seq,
    }
}

/// rework 物化的测试侧镜像（与 spawn_rework_order 同款落账形状；生产逻辑
/// 的幂等臂在模块内单测覆盖，这里只造崩溃前的已结算面）。
fn materialize_rework(state: &StateDir, source: &WorkOrder, rework_id: &str) {
    use af_workorder::{BudgetVoucher, Priority, Sla};
    let mut rework = WorkOrder::successor(
        rework_id,
        source.graph_version().clone(),
        Sla::new(None, Priority::Normal),
        source.id(),
    );
    rework
        .attach_budget_voucher(BudgetVoucher {
            voucher_id: format!("bv-{rework_id}"),
            amount: source.budget_voucher().map(|v| v.amount).unwrap_or(0),
            signed: false,
        })
        .unwrap();
    let dir = state.order_dir(rework_id);
    fs::create_dir_all(&dir).unwrap();
    let mut ledger =
        af_execline::OrderLedger::open(&dir.join("ledger.jsonl")).unwrap();
    ledger
        .append(af_execline::LedgerRecord {
            seq: 0,
            ts_ms: 1_767_225_600_000,
            order_id: rework_id.to_string(),
            stable_id: rework.stable_id().as_str().to_string(),
            op: "create_order".into(),
            idem_key: format!("{}/create_order", rework.stable_id()),
            ok: true,
            state_after: Some("Created".into()),
            error_kind: None,
            error_detail: None,
            order_after: Some(rework),
        })
        .unwrap();
}

/// cancel 源单的测试侧镜像（与 abort_source 同款落账形状）。
fn cancel_source(state: &StateDir, source: &mut WorkOrder) {
    source.cancel().unwrap();
    let order_id = source.id().to_string();
    let mut ledger = af_execline::OrderLedger::open(
        &state.order_dir(&order_id).join("ledger.jsonl"),
    )
    .unwrap();
    ledger
        .append(af_execline::LedgerRecord {
            seq: 0,
            ts_ms: 1_767_225_600_000,
            order_id,
            stable_id: source.stable_id().as_str().to_string(),
            op: "cancel".into(),
            idem_key: format!("{}/cancel", source.stable_id()),
            ok: true,
            state_after: Some("Cancelled".into()),
            error_kind: None,
            error_detail: None,
            order_after: Some(source.clone()),
        })
        .unwrap();
}

fn seed<'a>(
    order_id: &'a str,
    decision: Decision,
    next_orders: Vec<String>,
    role: Role,
) -> AdjudicationSeed<'a> {
    use std::sync::OnceLock;
    static KEYPAIR: OnceLock<Ed25519Keypair> = OnceLock::new();
    AdjudicationSeed {
        order_id,
        decision,
        next_orders,
        expires_at: time(DECISION_EXPIRY),
        actor_id: "steward-a",
        actor_role: role,
        actor_keypair: KEYPAIR.get_or_init(|| Ed25519Keypair::from_seed(&STEWARD_SEED)),
        worker_id: "steward-1",
        lease_ms: 60_000,
    }
}

fn retry_loop(
    root: &Path,
    station: &Station,
    client: &EngineClient,
    order_id: &str,
    decision: Decision,
    role: Role,
) -> af_order::adjudicate::StewardOutcome {
    let state = StateDir::new(root);
    run_steward_loop(
        &seed(order_id, decision, vec![], role),
        &LoopFacilities {
            state: &state,
            station,
            client,
            registry: &registry(role),
            now: time(NOW),
        },
    )
    .expect("重试闭环应走通")
}

/// W2：committed 线已落、complete 未落（改前的双写窗）。租约回炉后重试
/// 走重放幂等臂——不重签不重受理，admissions 恒 1 笔，任务收敛 done。
#[test]
fn fault_w2_crash_before_complete_replays_without_double_admission() {
    let root = scratch("w2");
    let order_id = "wo-fi-w2";
    submit_stub_order(&root, order_id);
    let clock = af_station::ManualClock::new(1_767_225_600_000);
    let station = Station::open_in_memory_with_clock(clock.clone())
        .unwrap()
        .into_station();
    let client = engine_client();
    let walked =
        walk_first_leg_to_crash(
            &root,
            &seed(order_id, Decision::Approve, vec![], Role::Approver),
            &station,
            &client,
            CrashAt::CommittedWritten,
        );
    assert_eq!(
        load_receipts(&StateDir::new(&root), order_id).unwrap().len(),
        1,
        "崩溃时受理账恰 1 笔"
    );
    assert_eq!(
        station.get(&walked.task_id).unwrap().unwrap().state,
        TaskState::Claimed,
        "崩溃后任务停在 claimed（lease 库未收敛）"
    );

    // 注入租约到期（不睡眠）→ 运维重试 = run_steward_loop（会生成新裁决单
    // adj-2 入队，但认领按 rowid 先拿到回炉的旧任务 adj-1）。
    clock.advance(CRASH_LEASE_MS + 1_000);
    let outcome = retry_loop(&root, &station, &client, order_id, Decision::Approve, Role::Approver);
    assert_eq!(outcome.task_id, walked.adjudication_id, "重试认领到旧任务");
    assert!(
        outcome.receipt.replayed,
        "重放臂返回的应是受理账原回执（replayed=true）"
    );
    assert_eq!(outcome.receipt.decision_seq, walked.receipt_seq);
    let receipts = load_receipts(&StateDir::new(&root), order_id).unwrap();
    println!("[after/W2] 重试后受理账行数 = {}", receipts.len());
    for r in &receipts {
        println!(
            "[after/W2]   receipt: seq={} decision={} replayed={}",
            r.decision_seq, r.decision, r.replayed
        );
    }
    assert_eq!(receipts.len(), 1, "同一裁决单不得二次受理（改前此处为 2）");
    assert_eq!(
        station.get(&walked.task_id).unwrap().unwrap().state,
        TaskState::Done,
        "重放臂完成任务收敛"
    );
    let journal = AdjudicationJournal::open(
        &StateDir::new(&root),
        order_id,
        &walked.adjudication_id,
    )
    .unwrap();
    assert!(journal.is_completed(), "意图账应收口 completed");
    let _ = fs::remove_dir_all(&root);
}

/// W1：admit 已落账、committed 线/分支/complete 全未落。reconcile 检出并
/// 幂等结算分支（rework 恰 1 张、不双物化）；租约到期后重试收敛、账仍 1 笔。
#[test]
fn fault_w1_crash_after_admit_reconcile_settles_and_replay_converges() {
    let root = scratch("w1");
    let order_id = "wo-fi-w1";
    submit_stub_order(&root, order_id);
    let clock = af_station::ManualClock::new(1_767_225_600_000);
    let station = Station::open_in_memory_with_clock(clock.clone())
        .unwrap()
        .into_station();
    let client = engine_client();
    let rework_id = format!("{order_id}-rw");
    let walked = walk_first_leg_to_crash(
        &root,
        &seed(order_id, Decision::Revise, vec![rework_id.clone()], Role::Reviser),
        &station,
        &client,
        CrashAt::Admitted,
    );
    let state = StateDir::new(&root);
    assert!(
        !state.order_dir(&rework_id).join("ledger.jsonl").exists(),
        "崩溃时 rework 尚未物化"
    );

    // reconcile（崩溃后无人值守扫描）：receipt 在账 → 检出 + 分支幂等结算
    // + committed 线补缺；任务 claimed 未到期 → LeaseHeld 登记。
    let reports = reconcile_adjudications(&state, &station).unwrap();
    println!("[after/W1] reconcile 报告 = {reports:?}");
    assert_eq!(reports.len(), 1, "恰一份意图账在飞");
    assert_eq!(reports[0].adjudication_id, walked.adjudication_id);
    assert!(matches!(reports[0].status, ReconcileStatus::LeaseHeld { .. }));
    assert!(
        state.order_dir(&rework_id).join("ledger.jsonl").exists(),
        "reconcile 应幂等结算 Revise 分支（物化 rework）"
    );
    let journal =
        AdjudicationJournal::open(&state, order_id, &walked.adjudication_id).unwrap();
    assert!(journal.latest_committed().is_some(), "committed 线应补缺");
    assert_eq!(
        journal.latest_committed().unwrap().rework_order_ids,
        vec![rework_id.clone()]
    );

    // 租约到期 → 重试：重放臂按 committed 线复用事实收敛（不再物化第二张）。
    clock.advance(CRASH_LEASE_MS + 1_000);
    let outcome = retry_loop(&root, &station, &client, order_id, Decision::Revise, Role::Reviser);
    assert_eq!(outcome.rework_order_ids, vec![rework_id.clone()]);
    assert_eq!(
        load_receipts(&state, order_id).unwrap().len(),
        1,
        "重放不得产生第二笔受理"
    );
    let rework_records = af_execline::OrderLedger::load(
        &state.order_dir(&rework_id).join("ledger.jsonl"),
    )
    .unwrap();
    assert_eq!(
        rework_records.iter().filter(|r| r.op == "create_order").count(),
        1,
        "rework 不得双物化"
    );
    assert_eq!(
        station.get(&walked.task_id).unwrap().unwrap().state,
        TaskState::Done
    );
    let _ = fs::remove_dir_all(&root);
}

/// W0：prepared 意图落账、admit 未到（提交点前崩溃）。reconcile 检出
/// PreparedResidue（无回执不可能双写，留检）；后续认领按首走重试成功。
#[test]
fn fault_w0_crash_before_admit_residue_detected_then_first_walk_succeeds() {
    let root = scratch("w0");
    let order_id = "wo-fi-w0";
    submit_stub_order(&root, order_id);
    let clock = af_station::ManualClock::new(1_767_225_600_000);
    let station = Station::open_in_memory_with_clock(clock.clone())
        .unwrap()
        .into_station();
    let client = engine_client();
    let walked = walk_first_leg_to_crash(
        &root,
        &seed(order_id, Decision::Approve, vec![], Role::Approver),
        &station,
        &client,
        CrashAt::PreparedWritten,
    );
    let state = StateDir::new(&root);
    assert!(
        !af_order::admit::admissions_path(&state, order_id).exists(),
        "提交点前崩溃：受理账零写入"
    );

    let reports = reconcile_adjudications(&state, &station).unwrap();
    println!("[after/W0] reconcile 报告 = {reports:?}");
    assert!(matches!(reports[0].status, ReconcileStatus::PreparedResidue { .. }));

    // 租约到期 → 重试按首走处理（无 committed 证据）：真签真受理，1 笔收敛。
    clock.advance(CRASH_LEASE_MS + 1_000);
    let outcome = retry_loop(&root, &station, &client, order_id, Decision::Approve, Role::Approver);
    assert_eq!(outcome.task_id, walked.adjudication_id);
    assert!(!outcome.receipt.replayed, "首走真受理，非重放");
    assert_eq!(load_receipts(&state, order_id).unwrap().len(), 1);
    assert_eq!(
        station.get(&walked.task_id).unwrap().unwrap().state,
        TaskState::Done
    );
    let _ = fs::remove_dir_all(&root);
}

/// W3：complete 已落、completed 收口线未落。reconcile 补线收口
/// （ClosedByReconcile），重复扫描幂等（第二次 = Converged，零新写入）。
#[test]
fn fault_w3_crash_after_complete_reconcile_closes_journal_idempotently() {
    let root = scratch("w3");
    let order_id = "wo-fi-w3";
    submit_stub_order(&root, order_id);
    let clock = af_station::ManualClock::new(1_767_225_600_000);
    let station = Station::open_in_memory_with_clock(clock)
        .unwrap()
        .into_station();
    let client = engine_client();
    let walked = walk_first_leg_to_crash(
        &root,
        &seed(order_id, Decision::Approve, vec![], Role::Approver),
        &station,
        &client,
        CrashAt::TaskCompleted,
    );
    let state = StateDir::new(&root);
    let journal =
        AdjudicationJournal::open(&state, order_id, &walked.adjudication_id).unwrap();
    assert!(!journal.is_completed(), "崩溃时收口线未落");

    let reports = reconcile_adjudications(&state, &station).unwrap();
    println!("[after/W3] reconcile 报告 = {reports:?}");
    assert!(matches!(reports[0].status, ReconcileStatus::ClosedByReconcile));
    let journal =
        AdjudicationJournal::open(&state, order_id, &walked.adjudication_id).unwrap();
    assert!(journal.is_completed(), "reconcile 应补收口线");

    // 幂等：重复扫描对已收敛账是 no-op。
    let again = reconcile_adjudications(&state, &station).unwrap();
    assert!(matches!(again[0].status, ReconcileStatus::Converged));
    let _ = fs::remove_dir_all(&root);
}

/// Reject/abort 臂的幂等收敛：abort 已生效（源单 Cancelled）后崩溃在
/// W1 窗，reconcile 重结算 abort 分支不得报错也不得重复落 cancel 行；
/// 重试重放臂收敛任务。
#[test]
fn fault_reject_abort_is_idempotent_across_crash_window() {
    let root = scratch("reject");
    let order_id = "wo-fi-reject";
    // 落一张已 release（非终态，有 abort 边）的源单。
    {
        use af_workorder::{BudgetVoucher, GraphVersion, LineBinding, Priority, Sla};
        let mut source = WorkOrder::new(
            order_id,
            GraphVersion::new("af-order/poc-1"),
            Sla::new(None, Priority::Normal),
        );
        source
            .bind_line(LineBinding {
                line_id: "line-poc".into(),
                line_version: "lv-1".into(),
            })
            .unwrap();
        source
            .attach_budget_voucher(BudgetVoucher {
                voucher_id: format!("bv-{order_id}"),
                amount: 100,
                signed: true,
            })
            .unwrap();
        source.release().unwrap();
        let dir = StateDir::new(&root).order_dir(order_id);
        fs::create_dir_all(&dir).unwrap();
        let mut ledger =
            af_execline::OrderLedger::open(&dir.join("ledger.jsonl")).unwrap();
        ledger
            .append(af_execline::LedgerRecord {
                seq: 0,
                ts_ms: 1_767_225_600_000,
                order_id: order_id.to_string(),
                stable_id: source.stable_id().as_str().to_string(),
                op: "release".into(),
                idem_key: format!("{}/release", source.stable_id()),
                ok: true,
                state_after: Some("Released".into()),
                error_kind: None,
                error_detail: None,
                order_after: Some(source),
            })
            .unwrap();
    }
    let clock = af_station::ManualClock::new(1_767_225_600_000);
    let station = Station::open_in_memory_with_clock(clock.clone())
        .unwrap()
        .into_station();
    let client = engine_client();
    // 首走停机在 CommittedWritten：abort 已生效（源单 Cancelled、cancel 行
    // 已落账）、complete 未落——验证 reconcile 与重放臂都不重复 abort。
    let walked = walk_first_leg_to_crash(
        &root,
        &seed(order_id, Decision::Reject, vec![], Role::Approver),
        &station,
        &client,
        CrashAt::CommittedWritten,
    );
    let state = StateDir::new(&root);
    let records = af_execline::OrderLedger::load(
        &state.order_dir(order_id).join("ledger.jsonl"),
    )
    .unwrap();
    assert_eq!(
        records.iter().filter(|r| r.op == "cancel").count(),
        1,
        "崩溃前 abort 已落一笔 cancel"
    );
    assert_eq!(
        af_order::state::current_state(&records).as_deref(),
        Some("Cancelled")
    );

    // reconcile：committed 线已在 → 只判读任务面（LeaseHeld），不动分支。
    let reports = reconcile_adjudications(&state, &station).unwrap();
    println!("[after/Reject] reconcile 报告 = {reports:?}");
    assert!(matches!(reports[0].status, ReconcileStatus::LeaseHeld { .. }));
    let records = af_execline::OrderLedger::load(
        &state.order_dir(order_id).join("ledger.jsonl"),
    )
    .unwrap();
    assert_eq!(
        records.iter().filter(|r| r.op == "cancel").count(),
        1,
        "reconcile 不得重复落 cancel 行"
    );

    // 租约到期 → 重试重放臂：源单已 Cancelled，重放不重 abort、不重受理。
    clock.advance(CRASH_LEASE_MS + 1_000);
    let outcome = retry_loop(&root, &station, &client, order_id, Decision::Reject, Role::Approver);
    assert!(outcome.aborted, "重放按 committed 线复用 abort 事实");
    assert!(outcome.receipt.replayed);
    assert_eq!(load_receipts(&state, order_id).unwrap().len(), 1);
    let records = af_execline::OrderLedger::load(
        &state.order_dir(order_id).join("ledger.jsonl"),
    )
    .unwrap();
    assert_eq!(
        records.iter().filter(|r| r.op == "cancel").count(),
        1,
        "重放不得重复 abort"
    );
    assert_eq!(
        station.get(&walked.task_id).unwrap().unwrap().state,
        TaskState::Done
    );
    let _ = fs::remove_dir_all(&root);
}

/// 意图账文件不进 next_adjudication_id 的 *.json 计数（单号不受污染）。
#[test]
fn journal_files_do_not_disturb_adjudication_numbering() {
    let root = scratch("numbering");
    let order_id = "wo-fi-numbering";
    submit_stub_order(&root, order_id);
    let state = StateDir::new(&root);
    let (doc1, _) =
        generate_adjudication_doc(&state, order_id, vec![], time(DECISION_EXPIRY)).unwrap();
    doc1.persist(&state).unwrap();
    // 人为落一份意图账（模拟在飞残留），再生成下一份裁决单。
    let mut journal =
        AdjudicationJournal::open(&state, order_id, &doc1.adjudication_id).unwrap();
    journal
        .append_prepared(PreparedIntent {
            adjudication_id: doc1.adjudication_id.clone(),
            task_id: doc1.adjudication_id.clone(),
            source_order_id: order_id.to_string(),
            source_stable_id: doc1.source_stable_id.clone(),
            decision: Decision::Approve,
            decision_seq: 1,
            decision_digest: "0".repeat(64),
            receipt_idempotency_key: format!("{}/admit/1/{}", doc1.source_stable_id, "0".repeat(64)),
            next_orders: vec![],
            ts_ms: 0,
        })
        .unwrap();
    let (doc2, _) =
        generate_adjudication_doc(&state, order_id, vec![], time(DECISION_EXPIRY)).unwrap();
    assert_eq!(doc2.adjudication_id, format!("adj-{order_id}-2"));
    // 意图账路径约定与后缀常量一致。
    assert_eq!(
        AdjudicationJournal::path(&state, order_id, &doc1.adjudication_id),
        state
            .root()
            .join("adjudications")
            .join(order_id)
            .join(format!("{}{JOURNAL_SUFFIX}", doc1.adjudication_id))
    );
    let _ = fs::remove_dir_all(&root);
}
