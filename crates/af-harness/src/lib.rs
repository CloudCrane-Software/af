//! af-harness —— 统一 agent 执行器外壳。
//!
//! 程序性是数据：一次 Run 由四元组 [`AgentSpec`] 驱动，而不是由一段写死的代理代码驱动。
//!
//! 1. **端点**：协议、base_url、模型、密钥引用、推理档位。明文密钥由调用方注入，不进规格。
//! 2. **上下文模板**：系统提示加 `{{槽}}`。渲染是字面替换，不引入模板库。
//! 3. **工具集**：名称、JSON Schema、副作用分类。纯函数本地执行；需落盘和一次性外部操作走
//!    [`af_effect_gateway`]（意图登记、幂等、纪元栅栏）。
//! 4. **策略**：轮次上限、完成信号词、连续无工具轮上限、预算维度引用。
//!
//! 每轮顺序：渲染 → 调模型 → 解析工具调用 → 按分类执行 → 写 [`af_trace`] →
//! [`af_stop`] 止损 → [`af_meter`] 按轮扣预算。预算不足或止损命中就停，不再开下一轮。
//! 每轮开始前如果余额已经不够，模型不会被调用。
//!
//! 每个 Run 用父凭证衰减出一张短期凭证。生效白名单是「规格工具集 ∩ 父凭证事实」。
//! 判定走 [`af_credential::denial`]，一次只注入一个 `request_tool`。
//! 不走 [`af_credential::evaluate`]：现有 Cedar 策略只放行 `git` / `test`，盖不住执行器工具名。
//!
//! Run 状态落在调用方目录的 `harness.db`（WAL、`Created` / `AlreadyDeclared`、声明不符即失败）。
//! 断点续跑恢复上下文快照和轮次。时刻经 [`Clock`] 注入。
//!
//! 真实供应商调用留在 `examples/smoke.rs`。`cargo test` 只打本 crate 的 [`stub`] HTTP 服务。

mod authz;
mod budget;
mod clock;
mod endpoint;
mod error;
mod http;
mod render;
mod run;
mod spec;
mod store;
pub mod stub;

pub use authz::{issue_run_credential, AuthzCode, RunCredential};
pub use budget::{BudgetBinding, BudgetFault, BudgetRecord};
pub use clock::{Clock, ManualClock, SystemClock};
pub use endpoint::{
    complete_once, EndpointKey, ModelMessage, ModelUsage, ParsedToolCall, TranscriptItem,
    ANTHROPIC_VERSION,
};
pub use error::Error;
pub use http::{HttpClient, HttpResponse, UreqClient};
pub use render::render;
pub use run::{
    Breakpoint, Launch, Opened, RunReport, RunStatus, TokenTotals, ToolFailure, ToolRegistry,
};
pub use spec::{
    decode_spec, encode_spec, AgentSpec, AuthStyle, BudgetRef, BudgetUnit, ContextTemplate,
    EffectClass, EndpointSpec, Policy, Protocol, ReasoningTier, StopCondition, ToolDecl,
    SPEC_VERSION,
};
pub use store::OpenStatus;

/// 四元组样例。与 `spec.example.json` 同一份字节。
pub const EXAMPLE_SPEC_JSON: &str = include_str!("../spec.example.json");

#[cfg(test)]
mod tests;

pub type Result<T> = std::result::Result<T, Error>;
