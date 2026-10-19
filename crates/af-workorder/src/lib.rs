//! af-workorder —— 工单作为一等对象(PoC)。
//!
//! 验证的语义(对应任务五点):
//! 1. **订单状态机 + 门控转换**:created → released → in_progress → completed → closed,
//!    可 cancelled([`state`]);release 前置 = 已绑定产线版本 + 预算凭证已签发,
//!    close 前置 = 存在合格终态工件;非法转换拒绝并**类型化指出缺什么**([`error::Missing`])。
//! 2. **SLA 字段**:日历解析出的绝对截止时刻(`DateTime<Utc>`)+ 优先级([`sla`])。
//! 3. **graph_version 不可变绑定**:私有字段 + 只读访问器,无任何 setter([`order::WorkOrder`])。
//! 4. **变更单**:差异描述 + 排水/接力策略字段;换版 = 生成后继工单,原单封存([`change`])。
//! 5. **多参与者决策工件**:[`decision::DecisionArtifact`],只读角色签发 approve → 拒绝。
//!
//! payload_hash 为 SHA-256 小写十六进制([`hash`]),跨进程确定。
//! 变更单签名是真 ed25519([`sign`]):在本 crate 内签发与验签,不经过 af-credential。
//! SLA 截止已接 af-calendar,不再是字符串占位。

pub mod change;
pub mod decision;
pub mod error;
pub mod hash;
pub mod order;
pub mod sign;
pub mod sla;
pub mod state;

pub use af_calendar::{Calendar, CalendarError};
pub use change::{ChangeOrder, DrainStrategy, RelayStrategy, SignedChangeOrder};
pub use decision::{Decision, DecisionArtifact, Permission, Role, Signature};
pub use error::{ChangeError, DecisionError, GateError, Missing, SignError};
pub use hash::{sha256, sha256_hex, SHA256_HEX_LEN, SHA256_LEN};
pub use order::{BudgetVoucher, GraphVersion, HistoryRecord, LineBinding, WorkArtifact, WorkOrder};
pub use sign::{
    Ed25519Keypair, Ed25519PublicKey, ED25519_PUBLIC_KEY_LEN, ED25519_SEED_LEN,
    ED25519_SIGNATURE_LEN, SIGNATURE_ALGORITHM_ED25519,
};
pub use sla::{Priority, Sla};
pub use state::{OrderState, Transition};
