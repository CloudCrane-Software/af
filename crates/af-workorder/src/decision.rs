//! 多参与者决策工件：DecisionArtifact。
//!
//! 校验分四层，缺一层都拒绝，顺序固定：
//! 1. `payload_hash` 与被签的业务 payload 一致（防“签 A 批 B”）；
//! 2. `permissions` 不得超出角色能力上限；
//! 3. `decision` 所需权限必须在 `permissions` 中；
//! 4. 用**外部受信 actor 注册表**里的 ed25519 公钥做真验签。
//!
//! v2 把 actor、角色、权限、决策结论以及 `decision_seq` / `basis` /
//! `next_orders_digest` / `expires_at` 全部纳入被签字节，不能只签 payload 后
//! 允许调用方改写“签的是什么决定”。工件只带 `key_id`，绝不携带公钥；actor、角色、公钥与
//! key 有效期均来自注册表。调用方注入 `validated_at`，避免用工件自带时间
//! 给自己的过期 key 续命。

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::DecisionError;
use crate::hash::{encode_hex, sha256_hex};
use crate::sign::{Ed25519Keypair, Ed25519PublicKey};

/// 权限位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    Read,
    Approve,
    Revise,
    Arbitrate,
}

/// 参与者角色。角色决定权限的能力上限（allowed_permissions）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    /// 只读：只能看。
    Reader,
    /// 审批：可批准/驳回。
    Approver,
    /// 修订：可提修改。
    Reviser,
    /// 仲裁：全权。
    Arbiter,
}

impl Role {
    pub fn allowed_permissions(self) -> &'static [Permission] {
        match self {
            Role::Reader => &[Permission::Read],
            Role::Approver => &[Permission::Read, Permission::Approve],
            Role::Reviser => &[Permission::Read, Permission::Revise],
            Role::Arbiter => &[
                Permission::Read,
                Permission::Approve,
                Permission::Revise,
                Permission::Arbitrate,
            ],
        }
    }

    pub fn default_permissions(self) -> Vec<Permission> {
        self.allowed_permissions().to_vec()
    }
}

/// 决策结论。每种决策对应一种必需权限。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Decision {
    Approve,
    Reject,
    Revise,
    Arbitrate,
}

impl Decision {
    /// 作出该决策所需的权限。Reject 与 Approve 同权（同样影响订单走向）。
    pub fn required_permission(self) -> Permission {
        match self {
            Decision::Approve | Decision::Reject => Permission::Approve,
            Decision::Revise => Permission::Revise,
            Decision::Arbitrate => Permission::Arbitrate,
        }
    }
}

/// ed25519 签名。公钥不在这个结构里；`key_id` 只用于查受信注册表。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signature {
    pub algorithm: String,
    pub key_id: String,
    pub bytes: Vec<u8>,
}

/// 仅用于构造“占位算法必须拒绝”的负例，不是可接受的签发路径。
pub const SIGNATURE_ALGORITHM_PLACEHOLDER: &str = "ed25519-placeholder";

impl Signature {
    /// 保留给迁移测试/负例构造；生产签发请用 [`Ed25519Keypair::sign`]。
    pub fn placeholder(key_id: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            algorithm: SIGNATURE_ALGORITHM_PLACEHOLDER.into(),
            key_id: key_id.into(),
            bytes,
        }
    }
}

/// 纳入决策签名的四个扩展字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionAttributes {
    pub decision_seq: u64,
    pub basis: Vec<String>,
    pub next_orders_digest: String,
    pub expires_at: DateTime<Utc>,
}

impl DecisionAttributes {
    pub fn new(
        decision_seq: u64,
        basis: Vec<String>,
        next_orders_digest: impl Into<String>,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            decision_seq,
            basis,
            next_orders_digest: next_orders_digest.into(),
            expires_at,
        }
    }
}

/// 注册表中的一个受信 actor。
///
/// `public_key_hex` 来自运营者管理的注册表/凭证存储，而不是 DecisionArtifact。
/// `key_id` 不入线格式，始终从公钥字节重算，防止注册表内两个字段漂移。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedActor {
    pub actor_id: String,
    pub role: Role,
    /// 32 字节 ed25519 公钥的小写十六进制。
    pub public_key_hex: String,
    /// 此 key 的失效时刻；恰在该时刻即拒绝。
    pub key_expires_at: DateTime<Utc>,
}

