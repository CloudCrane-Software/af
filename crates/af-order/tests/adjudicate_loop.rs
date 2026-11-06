//! PoC-C 件③ steward 认领与签字闭环集成验收（真引擎子进程，真 af-station 队列）。
//!
//! 链（v0.3 T2 触发链的本 crate 落点）：
//! submit（stub 档全生命周期 create→…→close + MES 事件）→ 生成裁决单 →
//! 入队（幂等键 = stable_id/裁决单id）→ 认领 → 复算文书 → 真签
//! DecisionArtifact → admit → 决策分支 → complete 任务。
//!
//! 角色纪律不造权限系统：Reviser 签 Approve / Reviser 签 Arbitrate 在引擎
//! 四层被拒（OR2106，af-workorder 角色默认权限集是唯一权限面）；Arbiter 签
//! Arbitrate 过链（只有 Arbiter 能 pivot）。与 tests/admit.rs 同款纪律：
//! 不用 mock 引擎。

use std::fs;
use std::path::{Path, PathBuf};

use af_execline::EngineClient;
use af_order::adjudicate::{
    run_steward_loop, AdjudicateError, AdjudicationSeed, LoopFacilities, STEWARD_ROLE,
};
use af_order::{cli, StateDir};
use af_station::{State as TaskState, Station};
use af_workorder::{Decision, Ed25519Keypair, Role, TrustedActor, TrustedActorRegistry};

const NOW: &str = "2026-11-03T00:00:00Z";
const KEY_EXPIRY: &str = "2027-01-01T00:00:00Z";
const DECISION_EXPIRY: &str = "2026-12-01T00:00:00Z";
const STEWARD_SEED: [u8; 32] = [0x61; 32];

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
        "af-order-loop-it-{}-{}-{}",
        std::process::id(),
        tag,
        nanos
    ));
    fs::create_dir_all(&dir).expect("创建测试 scratch 目录");
    dir
}

fn time(text: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&chrono::Utc)
}

