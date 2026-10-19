//! af-trace —— 分级背压追踪通道。
//!
//! 双通道语义：
//!
//! - **critical（移交同步、落盘确认、fail-closed）**：[`Trace::write_critical`] 把事件放进有界
//!   critical 缓冲。缓冲满时立即返回 `Err(TraceError::CriticalFull)`，事件原样退还。
//!   后台线程把事件以 JSONL 同步追加到调用方目录的 `critical.jsonl`，`sync_data` 成功后才记入
//!   镜像。写盘失败则留下事件并返回 [`TraceError::PersistFailed`]，调用方决定重试或中止。
//! - **normal（异步、有损）**：[`Trace::write_normal`] 放入有界队列，由后台线程追加到
//!   `normal.jsonl`（不逐条 fsync）。队列满或写盘失败都丢弃并 `gap_counter += 1`，不阻塞生产者。
//! - **OTel 非关键 lane（feature `otel`，默认关）**：落盘成功的 normal 事件可按
//!   [`TraceConfig`] 里的端点与批大小（配方层写入）导出成 OTLP span。导出失败或该 lane
//!   队列满只把同一条 `gap_counter` 加一，不阻塞生产者，也不让 [`Trace::open`] 失败。
//!   未开 feature、未配端点、或 exporter 建不起来时 lane 离线，只计数，不改缺口。
//!   critical 事件（含 gap span）不进入 OTel。真实 collector 不在 `cargo test` 里。
//! - **缺口可验证**：每次丢弃（磁盘或在线 OTel）都尝试发出 `kind="gap"` 的 critical span；
//!   critical 也满时合并，[`Trace::shutdown`] 强制补发。不变量：
//!   **`Σ(gap span.count) == gap_counter`**。
//!   OTel lane 离线时（默认），`gap_counter` 仍等于磁盘丢弃数。
//!   该计数写入 `trace.db`，与 critical 文件里的缺口合计一致；重开时对不上则启动失败。
//!
//! [`Trace::open`] 首启只接受空库，返回 [`OpenOutcome::Created`]（建表并写入声明）。
//! 重入返回 [`OpenOutcome::AlreadyDeclared`]，并核对 schema 版本、JSONL、critical 同步耐久性。
//! 外来 SQLite 对象或声明缺行都拒绝，不建表、不补行。声明与已存状态不符则失败，不静默迁就。
//!
//! 时刻经 [`Clock`] 注入，写入事件的 `ts_ms`。测试用 [`ManualClock`]。
//!
//! 契约：[`Trace::shutdown`] 必须在所有生产者线程结束之后调用。

mod clock;
mod error;
mod event;
mod otel;
mod store;
mod trace;

pub use clock::{Clock, ManualClock, SystemClock};
pub use error::TraceError;
pub use event::{sum_gap_counts, GapInfo, Level, TraceEvent};
pub use otel::OtelMode;
pub use store::{store_path, OpenOutcome};
pub use trace::{
    critical_log_path, normal_log_path, read_trace_log, ConsumerHold, ShutdownReport, Trace,
    TraceConfig, TraceSinks,
};
