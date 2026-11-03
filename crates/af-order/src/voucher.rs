//! 预算凭证签发：真 ed25519（af-workorder 原语），种子落订单目录（0600）。
//!
//! af-workorder 的 `Ed25519Keypair::from_seed` 不读系统熵（种子由调用方注入），
//! 本模块的熵源是 `/dev/urandom`（纯 std，不引 rand）。签发即自验：公钥重构 +
//! `verify_strict`，验不过 = 凭证不可信，立即失败。
//!
//! 重复 submit（同 order_id）：凭证材料已存在则原样读回，不重签、不换钥；
//! 盘上凭证与本单 order_id/amount 不符 = OR2003 拒绝——预算不可经重放偷改。

use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use af_workorder::{
    BudgetVoucher, Ed25519Keypair, Ed25519PublicKey, StableId, ED25519_SEED_LEN,
    SIGNATURE_ALGORITHM_ED25519,
};
use serde::{Deserialize, Serialize};

use crate::error::OrderError;
use crate::spec::OrderSpec;

/// 凭证被签 payload 的域分离前缀。
pub const VOUCHER_PAYLOAD_DOMAIN: &str = "af-order/voucher/v1";
/// 种子文件名（订单目录内，0600）。
pub const SEED_FILE: &str = "voucher-key.seed";
/// 凭证材料文件名（订单目录内）。
pub const VOUCHER_FILE: &str = "voucher.json";

/// 凭证材料（voucher.json 线格式）。签名字节存小写十六进制，便于人读与独立验签。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoucherRecord {
    pub voucher_id: String,
    pub order_id: String,
    pub stable_id: String,
    pub amount: u64,
    /// 产线绑定（红队 W1-C41：身份面字段一并钉死——凭证面对 amount 变更
    /// fail-loud（OR2003「预算不可经重放偷改」），line 同属身份面却无防线，
    /// 换 line 重放被静默吞掉；旧行无此键 = None，不核对（历史盘兼容）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_id: Option<String>,
    /// 产线版本（红队 W2-C-35：LineBinding 两字段同属身份面——spec 面
    /// check_line 两键都强制非空，凭证面却只钉 line_id，重放时只换
    /// line_version 即静默放行（bind_line 幂等跳过、订单绑定保持首驱版本、
    /// 零诊断）。与 line_id 同纪律 fail-loud；旧行无此键 = None，不核对
    /// （历史盘兼容）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_version: Option<String>,
    /// 任务包计划面 sha256（红队 W2-C55：graph/report 两臂全部预算帽
    /// （cost_cap/token_cap/节点 turns）都写在 plan.json 里，而 C-59 的
    /// digest 钉死只覆盖 task.json——提交后换一份同 graph_version、同
    /// registry_digest 的 plan.json 即可放宽预算且所有装订检查放行。凭证
    /// 面钉首绑摘要：重放时与当前计划面对账，不符即拒（与 line 同纪律）。
    /// 旧行无此键 = None，不核对（历史盘兼容）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub payload: String,
    pub algorithm: String,
    /// ed25519 公钥（32 字节小写十六进制）。
    pub key_id: String,
    /// 签名（64 字节小写十六进制）。
    pub signature_hex: String,
}

/// 被签 payload：域前缀 + 长度分隔的 order_id / stable_id / amount。
/// 长度前缀把字段隔开，字段值里出现分隔符也不改写边界（对齐 af-workorder
/// `decision_payload` 的做法）。
pub fn voucher_payload(order_id: &str, stable_id: &StableId, amount: u64) -> String {
    let sid = stable_id.as_str();
    format!(
        "{VOUCHER_PAYLOAD_DOMAIN}|{}|{order_id}|{}|{sid}|{amount}",
        order_id.len(),
        sid.len()
    )
}

