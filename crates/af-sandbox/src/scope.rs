//! exec 的凭证范围。
//!
//! 判定走 [`af_credential::denial`]，一次调用只注入一个 `request_tool`，名字固定为
//! [`SANDBOX_EXEC_TOOL`]。这是 af-credential 已验证的约束：check 是存在性查询，
//! 注入多个 `request_tool` 时只要有一个落在白名单就通过。
//!
//! 不单独用 [`af_credential::inspect`] 放行。衰减只能加 check，删不掉 `tool(...)` 事实；
//! 事实还在不代表工具仍可用。`fact_present` 只作为拒绝结果上的观察字段。
//!
//! 不走 [`af_credential::evaluate`]。现有 Cedar 策略只放行 `git` / `test`，没有
//! `sandbox:exec`。角色交集不是这里的工具白名单。`spend_cents` 是凭证的单次上限，
//! 不是沙箱并发预算。

use af_credential::{denial, inspect, Cents, Denial};
use biscuit_auth::Biscuit;

use crate::error::{Result, SandboxError};

/// exec 在凭证上要求的工具名。
pub const SANDBOX_EXEC_TOOL: &str = "sandbox:exec";

/// 一次 exec 呈现给凭证层的范围。令牌只在判定期间借用。
pub struct CredentialScope<'a> {
    token: &'a Biscuit,
    namespace: &'a str,
    spend_cents: Cents,
}

impl<'a> CredentialScope<'a> {
    pub fn new(token: &'a Biscuit, namespace: &'a str, spend_cents: Cents) -> Self {
        Self {
            token,
            namespace,
            spend_cents,
        }
    }

    pub fn namespace(&self) -> &'a str {
        self.namespace
    }

    pub fn spend_cents(&self) -> Cents {
        self.spend_cents
    }
}

/// 凭证是否放行 `sandbox:exec`。
///
/// `Ok(())` 表示 denial 未拒绝，且事实层确实有这个工具。
/// 拒绝时带上原始 [`Denial`] 和事实是否仍在。
/// `denial` / `inspect` 的 `Err`（限额、损坏）是 [`SandboxError::CredentialFault`]，不放行。
pub fn allow_sandbox_exec(scope: &CredentialScope<'_>) -> Result<()> {
    let caps = inspect(scope.token).map_err(fault)?;
    let fact_present = caps.tools.contains(SANDBOX_EXEC_TOOL);
    match denial(
        scope.token,
        SANDBOX_EXEC_TOOL,
        scope.namespace,
        scope.spend_cents,
    )
    .map_err(fault)?
    {
        Some(denial) => Err(denied(fact_present, denial)),
        None if fact_present => Ok(()),
        None => Err(SandboxError::CredentialFault {
            message: "authorize 放行但事实层没有 sandbox:exec".to_string(),
        }),
    }
}

fn denied(fact_present: bool, denial: Denial) -> SandboxError {
    SandboxError::ToolDenied {
        tool: SANDBOX_EXEC_TOOL,
        fact_present,
        denial,
    }
}

fn fault(err: impl ToString) -> SandboxError {
    SandboxError::CredentialFault {
        message: err.to_string(),
    }
}
