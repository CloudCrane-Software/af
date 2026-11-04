//! PoC-A2 admit 集成验收：真引擎子进程走 admit_decision 四层真验签路径 +
//! receipt 幂等窗两条负例（同键重放 / 同 seq 异摘要）+ 各步类型化拒绝。
//! PoC-C 增补：OR2109/2110/2111 与 seq 单调门的细则负例（空 basis / 伪造
//! basis / digest 不一致 / 凭证交集空 / seq 回退 五类必红）与正例。
//!
//! 与 tests/cli.rs 同款纪律：不用 mock 引擎——`EngineClient::locate` 定位/
//! 构建真 af-engine（CARGO_TARGET_DIR 经环境继承，与门面解析同优先级）。
//! 注：占位期 fixture（如 `artifact:fa-1:qualified`、占位 digest）只在恒过
//! stub 下成立；PoC-C 口径落地后请求输入面（basis/计划面/凭证面）一律按
//! admit docstring 口径取源单真实实体与现算摘要——断言面零改动。

use std::fs;
use std::path::{Path, PathBuf};

use af_execline::{EngineClient, LedgerRecord, OrderLedger};
use af_order::admit::{
    admissions_path, canonical_next_orders_digest, decision_digest, idempotency_key,
    AdmissionRequest,
};
use af_order::{parse_request, OrderError, StateDir};
use af_workorder::{
    Decision, DecisionArtifact, DecisionAttributes, Ed25519Keypair, GraphVersion, LineBinding,
    Permission, Priority, Role, Sla, TrustedActor, TrustedActorRegistry, WorkArtifact, WorkOrder,
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

/// 源单实际实体解析的决策依据（工件真实摘要 + 凭证 canonical 摘要，
/// 与生成侧 adjudicate::resolved_basis 同源，admit OR2109 可独立重算）。
fn valid_basis(order: &WorkOrder) -> Vec<String> {
    af_order::adjudicate::resolved_basis(order)
}

/// 计划面缺省：一个后继 rework 单（digest 现算进被签字节）。
fn default_next_orders(order_id: &str) -> Vec<String> {
    vec![format!("{order_id}-rw")]
}

/// 源单持有预算凭证 id（凭证求交的非空交保证）。
fn held_credential(order: &WorkOrder) -> String {
    order
        .budget_voucher()
        .map(|voucher| voucher.voucher_id.clone())
        .unwrap_or_else(|| "bv-absent".into())
}

/// 按 admission payload 真签一份决策工件（digest 由计划面同一规范化现算）。
fn seal_admission_decision(
    order: &WorkOrder,
    seq: u64,
    basis: Vec<String>,
    next_orders: &[String],
    decision_expiry: &str,
    seed: &[u8; 32],
) -> DecisionArtifact {
    let digest = canonical_next_orders_digest(next_orders).expect("测试计划面应可规范化");
    DecisionArtifact::seal(
        "user-a",
        Role::Approver,
        Decision::Approve,
        &order.admission_decision_payload(),
        &Ed25519Keypair::from_seed(seed),
        DecisionAttributes::new(seq, basis, digest, time(decision_expiry)),
    )
}

/// 组一份 admission 请求 JSON（走 parse_request 解析，连解析面一起验收）。
/// next_orders / required_credentials 是 PoC-C 落地的请求字段（计划面与
/// 凭证面），须与 decision 的被签 digest / 源单持有凭证面一致才能过链。
fn request_json(
    order: &WorkOrder,
    decision: &DecisionArtifact,
    registry: &TrustedActorRegistry,
    next_orders: &[String],
    required_credentials: &[String],
) -> String {
    json!({
        "order_id": order.id(),
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
        "next_orders": next_orders,
        "required_credentials": required_credentials,
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
    let next_orders = default_next_orders(order_id);
    let decision = seal_admission_decision(
        &order,
        7,
        valid_basis(&order),
        &next_orders,
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    let request = parse(&request_json(
        &order,
        &decision,
        &registry(),
        &next_orders,
        &[held_credential(&order)],
    ));

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
    // 该负例在幂等窗（先于第②步起的所有校验）被拒——basis 无需可解析。
    let redecided = seal_admission_decision(
        &order,
        7,
        vec!["artifact:fa-2:qualified".into()],
        &next_orders,
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    assert_ne!(
        decision_digest(&redecided),
        decision_digest(&decision),
        "测试前提：异摘要"
    );
    let conflicting = parse(&request_json(
        &order,
        &redecided,
        &registry(),
        &next_orders,
        &[held_credential(&order)],
    ));
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
    let next_orders = default_next_orders(order_id);
    let decision = seal_admission_decision(
        &order,
        1,
        valid_basis(&order),
        &next_orders,
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    let request = parse(&request_json(
        &order,
        &decision,
        &registry(),
        &next_orders,
        &[held_credential(&order)],
    ));
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
    let next_orders = default_next_orders(order_id);
    let decision = seal_admission_decision(
        &order,
        3,
        valid_basis(&order),
        &next_orders,
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    let request = parse(&request_json(
        &order,
        &decision,
        &registry(),
        &next_orders,
        &[held_credential(&order)],
    ));
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
    let next_orders = default_next_orders(order_id);
    let decision = seal_admission_decision(
        &order,
        1,
        valid_basis(&order),
        &next_orders,
        DECISION_EXPIRY,
        &[0x42; 32],
    );
    let request = parse(&request_json(
        &order,
        &decision,
        &registry(),
        &next_orders,
        &[held_credential(&order)],
    ));
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
    let next_orders = default_next_orders(order_id);
    let decision = seal_admission_decision(
        &order,
        1,
        valid_basis(&order),
        &next_orders,
        "2026-11-02T00:00:00Z", // 早于 validated_at
        &SEED_TRUSTED,
    );
    let request = parse(&request_json(
        &order,
        &decision,
        &registry(),
        &next_orders,
        &[held_credential(&order)],
    ));
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
    let next_orders = default_next_orders(order_id);
    let decision = seal_admission_decision(
        &order,
        1,
        valid_basis(&order),
        &next_orders,
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    let mut value: Value = serde_json::from_str(&request_json(
        &order,
        &decision,
        &registry(),
        &next_orders,
        &[held_credential(&order)],
    ))
    .unwrap();

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
    let next_orders = default_next_orders(order_id);
    let decision = seal_admission_decision(
        &order,
        1,
        valid_basis(&order),
        &next_orders,
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    let request = parse(&request_json(
        &order,
        &decision,
        &registry(),
        &next_orders,
        &[held_credential(&order)],
    ));
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

// ---------------------------------------------------------------------------
// PoC-C 决策校验链细则（OR2109/2110/2111 + seq 单调门）——五类必红 + 正例
// ---------------------------------------------------------------------------

/// 负例①：空 basis → OR2109（没有依据的决策不受理）。
#[test]
fn admit_rejects_empty_basis_with_or2109() {
    let state = scratch("empty-basis");
    let order_id = "wo-admit-eb";
    seed_terminal_order(&state, order_id, false);
    let order = latest_order(&state, order_id);
    let next_orders = default_next_orders(order_id);
    let decision = seal_admission_decision(
        &order,
        1,
        Vec::new(),
        &next_orders,
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    let request = parse(&request_json(
        &order,
        &decision,
        &registry(),
        &next_orders,
        &[held_credential(&order)],
    ));
    let err = af_order::admit::admit(
        &request,
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_err();
    assert_admit_err(err, "OR2109");
    let _ = fs::remove_dir_all(&state);
}

/// 负例②：伪造 basis——指向不存在的实体 / 真实体换摘要 → 都 OR2109。
#[test]
fn admit_rejects_forged_basis_with_or2109() {
    let state = scratch("forged-basis");
    let order_id = "wo-admit-fb";
    seed_terminal_order(&state, order_id, false);
    let order = latest_order(&state, order_id);
    let next_orders = default_next_orders(order_id);

    // (a) 实体不存在：源单没有 fa-nonexistent 工件。
    let forged_entity = vec![format!("artifact:fa-nonexistent:{}", "a".repeat(64))];
    let decision = seal_admission_decision(
        &order,
        1,
        forged_entity,
        &next_orders,
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    let request = parse(&request_json(
        &order,
        &decision,
        &registry(),
        &next_orders,
        &[held_credential(&order)],
    ));
    let err = af_order::admit::admit(
        &request,
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_err();
    assert_admit_err(err, "OR2109");

    // (b) 实体存在但摘要不符：真工件 id + 换掉的 digest。
    let real_id = order.artifacts()[0].artifact_id.clone();
    let forged_digest = vec![format!("artifact:{real_id}:{}", "b".repeat(64))];
    let decision2 = seal_admission_decision(
        &order,
        1,
        forged_digest,
        &next_orders,
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    let request2 = parse(&request_json(
        &order,
        &decision2,
        &registry(),
        &next_orders,
        &[held_credential(&order)],
    ));
    let err2 = af_order::admit::admit(
        &request2,
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_err();
    assert_admit_err(err2, "OR2109");
    let _ = fs::remove_dir_all(&state);
}

/// 负例③：digest 不一致——签的是 A 计划面，提交的是 B 计划面 → OR2110。
#[test]
fn admit_rejects_next_orders_digest_mismatch_with_or2110() {
    let state = scratch("digest");
    let order_id = "wo-admit-dm";
    seed_terminal_order(&state, order_id, false);
    let order = latest_order(&state, order_id);
    let signed_plan = default_next_orders(order_id);
    let submitted_plan = vec![format!("{order_id}-different")];
    let decision = seal_admission_decision(
        &order,
        1,
        valid_basis(&order),
        &signed_plan,
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    let request = parse(&request_json(
        &order,
        &decision,
        &registry(),
        &submitted_plan,
        &[held_credential(&order)],
    ));
    let err = af_order::admit::admit(
        &request,
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_err();
    assert_admit_err(err, "OR2110");
    let _ = fs::remove_dir_all(&state);
}

/// 负例④：凭证交集空——呈报与源单持有面无关的凭证 / 空清单 → OR2111。
#[test]
fn admit_rejects_empty_credential_intersection_with_or2111() {
    let state = scratch("cred");
    let order_id = "wo-admit-cred";
    seed_terminal_order(&state, order_id, false);
    let order = latest_order(&state, order_id);
    let next_orders = default_next_orders(order_id);
    let decision = seal_admission_decision(
        &order,
        1,
        valid_basis(&order),
        &next_orders,
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    // (a) 呈报无关凭证：交集空。
    let unrelated = vec!["bv-some-other-order".to_string()];
    let request = parse(&request_json(
        &order,
        &decision,
        &registry(),
        &next_orders,
        &unrelated,
    ));
    let err = af_order::admit::admit(
        &request,
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_err();
    assert_admit_err(err, "OR2111");
    // (b) 空清单：交集空。
    let request2 = parse(&request_json(
        &order,
        &decision,
        &registry(),
        &next_orders,
        &[],
    ));
    let err2 = af_order::admit::admit(
        &request2,
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_err();
    assert_admit_err(err2, "OR2111");
    let _ = fs::remove_dir_all(&state);
}

/// 负例⑤：seq 回退——先受理 seq=7，再提交合法但更小的 seq=5 → OR2107
/// 单调门（新 seq 必须 > 本源单已见最大 seq）。
#[test]
fn admit_rejects_seq_regression_with_or2107() {
    let state = scratch("seq-reg");
    let order_id = "wo-admit-seq";
    seed_terminal_order(&state, order_id, false);
    let order = latest_order(&state, order_id);
    let client = engine_client();
    let next_orders = default_next_orders(order_id);
    let first = seal_admission_decision(
        &order,
        7,
        valid_basis(&order),
        &next_orders,
        DECISION_EXPIRY,
        &SEED_TRUSTED,
    );
    af_order::admit::admit(
        &parse(&request_json(
            &order,
            &first,
            &registry(),
            &next_orders,
            &[held_credential(&order)],
        )),
        &StateDir::new(&state),
        &client,
        time(NOW),
    )
    .unwrap_or_else(|err| panic!("前置受理 seq=7 应过: {err}"));
    // 换 actor/换内容签一份合法 seq=5（不同摘要，非重放）——回退必红。
    let regressed = DecisionArtifact::seal(
        "user-a",
        Role::Approver,
        Decision::Approve,
        &order.admission_decision_payload(),
        &Ed25519Keypair::from_seed(&SEED_TRUSTED),
        {
            let digest = canonical_next_orders_digest(&next_orders).unwrap();
            DecisionAttributes::new(5, valid_basis(&order), digest, time(DECISION_EXPIRY))
        },
    );
    assert_ne!(
        decision_digest(&regressed),
        decision_digest(&first),
        "测试前提：非同键重放"
    );
    let err = af_order::admit::admit(
        &parse(&request_json(
            &order,
            &regressed,
            &registry(),
            &next_orders,
            &[held_credential(&order)],
        )),
        &StateDir::new(&state),
        &client,
        time(NOW),
    )
    .unwrap_err();
    let text = err.to_string();
    assert!(text.contains("OR2107"), "应报 OR2107: {text}");
    assert!(
        text.contains("回退") && text.contains("单调"),
        "拒因应点名 seq 单调回退: {text}"
    );
    let _ = fs::remove_dir_all(&state);
}

/// 正例：seq 单调递增连续受理全过（OR2109/2110/2111 全绿的第二组证据；
/// 首组在 happy/closed-source 两用例）。
#[test]
fn admit_accepts_increasing_seq_across_rechecks() {
    let state = scratch("seq-inc");
    let order_id = "wo-admit-inc";
    seed_terminal_order(&state, order_id, false);
    let order = latest_order(&state, order_id);
    let client = engine_client();
    let next_orders = default_next_orders(order_id);
    for seq in [1u64, 2] {
        let decision = seal_admission_decision(
            &order,
            seq,
            valid_basis(&order),
            &next_orders,
            DECISION_EXPIRY,
            &SEED_TRUSTED,
        );
        let receipt = af_order::admit::admit(
            &parse(&request_json(
                &order,
                &decision,
                &registry(),
                &next_orders,
                &[held_credential(&order)],
            )),
            &StateDir::new(&state),
            &client,
            time(NOW),
        )
        .unwrap_or_else(|err| panic!("seq={seq} 单调受理应过: {err}"));
        assert_eq!(receipt.decision_seq, seq);
        assert!(!receipt.replayed);
        assert!(receipt.checks.contains(&"basis_recheck".to_string()));
        assert!(receipt.checks.contains(&"digest_recheck".to_string()));
        assert!(receipt
            .checks
            .contains(&"credential_intersection".to_string()));
    }
    let _ = fs::remove_dir_all(&state);
}

/// 最近快照读回（各负例共用；与 admit 链第①步同口径）。
fn latest_order(state: &Path, order_id: &str) -> WorkOrder {
    OrderLedger::load(&StateDir::new(state).order_dir(order_id).join("ledger.jsonl"))
        .unwrap()
        .iter()
        .rev()
        .find_map(|record| record.order_after.clone())
        .unwrap()
}

// ---------------------------------------------------------------------------
// --decision/--role 面（admit_seal 补参后的受理链验收）：Reject / Revise /
// Arbiter 三路径正例与负例。负例一律断言零写入（admissions.jsonl 不落盘）。
// 配对判据 = af_workorder 既有权限面（Role::can_decide 与四层校验第③层
// 同源），受理侧不因工具面收窄而放宽。
// ---------------------------------------------------------------------------

/// 指定角色的受信注册表（actor 角色与工件角色一致——admit_seal 工具侧
/// --role 同款组法：注册表与工件同取所选角色）。
fn registry_with_role(role: Role) -> TrustedActorRegistry {
    TrustedActorRegistry::new(vec![TrustedActor::new(
        "user-a",
        role,
        &Ed25519Keypair::from_seed(&SEED_TRUSTED).public_key(),
        time(KEY_EXPIRY),
    )])
    .unwrap()
}

/// 按指定 role × decision 配对真签（admit_seal --role/--decision 同款面：
/// permissions 取角色默认集、payload 绑定 admission_decision_payload、
/// digest 由计划面同一规范化现算，与工具侧 DecisionArtifact::seal 同口径）。
fn seal_decision_as(
    order: &WorkOrder,
    role: Role,
    decision: Decision,
    seq: u64,
    next_orders: &[String],
) -> DecisionArtifact {
    let digest = canonical_next_orders_digest(next_orders).expect("测试计划面应可规范化");
    DecisionArtifact::seal(
        "user-a",
        role,
        decision,
        &order.admission_decision_payload(),
        &Ed25519Keypair::from_seed(&SEED_TRUSTED),
        DecisionAttributes::new(seq, valid_basis(order), digest, time(DECISION_EXPIRY)),
    )
}

/// Reject 正例：Approver 真签 Reject（Reject 与 Approve 同需 Approve 权），
/// admit 全链真验签受理，工件面 role/decision/permissions 与所选一致，
/// receipt.decision 如实记 "Reject"。
#[test]
fn admit_accepts_approver_signed_reject_and_records_decision() {
    let state = scratch("reject-pos");
    let order_id = "wo-admit-rej-pos";
    seed_released_order(&state, order_id);
    let order = latest_order(&state, order_id);
    let next_orders = default_next_orders(order_id);
    let decision = seal_decision_as(&order, Role::Approver, Decision::Reject, 1, &next_orders);
    assert_eq!(decision.role, Role::Approver);
    assert_eq!(decision.decision, Decision::Reject);
    assert_eq!(
        decision.permissions,
        vec![Permission::Read, Permission::Approve]
    );
    let request = parse(&request_json(
        &order,
        &decision,
        &registry_with_role(Role::Approver),
        &next_orders,
        &[held_credential(&order)],
    ));
    let receipt = af_order::admit::admit(
        &request,
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_or_else(|err| panic!("Approver×Reject 应全链受理: {err}"));
    assert_eq!(receipt.decision, "Reject");
    assert_eq!(receipt.actor_id, "user-a");
    assert!(!receipt.replayed);
    assert!(receipt.checks.contains(&"decision_four_layer".to_string()));
    let admissions = fs::read_to_string(admissions_path(&StateDir::new(&state), order_id))
        .expect("admissions.jsonl 应已落盘");
    assert_eq!(admissions.lines().count(), 1, "恰一笔受理");
    let _ = fs::remove_dir_all(&state);
}

/// Reject 负例：Reviser 无 Approve 权——Reviser×Reject 越权工件在四层第③层
/// 被拒（OR2106，拒因点名决策权限不足），状态树零写入。
#[test]
fn admit_rejects_reviser_signed_reject_with_or2106_and_zero_writes() {
    let state = scratch("reject-neg");
    let order_id = "wo-admit-rej-neg";
    seed_released_order(&state, order_id);
    let order = latest_order(&state, order_id);
    let next_orders = default_next_orders(order_id);
    let decision = seal_decision_as(&order, Role::Reviser, Decision::Reject, 1, &next_orders);
    let request = parse(&request_json(
        &order,
        &decision,
        &registry_with_role(Role::Reviser),
        &next_orders,
        &[held_credential(&order)],
    ));
    let err = af_order::admit::admit(
        &request,
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_err();
    let text = err.to_string();
    assert_admit_err(err, "OR2106");
    assert!(
        text.contains("决策权限不足"),
        "拒因应点名决策权限不足: {text}"
    );
    assert!(
        !admissions_path(&StateDir::new(&state), order_id).exists(),
        "第③步拒绝零写入"
    );
    let _ = fs::remove_dir_all(&state);
}

/// Revise 正例：Reviser 真签 Revise（Revise 权在 Reviser 权限面内），
/// admit 全链受理，receipt.decision 如实记 "Revise"，permissions=[Read, Revise]。
#[test]
fn admit_accepts_reviser_signed_revise_and_records_decision() {
    let state = scratch("revise-pos");
    let order_id = "wo-admit-rvs-pos";
    seed_released_order(&state, order_id);
    let order = latest_order(&state, order_id);
    let next_orders = default_next_orders(order_id);
    let decision = seal_decision_as(&order, Role::Reviser, Decision::Revise, 1, &next_orders);
    assert_eq!(decision.role, Role::Reviser);
    assert_eq!(decision.decision, Decision::Revise);
    assert_eq!(
        decision.permissions,
        vec![Permission::Read, Permission::Revise]
    );
    let request = parse(&request_json(
        &order,
        &decision,
        &registry_with_role(Role::Reviser),
        &next_orders,
        &[held_credential(&order)],
    ));
    let receipt = af_order::admit::admit(
        &request,
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_or_else(|err| panic!("Reviser×Revise 应全链受理: {err}"));
    assert_eq!(receipt.decision, "Revise");
    assert!(!receipt.replayed);
    assert!(receipt.checks.contains(&"decision_four_layer".to_string()));
    let admissions = fs::read_to_string(admissions_path(&StateDir::new(&state), order_id))
        .expect("admissions.jsonl 应已落盘");
    assert_eq!(admissions.lines().count(), 1, "恰一笔受理");
    let _ = fs::remove_dir_all(&state);
}

/// Revise 负例：Approver 无 Revise 权——Approver×Revise 越权工件在四层第③层
/// 被拒（OR2106），状态树零写入。
#[test]
fn admit_rejects_approver_signed_revise_with_or2106_and_zero_writes() {
    let state = scratch("revise-neg");
    let order_id = "wo-admit-rvs-neg";
    seed_released_order(&state, order_id);
    let order = latest_order(&state, order_id);
    let next_orders = default_next_orders(order_id);
    let decision = seal_decision_as(&order, Role::Approver, Decision::Revise, 1, &next_orders);
    let request = parse(&request_json(
        &order,
        &decision,
        &registry_with_role(Role::Approver),
        &next_orders,
        &[held_credential(&order)],
    ));
    let err = af_order::admit::admit(
        &request,
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_err();
    let text = err.to_string();
    assert_admit_err(err, "OR2106");
    assert!(
        text.contains("决策权限不足"),
        "拒因应点名决策权限不足: {text}"
    );
    assert!(
        !admissions_path(&StateDir::new(&state), order_id).exists(),
        "第③步拒绝零写入"
    );
    let _ = fs::remove_dir_all(&state);
}

/// Arbiter 正例：Arbiter 全权——同一源单连续受理 Arbiter 真签的 Reject 与
/// Revise（seq 单调 1→2），两笔回执 decision 分别如实记 "Reject"/"Revise"，
/// 工件 permissions = 角色全量默认集 [Read, Approve, Revise, Arbitrate]。
#[test]
fn admit_accepts_arbiter_signed_reject_then_revise_with_monotonic_seq() {
    let state = scratch("arbiter-pos");
    let order_id = "wo-admit-arb-pos";
    seed_released_order(&state, order_id);
    let order = latest_order(&state, order_id);
    let client = engine_client();
    let next_orders = default_next_orders(order_id);
    let registry = registry_with_role(Role::Arbiter);
    for (seq, kind) in [(1u64, Decision::Reject), (2u64, Decision::Revise)] {
        let decision = seal_decision_as(&order, Role::Arbiter, kind, seq, &next_orders);
        assert_eq!(decision.role, Role::Arbiter);
        assert_eq!(decision.decision, kind);
        assert_eq!(
            decision.permissions,
            vec![
                Permission::Read,
                Permission::Approve,
                Permission::Revise,
                Permission::Arbitrate,
            ]
        );
        let request = parse(&request_json(
            &order,
            &decision,
            &registry,
            &next_orders,
            &[held_credential(&order)],
        ));
        let receipt = af_order::admit::admit(&request, &StateDir::new(&state), &client, time(NOW))
            .unwrap_or_else(|err| panic!("Arbiter seq={seq} {kind:?} 应全链受理: {err}"));
        assert_eq!(
            receipt.decision,
            serde_json::to_value(kind)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            "回执 decision 应与所选一致"
        );
        assert_eq!(receipt.decision_seq, seq);
        assert!(!receipt.replayed);
    }
    let admissions = fs::read_to_string(admissions_path(&StateDir::new(&state), order_id))
        .expect("admissions.jsonl 应已落盘");
    assert_eq!(admissions.lines().count(), 2, "两笔受理（seq 单调）");
    let _ = fs::remove_dir_all(&state);
}

/// Arbiter 负例：工件声称 Arbiter 但注册表只登记 Approver——签名虽真，
/// 角色冒升在四层第④层被拒（OR2106，拒因点名角色越权），状态树零写入。
/// （工具面 admit_seal 的注册表与工件同取 --role，不产此形态；此负例钉住
/// 受理侧对"自带角色提权工件"的兜底。）
#[test]
fn admit_rejects_claimed_arbiter_not_backed_by_registry_with_or2106_and_zero_writes() {
    let state = scratch("arbiter-neg");
    let order_id = "wo-admit-arb-neg";
    seed_released_order(&state, order_id);
    let order = latest_order(&state, order_id);
    let next_orders = default_next_orders(order_id);
    let decision = seal_decision_as(&order, Role::Arbiter, Decision::Reject, 1, &next_orders);
    // 注册表登记的是 Approver（与工件声称的 Arbiter 不符）。
    let request = parse(&request_json(
        &order,
        &decision,
        &registry_with_role(Role::Approver),
        &next_orders,
        &[held_credential(&order)],
    ));
    let err = af_order::admit::admit(
        &request,
        &StateDir::new(&state),
        &engine_client(),
        time(NOW),
    )
    .unwrap_err();
    let text = err.to_string();
    assert_admit_err(err, "OR2106");
    assert!(
        text.contains("角色越权"),
        "拒因应点名角色越权（注册表 vs 工件声称）: {text}"
    );
    assert!(
        !admissions_path(&StateDir::new(&state), order_id).exists(),
        "第③步拒绝零写入"
    );
    let _ = fs::remove_dir_all(&state);
}