/// 签发或读回预算凭证，返回（订单凭证，凭证材料）。
pub fn issue_or_load(
    order_dir: &Path,
    spec: &OrderSpec,
    stable_id: &StableId,
    plan_sha256: Option<&str>,
) -> Result<(BudgetVoucher, VoucherRecord), OrderError> {
    let voucher_path = order_dir.join(VOUCHER_FILE);
    if voucher_path.is_file() {
        let record = load_record(&voucher_path)?;
        if record.order_id != spec.order_id || record.amount != spec.budget {
            return Err(OrderError::Voucher {
                detail: format!(
                    "已有凭证与本单不符: 盘上 order_id={} amount={}，本次 order_id={} amount={}（预算不可经重放偷改）",
                    record.order_id, record.amount, spec.order_id, spec.budget
                ),
            });
        }
        // 红队 W1-C41：line 身份面核对——盘上有 line（新凭证）而本次不符
        // = 换线重放，与换预算同性质 fail-loud（OR2003 同码）。
        if let Some(recorded_line) = &record.line_id {
            if recorded_line != &spec.line.line_id {
                return Err(OrderError::Voucher {
                    detail: format!(
                        "已有凭证与本单不符: 盘上 line_id={}，本次 line_id={}（产线绑定不可经重放偷改——\
                         身份面字段与预算同纪律，红队 W1-C41）",
                        recorded_line, spec.line.line_id
                    ),
                });
            }
        }
        // 红队 W2-C-35：line_version 与 line_id 同属 LineBinding 身份面（spec
        // check_line 两键都强制非空）——只钉 line_id 时换版本重放被静默放行
        // （bind_line 幂等跳过、订单绑定保持首驱版本、零诊断），同性质 fail-loud。
        if let Some(recorded_version) = &record.line_version {
            if recorded_version != &spec.line.line_version {
                return Err(OrderError::Voucher {
                    detail: format!(
                        "已有凭证与本单不符: 盘上 line_version={}，本次 line_version={}（产线版本不可经重放偷改——\
                         LineBinding 两字段同属身份面，红队 W2-C-35）",
                        recorded_version, spec.line.line_version
                    ),
                });
            }
        }
        // 红队 W2-C55：plan 摘要身份面核对——盘上有摘要而本次不符 = 换计划
        // 重放（放宽预算的标准手法），与换预算/换线同性质 fail-loud。
        if let (Some(recorded), Some(current)) = (&record.plan_sha256, plan_sha256) {
            if recorded != current {
                return Err(OrderError::Voucher {
                    detail: format!(
                        "已有凭证与本单不符: 盘上 plan_sha256={}，本次 {}（任务包计划面不可经重放偷换——\
                         graph/report 预算帽全在 plan.json，红队 W2-C55）",
                        recorded, current
                    ),
                });
            }
        }
        return Ok((budget_voucher(&record), record));
    }

    let seed = load_or_create_seed(&order_dir.join(SEED_FILE))?;
    let keypair = Ed25519Keypair::from_seed(&seed);
    let payload = voucher_payload(&spec.order_id, stable_id, spec.budget);
    let signature = keypair.sign(payload.as_bytes());
    // 签发即自验：公钥重构 + verify_strict，不过 = 凭证不可信。
    let public = Ed25519PublicKey::from_bytes(&keypair.public_key().to_bytes()).map_err(|err| {
        OrderError::Voucher {
            detail: format!("公钥重构失败: {err}"),
        }
    })?;
    public
        .verify(payload.as_bytes(), &signature)
        .map_err(|err| OrderError::Voucher {
            detail: format!("签发自验失败: {err}"),
        })?;

    let record = VoucherRecord {
        voucher_id: format!("bv-{}", spec.order_id),
        order_id: spec.order_id.clone(),
        stable_id: stable_id.as_str().to_string(),
        amount: spec.budget,
        line_id: Some(spec.line.line_id.clone()),
        line_version: Some(spec.line.line_version.clone()),
        plan_sha256: plan_sha256.map(str::to_string),
        note: spec.note.clone(),
        payload,
        algorithm: SIGNATURE_ALGORITHM_ED25519.to_string(),
        key_id: signature.key_id.clone(),
        signature_hex: encode_hex(&signature.bytes),
    };
    let text = serde_json::to_string_pretty(&record).map_err(|err| OrderError::Voucher {
        detail: format!("凭证序列化失败: {err}"),
    })?;
    fs::write(&voucher_path, format!("{text}\n")).map_err(|err| OrderError::Voucher {
        detail: format!("凭证落盘失败 {}: {err}", voucher_path.display()),
    })?;
    Ok((budget_voucher(&record), record))
}

