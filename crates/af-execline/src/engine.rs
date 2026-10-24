//! af-engine 子进程客户端：9-op 单行 JSON stdin/stdout 协议。
//!
//! 真引擎、不打桩：每次调用拉起一个 af-engine 进程，一行请求进、一行响应出，
//! 然后进程退出（引擎无状态，快照整单进出）。本 crate 不链接 af-engine 库——
//! 库方式调用等于把判定搬进本进程，绕过"必经引擎"门。
//!
//! 二进定位：`<target_dir>/debug/af-engine` 已存在即用；否则在 workspace 根执行
//! `cargo build -p af-engine`（`CARGO_TARGET_DIR` 指向同一 target 目录）。
//! 构建失败或二进制仍缺失 = 明确错误（EX1001 / EX1002），不静默跳过。
//! 构建目录纪律见 ADR-009：只用 `~/afbuild-phase2`，严禁占用 af/target。

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use af_workorder::{DecisionError, Missing, OrderState, Transition};
use serde::Deserialize;
use serde_json::Value;

use crate::error::ExeclineError;

/// af-workorder `GateError` 的线格式镜像（外部标签枚举，变体名与字段名逐字保持）。
///
/// 为什么需要镜像而不是直接用 `af_workorder::GateError`：
/// `GateError::MutationForbidden` 的 `op: &'static str` 字段使
/// `GateError: Deserialize<'de>` 只在 `'de: 'static` 时成立（serde 对 `&str`
/// 字段的隐式借用规则收集 `'static`），无法从运行时缓冲区——引擎 stdout 那行
/// JSON——反序列化。镜像只把 `op` 落成 `String`，线格式一个字不改写。
/// af-workorder 若增删门控变体，本镜像按未知变体/未知字段拒绝（fail-loud，
/// 不静默丢错）。`DecisionError` 无此问题，直接用 af-workorder 原类型。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum GateRejection {
    IllegalTransition {
        from: OrderState,
        to: OrderState,
        allowed: Vec<Transition>,
    },
    PreconditionsNotMet {
        transition: Transition,
        missing: Vec<Missing>,
    },
    MutationForbidden { state: OrderState, op: String },
}

impl GateRejection {
    /// 类型化缺失前置条件清单（仅 `PreconditionsNotMet` 有）。
    /// 与 af-workorder `GateError::missing` 同语义。
    pub fn missing(&self) -> Option<&[Missing]> {
        match self {
            Self::PreconditionsNotMet { missing, .. } => Some(missing),
            _ => None,
        }
    }
}

impl fmt::Display for GateRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IllegalTransition {
                from,
                to,
                allowed,
            } => write!(
                f,
                "非法状态转换 {from:?} → {to:?}（状态机不存在这条边，允许出边 {allowed:?}）"
            ),
            Self::PreconditionsNotMet {
                transition,
                missing,
            } => write!(f, "门控拒绝 {transition:?}: 前置条件缺失 {missing:?}"),
            Self::MutationForbidden { state, op } => write!(f, "状态 {state:?} 下禁止操作 {op}"),
        }
    }
}

/// 引擎协议内拒绝。`kind` 与 af-engine `WireError` 一一对应；
/// `Gate` 用 [`GateRejection`] 镜像承接（见其上文档），`Decision` 是
/// af-workorder 派生 serde 的原样结果（无借用字段，可直接反序列化）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind")]
pub enum EngineRejection {
    BadRequest { message: String },
    RejectionEnvelope { field: String, detail: String },
    Gate { gate: GateRejection },
    Decision { decision: DecisionError },
}

impl fmt::Display for EngineRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadRequest { message } => write!(f, "BadRequest: {message}"),
            Self::RejectionEnvelope { field, detail } => {
                write!(f, "RejectionEnvelope[{field}]: {detail}")
            }
            Self::Gate { gate } => write!(f, "Gate: {gate}"),
            Self::Decision { decision } => write!(f, "Decision: {decision}"),
        }
    }
}

