//! af-workorder —— 工单作为一等对象(PoC)。
//!
//! 验证的语义(对应任务五点):
//! 1. **订单状态机 + 门控转换**:created → released → in_progress → completed → closed,
//!    可 cancelled([`state`]);release 前置 = 已绑定产线版本 + 预算凭证已签发,
//!    close 前置 = 存在合格终态工件;非法转换拒绝并**类型化指出缺什么**([`error::Missing`])。
//! 2. **SLA 字段**:时限 + 优先级([`sla`])。
//! 3. **graph_version 不可变绑定**:私有字段 + 只读访问器,无任何 setter([`order::WorkOrder`])。
//! 4. **变更单**:差异描述 + 排水/接力策略字段;换版 = 生成后继工单,原单封存([`change`])。
//! 5. **多参与者决策工件**:[`decision::DecisionArtifact`],只读角色签发 approve → 拒绝。
//!
//! 占位声明:payload_hash 用 FNV-1a64、签名是 ed25519 占位结构(不做真实密码学),
//! 均为 PoC 简化,接口形状已按最终方案预留。

pub mod change;
pub mod decision;
pub mod error;
pub mod hash;
pub mod order;
pub mod sla;
pub mod state;

pub use change::{ChangeOrder, DrainStrategy, RelayStrategy};
pub use decision::{Decision, DecisionArtifact, Permission, Role, Signature};
pub use error::{ChangeError, DecisionError, GateError, Missing};
pub use hash::{fnv1a64, fnv1a64_hex};
pub use order::{BudgetVoucher, GraphVersion, HistoryRecord, LineBinding, WorkArtifact, WorkOrder};
pub use sla::{Priority, Sla};
pub use state::{OrderState, Transition};