fn budget_voucher(record: &VoucherRecord) -> BudgetVoucher {
    BudgetVoucher {
        voucher_id: record.voucher_id.clone(),
        amount: record.amount,
        signed: true,
    }
}

fn load_record(path: &Path) -> Result<VoucherRecord, OrderError> {
    let text = fs::read_to_string(path).map_err(|err| OrderError::Voucher {
        detail: format!("凭证读取失败 {}: {err}", path.display()),
    })?;
    serde_json::from_str(&text).map_err(|err| OrderError::Voucher {
        detail: format!("凭证损坏 {}: {err}", path.display()),
    })
}

/// 读回或创建 32 字节 ed25519 种子。新种子 /dev/urandom 取熵，
/// `create_new + 0600` 落盘 + fsync；已存在但长度不对 = 拒绝（不猜）。
fn load_or_create_seed(path: &Path) -> Result<[u8; ED25519_SEED_LEN], OrderError> {
    if path.is_file() {
        let bytes = fs::read(path).map_err(|err| OrderError::Voucher {
            detail: format!("种子读取失败 {}: {err}", path.display()),
        })?;
        let len = bytes.len();
        return bytes.try_into().map_err(|_| OrderError::Voucher {
            detail: format!(
                "种子长度 {len} ≠ {ED25519_SEED_LEN}（{}），拒绝",
                path.display()
            ),
        });
    }
    let mut seed = [0u8; ED25519_SEED_LEN];
    let mut urandom = fs::File::open("/dev/urandom").map_err(|err| OrderError::Voucher {
        detail: format!("系统熵不可用: {err}"),
    })?;
    urandom
        .read_exact(&mut seed)
        .map_err(|err| OrderError::Voucher {
            detail: format!("系统熵读取失败: {err}"),
        })?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|err| OrderError::Voucher {
            detail: format!("种子创建失败 {}: {err}", path.display()),
        })?;
    file.write_all(&seed)
        .and_then(|()| file.sync_all())
        .map_err(|err| OrderError::Voucher {
            detail: format!("种子落盘失败: {err}"),
        })?;
    Ok(seed)
}

