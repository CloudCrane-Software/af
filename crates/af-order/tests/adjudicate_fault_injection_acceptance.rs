//! 风险#5 故障注入验收（独立验收面，`test/risk5-fault-injection`）：
//! 既有 `adjudicate_fault_injection.rs` 已覆盖 W0/W1/W2/W3 四个崩溃窗的主干，
//! 本文件是**外派独立验收**——针对其未覆盖的中间态形态与补偿幂等面：
//!
//! - A1 W1.5：Revise 多 rework **部分物化**即崩（WIP 面只落一半）→ reconcile
//!   幂等补齐 + 双跑补偿零新写入；重放收敛不双物化；
//! - A2 W1.5'：Reject **cancel 行已落、committed 线未落**即崩 → reconcile
//!   幂等结算不双落 cancel；重放收敛；
//! - A3 差距实证：**Closed 源单 + Reject**——OR2102 放行 Closed，受理账已
//!   落（提交点已过）后 `abort_source` 撞 MissingEdge（Closed 无 abort 边），
//!   重放臂/reconcile 均不可收敛（悬空态：prepared 在账、回执在账、committed
//!   永缺）；租约未到期窗内重试还会**再开一轮受理再撞墙**（受理账 +1）；
//! - A4 差距实证：单份毒化意图账（A3 形态）令 `reconcile_adjudications`
//!   **整体 Err**——同批健康账（W3 形态）的收口被阻断；
//! - A5 补偿调用副作用：W2 崩溃后一次补偿收敛，但**每次 run_steward_loop
//!   都先新造一份裁决单入队**；二次补偿会认领残单走**真受理**（seq+1 第二
//!   笔）——「补偿跑两次与跑一次相同」在调用面不成立；
//! - A6 lease 库漂移：W1 崩溃后 lease 库丢任务 → reconcile 永远留检
//!   （PreparedResidue 缺位形态），重试只能开新轮次，旧意图账永不收敛；
//! - A7 受理账漂移：committed 线在、受理账被抹 → reconcile 报
//!   PreparedResidue（不 fail-loud，与重放臂不对称）；重放臂 fail-loud
//!   拒绝重放（「跨库账面漂移」），任务永久悬空；
//! - A8 注入点返回错误：越权角色（Reviser+Approve）首走 admit 被 OR2106
//!   拒 → prepared 残留（无回执不可能双写）；换正权角色重试按首走收敛，
//!   受理账恒 1 笔。
//!
//! 注入纪律（与既有套件同款）：**零生产注入钩子**——崩溃由测试在真实 pub
//! 调用之间停机编排（镜像 `run_steward_loop` 首走步骤），时钟用 af-station
//! ManualClock 推进租约到期（不睡眠）；引擎走真子进程，队列走真 af-station。
//! 断言全部从盘上/库上重载复算。A3/A4/A6/A7 是**差距登记的实证**（断言
//! 钉住当前行为，不放宽判据——收敛失败的读数如实写红）。

use std::fs;
use std::path::{Path, PathBuf};

use af_execline::EngineClient;
use af_order::adjudicate::{
    generate_adjudication_doc, loop_gate_list, loop_resource_profile, reconcile_adjudications,
    run_steward_loop, template_body_sha256, AdjudicateError, AdjudicationJournal, AdjudicationSeed,
    CommittedFact, LoopFacilities, PreparedIntent, ReconcileStatus, STEWARD_ROLE,
};
use af_order::admit::{
    admissions_path, admit, decision_digest, idempotency_key, load_receipts, AdjudicationTemplate,
    AdmissionRequest,
};
use af_order::state::LEDGER_FILE;
use af_order::StateDir;
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
/// 正常重试租约（与既有套件同值）。
const RETRY_LEASE_MS: u64 = 60_000;
const WORKER: &str = "steward-1";

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
        "af-order-acc-fi-{}-{tag}-{nanos}",
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

fn registry(role: Role) -> TrustedActorRegistry {
    TrustedActorRegistry::new(vec![TrustedActor::new(
        "steward-a",
        role,
        &Ed25519Keypair::from_seed(&STEWARD_SEED).public_key(),
        time(KEY_EXPIRY),
    )])
    .unwrap()
}

/// 落一张已 Closed 源单（合格终态工件 + 预算凭证；admit OR2102 放行 Closed）。
fn seed_closed_order(root: &Path, order_id: &str) {
    use af_workorder::{BudgetVoucher, GraphVersion, LineBinding, Priority, Sla};
    let mut order = WorkOrder::new(
        order_id,
        GraphVersion::new("af-order/poc-1"),
        Sla::new(None, Priority::Normal),
    );
    order
        .bind_line(LineBinding {
            line_id: "line-poc".into(),
            line_version: "lv-1".into(),
        })
        .unwrap();
    order
        .attach_budget_voucher(BudgetVoucher {
            voucher_id: format!("bv-{order_id}"),
            amount: 100,
            signed: true,
        })
        .unwrap();
    order.release().unwrap();
    order.start().unwrap();
    let artifact_bytes = format!("{{\"artifact\":\"{order_id}\"}}\n");
    order
        .attach_artifact(af_workorder::WorkArtifact::from_bytes(
            "fa-1",
            true,
            true,
            artifact_bytes.as_bytes(),
        ))
        .unwrap();
    order.complete().unwrap();
    order.close().unwrap();
    let dir = StateDir::new(root).order_dir(order_id);
    fs::create_dir_all(&dir).unwrap();
    af_execline::OrderLedger::open(&dir.join(LEDGER_FILE))
        .unwrap()
        .append(af_execline::LedgerRecord {
            seq: 0,
            ts_ms: 1_767_225_600_000,
            order_id: order_id.to_string(),
            stable_id: order.stable_id().as_str().to_string(),
            op: "close".into(),
            idem_key: format!("{}/close", order.stable_id()),
            ok: true,
            state_after: Some("Closed".into()),
            error_kind: None,
            error_detail: None,
            order_after: Some(order),
        })
        .unwrap();
    // 工件字节落盘（OR2109 对实际字节重算的落点）。
    let artifacts_dir = dir.join("artifacts");
    fs::create_dir_all(&artifacts_dir).unwrap();
    fs::write(artifacts_dir.join("fa-1.json"), &artifact_bytes).unwrap();
}

