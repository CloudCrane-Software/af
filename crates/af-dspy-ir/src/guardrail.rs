//! best-of-N 之前的静态 schema 预筛。
//!
//! 优化器吐出的候选先过 IR schema：引用完整、字段名、类型约束。
//! 不合格的不进 [`ScreenBatch::admitted`]，因此没有评测入口。
//! 每一条候选的放行或拒绝都写进账本，原因是 DI 码或 JG 码。
//!
//! [`AdmittedCandidate`] 没有公开构造。评测方只能拿这次预筛交出来的副本。

use std::collections::BTreeSet;

use serde_json::Value;

use crate::codes::{JG_BLANK_BATCH, JG_BLANK_CANDIDATE, JG_DUPLICATE_CANDIDATE};
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
