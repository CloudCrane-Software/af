//! 【临时 PoC 探针 · 改前读数】风险#5：跨库无原子事务——steward 闭环
//! 「admit 已提交（订单受理账）↔ station.complete 未落（lease 库）」崩溃窗的
//! 双写实证。本文件**只镜像** run_steward_loop 的既有 pub 步骤并在中途停机
//! （无生产注入钩子——这本身就是改前现状的一部分）；跑法：
//!
//! ```text
//! cargo test -p af-order --test poc_crossdb_before -- --nocapture
//! ```
//!
//! 读数面：受理账 admissions.jsonl 行数（改前预期 2 = 同一裁决单二次受理）、
//! 队列任务态、裁决单目录、有无任何残留检出面。

use std::fs;
use std::path::{Path, PathBuf};

use af_execline::EngineClient;
use af_order::adjudicate::{
    generate_adjudication_doc, loop_gate_list, loop_resource_profile, run_steward_loop,
    template_body_sha256, AdjudicationSeed, LoopFacilities, STEWARD_ROLE,
};
use af_order::admit::{
    admit, load_receipts, AdmissionRequest, AdjudicationTemplate,
};
use af_order::{cli, StateDir};
use af_station::{State as TaskState, Station};
use af_workorder::{
    Decision, DecisionArtifact, DecisionAttributes, Ed25519Keypair, Role, TrustedActor,
    TrustedActorRegistry,
};

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
        "af-order-poc-before-{}-{tag}-{nanos}",
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
            r#"{{"order_id":"{order_id}","line":{{"line_id":"line-poc","line_version":"lv-1"}},"budget":100,"note":"风险#5 改前探针单"}}"#
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

fn registry() -> TrustedActorRegistry {
    TrustedActorRegistry::new(vec![TrustedActor::new(
        "steward-a",
        Role::Approver,
        &Ed25519Keypair::from_seed(&STEWARD_SEED).public_key(),
        time(KEY_EXPIRY),
    )])
    .unwrap()
}

/// 改前读数：崩溃窗（admit 已落账、complete 未落）后的真实后果。
/// 步骤逐条镜像 adjudicate.rs::run_steward_loop（②生成→落盘→入队→认领→
/// 复算→真签→admit），唯一差别 = admit 之后停机（不调 station.complete）。
#[test]
fn poc_before_crash_between_admit_and_complete_double_writes() {
    let root = scratch("crash");
    let order_id = "wo-poc-before";
    submit_stub_order(&root, order_id);
    let state = StateDir::new(&root);
    let clock = af_station::ManualClock::new(1_767_225_600_000);
    let station = Station::open_in_memory_with_clock(clock.clone())
        .unwrap()
        .into_station();
    let keypair = Ed25519Keypair::from_seed(&STEWARD_SEED);

    // —— 以下手工镜像 run_steward_loop 首走（adjudicate.rs:409-487）——
    let (doc, source) =
        generate_adjudication_doc(&state, order_id, vec![], time(DECISION_EXPIRY)).unwrap();
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
        .claim("steward-1", STEWARD_ROLE, 1_000)
        .unwrap()
        .unwrap();
    assert_eq!(task.id, doc.adjudication_id);
    // next_decision_seq 同款复算：已受理最大 seq + 1（无受理 = 1）。
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
    let decision = DecisionArtifact::seal(
        "steward-a",
        Role::Approver,
        Decision::Approve,
        &payload,
        &keypair,
        attributes,
    );
    let held_voucher = source.budget_voucher().unwrap().voucher_id.clone();
    let receipt = admit(
        &AdmissionRequest {
            order_id: order_id.to_string(),
            decision,
            actor_registry: registry(),
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
        &engine_client(),
        time(NOW),
    )
    .expect("崩溃前 admit 应已提交（受理账落一笔）");
    println!("[before] 首走 admit 已提交: seq={} key={}", receipt.decision_seq, receipt.idempotency_key);

    // ── 注入「中途崩溃」：此处停机，station.complete 永不执行 ──
    let receipts_after_crash = load_receipts(&state, order_id).unwrap();
    println!(
        "[before] 崩溃后受理账行数 = {}（任务停在 claimed，等待租约回收）",
        receipts_after_crash.len()
    );
    let stuck = station.get(&doc.adjudication_id).unwrap().unwrap();
    println!(
        "[before] 崩溃后任务态 = {:?}（claimant={:?} lease_until={}）",
        stuck.state, stuck.claimant, stuck.lease_until.unwrap_or(0)
    );

    // ── 租约到期（注入时钟推进，不睡眠）→ 运维重试 = 再跑 run_steward_loop ──
    clock.advance(2_000);
    let seed = AdjudicationSeed {
        order_id,
        decision: Decision::Approve,
        next_orders: vec![],
        expires_at: time(DECISION_EXPIRY),
        actor_id: "steward-a",
        actor_role: Role::Approver,
        actor_keypair: &keypair,
        worker_id: "steward-1",
        lease_ms: 60_000,
    };
    let outcome = run_steward_loop(
        &seed,
        &LoopFacilities {
            state: &state,
            station: &station,
            client: &engine_client(),
            registry: &registry(),
            now: time(NOW),
        },
    )
    .expect("重试闭环在改前能走通（但会双写）");

    // ── 读数 ──
    let receipts = load_receipts(&state, order_id).unwrap();
    println!("[before] 重试后受理账行数 = {}", receipts.len());
    for r in &receipts {
        println!(
            "[before]   receipt: seq={} decision={} replayed={} key={}",
            r.decision_seq, r.decision, r.replayed, r.idempotency_key
        );
    }
    println!(
        "[before] 重试认领到的是旧任务 {}；新生成的 {} 入队后留在队列",
        outcome.task_id, outcome.adjudication_id
    );
    println!(
        "[before] 队列: queued={} claimed={} done={}",
        station.count_by_state(TaskState::Queued).unwrap(),
        station.count_by_state(TaskState::Claimed).unwrap(),
        station.count_by_state(TaskState::Done).unwrap(),
    );
    let adj_dir = state.root().join("adjudications").join(order_id);
    let files: Vec<String> = fs::read_dir(&adj_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    println!("[before] 裁决单目录文件 = {files:?}");
    let has_journal = files.iter().any(|f| f.contains("journal"));
    println!(
        "[before] 残留检出面（意图账/journal）存在 = {has_journal}（改前无任何 prepared/committed 面）"
    );
    // 改前判读（探针如实记录，不做断言门——判读进 poc/README）：
    let double = receipts.len() >= 2;
    println!(
        "[before] 同一裁决单是否被二次受理（双写）= {double}（seq 1 与 2 均在账）"
    );
    let _ = fs::remove_dir_all(&root);
}
