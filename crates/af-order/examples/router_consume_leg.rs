//! event-router 消费腿运行器（PoC-D 面 → PoC-C 落点；联调/留证面，非生产
//! CLI 子命令）：消费一张**真实的 af.trigger.v1 信封**（Trigger 收件箱落盘
//! 产物），把它接到 [`af_order::adjudicate`] 的裁决环上，事件驱动地跑通
//! 「信封 → 裁决单 → af-station 队列 → steward 认领 → 真签 → admit 受理 →
//! complete → 消费游标落账」全链。每条腿 stdout 留 JSON 证据。
//!
//! 与 examples/adjudicate_loop 的判据差异（腿①替换）：adjudicate_loop 腿①
//! **自己造一张源任务单**（CLI submit 内嵌）；本运行器不造单——源单身份
//! （order_id/stable_id/MES complete 事件）全部**从已验信封载荷派生**并与
//! state 对账（[`af_order::router::resolve_adjudication_target`]），上游
//! 源单由外部（真实 af-order CLI submit）先行闭环。
//!
//! 用法：
//! ```text
//! router_consume_leg --envelope <inbox/…/delivery.json> --state <dir> \
//!                    --station-db <station.db> --triggers-home <dir> \
//!                    [--registry <registry.json>] [--workspace <dir>] \
//!                    [--target-dir <dir>] [--actor steward-poc]
//! ```
//!
//! steward 密钥固定种子（复现留证，与 adjudicate_loop 同款纪律）。重放投递
//! （consumed.jsonl 游标命中）在腿③ fail-closed 拒派，exit 1。

use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use af_execline::EngineClient;
use af_order::adjudicate::{run_steward_loop, AdjudicationSeed, LoopFacilities, STEWARD_ROLE};
use af_order::router::{
    append_consumed, duplicate_consumed, load_consumed, load_registry,
    resolve_adjudication_target, verify_envelope, ConsumedRecord,
};
use af_order::{cli, StateDir};
use af_station::Station;
use af_workorder::{Decision, Ed25519Keypair, Role, TrustedActor, TrustedActorRegistry};

const NOW: &str = "2026-11-04T00:00:00Z";
const KEY_EXPIRY: &str = "2027-01-01T00:00:00Z";
const DECISION_EXPIRY: &str = "2026-12-01T00:00:00Z";
/// steward 固定种子（64 hex，复现面；与 adjudicate_loop 同款）。
const STEWARD_SEED_HEX: &str =
    "6161616161616161616161616161616161616161616161616161616161616161";

fn fail(leg: &str, err: impl std::fmt::Display) -> ! {
    eprintln!(
        "{}",
        serde_json::json!({ "leg": leg, "ok": false, "error": err.to_string() })
    );
    std::process::exit(1);
}

fn leg(name: &str, value: serde_json::Value) {
    println!(
        "{}",
        serde_json::json!({ "leg": name, "ok": true, "detail": value })
    );
}

fn time(text: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&chrono::Utc)
}

