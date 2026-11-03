//! PoC-A2 admit 集成验收：真引擎子进程走 admit_decision 四层真验签路径 +
//! receipt 幂等窗两条负例（同键重放 / 同 seq 异摘要）+ 各步类型化拒绝。
//!
//! 与 tests/cli.rs 同款纪律：不用 mock 引擎——`EngineClient::locate` 定位/
//! 构建真 af-engine（CARGO_TARGET_DIR 经环境继承，与门面解析同优先级）。

use std::fs;
use std::path::{Path, PathBuf};

use af_execline::{EngineClient, LedgerRecord, OrderLedger};
use af_order::admit::{admissions_path, decision_digest, idempotency_key, AdmissionRequest};
use af_order::{parse_request, OrderError, StateDir};
use af_workorder::{
    Decision, DecisionArtifact, DecisionAttributes, Ed25519Keypair, GraphVersion, LineBinding,
    Priority, Role, Sla, TrustedActor, TrustedActorRegistry, WorkArtifact, WorkOrder,
};
use serde_json::{json, Value};

const NOW: &str = "2026-11-03T00:00:00Z";
const KEY_EXPIRY: &str = "2027-01-01T00:00:00Z";
const DECISION_EXPIRY: &str = "2026-11-04T00:00:00Z";
const SEED_TRUSTED: [u8; 32] = [0x31; 32];

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
        "af-order-admit-it-{}-{}-{}",
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

/// 造一张已 release 的源单并落 ledger（create_order + release 两行，
/// 恢复面只看最近快照——与驱动层同口径）。
fn seed_released_order(state: &Path, order_id: &str) {
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
    let mut ledger = OrderLedger::open(&dir.join("ledger.jsonl")).unwrap();
    let stable_id = order.stable_id().as_str().to_string();
    let row = |op: &str, snapshot: WorkOrder| LedgerRecord {
        seq: 0,
        ts_ms: 1_767_225_600_000,
        order_id: order_id.to_string(),
        stable_id: stable_id.clone(),
        op: op.to_string(),
        idem_key: format!("{stable_id}/{op}"),
        ok: true,
        state_after: Some("Created".into()),
        error_kind: None,
        error_detail: None,
        order_after: Some(snapshot),
    };
    ledger.append(row("create_order", order.clone())).unwrap();
    order.release().unwrap();
    ledger.append(row("release", order)).unwrap();
}

/// 造一张已 Closed 的终态源单（全生命周期走过）。
fn seed_terminal_order(state: &Path, order_id: &str, cancelled: bool) {
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
            voucher_id: "bv-x".into(),
            amount: 100,
            signed: true,
        })
        .unwrap();
    let dir = StateDir::new(state).order_dir(order_id);
    fs::create_dir_all(&dir).unwrap();
    let mut ledger = OrderLedger::open(&dir.join("ledger.jsonl")).unwrap();
    let stable_id = order.stable_id().as_str().to_string();
    order.release().unwrap();
    if cancelled {
        order.cancel().unwrap();
    } else {
        order.start().unwrap();
        order
            .attach_artifact(WorkArtifact::from_bytes("fa-1", true, true, b"done"))
            .unwrap();
        order.complete().unwrap();
        order.close().unwrap();
    }
    ledger
        .append(LedgerRecord {
            seq: 0,
            ts_ms: 1_767_225_600_000,
            order_id: order_id.to_string(),
            stable_id,
            op: if cancelled { "cancel" } else { "close" }.into(),
            idem_key: format!("{order_id}/{}", if cancelled { "cancel" } else { "close" }),
            ok: true,
            state_after: Some(if cancelled { "Cancelled" } else { "Closed" }.into()),
            error_kind: None,
            error_detail: None,
            order_after: Some(order),
        })
        .unwrap();
}

fn registry() -> TrustedActorRegistry {
    TrustedActorRegistry::new(vec![TrustedActor::new(
        "user-a",
        Role::Approver,
        &Ed25519Keypair::from_seed(&SEED_TRUSTED).public_key(),
        time(KEY_EXPIRY),
    )])
    .unwrap()
}

