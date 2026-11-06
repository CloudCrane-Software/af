//! 变更单:差异描述 + 排水/接力策略 + ed25519 签名。
//!
//! 语义定位:工单的 graph_version 不可变,所以"改版本"不是改原单,而是
//! 生成一张绑定新版本的**后继工单**(`supersedes` 指回原单),原单封存留痕。
//!
//! 签名在本 crate 内闭环([`crate::sign`]),不调用 af-credential。
//! 被签字节 = [`CHANGE_ORDER_SIGNING_DOMAIN`] || `serde_json` 编码。
//! 本结构没有 Map,字段顺序按声明固定,同一张单的编码跨进程一致。

use serde::{Deserialize, Serialize};

use crate::decision::Signature;
use crate::error::ChangeError;
use crate::order::{GraphVersion, WorkOrder};
use crate::sign::{Ed25519Keypair, Ed25519PublicKey};
use crate::sla::Sla;

/// 变更单签名的域分离前缀。签发与验签必须使用同一前缀。
pub const CHANGE_ORDER_SIGNING_DOMAIN: &[u8] = b"af-workorder/change-order/v1\0";

/// 排水策略:切版本时,旧版本上的在制品(in-flight)怎么清场。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrainStrategy {
    /// 不排水:不等在制,直接切(在制按接力策略处置)
    NoDrain,
    /// 全量排水:排空所有在制后才允许切换
    DrainAllInflight,
    /// 上限排水:在制数量降到 `max_inflight` 以下即放行
    DrainWithCap { max_inflight: u32 },
}

/// 接力策略:切换瞬间,新旧版本如何交接。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelayStrategy {
    /// 冷接力:旧版停稳 → 新版起跑(允许停机窗口)
    ColdHandover,
    /// 温接力:双跑交接,重叠预算 `overlap_budget`(秒)内新旧并存在制
    WarmHandover { overlap_budget: u32 },
    /// 热接力:在制不落地,直接由新版接管
    HotTakeover,
}

/// 变更单。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeOrder {
    pub change_id: String,
    /// 目标工单 id
    pub work_order_id: String,
    pub from_graph_version: GraphVersion,
    pub to_graph_version: GraphVersion,
    /// 差异描述(人类可读,必填)
    pub diff_summary: String,
    /// 差异涉及的图谱节点/路径(机器可核对)
    pub diff_paths: Vec<String>,
    pub drain: DrainStrategy,
    pub relay: RelayStrategy,
}

impl ChangeOrder {
    /// 变更单自身的完备性校验(与目标工单无关的规则):
    /// ① from ≠ to;② 差异描述非空;③ 排水/接力策略组合不矛盾。
    pub fn validate(&self) -> Result<(), ChangeError> {
        if self.from_graph_version == self.to_graph_version {
            return Err(ChangeError::FromEqualsTo(self.from_graph_version.clone()));
        }
        if self.diff_summary.trim().is_empty() {
            return Err(ChangeError::EmptyDiffSummary);
        }
        if matches!(self.drain, DrainStrategy::DrainAllInflight)
            && matches!(self.relay, RelayStrategy::HotTakeover)
        {
            return Err(ChangeError::ConflictingDrainRelay);
        }
        Ok(())
    }

    /// 是否适用于目标工单:指向同一张单、基于的 from 版本与工单当前绑定一致(防陈旧)、
    /// 且工单未到终态(终态封存,不接受变更)。
    pub fn applies_to(&self, order: &WorkOrder) -> bool {
        self.work_order_id == order.id()
            && &self.from_graph_version == order.graph_version()
            && !order.state().is_terminal()
    }

    /// 由变更单生成后继工单:Created 态、绑定 to_graph_version、supersedes 指回原单。
    /// 原工单不被触碰(graph_version 不可变的兑现)。
    pub fn spawn_successor(
        &self,
        new_id: impl Into<String>,
        sla: Sla,
    ) -> Result<WorkOrder, ChangeError> {
        self.validate()?;
        Ok(WorkOrder::successor(
            new_id,
            self.to_graph_version.clone(),
            sla,
            self.work_order_id.clone(),
        ))
    }

    /// 被签字节:域分离前缀接 serde_json 编码。不在这里做业务校验。
    pub fn signing_message(&self) -> Result<Vec<u8>, ChangeError> {
        let body =
            serde_json::to_vec(self).map_err(|err| ChangeError::SigningPayload(err.to_string()))?;
        let mut message = Vec::with_capacity(CHANGE_ORDER_SIGNING_DOMAIN.len() + body.len());
        message.extend_from_slice(CHANGE_ORDER_SIGNING_DOMAIN);
        message.extend_from_slice(&body);
        Ok(message)
    }

