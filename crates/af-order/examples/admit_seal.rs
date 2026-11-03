//! steward 侧准入签名工具（开发/联调面，非生产 CLI 面）：为一张已落账源单
//! 真签一份 DecisionArtifact（绑定 `admission_decision_payload`）并组
//! admission.json，供 `af-order admit --request` 消费。
//!
//! 用法：
//! ```text
//! admit_seal --state <dir> --order-id <id> --out <admission.json> \
//!            [--actor-id user-a] [--seq 7] [--basis artifact:fa-x:qualified] \
//!            [--expires 2026-11-04T00:00:00Z] [--seed-hex <64hex>]
//! ```
//!
//! 不给 --seed-hex 时从 /dev/urandom 取新种子（ Steward 一次性密钥联调形态；
//! 种子与注册表一并打印，便于复现）。模板/资源画像/门清单取骨架缺省样例
//! （与 tests/admit.rs 同源），只供联调——生产面由配方层供给。

use std::fs;
use std::io::Read;
use std::path::PathBuf;

use af_order::admit::{
    AdjudicationTemplate, AdmissionRequest, GateListSpec, GateSpec, ResourceClaim,
    ResourceProfileSpec,
};
use af_workorder::{
    Decision, DecisionArtifact, DecisionAttributes, Ed25519Keypair, Role, TrustedActor,
    TrustedActorRegistry,
};
use chrono::{DateTime, Utc};
use serde_json::json;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(err) = run(&args) {
        eprintln!("admit_seal: {err}");
        std::process::exit(2);
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let mut state: Option<PathBuf> = None;
    let mut order_id: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut actor_id = "user-a".to_string();
    let mut seq = 7u64;
    let mut basis = vec!["artifact:fa-wo-poca2-001:qualified".to_string()];
    let mut expires: DateTime<Utc> = DateTime::parse_from_rfc3339("2026-11-04T00:00:00Z")
        .map_err(|err| err.to_string())?
        .with_timezone(&Utc);
    let mut seed_hex: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{flag} 需要一个值"))?;
        match flag {
            "--state" => state = Some(PathBuf::from(value)),
            "--order-id" => order_id = Some(value.clone()),
            "--out" => out = Some(PathBuf::from(value)),
            "--actor-id" => actor_id = value.clone(),
            "--seq" => {
                seq = value
                    .parse()
                    .map_err(|_| format!("--seq 不是 u64: {value}"))?
            }
            "--basis" => basis = vec![value.clone()],
            "--expires" => {
                expires = DateTime::parse_from_rfc3339(value)
                    .map_err(|err| format!("--expires 不是合法 RFC3339: {err}"))?
                    .with_timezone(&Utc)
            }
            "--seed-hex" => seed_hex = Some(value.clone()),
            other => return Err(format!("未知参数 {other}")),
        }
        index += 2;
    }
    let (state, order_id, out) = match (state, order_id, out) {
        (Some(state), Some(order_id), Some(out)) => (state, order_id, out),
        _ => {
            return Err("缺参数：--state <dir> --order-id <id> --out <admission.json> 必填".into())
        }
    };

    // 恢复源单（只信盘上 ledger 最近快照）。
    let ledger_path = state.join("orders").join(&order_id).join("ledger.jsonl");
    let records = af_execline::OrderLedger::load(&ledger_path)
        .map_err(|err| format!("ledger 读取失败 {}: {err}", ledger_path.display()))?;
    let order = records
        .iter()
        .rev()
        .find_map(|record| record.order_after.clone())
        .ok_or_else(|| format!("全账本无订单快照: {}", ledger_path.display()))?;

    // steward 密钥：显式种子联调复现，否则 urandom 新铸。
    let seed: [u8; 32] = match &seed_hex {
        Some(hex) => decode_hex_32(hex)?,
        None => {
            let mut seed = [0u8; 32];
            fs::File::open("/dev/urandom")
                .and_then(|mut file| file.read_exact(&mut seed))
                .map_err(|err| format!("/dev/urandom 读取失败: {err}"))?;
            seed
        }
    };
    let keypair = Ed25519Keypair::from_seed(&seed);
    let key_expires_at = DateTime::parse_from_rfc3339("2027-01-01T00:00:00Z")
        .map_err(|err| err.to_string())?
        .with_timezone(&Utc);
    let registry = TrustedActorRegistry::new(vec![TrustedActor::new(
        actor_id.clone(),
        Role::Approver,
        &keypair.public_key(),
        key_expires_at,
    )])
    .map_err(|err| format!("注册表无效: {err}"))?;

    // 按 admission payload 真签（与 admit 链第③步引擎绑定值同源）。
    let payload = order.admission_decision_payload();
    let decision = DecisionArtifact::seal(
        actor_id.clone(),
        Role::Approver,
        Decision::Approve,
        &payload,
        &keypair,
        DecisionAttributes::new(seq, basis, "a".repeat(64), expires),
    );
    let request = AdmissionRequest {
        order_id: order_id.clone(),
        decision: decision.clone(),
        actor_registry: registry.clone(),
        template: AdjudicationTemplate {
            template_id: "tpl-adjudication-v1".into(),
            template_version: "1.0".into(),
            doc_kind: "裁决单".into(),
            body_sha256: "c".repeat(64),
        },
        resource_profile: ResourceProfileSpec {
            profile_id: "rp-poc".into(),
            profile_version: "1.0".into(),
            claims: vec![ResourceClaim {
                resource: "glm-cp".into(),
                units: 1,
            }],
        },
        gate_list: GateListSpec {
            gates: vec![GateSpec {
                gate_id: "F1".into(),
                kind: "fast".into(),
                required: true,
            }],
        },
        validated_at: None,
    };
    let text = serde_json::to_string_pretty(&json!({
        "order_id": request.order_id,
        "decision": serde_json::to_value(&decision).map_err(|err| err.to_string())?,
        "actor_registry": serde_json::to_value(&registry).map_err(|err| err.to_string())?,
        "template": serde_json::to_value(&request.template).map_err(|err| err.to_string())?,
        "resource_profile": serde_json::to_value(&request.resource_profile).map_err(|err| err.to_string())?,
        "gate_list": serde_json::to_value(&request.gate_list).map_err(|err| err.to_string())?,
    }))
    .map_err(|err| err.to_string())?;
    fs::write(&out, format!("{text}\n")).map_err(|err| format!("落盘 {}: {err}", out.display()))?;

    println!(
        "{{\"admission_json\": {}, \"seed_hex\": {}, \"key_id\": {}, \"stable_id\": {}, \"payload\": {}}}",
        json!(out.display().to_string()),
        json!(encode_hex(&seed)),
        json!(keypair.public_key().key_id()),
        json!(order.stable_id().as_str()),
        json!(payload),
    );
    Ok(())
}

fn decode_hex_32(text: &str) -> Result<[u8; 32], String> {
    if text.len() != 64 {
        return Err(format!("--seed-hex 长度 {} ≠ 64", text.len()));
    }
    let mut out = [0u8; 32];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
            .map_err(|err| format!("--seed-hex 非十六进制: {err}"))?;
    }
    Ok(out)
}

fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