/// 小写十六进制编码（af_workorder::hash::encode_hex 是 pub(crate)，此处自带，
/// 纯 std 实现，不引依赖）。
fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_workorder::{Ed25519PublicKey, Signature};
    use serde_json::Value;

    fn spec(order_id: &str, budget: u64) -> OrderSpec {
        OrderSpec {
            order_id: order_id.into(),
            line: af_workorder::LineBinding {
                line_id: "line-poc".into(),
                line_version: "lv-1".into(),
            },
            budget,
            note: Some("骨架验收".into()),
        }
    }

    #[test]
    fn plan_digest_is_pinned_and_replay_checked() {
        // 红队 W2-C55：plan.json 摘要入凭证身份面——首绑建档，重放时与当前计划
        // 面对账，不符即拒（与换预算/换线同纪律）。graph/report 预算帽全在
        // plan.json，不钉计划面即可「同版本同注册表换计划」放宽预算。
        let dir = std::env::temp_dir().join(format!("af-voucher-w2c55-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let stable = af_workorder::StableId::generate("wo-plan-digest");
        let digest_a = "a".repeat(64);
        let digest_b = "b".repeat(64);

        let (_, record) =
            issue_or_load(&dir, &spec("wo-plan-digest", 42), &stable, Some(&digest_a)).unwrap();
        assert_eq!(
            record.plan_sha256.as_deref(),
            Some(digest_a.as_str()),
            "首绑摘要应入凭证材料"
        );
        // 同摘要重放：照过（幂等）。
        issue_or_load(&dir, &spec("wo-plan-digest", 42), &stable, Some(&digest_a))
            .expect("同计划面重放应放行");
        // 换计划面重放：拒（OR2003 同码）。
        let err = issue_or_load(&dir, &spec("wo-plan-digest", 42), &stable, Some(&digest_b))
            .expect_err("换 plan.json 重放应拒");
        assert!(
            err.to_string().contains("plan_sha256"),
            "拒因须点名计划面摘要：{err}"
        );
        // 旧凭证无此键（历史盘兼容）：None 不核对。
        let legacy = dir.join("legacy");
        std::fs::create_dir_all(&legacy).unwrap();
        let (_, rec) = issue_or_load(&legacy, &spec("wo-plan-digest", 42), &stable, None).unwrap();
        assert_eq!(rec.plan_sha256, None, "未提供摘要 = 不建档不核对");
        issue_or_load(&legacy, &spec("wo-plan-digest", 42), &stable, None).expect("无摘要面不核对");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("af-order-voucher-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// payload 域分离 + 长度前缀：字段含分隔符也不改写边界。
    #[test]
    fn voucher_payload_is_domain_separated_and_length_prefixed() {
        let stable = StableId::generate("wo-1");
        let payload = voucher_payload("wo|1", &stable, 5);
        assert!(payload.starts_with("af-order/voucher/v1|"), "{payload}");
        assert!(payload.contains("|4|wo|1|"), "order_id 长度前缀: {payload}");
        assert!(
            payload.contains(&format!("|{}|", stable.as_str().len())),
            "{payload}"
        );
        assert!(payload.ends_with("|5"), "{payload}");
    }

    /// 签发 → 材料落盘、种子 0600、外部可独立验签；再签 = 读回同一份。
    #[test]
    fn issue_persists_and_second_load_returns_same_record() {
        let dir = scratch("issue");
        let stable = StableId::generate("wo-v1");
        let (voucher, record) = issue_or_load(&dir, &spec("wo-v1", 42), &stable, None).unwrap();
        assert_eq!(voucher.voucher_id, "bv-wo-v1");
        assert_eq!(voucher.amount, 42);
        assert!(voucher.signed, "签发即 signed=true（release 门材料）");

        let on_disk: Value =
            serde_json::from_str(&fs::read_to_string(dir.join(VOUCHER_FILE)).unwrap()).unwrap();
        assert_eq!(on_disk["signature_hex"], record.signature_hex);
        let key_id = on_disk["key_id"].as_str().unwrap();
        let signature = Signature {
            algorithm: "ed25519".into(),
            key_id: key_id.to_string(),
            bytes: decode_hex(on_disk["signature_hex"].as_str().unwrap()),
        };
        Ed25519PublicKey::from_bytes(&decode_hex(key_id).try_into().unwrap())
            .unwrap()
            .verify(on_disk["payload"].as_str().unwrap().as_bytes(), &signature)
            .expect("盘上凭证可独立验签");

        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(dir.join(SEED_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "种子必须 0600");

        let (_, again) = issue_or_load(&dir, &spec("wo-v1", 42), &stable, None).unwrap();
        assert_eq!(again, record, "重复签发读回同一份材料，不重签");

        let err = issue_or_load(&dir, &spec("wo-v1", 43), &stable, None).unwrap_err();
        assert!(err.to_string().contains("OR2003"), "{err}");
        assert!(err.to_string().contains("43"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    fn decode_hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }
}