/// 落一张已 release（非终态，有 abort 边）源单。
fn seed_released_order(root: &Path, order_id: &str) {
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
    let dir = StateDir::new(root).order_dir(order_id);
    fs::create_dir_all(&dir).unwrap();
    af_execline::OrderLedger::open(&dir.join(LEDGER_FILE))
        .unwrap()
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

fn seed<'a>(order_id: &'a str, decision: Decision, role: Role) -> AdjudicationSeed<'a> {
    use std::sync::OnceLock;
    static KEYPAIR: OnceLock<Ed25519Keypair> = OnceLock::new();
    AdjudicationSeed {
        order_id,
        decision,
        next_orders: vec![],
        expires_at: time(DECISION_EXPIRY),
        actor_id: "steward-a",
        actor_role: role,
        actor_keypair: KEYPAIR.get_or_init(|| Ed25519Keypair::from_seed(&STEWARD_SEED)),
        worker_id: WORKER,
        lease_ms: RETRY_LEASE_MS,
    }
}

fn facilities<'a>(
    state: &'a StateDir,
    station: &'a Station,
    client: &'a EngineClient,
    registry: &'a TrustedActorRegistry,
) -> LoopFacilities<'a> {
    LoopFacilities {
        state,
        station,
        client,
        registry,
        now: time(NOW),
    }
}

/// 镜像首走停机点（按 run_steward_loop 首走相位序）。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stop {
    /// prepared 线落、admit 未到（W0）。
    PreparedWritten,
    /// admit 已落账、分支未动（W1）。
    Admitted,
    /// Revise：前 n 张 rework 已物化即停（W1.5 部分结算）。
    ReviseMaterialized(usize),
    /// Reject：cancel 行已落、committed 线未落（W1.5' 半程 abort）。
    AbortApplied,
    /// committed 线已落、complete 未落（W2）。
    CommittedWritten,
    /// complete 已落、completed 线未落（W3）。
    TaskCompleted,
}

/// 镜像首走产物（供断言与读数）。
struct Walked {
    adjudication_id: String,
    task_id: String,
    receipt_key: String,
    receipt_seq: u64,
}

/// 镜像首走的外部设施面（收拢参数列）。
#[derive(Clone, Copy)]
struct WalkCtx<'a> {
    root: &'a Path,
    station: &'a Station,
    client: &'a EngineClient,
}

/// 镜像 run_steward_loop 首走到指定停机点（故障注入面；全部走真实 pub 调用）。
/// 返回前打印注入读数（停机点 + 受理回执键面）。
fn walk_to_crash(
    ctx: &WalkCtx<'_>,
    order_id: &str,
    decision: Decision,
    next_orders: Vec<String>,
    role: Role,
    stop: &Stop,
) -> Walked {
    let walked = walk_first_leg(ctx, order_id, decision, next_orders, role, stop);
    println!(
        "[注入] {order_id} {} 停机于 {:?}：task={} receipt(seq={} key={})",
        walked.adjudication_id, stop, walked.task_id, walked.receipt_seq, walked.receipt_key
    );
    walked
}