    /// 校验通过后签名。非法变更单(from=to、空差异、策略矛盾)不会被签上。
    pub fn sign(&self, key: &Ed25519Keypair) -> Result<Signature, ChangeError> {
        self.validate()?;
        let message = self.signing_message()?;
        Ok(key.sign(&message))
    }

    /// 先验签,再做业务校验。签名不匹配时返回签名错误,即使变更单本身也不合法。
    pub fn verify(&self, signature: &Signature, key: &Ed25519PublicKey) -> Result<(), ChangeError> {
        let message = self.signing_message()?;
        key.verify(&message, signature)
            .map_err(ChangeError::SignatureInvalid)?;
        self.validate()?;
        Ok(())
    }
}

/// 已签名的变更单。验签通过后才能 [`Self::spawn_successor`]。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedChangeOrder {
    pub order: ChangeOrder,
    pub signature: Signature,
}

impl SignedChangeOrder {
    pub fn sign(order: ChangeOrder, key: &Ed25519Keypair) -> Result<Self, ChangeError> {
        let signature = order.sign(key)?;
        Ok(Self { order, signature })
    }

    pub fn verify(&self, key: &Ed25519PublicKey) -> Result<(), ChangeError> {
        self.order.verify(&self.signature, key)
    }

    /// 验签通过后生成后继工单。验签失败不产生后继单。
    pub fn spawn_successor(
        &self,
        key: &Ed25519PublicKey,
        new_id: impl Into<String>,
        sla: Sla,
    ) -> Result<WorkOrder, ChangeError> {
        self.verify(key)?;
        self.order.spawn_successor(new_id, sla)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sla::Priority;

    fn change(from: &str, to: &str, drain: DrainStrategy, relay: RelayStrategy) -> ChangeOrder {
        ChangeOrder {
            change_id: "ch-1".into(),
            work_order_id: "wo-1".into(),
            from_graph_version: GraphVersion::new(from),
            to_graph_version: GraphVersion::new(to),
            diff_summary: "节点 N12 的判定阈值 0.8 → 0.9".into(),
            diff_paths: vec!["graph/N12".into()],
            drain,
            relay,
        }
    }

    #[test]
    fn serde_roundtrip_keeps_all_fields() {
        let c = change(
            "g-3",
            "g-4",
            DrainStrategy::DrainWithCap { max_inflight: 5 },
            RelayStrategy::WarmHandover {
                overlap_budget: 120,
            },
        );
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<ChangeOrder>(&json).unwrap(), c);
    }

    #[test]
    fn from_equals_to_is_rejected() {
        let err = change(
            "g-3",
            "g-3",
            DrainStrategy::NoDrain,
            RelayStrategy::ColdHandover,
        )
        .validate()
        .unwrap_err();
        assert!(matches!(err, ChangeError::FromEqualsTo(_)));
    }

    #[test]
    fn empty_diff_summary_is_rejected() {
        let mut c = change(
            "g-3",
            "g-4",
            DrainStrategy::NoDrain,
            RelayStrategy::ColdHandover,
        );
        c.diff_summary = "   ".into();
        assert_eq!(c.validate().unwrap_err(), ChangeError::EmptyDiffSummary);
    }

    #[test]
    fn drain_all_plus_hot_takeover_conflicts() {
        let err = change(
            "g-3",
            "g-4",
            DrainStrategy::DrainAllInflight,
            RelayStrategy::HotTakeover,
        )
        .validate()
        .unwrap_err();
        assert_eq!(err, ChangeError::ConflictingDrainRelay);
        // 其余组合合法
        assert!(change(
            "g-3",
            "g-4",
            DrainStrategy::DrainAllInflight,
            RelayStrategy::ColdHandover
        )
        .validate()
        .is_ok());
        assert!(change(
            "g-3",
            "g-4",
            DrainStrategy::NoDrain,
            RelayStrategy::HotTakeover
        )
        .validate()
        .is_ok());
    }

    #[test]
    fn spawn_successor_binds_new_version_and_supersedes() {
        let c = change(
            "g-3",
            "g-4",
            DrainStrategy::NoDrain,
            RelayStrategy::ColdHandover,
        );
        let successor = c
            .spawn_successor("wo-1b", Sla::new(None, Priority::High))
            .unwrap();
        assert_eq!(successor.graph_version().as_str(), "g-4");
        assert_eq!(successor.supersedes(), Some("wo-1"));
        assert_eq!(successor.state(), crate::state::OrderState::Created);
    }

