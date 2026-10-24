//! af-order —— 下单门面 CLI 骨架（ADR-002 / ADR-003）。
//!
//! 纯 CLI + 状态文件 + af-engine 子进程调用，无常驻、无 docker / LLM / 网络
//! （ADR-003：下单门面无需守护进程即可受理订单）。三个子命令：
//!
//! ```text
//! af-order submit --spec <order.json> --state <dir>   # 一命令走通全生命周期
//! af-order list   --state <dir>                        # 全部订单及终态
//! af-order show   <order-id> --state <dir>             # 单订单 ledger + MES
//! ```
//!
//! `submit` 读极简 OrderSpec（order_id / line / budget / note），然后：
//! 构造订单 → bind_line → 签发预算凭证（ed25519，种子落 state 目录 0600）→
//! attach_voucher → release → start → 执行桩单元 → attach_artifact →
//! complete → close。生命周期驱动、ledger 落盘、MES 事件全部复用
//! af-execline（`OrderLifecycleDriver` / `OrderLedger` / `MesFileWriter`），
//! 门控判定一律走真 af-engine 子进程——本 crate 不重造这些轮子。
//!
//! 当前状态：RED（验收测试已落，实现未落）。

pub mod cli;
pub mod error;
pub mod spec;

pub use error::OrderError;
pub use spec::{parse_spec, OrderSpec, SpecError, SPEC_FIELDS};
