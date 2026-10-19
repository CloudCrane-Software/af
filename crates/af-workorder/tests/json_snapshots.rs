//! DecisionArtifact 与 SignedChangeOrder 的 JSON 快照。
//!
//! 快照文件在 `tests/snapshots/*.snap`，直接入库。生成时用 `INSTA_UPDATE=1`，
//! 不走 `cargo insta review`。普通 `cargo test` 只比对，不改快照。
//!
//! 签名字节、key_id、payload_hash 在写入前涂红。变更单的 ed25519 在注入种子下
//! 字节确定，但这些字段是密钥材料或哈希，不进快照；涂红后的文本二次运行无 diff。

use std::fs;
use std::path::Path;

use af_workorder::{
    sha256_hex, BudgetVoucher, ChangeOrder, Decision, DecisionArtifact, DrainStrategy,
    Ed25519Keypair, GraphVersion, LineBinding, Missing, OrderState, Priority, RelayStrategy, Role,
    SignedChangeOrder, Sla, WorkArtifact, WorkOrder, ED25519_SIGNATURE_LEN, SHA256_HEX_LEN,
    SIGNATURE_ALGORITHM_ED25519,
};
use chrono::{DateTime, Utc};
use serde::Serialize;

const CHANGE_SEED: [u8; 32] = [0x7a; 32];

fn utc(rfc3339: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(rfc3339)
        .unwrap()
        .with_timezone(&Utc)
}

fn sla() -> Sla {
    Sla::new(Some(utc("2026-11-01T00:00:00Z")), Priority::High)
}

/// 走到 Completed，尚未挂工件。截止时刻是注入的常量，不读系统钟。
fn drive_to_completed(id: &str) -> WorkOrder {
    let mut order = WorkOrder::new(id, GraphVersion::new("g-3"), sla());
    order
        .bind_line(LineBinding {
            line_id: "line-A".into(),
            line_version: "lv-7".into(),
        })
        .unwrap();
    order
        .attach_budget_voucher(BudgetVoucher {
            voucher_id: "bv-1".into(),
            amount: 500,
            signed: true,
        })
        .unwrap();
    assert_eq!(order.release().unwrap(), OrderState::Released);
    assert_eq!(order.start().unwrap(), OrderState::InProgress);
    assert_eq!(order.complete().unwrap(), OrderState::Completed);
    order
}

/// 决策工件的 serde 字段摊平在根上，后面接上门控结果。
/// 这样 `.signature` / `.payload_hash` 的涂红选择器落在根上。
#[derive(Serialize)]
struct DecisionView<'a> {
    #[serde(flatten)]
    artifact: &'a DecisionArtifact,
    outcome: &'static str,
    qualified_final_artifact: Option<&'a WorkArtifact>,
    missing: &'a [Missing],
    detail: &'a [String],
    gate_message: &'a str,
}

fn snapshot_text(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(format!("json_snapshots__{name}.snap"));
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("读取快照 {path:?}: {err}"))
}

/// 快照里必须是涂红占位，且不得残留密钥材料或哈希原文。
fn assert_redacted(name: &str, secrets: &[&str], expect_payload_hash: bool) {
    let text = snapshot_text(name);
    assert!(
        text.contains("\"[signature]\""),
        "快照 {name} 应把签名涂成 [signature]"
    );
    if expect_payload_hash {
        assert!(
            text.contains("\"[payload_hash]\""),
            "快照 {name} 应把 payload_hash 涂成 [payload_hash]"
        );
    }
    for secret in secrets {
        assert!(!secret.is_empty(), "用来检查涂红的原文不能是空串");
        assert!(!text.contains(secret), "快照 {name} 仍含未涂红字段");
    }
}

#[test]
fn qualified_final_artifact_release_decision() {
    let mut order = drive_to_completed("wo-close");
    let piece = WorkArtifact {
        artifact_id: "fa-1".into(),
        is_final: true,
        qualified: true,
    };
    order.attach_artifact(piece.clone()).unwrap();
    assert_eq!(order.close().unwrap(), OrderState::Closed);

    let payload = "workorder:wo-close:graph:g-3:artifact:fa-1:final:qualified:decision:approve";
    let decision = DecisionArtifact::seal(
        "user-approver",
        Role::Approver,
        Decision::Approve,
        payload,
        "k-close-approve",
    );
    decision.validate_against(payload).unwrap();
    assert_eq!(decision.decision, Decision::Approve);
    assert_eq!(decision.payload_hash.len(), SHA256_HEX_LEN);
    assert_eq!(decision.payload_hash, sha256_hex(payload.as_bytes()));
    assert_eq!(decision.signature.algorithm, "ed25519-placeholder");
    assert_eq!(decision.signature.bytes.len(), ED25519_SIGNATURE_LEN);
    assert!(decision.signature.bytes.iter().all(|byte| *byte == 0xde));

    let view = DecisionView {
        artifact: &decision,
        outcome: "closed",
        qualified_final_artifact: Some(&piece),
        missing: &[],
        detail: &[],
        gate_message: "close 放行",
    };
    insta::assert_json_snapshot!(
        "qualified_final_artifact_release_decision",
        view,
        {
            ".signature" => "[signature]",
            ".payload_hash" => "[payload_hash]",
        }
    );
    // 占位签名字节是 0xde，未涂红时 JSON 数字为 222。
    assert_redacted(
        "qualified_final_artifact_release_decision",
        &[&decision.payload_hash, "k-close-approve", "222"],
        true,
    );
}

