//! 多参与者决策工件:DecisionArtifact。
//!
//! 校验分四层,缺一层都拒绝:
//! 1. payload_hash 与被签的 payload 一致(防"签 A 批 B");
//! 2. permissions 不得超出角色能力上限(防"只读角色被塞进 Approve 权限"绕过);
//! 3. decision 所需权限必须在 permissions 中(防越权决策——ask 的核心用例:
//!    只读角色签发 approve → 拒绝);
//! 4. 占位签名非空且算法标签正确(ed25519 真实现留给最终方案,结构已定)。

use serde::{Deserialize, Serialize};

use crate::error::DecisionError;
use crate::hash::sha256_hex;

/// 权限位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    Read,
    Approve,
    Revise,
    Arbitrate,
}

/// 参与者角色。角色决定权限的能力上限(allowed_permissions)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    /// 只读:只能看
    Reader,
    /// 审批:可批准/驳回
    Approver,
    /// 修订:可提修改
    Reviser,
    /// 仲裁:全权
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
    /// 作出该决策所需的权限。Reject 与 Approve 同权(同样影响订单走向)。
    pub fn required_permission(self) -> Permission {
        match self {
            Decision::Approve | Decision::Reject => Permission::Approve,
            Decision::Revise => Permission::Revise,
            Decision::Arbitrate => Permission::Arbitrate,
        }
    }
}

/// 签名占位结构:字段形状对齐 ed25519(算法标签 + key id + 签名字节),
/// 真实加解密留给最终方案(如 ed25519-dalek),接口不变。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    pub algorithm: String,
    pub key_id: String,
    pub bytes: Vec<u8>,
}

pub const SIGNATURE_ALGORITHM_PLACEHOLDER: &str = "ed25519-placeholder";

impl Signature {
    pub fn placeholder(key_id: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            algorithm: SIGNATURE_ALGORITHM_PLACEHOLDER.into(),
            key_id: key_id.into(),
            bytes,
        }
    }

    /// 占位校验:算法标签正确 + 签名字节非空。不做真实密码学验证(诚实声明)。
    pub fn verify_placeholder(&self) -> bool {
        self.algorithm == SIGNATURE_ALGORITHM_PLACEHOLDER && !self.bytes.is_empty()
    }
}

/// 决策工件:一个参与者对一份 payload(payload_hash 锚定)作出的、带签名的决策。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionArtifact {
    pub actor_id: String,
    pub role: Role,
    pub permissions: Vec<Permission>,
    pub decision: Decision,
    /// 被签 payload 的哈希(SHA-256 小写 hex)。见 [`crate::hash`]。
    pub payload_hash: String,
    pub signature: Signature,
}

impl DecisionArtifact {
    /// 便捷签发:permissions 取角色默认集,payload_hash 就地计算。
    pub fn seal(
        actor_id: impl Into<String>,
        role: Role,
        decision: Decision,
        payload: &str,
        key_id: impl Into<String>,
    ) -> Self {
        Self {
            actor_id: actor_id.into(),
            permissions: role.default_permissions(),
            role,
            decision,
            payload_hash: sha256_hex(payload.as_bytes()),
            signature: Signature::placeholder(key_id, vec![0xde; 64]),
        }
    }

