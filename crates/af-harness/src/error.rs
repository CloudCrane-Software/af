use crate::authz::AuthzCode;
use crate::budget::BudgetFault;

pub type Result<T> = std::result::Result<T, Error>;

/// 执行器故障。循环的正常停止写在 [`crate::RunStatus`] 里，不在这里。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("缺少版本字段 v")]
    MissingVersion,
    #[error("版本字段无法读取: {raw}")]
    UnreadableVersion { raw: String },
    #[error("未知版本 {found}，只接受 {expected}")]
    UnknownVersion { found: u64, expected: u32 },
    #[error("信封缺少 body")]
    MissingBody,
    #[error("信封含未声明字段 {0}")]
    UnknownField(String),
    #[error("规格不合法: {0}")]
    InvalidSpec(String),
    #[error("模板槽 {0} 未声明")]
    UnknownSlot(String),
    #[error("模板槽 {0} 没有注入值")]
    MissingSlot(String),
    /// 端点返回了非 2xx。`message` 已抹掉明文密钥。
    #[error("端点 HTTP {status}: {message}")]
    Endpoint { status: u16, message: String },
    /// 传输失败。`message` 已抹掉明文密钥。
    #[error("端点传输失败: {0}")]
    Transport(String),
    #[error("协议解析失败: {0}")]
    Protocol(String),
    #[error("预算拒绝: {0}")]
    Budget(BudgetFault),
    #[error("工具 {tool} 越权 ({code}): {detail}")]
    Unauthorized {
        tool: String,
        fact_present: bool,
        code: AuthzCode,
        detail: String,
    },
    /// `denial` / `inspect` / 验签的真错误。不放行。
    #[error("凭证故障: {0}")]
    CredentialFault(String),
    #[error("声明与库内状态不符: {0}")]
    DeclarationMismatch(String),
    #[error("连接配方没有生效: {0}")]
    Recipe(String),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("effect gateway: {0}")]
    Gateway(Box<af_effect_gateway::GatewayError>),
    #[error("trace: {0}")]
    Trace(String),
    /// 副作用已经执行，本轮还没提交。续跑会重放同一幂等键。
    #[error("运行 {run_id} 在第 {turn} 轮提交前中断")]
    Interrupted { run_id: String, turn: u32 },
    #[error("工单: {0}")]
    Order(String),
}

/// 把明文密钥从错误文本里抹掉。空密钥不动原文。
pub(crate) fn redact(text: &str, secret: &str) -> String {
    if secret.is_empty() {
        return text.to_string();
    }
    text.replace(secret, "***")
}
