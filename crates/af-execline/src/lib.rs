//! af-execline —— 统一执行面 PoC（订单生命周期驱动）。
//!
//! 定位（ADR-002）：第二阶段 `crates/af-execline` 的第一块砖——证明订单语义与
//! 执行可编程耦合。P4 起 swe-v 真实执行单元（[`suite::swev_unit`]）在订单面
//! 之外也可直调（判分链 + 结果行先独立验收；驱动层接线 = P5）：
//!
//! ```text
//! 构造订单 → af-engine 门控（bind_line→attach_voucher→release→start）
//!          → 执行单元（桩或 swe-v：setup→run→gate→verify→行落盘）
//!          → attach_artifact → complete → close
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

pub mod agent;
pub mod clock;
pub mod driver;
pub mod engine;
pub mod error;
pub mod exec;
pub mod graph_unit;
pub mod hostbridge;
pub mod judge;
pub mod ledger;
pub mod mes;
pub mod mes_ch;
pub mod pkg;
pub mod planbind;
pub mod profile;
pub mod record;
pub mod shadow;
pub mod suite;
pub mod tracetree;

pub use agent::{
    bench_paper_order, AgentLaunch, AgentRunFailure, AgentRunOutcome, AgentRunner, BashFn,
    HarnessRunner, ToolBinding, ToolFn,
};
pub use clock::{Clock, FixedClock, SystemClock};
pub use driver::{AttemptLayout, DriveReport, DriverConfig, OrderLifecycleDriver};
pub use engine::{EngineAccept, EngineClient, EngineOutcome, EngineRejection};
pub use error::ExeclineError;
pub use exec::{file_sha256_hex, ExecContext, ExecOutput, ExecuteUnit, StubExecuteUnit};
pub use graph_unit::{linearize, GraphExecuteUnit, GraphUnitSpec};
pub use hostbridge::{HostBridge, SshBridge};
pub use judge::{invoke as judge_invoke, ImportAnchor, JudgeLocator};
pub use ledger::{LedgerRecord, OrderLedger};
pub use mes::{MesFileWriter, MesSink, MesSyncKind, MesSyncReport};
pub use mes_ch::{
    resolve_mes_ch_endpoint, resolve_mes_ch_endpoint_with, MesChEndpoint, MesChSink, MesChTarget,
};
pub use planbind::{bind as planbind_bind, BindExpectations, PlanBind};
pub use record::{ExecutionFacts, FileRecordSink, PendingRecordSink, RecordSink, TbExecutionFacts};
pub use suite::report_unit::{
    draft_fn_real_ring, html_to_text, invoke_citation, invoke_rubric, validate_envelope,
    ArxivWebFetcher, DraftFn, DraftInput, DraftOutput, FetchedDoc, IngestFetcher, LocalFileFetcher,
    PaperEnvelope, ReportExecuteUnit, ReportJudgeLocator, ReportTask, ReportUnitSpec, RingTypes,
    SourceFetcher,
};
pub use suite::swev_unit::{SwevExecuteUnit, SwevUnitSpec};
pub use suite::tb_unit::{TbExecuteUnit, TbUnitSpec};
