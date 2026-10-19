//! 错误类型。核心要求:非法转换必须拒绝,并**指出缺的是哪条前置条件**——
//! 所以 [`GateError::PreconditionsNotMet`] 携带类型化的 `Vec<Missing>`,而不是一个字符串。

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::decision::Decision;
use crate::state::{OrderState, Transition};

/// release / close 门控可能缺失的前置条件(类型化,供程序判断与测试断言)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Missing {
    /// release:未绑定产线版本
    LineVersionNotBound,
    /// release:预算凭证未签发
    BudgetVoucherNotIssued,
    /// release:预算凭证已挂上但未签名
    BudgetVoucherNotSigned { voucher_id: String },
    /// close:不存在"合格 + 终态"的工件
    NoQualifiedFinalArtifact,
}

impl std::fmt::Display for Missing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Missing::LineVersionNotBound => write!(f, "产线版本未绑定"),
            Missing::BudgetVoucherNotIssued => write!(f, "预算凭证未签发"),
            Missing::BudgetVoucherNotSigned { voucher_id } => {
                write!(f, "预算凭证 {voucher_id} 已登记但未签名")
            }
            Missing::NoQualifiedFinalArtifact => write!(f, "缺少合格的终态工件"),
        }
    }
}

/// 工单门控错误。
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum GateError {
    #[error("非法状态转换 {from:?} → {to:?}:状态机不存在这条边(允许的出边:{allowed:?})")]
    IllegalTransition {
        from: OrderState,
        to: OrderState,
        allowed: Vec<Transition>,
    },
    #[error("门控拒绝 {transition:?}:前置条件缺失 {missing:?}")]
    PreconditionsNotMet {
        transition: Transition,
        missing: Vec<Missing>,
    },
    #[error("状态 {state:?} 下禁止操作 {op}")]
    MutationForbidden { state: OrderState, op: &'static str },
}

impl GateError {
    /// 程序化读取缺失前置条件(仅门控错误有)。
    pub fn missing(&self) -> Option<&[Missing]> {
        match self {
            GateError::PreconditionsNotMet { missing, .. } => Some(missing),
            _ => None,
        }
    }
}

/// 决策工件校验错误。
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum DecisionError {
    #[error("payload_hash 不匹配:期望 {expected},实际 {actual}")]
    PayloadHashMismatch { expected: String, actual: String },
    #[error("角色 {role:?} 的权限表越界:持有 {offending:?},角色只允许 {role_allows:?}")]
    RoleExceedsCapability {
        role: crate::decision::Role,
        offending: crate::decision::Permission,
        role_allows: Vec<crate::decision::Permission>,
    },
    #[error("决策权限不足:决策 {decision:?} 需要 {required:?},实际持有 {granted:?}")]
    PermissionDenied {
        decision: Decision,
        required: crate::decision::Permission,
        granted: Vec<crate::decision::Permission>,
    },
    #[error("占位签名校验失败:{reason}")]
    InvalidSignature { reason: String },
}

/// ed25519 签发/验签错误。变更单本地闭环使用,不经过 af-credential。
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum SignError {
    #[error("ed25519 公钥不是合法曲线点,或是小阶公钥")]
    InvalidPublicKey,
    #[error("签名算法不匹配:期望 {expected},实际 {actual}")]
    AlgorithmMismatch { expected: String, actual: String },
    #[error("key_id 与验签公钥不一致:期望 {expected},实际 {actual}")]
    KeyIdMismatch { expected: String, actual: String },
    #[error("签名长度非法:ed25519 需要 {expected} 字节,实际 {actual}")]
    MalformedSignature { expected: usize, actual: usize },
    #[error("ed25519 验签失败")]
    VerificationFailed,
}

/// 变更单校验错误。
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum ChangeError {
    #[error("from_graph_version 与 to_graph_version 相同({0:?}),不构成变更")]
    FromEqualsTo(crate::order::GraphVersion),
    #[error("diff_summary 为空:变更单必须写清差异描述")]
    EmptyDiffSummary,
    #[error(
        "策略组合矛盾:DrainAllInflight(排空全部在制才切)与 HotTakeover(在制立即由新版接管)互斥"
    )]
    ConflictingDrainRelay,
    #[error("变更单签名载荷编码失败:{0}")]
    SigningPayload(String),
    #[error("变更单签名无效:{0}")]
    SignatureInvalid(SignError),
}

/// 稳定身份解析错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum IdentityError {
    #[error("stable_id 为空或含空白,不能作为订单身份")]
    EmptyStableId,
}

/// 分片映射错误。分片数来自配方层,0 不是合法配方值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum ShardError {
    #[error("分片数为 0:配方层必须给出正整数")]
    ZeroShardCount,
}