fn walk_first_leg(
    ctx: &WalkCtx<'_>,
    order_id: &str,
    decision: Decision,
    next_orders: Vec<String>,
    role: Role,
    stop: &Stop,
) -> Walked {
    let WalkCtx {
        root,
        station,
        client,
    } = *ctx;
    let state = StateDir::new(root);
    let keypair = Ed25519Keypair::from_seed(&STEWARD_SEED);
    let (doc, source) =
        generate_adjudication_doc(&state, order_id, next_orders.clone(), time(DECISION_EXPIRY))
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
        .claim(WORKER, STEWARD_ROLE, CRASH_LEASE_MS)
        .unwrap()
        .unwrap();
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
    let signed =
        DecisionArtifact::seal("steward-a", role, decision, &payload, &keypair, attributes);
    let digest = decision_digest(&signed);
    let stable_id = source.stable_id().as_str().to_string();
    let receipt_key = idempotency_key(&stable_id, seq, &digest);
    let mut journal = AdjudicationJournal::open(&state, order_id, &doc.adjudication_id).unwrap();
    journal
        .append_prepared(PreparedIntent {
            adjudication_id: doc.adjudication_id.clone(),
            task_id: task.id.clone(),
            source_order_id: order_id.to_string(),
            source_stable_id: stable_id,
            decision,
            decision_seq: seq,
            decision_digest: digest,
            receipt_idempotency_key: receipt_key.clone(),
            next_orders: doc.next_orders.clone(),
            ts_ms: chrono::Utc::now().timestamp_millis(),
        })
        .unwrap();
    if *stop == Stop::PreparedWritten {
        return Walked {
            adjudication_id: doc.adjudication_id,
            task_id: task.id,
            receipt_key,
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
    if *stop == Stop::Admitted {
        return Walked {
            adjudication_id: doc.adjudication_id,
            task_id: task.id,
            receipt_key,
            receipt_seq: seq,
        };
    }
    // 分支结算（镜像 settle_decision_branches 的落账形状）。
    let mut source = source;
    let (rework_order_ids, aborted) = match decision {
        Decision::Revise => {
            let n = match stop {
                Stop::ReviseMaterialized(n) => *n,
                _ => doc.next_orders.len(),
            };
            let mut ids = Vec::new();
            for rework_id in doc.next_orders.iter().take(n) {
                materialize_rework(&state, &source, rework_id);
                ids.push(rework_id.clone());
            }
            if let Stop::ReviseMaterialized(actual) = stop {
                assert_eq!(*actual, ids.len(), "部分物化停机点应小于计划面");
                return Walked {
                    adjudication_id: doc.adjudication_id,
                    task_id: task.id,
                    receipt_key,
                    receipt_seq: seq,
                };
            }
            (ids, false)
        }
        Decision::Reject => {
            cancel_source(&state, &mut source);
            if *stop == Stop::AbortApplied {
                return Walked {
                    adjudication_id: doc.adjudication_id,
                    task_id: task.id,
                    receipt_key,
                    receipt_seq: seq,
                };
            }
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
    if *stop == Stop::CommittedWritten {
        return Walked {
            adjudication_id: doc.adjudication_id,
            task_id: task.id,
            receipt_key,
            receipt_seq: seq,
        };
    }
    assert!(station.complete(&task.id, WORKER).unwrap());
    Walked {
        adjudication_id: doc.adjudication_id,
        task_id: task.id,
        receipt_key,
        receipt_seq: seq,
    }
}

/// rework 物化的测试侧镜像（与 spawn_rework_order 同款落账形状）。
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
    af_execline::OrderLedger::open(&dir.join(LEDGER_FILE))
        .unwrap()
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
    af_execline::OrderLedger::open(&state.order_dir(&order_id).join(LEDGER_FILE))
        .unwrap()
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

/// 运维重试 = run_steward_loop（与既有套件同款：next_orders 空，重放臂按
/// 被认领文书自己的计划面收敛）。
fn retry_loop(
    root: &Path,
    station: &Station,
    client: &EngineClient,
    order_id: &str,
    decision: Decision,
    role: Role,
) -> Result<af_order::adjudicate::StewardOutcome, AdjudicateError> {
    let state = StateDir::new(root);
    run_steward_loop(
        &seed(order_id, decision, role),
        &facilities(&state, station, client, &registry(role)),
    )
}

/// 读数辅助：意图账相位序列（原始读数面）。
fn journal_phases(root: &Path, order_id: &str, adjudication_id: &str) -> Vec<String> {
    let journal = AdjudicationJournal::open(&StateDir::new(root), order_id, adjudication_id)
        .map(|j| j.lines().to_vec())
        .unwrap_or_default();
    journal
        .iter()
        .map(|line| match line {
            af_order::adjudicate::JournalLine::Prepared(_) => "prepared".into(),
            af_order::adjudicate::JournalLine::Committed(_) => "committed".into(),
            af_order::adjudicate::JournalLine::Completed { .. } => "completed".into(),
        })
        .collect()
}

fn count_receipts(root: &Path, order_id: &str) -> usize {
    load_receipts(&StateDir::new(root), order_id).unwrap().len()
}

fn count_ledger_ops(root: &Path, order_id: &str, op: &str) -> usize {
    af_execline::OrderLedger::load(&StateDir::new(root).order_dir(order_id).join(LEDGER_FILE))
        .unwrap()
        .iter()
        .filter(|r| r.op == op)
        .count()
}

fn task_state(station: &Station, task_id: &str) -> TaskState {
    station.get(task_id).unwrap().unwrap().state
}

/// A1（W1.5 部分结算）：Revise 计划面两张 rework，崩溃在第一张已物化、
/// 第二张未物化、committed 线未落。判据：reconcile 幂等补齐（已有面不双落、
/// 缺面补上），双跑补偿零新写入；租约回炉后重放收敛，受理账恒 1 笔。
#[test]
fn acc_a1_partial_rework_settlement_converges() {
    let root = scratch("a1");
    let order_id = "wo-acc-a1";
    seed_closed_order(&root, order_id);
    let rw1 = format!("{order_id}-rw1");
    let rw2 = format!("{order_id}-rw2");
    let clock = af_station::ManualClock::new(1_767_225_600_000);
    let station = Station::open_in_memory_with_clock(clock.clone())
        .unwrap()
        .into_station();
    let client = engine_client();

    // 注入：首走到「第一张 rework 已物化」即停（W1.5）。
    let walked = walk_to_crash(
        &WalkCtx {
            root: &root,
            station: &station,
            client: &client,
        },
        order_id,
        Decision::Revise,
        vec![rw1.clone(), rw2.clone()],
        Role::Reviser,
        &Stop::ReviseMaterialized(1),
    );
    let state = StateDir::new(&root);
    println!(
        "[A1/注入后] journal = {:?}",
        journal_phases(&root, order_id, &walked.adjudication_id)
    );
    println!(
        "[A1/注入后] rw1 create_order 行 = {}，rw2 账本在否 = {}，receipts = {}",
        count_ledger_ops(&root, &rw1, "create_order"),
        state.order_dir(&rw2).join(LEDGER_FILE).exists(),
        count_receipts(&root, order_id)
    );
    assert_eq!(
        journal_phases(&root, order_id, &walked.adjudication_id),
        vec!["prepared"]
    );
    assert_eq!(count_ledger_ops(&root, &rw1, "create_order"), 1);
    assert!(!state.order_dir(&rw2).join(LEDGER_FILE).exists());
    assert_eq!(count_receipts(&root, order_id), 1, "提交点已过");

    // 补偿① reconcile：receipt 在账 → 幂等结算（rw1 跳过、rw2 补物化）+ committed 线补缺。
    let reports = reconcile_adjudications(&state, &station).unwrap();
    println!("[A1/补偿①reconcile] {reports:?}");
    assert!(matches!(
        reports[0].status,
        ReconcileStatus::LeaseHeld { .. }
    ));
    let journal = AdjudicationJournal::open(&state, order_id, &walked.adjudication_id).unwrap();
    let committed = journal.latest_committed().unwrap();
    assert_eq!(committed.rework_order_ids, vec![rw1.clone(), rw2.clone()]);
    assert_eq!(
        count_ledger_ops(&root, &rw1, "create_order"),
        1,
        "已有面不双落"
    );
    assert_eq!(count_ledger_ops(&root, &rw2, "create_order"), 1, "缺面补上");

    // 补偿② 再跑一次 reconcile：零新写入（补偿幂等判据）。
    let lines_before = journal.lines().len();
    let again = reconcile_adjudications(&state, &station).unwrap();
    println!("[A1/补偿②reconcile再跑] {again:?}");
    assert!(matches!(again[0].status, ReconcileStatus::LeaseHeld { .. }));
    let journal = AdjudicationJournal::open(&state, order_id, &walked.adjudication_id).unwrap();
    assert_eq!(
        journal.lines().len(),
        lines_before,
        "双跑补偿不得新增意图账行"
    );
    assert_eq!(count_ledger_ops(&root, &rw1, "create_order"), 1);
    assert_eq!(count_ledger_ops(&root, &rw2, "create_order"), 1);

    // 补偿③ 租约回炉 → 重放臂按 committed 线收敛（不重物化、不二受理）。
    clock.advance(CRASH_LEASE_MS + 1_000);
    let outcome = retry_loop(
        &root,
        &station,
        &client,
        order_id,
        Decision::Revise,
        Role::Reviser,
    )
    .expect("重放收敛应走通");
    assert_eq!(outcome.rework_order_ids, vec![rw1.clone(), rw2.clone()]);
    assert_eq!(task_state(&station, &walked.task_id), TaskState::Done);
    assert_eq!(count_receipts(&root, order_id), 1, "重放不得二受理");
    assert_eq!(
        count_ledger_ops(&root, &rw1, "create_order"),
        1,
        "不双物化 rw1"
    );
    assert_eq!(
        count_ledger_ops(&root, &rw2, "create_order"),
        1,
        "不双物化 rw2"
    );
    println!(
        "[A1/补偿后] journal = {:?}，receipts = {}",
        journal_phases(&root, order_id, &walked.adjudication_id),
        count_receipts(&root, order_id)
    );
    assert_eq!(
        journal_phases(&root, order_id, &walked.adjudication_id),
        vec!["prepared", "committed", "completed"]
    );
    let _ = fs::remove_dir_all(&root);
}

/// A2（W1.5' 半程 abort）：Reject，cancel 行已落、committed 线未落即崩。
/// 判据：reconcile 幂等结算（不双落 cancel）+ committed 线补缺；重放收敛。
#[test]
fn acc_a2_mid_abort_converges() {
    let root = scratch("a2");
    let order_id = "wo-acc-a2";
    seed_released_order(&root, order_id);
    let clock = af_station::ManualClock::new(1_767_225_600_000);
    let station = Station::open_in_memory_with_clock(clock.clone())
        .unwrap()
        .into_station();
    let client = engine_client();

    let walked = walk_to_crash(
        &WalkCtx {
            root: &root,
            station: &station,
            client: &client,
        },
        order_id,
        Decision::Reject,
        vec![],
        Role::Approver,
        &Stop::AbortApplied,
    );
    println!(
        "[A2/注入后] journal = {:?}",
        journal_phases(&root, order_id, &walked.adjudication_id)
    );
    assert_eq!(
        journal_phases(&root, order_id, &walked.adjudication_id),
        vec!["prepared"]
    );
    assert_eq!(
        count_ledger_ops(&root, order_id, "cancel"),
        1,
        "崩溃前 abort 已落一笔"
    );

    // 补偿① reconcile：receipt 在账 → 幂等重结算（已 Cancelled 直接收敛）+ committed 线补缺。
    let state = StateDir::new(&root);
    let reports = reconcile_adjudications(&state, &station).unwrap();
    println!("[A2/补偿①reconcile] {reports:?}");
    assert!(matches!(
        reports[0].status,
        ReconcileStatus::LeaseHeld { .. }
    ));
    let journal = AdjudicationJournal::open(&state, order_id, &walked.adjudication_id).unwrap();
    assert!(journal.latest_committed().unwrap().aborted);
    assert_eq!(
        count_ledger_ops(&root, order_id, "cancel"),
        1,
        "不得双落 cancel"
    );

    // 补偿② 再跑 reconcile：零新写入。
    let lines_before = journal.lines().len();
    let again = reconcile_adjudications(&state, &station).unwrap();
    println!("[A2/补偿②reconcile再跑] {again:?}");
    let journal = AdjudicationJournal::open(&state, order_id, &walked.adjudication_id).unwrap();
    assert_eq!(journal.lines().len(), lines_before);
    assert_eq!(count_ledger_ops(&root, order_id, "cancel"), 1);

    // 补偿③ 租约回炉 → 重放按 committed 线收敛。
    clock.advance(CRASH_LEASE_MS + 1_000);
    let outcome = retry_loop(
        &root,
        &station,
        &client,
        order_id,
        Decision::Reject,
        Role::Approver,
    )
    .expect("重放收敛应走通");
    assert!(outcome.aborted);
    assert_eq!(count_receipts(&root, order_id), 1);
    assert_eq!(
        count_ledger_ops(&root, order_id, "cancel"),
        1,
        "重放不得重复 abort"
    );
    assert_eq!(task_state(&station, &walked.task_id), TaskState::Done);
    println!(
        "[A2/补偿后] journal = {:?}，cancel 行 = {}，receipts = {}",
        journal_phases(&root, order_id, &walked.adjudication_id),
        count_ledger_ops(&root, order_id, "cancel"),
        count_receipts(&root, order_id)
    );
    assert_eq!(
        journal_phases(&root, order_id, &walked.adjudication_id),
        vec!["prepared", "committed", "completed"]
    );
    let _ = fs::remove_dir_all(&root);
}

/// A3（差距实证 G-1）：Closed 源单 + Reject——admit（OR2102 放行 Closed）已把
/// 受理账落定（提交点已过），`abort_source` 撞 MissingEdge（Closed 无 abort
/// 边）。补偿两面（重放臂/reconcile）均不可收敛；租约未到期窗内重试还会
/// 再开一轮受理再撞墙（受理账 +1）。断言钉住当前行为——这是**登记差距**，
/// 不是验收通过。
#[test]
fn acc_a3_reject_on_closed_source_is_nonconvergent() {
    let root = scratch("a3");
    let order_id = "wo-acc-a3";
    seed_closed_order(&root, order_id);
    let clock = af_station::ManualClock::new(1_767_225_600_000);
    let station = Station::open_in_memory_with_clock(clock.clone())
        .unwrap()
        .into_station();
    let client = engine_client();
    let state = StateDir::new(&root);

    // 首走（真路径，无镜像）：receipt 落账后 settle 撞 MissingEdge。
    let err = retry_loop(
        &root,
        &station,
        &client,
        order_id,
        Decision::Reject,
        Role::Approver,
    )
    .expect_err("Closed+Reject 应在提交点后报 MissingEdge");
    println!("[A3/首走] err = {err}");
    assert!(matches!(err, AdjudicateError::MissingEdge(_)), "{err}");
    println!(
        "[A3/注入后] receipts = {}，cancel 行 = {}，task(adj-1) = {:?}",
        count_receipts(&root, order_id),
        count_ledger_ops(&root, order_id, "cancel"),
        task_state(&station, "adj-wo-acc-a3-1")
    );
    assert_eq!(
        count_receipts(&root, order_id),
        1,
        "提交点已过：受理账已落 1 笔"
    );
    assert_eq!(
        count_ledger_ops(&root, order_id, "cancel"),
        0,
        "分支结算未发生"
    );
    assert_eq!(task_state(&station, "adj-wo-acc-a3-1"), TaskState::Claimed);
    // 悬空形态：prepared 在账 + 回执在账，committed/completed 永缺。
    assert_eq!(
        journal_phases(&root, order_id, "adj-wo-acc-a3-1"),
        vec!["prepared"]
    );

    // 补偿① reconcile：重结算撞 MissingEdge → 整体 Err（健康账也被阻断，另见 A4）。
    let err =
        reconcile_adjudications(&state, &station).expect_err("reconcile 应被 MissingEdge 打断");
    println!("[A3/补偿①reconcile] err = {err}");
    assert!(matches!(err, AdjudicateError::MissingEdge(_)), "{err}");
    assert_eq!(
        journal_phases(&root, order_id, "adj-wo-acc-a3-1"),
        vec!["prepared"],
        "reconcile 不可收敛此形态"
    );

    // 补偿② 租约**未到期**即重试：认领到 adj-2（本轮新生成单）→ 首走真受理
    // seq=2 → 再撞 MissingEdge——受理账再 +1（每次抢先重试多一笔不可结算受理）。
    let err = retry_loop(
        &root,
        &station,
        &client,
        order_id,
        Decision::Reject,
        Role::Approver,
    )
    .expect_err("抢先重试应再次撞 MissingEdge");
    println!("[A3/补偿②抢先重试] err = {err}");
    assert!(matches!(err, AdjudicateError::MissingEdge(_)), "{err}");
    println!(
        "[A3/补偿②后] receipts = {}，journal(adj-1) = {:?}，journal(adj-2) = {:?}",
        count_receipts(&root, order_id),
        journal_phases(&root, order_id, "adj-wo-acc-a3-1"),
        journal_phases(&root, order_id, "adj-wo-acc-a3-2")
    );
    assert_eq!(
        count_receipts(&root, order_id),
        2,
        "抢先重试=第二笔不可结算受理"
    );
    assert_eq!(
        journal_phases(&root, order_id, "adj-wo-acc-a3-2"),
        vec!["prepared"],
        "adj-2 同样悬空"
    );

    // 补偿③ 租约到期回炉再重试：重放臂认领 adj-1 → 重结算 → 仍 MissingEdge。
    clock.advance(RETRY_LEASE_MS + 1_000);
    let err = retry_loop(
        &root,
        &station,
        &client,
        order_id,
        Decision::Reject,
        Role::Approver,
    )
    .expect_err("重放臂同样不可收敛");
    println!("[A3/补偿③重放] err = {err}");
    assert!(matches!(err, AdjudicateError::MissingEdge(_)), "{err}");
    assert_eq!(
        count_receipts(&root, order_id),
        2,
        "重放不新增受理，但旧账永不收敛"
    );
    assert_eq!(
        journal_phases(&root, order_id, "adj-wo-acc-a3-1"),
        vec!["prepared"],
        "悬空态：既非「已 prepared 未 committed」（提交点已过），也非「已完成」"
    );
    assert_eq!(
        task_state(&station, "adj-wo-acc-a3-1"),
        TaskState::Claimed,
        "任务面随租约回炉永远打转"
    );
    let _ = fs::remove_dir_all(&root);
}

/// A4（差距实证 G-2）：单份毒化意图账（A3 形态）令 reconcile 整体 Err，
/// 同批**健康**意图账（W3 形态：任务已 done、待补收口线）的收敛被阻断。
#[test]
fn acc_a4_poisoned_journal_blocks_whole_reconcile_scan() {
    let root = scratch("a4");
    let poison_id = "wo-acc-a4-a-poison"; // 字典序在前：扫描先撞毒账
    let healthy_id = "wo-acc-a4-b-healthy";
    seed_closed_order(&root, poison_id);
    seed_closed_order(&root, healthy_id);
    let clock = af_station::ManualClock::new(1_767_225_600_000);
    let station = Station::open_in_memory_with_clock(clock)
        .unwrap()
        .into_station();
    let client = engine_client();

    // 毒账：Closed+Reject，receipt 已落、settle 永远 MissingEdge。
    let err = retry_loop(
        &root,
        &station,
        &client,
        poison_id,
        Decision::Reject,
        Role::Approver,
    )
    .expect_err("毒账应在提交点后报 MissingEdge");
    assert!(matches!(err, AdjudicateError::MissingEdge(_)), "{err}");
    // 健康账：Approve 全走完到 complete，只缺 completed 收口线（W3 形态）。
    let healthy = walk_to_crash(
        &WalkCtx {
            root: &root,
            station: &station,
            client: &client,
        },
        healthy_id,
        Decision::Approve,
        vec![],
        Role::Approver,
        &Stop::TaskCompleted,
    );
    let state = StateDir::new(&root);
    assert_eq!(
        journal_phases(&root, healthy_id, &healthy.adjudication_id),
        vec!["prepared", "committed"]
    );
    assert_eq!(task_state(&station, &healthy.task_id), TaskState::Done);

    // 补偿：reconcile 整体 Err——毒账先撞（字典序），健康账的收口没做。
    let err = reconcile_adjudications(&state, &station).expect_err("毒账应打断整体扫描");
    println!("[A4/补偿reconcile] err = {err}");
    assert!(matches!(err, AdjudicateError::MissingEdge(_)), "{err}");
    println!(
        "[A4/补偿后] 健康账 journal = {:?}（无 completed ⇒ 收敛被阻断）",
        journal_phases(&root, healthy_id, &healthy.adjudication_id)
    );
    assert_eq!(
        journal_phases(&root, healthy_id, &healthy.adjudication_id),
        vec!["prepared", "committed"],
        "单账毒化阻断了健康账的收口（G-2）"
    );
    let _ = fs::remove_dir_all(&root);
}

/// A5（补偿调用副作用 G-3）：W2 崩溃后一次补偿（run_steward_loop）收敛旧单，
/// 但调用自身先无条件生成新裁决单入队——队列留残单；**二次补偿**会认领残单
/// 走真受理（seq+1 第二笔）。「补偿跑两次与跑一次结果相同」在调用面不成立
/// （重放臂自身对同一裁决单是幂等的）。
#[test]
fn acc_a5_recovery_retry_spawns_pending_round_on_second_call() {
    let root = scratch("a5");
    let order_id = "wo-acc-a5";
    seed_closed_order(&root, order_id);
    let clock = af_station::ManualClock::new(1_767_225_600_000);
    let station = Station::open_in_memory_with_clock(clock.clone())
        .unwrap()
        .into_station();
    let client = engine_client();

    let walked = walk_to_crash(
        &WalkCtx {
            root: &root,
            station: &station,
            client: &client,
        },
        order_id,
        Decision::Approve,
        vec![],
        Role::Approver,
        &Stop::CommittedWritten,
    );
    assert_eq!(count_receipts(&root, order_id), 1);

    // 补偿①：租约回炉 → 重放收敛 adj-1。
    clock.advance(CRASH_LEASE_MS + 1_000);
    let outcome = retry_loop(
        &root,
        &station,
        &client,
        order_id,
        Decision::Approve,
        Role::Approver,
    )
    .expect("重放收敛应走通");
    assert_eq!(outcome.task_id, walked.adjudication_id);
    assert!(outcome.receipt.replayed);
    assert_eq!(count_receipts(&root, order_id), 1);
    assert_eq!(task_state(&station, &walked.task_id), TaskState::Done);
    // 残面读数：补偿调用自己入队的 adj-2 留在队列（无人认领、无意图账）。
    let residue_id = "adj-wo-acc-a5-2";
    println!(
        "[A5/补偿①后] receipts = {}，task(adj-2) = {:?}，journal(adj-2) = {:?}",
        count_receipts(&root, order_id),
        task_state(&station, residue_id),
        journal_phases(&root, order_id, residue_id)
    );
    assert_eq!(
        task_state(&station, residue_id),
        TaskState::Queued,
        "残单在队"
    );
    assert_eq!(
        journal_phases(&root, order_id, residue_id),
        Vec::<String>::new(),
        "残单无意图账（未被认领过）"
    );

    // 补偿②：认领残单 adj-2 → 首走**真受理** seq=2（受理账第二笔）。
    let outcome2 = retry_loop(
        &root,
        &station,
        &client,
        order_id,
        Decision::Approve,
        Role::Approver,
    )
    .expect("二次补偿会走通（但走的是新轮次真受理）");
    assert_eq!(outcome2.task_id, residue_id, "二次补偿认领的是残单");
    assert!(!outcome2.receipt.replayed, "残单走首走真受理");
    let receipts = load_receipts(&StateDir::new(&root), order_id).unwrap();
    println!(
        "[A5/补偿②后] receipts = {}（seq 集合 = {:?}）",
        receipts.len(),
        receipts.iter().map(|r| r.decision_seq).collect::<Vec<_>>()
    );
    assert_eq!(receipts.len(), 2, "二次补偿 = 第二笔受理（新轮次）");
    assert_eq!(task_state(&station, residue_id), TaskState::Done);
    assert_eq!(
        journal_phases(&root, order_id, residue_id),
        vec!["prepared", "committed", "completed"]
    );
    let _ = fs::remove_dir_all(&root);
}

/// A6（差距实证 G-4）：lease 库漂移——W1 崩溃后 lease 库丢任务（基础设施级
/// 故障注入：换一个全新 station 实例）。reconcile 永远留检（PreparedResidue
/// 缺位形态），重试只能开新轮次，旧意图账永不收敛。
#[test]
fn acc_a6_lease_db_drift_leaves_permanent_residue() {
    let root = scratch("a6");
    let order_id = "wo-acc-a6";
    seed_closed_order(&root, order_id);
    let clock = af_station::ManualClock::new(1_767_225_600_000);
    let station_a = Station::open_in_memory_with_clock(clock.clone())
        .unwrap()
        .into_station();
    let client = engine_client();

    let walked = walk_to_crash(
        &WalkCtx {
            root: &root,
            station: &station_a,
            client: &client,
        },
        order_id,
        Decision::Approve,
        vec![],
        Role::Approver,
        &Stop::Admitted,
    );
    assert_eq!(count_receipts(&root, order_id), 1);

    // 注入：lease 库漂移（换新 station —— 任务 adj-1 在库上缺位）。
    let station_b = Station::open_in_memory_with_clock(clock)
        .unwrap()
        .into_station();
    let state = StateDir::new(&root);
    let reports = reconcile_adjudications(&state, &station_b).unwrap();
    println!("[A6/注入后reconcile] {reports:?}");
    match &reports[0].status {
        ReconcileStatus::PreparedResidue { detail } => {
            println!("[A6/留检细节] {detail}");
            assert!(detail.contains("缺位"), "应检出 lease 库缺位形态: {detail}");
        }
        other => panic!("应报 PreparedResidue，实得 {other:?}"),
    }
    // 读数（实测）：settle/committed 线补缺发生在任务缺位检查**之前**——
    // journal/WIP 面先收敛到 committed，只有任务面/收口线被留检。
    assert_eq!(
        journal_phases(&root, order_id, &walked.adjudication_id),
        vec!["prepared", "committed"],
        "journal 面已收敛到 committed（receipt 在账即结算），任务面留检"
    );

    // 补偿：重试只能在新库上开新轮次（adj-2 首走真受理 seq=2），旧账永留检。
    let outcome = retry_loop(
        &root,
        &station_b,
        &client,
        order_id,
        Decision::Approve,
        Role::Approver,
    )
    .expect("新轮次应走通");
    assert_eq!(outcome.task_id, "adj-wo-acc-a6-2");
    assert_eq!(count_receipts(&root, order_id), 2);
    let reports = reconcile_adjudications(&state, &station_b).unwrap();
    println!("[A6/补偿后再reconcile] {reports:?}");
    assert!(matches!(
        reports[0].status,
        ReconcileStatus::PreparedResidue { .. }
    ));
    assert_eq!(
        journal_phases(&root, order_id, &walked.adjudication_id),
        vec!["prepared", "committed"],
        "G-4：旧意图账（回执在账、committed 线已补）的 completed 收口线永不落——\
         任务缺位 ⇒ 无 complete 面，reconcile 每轮留检"
    );
    let _ = fs::remove_dir_all(&root);
}

/// A7（差距实证 G-5）：受理账漂移——W2 崩溃后受理账被抹（受理账在落账后
/// 损坏的形态）。reconcile 报 PreparedResidue（不 fail-loud，与重放臂不对称）；
/// 重放臂 fail-loud 拒绝重放（「跨库账面漂移」），任务永久悬空。
#[test]
fn acc_a7_admissions_ledger_drift_fails_loud_and_hangs() {
    let root = scratch("a7");
    let order_id = "wo-acc-a7";
    seed_closed_order(&root, order_id);
    let clock = af_station::ManualClock::new(1_767_225_600_000);
    let station = Station::open_in_memory_with_clock(clock.clone())
        .unwrap()
        .into_station();
    let client = engine_client();
    let state = StateDir::new(&root);

    let walked = walk_to_crash(
        &WalkCtx {
            root: &root,
            station: &station,
            client: &client,
        },
        order_id,
        Decision::Approve,
        vec![],
        Role::Approver,
        &Stop::CommittedWritten,
    );
    assert_eq!(count_receipts(&root, order_id), 1);

    // 注入：抹掉受理账（committed 线在、回执缺位 = 跨库账面漂移）。
    fs::remove_file(admissions_path(&state, order_id)).unwrap();
    println!(
        "[A7/注入后] journal = {:?}，receipts = {}",
        journal_phases(&root, order_id, &walked.adjudication_id),
        count_receipts(&root, order_id)
    );
    assert_eq!(
        journal_phases(&root, order_id, &walked.adjudication_id),
        vec!["prepared", "committed"]
    );

    // 补偿① reconcile：报 PreparedResidue（回执缺席面），不 fail-loud。
    let reports = reconcile_adjudications(&state, &station).unwrap();
    println!("[A7/补偿①reconcile] {reports:?}");
    assert!(matches!(
        reports[0].status,
        ReconcileStatus::PreparedResidue { .. }
    ));

    // 补偿② 重放臂：fail-loud 拒绝重放（账面漂移），任务永久悬空。
    clock.advance(CRASH_LEASE_MS + 1_000);
    let err = retry_loop(
        &root,
        &station,
        &client,
        order_id,
        Decision::Approve,
        Role::Approver,
    )
    .expect_err("重放臂应拒绝漂移账");
    println!("[A7/补偿②重放] err = {err}");
    assert!(matches!(err, AdjudicateError::Queue(_)), "{err}");
    assert!(
        err.to_string().contains("漂移"),
        "应点名跨库账面漂移: {err}"
    );
    assert_eq!(
        journal_phases(&root, order_id, &walked.adjudication_id),
        vec!["prepared", "committed"],
        "G-5：committed 线在、回执缺位——重放被拒，永不收敛（fail-loud 留检）"
    );
    assert_eq!(task_state(&station, &walked.task_id), TaskState::Claimed);
    let _ = fs::remove_dir_all(&root);
}

/// A8（注入点返回错误）：越权角色（Reviser 签 Approve）首走——admit 被
/// OR2106 拒（零外部效应：受理账零写入），prepared 残留；reconcile 检出
/// PreparedResidue（无回执不可能双写）；换正权角色重试按首走收敛，受理账
/// 恒 1 笔。
#[test]
fn acc_a8_admit_rejection_leaves_residue_then_first_walk_converges() {
    let root = scratch("a8");
    let order_id = "wo-acc-a8";
    seed_closed_order(&root, order_id);
    let clock = af_station::ManualClock::new(1_767_225_600_000);
    let station = Station::open_in_memory_with_clock(clock.clone())
        .unwrap()
        .into_station();
    let client = engine_client();
    let state = StateDir::new(&root);

    // 注入：越权角色（Reviser 无 Approve 权）→ admit OR2106 拒。
    let err = retry_loop(
        &root,
        &station,
        &client,
        order_id,
        Decision::Approve,
        Role::Reviser,
    )
    .expect_err("越权首走应被 OR2106 拒");
    println!("[A8/注入] err = {err}");
    assert!(matches!(err, AdjudicateError::Order(_)), "{err}");
    assert!(
        err.to_string().contains("OR2106"),
        "应是引擎四层越权拒绝: {err}"
    );
    println!(
        "[A8/注入后] receipts = {}，journal(adj-1) = {:?}",
        count_receipts(&root, order_id),
        journal_phases(&root, order_id, "adj-wo-acc-a8-1")
    );
    assert!(!admissions_path(&state, order_id).exists(), "受理账零写入");
    assert_eq!(
        journal_phases(&root, order_id, "adj-wo-acc-a8-1"),
        vec!["prepared"],
        "prepared 残留（意图在账、提交点未到）"
    );

    // 补偿① reconcile：PreparedResidue（无回执 = 无已提交事实，留检）。
    let reports = reconcile_adjudications(&state, &station).unwrap();
    println!("[A8/补偿①reconcile] {reports:?}");
    assert!(matches!(
        reports[0].status,
        ReconcileStatus::PreparedResidue { .. }
    ));

    // 补偿② 换正权角色重试：首走认领持 60s 租约（RETRY_LEASE_MS），推过
    // 到期回炉 → 认领到旧任务 adj-1 → 首走真受理收敛。
    clock.advance(RETRY_LEASE_MS + 1_000);
    let outcome = retry_loop(
        &root,
        &station,
        &client,
        order_id,
        Decision::Approve,
        Role::Approver,
    )
    .expect("正权角色重试应收敛");
    assert_eq!(outcome.task_id, "adj-wo-acc-a8-1");
    assert!(!outcome.receipt.replayed, "首走真受理");
    assert_eq!(count_receipts(&root, order_id), 1, "受理账恒 1 笔");
    assert_eq!(task_state(&station, "adj-wo-acc-a8-1"), TaskState::Done);
    assert_eq!(
        journal_phases(&root, order_id, "adj-wo-acc-a8-1"),
        vec!["prepared", "prepared", "committed", "completed"],
        "重试产生第二条 prepared（意图在账、历史不抹）后收敛"
    );
    let _ = fs::remove_dir_all(&root);
}
