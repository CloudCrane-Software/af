//! af-sandbox —— 沙箱会话客户端的语义层。
//!
//! 沙箱是工具执行面。本 crate 提供会话的打开、执行、取产物和关闭，
//! 不实现 CubeSandbox 的线路协议。测试与无环境调用方使用 [`StubHandle`]。
//! 真实 CubeSandbox 集成不在 `cargo test` 里。
//!
//! - [`Session::open`] 把调用方注入的镜像和 cpu / 内存 / 超时原样交给后端。
//! - [`Session::exec`] 要求凭证放行工具 [`SANDBOX_EXEC_TOOL`]，并且调用方已经
//!   对沙箱并发维做出 [`BudgetDecision`]。扣费在调用前完成，决策留在类型里。
//! - 后端不可用时 exec 返回 [`SandboxError::SandboxDown`]。
//! - 会话句柄带纪元和 `boot_id`。宿主 [`SandboxHost::resume`] 或后端代际变化后，
//!   旧句柄不能再执行。跨宿主重新绑定也不会向后端探活。
//!
//! 时刻由 [`Clock`] 注入。超时比较这些读数，不睡眠。

mod backend;
mod budget;
mod clock;
mod error;
mod scope;
mod session;
mod types;

pub use backend::{
    BackendExec, BackendFailure, ExecRecord, OpenGrant, OpenRecord, SandboxBackend, StubHandle,
};
pub use budget::{
    accept_charge, decide_concurrency, BudgetDecision, BudgetDimension, BudgetRefusal,
    ConcurrencyBudget, ConcurrencyCharge, ConcurrencyRequest,
};
pub use clock::{Clock, ManualClock, SystemClock};
pub use error::{Result, SandboxError};
pub use scope::{allow_sandbox_exec, CredentialScope, SANDBOX_EXEC_TOOL};
pub use session::{ExecOutput, SandboxHost, Session, SessionHandle};
pub use types::{Artifact, ArtifactSpec, Command, Image, Resources};
