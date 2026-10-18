//! af-credential —— 凭证签发与权限求交 PoC
//!
//! 职责：
//! 1. 用 biscuit-auth 6.x 签发根凭证（工具白名单 / 命名空间 / 预算），并支持衰减（spawn 子 agent）。
//! 2. 用 cedar-policy 4.x 写角色策略（Reviewer 可 test / builder 只可 git）。
//! 3. 交集判定器：有效权限 = Cedar 判 allow ∩ 凭证（含衰减后 checks）判定 allow。
//!
//! 事实词表（biscuit 6 无 schema/词表机制，词表只能靠这里的常量约定）：
//! - `tool("git")` / `tool("test")`        —— 凭证携带的能力（authority 块）
//! - `namespace("line-a")`                 —— 凭证绑定的命名空间
//! - `budget_cents(1000)`                  —— 凭证预算（biscuit 6 已删除 Float，用“美分”整数）
//! - `request_tool` / `request_ns` / `spend_cents` —— 验证方（网关）声明的本次请求事实
//!
//! 注意：衰减（append 新块）只能**加** check，不能删事实——这是 biscuit 的单调性设计。

mod credential;
mod judge;
mod policy;

pub use credential::{
    authorize, attenuate, deserialize_verify, inspect, issue_root, serialize, ChildSpec, RootSpec,
    TokenCaps, F_BUDGET_CENTS, F_NAMESPACE, F_REQUEST_NS, F_REQUEST_TOOL, F_SPEND_CENTS, F_TOOL,
};
pub use judge::{evaluate, AccessQuery, Verdict};
pub use policy::{cedar_allows, CEDAR_POLICIES};