impl TrustedActor {
    pub fn new(
        actor_id: impl Into<String>,
        role: Role,
        public_key: &Ed25519PublicKey,
        key_expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            actor_id: actor_id.into(),
            role,
            public_key_hex: encode_hex(&public_key.to_bytes()),
            key_expires_at,
        }
    }

    fn public_key(&self) -> Result<Ed25519PublicKey, DecisionError> {
        let raw = decode_hex_32(&self.public_key_hex).map_err(|reason| {
            DecisionError::InvalidActorRegistry {
                reason: format!("actor {} 的公钥 {reason}", self.actor_id),
            }
        })?;
        Ed25519PublicKey::from_bytes(&raw).map_err(|err| DecisionError::InvalidActorRegistry {
            reason: format!("actor {} 的公钥无效: {err}", self.actor_id),
        })
    }

    fn key_id(&self) -> Result<String, DecisionError> {
        Ok(self.public_key()?.key_id())
    }
}

/// 受信 actor 注册表。重复 actor / 重复公钥、空白身份和坏公钥全部 fail-closed。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedActorRegistry {
    pub actors: Vec<TrustedActor>,
}

impl TrustedActorRegistry {
    pub fn new(actors: Vec<TrustedActor>) -> Result<Self, DecisionError> {
        let registry = Self { actors };
        registry.validate()?;
        Ok(registry)
    }

    pub fn validate(&self) -> Result<(), DecisionError> {
        let mut actor_ids = BTreeSet::new();
        let mut key_ids = BTreeSet::new();
        for actor in &self.actors {
            if actor.actor_id.trim().is_empty()
                || actor.actor_id.chars().any(char::is_whitespace)
            {
                return Err(DecisionError::InvalidActorRegistry {
                    reason: "actor_id 为空或含空白".into(),
                });
            }
            if !actor_ids.insert(actor.actor_id.clone()) {
                return Err(DecisionError::InvalidActorRegistry {
                    reason: format!("重复 actor_id: {}", actor.actor_id),
                });
            }
            let key_id = actor.key_id()?;
            if !key_ids.insert(key_id.clone()) {
                return Err(DecisionError::InvalidActorRegistry {
                    reason: format!("公钥重复绑定到多个 actor: {key_id}"),
                });
            }
        }
        Ok(())
    }

    fn actor(&self, actor_id: &str) -> Result<&TrustedActor, DecisionError> {
        self.actors
            .iter()
            .find(|actor| actor.actor_id == actor_id)
            .ok_or_else(|| DecisionError::UnknownActor {
                actor_id: actor_id.to_string(),
            })
    }
}

/// v2 决策签名域。域标签与长度明确的 JSON 字段共同防止和 change-order /
/// voucher 签名串用；身份、权限、结论、四个扩展字段与业务 payload 在同一份被签字节里。
pub const DECISION_SIGNING_DOMAIN: &str = "af-workorder/decision/v2";

#[derive(Serialize)]
struct DecisionSigningPayload<'a> {
    domain: &'static str,
    actor_id: &'a str,
    role: Role,
    permissions: &'a [Permission],
    decision: Decision,
    payload: &'a str,
    decision_seq: u64,
    basis: &'a [String],
    next_orders_digest: &'a str,
    expires_at: &'a DateTime<Utc>,
}

fn signing_bytes(
    actor_id: &str,
    role: Role,
    permissions: &[Permission],
    decision: Decision,
    payload: &str,
    attributes: &DecisionAttributes,
) -> Result<Vec<u8>, DecisionError> {
    serde_json::to_vec(&DecisionSigningPayload {
        domain: DECISION_SIGNING_DOMAIN,
        actor_id,
        role,
        permissions,
        decision,
        payload,
        decision_seq: attributes.decision_seq,
        basis: &attributes.basis,
        next_orders_digest: &attributes.next_orders_digest,
        expires_at: &attributes.expires_at,
    })
    .map_err(|err| DecisionError::SigningPayload {
        reason: err.to_string(),
    })
}

/// 决策工件：一个参与者对一份 payload（payload_hash 锚定）作出的、带签名的决策。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionArtifact {
    pub actor_id: String,
    pub role: Role,
    pub permissions: Vec<Permission>,
    pub decision: Decision,
    /// 被签业务 payload 的哈希（SHA-256 小写 hex）。
    pub payload_hash: String,
    pub decision_seq: u64,
    pub basis: Vec<String>,
    pub next_orders_digest: String,
    pub expires_at: DateTime<Utc>,
    pub signature: Signature,
}

