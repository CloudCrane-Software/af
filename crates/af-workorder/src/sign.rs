//! ed25519 签名。
//!
//! 签名能力从凭证层下沉到本 crate:变更单在这里签发并验签,不调用 af-credential,
//! 也不改变该 crate 的公共 API。原语是 ed25519-dalek 2.2,与 biscuit-auth
//! (af-credential 的签名实现)同一锁版本;按密钥材料复制下沉,而不是把订单签名绕进 biscuit token。
//!
//! 种子由调用方注入。[`Ed25519Keypair::from_seed`] 不读取系统熵。

use std::fmt;

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};

use crate::decision::Signature;
use crate::error::SignError;
use crate::hash::encode_hex;

/// 真实 ed25519 签名的算法标签。与决策工件的占位标签 `ed25519-placeholder` 区分。
pub const SIGNATURE_ALGORITHM_ED25519: &str = "ed25519";

/// ed25519 签名字节长度。
pub const ED25519_SIGNATURE_LEN: usize = 64;
/// ed25519 公钥字节长度。
pub const ED25519_PUBLIC_KEY_LEN: usize = 32;
/// ed25519 种子长度。传给 [`Ed25519Keypair::from_seed`] 的就是这 32 字节。
pub const ED25519_SEED_LEN: usize = 32;

/// 签名密钥。Debug 只打印公钥标识,不打印种子。
#[derive(Clone)]
pub struct Ed25519Keypair {
    signing: SigningKey,
}

impl Ed25519Keypair {
    /// 由调用方注入的 32 字节种子构造。相同种子得到相同密钥与确定性签名。
    pub fn from_seed(seed: &[u8; ED25519_SEED_LEN]) -> Self {
        Self {
            signing: SigningKey::from_bytes(seed),
        }
    }

    pub fn public_key(&self) -> Ed25519PublicKey {
        Ed25519PublicKey {
            key: self.signing.verifying_key(),
        }
    }

    /// 对 `message` 做 ed25519 签名。ed25519 本身确定性:同一密钥、同一消息,签名字节相同。
    pub fn sign(&self, message: &[u8]) -> Signature {
        let sig = self.signing.sign(message);
        Signature {
            algorithm: SIGNATURE_ALGORITHM_ED25519.to_string(),
            key_id: self.public_key().key_id(),
            bytes: sig.to_bytes().to_vec(),
        }
    }
}

impl fmt::Debug for Ed25519Keypair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ed25519Keypair")
            .field("public_key", &self.public_key().key_id())
            .finish_non_exhaustive()
    }
}

/// 验签公钥。`key_id` 是公钥字节的小写十六进制,签名里的 `key_id` 必须与它一致。
#[derive(Clone, Copy)]
pub struct Ed25519PublicKey {
    key: VerifyingKey,
}

impl Ed25519PublicKey {
    /// 从 32 字节公钥构造。不在曲线上,或是小阶(weak)公钥,都拒绝。
    pub fn from_bytes(bytes: &[u8; ED25519_PUBLIC_KEY_LEN]) -> Result<Self, SignError> {
        let key = VerifyingKey::from_bytes(bytes).map_err(|_| SignError::InvalidPublicKey)?;
        if key.is_weak() {
            return Err(SignError::InvalidPublicKey);
        }
        Ok(Self { key })
    }

    pub fn to_bytes(&self) -> [u8; ED25519_PUBLIC_KEY_LEN] {
        self.key.to_bytes()
    }

    pub fn key_id(&self) -> String {
        encode_hex(&self.to_bytes())
    }

    /// 验签。顺序:算法标签 → key_id → 长度 → ed25519 严格校验(拒绝可锻签名与小阶公钥)。
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), SignError> {
        if signature.algorithm != SIGNATURE_ALGORITHM_ED25519 {
            return Err(SignError::AlgorithmMismatch {
                expected: SIGNATURE_ALGORITHM_ED25519.to_string(),
                actual: signature.algorithm.clone(),
            });
        }
        let key_id = self.key_id();
        if signature.key_id != key_id {
            return Err(SignError::KeyIdMismatch {
                expected: key_id,
                actual: signature.key_id.clone(),
            });
        }
        if signature.bytes.len() != ED25519_SIGNATURE_LEN {
            return Err(SignError::MalformedSignature {
                expected: ED25519_SIGNATURE_LEN,
                actual: signature.bytes.len(),
            });
        }
        let mut raw = [0u8; ED25519_SIGNATURE_LEN];
        raw.copy_from_slice(&signature.bytes);
        let sig = ed25519_dalek::Signature::from_bytes(&raw);
        self.key
            .verify_strict(message, &sig)
            .map_err(|_| SignError::VerificationFailed)
    }
}

