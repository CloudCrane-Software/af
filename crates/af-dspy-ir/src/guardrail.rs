//! best-of-N 之前的静态 schema 预筛。
//!
//! 优化器吐出的候选先过 IR schema：引用完整、字段名、类型约束。
//! 不合格的不进 [`ScreenBatch::admitted`]，因此没有评测入口。
//! 每一条候选的放行或拒绝都写进账本，原因是 DI 码或 JG 码。
//!
//! [`AdmittedCandidate`] 没有公开构造。评测方只能拿这次预筛交出来的副本。
//!
//! 修复判定只放行没有评测分数的 `E_INFRA` / `E_SUBJECT`。低分类一律拒绝。
//! [`RepairPermit`] 没有公开构造。

use std::collections::BTreeSet;

use serde_json::Value;

use crate::codes::{
    E_INFRA, E_LOW_SCORE, E_SCORE, E_SUBJECT, JG_BLANK_BATCH, JG_BLANK_CANDIDATE, JG_BLANK_RUN,
    JG_DUPLICATE_CANDIDATE, JG_LOW_SCORE, JG_NOT_A_CRASH, JG_SCORE_ON_CRASH,
};
use crate::error::IrFailure;
use crate::ledger::{GuardEvent, GuardReason, Ledger};
use crate::model::{blank, Program};
use crate::parse::parse_program;
use crate::Clock;

/// 护栏挂接点。预筛和修复判定都从这里进，账本也只有这里能追加。
#[derive(Debug)]
pub struct Guardrails<C: Clock> {
    clock: C,
    ledger: Ledger,
}

impl<C: Clock> Guardrails<C> {
    pub fn new(clock: C) -> Self {
        Self {
            clock,
            ledger: Ledger::new(),
        }
    }

    /// 用已有账本继续追加。恢复不会把历史行换成放行凭证。
    pub fn restore(clock: C, ledger: Ledger) -> Self {
        Self { clock, ledger }
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    pub fn clock(&self) -> &C {
        &self.clock
    }

    /// best-of-N 评测前的唯一预筛挂接点。
    ///
    /// 同一批共用调用当时的注入时刻。放行顺序与输入里合格候选的顺序一致。
    pub fn screen_before_best_of_n(
        &mut self,
        batch_id: &str,
        candidates: &[Candidate],
    ) -> ScreenBatch {
        let at_ms = self.clock.now_ms();
        let mut seen = BTreeSet::new();
        let mut admitted = Vec::new();
        let mut rejected = Vec::new();
        let batch_blank = blank(batch_id);

        for candidate in candidates {
            let mut reasons = Vec::new();
            if batch_blank {
                reasons.push(reason(JG_BLANK_BATCH, "批次 id 为空，整批不进评测"));
            }
            if blank(&candidate.id) {
                reasons.push(reason(JG_BLANK_CANDIDATE, "候选 id 为空"));
            } else if !seen.insert(candidate.id.clone()) {
                reasons.push(reason(
                    JG_DUPLICATE_CANDIDATE,
                    format!(
                        "候选 id `{}` 在本批里重复，后来的这条不进评测",
                        candidate.id
                    ),
                ));
            }

            match check_schema(candidate) {
                SchemaCheck::Pass { program } if reasons.is_empty() => {
                    let seq = self.ledger.append(
                        at_ms,
                        GuardEvent::ScreenAdmit {
                            batch_id: batch_id.to_string(),
                            candidate_id: candidate.id.clone(),
                            program_id: program.program_id.clone(),
                        },
                    );
                    admitted.push(AdmittedCandidate {
                        id: candidate.id.clone(),
                        program,
                        ledger_seq: seq,
                    });
                }
                SchemaCheck::Pass { program } => {
                    let program_id = Some(program.program_id);
                    rejected.push(self.record_reject(
                        at_ms,
                        batch_id,
                        &candidate.id,
                        program_id,
                        reasons,
                    ));
                }
                SchemaCheck::Fail {
                    program_id,
                    reasons: schema,
                } => {
                    reasons.extend(schema);
                    rejected.push(self.record_reject(
                        at_ms,
                        batch_id,
                        &candidate.id,
                        program_id,
                        reasons,
                    ));
                }
            }
        }

        ScreenBatch {
            batch_id: batch_id.to_string(),
            admitted,
            rejected,
        }
    }

    fn record_reject(
        &mut self,
        at_ms: i64,
        batch_id: &str,
        candidate_id: &str,
        program_id: Option<String>,
        reasons: Vec<GuardReason>,
    ) -> RejectedCandidate {
        let seq = self.ledger.append(
            at_ms,
            GuardEvent::ScreenReject {
                batch_id: batch_id.to_string(),
                candidate_id: candidate_id.to_string(),
                program_id: program_id.clone(),
                reasons: reasons.clone(),
            },
        );
        RejectedCandidate {
            id: candidate_id.to_string(),
            ledger_seq: seq,
            program_id,
            reasons,
        }
    }

    /// 修复动作前的唯一判定挂接点。只修崩溃，不修低分。
    ///
    /// # Examples
    /// ```
    /// use af_dspy_ir::{FailureClass, Guardrails, ManualClock, RunResult, JG_LOW_SCORE};
    ///
    /// let mut guards = Guardrails::new(ManualClock::new(1));
    /// let verdict = guards.judge_repair(&RunResult {
    ///     run_id: "run-1".into(),
    ///     failure: Some(FailureClass::LowScore),
    ///     scored: true,
    /// });
    /// assert!(verdict.permit().is_none());
    /// assert_eq!(verdict.denial().unwrap().reason_code(), JG_LOW_SCORE);
    /// ```
    pub fn judge_repair(&mut self, run: &RunResult) -> RepairVerdict {
        let at_ms = self.clock.now_ms();
        let failure_code = ledger_failure_code(run.failure.as_ref());
        match decide(run) {
            RepairChoice::Allow(code) => {
                let seq = self.ledger.append(
                    at_ms,
                    GuardEvent::RepairAllow {
                        run_id: run.run_id.clone(),
                        code: code.as_str().to_string(),
                        scored: false,
                    },
                );
                RepairVerdict::Allow(RepairPermit {
                    run_id: run.run_id.clone(),
                    code,
                    ledger_seq: seq,
                })
            }
            RepairChoice::Deny {
                reason_code,
                message,
            } => {
                let seq = self.ledger.append(
                    at_ms,
                    GuardEvent::RepairDeny {
                        run_id: run.run_id.clone(),
                        failure_code,
                        scored: run.scored,
                        reason_code: reason_code.to_string(),
                        message: message.clone(),
                    },
                );
                RepairVerdict::Deny(RepairDenial {
                    run_id: run.run_id.clone(),
                    ledger_seq: seq,
                    reason_code: reason_code.to_string(),
                    message,
                })
            }
        }
    }
}

/// 优化器交出的一个候选。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub id: String,
    pub ir: CandidateIr,
}

