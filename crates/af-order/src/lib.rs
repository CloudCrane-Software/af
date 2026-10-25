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
//! 状态目录布局（[`state`]）：
//!
//! ```text
//! <state>/orders/<order_id>/{ledger.jsonl, mes-events.jsonl, artifacts/,
//!                           voucher.json, voucher-key.seed(0600)}
//! ```
//!
//! 错误码契约（ADR-007 §3）：OR1xxx（用法）/ OR2xxx（spec / 凭证 / 状态）；
//! 引擎与账本失败透传 af-execline 的 EX 稳定码，见 [`error`]。

pub mod cli;
pub mod error;
pub mod exec;
pub mod spec;
pub mod state;
pub mod voucher;

pub use error::OrderError;
pub use exec::{
    parse_exec_flags, parse_unit_choice, report_unit_for, resolve_unit, swev_unit_spec_for,
    RealExec, SelectedUnit, UnitChoice, EXEC_FLAGS,
};
pub use spec::{parse_spec, OrderSpec, SpecError, SPEC_FIELDS};
pub use state::{OrderSummary, StateDir};
pub use voucher::{issue_or_load, VoucherRecord};