impl fmt::Debug for Ed25519PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ed25519PublicKey")
            .field("key_id", &self.key_id())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED_A: [u8; 32] = [
        0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc,
        0xfe, 0x0f, 0x1e, 0x2d, 0x3c, 0x4b, 0x5a, 0x69, 0x78, 0x87, 0x96, 0xa5, 0xb4, 0xc3, 0xd2,
        0xe1, 0xf0,
    ];
    const SEED_B: [u8; 32] = [0x42; 32];
    const MSG: &[u8] = b"af-workorder change-order fixture";

    #[test]
    fn sign_verify_roundtrip_is_deterministic_and_debug_hides_seed() {
        let key = Ed25519Keypair::from_seed(&SEED_A);
        let again = Ed25519Keypair::from_seed(&SEED_A);
        let sig = key.sign(MSG);
        assert_eq!(sig, again.sign(MSG));
        assert_eq!(sig.algorithm, SIGNATURE_ALGORITHM_ED25519);
        assert_eq!(sig.bytes.len(), ED25519_SIGNATURE_LEN);
        assert_eq!(sig.key_id, key.public_key().key_id());
        assert_eq!(sig.key_id.len(), ED25519_PUBLIC_KEY_LEN * 2);

        key.public_key().verify(MSG, &sig).unwrap();
        let restored = Ed25519PublicKey::from_bytes(&key.public_key().to_bytes()).unwrap();
        restored.verify(MSG, &sig).unwrap();

        let rendered = format!("{key:?}");
        let seed_hex = encode_hex(&SEED_A);
        assert!(
            !rendered.contains(&seed_hex),
            "Debug 不得泄漏种子: {rendered}"
        );
    }

    #[test]
    fn rejects_tamper_wrong_key_bad_algorithm_and_invalid_public_key() {
        let key_a = Ed25519Keypair::from_seed(&SEED_A);
        let key_b = Ed25519Keypair::from_seed(&SEED_B);
        let pk_a = key_a.public_key();
        let pk_b = key_b.public_key();
        assert_ne!(pk_a.key_id(), pk_b.key_id());
        let sig = key_a.sign(MSG);

        let mut flipped = sig.clone();
        flipped.bytes[0] ^= 0x01;
        assert_eq!(
            pk_a.verify(MSG, &flipped).unwrap_err(),
            SignError::VerificationFailed
        );

        assert!(matches!(
            pk_b.verify(MSG, &sig).unwrap_err(),
            SignError::KeyIdMismatch { .. }
        ));

        // key_id 改成 B,签名字节仍是 A 的:必须走到密码学校验并失败,不能只信 key_id。
        let mut relabeled = sig.clone();
        relabeled.key_id = pk_b.key_id();
        assert_eq!(
            pk_b.verify(MSG, &relabeled).unwrap_err(),
            SignError::VerificationFailed
        );

        let mut placeholder = sig.clone();
        placeholder.algorithm = crate::decision::SIGNATURE_ALGORITHM_PLACEHOLDER.to_string();
        assert!(matches!(
            pk_a.verify(MSG, &placeholder).unwrap_err(),
            SignError::AlgorithmMismatch { .. }
        ));

        let mut empty = sig.clone();
        empty.bytes.clear();
        assert_eq!(
            pk_a.verify(MSG, &empty).unwrap_err(),
            SignError::MalformedSignature {
                expected: ED25519_SIGNATURE_LEN,
                actual: 0,
            }
        );

        // 合法公钥的最低位翻转后离开曲线,必须拒绝。全零是小阶(单位元)公钥,同样拒绝。
        let mut off_curve = key_a.public_key().to_bytes();
        off_curve[0] ^= 0x01;
        assert_eq!(
            Ed25519PublicKey::from_bytes(&off_curve).unwrap_err(),
            SignError::InvalidPublicKey
        );
        assert_eq!(
            Ed25519PublicKey::from_bytes(&[0; 32]).unwrap_err(),
            SignError::InvalidPublicKey
        );

        let mut other_msg = MSG.to_vec();
        other_msg[0] ^= 0x01;
        assert_eq!(
            pk_a.verify(&other_msg, &sig).unwrap_err(),
            SignError::VerificationFailed
        );
    }
}