    /// 四层校验(顺序固定,便于错误可复现):hash → 角色能力上限 → 决策权限 → 占位签名。
    pub fn validate_against(&self, payload: &str) -> Result<(), DecisionError> {
        // ① payload 锚定
        let expected = sha256_hex(payload.as_bytes());
        if self.payload_hash != expected {
            return Err(DecisionError::PayloadHashMismatch {
                expected,
                actual: self.payload_hash.clone(),
            });
        }
        // ② 权限表不得越过角色能力上限
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
        // ③ 决策本身要有所需权限(ask 的核心用例在这一层拒绝)
        let required = self.decision.required_permission();
        if !self.permissions.contains(&required) {
            return Err(DecisionError::PermissionDenied {
                decision: self.decision,
                required,
                granted: self.permissions.clone(),
            });
        }
        // ④ 占位签名
        if !self.signature.verify_placeholder() {
            return Err(DecisionError::InvalidSignature {
                reason: format!("算法标签应为 {SIGNATURE_ALGORITHM_PLACEHOLDER} 或签名字节为空"),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAYLOAD: &str = "workorder:wo-9:release-request:v3";

    #[test]
    fn reader_issuing_approve_is_rejected() {
        let art = DecisionArtifact::seal("user-r", Role::Reader, Decision::Approve, PAYLOAD, "k1");
        let err = art.validate_against(PAYLOAD).unwrap_err();
        assert_eq!(
            err,
            DecisionError::PermissionDenied {
                decision: Decision::Approve,
                required: Permission::Approve,
                granted: vec![Permission::Read],
            }
        );
        let text = format!("{err}");
        assert!(text.contains("权限不足"), "错误应可读:{text}");
    }

    #[test]
    fn reader_with_forged_approve_permission_is_still_rejected_by_role_cap() {
        // 伪造:给只读角色直接塞 Approve 权限位 → 第②层拦截
        let art = DecisionArtifact {
            actor_id: "user-r".into(),
            role: Role::Reader,
            permissions: vec![Permission::Read, Permission::Approve],
            decision: Decision::Approve,
            payload_hash: sha256_hex(PAYLOAD.as_bytes()),
            signature: Signature::placeholder("k1", vec![1; 64]),
        };
        let err = art.validate_against(PAYLOAD).unwrap_err();
        assert!(matches!(err, DecisionError::RoleExceedsCapability { .. }));
    }

    #[test]
    fn approver_can_approve_when_hash_and_signature_ok() {
        let art =
            DecisionArtifact::seal("user-a", Role::Approver, Decision::Approve, PAYLOAD, "k2");
        assert_eq!(art.validate_against(PAYLOAD), Ok(()));
        assert_eq!(art.payload_hash.len(), crate::hash::SHA256_HEX_LEN);
        assert_eq!(art.payload_hash, sha256_hex(PAYLOAD.as_bytes()));
        assert_eq!(art.signature.algorithm, "ed25519-placeholder");
    }

    #[test]
    fn approver_can_reject_too() {
        let art = DecisionArtifact::seal("user-a", Role::Approver, Decision::Reject, PAYLOAD, "k2");
        assert_eq!(art.validate_against(PAYLOAD), Ok(()));
    }

    #[test]
    fn reviser_cannot_approve_but_can_revise() {
        let art = DecisionArtifact::seal("user-v", Role::Reviser, Decision::Approve, PAYLOAD, "k3");
        assert!(matches!(
            art.validate_against(PAYLOAD),
            Err(DecisionError::PermissionDenied { .. })
        ));
        let art = DecisionArtifact::seal("user-v", Role::Reviser, Decision::Revise, PAYLOAD, "k3");
        assert_eq!(art.validate_against(PAYLOAD), Ok(()));
    }

    #[test]
    fn arbiter_has_full_power() {
        for d in [
            Decision::Approve,
            Decision::Reject,
            Decision::Revise,
            Decision::Arbitrate,
        ] {
            let art = DecisionArtifact::seal("user-z", Role::Arbiter, d, PAYLOAD, "k4");
            assert_eq!(
                art.validate_against(PAYLOAD),
                Ok(()),
                "arbiter 应可作 {d:?}"
            );
        }
    }

    #[test]
    fn hash_mismatch_is_caught() {
        let art =
            DecisionArtifact::seal("user-a", Role::Approver, Decision::Approve, PAYLOAD, "k2");
        let err = art.validate_against("被掉包的 payload").unwrap_err();
        assert!(matches!(err, DecisionError::PayloadHashMismatch { .. }));
    }

    #[test]
    fn empty_signature_is_rejected() {
        let mut art =
            DecisionArtifact::seal("user-a", Role::Approver, Decision::Approve, PAYLOAD, "k2");
        art.signature.bytes.clear();
        assert!(matches!(
            art.validate_against(PAYLOAD),
            Err(DecisionError::InvalidSignature { .. })
        ));
    }

    #[test]
    fn decision_artifact_serde_roundtrip() {
        let art =
            DecisionArtifact::seal("user-a", Role::Approver, Decision::Approve, PAYLOAD, "k2");
        let json = serde_json::to_string(&art).unwrap();
        assert_eq!(
            serde_json::from_str::<DecisionArtifact>(&json).unwrap(),
            art
        );
    }
}