/// 候选的 IR 形态。JSON 会先过拒绝式信封，再过与值相同的语义校验。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateIr {
    Program(Program),
    Json(String),
}

/// 一次预筛的结果。放行列表里没有不合格候选。
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct ScreenBatch {
    batch_id: String,
    admitted: Vec<AdmittedCandidate>,
    rejected: Vec<RejectedCandidate>,
}

impl ScreenBatch {
    pub fn batch_id(&self) -> &str {
        &self.batch_id
    }

    pub fn admitted(&self) -> &[AdmittedCandidate] {
        &self.admitted
    }

    pub fn rejected(&self) -> &[RejectedCandidate] {
        &self.rejected
    }
}

/// 允许进入 best-of-N 评测的候选。程序是预筛当时校验通过的副本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedCandidate {
    id: String,
    program: Program,
    ledger_seq: u64,
}

impl AdmittedCandidate {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn program(&self) -> &Program {
        &self.program
    }

    pub fn into_program(self) -> Program {
        self.program
    }

    /// 对应账本里 `screen_admit` 那一行的序号。
    pub fn ledger_seq(&self) -> u64 {
        self.ledger_seq
    }
}

/// 被预筛拒掉的候选。调用方不得把它送去评测。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedCandidate {
    id: String,
    ledger_seq: u64,
    program_id: Option<String>,
    reasons: Vec<GuardReason>,
}

impl RejectedCandidate {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn ledger_seq(&self) -> u64 {
        self.ledger_seq
    }

    pub fn program_id(&self) -> Option<&str> {
        self.program_id.as_deref()
    }

    pub fn reasons(&self) -> &[GuardReason] {
        &self.reasons
    }

    pub fn has_code(&self, code: &str) -> bool {
        self.reasons.iter().any(|reason| reason.code == code)
    }
}

enum SchemaCheck {
    Pass {
        program: Program,
    },
    Fail {
        program_id: Option<String>,
        reasons: Vec<GuardReason>,
    },
}

fn check_schema(candidate: &Candidate) -> SchemaCheck {
    match &candidate.ir {
        CandidateIr::Program(program) => match program.validate() {
            Ok(()) => SchemaCheck::Pass {
                program: program.clone(),
            },
            Err(failure) => SchemaCheck::Fail {
                program_id: Some(program.program_id.clone()),
                reasons: schema_reasons(&failure),
            },
        },
        CandidateIr::Json(text) => match parse_program(text) {
            Ok(program) => SchemaCheck::Pass { program },
            Err(failure) => SchemaCheck::Fail {
                program_id: peek_program_id(text),
                reasons: schema_reasons(&failure),
            },
        },
    }
}

fn schema_reasons(failure: &IrFailure) -> Vec<GuardReason> {
    failure
        .errors
        .iter()
        .map(|err| GuardReason {
            code: err.code_str().to_string(),
            message: err.message.clone(),
        })
        .collect()
}

