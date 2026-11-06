//! af-dspy-ir — DSPy 程序的 IR schema v1。
//!
//! 这是 L2 适配器侧的 schema 件，不是内核抽象。
//!
//! DSPy `Signature.dump_state` 的 `fields` 只有 `prefix` 和 `description`，没有字段名；
//! `load_state` 按位置套回调用方手里的签名。`Signature.delete` 对不存在的名字是
//! `fields.pop(name, None)`，不报错。本 crate 把带字段名的 IR 当作真相源：
//! 缺字段名、坏引用、删除不存在的字段都显式失败。
//!
//! 文档是拒绝式信封。没有 `version`，或 `version` 不是整数 `1`，整份拒绝，不默认成 v1。
//! 字段清单见 [`V1_FIELDS`]。
//!
//! JIT 护栏挂在本 crate 上，不改 schema v1 的校验语义。
//! best-of-N 之前走 [`Guardrails::screen_before_best_of_n`]。
//! 修复之前走 [`Guardrails::judge_repair`]：只修崩溃，不修低分。
//! 时刻用 [`Clock`] 注入，只抄进 [`Ledger`]，不参与放行。
//! 规格在 crate 根目录的 `JIT-GUARDRAILS.md`。

mod archive;
mod clock;
mod codes;
mod error;
mod fields;
mod guardrail;
mod ledger;
mod model;
mod parse;
mod validate;

pub use archive::{apply_dspy_archive, to_dspy_archive};
pub use clock::{Clock, ManualClock, SystemClock};
pub use codes::{
    E_INFRA, E_LOW_SCORE, E_SCORE, E_SUBJECT, JG_BLANK_BATCH, JG_BLANK_CANDIDATE, JG_BLANK_RUN,
    JG_DUPLICATE_CANDIDATE, JG_LOW_SCORE, JG_NOT_A_CRASH, JG_SCORE_ON_CRASH,
};
pub use error::{ErrorCode, IrError, IrFailure};
pub use fields::{FieldSpec, V1_FIELDS};
pub use guardrail::{
    AdmittedCandidate, Candidate, CandidateIr, CrashCode, FailureClass, Guardrails,
    RejectedCandidate, RepairDenial, RepairPermit, RepairVerdict, RunResult, ScreenBatch,
};
pub use ledger::{GuardEvent, GuardReason, Ledger, LedgerEntry, LedgerError, LEDGER_VERSION};
pub use model::{
    Demo, Field, LmConfig, OptimizationState, OptimizationStatus, Predictor, PredictorConfig,
    Program, Signature,
};
pub use parse::{parse_program, to_json};

/// 当前 IR 文档版本。信封里的 `version` 只能是这个数。
pub const IR_VERSION: u32 = 1;