/// 按 admission payload 真签一份决策工件。
fn seal_admission_decision(
    order: &WorkOrder,
    seq: u64,
    basis: Vec<String>,
    decision_expiry: &str,
    seed: &[u8; 32],
) -> DecisionArtifact {
    DecisionArtifact::seal(
        "user-a",
        Role::Approver,
        Decision::Approve,
        &order.admission_decision_payload(),
        &Ed25519Keypair::from_seed(seed),
        DecisionAttributes::new(seq, basis, "a".repeat(64), time(decision_expiry)),
    )
}

/// 组一份 admission 请求 JSON（走 parse_request 解析，连解析面一起验收）。
fn request_json(
    order_id: &str,
    decision: &DecisionArtifact,
    registry: &TrustedActorRegistry,
) -> String {
    json!({
        "order_id": order_id,
        "decision": serde_json::to_value(decision).unwrap(),
        "actor_registry": serde_json::to_value(registry).unwrap(),
        "template": {
            "template_id": "tpl-adjudication-v1",
            "template_version": "1.0",
            "doc_kind": "裁决单",
            "body_sha256": "c".repeat(64),
        },
        "resource_profile": {
            "profile_id": "rp-poc",
            "profile_version": "1.0",
            "claims": [{ "resource": "glm-cp", "units": 1 }],
        },
        "gate_list": {
            "gates": [{ "gate_id": "F1", "kind": "fast", "required": true }],
        },
    })
    .to_string()
}

fn parse(text: &str) -> AdmissionRequest {
    parse_request(text).expect("请求应可解析")
}

fn assert_admit_err(err: OrderError, code: &str) {
    let text = err.to_string();
    assert!(text.contains(code), "应报 {code}: {text}");
    assert_eq!(err.exit_code(), 1, "admit 拒绝退出码 1: {text}");
}

