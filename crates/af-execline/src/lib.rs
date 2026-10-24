//! af-execline —— 统一执行面 PoC（订单生命周期驱动）。
//!
//! 定位（ADR-002）：第二阶段 `crates/af-execline` 的第一块砖——证明订单语义与
//! 执行可编程耦合。纯进程内实现，不连 docker / LLM / 网络：
//!
//! ```text
//! 构造订单 → af-engine 门控（bind_line→attach_voucher→release→start）
//!          → 执行桩单元（产出工件）→ attach_artifact → complete → close
//! ```
//!
//! 判定一律走真 af-engine 子进程（9-op 单行 JSON stdin/stdout 协议，不打桩，
//! 见 [`engine::EngineClient`]）。三条账本全程落盘（ADR-003：无常驻编排引擎，
//! 订单语义由引擎 + ledger + MES 事件账本承载）：
//!
//! - 订单 ledger：`ledger.jsonl`，append-only，每行带连续序号、op、幂等键、
//!   判定结果与整单快照（[`ledger::OrderLedger`]）；
//! - MES 事件：`mes-events.jsonl`，`af_mes::MesEvent` 的 JSONL stub 落盘，
//!   不连 ClickHouse（[`mes::MesFileWriter]；ADR-004 过渡态，目标态由 af-mes
//!   outbox 承接）；
//! - 工件：执行单元产出文件（[`exec::ExecuteUnit]），登记为 `WorkArtifact`
//!   后经引擎 `attach_artifact` 入单。
//!
//! 幂等语义（落地契约 v0 §6.2）：每个 op 带幂等键 `{stable_id}/{op}`；
//! 同一 order_id 二次驱动只重放已完成的 op，不产生重复终态。
//!
//! 错误码契约（ADR-007）：稳定码 EX1xxx（引擎）/ EX2xxx（账本）/ EX3xxx
//! （执行单元），数字只增不改，见 [`error::ErrorCode]]。

pub mod clock;
pub mod driver;
pub mod engine;
pub mod error;
pub mod exec;
pub mod ledger;
pub mod mes;
pub mod profile;
pub mod suite;

pub use clock::{Clock, FixedClock, SystemClock};
pub use driver::{DriveReport, DriverConfig, OrderLifecycleDriver};
pub use engine::{EngineAccept, EngineClient, EngineOutcome, EngineRejection};
pub use error::ExeclineError;
pub use exec::{ExecContext, ExecOutput, ExecuteUnit, StubExecuteUnit};
pub use ledger::{LedgerRecord, OrderLedger};
pub use mes::MesFileWriter;
