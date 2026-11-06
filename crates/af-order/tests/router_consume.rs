//! event-router 消费腿集成验收（PoC-D 面 → PoC-C 落点）：一张 af.trigger.v1
//! 信封（Trigger 收件箱 envelope_for 同款口径构造）→ 读侧验证 → 裁决目标
//! 从信封派生（与 state 对账）→ 防重放游标 → 既有 steward 闭环（生成裁决单
//! → 入队 → 认领 → 真签 → admit → complete）→ 游标落账。
//!
//! 判据（对 examples/adjudicate_loop 腿①的替换）：本测试**不手植源单**（正
//! 例走真实 CLI submit 上游闭环，源单身份全部从已验信封载荷派生）；与
//! tests/adjudicate_loop.rs 同款纪律：不用 mock 引擎。负例证明读侧纪律
//! （篡改 body_b64 / 篡改冗余解析态 body）在派单前拒读零派单。

use std::fs;
use std::path::{Path, PathBuf};

use af_execline::EngineClient;
use af_order::adjudicate::{run_steward_loop, verify_doc, AdjudicationSeed, LoopFacilities};
use af_order::router::{
    append_consumed, duplicate_consumed, load_consumed, load_registry, resolve_adjudication_target,
    verify_envelope, ConsumedRecord, RouterError,
};
use af_order::{cli, StateDir};
use af_station::{State as TaskState, Station};
use af_workorder::{
    sha256_hex, Decision, Ed25519Keypair, Role, TrustedActor, TrustedActorRegistry,
};

const NOW: &str = "2026-11-04T00:00:00Z";
const KEY_EXPIRY: &str = "2027-01-01T00:00:00Z";
const DECISION_EXPIRY: &str = "2026-12-01T00:00:00Z";
const STEWARD_SEED: [u8; 32] = [0x61; 32];
const SOURCE: &str = "mes-execline";

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
        "af-order-router-it-{}-{}-{}",
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