impl DecisionArtifact {
    /// 用真 ed25519 签发。permissions 取角色默认集，payload_hash 就地计算；
    /// 四个扩展字段与 payload 一起进入被签字节。
    pub fn seal(
        actor_id: impl Into<String>,
        role: Role,
        decision: Decision,
        payload: &str,
        keypair: &Ed25519Keypair,
        attributes: DecisionAttributes,
    ) -> Self {
        let actor_id = actor_id.into();
        let permissions = role.default_permissions();
        let bytes = signing_bytes(
            &actor_id,
            role,
            &permissions,
            decision,
            payload,
            &attributes,
        )
        .expect("DecisionSigningPayload 只含可序列化字段");
        Self {
            actor_id,
            permissions,
            role,
            decision,
            payload_hash: sha256_hex(payload.as_bytes()),
            decision_seq: attributes.decision_seq,
            basis: attributes.basis,
            next_orders_digest: attributes.next_orders_digest,
            expires_at: attributes.expires_at,
            signature: keypair.sign(&bytes),
        }
    }

    fn attributes(&self) -> DecisionAttributes {
        DecisionAttributes {
            decision_seq: self.decision_seq,
            basis: self.basis.clone(),
            next_orders_digest: self.next_orders_digest.clone(),
            expires_at: self.expires_at,
        }
    }

    /// 四层校验（顺序固定）：hash → 角色能力上限 → 决策权限 → 注册表真验签。
    pub fn validate_against(
        &self,
        payload: &str,
        registry: &TrustedActorRegistry,
        validated_at: DateTime<Utc>,
    ) -> Result<(), DecisionError> {
        // ① payload 锚定。
        let expected = sha256_hex(payload.as_bytes());
        if self.payload_hash != expected {
            return Err(DecisionError::PayloadHashMismatch {
                expected,
                actual: self.payload_hash.clone(),
            });
        }
        // ② 权限表不得越过角色能力上限。
        let role_allows = self.role.allowed_permissions();
        for &p in &self.permissions {
            if !role_allows.contains(&p) {
                return Err(DecisionError::RoleExceedsCapability {
                    role: self.role,
                    offending: p,
                    role_allows: role_allows.to_vec(),
                });
            }
        }
        // ③ 决策本身要有所需权限。
        let required = self.decision.required_permission();
        if !self.permissions.contains(&required) {
            return Err(DecisionError::PermissionDenied {
                decision: self.decision,
                required,
                granted: self.permissions.clone(),
            });
        }
        // ④ 注册表身份/角色/key 有效期 + 真 ed25519 verify_strict。
        registry.validate()?;
        let actor = registry.actor(&self.actor_id)?;
        if actor.role != self.role {
            return Err(DecisionError::ActorRoleMismatch {
                actor_id: self.actor_id.clone(),
                registered: actor.role,
                claimed: self.role,
            });
        }
        let registered_key_id = actor.key_id()?;
        if self.signature.key_id != registered_key_id {
            return Err(DecisionError::KeyNotTrusted {
                actor_id: self.actor_id.clone(),
                key_id: self.signature.key_id.clone(),
            });
        }
        if validated_at >= actor.key_expires_at {
            return Err(DecisionError::ExpiredKey {
                actor_id: self.actor_id.clone(),
                key_id: registered_key_id,
                key_expires_at: actor.key_expires_at,
                validated_at,
            });
        }
        let message = signing_bytes(
            &self.actor_id,
            self.role,
            &self.permissions,
            self.decision,
            payload,
            &self.attributes(),
        )?;
        actor
            .public_key()?
            .verify(&message, &self.signature)
            .map_err(|source| DecisionError::InvalidSignature { source })
    }
}

