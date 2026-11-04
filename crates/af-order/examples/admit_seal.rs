//! steward 侧准入签名工具（开发/联调面，非生产 CLI 面）：为一张已落账源单
//! 真签一份 DecisionArtifact（绑定 `admission_decision_payload`）并组
//! admission.json，供 `af-order admit --request` 消费。
//!
//! 用法：
//! ```text
//! admit_seal --state <dir> --order-id <id> --out <admission.json> \
//!            [--actor-id user-a] [--seq 7] [--basis artifact:<id>:<sha256>] \
//!            [--next-orders id1,id2] [--expires 2026-11-04T00:00:00Z] \
//!            [--seed-hex <64hex>] [--decision approve|revise|reject] \
//!            [--role approver|reviser|arbiter]
//! ```
//!
//! `--decision`/`--role` 缺省保持现行为 Approve/Approver。配对合法性在
//! 签出之前判（`Role::can_decide`，判据 = af-workorder 既有权限面，与
//! admit 四层校验第③层同源）：Reviser 无 Approve 权（Approve/Reject 都
//! 越权）、Approver 无 Revise 权、只有 Arbiter 全权——非法配对直接拒绝
//! 签发（中文报错，零读取零写入），不放行到受理侧才红。
//!
//! 不给 --seed-hex 时从 /dev/urandom 取新种子（Steward 一次性密钥联调形态；
//! 种子与注册表一并打印，便于复现）。basis 缺省 = 源单实际实体解析
//! （af_order::adjudicate::resolved_basis），计划面 digest 与受理侧同一规范
//! 化现算，凭证面缺省呈报源单持有预算凭证——默认产出可通过 admit 全链的
//! 请求。模板/资源画像/门清单取骨架缺省样例（与 tests/admit.rs 同源），
//! 只供联调——生产面由配方层供给。注册表 actor 角色与工件角色同取 --role
//! （四层第④层角色一致性在工具侧不制造自相矛盾的请求）。

use std::fs;
use std::io::Read;
use std::path::PathBuf;

