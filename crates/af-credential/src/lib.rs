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
//! - `budget_cents(1000)`                  —— 凭证 per-call 上限（`Cents` = i64 美分）
//! - `request_tool` / `request_ns` / `spend_cents` —— 验证方（网关）声明的本次请求事实
//!
//! 金额词表与 af-meter 对齐：`budget_cents` / `spend_cap_cents` / `spend_cents` 都是 [`Cents`]（i64 美分）。
//! 累计消耗不在凭证里，账本在 af-meter。
//!
//! 注意：衰减（append 新块）只能**加** check，不能删事实——这是 biscuit 的单调性设计。
//!
//! 持久根：[`open`] 把根私钥写在 `<dir>/root.key`（权限 0600），元数据在同目录的
//! `credential.db`。两件在 `.init` 里写完才 rename 进活目录。首启签发引导凭证并落库；
//! 重入原样重放，不重新签名。活目录只剩一件时用 [`quarantine_incomplete`] 把残件挪走。
//! [`rotate`] 换新根并记下代数。旧根签发的凭证只在调用方给出的宽限内可验。

mod clock;
mod credential;
mod judge;
mod policy;
mod root;

pub use clock::{Clock, ManualClock, SystemClock};
pub use credential::{
    attenuate, authorize, denial, deserialize_verify, inspect, issue_root, serialize, Cents,
    ChildSpec, Denial, DenialBody, DenialCheck, DenialEffect, DenialPolicy, RootSpec, TokenCaps,
    F_BUDGET_CENTS, F_NAMESPACE, F_REQUEST_NS, F_REQUEST_TOOL, F_SPEND_CENTS, F_TOOL,
};
pub use judge::{evaluate, AccessQuery, Verdict};
pub use policy::{cedar_allows, CEDAR_POLICIES};
pub use root::{
    open, open_with_clock, quarantine_incomplete, rotate, rotate_with_clock, GracePolicy,
    OpenOutcome, OpenedRoot, RootError, Rotation, SCHEMA_VERSION,
};