fn decode_hex_32(text: &str) -> Result<[u8; 32], String> {
    if text.len() != 64 {
        return Err(format!("长度 {} ≠ 64 hex", text.len()));
    }
    if !text
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("必须是 32 字节小写 hex".into());
    }
    let mut out = [0u8; 32];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
            .map_err(|err| err.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SignError;
    use crate::sign::SIGNATURE_ALGORITHM_ED25519;

    const PAYLOAD: &str = "workorder:wo-9:release-request:v3";
    const NOW: &str = "2026-11-03T00:00:00Z";
    const KEY_EXPIRY: &str = "2027-01-01T00:00:00Z";
    const DECISION_EXPIRY: &str = "2026-11-04T00:00:00Z";
    const SEED: [u8; 32] = [0x31; 32];

    fn time(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn attributes() -> DecisionAttributes {
        DecisionAttributes::new(
            7,
            vec!["artifact:fa-1:qualified".into()],
            "a".repeat(64),
            time(DECISION_EXPIRY),
        )
    }

    fn keypair() -> Ed25519Keypair {
        Ed25519Keypair::from_seed(&SEED)
    }

    fn registry() -> TrustedActorRegistry {
        TrustedActorRegistry::new(vec![TrustedActor::new(
            "user-a",
            Role::Approver,
            &keypair().public_key(),
            time(KEY_EXPIRY),
        )])
        .unwrap()
    }

    fn approver() -> DecisionArtifact {
        DecisionArtifact::seal(
            "user-a",
            Role::Approver,
            Decision::Approve,
            PAYLOAD,
            &keypair(),
            attributes(),
        )
    }

    fn validate(artifact: &DecisionArtifact) -> Result<(), DecisionError> {
        artifact.validate_against(PAYLOAD, &registry(), time(NOW))
    }

    #[test]
    fn reader_issuing_approve_is_rejected_before_signature_lookup() {
        let art = DecisionArtifact::seal(
            "user-r",
            Role::Reader,
            Decision::Approve,
            PAYLOAD,
            &keypair(),
            attributes(),
        );
        let err = art
            .validate_against(PAYLOAD, &registry(), time(NOW))
            .unwrap_err();
        assert_eq!(
            err,
            DecisionError::PermissionDenied {
                decision: Decision::Approve,
                required: Permission::Approve,
                granted: vec![Permission::Read],
            }
        );
    }

    #[test]
    fn reader_with_forged_approve_permission_is_rejected_by_role_cap() {
        let mut art = approver();
        art.actor_id = "user-r".into();
        art.role = Role::Reader;
        art.permissions = vec![Permission::Read, Permission::Approve];
        assert!(matches!(
            validate(&art),
            Err(DecisionError::RoleExceedsCapability { .. })
        ));
    }

    #[test]
    fn approver_can_approve_with_real_signature_and_signed_v2_fields() {
        let art = approver();
        assert_eq!(validate(&art), Ok(()));
        assert_eq!(art.payload_hash, sha256_hex(PAYLOAD.as_bytes()));
        assert_eq!(art.signature.algorithm, SIGNATURE_ALGORITHM_ED25519);
        assert_eq!(art.signature.bytes.len(), 64);
        assert_eq!(art.decision_seq, 7);
        assert_eq!(art.basis, attributes().basis);
        assert_eq!(art.next_orders_digest, "a".repeat(64));
    }

    #[test]
    fn wrong_key_and_self_attested_key_are_rejected() {
        let wrong = Ed25519Keypair::from_seed(&[0x42; 32]);
        let art = DecisionArtifact::seal(
            "user-a",
            Role::Approver,
            Decision::Approve,
            PAYLOAD,
            &wrong,
            attributes(),
        );
        assert!(matches!(
            validate(&art),
            Err(DecisionError::KeyNotTrusted { .. })
        ));

        // 把工件里的 key_id 改回受信值也不够：签名字节仍来自错 key。
        let mut relabeled = art;
        relabeled.signature.key_id = keypair().public_key().key_id();
        assert_eq!(
            validate(&relabeled),
            Err(DecisionError::InvalidSignature {
                source: SignError::VerificationFailed
            })
        );
    }

    #[test]
    fn placeholder_algorithm_is_rejected() {
        let mut art = approver();
        art.signature.algorithm = SIGNATURE_ALGORITHM_PLACEHOLDER.into();
        assert_eq!(
            validate(&art),
            Err(DecisionError::InvalidSignature {
                source: SignError::AlgorithmMismatch {
                    expected: SIGNATURE_ALGORITHM_ED25519.into(),
                    actual: SIGNATURE_ALGORITHM_PLACEHOLDER.into(),
                }
            })
        );
    }

    #[test]
    fn tampered_payload_and_signed_fields_are_rejected() {
        let art = approver();
        assert!(matches!(
            art.validate_against("被掉包的 payload", &registry(), time(NOW)),
            Err(DecisionError::PayloadHashMismatch { .. })
        ));

        // 攻击者重算第一层 hash，但没有私钥，第四层仍必须红。
        let mut rehashed = art.clone();
        rehashed.payload_hash = sha256_hex("被掉包的 payload".as_bytes());
        assert_eq!(
            rehashed.validate_against("被掉包的 payload", &registry(), time(NOW)),
            Err(DecisionError::InvalidSignature {
                source: SignError::VerificationFailed
            })
        );

        for tampered in [
            {
                let mut art = art.clone();
                art.decision = Decision::Reject;
                art
            },
            {
                let mut art = art.clone();
                art.permissions.reverse();
                art
            },
            {
                let mut art = art.clone();
                art.decision_seq += 1;
                art
            },
            {
                let mut art = art.clone();
                art.basis[0].push_str(":tampered");
                art
            },
            {
                let mut art = art.clone();
                art.next_orders_digest = "b".repeat(64);
                art
            },
            {
                let mut art = art.clone();
                art.expires_at += chrono::Duration::seconds(1);
                art
            },
        ] {
            assert_eq!(
                validate(&tampered),
                Err(DecisionError::InvalidSignature {
                    source: SignError::VerificationFailed
                })
            );
        }
    }

    #[test]
    fn expired_key_unknown_actor_and_role_switch_are_structured_rejections() {
        let art = approver();
        assert!(matches!(
            art.validate_against(PAYLOAD, &registry(), time(KEY_EXPIRY)),
            Err(DecisionError::ExpiredKey { .. })
        ));

        let mut unknown = art.clone();
        unknown.actor_id = "nobody".into();
        assert!(matches!(
            validate(&unknown),
            Err(DecisionError::UnknownActor { .. })
        ));

        let elevated = DecisionArtifact::seal(
            "user-a",
            Role::Arbiter,
            Decision::Arbitrate,
            PAYLOAD,
            &keypair(),
            attributes(),
        );
        assert!(matches!(
            validate(&elevated),
            Err(DecisionError::ActorRoleMismatch { .. })
        ));
    }

    #[test]
    fn malformed_empty_and_tampered_signatures_are_rejected() {
        let mut empty = approver();
        empty.signature.bytes.clear();
        assert!(matches!(
            validate(&empty),
            Err(DecisionError::InvalidSignature {
                source: SignError::MalformedSignature { .. }
            })
        ));
        let mut flipped = approver();
        flipped.signature.bytes[0] ^= 1;
        assert_eq!(
            validate(&flipped),
            Err(DecisionError::InvalidSignature {
                source: SignError::VerificationFailed
            })
        );
    }

    #[test]
    fn artifact_cannot_smuggle_its_own_public_key() {
        let mut value = serde_json::to_value(approver()).unwrap();
        value.as_object_mut().unwrap().insert(
            "public_key_hex".into(),
            serde_json::Value::String(keypair().public_key().key_id()),
        );
        let err = serde_json::from_value::<DecisionArtifact>(value).unwrap_err();
        assert!(err.to_string().contains("unknown field"), "{err}");
    }

    #[test]
    fn registry_rejects_duplicate_actor_and_bad_key() {
        let actor = TrustedActor::new(
            "user-a",
            Role::Approver,
            &keypair().public_key(),
            time(KEY_EXPIRY),
        );
        assert!(matches!(
            TrustedActorRegistry::new(vec![actor.clone(), actor.clone()]),
            Err(DecisionError::InvalidActorRegistry { .. })
        ));
        let mut bad = actor;
        bad.public_key_hex = "00".repeat(32);
        assert!(matches!(
            TrustedActorRegistry::new(vec![bad]),
            Err(DecisionError::InvalidActorRegistry { .. })
        ));
    }

    #[test]
    fn decision_artifact_serde_roundtrip() {
        let art = approver();
        let json = serde_json::to_string(&art).unwrap();
        assert_eq!(
            serde_json::from_str::<DecisionArtifact>(&json).unwrap(),
            art
        );
        assert_eq!(art.signature.algorithm, SIGNATURE_ALGORITHM_ED25519);
    }
}