/// 放行响应：整单快照 + 引擎版本（`engine_version` 是编译进二进制的 crate 版本）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineAccept {
    pub order: af_workorder::WorkOrder,
    pub engine_version: String,
}

/// 一次引擎调用的结局。放行与拒绝互斥，与协议一致。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineOutcome {
    Accept(EngineAccept),
    Reject(EngineRejection),
}

/// 响应线格式。`ok` 与 `order` / `error` 的互斥由本结构在解析后复核。
#[derive(Debug, Deserialize)]
struct RawResponse {
    ok: bool,
    #[serde(default)]
    order: Option<af_workorder::WorkOrder>,
    #[serde(default)]
    error: Option<EngineRejection>,
    engine_version: String,
}

/// af-engine 客户端。
#[derive(Debug, Clone)]
pub struct EngineClient {
    bin: PathBuf,
}

impl EngineClient {
    /// 定位或构建 af-engine 二进制。
    ///
    /// `workspace_root`：af 仓库根（含 `Cargo.toml` workspace）。
    /// `target_dir`：构建产物目录（ADR-009：`~/afbuild-phase2`）。
    pub fn locate(_workspace_root: &Path, _target_dir: &Path) -> Result<Self, ExeclineError> {
        unimplemented!("RED：EngineClient::locate 待最小实现")
    }

    /// 二进制路径。证明判定走的是真引擎进程，而非进程内库调用。
    pub fn bin(&self) -> &Path {
        &self.bin
    }

    /// 一次 op 调用。`request` 是完整的一行请求对象（含 `op` / `order` / 该 op 参数）。
    pub fn call(&self, _request: &Value) -> Result<EngineOutcome, ExeclineError> {
        unimplemented!("RED：EngineClient::call 待最小实现")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 拒绝线按 `kind` 内标签解析，Gate 的缺失清单保持类型化。
    #[test]
    fn rejection_wire_kinds_parse_with_typed_missing() {
        let gate: EngineRejection = serde_json::from_str(
            r#"{"kind":"Gate","gate":{"PreconditionsNotMet":{"transition":"Release","missing":["BudgetVoucherNotIssued"]}}}"#,
        )
        .expect("Gate 拒绝线应可解析");
        match &gate {
            EngineRejection::Gate { gate } => {
                assert_eq!(
                    gate.missing().unwrap(),
                    &[af_workorder::Missing::BudgetVoucherNotIssued],
                    "缺失清单类型化，顺序保持库的固定顺序"
                );
            }
            other => panic!("期望 Gate，实际 {other:?}"),
        }
        assert!(gate.to_string().contains("Gate:"), "{gate}");
        assert!(
            gate.to_string().contains("BudgetVoucherNotIssued"),
            "{gate}"
        );

        let mutation: EngineRejection = serde_json::from_str(
            r#"{"kind":"Gate","gate":{"MutationForbidden":{"state":"Released","op":"bind_line"}}}"#,
        )
        .expect("MutationForbidden 拒绝线应可解析");
        match &mutation {
            EngineRejection::Gate { gate } => {
                assert!(matches!(gate, GateRejection::MutationForbidden { .. }));
                assert!(gate.missing().is_none(), "MutationForbidden 没有缺失清单");
                assert!(mutation.to_string().contains("bind_line"), "{mutation}");
            }
            other => panic!("期望 Gate，实际 {other:?}"),
        }

        let bad: EngineRejection =
            serde_json::from_str(r#"{"kind":"BadRequest","message":"缺少字段 op"}"#)
                .expect("BadRequest 拒绝线应可解析");
        assert!(matches!(bad, EngineRejection::BadRequest { .. }));
    }

    /// 线格式以外的变体名按未知拒绝，不静默当成别的东西。
    #[test]
    fn unknown_gate_variant_is_rejected_not_ignored() {
        let err = serde_json::from_str::<EngineRejection>(
            r#"{"kind":"Gate","gate":{"SomeFutureVariant":{"x":1}}}"#,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("unknown variant"),
            "未知门控变体必须拒绝: {err}"
        );
    }
}