/// 任务单全生命周期：CLI submit --unit stub（stub 冒烟档，真引擎门控 + 真
/// 驱动层账本 + MES 事件落盘），返回 (state 根, order_id)。
fn submit_stub_order(root: &Path, order_id: &str) -> String {
    let spec_path = root.join(format!("{order_id}-spec.json"));
    fs::write(
        &spec_path,
        format!(
            r#"{{"order_id":"{order_id}","line":{{"line_id":"line-poc","line_version":"lv-1"}},"budget":100,"note":"PoC-C 裁决环闭环单"}}"#
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
    .expect("submit 全生命周期应走通")
}

/// 源单最近快照状态名（与驱动层账本同口径）。
fn latest_state(state: &Path, order_id: &str) -> String {
    let records = af_execline::OrderLedger::load(
        &StateDir::new(state)
            .order_dir(order_id)
            .join("ledger.jsonl"),
    )
    .unwrap();
    af_order::state::current_state(&records).unwrap()
}

/// 落一张已 release 的源单（账本手植，供 Reject-abort 臂：非终态有 abort 边）。
fn seed_released_order(state: &Path, order_id: &str) {
    use af_execline::LedgerRecord;
    use af_workorder::{GraphVersion, LineBinding, Priority, Sla, WorkOrder};
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
        .attach_budget_voucher(af_workorder::BudgetVoucher {
            voucher_id: format!("bv-{order_id}"),
            amount: 100,
            signed: true,
        })
        .unwrap();
    let dir = StateDir::new(state).order_dir(order_id);
    fs::create_dir_all(&dir).unwrap();
    let mut ledger = af_execline::OrderLedger::open(&dir.join("ledger.jsonl")).unwrap();
    order.release().unwrap();
    let record = LedgerRecord {
        seq: 0,
        ts_ms: 1_767_225_600_000,
        order_id: order_id.to_string(),
        stable_id: order.stable_id().as_str().to_string(),
        op: "release".into(),
        idem_key: format!("{}/release", order.stable_id()),
        ok: true,
        state_after: Some("Released".into()),
        error_kind: None,
        error_detail: None,
        order_after: Some(order),
    };
    ledger.append(record).unwrap();
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

#[test]
fn loop_approve_full_chain_from_submit_to_done() {
    let root = scratch("approve");
    let order_id = "wo-loop-approve";
    let summary = submit_stub_order(&root, order_id);
    assert!(
        summary.contains("\"state\": \"Closed\"") || summary.contains("\"state\":\"Closed\""),
        "submit 应闭环: {summary}"
    );
    // MES 事件面：驱动层 complete 腿已落 mes-events.jsonl。
    let mes_path = StateDir::new(&root)
        .order_dir(order_id)
        .join("mes-events.jsonl");
    let mes = fs::read_to_string(&mes_path).expect("MES 事件文件应已落盘");
    assert!(mes.contains("complete"), "MES 应含 complete 事件: {mes}");

    let state = StateDir::new(&root);
    let station = Station::open_in_memory().unwrap().into_station();
    let outcome = run_steward_loop(
        &seed(order_id, Decision::Approve, Vec::new(), Role::Approver),
        &LoopFacilities {
            state: &state,
            station: &station,
            client: &engine_client(),
            registry: &registry(Role::Approver),
            now: time(NOW),
        },
    )
    .unwrap_or_else(|err| panic!("Approve 闭环应走通: {err}"));

    // 入队幂等键 = 源单 stable_id + 裁决单 id。
    assert_eq!(
        outcome.enqueue,
        af_station::Enqueued::Inserted {
            id: outcome.adjudication_id.clone()
        }
    );
    // 裁决单文书落盘且可复算。
    assert_eq!(outcome.doc.doc_kind, "裁决单");
    assert_eq!(outcome.doc.template_id, "tpl-adjudication-v1");
    assert_eq!(
        outcome.doc_path,
        state
            .root()
            .join("adjudications")
            .join(order_id)
            .join(format!("{}.json", outcome.adjudication_id))
    );
    af_order::adjudicate::verify_doc(&outcome.doc).expect("落盘文书应可复算");
    // receipt：Closed 源单受理留档，四口径检查全在。
    assert_eq!(outcome.receipt.source_state, "Closed");
    assert_eq!(outcome.receipt.decision, "Approve");
    for check in ["basis_recheck", "digest_recheck", "credential_intersection"] {
        assert!(
            outcome.receipt.checks.iter().any(|name| name == check),
            "checks 应含 {check}: {:?}",
            outcome.receipt.checks
        );
    }
    // 任务闭环 done。
    let task = station.get(&outcome.task_id).unwrap().unwrap();
    assert_eq!(task.state, TaskState::Done);
    assert_eq!(
        station.count_by_state(TaskState::Queued).unwrap()
            + station.count_by_state(TaskState::Claimed).unwrap(),
        0,
        "队列应清空"
    );
    // 受理账本恰一笔。
    let admissions =
        fs::read_to_string(af_order::admit::admissions_path(&state, order_id)).unwrap();
    assert_eq!(admissions.lines().count(), 1);
    let _ = fs::remove_dir_all(&root);
}

/// 角色阶梯纪律（既有权限面，非新造）：Reviser 签 Approve 与签 Arbitrate
/// （pivot）都在引擎四层被拒（OR2106），任务保持 claimed 不完成。
#[test]
fn loop_role_discipline_reviser_cannot_approve_or_pivot() {
    let root = scratch("reviser");
    let order_id = "wo-loop-reviser";
    submit_stub_order(&root, order_id);
    let state = StateDir::new(&root);
    let station = Station::open_in_memory().unwrap().into_station();

    // (a) Reviser 无 Approve 权限。
    let err = run_steward_loop(
        &seed(order_id, Decision::Approve, Vec::new(), Role::Reviser),
        &LoopFacilities {
            state: &state,
            station: &station,
            client: &engine_client(),
            registry: &registry(Role::Reviser),
            now: time(NOW),
        },
    )
    .unwrap_err();
    let text = err.to_string();
    assert!(text.contains("OR2106"), "Reviser 批准应 OR2106: {text}");
    // 任务保持 claimed（失败面留给运营者，不静默吞任务）；受理零写入。
    let queued = station.count_by_state(TaskState::Queued).unwrap();
    let claimed = station.count_by_state(TaskState::Claimed).unwrap();
    assert_eq!(queued, 0, "任务已被认领，不应回 queued");
    assert_eq!(claimed, 1, "失败后任务保持 claimed 留检");
    assert!(
        !af_order::admit::admissions_path(&state, order_id).exists(),
        "被拒受理零写入"
    );

    // (b) Reviser 无 Arbitrate 权限——只有 Arbiter 能 pivot。
    let err2 = run_steward_loop(
        &seed(order_id, Decision::Arbitrate, Vec::new(), Role::Reviser),
        &LoopFacilities {
            state: &state,
            station: &station,
            client: &engine_client(),
            registry: &registry(Role::Reviser),
            now: time(NOW),
        },
    )
    .unwrap_err();
    assert!(
        err2.to_string().contains("OR2106"),
        "Reviser pivot 应 OR2106: {err2}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// Arbiter 可 pivot（过链）+ Revise 走 rework（按被签计划面物化后继待修订单）。
#[test]
fn loop_arbiter_pivots_and_revise_materializes_rework() {
    let root = scratch("arbiter-revise");
    let order_id = "wo-loop-arb";
    submit_stub_order(&root, order_id);
    let state = StateDir::new(&root);
    let rework_id = format!("{order_id}-rw");

    // (a) Arbiter 签 Arbitrate：唯一有 pivot 权的阶，全链过。
    let station = Station::open_in_memory().unwrap().into_station();
    let outcome = run_steward_loop(
        &seed(order_id, Decision::Arbitrate, Vec::new(), Role::Arbiter),
        &LoopFacilities {
            state: &state,
            station: &station,
            client: &engine_client(),
            registry: &registry(Role::Arbiter),
            now: time(NOW),
        },
    )
    .unwrap_or_else(|err| panic!("Arbiter pivot 应过: {err}"));
    assert_eq!(outcome.receipt.decision, "Arbitrate");
    assert!(
        !station.complete(&outcome.task_id, "steward-1").unwrap(),
        "任务已 done，重复 complete 必须失败（现任判定）"
    );

    // (b) Revise：被签计划面声明 rework 单号，闭环物化后继工单（Created = 待修订）。
    let station2 = Station::open_in_memory().unwrap().into_station();
    let outcome2 = run_steward_loop(
        &seed(
            order_id,
            Decision::Revise,
            vec![rework_id.clone()],
            Role::Reviser,
        ),
        &LoopFacilities {
            state: &state,
            station: &station2,
            client: &engine_client(),
            registry: &registry(Role::Reviser),
            now: time(NOW),
        },
    )
    .unwrap_or_else(|err| panic!("Revise 闭环应走通: {err}"));
    assert_eq!(outcome2.receipt.decision, "Revise");
    assert_eq!(outcome2.rework_order_ids, vec![rework_id.clone()]);
    let records = af_execline::OrderLedger::load(&state.order_dir(&rework_id).join("ledger.jsonl"))
        .expect("rework 单应落账");
    let rework = records
        .iter()
        .rev()
        .find_map(|record| record.order_after.clone())
        .unwrap();
    assert_eq!(
        af_order::state::current_state(&records).as_deref(),
        Some("Created"),
        "rework 单待修订态"
    );
    assert_eq!(rework.supersedes(), Some(order_id), "rework 后继关系");
    assert_eq!(rework.graph_version().as_str(), "af-order/poc-1");
    let task2 = station2.get(&outcome2.task_id).unwrap().unwrap();
    assert_eq!(task2.state, TaskState::Done);
    let _ = fs::remove_dir_all(&root);
}

/// Reject 分支：非终态源单真 abort（cancel + 账本 cancel 行）；终态 Closed
/// 源单无 abort 边——如实 MissingEdge 报红，不改单、不完成任务。
#[test]
fn loop_reject_aborts_active_source_and_fails_loud_on_terminal() {
    let root = scratch("reject");
    let order_id = "wo-loop-reject";
    seed_released_order(&root, order_id);
    let state = StateDir::new(&root);
    let station = Station::open_in_memory().unwrap().into_station();
    let outcome = run_steward_loop(
        &seed(order_id, Decision::Reject, Vec::new(), Role::Approver),
        &LoopFacilities {
            state: &state,
            station: &station,
            client: &engine_client(),
            registry: &registry(Role::Approver),
            now: time(NOW),
        },
    )
    .unwrap_or_else(|err| panic!("非终态 Reject 闭环应走通: {err}"));
    assert!(outcome.aborted, "非终态源单应被 abort");
    assert_eq!(
        latest_state(&root, order_id),
        "Cancelled",
        "源单转 Cancelled"
    );
    let records =
        af_execline::OrderLedger::load(&state.order_dir(order_id).join("ledger.jsonl")).unwrap();
    assert!(
        records.iter().any(|record| record.op == "cancel"),
        "abort 应落 cancel 账本行"
    );
    assert_eq!(outcome.receipt.decision, "Reject");
    let task = station.get(&outcome.task_id).unwrap().unwrap();
    assert_eq!(task.state, TaskState::Done);

    // 终态 Closed 源单：状态机无 Closed→Cancelled 边，MissingEdge 如实报红。
    let order_id2 = "wo-loop-reject-closed";
    submit_stub_order(&root, order_id2);
    let station2 = Station::open_in_memory().unwrap().into_station();
    let err = run_steward_loop(
        &seed(order_id2, Decision::Reject, Vec::new(), Role::Approver),
        &LoopFacilities {
            state: &state,
            station: &station2,
            client: &engine_client(),
            registry: &registry(Role::Approver),
            now: time(NOW),
        },
    )
    .unwrap_err();
    assert!(
        matches!(err, AdjudicateError::MissingEdge(_)),
        "终态 abort 应 MissingEdge: {err}"
    );
    assert_eq!(latest_state(&root, order_id2), "Closed", "源单不得被改");
    let stuck = station2.count_by_state(TaskState::Claimed).unwrap();
    assert_eq!(stuck, 1, "失败面任务保持 claimed 留检，不假装完成");
    let _ = fs::remove_dir_all(&root);
}

/// 同一裁决单幂等键重入队：Duplicate，不产生第二行（事件重投形态）。
#[test]
fn loop_reenqueue_same_adjudication_is_duplicate() {
    let root = scratch("reenqueue");
    let order_id = "wo-loop-redo";
    submit_stub_order(&root, order_id);
    let state = StateDir::new(&root);
    let station = Station::open_in_memory().unwrap().into_station();
    let outcome = run_steward_loop(
        &seed(order_id, Decision::Approve, Vec::new(), Role::Approver),
        &LoopFacilities {
            state: &state,
            station: &station,
            client: &engine_client(),
            registry: &registry(Role::Approver),
            now: time(NOW),
        },
    )
    .unwrap();
    // 用同一裁决单号 + 同一幂等键再入队（模拟 event-router 重投）。
    let key = format!(
        "{}/{}",
        outcome.doc.source_stable_id, outcome.doc.adjudication_id
    );
    let again = station
        .enqueue(af_station::NewTask {
            id: "adj-redo-probe",
            payload_ref: "whatever",
            required_role: STEWARD_ROLE,
            idempotency_key: Some(&key),
            deadline: None,
        })
        .unwrap();
    assert_eq!(
        again,
        af_station::Enqueued::Duplicate {
            existing_id: outcome.adjudication_id.clone()
        },
        "同幂等键重入队必须 Duplicate"
    );
    let _ = fs::remove_dir_all(&root);
}
