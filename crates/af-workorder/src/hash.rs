//! payload_hash 的占位实现:FNV-1a 64 位。
//!
//! 为什么不用 SHA-256:遵守 PoC 依赖从简纪律(不加 sha2/blake)。
//! FNV-1a 无外部依赖、跨进程确定(满足"多方对同一 payload 算出同一 hash 才可比"的语义),
//! 但**无抗碰撞性**,只作占位。最终方案应替换为 SHA-256/BLAKE3,调用方接口不变。

/// FNV-1a 64 位哈希。
pub fn fnv1a64(data: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// 十六进制(16 位小写,零填充)。
pub fn fnv1a64_hex(data: &[u8]) -> String {
    format!("{:016x}", fnv1a64(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_known_vector() {
        // FNV-1a 64 对空串的公认测试向量
        assert_eq!(fnv1a64(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a64(b"a"), 0xaf63dc4c8601ec8c);
    }

    #[test]
    fn hex_is_fixed_width_lowercase() {
        let hex = fnv1a64_hex(b"");
        assert_eq!(hex, "cbf29ce484222325");
        assert_eq!(hex.len(), 16);
    }
}