    #[test]
    fn invalid_change_cannot_spawn_successor() {
        let c = change(
            "g-3",
            "g-3",
            DrainStrategy::NoDrain,
            RelayStrategy::ColdHandover,
        );
        assert!(c
            .spawn_successor("wo-1b", Sla::new(None, Priority::Low))
            .is_err());
    }

    const SEED: [u8; 32] = [0x7a; 32];
    const OTHER_SEED: [u8; 32] = [0x19; 32];

    fn sample() -> ChangeOrder {
        change(
            "g-3",
            "g-4",
            DrainStrategy::DrainWithCap { max_inflight: 5 },
            RelayStrategy::WarmHandover {
                overlap_budget: 120,
            },
        )
    }

    #[test]
    fn signing_domain_bytes_stay_on_v1() {
        assert_eq!(
            CHANGE_ORDER_SIGNING_DOMAIN,
            b"af-workorder/change-order/v1\0"
        );
        let order = sample();
        let message = order.signing_message().unwrap();
        assert!(message.starts_with(CHANGE_ORDER_SIGNING_DOMAIN));
        let body = &message[CHANGE_ORDER_SIGNING_DOMAIN.len()..];
        let parsed: ChangeOrder = serde_json::from_slice(body).unwrap();
        assert_eq!(parsed, order);
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            !text.contains("stable_id"),
            "签名域仍只覆盖变更单自身,不掺入工单身份:{text}"
        );
    }

    #[test]
    fn signed_change_order_roundtrip_verifies() {
        let order = sample();
        let key = Ed25519Keypair::from_seed(&SEED);
        let public_key = key.public_key();
        let message = order.signing_message().unwrap();
        assert!(message.starts_with(CHANGE_ORDER_SIGNING_DOMAIN));
        assert_eq!(message, order.clone().signing_message().unwrap());

        let signed = SignedChangeOrder::sign(order.clone(), &key).unwrap();
        let again = SignedChangeOrder::sign(order, &key).unwrap();
        assert_eq!(signed.signature.bytes, again.signature.bytes);
        assert_eq!(
            signed.signature.bytes.len(),
            crate::sign::ED25519_SIGNATURE_LEN
        );
        signed.verify(&public_key).unwrap();

        let json = serde_json::to_string(&signed).unwrap();
        let restored: SignedChangeOrder = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, signed);
        let public_key_again = Ed25519PublicKey::from_bytes(&public_key.to_bytes()).unwrap();
        restored.verify(&public_key_again).unwrap();
    }

    #[test]
    fn tampered_order_or_signature_fails_verify_and_spawn() {
        let key = Ed25519Keypair::from_seed(&SEED);
        let public_key = key.public_key();
        let mut signed = SignedChangeOrder::sign(sample(), &key).unwrap();
        signed.order.diff_summary = "N12 阈值改成另一处".into();
        assert!(matches!(
            signed.verify(&public_key),
            Err(ChangeError::SignatureInvalid(
                crate::error::SignError::VerificationFailed
            ))
        ));
        assert!(signed
            .spawn_successor(&public_key, "wo-1b", Sla::new(None, Priority::Low))
            .is_err());

        let mut signed = SignedChangeOrder::sign(sample(), &key).unwrap();
        signed.signature.bytes[10] ^= 0xff;
        assert!(matches!(
            signed.verify(&public_key),
            Err(ChangeError::SignatureInvalid(
                crate::error::SignError::VerificationFailed
            ))
        ));

        let other = Ed25519Keypair::from_seed(&OTHER_SEED);
        let signed = SignedChangeOrder::sign(sample(), &key).unwrap();
        assert!(matches!(
            signed.verify(&other.public_key()),
            Err(ChangeError::SignatureInvalid(
                crate::error::SignError::KeyIdMismatch { .. }
            ))
        ));
    }

    #[test]
    fn invalid_change_order_is_not_signed() {
        let order = change(
            "g-3",
            "g-3",
            DrainStrategy::NoDrain,
            RelayStrategy::ColdHandover,
        );
        let err = SignedChangeOrder::sign(order, &Ed25519Keypair::from_seed(&SEED)).unwrap_err();
        assert!(matches!(err, ChangeError::FromEqualsTo(_)));
    }

    #[test]
    fn verified_signature_spawns_successor() {
        let key = Ed25519Keypair::from_seed(&SEED);
        let public_key = key.public_key();
        let signed = SignedChangeOrder::sign(sample(), &key).unwrap();
        let successor = signed
            .spawn_successor(&public_key, "wo-1b", Sla::new(None, Priority::High))
            .unwrap();
        assert_eq!(successor.graph_version().as_str(), "g-4");
        assert_eq!(successor.supersedes(), Some("wo-1"));
        assert_eq!(successor.state(), crate::state::OrderState::Created);
    }
}
