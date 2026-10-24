//! 执行面错误。稳定码 EX1xxx（引擎）/ EX2xxx（账本）/ EX3xxx（执行单元），
//! 数字只增不改——沿用 af-dspy-ir DI1xxx、af-graph E2001 风格，供日志与测试断言。

use std::fmt;

use crate::engine::EngineRejection;

/// 稳定错误码。数字只增不改。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// af-engine 二进制在 target 目录找不到（含尝试构建后仍缺失）。
    EngineBinaryMissing,
    /// `cargo build -p af-engine` 失败。
    EngineBuildFailed,
    /// af-engine 子进程拉起失败。
    EngineSpawnFailed,
    /// af-engine 非零退出或 stdout 读不到。
    EngineCrashed,
    /// af-engine 响应不是合法的一行 JSON 协议体。
    EngineBadResponse,
    /// af-engine 协议内拒绝（Gate / Decision / BadRequest / RejectionEnvelope）。
    EngineRejected,
    /// ledger 文件读写失败。
    LedgerIo,
    /// ledger 内容损坏：序号不连续、坏行、身份不一致。
    LedgerCorrupt,
    /// 执行单元失败。
    UnitFailed,
}

impl ErrorCode {
    /// 对外稳定的 `EX1xxx` 字符串。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EngineBinaryMissing => "EX1001",
            Self::EngineBuildFailed => "EX1002",
            Self::EngineSpawnFailed => "EX1003",
            Self::EngineCrashed => "EX1004",
            Self::EngineBadResponse => "EX1005",
            Self::EngineRejected => "EX1006",
            Self::LedgerIo => "EX2001",
            Self::LedgerCorrupt => "EX2002",
            Self::UnitFailed => "EX3001",
        }
    }

    /// 大类名，给结构化日志用。
    pub const fn name(self) -> &'static str {
        match self {
            Self::EngineBinaryMissing => "ENGINE_BINARY_MISSING",
            Self::EngineBuildFailed => "ENGINE_BUILD_FAILED",
            Self::EngineSpawnFailed => "ENGINE_SPAWN_FAILED",
            Self::EngineCrashed => "ENGINE_CRASHED",
            Self::EngineBadResponse => "ENGINE_BAD_RESPONSE",
            Self::EngineRejected => "ENGINE_REJECTED",
            Self::LedgerIo => "LEDGER_IO",
            Self::LedgerCorrupt => "LEDGER_CORRUPT",
            Self::UnitFailed => "UNIT_FAILED",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 执行面错误。所有失败路径都带稳定码；引擎拒绝保留类型化
/// `GateError` / `DecisionError`（不改写变体名与字段），不降级成字符串。
#[derive(Debug, thiserror::Error)]
pub enum ExeclineError {
    #[error("EX1001: af-engine 二进制缺失: {path}（已尝试构建仍未找到；请在 workspace 根执行 cargo build -p af-engine）")]
    EngineBinaryMissing { path: String },
    #[error("EX1002: af-engine 构建失败: {detail}")]
    EngineBuildFailed { detail: String },
    #[error("EX1003: af-engine 子进程拉起失败: {detail}")]
    EngineSpawnFailed { detail: String },
    #[error("EX1004: af-engine 进程异常: {detail}")]
    EngineCrashed { detail: String },
    #[error("EX1005: af-engine 响应无法解析: {detail}")]
    EngineBadResponse { detail: String },
    #[error("EX1006: af-engine 拒绝 op={op}: {rejection}")]
    EngineRejected { op: String, rejection: EngineRejection },
    #[error("EX2001: ledger IO 失败: {detail}")]
    LedgerIo { detail: String },
    #[error("EX2002: ledger 损坏: {detail}")]
    LedgerCorrupt { detail: String },
    #[error("EX3001: 执行单元失败: {detail}")]
    UnitFailed { detail: String },
}

impl ExeclineError {
    /// 稳定错误码。
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::EngineBinaryMissing { .. } => ErrorCode::EngineBinaryMissing,
            Self::EngineBuildFailed { .. } => ErrorCode::EngineBuildFailed,
            Self::EngineSpawnFailed { .. } => ErrorCode::EngineSpawnFailed,
            Self::EngineCrashed { .. } => ErrorCode::EngineCrashed,
            Self::EngineBadResponse { .. } => ErrorCode::EngineBadResponse,
            Self::EngineRejected { .. } => ErrorCode::EngineRejected,
            Self::LedgerIo { .. } => ErrorCode::LedgerIo,
            Self::LedgerCorrupt { .. } => ErrorCode::LedgerCorrupt,
            Self::UnitFailed { .. } => ErrorCode::UnitFailed,
        }
    }
}

impl From<std::io::Error> for ExeclineError {
    fn from(err: std::io::Error) -> Self {
        Self::LedgerIo {
            detail: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 码表锁定：EX1001 起连续分配，数字只增不改。
    #[test]
    fn codes_are_stable_and_contiguous() {
        assert_eq!(ErrorCode::EngineBinaryMissing.as_str(), "EX1001");
        assert_eq!(ErrorCode::EngineRejected.as_str(), "EX1006");
        assert_eq!(ErrorCode::LedgerIo.as_str(), "EX2001");
        assert_eq!(ErrorCode::LedgerCorrupt.as_str(), "EX2002");
        assert_eq!(ErrorCode::UnitFailed.as_str(), "EX3001");
        assert_eq!(
            ErrorCode::EngineRejected.name(),
            "ENGINE_REJECTED",
            "name 与码一一对应"
        );
    }

    #[test]
    fn io_error_maps_to_ledger_io() {
        let err = ExeclineError::from(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no such file",
        ));
        assert_eq!(err.code(), ErrorCode::LedgerIo);
        assert!(err.to_string().starts_with("EX2001:"), "{err}");
    }
}