fn decode_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    for (index, chunk) in STEWARD_SEED_HEX.as_bytes().chunks(2).enumerate() {
        let hex = std::str::from_utf8(chunk).unwrap();
        seed[index] = u8::from_str_radix(hex, 16).unwrap();
    }
    seed
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut envelope_path: Option<PathBuf> = None;
    let mut state_path: Option<PathBuf> = None;
    let mut station_db: Option<PathBuf> = None;
    let mut triggers_home: Option<PathBuf> = None;
    let mut registry_path: Option<PathBuf> = None;
    let mut workspace: Option<PathBuf> = None;
    let mut target_dir: Option<PathBuf> = None;
    let mut actor_id = "steward-poc".to_string();
    let mut index = 0;
    while index < args.len() {
        let (flag, value) = {
            let flag = args[index].as_str();
            let value = args
                .get(index + 1)
                .unwrap_or_else(|| panic!("{flag} 需要一个值"));
            (flag.to_string(), value.clone())
        };
        match flag.as_str() {
            "--envelope" => envelope_path = Some(PathBuf::from(value)),
            "--state" => state_path = Some(PathBuf::from(value)),
            "--station-db" => station_db = Some(PathBuf::from(value)),
            "--triggers-home" => triggers_home = Some(PathBuf::from(value)),
            "--registry" => registry_path = Some(PathBuf::from(value)),
            "--workspace" => workspace = Some(PathBuf::from(value)),
            "--target-dir" => target_dir = Some(PathBuf::from(value)),
            "--actor" => actor_id = value,
            other => fail("parse", format!("未知参数 {other}")),
        }
        index += 2;
    }
    let envelope_path =
        envelope_path.unwrap_or_else(|| fail("parse", "缺 --envelope <path>"));
    let state_path = state_path.unwrap_or_else(|| fail("parse", "缺 --state <dir>"));
    let station_db = station_db.unwrap_or_else(|| fail("parse", "缺 --station-db <path>"));
    let triggers_home =
        triggers_home.unwrap_or_else(|| fail("parse", "缺 --triggers-home <dir>"));
    let registry_path =
        registry_path.unwrap_or_else(|| triggers_home.join("registry.json"));
    let workspace =
        workspace.unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let target_dir = target_dir.unwrap_or_else(|| {
        std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| workspace.join("target"))
    });

    // ── 腿①：信封消费（读侧第一道门：形状 → body_b64 自证 → 冗余字段
    //    逐项核对 → 注册表重推导）────────────────────────────────────
    let envelope_text = fs::read_to_string(&envelope_path).unwrap_or_else(|err| {
        fail(
            "envelope_verify",
            format!("{}: {err}", envelope_path.display()),
        )
    });
    let registry = load_registry(&registry_path)
        .unwrap_or_else(|err| fail("envelope_verify", err));
    let verified = verify_envelope(&envelope_text, &registry)
        .unwrap_or_else(|err| fail("envelope_verify", err));
    leg(
        "envelope_verify",
        serde_json::json!({
            "envelope_path": envelope_path.display().to_string(),
            "contract": verified.envelope.contract,
            "event_id": verified.envelope.event_id,
            "thread_key": verified.envelope.thread_key,
            "payload_ref": verified.envelope.payload_ref,
            "payload_sha256": verified.envelope.payload_sha256,
            "dedup_key": verified.envelope.dedup_key,
            "source": verified.envelope.source,
            "received_at": verified.envelope.received_at,
            "delivery_id": verified.envelope.delivery_id,
            "event_type": verified.envelope.event_type,
            "body_bytes_len": verified.body_bytes_len,
            "note": "唯一可信载荷=body_b64（payload_sha256 自证）；冗余字段（event_type/delivery_id/body）已与重解析逐项核对，thread_key/dedup_key 已按收件箱兜底分支从注册表+已验载荷重推导核对",
        }),
    );

    // ── 腿②：裁决目标派生（读侧第二道门：源单身份从信封派生并与 state
    //    对账——替代 adjudicate_loop 的「自己造源任务单」腿①）──────────
    let state = StateDir::new(&state_path);
    let target = resolve_adjudication_target(&state, &verified)
        .unwrap_or_else(|err| fail("adjudication_target", err));
    leg(
        "adjudication_target",
        serde_json::json!({
            "order_id": target.order_id,
            "stable_id": target.stable_id,
            "mes_event_id": target.mes_event_id,
            "source_state": target.source_state,
            "ledger_records": target.ledger_records,
            "mes_events_total": target.mes_events_total,
            "note": "源单身份从已验信封载荷派生；stable_id 与账本对账、MES complete 事件与源单 MES 正本逐字对账过",
        }),
    );

    // ── 腿③：防重放游标（consumed.jsonl；delivery/dedup 两语义任一命中即拒）──
    let consumed = load_consumed(&triggers_home).unwrap_or_else(|err| fail("dedup_cursor", err));
    if let Some(hit) =
        duplicate_consumed(&consumed, &verified.envelope.delivery_id, &verified.envelope.dedup_key)
    {
        fail(
            "dedup_cursor",
            format!(
                "重放投递拒派：delivery_id={} dedup_key={} 已于 {} 消费（order_id={}）",
                hit.delivery_id, hit.dedup_key, hit.consumed_at, hit.order_id
            ),
        );
    }
    leg(
        "dedup_cursor",
        serde_json::json!({
            "consumed_path": af_order::router::consumed_path(&triggers_home).display().to_string(),
            "consumed_entries": consumed.len(),
            "verdict": "fresh",
        }),
    );

    // ── steward 面：固定种子密钥 + 受信注册表（复现留证）────────────
    static KEYPAIR: OnceLock<Ed25519Keypair> = OnceLock::new();
    let keypair = KEYPAIR.get_or_init(|| Ed25519Keypair::from_seed(&decode_seed()));
    let actor_registry = TrustedActorRegistry::new(vec![TrustedActor::new(
        actor_id.clone(),
        Role::Approver,
        &keypair.public_key(),
        time(KEY_EXPIRY),
    )])
    .unwrap_or_else(|err| fail("registry", err));
    leg(
        "steward",
        serde_json::json!({
            "actor_id": actor_id,
            "role": "Approver",
            "key_id": keypair.public_key().key_id(),
            "seed_hex": STEWARD_SEED_HEX,
            "decision": "Approve",
            "worker_id": "steward-1",
            "note": "Decision=Approve→原单继续（receipt 留档）；角色纪律走 af-workorder 四层校验（OR2106），不另造权限面",
        }),
    );

    // ── 腿④-⑦：既有 steward 闭环（生成裁决单 → 入队 → 认领 → 真签 →
    //    admit → complete；全部复用 adjudicate.rs 既有件，零新机制）────
    let station = Station::open(&station_db)
        .unwrap_or_else(|err| fail("station_open", err))
        .into_station();
    let client = EngineClient::locate(&workspace, &target_dir)
        .unwrap_or_else(|err| fail("engine_locate", err));
    let order_id = target.order_id.clone();
    let outcome = run_steward_loop(
        &AdjudicationSeed {
            order_id: &order_id,
            decision: Decision::Approve,
            next_orders: Vec::new(),
            expires_at: time(DECISION_EXPIRY),
            actor_id: &actor_id,
            actor_role: Role::Approver,
            actor_keypair: keypair,
            worker_id: "steward-1",
            lease_ms: 60_000,
        },
        &LoopFacilities {
            state: &state,
            station: &station,
            client: &client,
            registry: &actor_registry,
            now: time(NOW),
        },
    )
    .unwrap_or_else(|err| fail("steward_loop", err));

    leg(
        "adjudication_doc",
        serde_json::json!({
            "adjudication_id": outcome.doc.adjudication_id,
            "doc_kind": outcome.doc.doc_kind,
            "template_id": outcome.doc.template_id,
            "path": outcome.doc_path.display().to_string(),
            "doc_sha256": outcome.doc.doc_sha256,
            "source_stable_id": outcome.doc.source_stable_id,
            "subject": outcome.doc.subject,
            "basis": outcome.doc.basis,
            "next_orders": outcome.doc.next_orders,
            "next_orders_digest": outcome.doc.next_orders_digest,
            "expires_at": outcome.doc.expires_at.to_rfc3339(),
        }),
    );
    leg(
        "enqueue",
        serde_json::json!({
            "station_db": station_db.display().to_string(),
            "idempotency_key": format!("{}/{}", outcome.doc.source_stable_id, outcome.adjudication_id),
            "required_role": STEWARD_ROLE,
            "enqueued": match &outcome.enqueue {
                af_station::Enqueued::Inserted { id } => serde_json::json!({"inserted": id}),
                af_station::Enqueued::Duplicate { existing_id } => serde_json::json!({"duplicate_of": existing_id}),
            },
        }),
    );

    // ── 消费游标落账（派单闭环成功后；契约 §4 一行一事件）────────────
    let consumed_record = ConsumedRecord {
        delivery_id: verified.envelope.delivery_id.clone(),
        source: verified.envelope.source.clone(),
        event_type: verified.envelope.event_type.clone(),
        dedup_key: verified.envelope.dedup_key.clone(),
        order_id: target.order_id.clone(),
        consumed_at: chrono::Utc::now().to_rfc3339(),
    };
    append_consumed(&triggers_home, &consumed_record)
        .unwrap_or_else(|err| fail("consumed_append", err));
    leg("consumed_appended", serde_json::to_value(&consumed_record).unwrap());

    // ── 腿⑧：终态证据（任务 done / 受理账本 / CLI show 对拍）──────────
    let task = station
        .get(&outcome.task_id)
        .unwrap_or_else(|err| fail("task_get", err))
        .unwrap_or_else(|| fail("task_get", "任务消失"));
    let receipt_path = af_order::admit::admissions_path(&state, &order_id);
    let show = cli::run(&[
        "show".into(),
        order_id.clone(),
        "--state".into(),
        state_path.display().to_string(),
    ])
    .unwrap_or_else(|err| fail("show", err));
    let show_value: serde_json::Value = serde_json::from_str(&show).unwrap();
    leg(
        "receipt_and_completion",
        serde_json::json!({
            "task_id": outcome.task_id,
            "task_state": task.state.as_str(),
            "claimant": task.claimant,
            "admissions_path": receipt_path.display().to_string(),
            "receipt": serde_json::to_value(&outcome.receipt).unwrap(),
            "cli_show_state": show_value["state"],
            "cli_show_ledger_ops": show_value["ledger"]["ops"].as_array().map(|ops| ops.len()),
            "cli_show_mes_events": show_value["mes"]["events"].as_array().map(|events| events.len()),
        }),
    );
    println!(
        "{}",
        serde_json::json!({
            "leg": "loop_complete",
            "ok": true,
            "summary": {
                "order_id": order_id,
                "delivery_id": verified.envelope.delivery_id,
                "verdict": "af.trigger.v1 信封（Trigger 收件箱真产出）→ 读侧验证（body_b64 自证+冗余字段核对+注册表重推导）→ 裁决目标从信封派生（stable_id/MES complete 事件与源单正本对账）→ 防重放游标 fresh → 裁决单生成落盘 → af-station 入队(幂等键 UNIQUE) → steward 原子认领 → 复算文书 → DecisionArtifact 真签 → admit OR21xx 全链受理 → complete 队列任务 → consumed.jsonl 游标落账",
            }
        })
    );
}
