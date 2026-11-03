//! af-order 错误。稳定码 `OR1xxx`（用法）/ `OR2xxx`（spec / 凭证 / 状态），
//! 数字只增不改（ADR-007 §3 错误码契约）；引擎、账本与执行失败透传
//! af-execline 的 `EX1xxx`/`EX2xxx`/`EX3xxx` 稳定码，不改写、不吞错。

use af_execline::ExeclineError;

use crate::spec::SpecError;

/// af-order 错误。全部失败路径带稳定码；Display 即用户可见的原因。
#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    /// OR1001：命令行用法错误（退出码 2）。
    #[error(
        "OR1001: 用法错误: {detail}（用法: af-order submit --spec <order.json> --state <dir> | af-order list --state <dir> | af-order show <order-id> --state <dir>）"
    )]
    Usage { detail: String },
    /// OR2001：spec 文件读不到。
    #[error("OR2001: spec 文件读取失败 {path}: {detail}")]
    SpecRead { path: String, detail: String },
    /// OR2002：spec 被拒（缺字段 / 未知字段 / 类型不合 / 路径不安全，全部点名）。
    #[error("OR2002: spec 被拒: {detail}")]
    SpecInvalid { detail: String },
    /// OR2003：凭证签发 / 密钥操作失败。
    #[error("OR2003: 凭证/密钥失败: {detail}")]
    Voucher { detail: String },
    /// OR2004：订单不存在（state/orders/ 下查无此单）。
    #[error("OR2004: 订单不存在: {order_id}（--state 目录内无此订单的 ledger）")]
    OrderNotFound { order_id: String },
    /// OR2005：状态目录 / 账本读取失败（内嵌 af-execline EX2xxx 详情）。
    #[error("OR2005: 状态读取失败: {detail}")]
    State { detail: String },
    /// 引擎 / 账本 / 执行失败：af-execline 稳定码原样透传。
    #[error("{source}")]
    Submit { source: ExeclineError },
}

impl OrderError {
    /// 进程退出码：用法错误 2，其余 1。
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Usage { .. } => 2,
            _ => 1,
        }
    }
}

impl From<SpecError> for OrderError {
    fn from(err: SpecError) -> Self {
        Self::SpecInvalid {
            detail: err.to_string(),
        }
    }
}

impl From<ExeclineError> for OrderError {
    fn from(source: ExeclineError) -> Self {
        Self::Submit { source }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 码表锁定 + 用法错误退出码 2、其余 1。
    #[test]
    fn codes_and_exit_codes_are_stable() {
        let usage = OrderError::Usage {
            detail: "缺少子命令".into(),
        };
        assert!(usage.to_string().starts_with("OR1001:"), "{usage}");
        assert_eq!(usage.exit_code(), 2);

        let not_found = OrderError::OrderNotFound {
            order_id: "wo-x".into(),
        };
        assert!(not_found.to_string().contains("OR2004"), "{not_found}");
        assert!(not_found.to_string().contains("wo-x"), "{not_found}");
        assert_eq!(not_found.exit_code(), 1);

        let spec_invalid = OrderError::from(SpecError::MissingField("budget".into()));
        assert!(
            spec_invalid.to_string().contains("OR2002"),
            "{spec_invalid}"
        );
        assert!(
            spec_invalid.to_string().contains("budget"),
            "{spec_invalid}"
        );
        assert_eq!(spec_invalid.exit_code(), 1);
    }
}