fn peek_program_id(text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(text).ok()?;
    value
        .get("program_id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn reason(code: &str, message: impl Into<String>) -> GuardReason {
    GuardReason {
        code: code.to_string(),
        message: message.into(),
    }
}

/// 上次运行的失败码分类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureClass {
    /// 基础设施崩溃，码 [`E_INFRA`]。
    Infra,
    /// 候选程序自身崩溃，码 [`E_SUBJECT`]。
    Subject,
    /// 低分。一律拒绝修复。
    LowScore,
    /// 其他失败码。文本原样保留。
    Other(String),
}

impl FailureClass {
    /// 按失败码字符串归类。只认精确的崩溃码和低分码。
    pub fn parse(code: &str) -> Self {
        match code {
            E_INFRA => Self::Infra,
            E_SUBJECT => Self::Subject,
            E_SCORE | E_LOW_SCORE => Self::LowScore,
            other => Self::Other(other.to_string()),
        }
    }

    /// 账本上的原始码。低分枚举写成 [`E_SCORE`]。
    pub fn as_code(&self) -> &str {
        match self {
            Self::Infra => E_INFRA,
            Self::Subject => E_SUBJECT,
            Self::LowScore => E_SCORE,
            Self::Other(code) => code.as_str(),
        }
    }

    fn normalized(self) -> Self {
        match self {
            Self::Other(code) => Self::parse(&code),
            other => other,
        }
    }
}

/// 上次运行结果。`scored` 表示这次运行留下了评测分数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    pub run_id: String,
    pub failure: Option<FailureClass>,
    pub scored: bool,
}

/// 允许修复的崩溃类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashCode {
    Infra,
    Subject,
}

impl CrashCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Infra => E_INFRA,
            Self::Subject => E_SUBJECT,
        }
    }
}

impl std::fmt::Display for CrashCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 修复判定。忽略它就会丢掉账本序号，所以必须使用。
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub enum RepairVerdict {
    Allow(RepairPermit),
    Deny(RepairDenial),
}

impl RepairVerdict {
    pub fn permit(&self) -> Option<&RepairPermit> {
        match self {
            Self::Allow(permit) => Some(permit),
            Self::Deny(_) => None,
        }
    }

    pub fn denial(&self) -> Option<&RepairDenial> {
        match self {
            Self::Deny(denial) => Some(denial),
            Self::Allow(_) => None,
        }
    }
}

/// 允许进入修复的凭证。只有没有分数的崩溃类能拿到。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairPermit {
    run_id: String,
    code: CrashCode,
    ledger_seq: u64,
}

impl RepairPermit {
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn code(&self) -> CrashCode {
        self.code
    }

    /// 对应账本里 `repair_allow` 那一行的序号。
    pub fn ledger_seq(&self) -> u64 {
        self.ledger_seq
    }
}

/// 拒绝修复。低分、非崩溃、带分数的崩溃码、空运行 id 都在这里。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairDenial {
    run_id: String,
    ledger_seq: u64,
    reason_code: String,
    message: String,
}

impl RepairDenial {
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn ledger_seq(&self) -> u64 {
        self.ledger_seq
    }

    pub fn reason_code(&self) -> &str {
        &self.reason_code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

enum RepairChoice {
    Allow(CrashCode),
    Deny {
        reason_code: &'static str,
        message: String,
    },
}

fn decide(run: &RunResult) -> RepairChoice {
    if blank(&run.run_id) {
        return RepairChoice::Deny {
            reason_code: JG_BLANK_RUN,
            message: "run_id 为空，拒绝修复".to_string(),
        };
    }
    let class = run.failure.clone().map(FailureClass::normalized);
    if matches!(class, Some(FailureClass::LowScore)) {
        return RepairChoice::Deny {
            reason_code: JG_LOW_SCORE,
            message: "低分类一律拒绝修复".to_string(),
        };
    }
    let crash = match class {
        Some(FailureClass::Infra) => Some(CrashCode::Infra),
        Some(FailureClass::Subject) => Some(CrashCode::Subject),
        _ => None,
    };
    if let Some(code) = crash {
        if run.scored {
            return RepairChoice::Deny {
                reason_code: JG_SCORE_ON_CRASH,
                message: format!("崩溃码 {code} 附带了评测分数，拒绝修复"),
            };
        }
        return RepairChoice::Allow(code);
    }
    RepairChoice::Deny {
        reason_code: JG_NOT_A_CRASH,
        message: not_a_crash_message(run),
    }
}

fn not_a_crash_message(run: &RunResult) -> String {
    match run.failure.as_ref() {
        None => "没有崩溃码，拒绝修复".to_string(),
        Some(class) if blank(class.as_code()) => "失败码为空，不是崩溃类，拒绝修复".to_string(),
        Some(class) => format!("失败码 {} 不是崩溃类，拒绝修复", class.as_code()),
    }
}

fn ledger_failure_code(failure: Option<&FailureClass>) -> Option<String> {
    let code = failure?.as_code();
    if blank(code) {
        None
    } else {
        Some(code.to_string())
    }
}
