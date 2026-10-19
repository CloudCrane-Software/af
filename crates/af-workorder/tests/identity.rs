//! 稳定身份、分片与引擎 run 的对外行为。
//!
//! 时刻与熵都不读系统:身份由订单 id 或调用方带回的 stable_id 决定。

use af_workorder::{
    decode_work_order, sha256_hex, Decision, DecisionError, GraphVersion, OrderDecodeError,
    Priority, RejectionKind, Role, RunContext, Sla, StableId, WorkOrder,
};
use chrono::{DateTime, Utc};

fn sla() -> Sla {
    let deadline = DateTime::parse_from_rfc3339("2026-11-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    Sla::new(Some(deadline), Priority::Normal)
}

#[test]
fn same_stable_id_keeps_identity_across_runs() {
    let mut first = WorkOrder::new("wo-1", GraphVersion::new("g-3"), sla());
    let sid = first.stable_id().clone();
    first.continue_as_new(RunContext::new("run-1"));

    let second = WorkOrder::with_stable_id(
        "wo-1",
        GraphVersion::new("g-3"),
        sla(),
        sid.clone(),
        RunContext::new("run-2"),
    );
    assert!(first.same_identity(&second));
    assert_eq!(first.stable_id(), second.stable_id());
    assert_eq!(first.stable_id(), &sid);
    assert_ne!(first.run_id(), second.run_id());
    assert_eq!(second.run_id(), "run-2");

    let json = serde_json::to_string(&second).unwrap();
    let restored: WorkOrder = serde_json::from_str(&json).unwrap();
    assert!(first.same_identity(&restored));
    assert_eq!(restored.run_id(), "run-2");
    assert_eq!(restored.stable_id().as_str(), sid.as_str());
}

#[test]
fn shard_key_is_deterministic_for_a_stable_id() {
    let sid = StableId::parse("sid-fixed").unwrap();
    let run_a = WorkOrder::with_stable_id(
        "wo-1",
        GraphVersion::new("g-3"),
        sla(),
        sid.clone(),
        RunContext::new("run-a"),
    );
    let mut run_b = WorkOrder::with_stable_id(
        "wo-1",
        GraphVersion::new("g-9"),
        sla(),
        sid.clone(),
        RunContext::new("run-b"),
    );
    assert_eq!(run_a.shard_key(64).unwrap(), 32);
    assert_eq!(run_b.shard_key(64).unwrap(), run_a.shard_key(64).unwrap());
    assert_eq!(sid.shard_key(100).unwrap(), 36);
    run_b.continue_as_new(RunContext::new("run-c"));
    assert_eq!(run_b.shard_key(64).unwrap(), 32);
    assert_eq!(run_b.shard_key(1).unwrap(), 0);
    assert!(run_a.shard_key(0).is_err());

    let other = StableId::parse("sid-other").unwrap();
    assert_eq!(other.shard_key(64).unwrap(), 11);
    assert_ne!(other.shard_key(64).unwrap(), sid.shard_key(64).unwrap());
}

#[test]
fn legacy_payload_without_stable_id_is_rejected() {
    let legacy = r#"{
        "id": "wo-legacy",
        "state": "Created",
        "graph_version": "g-3",
        "line": null,
        "budget_voucher": null,
        "artifacts": [],
        "sla": {"deadline": "2026-10-30T00:00:00Z", "priority": "Normal"},
        "supersedes": null,
        "history": []
    }"#;
    let err = decode_work_order(legacy).unwrap_err();
    let OrderDecodeError::Rejected(envelope) = err else {
        panic!("老载荷应被拒绝式信封挡住,不能补一个 stable_id");
    };
    assert_eq!(envelope.kind, RejectionKind::MissingField);
    assert_eq!(envelope.field, "stable_id");
    assert!(envelope.detail.contains("stable_id"));
    let rendered = serde_json::to_string(&envelope).unwrap();
    assert!(rendered.contains("\"field\":\"stable_id\""), "{rendered}");
    assert!(rendered.contains("missing_field"), "{rendered}");

    let fresh = WorkOrder::with_stable_id(
        "wo-1",
        GraphVersion::new("g-3"),
        sla(),
        StableId::parse("sid-fixed").unwrap(),
        RunContext::new("run-1"),
    );
    let mut value = serde_json::to_value(&fresh).unwrap();
    value.as_object_mut().unwrap().remove("run_id");
    let err = decode_work_order(&value.to_string()).unwrap_err();
    let OrderDecodeError::Rejected(envelope) = err else {
        panic!("缺 run_id 也应点名该字段");
    };
    assert_eq!(envelope.field, "run_id");
    assert_eq!(envelope.kind, RejectionKind::MissingField);

    {
        let obj = value.as_object_mut().unwrap();
        obj.insert("run_id".into(), serde_json::json!("run-1"));
        obj.insert("stable_id".into(), serde_json::json!(""));
    }
    let err = decode_work_order(&value.to_string()).unwrap_err();
    let OrderDecodeError::Rejected(envelope) = err else {
        panic!("空 stable_id 应拒绝");
    };
    assert_eq!(envelope.field, "stable_id");
    assert_eq!(envelope.kind, RejectionKind::BlankField);

    let restored = decode_work_order(&serde_json::to_string(&fresh).unwrap()).unwrap();
    assert_eq!(restored, fresh);
}

#[test]
fn decision_artifact_hash_includes_stable_id() {
    let mut order = WorkOrder::with_stable_id(
        "wo-1",
        GraphVersion::new("g-3"),
        sla(),
        StableId::parse("sid-fixed").unwrap(),
        RunContext::new("run-CAN-9f3e"),
    );
    let payload = order.decision_payload("release-request");
    assert_eq!(
        payload,
        "af-workorder/decision-payload/v1|9|sid-fixed|15|release-request"
    );
    assert!(payload.contains("sid-fixed"));
    assert!(!payload.contains("run-CAN-9f3e"));
    let expected = "18d638628bd06103e7138ac40e673dee658d184f65965834b3a96c60e519754c";
    assert_eq!(sha256_hex(payload.as_bytes()), expected);

    let artifact = order.seal_decision(
        "user-a",
        Role::Approver,
        Decision::Approve,
        "release-request",
        "k2",
    );
    assert_eq!(artifact.payload_hash, expected);
    artifact.validate_against(&payload).unwrap();

    let stripped = "af-workorder/decision-payload/v1|0||15|release-request";
    assert!(matches!(
        artifact.validate_against(stripped),
        Err(DecisionError::PayloadHashMismatch { .. })
    ));

    order.continue_as_new(RunContext::new("run-other"));
    let again = order.seal_decision(
        "user-a",
        Role::Approver,
        Decision::Approve,
        "release-request",
        "k2",
    );
    assert_eq!(again.payload_hash, expected);
    assert_eq!(order.decision_payload("release-request"), payload);

    let other = WorkOrder::with_stable_id(
        "wo-1",
        GraphVersion::new("g-3"),
        sla(),
        StableId::parse("sid-other").unwrap(),
        RunContext::new("run-CAN-9f3e"),
    );
    assert_ne!(other.decision_payload("release-request"), payload);
    assert_ne!(
        other
            .seal_decision(
                "user-a",
                Role::Approver,
                Decision::Approve,
                "release-request",
                "k2",
            )
            .payload_hash,
        expected
    );
}