use af_order::admit::{
    canonical_next_orders_digest, AdjudicationTemplate, AdmissionRequest, GateListSpec, GateSpec,
    ResourceClaim, ResourceProfileSpec,
};
use af_order::adjudicate::{resolved_basis, template_body_sha256};
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
    let mut basis: Option<Vec<String>> = None;
    let mut next_orders: Vec<String> = Vec::new();
    let mut expires: DateTime<Utc> = DateTime::parse_from_rfc3339("2026-11-04T00:00:00Z")
        .map_err(|err| err.to_string())?
        .with_timezone(&Utc);
    let mut seed_hex: Option<String> = None;
    // 缺省保持现行为：Approver + Approve（不许改既有默认语义）。
    let mut role = Role::Approver;
    let mut decision = Decision::Approve;

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
            "--basis" => basis = Some(vec![value.clone()]),
            "--next-orders" => {
                next_orders = value
                    .split(',')
                    .map(str::trim)
                    .filter(|entry| !entry.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            "--expires" => {
                expires = DateTime::parse_from_rfc3339(value)
                    .map_err(|err| format!("--expires 不是合法 RFC3339: {err}"))?
                    .with_timezone(&Utc);
            }
            "--seed-hex" => seed_hex = Some(value.clone()),
            "--decision" => decision = parse_decision(value)?,
            "--role" => role = parse_role(value)?,
            other => return Err(format!("未知参数 {other}")),
        }
        index += 2;
    }
    // 配对合法性先于一切读取/签出（判据 = af-workorder 既有权限面：
    // Decision::required_permission ∈ Role::allowed_permissions，与 admit
    // 四层校验第③层同源，不是另造的一套阶梯）。非法配对零读取零写入。
    if !role.can_decide(decision) {
        return Err(format!(
            "非法配对：--role {} 不能签 --decision {}——{decision:?} 所需权限 {:?} 不在 {role:?} 角色的权限面 {:?} 内\
             （根本法角色阶梯 Reader < Approver < Reviser < Arbiter，只有 Arbiter 全权；Reviser 无 Approve 权，\
             Reject 与 Approve 同需 Approve 权）。拒绝签发：未读账本、未写任何文件",
            role_name(role),
            decision_name(decision),
            decision.required_permission(),
            role.allowed_permissions(),
        ));
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

    // basis 缺省 = 源单实际实体解析（工件/凭证真实摘要，可过 OR2109）。
    let basis = basis.unwrap_or_else(|| resolved_basis(&order));
    // 凭证面缺省 = 呈报源单持有预算凭证（可过 OR2111）。
    let held_voucher = order
        .budget_voucher()
        .map(|voucher| voucher.voucher_id.clone())
        .ok_or_else(|| format!("源单 {order_id} 未绑预算凭证，无法组凭证面"))?;
    // 计划面 digest 与受理侧同一规范化现算（可过 OR2110）。
    let digest = canonical_next_orders_digest(&next_orders)
        .map_err(|reason| format!("--next-orders 不合法: {reason}"))?;

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
        role,
        &keypair.public_key(),
        key_expires_at,
    )])
    .map_err(|err| format!("注册表无效: {err}"))?;

    // 按 admission payload 真签（与 admit 链第③步引擎绑定值同源）。
    // permissions 取角色默认集（DecisionArtifact::seal 既有口径），
    // role/decision 与 --role/--decision 所选一致，进被签字节。
    let payload = order.admission_decision_payload();
    let decision = DecisionArtifact::seal(
        actor_id.clone(),
        role,
        decision,
        &payload,
        &keypair,
        DecisionAttributes::new(seq, basis, digest, expires),
    );
    let request = AdmissionRequest {
        order_id: order_id.clone(),
        decision: decision.clone(),
        actor_registry: registry.clone(),
        template: AdjudicationTemplate {
            template_id: "tpl-adjudication-v1".into(),
            template_version: "1.0".into(),
            doc_kind: "裁决单".into(),
            body_sha256: template_body_sha256(),
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
        next_orders,
        required_credentials: vec![held_voucher],
        validated_at: None,
    };
    let text = serde_json::to_string_pretty(&json!({
        "order_id": request.order_id,
        "decision": serde_json::to_value(&decision).map_err(|err| err.to_string())?,
        "actor_registry": serde_json::to_value(&registry).map_err(|err| err.to_string())?,
        "template": serde_json::to_value(&request.template).map_err(|err| err.to_string())?,
        "resource_profile": serde_json::to_value(&request.resource_profile).map_err(|err| err.to_string())?,
        "gate_list": serde_json::to_value(&request.gate_list).map_err(|err| err.to_string())?,
        "next_orders": serde_json::to_value(&request.next_orders).map_err(|err| err.to_string())?,
        "required_credentials": serde_json::to_value(&request.required_credentials).map_err(|err| err.to_string())?,
    }))
    .map_err(|err| err.to_string())?;
    fs::write(&out, format!("{text}\n")).map_err(|err| format!("落盘 {}: {err}", out.display()))?;

    println!(
        "{{\"admission_json\": {}, \"seed_hex\": {}, \"key_id\": {}, \"stable_id\": {}, \"role\": {}, \"decision\": {}, \"permissions\": {}, \"payload\": {}}}",
        json!(out.display().to_string()),
        json!(encode_hex(&seed)),
        json!(keypair.public_key().key_id()),
        json!(order.stable_id().as_str()),
        // 三个面取自签出的工件本身（不是再抄一遍参数）：role/permissions
        // 与 --role 所选一致、decision 与 --decision 所选一致即此处留证。
        json!(decision.role),
        json!(decision.decision),
        json!(decision.permissions),
        json!(payload),
    );
    Ok(())
}

/// --role 取值 → Role。只收可裁决角色：Reader 对任何决策都无权限
/// （can_decide 恒 false），开放它只会产出必然被拒的工件，不进工具面。
fn parse_role(text: &str) -> Result<Role, String> {
    match text {
        "approver" => Ok(Role::Approver),
        "reviser" => Ok(Role::Reviser),
        "arbiter" => Ok(Role::Arbiter),
        other => Err(format!(
            "--role 只接受 approver|reviser|arbiter（收到 {other:?}；reader 只读，对任何裁决都无权限，不开放）"
        )),
    }
}

/// --role 回显名（非法配对报错里与用户输入同形）。
fn role_name(role: Role) -> &'static str {
    match role {
        Role::Approver => "approver",
        Role::Reviser => "reviser",
        Role::Arbiter => "arbiter",
        Role::Reader => "reader",
    }
}

/// --decision 取值 → Decision。本工具面只收 approve|revise|reject 三种
/// 裁决结论；arbitrate（pivot）是 Arbiter 专属的第四种，归裁决环
/// adjudicate 腿的生产面，不在本联调签发面收值。
fn parse_decision(text: &str) -> Result<Decision, String> {
    match text {
        "approve" => Ok(Decision::Approve),
        "revise" => Ok(Decision::Revise),
        "reject" => Ok(Decision::Reject),
        other => Err(format!(
            "--decision 只接受 approve|revise|reject（收到 {other:?}）"
        )),
    }
}

/// --decision 回显名（非法配对报错里与用户输入同形）。
fn decision_name(decision: Decision) -> &'static str {
    match decision {
        Decision::Approve => "approve",
        Decision::Revise => "revise",
        Decision::Reject => "reject",
        Decision::Arbitrate => "arbitrate",
    }
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
