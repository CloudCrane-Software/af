//! 交集判定器：有效权限 = Cedar 判 allow ∩ 凭证携带 capability（含衰减 checks）判 allow。

use anyhow::Result;
use biscuit_auth::Biscuit;

use crate::credential::{authorize, inspect, TokenCaps};
use crate::policy::cedar_allows;

/// 一次工具调用请求（主体角色 + 工具 + 命名空间 + 本次申报开销）。
pub struct AccessQuery {
    pub role: String,
    pub tool: String,
    pub namespace: String,
    pub spend_cents: i64,
}

/// 判定结果：两侧各自的结论 + 最终交集结论。
#[derive(Debug)]
pub struct Verdict {
    /// 有效权限 = cedar && biscuit
    pub allow: bool,
    pub cedar_allow: bool,
    pub biscuit_allow: bool,
    /// 凭证事实里是否携带该（namespace, tool）能力——供观察“事实还在但已被 check 封死”。
    pub token_carries_capability: bool,
    pub reason: String,
}

/// 交集判定。
///
/// biscuit 侧已经把“凭证携带的能力 + 衰减后新增的限制”编码进 authorizer；
/// Cedar 侧负责角色→工具→资源的授权面。两边都 allow 才放行。
pub fn evaluate(token: &Biscuit, q: &AccessQuery) -> Result<Verdict> {
    let cedar = cedar_allows(&q.role, &q.tool, &q.namespace)?;
    let biscuit = authorize(token, &q.tool, &q.namespace, q.spend_cents)?;
    let caps: TokenCaps = inspect(token)?;
    let token_carries_capability = caps.tools.contains(&q.tool)
        && caps.namespace.as_deref() == Some(q.namespace.as_str());
    let allow = cedar && biscuit;
    let reason = format!(
        "cedar={}, biscuit={}, token_carries({} @ {})={}, blocks={}",
        cedar, biscuit, q.tool, q.namespace, token_carries_capability, caps.block_count
    );
    Ok(Verdict {
        allow,
        cedar_allow: cedar,
        biscuit_allow: biscuit,
        token_carries_capability,
        reason,
    })
}
