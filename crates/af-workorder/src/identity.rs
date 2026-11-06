//! 订单稳定身份与分片映射。
//!
//! Temporal continue-as-new 会更换引擎 `run_id`。订单身份不能跟着 run 走,
//! 所以 [`StableId`] 在创建时生成并写进工单,之后没有修改入口。
//! 换 run 只走 [`crate::order::WorkOrder::continue_as_new`]。
//!
//! 分片键 `shard_key = sha256(stable_id) mod shard_count`。
//! `shard_count` 由配方层传入,不存在工单上。

use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{IdentityError, RejectionEnvelope, ShardError};
use crate::hash::{sha256, sha256_hex};

/// 生成稳定身份时的域分离前缀。材料是此前缀接订单 id 的字节,不含 run_id。
pub const STABLE_ID_DOMAIN: &[u8] = b"af-workorder/stable-id/v1\0";

/// 决策工件被签 payload 的域。`stable_id` 在这个字符串里,`run_id` 不在。
pub const DECISION_PAYLOAD_DOMAIN: &str = "af-workorder/decision-payload/v1";

/// 跨 continue-as-new 不变的订单身份。
///
/// 线格式是一个非空字符串。空白字符串在解析时拒绝,不生成默认身份。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct StableId(String);

impl StableId {
    /// 由订单 id 生成。同一 id 得到同一身份;不读系统熵,也不读 run。
    pub fn generate(order_id: &str) -> Self {
        let mut material = Vec::with_capacity(STABLE_ID_DOMAIN.len() + order_id.len());
        material.extend_from_slice(STABLE_ID_DOMAIN);
        material.extend_from_slice(order_id.as_bytes());
        Self(format!("sid-{}", sha256_hex(&material)))
    }

    /// 接受已经存在的身份(CAN 续接时原样带回)。空串与含空白的串拒绝。
    pub fn parse(raw: impl Into<String>) -> Result<Self, IdentityError> {
        let raw = raw.into();
        if raw.is_empty() || raw.chars().any(char::is_whitespace) {
            return Err(IdentityError::EmptyStableId);
        }
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `sha256(stable_id)` 按大端 256 位整数对 `shard_count` 取模。
    ///
    /// `shard_count` 是配方层的分片数,必须 ≥ 1。同一身份与同一分片数的结果跨进程固定。
    pub fn shard_key(&self, shard_count: u32) -> Result<u32, ShardError> {
        shard_index(self.as_str(), shard_count)
    }
}

impl std::fmt::Display for StableId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for StableId {
    type Err = IdentityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl<'de> Deserialize<'de> for StableId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        StableId::parse(raw).map_err(serde::de::Error::custom)
    }
}

/// 引擎一次执行的上下文。continue-as-new 换的是这里的 `run_id`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunContext {
    run_id: String,
}

impl RunContext {
    pub fn new(run_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
        }
    }

    /// 还没绑到引擎 run。[`crate::order::WorkOrder::new`] 的初始上下文。
    pub fn detached() -> Self {
        Self {
            run_id: String::new(),
        }
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn is_detached(&self) -> bool {
        self.run_id.is_empty()
    }

    pub(crate) fn into_run_id(self) -> String {
        self.run_id
    }
}

/// 决策工件的被签字符串。只纳入 `stable_id` 与调用方给出的 subject。
///
/// 长度前缀把两段隔开,subject 里出现另一段的文本也不会改写身份边界。
/// run_id 不进入这段字节,换 run 不改变决策哈希。
pub fn decision_payload(stable_id: &StableId, subject: &str) -> String {
    format!(
        "{DECISION_PAYLOAD_DOMAIN}|{sid_len}|{sid}|{subject_len}|{subject}",
        sid_len = stable_id.as_str().len(),
        sid = stable_id.as_str(),
        subject_len = subject.len(),
    )
}

/// 老载荷身份字段的门禁。非对象留给 serde 报类型错误。
///
/// 先看 `stable_id`,再看 `run_id`。两个都缺时,报错点名 `stable_id`。
/// 不在这里生成或填入默认身份。
pub(crate) fn inspect_identity_fields(value: &Value) -> Result<(), RejectionEnvelope> {
    let Some(obj) = value.as_object() else {
        return Ok(());
    };
    inspect_stable_id(obj)?;
    inspect_run_id(obj)?;
    Ok(())
}

fn inspect_stable_id(obj: &Map<String, Value>) -> Result<(), RejectionEnvelope> {
    match obj.get("stable_id") {
        None | Some(Value::Null) => Err(RejectionEnvelope::missing("stable_id")),
        Some(Value::String(raw)) => match StableId::parse(raw.as_str()) {
            Ok(_) => Ok(()),
            Err(IdentityError::EmptyStableId) => Err(RejectionEnvelope::blank("stable_id")),
        },
        Some(_) => Err(RejectionEnvelope::invalid_type("stable_id")),
    }
}

fn inspect_run_id(obj: &Map<String, Value>) -> Result<(), RejectionEnvelope> {
    match obj.get("run_id") {
        None | Some(Value::Null) => Err(RejectionEnvelope::missing("run_id")),
        Some(Value::String(_)) => Ok(()),
        Some(_) => Err(RejectionEnvelope::invalid_type("run_id")),
    }
}

fn shard_index(stable_id: &str, shard_count: u32) -> Result<u32, ShardError> {
    if shard_count == 0 {
        return Err(ShardError::ZeroShardCount);
    }
    let digest = sha256(stable_id.as_bytes());
    let modulus = u128::from(shard_count);
    let mut acc: u128 = 0;
    for byte in digest {
        acc = (acc * 256 + u128::from(byte)) % modulus;
    }
    // acc 已经对 shard_count 取模,而 shard_count 本身是 u32,所以结果放得进 u32。
    Ok(u32::try_from(acc).expect("shard index fits in u32"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_is_deterministic_and_ignores_nothing_but_order_id() {
        let id = StableId::generate("wo-1");
        assert_eq!(id, StableId::generate("wo-1"));
        assert_eq!(
            id.as_str(),
            "sid-f905c3b464315a0a05fe2682f64a93aa2194016459b62a5b554f8d60692f451f"
        );
        assert_ne!(id, StableId::generate("wo-2"));
        assert!(!id.as_str().contains("run"));
    }

    #[test]
    fn parse_rejects_empty_and_whitespace() {
        assert_eq!(
            StableId::parse("").unwrap_err(),
            IdentityError::EmptyStableId
        );
        assert_eq!(
            StableId::parse("   ").unwrap_err(),
            IdentityError::EmptyStableId
        );
        assert_eq!(
            StableId::parse("sid has space").unwrap_err(),
            IdentityError::EmptyStableId
        );
        assert_eq!(StableId::parse("sid-fixed").unwrap().as_str(), "sid-fixed");
    }

    #[test]
    fn shard_key_matches_sha256_mod_and_rejects_zero() {
        let id = StableId::parse("sid-fixed").unwrap();
        assert_eq!(id.shard_key(0).unwrap_err(), ShardError::ZeroShardCount);
        assert_eq!(id.shard_key(1).unwrap(), 0);
        // sha256("sid-fixed") 按大端 256 位取模。向量与实现分开算过。
        assert_eq!(id.shard_key(64).unwrap(), 32);
        assert_eq!(id.shard_key(100).unwrap(), 36);
        assert_eq!(
            StableId::parse("sid-other").unwrap().shard_key(64).unwrap(),
            11
        );
        let again = id.shard_key(64).unwrap();
        for _ in 0..8 {
            assert_eq!(id.shard_key(64).unwrap(), again);
        }
    }
}