#[test]
fn admit_full_chain_then_idempotent_replay_and_seq_conflict() {
    let state = scratch("happy");
    let order_id = "wo-admit-happy";
    seed_released_order(&state, order_id);
    let order = {
        let records = OrderLedger::load(
            &StateDir::new(&state)
                .order_dir(order_id)
                .join("ledger.jsonl"),
        )
        .unwrap();
        records
            .iter()
            .rev()
            .find_map(|record| record.order_after.clone())
            .unwrap()
    };
    let client = engine_client();
    let decision = seal_admission_decision(
        &order,
        7,
        vec!["artifact:fa-1:qualified".into()],
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    let request = parse(&request_json(order_id, &decision, &registry()));

    // 首次受理：全链过，回执落盘。
    let receipt =
        af_order::admit::admit(&request, &StateDir::new(&state), &client, time(NOW)).unwrap();
    let expected_key = idempotency_key(order.stable_id().as_str(), 7, &decision_digest(&decision));
    assert_eq!(receipt.idempotency_key, expected_key);
    assert!(!receipt.replayed);
    assert_eq!(receipt.source_state, "Released");
    assert_eq!(receipt.decision_seq, 7);
    assert_eq!(
        receipt.checks,
        vec![
            "source_recovery",
            "terminal_state",
            "template_schema",
            "resource_profile_schema",
            "gate_list_schema",
            "decision_four_layer",
            "seq_window",
            "expiry",
            "basis_recheck",
            "digest_recheck",
            "credential_intersection",
        ]
    );
    assert!(
        receipt.engine_version.starts_with("0.2"),
        "真引擎版本入回执: {}",
        receipt.engine_version
    );
    let admissions = fs::read_to_string(admissions_path(&StateDir::new(&state), order_id))
        .expect("admissions.jsonl 应已落盘");
    assert_eq!(admissions.lines().count(), 1, "恰一笔受理");

    // 同键同摘要重放：返回原回执（replayed=true），零新增写入。
    let replayed =
        af_order::admit::admit(&request, &StateDir::new(&state), &client, time(NOW)).unwrap();
    assert!(replayed.replayed);
    let mut original = receipt.clone();
    original.replayed = true;
    assert_eq!(replayed, original, "重放应返回原回执");
    let admissions_after =
        fs::read_to_string(admissions_path(&StateDir::new(&state), order_id)).unwrap();
    assert_eq!(admissions_after.lines().count(), 1, "重放不得追加第二笔");

    // 同 seq 异摘要（同一 actor 重签不同 basis）：OR2107 拒绝，零写入。
    let redecided = seal_admission_decision(
        &order,
        7,
        vec!["artifact:fa-2:qualified".into()],
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    assert_ne!(
        decision_digest(&redecided),
        decision_digest(&decision),
        "测试前提：异摘要"
    );
    let conflicting = parse(&request_json(order_id, &redecided, &registry()));
    let err = af_order::admit::admit(&conflicting, &StateDir::new(&state), &client, time(NOW))
        .unwrap_err();
    assert_admit_err(err, "OR2107");
    let admissions_final =
        fs::read_to_string(admissions_path(&StateDir::new(&state), order_id)).unwrap();
    assert_eq!(
        admissions_final.lines().count(),
        1,
        "被拒请求零外部效应：不得追加受理行"
    );
    let _ = fs::remove_dir_all(&state);
}

#[test]
fn admit_rejects_cancelled_source_with_or2102_and_zero_writes() {
    let state = scratch("cancelled");
    let order_id = "wo-admit-cancelled";
    seed_terminal_order(&state, order_id, true);
    let order = {
        let records = OrderLedger::load(
            &StateDir::new(&state)
                .order_dir(order_id)
                .join("ledger.jsonl"),
        )
        .unwrap();
        records
            .iter()
            .rev()
            .find_map(|record| record.order_after.clone())
            .unwrap()
    };
    let decision = seal_admission_decision(
        &order,
        1,
        vec!["artifact:fa-1:qualified".into()],
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    let request = parse(&request_json(order_id, &decision, &registry()));
    let err = af_order::admit::admit(
        &request,
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_err();
    assert_admit_err(err, "OR2102");
    assert!(
        !admissions_path(&StateDir::new(&state), order_id).exists(),
        "第①步拒绝零写入"
    );
    let _ = fs::remove_dir_all(&state);
}

/// 闭环单（Closed）受理：裁决留档正是本链用途，receipt.source_state 如实
/// 记录 Closed——PoC-A2 任务2 的 CLI 闭环走的就是这条臂。
#[test]
fn admit_accepts_closed_source_and_records_source_state() {
    let state = scratch("closed-source");
    let order_id = "wo-admit-closed";
    seed_terminal_order(&state, order_id, false);
    let order = {
        let records = OrderLedger::load(
            &StateDir::new(&state)
                .order_dir(order_id)
                .join("ledger.jsonl"),
        )
        .unwrap();
        records
            .iter()
            .rev()
            .find_map(|record| record.order_after.clone())
            .unwrap()
    };
    assert_eq!(order.state(), af_workorder::OrderState::Closed);
    let decision = seal_admission_decision(
        &order,
        3,
        vec!["artifact:fa-1:qualified".into()],
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    let request = parse(&request_json(order_id, &decision, &registry()));
    let receipt = af_order::admit::admit(
        &request,
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap();
    assert_eq!(receipt.source_state, "Closed");
    assert!(!receipt.replayed);
    let _ = fs::remove_dir_all(&state);
}

#[test]
fn admit_rejects_untrusted_signature_with_or2106() {
    let state = scratch("badsign");
    let order_id = "wo-admit-badsign";
    seed_released_order(&state, order_id);
    let order = {
        let records = OrderLedger::load(
            &StateDir::new(&state)
                .order_dir(order_id)
                .join("ledger.jsonl"),
        )
        .unwrap();
        records
            .iter()
            .rev()
            .find_map(|record| record.order_after.clone())
            .unwrap()
    };
    // 未受信 key 签发（key 不在注册表）——四层第④层必拒。
    let decision = seal_admission_decision(
        &order,
        1,
        vec!["artifact:fa-1:qualified".into()],
        DECISION_EXPIRY,
        &[0x42; 32],
    );
    let request = parse(&request_json(order_id, &decision, &registry()));
    let err = af_order::admit::admit(
        &request,
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_err();
    assert_admit_err(err, "OR2106");
    assert!(
        !admissions_path(&StateDir::new(&state), order_id).exists(),
        "第③步拒绝零写入"
    );
    let _ = fs::remove_dir_all(&state);
}

#[test]
fn admit_rejects_expired_decision_with_or2108() {
    let state = scratch("expired");
    let order_id = "wo-admit-expired";
    seed_released_order(&state, order_id);
    let order = {
        let records = OrderLedger::load(
            &StateDir::new(&state)
                .order_dir(order_id)
                .join("ledger.jsonl"),
        )
        .unwrap();
        records
            .iter()
            .rev()
            .find_map(|record| record.order_after.clone())
            .unwrap()
    };
    let decision = seal_admission_decision(
        &order,
        1,
        vec!["artifact:fa-1:qualified".into()],
        "2026-11-02T00:00:00Z", // 早于 validated_at
        &SEED_TRUSTED,
    );
    let request = parse(&request_json(order_id, &decision, &registry()));
    let err = af_order::admit::admit(
        &request,
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_err();
    assert_admit_err(err, "OR2108");
    assert!(
        !admissions_path(&StateDir::new(&state), order_id).exists(),
        "第④步拒绝零写入"
    );
    let _ = fs::remove_dir_all(&state);
}

#[test]
fn admit_rejects_schema_violations_with_typed_codes() {
    let state = scratch("schema");
    let order_id = "wo-admit-schema";
    seed_released_order(&state, order_id);
    let order = {
        let records = OrderLedger::load(
            &StateDir::new(&state)
                .order_dir(order_id)
                .join("ledger.jsonl"),
        )
        .unwrap();
        records
            .iter()
            .rev()
            .find_map(|record| record.order_after.clone())
            .unwrap()
    };
    let decision = seal_admission_decision(
        &order,
        1,
        vec!["artifact:fa-1:qualified".into()],
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    let mut value: Value =
        serde_json::from_str(&request_json(order_id, &decision, &registry())).unwrap();

    // 模板 doc_kind 越集 → OR2103。
    value["template"]["doc_kind"] = json!("派单");
    let err = af_order::admit::admit(
        &parse(&value.to_string()),
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_err();
    assert_admit_err(err, "OR2103");

    // 资源画像 units=0 → OR2104。
    value["template"]["doc_kind"] = json!("裁决单");
    value["resource_profile"]["claims"][0]["units"] = json!(0);
    let err = af_order::admit::admit(
        &parse(&value.to_string()),
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_err();
    assert_admit_err(err, "OR2104");

    // 门清单重复 gate_id → OR2105。
    value["resource_profile"]["claims"][0]["units"] = json!(1);
    value["gate_list"]["gates"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "gate_id": "F1", "kind": "fast", "required": false }));
    let err = af_order::admit::admit(
        &parse(&value.to_string()),
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_err();
    assert_admit_err(err, "OR2105");

    assert!(
        !admissions_path(&StateDir::new(&state), order_id).exists(),
        "第②步拒绝零写入"
    );
    let _ = fs::remove_dir_all(&state);
}

/// 查无源单 = OR2101（状态面拒绝，零写入）。
#[test]
fn admit_rejects_missing_source_with_or2101() {
    let state = scratch("nosource");
    let order_id = "wo-admit-absent";
    let order = WorkOrder::new(
        order_id,
        GraphVersion::new("af-order/poc-1"),
        Sla::new(None, Priority::Normal),
    );
    let decision = seal_admission_decision(
        &order,
        1,
        vec!["artifact:fa-1:qualified".into()],
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    let request = parse(&request_json(order_id, &decision, &registry()));
    let err = af_order::admit::admit(
        &request,
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_err();
    assert_admit_err(err, "OR2101");
    assert!(
        !admissions_path(&StateDir::new(&state), order_id).exists(),
        "第①步拒绝零写入"
    );
    let _ = fs::remove_dir_all(&state);
}
