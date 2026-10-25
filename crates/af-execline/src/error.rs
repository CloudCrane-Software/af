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
    /// MES ClickHouse 真实记账失败（消债 B②③：驱动写缝的 CH 下游送批
    /// Parked / 对账读失败 / 目标声明不可解析）。fail-loud——不接受
    /// "可能已写"：at-least-once 由 outbox + 读侧 event_id 对账收敛。
    MesCh,
    /// 执行单元失败。
    UnitFailed,
    /// 判分脚本指纹门失败：脚本 sha256 ≠ 声明锚，或脚本不可读（无法验真）。
    /// 吸收方案 §3.2-2 fail-loud——拒跑、不降级、不跳判、不静默换脚本。
    JudgeFingerprintMismatch,
    /// 计划装订-模板引用语法非法（template_ref 不合 `pkg.<包>.<名>[@<版本>]`）。
    /// EX4xxx 段首例（G4 设计 §2.5：计划装订是新失败类别；E9004→EX4004 码位
    /// 留给 planbind 通用装订器的 digest 核对，数字只增——ADR-007）。
    TemplateRefInvalid,
    /// 计划装订-模板未登记/MANIFEST.sha256 清单不可读（装载期门，fail-loud）。
    TemplateUnregistered,
    /// 计划装订-模板漂移：文件 sha256 ≠ 清单登记值，或 template_ref 版本段
    /// ≠ 清单版本（同 E9004 的拒绝式纪律——不降级、不静默换模板）。
    TemplateDrift,
    /// 计划装订-注册表 digest 不符（E9004 执行面消费——planbind 唯一正本，
    /// 裁决9；文案首 token 带 E9004，点名计划携带/本地重算双 digest）。
    PlanDigestMismatch,
    /// 计划装订-一致性核对失败：信封版本/graph_version/line 绑定/node_exec
    /// 与 graph.nodes 对齐（planbind 附带核对面，设计 §2.5）。
    PlanBindMismatch,
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
            Self::MesCh => "EX2003",
            Self::UnitFailed => "EX3001",
            Self::JudgeFingerprintMismatch => "EX3002",
            Self::TemplateRefInvalid => "EX4001",
            Self::TemplateUnregistered => "EX4002",
            Self::TemplateDrift => "EX4003",
            Self::PlanDigestMismatch => "EX4004",
            Self::PlanBindMismatch => "EX4005",
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
            Self::MesCh => "MES_CH",
            Self::UnitFailed => "UNIT_FAILED",
            Self::JudgeFingerprintMismatch => "JUDGE_FINGERPRINT_MISMATCH",
            Self::TemplateRefInvalid => "TEMPLATE_REF_INVALID",
            Self::TemplateUnregistered => "TEMPLATE_UNREGISTERED",
            Self::TemplateDrift => "TEMPLATE_DRIFT",
            Self::PlanDigestMismatch => "PLAN_DIGEST_MISMATCH",
            Self::PlanBindMismatch => "PLAN_BIND_MISMATCH",
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
    EngineRejected {
        op: String,
        rejection: EngineRejection,
    },
    #[error("EX2001: ledger IO 失败: {detail}")]
    LedgerIo { detail: String },
    #[error("EX2002: ledger 损坏: {detail}")]
    LedgerCorrupt { detail: String },
    /// EX2003：MES ClickHouse 记账失败（fail-loud；at-least-once 由 outbox +
    /// 读侧 event_id 对账收敛，重跑 submit 只补差量）。
    #[error("EX2003: MES ClickHouse 记账失败: {detail}")]
    MesCh { detail: String },
    #[error("EX3001: 执行单元失败: {detail}")]
    UnitFailed { detail: String },
    #[error("EX3002: 判分脚本指纹不匹配: {script} 实测 {actual} ≠ 声明锚 {expected}（fail-loud：拒跑、不降级、不跳判、不静默换脚本；吸收方案 §3.2-2）")]
    JudgeFingerprintMismatch {
        script: String,
        expected: String,
        actual: String,
    },
    #[error("EX4001: 计划装订-模板引用非法: {reference}: {detail}")]
    TemplateRefInvalid { reference: String, detail: String },
    #[error("EX4002: 计划装订-模板未登记/清单不可读: {reference}: {detail}")]
    TemplateUnregistered { reference: String, detail: String },
    #[error("EX4003: 计划装订-模板漂移（sha256/版本不符）: {reference}: {detail}")]
    TemplateDrift { reference: String, detail: String },
    /// E9004→EX4004（裁决9 唯一正本；编译器/执行面共用语义码 E9004 首 token）。
    #[error("E9004/EX4004: 计划装订-registry_digest 不符: 本地重算 {actual} ≠ 计划携带 expected {expected}（执行面拒启动——组件注册表与编译期漂移）")]
    PlanDigestMismatch { expected: String, actual: String },
    #[error("EX4005: 计划装订-一致性核对失败: {detail}")]
    PlanBindMismatch { detail: String },
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
            Self::MesCh { .. } => ErrorCode::MesCh,
            Self::UnitFailed { .. } => ErrorCode::UnitFailed,
            Self::JudgeFingerprintMismatch { .. } => ErrorCode::JudgeFingerprintMismatch,
            Self::TemplateRefInvalid { .. } => ErrorCode::TemplateRefInvalid,
            Self::TemplateUnregistered { .. } => ErrorCode::TemplateUnregistered,
            Self::TemplateDrift { .. } => ErrorCode::TemplateDrift,
            Self::PlanDigestMismatch { .. } => ErrorCode::PlanDigestMismatch,
            Self::PlanBindMismatch { .. } => ErrorCode::PlanBindMismatch,
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
        assert_eq!(ErrorCode::MesCh.as_str(), "EX2003");
        assert_eq!(ErrorCode::UnitFailed.as_str(), "EX3001");
        assert_eq!(ErrorCode::JudgeFingerprintMismatch.as_str(), "EX3002");
        assert_eq!(
            ErrorCode::EngineRejected.name(),
            "ENGINE_REJECTED",
            "name 与码一一对应"
        );
        assert_eq!(
            ErrorCode::JudgeFingerprintMismatch.name(),
            "JUDGE_FINGERPRINT_MISMATCH"
        );
        // EX4xxx 段开段（G4 计划装订-模板机制）：EX4001-4003。
        assert_eq!(ErrorCode::TemplateRefInvalid.as_str(), "EX4001");
        assert_eq!(ErrorCode::TemplateUnregistered.as_str(), "EX4002");
        assert_eq!(ErrorCode::TemplateDrift.as_str(), "EX4003");
        assert_eq!(ErrorCode::PlanDigestMismatch.as_str(), "EX4004");
        assert_eq!(ErrorCode::PlanBindMismatch.as_str(), "EX4005");
        assert_eq!(ErrorCode::TemplateDrift.name(), "TEMPLATE_DRIFT");
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
