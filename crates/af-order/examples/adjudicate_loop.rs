//! PoC-C 件④ 真实闭环运行器（联调/留证面，非生产 CLI 子命令）：对隔离
//! state 目录跑通一单——任务单 create→…→complete（CLI submit stub 档，真
//! 引擎门控 + 驱动层账本 + MES 事件）→ 生成裁决单 → 入 af-station 队列 →
//! steward 认领 → 真签 DecisionArtifact（af-workorder ed25519）→ admit
//! 受理（OR21xx 全链）→ complete 队列任务。每条腿 stdout 留 JSON 证据。
//!
//! 用法：
//! ```text
//! adjudicate_loop --state <dir> --station-db <station.db> \
//!                 [--workspace <dir>] [--target-dir <dir>] \
//!                 [--order-id wo-pocc-loop] [--actor steward-poc]
//! ```
//!
//! steward 密钥用固定种子（--seed 面向复现留证），actor 注册表随运行打印。

use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use af_execline::EngineClient;
use af_order::adjudicate::{run_steward_loop, AdjudicationSeed, LoopFacilities, STEWARD_ROLE};
use af_order::{cli, StateDir};
use af_station::Station;
use af_workorder::{Decision, Ed25519Keypair, Role, TrustedActor, TrustedActorRegistry};

const NOW: &str = "2026-11-03T00:00:00Z";
const KEY_EXPIRY: &str = "2027-01-01T00:00:00Z";
const DECISION_EXPIRY: &str = "2026-12-01T00:00:00Z";
/// steward 固定种子（64 hex，复现面）。
const STEWARD_SEED_HEX: &str = "6161616161616161616161616161616161616161616161616161616161616161";

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
    let mut state: Option<PathBuf> = None;
    let mut station_db: Option<PathBuf> = None;
    let mut workspace: Option<PathBuf> = None;
    let mut target_dir: Option<PathBuf> = None;
    let mut order_id = "wo-pocc-loop".to_string();
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
            "--state" => state = Some(PathBuf::from(value)),
            "--station-db" => station_db = Some(PathBuf::from(value)),
            "--workspace" => workspace = Some(PathBuf::from(value)),
            "--target-dir" => target_dir = Some(PathBuf::from(value)),
            "--order-id" => order_id = value,
            "--actor" => actor_id = value,
            other => fail("parse", format!("未知参数 {other}")),
        }
        index += 2;
    }
    let state_path = state.unwrap_or_else(|| fail("parse", "缺 --state <dir>"));
    let station_db = station_db.unwrap_or_else(|| fail("parse", "缺 --station-db <path>"));
    let workspace =
        workspace.unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let target_dir = target_dir.unwrap_or_else(|| {
        std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| workspace.join("target"))
    });

    // ── 腿①：任务单 create→…→complete（CLI submit stub 档全生命周期）──
    let spec_path = state_path.join(format!("{order_id}-spec.json"));
    if let Some(parent) = spec_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(
        &spec_path,
        format!(
            r#"{{"order_id":"{order_id}","line":{{"line_id":"line-poc","line_version":"lv-1"}},"budget":100,"note":"PoC-C 裁决环闭环单"}}"#
        ),
    )
    .unwrap();
    let submit_out = cli::run(&[
        "submit".into(),
        "--spec".into(),
        spec_path.display().to_string(),
        "--state".into(),
        state_path.display().to_string(),
        "--unit".into(),
        "stub".into(),
        "--workspace".into(),
        workspace.display().to_string(),
        "--target-dir".into(),
        target_dir.display().to_string(),
    ])
    .unwrap_or_else(|err| fail("submit", err));
    let submit_value: serde_json::Value = serde_json::from_str(&submit_out).unwrap();
    if submit_value["state"].as_str() != Some("Closed") {
        fail("submit", format!("订单未闭环: {submit_out}"));
    }
    leg(
        "submit",
        serde_json::json!({
            "command": format!("af-order submit --spec {} --state {} --unit stub", spec_path.display(), state_path.display()),
            "order_id": order_id,
            "stable_id": submit_value["stable_id"],
            "state": submit_value["state"],
            "replayed": submit_value["replayed"],
            "ledger_ops_appended": submit_value["ledger_ops_appended"],
            "mes_events_emitted": submit_value["mes_events_emitted"],
            "order_dir": submit_value["order_dir"],
        }),
    );

    // ── 腿②：MES 事件证据（驱动层 complete 腿的落盘事件）────────────
    let state = StateDir::new(&state_path);
    let mes_path = state.order_dir(&order_id).join("mes-events.jsonl");
    let mes = fs::read_to_string(&mes_path)
        .unwrap_or_else(|err| fail("mes", format!("{}: {err}", mes_path.display())));
    // MES 事件线格式的种类字段是 kind（release/start/complete/close，见
    // af-execline MesFileWriter 落盘行）。
    let kinds: Vec<String> = mes
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|value| value["kind"].as_str().map(str::to_string))
        .collect();
    let ops = &kinds;
    if !ops.iter().any(|op| op == "complete") {
        fail("mes", format!("MES 事件缺 complete 腿: {ops:?}"));
    }
    leg(
        "mes_events",
        serde_json::json!({
            "path": mes_path.display().to_string(),
            "event_count": mes.lines().count(),
            "kinds": ops,
        }),
    );

    // ── steward 面：固定种子密钥 + 受信注册表（复现留证）────────────
    static KEYPAIR: OnceLock<Ed25519Keypair> = OnceLock::new();
    let keypair = KEYPAIR.get_or_init(|| Ed25519Keypair::from_seed(&decode_seed()));
    let registry = TrustedActorRegistry::new(vec![TrustedActor::new(
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
            "note": "Decision=Approve→原单继续（receipt 留档）；Reviser 无 Approve 权、Arbitrate 仅 Arbiter（引擎四层既有权限面，OR2106）",
        }),
    );

    // ── 腿③-⑥：裁决单生成 → 入队 → 认领 → 真签 → admit → complete ──
    let station = Station::open(&station_db)
        .unwrap_or_else(|err| fail("station_open", err))
        .into_station();
    let client = EngineClient::locate(&workspace, &target_dir)
        .unwrap_or_else(|err| fail("engine_locate", err));
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
            registry: &registry,
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
            "idempotency_key": format!("{}/{}", outcome.doc.source_stable_id, outcome.doc.adjudication_id),
            "required_role": STEWARD_ROLE,
            "enqueued": match &outcome.enqueue {
                af_station::Enqueued::Inserted { id } => serde_json::json!({"inserted": id}),
                af_station::Enqueued::Duplicate { existing_id } => serde_json::json!({"duplicate_of": existing_id}),
            },
        }),
    );

    // ── 腿⑦：终态证据（任务 done / 受理账本 / CLI show 对拍）────────
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
                "verdict": "任务单闭环 → MES complete 事件 → 裁决单生成落盘 → af-station 入队(幂等键 UNIQUE) → steward 原子认领 → 复算文书 → DecisionArtifact 真签 → admit OR21xx 全链受理 → complete 队列任务",
            }
        })
    );
}
