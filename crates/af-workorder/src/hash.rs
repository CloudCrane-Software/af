//! payload_hash:SHA-256。
//!
//! 替换原先的 FNV-1a64 占位。SHA-256 跨进程确定,且具备抗碰撞性:
//! 不同 payload 的哈希必须不同,决策校验才能挡住"签 A 批 B"。
//! 实现用 `sha2` 0.10,与 af-credential 传递依赖里 ed25519-dalek 使用的
//! 同一主版本一致,不另引入 blake3。
//!
//! [`crate::decision::DecisionArtifact`] 的字段不变,仍是小写十六进制字符串。

use sha2::{Digest, Sha256};

/// SHA-256 摘要长度(字节)。
pub const SHA256_LEN: usize = 32;
/// SHA-256 小写十六进制长度。
pub const SHA256_HEX_LEN: usize = 64;

/// SHA-256 摘要。
pub fn sha256(data: &[u8]) -> [u8; SHA256_LEN] {
    Sha256::digest(data).into()
}

/// SHA-256 小写十六进制(64 字符)。
pub fn sha256_hex(data: &[u8]) -> String {
    encode_hex(&sha256(data))
}

pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FIPS 180-4 / NIST 已知向量。
    #[test]
    fn nist_sha256_known_vectors() {
        assert_eq!(
            sha256(b""),
            [
                0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
                0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
                0x78, 0x52, 0xb8, 0x55,
            ]
        );
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(sha256_hex(b"").len(), SHA256_HEX_LEN);
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // 同一输入重复计算必须相同(跨进程确定的前提)。
        assert_eq!(sha256_hex(b"abc"), sha256_hex(b"abc"));
    }

    /// 不同输入的哈希必须不同,包括单比特差异。
    #[test]
    fn distinct_inputs_have_distinct_hashes() {
        // "abb" 与 "abc" 只差末字节的一个比特(0x62 vs 0x63)。
        let inputs: &[&[u8]] = &[
            b"",
            b"a",
            b"b",
            b"abc",
            b"abb",
            b"workorder:wo-9:release-request:v3",
            b"workorder:wo-9:release-request:v4",
        ];
        let mut seen = std::collections::HashSet::new();
        for input in inputs {
            let digest = sha256(input);
            assert!(
                seen.insert(digest),
                "不同输入碰撞: {input:?} -> {}",
                sha256_hex(input)
            );
            let hex = sha256_hex(input);
            assert_eq!(hex.len(), SHA256_HEX_LEN);
            assert!(hex
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
            assert_eq!(sha256(input), digest);
        }
        assert_eq!(seen.len(), inputs.len());
    }
}