/// 测试内联 base64 编码器（标准字母表带 padding；与 lib 侧严格解码互为对拍）。
fn b64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let bytes = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let acc = (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
        out.push(ALPHABET[(acc >> 18) as usize & 0x3f] as char);
        out.push(ALPHABET[(acc >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(acc >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[acc as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

/// 收件箱运行位注册表（mes 源；读侧只认 schema+sources）。
fn write_registry(triggers_home: &Path) {
    fs::create_dir_all(triggers_home).unwrap();
    fs::write(
        triggers_home.join("registry.json"),
        format!(
            r#"{{"schema":"af.trigger.registry.v1","sources":[{{"name":"{SOURCE}","domain":"mes","key_id":"poc-test-key-1","hmac":"sha256","events":["order_complete"]}}]}}"#
        ),
    )
    .unwrap();
}

/// 按收件箱 envelope_for 同款口径构造信封（七字段+外层，兜底分支口径：
/// thread_key=域:repo、dedup_key=etype:repo、repo=line/<order_id>）。
fn envelope_text(order_id: &str, stable_id: &str, mes_complete: &serde_json::Value) -> String {
    let delivery_id = format!("mes-{order_id}-complete-1");
    let body = serde_json::json!({
        "event": "order_complete",
        "delivery_id": delivery_id,
        "repository": {"full_name": format!("line-poc/{order_id}")},
        "order_id": order_id,
        "stable_id": stable_id,
        "mes_event": mes_complete,
    });
    let body_bytes = serde_json::to_vec(&body).unwrap();
    serde_json::json!({
        "contract": "af.trigger.v1",
        "event_id": format!("{SOURCE}-{delivery_id}"),
        "thread_key": format!("mes:line-poc/{order_id}"),
        "payload_ref": format!("inbox/{SOURCE}/{delivery_id}.json"),
        "payload_sha256": sha256_hex(&body_bytes),
        "dedup_key": format!("order_complete:line-poc/{order_id}"),
        "source": SOURCE,
        "received_at": "2026-11-04T09:00:00+00:00",
        "delivery_id": delivery_id,
        "event_type": "order_complete",
        "key_id": "poc-test-key-1",
        "hmac": "sha256",
        "biz_object": "",
        "body": body,
        "body_b64": b64_encode(&body_bytes),
    })
    .to_string()
}

/// 上游源单：真实 CLI submit（stub 冒烟档全生命周期，真引擎门控 + 驱动层
/// 账本 + MES 事件落盘），返回 (stable_id, MES complete 行)。
fn submit_upstream_order(root: &Path, order_id: &str) -> (String, serde_json::Value) {
    let spec_path = root.join(format!("{order_id}-spec.json"));
    fs::write(
        &spec_path,
        format!(
            r#"{{"order_id":"{order_id}","line":{{"line_id":"line-poc","line_version":"lv-1"}},"budget":100,"note":"消费腿集成验收上游单"}}"#
        ),
    )
    .unwrap();
    let workspace = workspace_root();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target"));
    let submit_out = cli::run(&[
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
    .expect("上游 submit 应闭环");
    let submit_value: serde_json::Value = serde_json::from_str(&submit_out).unwrap();
    assert_eq!(
        submit_value["state"].as_str(),
        Some("Closed"),
        "{submit_out}"
    );
    let stable_id = submit_value["stable_id"]
        .as_str()
        .expect("stable_id")
        .to_string();
    let mes = fs::read_to_string(
        StateDir::new(root)
            .order_dir(order_id)
            .join("mes-events.jsonl"),
    )
    .expect("MES 正本应已落盘");
    let complete = mes
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|value| value["kind"].as_str() == Some("complete"))
        .expect("MES 正本应含 complete 事件");
    (stable_id, complete)
}

fn steward_registry() -> TrustedActorRegistry {
    TrustedActorRegistry::new(vec![TrustedActor::new(
        "steward-a",
        Role::Approver,
        &Ed25519Keypair::from_seed(&STEWARD_SEED).public_key(),
        time(KEY_EXPIRY),
    )])
    .unwrap()
}

/// 消费腿正例全链：信封驱动（源单身份从信封派生，非手植）→ 验证 → 对账 →
/// 游标 fresh → steward 闭环 → 游标落账 → 重放两语义命中。
#[test]
fn consume_leg_runs_full_chain_from_envelope() {
    let root = scratch("full");
    let order_id = "wo-router-it-full";
    let (stable_id, mes_complete) = submit_upstream_order(&root, order_id);

    let triggers_home = root.join("triggers-home");
    write_registry(&triggers_home);
    let registry = load_registry(&triggers_home.join("registry.json")).unwrap();
    let text = envelope_text(order_id, &stable_id, &mes_complete);

    // 读侧两道门 + 派生对账。
    let verified = verify_envelope(&text, &registry).unwrap();
    let state = StateDir::new(&root);
    let target = resolve_adjudication_target(&state, &verified).unwrap();
    assert_eq!(target.order_id, order_id);
    assert_eq!(target.stable_id, stable_id);
    assert_eq!(target.source_state, "Closed");
    assert_eq!(
        target.mes_event_id,
        mes_complete["event_id"].as_str().unwrap()
    );
    assert!(
        target.mes_events_total >= 4,
        "release/start/complete/close 至少四事件"
    );

    // 游标 fresh。
    assert!(load_consumed(&triggers_home).unwrap().is_empty());
    assert_eq!(
        duplicate_consumed(
            &load_consumed(&triggers_home).unwrap(),
            &verified.envelope.delivery_id,
            &verified.envelope.dedup_key
        ),
        None
    );

    // 既有 steward 闭环（真引擎、in-memory 队列；消费腿只接线不重造）。
    let station = Station::open_in_memory().unwrap().into_station();
    let outcome = run_steward_loop(
        &AdjudicationSeed {
            order_id,
            decision: Decision::Approve,
            next_orders: Vec::new(),
            expires_at: time(DECISION_EXPIRY),
            actor_id: "steward-a",
            actor_role: Role::Approver,
            actor_keypair: &Ed25519Keypair::from_seed(&STEWARD_SEED),
            worker_id: "steward-1",
            lease_ms: 60_000,
        },
        &LoopFacilities {
            state: &state,
            station: &station,
            client: &engine_client(),
            registry: &steward_registry(),
            now: time(NOW),
        },
    )
    .unwrap_or_else(|err| panic!("消费腿闭环应走通: {err}"));
    let task = station.get(&outcome.task_id).unwrap().unwrap();
    assert_eq!(task.state, TaskState::Done, "队列任务应闭环 done");
    assert_eq!(
        station.count_by_state(TaskState::Queued).unwrap()
            + station.count_by_state(TaskState::Claimed).unwrap(),
        0,
        "队列应清空"
    );
    assert_eq!(outcome.doc.doc_kind, "裁决单");
    assert_eq!(outcome.doc.template_id, "tpl-adjudication-v1");
    assert_eq!(outcome.doc.source_stable_id, stable_id);
    verify_doc(&outcome.doc).expect("落盘文书应可复算");
    assert_eq!(outcome.receipt.decision, "Approve");
    assert_eq!(outcome.receipt.source_state, "Closed");

    // 游标落账 → 重放两语义命中（delivery 幂等 + 业务去重）。
    let record = ConsumedRecord {
        delivery_id: verified.envelope.delivery_id.clone(),
        source: verified.envelope.source.clone(),
        event_type: verified.envelope.event_type.clone(),
        dedup_key: verified.envelope.dedup_key.clone(),
        order_id: order_id.to_string(),
        consumed_at: "2026-11-04T10:00:00+00:00".into(),
    };
    append_consumed(&triggers_home, &record).unwrap();
    let consumed = load_consumed(&triggers_home).unwrap();
    assert_eq!(consumed.len(), 1);
    assert_eq!(
        duplicate_consumed(&consumed, &record.delivery_id, "fresh-dedup"),
        Some(&record),
        "delivery_id 重放（网络重试语义）应命中"
    );
    assert_eq!(
        duplicate_consumed(&consumed, "mes-other-delivery", &record.dedup_key),
        Some(&record),
        "dedup_key 重放（业务重放语义）应命中"
    );
    let _ = fs::remove_dir_all(&root);
}

/// 读侧纪律负例：篡改 body_b64（sha 自证红）与篡改冗余解析态 body（保留
/// 合法 body_b64+hash，修复批 N16 形态）都在派单前拒读——零裁决单、零游标。
#[test]
fn consume_leg_rejects_tampering_before_dispatch() {
    let root = scratch("tamper");
    let order_id = "wo-router-it-tamper";
    let (stable_id, mes_complete) = submit_upstream_order(&root, order_id);
    let triggers_home = root.join("triggers-home");
    write_registry(&triggers_home);
    let registry = load_registry(&triggers_home.join("registry.json")).unwrap();

    // (a) 篡改 body_b64：payload_sha256 自证不过。
    let mut payload: serde_json::Value =
        serde_json::from_str(&envelope_text(order_id, &stable_id, &mes_complete)).unwrap();
    payload["body_b64"] = serde_json::json!(b64_encode(br#"{"event":"order_complete"}"#));
    let err = verify_envelope(&payload.to_string(), &registry).unwrap_err();
    assert!(matches!(err, RouterError::Payload(_)), "{err}");

    // (b) 篡改冗余解析态 body（合法 body_b64+hash 保留）：CrossCheck 拒读。
    let mut parsed: serde_json::Value =
        serde_json::from_str(&envelope_text(order_id, &stable_id, &mes_complete)).unwrap();
    parsed["body"]["stable_id"] = serde_json::json!("sid-someone-else");
    let err = verify_envelope(&parsed.to_string(), &registry).unwrap_err();
    assert!(matches!(err, RouterError::CrossCheck(_)), "{err}");

    // 零派单：无裁决单落盘、游标空、受理账本零写入。
    let state = StateDir::new(&root);
    assert!(
        !state.root().join("adjudications").exists(),
        "拒读不得产生任何裁决单"
    );
    assert!(load_consumed(&triggers_home).unwrap().is_empty());
    assert!(
        !af_order::admit::admissions_path(&state, order_id).exists(),
        "拒读零受理"
    );
    let _ = fs::remove_dir_all(&root);
}