#[test]
fn reject_decision_names_missing_qualified_final_artifact() {
    let mut order = drive_to_completed("wo-close");
    // 终态不合格、合格但非终态，都不构成 close 放行条件。
    order
        .attach_artifact(WorkArtifact {
            artifact_id: "fa-bad".into(),
            is_final: true,
            qualified: false,
        })
        .unwrap();
    order
        .attach_artifact(WorkArtifact {
            artifact_id: "ia-1".into(),
            is_final: false,
            qualified: true,
        })
        .unwrap();
    let err = order.close().unwrap_err();
    let missing = err.missing().expect("门控拒绝应带缺失清单");
    assert_eq!(missing, &[Missing::NoQualifiedFinalArtifact]);
    let detail: Vec<String> = missing.iter().map(ToString::to_string).collect();
    assert_eq!(detail, ["缺少合格的终态工件"]);
    let gate_message = err.to_string();
    assert!(
        gate_message.contains("NoQualifiedFinalArtifact"),
        "错误应点名缺的前置:{gate_message}"
    );
    assert!(
        gate_message.contains("Close"),
        "错误应点名被拒绝的转换:{gate_message}"
    );

    let payload = "workorder:wo-close:graph:g-3:missing:NoQualifiedFinalArtifact:decision:reject";
    let decision = DecisionArtifact::seal(
        "user-approver",
        Role::Approver,
        Decision::Reject,
        payload,
        "k-close-reject",
    );
    decision.validate_against(payload).unwrap();
    assert_eq!(decision.decision, Decision::Reject);
    assert_eq!(decision.payload_hash, sha256_hex(payload.as_bytes()));
    assert_eq!(decision.signature.bytes.len(), ED25519_SIGNATURE_LEN);

    let view = DecisionView {
        artifact: &decision,
        outcome: "rejected",
        qualified_final_artifact: None,
        missing,
        detail: &detail,
        gate_message: &gate_message,
    };
    insta::assert_json_snapshot!(
        "reject_decision_names_missing_qualified_final_artifact",
        view,
        {
            ".signature" => "[signature]",
            ".payload_hash" => "[payload_hash]",
        }
    );
    // 占位签名字节是 0xde，未涂红时 JSON 数字为 222。
    assert_redacted(
        "reject_decision_names_missing_qualified_final_artifact",
        &[&decision.payload_hash, "k-close-reject", "222"],
        true,
    );
}

#[test]
fn signed_change_order_roundtrip() {
    let order = ChangeOrder {
        change_id: "ch-1".into(),
        work_order_id: "wo-1".into(),
        from_graph_version: GraphVersion::new("g-3"),
        to_graph_version: GraphVersion::new("g-4"),
        diff_summary: "节点 N12 的判定阈值 0.8 → 0.9".into(),
        diff_paths: vec!["graph/N12".into()],
        drain: DrainStrategy::DrainWithCap { max_inflight: 5 },
        relay: RelayStrategy::WarmHandover {
            overlap_budget: 120,
        },
    };
    let key = Ed25519Keypair::from_seed(&CHANGE_SEED);
    let public_key = key.public_key();
    let signed = SignedChangeOrder::sign(order, &key).unwrap();

    let json = serde_json::to_string(&signed).unwrap();
    let restored: SignedChangeOrder = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, signed);
    restored.verify(&public_key).unwrap();

    let successor = restored
        .spawn_successor(&public_key, "wo-1b", sla())
        .unwrap();
    assert_eq!(successor.graph_version().as_str(), "g-4");
    assert_eq!(successor.supersedes(), Some("wo-1"));
    assert_eq!(successor.state(), OrderState::Created);

    assert_eq!(restored.signature.algorithm, SIGNATURE_ALGORITHM_ED25519);
    assert_eq!(restored.signature.key_id, public_key.key_id());
    assert_eq!(restored.signature.key_id.len(), 64);
    assert_eq!(restored.signature.bytes.len(), ED25519_SIGNATURE_LEN);
    let again = SignedChangeOrder::sign(restored.order.clone(), &key).unwrap();
    assert_eq!(
        again.signature.bytes, restored.signature.bytes,
        "同一种子的签名字节必须相同，快照仍不收录这些字节"
    );

    insta::assert_json_snapshot!(
        "signed_change_order_roundtrip",
        restored,
        {
            ".signature" => "[signature]",
        }
    );
    assert_redacted(
        "signed_change_order_roundtrip",
        &[&restored.signature.key_id],
        false,
    );
}
