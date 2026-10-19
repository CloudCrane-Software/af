//! 短期凭证。
//!
//! 生效白名单 = 规格工具名 ∩ 父凭证 `tool(...)` 事实。
//! 父凭证经 [`af_credential::attenuate`] 收成这张短期凭证：事实删不掉，check 把白名单收紧。
//! 空交集衰减到一个不会被调用的哨兵名，避免退回父凭证的全量白名单。
//!
//! 每次判定走 [`af_credential::denial`]，只注入一个 `request_tool`。
//! 不走 [`af_credential::evaluate`]：Cedar 策略只放行 `git` / `test`。
//! 事实还在只作为拒绝结果上的观察字段，不单独放行。

use crate::error::{Error, Result};
use af_credential::{attenuate, denial, deserialize_verify, inspect, serialize, ChildSpec};
use biscuit_auth::{Biscuit, PublicKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

const EMPTY_SENTINEL: &str = "__af_harness_none__";

/// 越权原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthzCode {
    /// 不在规格与父凭证事实的交集里。
    OutsideIntersection,
    /// 交集内，但短期凭证的 check 拒绝了这次调用。
    CredentialDenied,
}

impl fmt::Display for AuthzCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            AuthzCode::OutsideIntersection => "outside_intersection",
            AuthzCode::CredentialDenied => "credential_denied",
        })
    }
}

/// 一次 Run 领到的短期凭证。Debug 不打印令牌字节。
pub struct RunCredential {
    token_bytes: Vec<u8>,
    public_key: PublicKey,
    /// 求交之后的白名单，已排序去重。不含哨兵名。
    pub tools: Vec<String>,
    facts: BTreeSet<String>,
    namespace: String,
    tool_spend_cents: i64,
}

impl fmt::Debug for RunCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RunCredential")
            .field("tools", &self.tools)
            .field("namespace", &self.namespace)
            .field("tool_spend_cents", &self.tool_spend_cents)
            .finish_non_exhaustive()
    }
}

/// 用父凭证签发短期凭证。`public_key` 必须是签发这张父凭证的根公钥。
pub fn issue_run_credential(
    parent: &Biscuit,
    public_key: &PublicKey,
    spec_tools: &[String],
    namespace: &str,
    spend_cap_cents: i64,
    tool_spend_cents: i64,
) -> Result<RunCredential> {
    if spend_cap_cents < 0 || tool_spend_cents < 0 {
        return Err(Error::InvalidSpec(
            "spend_cap_cents 与 tool_spend_cents 不能为负".into(),
        ));
    }
    let parent_bytes = serialize(parent).map_err(fault)?;
    let parent = deserialize_verify(&parent_bytes, public_key).map_err(fault)?;
    let caps = inspect(&parent).map_err(fault)?;
    let mut allowed: Vec<String> = spec_tools
        .iter()
        .filter(|name| caps.tools.contains(*name))
        .cloned()
        .collect();
    allowed.sort();
    allowed.dedup();
    let whitelist = if allowed.is_empty() {
        vec![EMPTY_SENTINEL.to_string()]
    } else {
        allowed.clone()
    };
    let child = attenuate(
        &parent,
        &ChildSpec {
            tools: whitelist,
            spend_cap_cents,
        },
    )
    .map_err(fault)?;
    let token_bytes = serialize(&child).map_err(fault)?;
    deserialize_verify(&token_bytes, public_key).map_err(fault)?;
    let facts = inspect(&child).map_err(fault)?.tools.into_iter().collect();
    Ok(RunCredential {
        token_bytes,
        public_key: *public_key,
        tools: allowed,
        facts,
        namespace: namespace.to_string(),
        tool_spend_cents,
    })
}

impl RunCredential {
    /// 这次工具调用是否落在短期凭证里。
    ///
    /// `Ok(())` 表示在交集内且 `denial` 没有拒绝。
    /// 越权是 [`Error::Unauthorized`]。验签或执行限额是 [`Error::CredentialFault`]。
    pub fn authorize(&self, tool: &str) -> Result<()> {
        let fact_present = self.facts.contains(tool);
        if !self.tools.iter().any(|name| name == tool) {
            return Err(Error::Unauthorized {
                tool: tool.to_string(),
                fact_present,
                code: AuthzCode::OutsideIntersection,
                detail: "工具不在规格与父凭证事实的交集里".into(),
            });
        }
        let token = deserialize_verify(&self.token_bytes, &self.public_key).map_err(fault)?;
        match denial(&token, tool, &self.namespace, self.tool_spend_cents).map_err(fault)? {
            None => Ok(()),
            Some(denied) => Err(Error::Unauthorized {
                tool: tool.to_string(),
                fact_present,
                code: AuthzCode::CredentialDenied,
                detail: denied.display,
            }),
        }
    }
}

fn fault(err: impl ToString) -> Error {
    Error::CredentialFault(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_credential::{issue_root, RootSpec};

    fn parent(tools: &[&str]) -> (biscuit_auth::Biscuit, biscuit_auth::PublicKey) {
        let root = issue_root(&RootSpec {
            tools: tools.iter().map(|tool| (*tool).to_string()).collect(),
            namespace: "line-a".into(),
            budget_cents: 10_000,
        })
        .unwrap();
        let public_key = root.keypair.public();
        (root.token, public_key)
    }

    #[test]
    fn intersection_drops_tools_the_parent_does_not_carry() {
        let (token, public_key) = parent(&["git", "test"]);
        let issued = issue_run_credential(
            &token,
            &public_key,
            &["git".into(), "deploy".into()],
            "line-a",
            100,
            1,
        )
        .unwrap();
        assert_eq!(issued.tools, vec!["git".to_string()]);
        let err = issued.authorize("deploy").unwrap_err();
        match err {
            Error::Unauthorized {
                code: AuthzCode::OutsideIntersection,
                fact_present: false,
                ..
            } => {}
            other => panic!("{other:?}"),
        }
        issued.authorize("git").unwrap();
    }

    #[test]
    fn empty_intersection_does_not_fall_back_to_parent_facts() {
        let (token, public_key) = parent(&["git"]);
        let issued =
            issue_run_credential(&token, &public_key, &["deploy".into()], "line-a", 100, 1)
                .unwrap();
        assert!(issued.tools.is_empty());
        let err = issued.authorize("git").unwrap_err();
        match err {
            Error::Unauthorized {
                code: AuthzCode::OutsideIntersection,
                fact_present: true,
                ..
            } => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn spend_over_cap_is_credential_denied_while_the_fact_remains() {
        let (token, public_key) = parent(&["git"]);
        let issued =
            issue_run_credential(&token, &public_key, &["git".into()], "line-a", 10, 50).unwrap();
        let err = issued.authorize("git").unwrap_err();
        match err {
            Error::Unauthorized {
                code: AuthzCode::CredentialDenied,
                fact_present: true,
                detail,
                ..
            } => assert!(
                detail.contains("spend_cents") || detail.contains("<="),
                "{detail}"
            ),
            other => panic!("{other:?}"),
        }
    }
}
