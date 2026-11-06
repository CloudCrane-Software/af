//! JIT 护栏：best-of-N 前的静态 schema 预筛，以及 repair_only_on_error。

use std::collections::BTreeMap;

use af_dspy_ir::{
    parse_program, to_json, Candidate, CandidateIr, Clock, CrashCode, Demo, ErrorCode,
    FailureClass, Field, GuardEvent, Guardrails, Ledger, ManualClock, OptimizationState,
    OptimizationStatus, Predictor, PredictorConfig, Program, RunResult, Signature, SystemClock,
    E_INFRA, E_LOW_SCORE, E_SCORE, E_SUBJECT, IR_VERSION, JG_BLANK_BATCH, JG_BLANK_CANDIDATE,
    JG_BLANK_RUN, JG_DUPLICATE_CANDIDATE, JG_LOW_SCORE, JG_NOT_A_CRASH, JG_SCORE_ON_CRASH,
};
use serde_json::{json, Value};

fn field(name: &str, type_name: &str) -> Field {
    Field {
        name: name.to_string(),
        prefix: format!("{name}:"),
        description: String::new(),
        type_name: type_name.to_string(),
    }
}

fn valid_program() -> Program {
    let mut values = BTreeMap::new();
    values.insert("question".to_string(), json!("What is 2+2?"));
    values.insert("answer".to_string(), json!("4"));
    Program {
        version: IR_VERSION,
        program_id: "qa-cot".to_string(),
        signatures: vec![Signature {
            name: "BasicQA".to_string(),
            instructions: "Answer questions with short factoid answers.".to_string(),
            inputs: vec![field("question", "str")],
            outputs: vec![field("answer", "str")],
        }],
        demos: vec![Demo {
            id: "demo-1".to_string(),
            signature: "BasicQA".to_string(),
            values,
        }],
        predictors: vec![Predictor {
            name: "generate_answer".to_string(),
            signature: "BasicQA".to_string(),
            demos: vec!["demo-1".to_string()],
            config: PredictorConfig {
                temperature_millis: None,
                rollout_id: None,
                n: None,
            },
            lm: None,
        }],
        optimization: OptimizationState {
            optimizer: None,
            status: OptimizationStatus::Unoptimized,
            metric: None,
            score_millis: None,
            compiled: false,
        },
    }
}

fn candidate(id: &str, program: Program) -> Candidate {
    Candidate {
        id: id.to_string(),
        ir: CandidateIr::Program(program),
    }
}

fn json_candidate(id: &str, program: &Program) -> Candidate {
    Candidate {
        id: id.to_string(),
        ir: CandidateIr::Json(to_json(program).expect("valid ir json")),
    }
}

fn guards(at_ms: i64) -> (ManualClock, Guardrails<ManualClock>) {
    let clock = ManualClock::new(at_ms);
    let guards = Guardrails::new(clock.clone());
    (clock, guards)
}

#[test]
fn valid_program_and_json_are_admitted_with_injected_time() {
    let (clock, mut guards) = guards(-25);
    let program = valid_program();
    let batch = guards.screen_before_best_of_n(
        "bo-1",
        &[
            candidate("c-program", program.clone()),
            json_candidate("c-json", &program),
        ],
    );
    assert_eq!(batch.batch_id(), "bo-1");
    assert!(batch.rejected().is_empty());
    assert_eq!(batch.admitted().len(), 2);
    assert_eq!(batch.admitted()[0].id(), "c-program");
    assert_eq!(batch.admitted()[0].program(), &program);
    assert_eq!(batch.admitted()[0].ledger_seq(), 1);
    assert_eq!(batch.admitted()[1].id(), "c-json");
    assert_eq!(batch.admitted()[1].program(), &program);
    assert_eq!(batch.admitted()[1].clone().into_program(), program);

    assert_eq!(guards.ledger().len(), 2);
    for entry in guards.ledger().entries() {
        assert_eq!(entry.at_ms, -25);
        assert!(matches!(entry.event, GuardEvent::ScreenAdmit { .. }));
    }
    clock.advance(0);
    assert_eq!(Clock::now_ms(guards.clock()), -25);

    let empty = guards.screen_before_best_of_n("bo-empty", &[]);
    assert!(empty.admitted().is_empty());
    assert_eq!(guards.ledger().len(), 2);
}

#[test]
fn schema_failures_never_enter_evaluation() {
    let (_clock, mut guards) = guards(10);
    let mut bad_sig = valid_program();
    bad_sig.predictors[0].signature = "MissingSig".to_string();
    let mut bad_demo = valid_program();
    bad_demo.predictors[0].demos = vec!["missing-demo".to_string()];
    let mut bad_name = valid_program();
    bad_name.signatures[0].inputs[0].name.clear();
    let mut bad_type = valid_program();
    bad_type.signatures[0].outputs[0].type_name = " ".to_string();
    let mut bad_n = valid_program();
    bad_n.predictors[0].config.n = Some(0);
    let mut cross = valid_program();
    cross.signatures.push(Signature {
        name: "Grade".to_string(),
        instructions: String::new(),
        inputs: vec![field("question", "str")],
        outputs: vec![field("grade", "str")],
    });
    let mut grade_values = BTreeMap::new();
    grade_values.insert("question".to_string(), json!("q"));
    grade_values.insert("grade".to_string(), json!("A"));
    cross.demos.push(Demo {
        id: "demo-2".to_string(),
        signature: "Grade".to_string(),
        values: grade_values,
    });
    cross.predictors[0].demos = vec!["demo-2".to_string()];

    let mut unknown_key =
        serde_json::from_str::<Value>(&to_json(&valid_program()).expect("json")).expect("value");
    unknown_key
        .as_object_mut()
        .expect("object")
        .insert("extra".to_string(), json!(1));
    let mut bad_version = unknown_key.clone();
    bad_version["version"] = json!(2);
    bad_version.as_object_mut().expect("object").remove("extra");
    let mut missing_field =
        serde_json::from_str::<Value>(&to_json(&valid_program()).expect("json")).expect("value");
    missing_field
        .as_object_mut()
        .expect("object")
        .remove("program_id");

    let cases = [
        (
            "bad-sig",
            CandidateIr::Program(bad_sig),
            ErrorCode::UnknownSignature.as_str(),
        ),
        (
            "bad-demo",
            CandidateIr::Program(bad_demo),
            ErrorCode::UnknownDemo.as_str(),
        ),
        (
            "bad-name",
            CandidateIr::Program(bad_name),
            ErrorCode::MissingFieldName.as_str(),
        ),
        (
            "bad-type",
            CandidateIr::Program(bad_type),
            ErrorCode::InvalidValue.as_str(),
        ),
        (
            "bad-n",
            CandidateIr::Program(bad_n),
            ErrorCode::InvalidValue.as_str(),
        ),
        (
            "cross",
            CandidateIr::Program(cross),
            ErrorCode::DemoSignatureMismatch.as_str(),
        ),
        (
            "unknown-key",
            CandidateIr::Json(unknown_key.to_string()),
            ErrorCode::Schema.as_str(),
        ),
        (
            "bad-version",
            CandidateIr::Json(bad_version.to_string()),
            ErrorCode::UnknownVersion.as_str(),
        ),
        (
            "missing-field",
            CandidateIr::Json(missing_field.to_string()),
            ErrorCode::MissingField.as_str(),
        ),
        (
            "not-object",
            CandidateIr::Json("[]".to_string()),
            ErrorCode::Schema.as_str(),
        ),
    ];

    let candidates: Vec<Candidate> = cases
        .iter()
        .map(|(id, ir, _)| Candidate {
            id: (*id).to_string(),
            ir: ir.clone(),
        })
        .collect();
    let batch = guards.screen_before_best_of_n("bo-bad", &candidates);
    assert!(batch.admitted().is_empty(), "不合格候选不得进评测");
    assert_eq!(batch.rejected().len(), cases.len());
    for (rejected, (id, _, code)) in batch.rejected().iter().zip(cases.iter()) {
        assert_eq!(rejected.id(), *id);
        assert!(
            rejected.has_code(code),
            "{id} 应含 {code}，实际 {:?}",
            rejected.reasons()
        );
        assert!(rejected.ledger_seq() >= 1);
    }

    let version = batch
        .rejected()
        .iter()
        .find(|item| item.id() == "bad-version")
        .expect("version");
    assert_eq!(version.program_id(), Some("qa-cot"));
    let not_object = batch
        .rejected()
        .iter()
        .find(|item| item.id() == "not-object")
        .expect("array");
    assert_eq!(not_object.program_id(), None);

    assert!(guards.ledger().entries().iter().all(|entry| {
        entry.at_ms == 10 && matches!(entry.event, GuardEvent::ScreenReject { .. })
    }));
}

#[test]
fn demo_values_are_not_rechecked_against_type_name() {
    let (_clock, mut guards) = guards(1);
    let mut program = valid_program();
    program.signatures[0].inputs[0].type_name = "list[str]".to_string();
    program.demos[0]
        .values
        .insert("question".to_string(), json!(2));
    program.validate().expect("w4b 不按 type_name 复核 demo 值");
    let batch = guards.screen_before_best_of_n("bo-types", &[candidate("c1", program)]);
    assert_eq!(batch.admitted().len(), 1);
    assert!(batch.rejected().is_empty());
}

#[test]
fn mixed_batch_keeps_admit_order_and_ledger_joins_one_to_one() {
    let (clock, mut guards) = guards(100);
    let mut broken = valid_program();
    broken.demos[0].values.remove("answer");
    let mut unknown_field = valid_program();
    unknown_field.demos[0]
        .values
        .insert("extra".to_string(), json!("nope"));

    let first = valid_program();
    let third = valid_program();
    let batch = guards.screen_before_best_of_n(
        "bo-mix",
        &[
            candidate("keep-1", first.clone()),
            candidate("drop-incomplete", broken),
            candidate("keep-2", third.clone()),
            candidate("drop-unknown-field", unknown_field),
        ],
    );
    assert_eq!(
        batch
            .admitted()
            .iter()
            .map(af_dspy_ir::AdmittedCandidate::id)
            .collect::<Vec<_>>(),
        vec!["keep-1", "keep-2"]
    );
    assert_eq!(batch.admitted()[0].program(), &first);
    assert_eq!(batch.admitted()[1].program(), &third);
    assert_eq!(
        batch
            .rejected()
            .iter()
            .map(|item| item.id())
            .collect::<Vec<_>>(),
        vec!["drop-incomplete", "drop-unknown-field"]
    );
    assert!(batch.rejected()[0].has_code(ErrorCode::IncompleteDemo.as_str()));
    assert!(batch.rejected()[1].has_code(ErrorCode::UnknownField.as_str()));

    let entries = guards.ledger().entries();
    assert_eq!(entries.len(), 4);
    let ids = ["keep-1", "drop-incomplete", "keep-2", "drop-unknown-field"];
    for (index, entry) in entries.iter().enumerate() {
        assert_eq!(entry.seq, u64::try_from(index).unwrap() + 1);
        assert_eq!(entry.at_ms, 100);
        match &entry.event {
            GuardEvent::ScreenAdmit {
                batch_id,
                candidate_id,
                program_id,
            } => {
                assert_eq!(batch_id, "bo-mix");
                assert_eq!(candidate_id, ids[index]);
                assert_eq!(program_id, "qa-cot");
            }
            GuardEvent::ScreenReject {
                batch_id,
                candidate_id,
                reasons,
                ..
            } => {
                assert_eq!(batch_id, "bo-mix");
                assert_eq!(candidate_id, ids[index]);
                assert!(!reasons.is_empty());
            }
            other => panic!("预筛不应写出 {other:?}"),
        }
    }
    assert_eq!(batch.admitted()[0].ledger_seq(), 1);
    assert_eq!(batch.rejected()[0].ledger_seq(), 2);
    assert_eq!(batch.admitted()[1].ledger_seq(), 3);

    clock.set(250);
    let again = guards.screen_before_best_of_n("bo-next", &[candidate("keep-3", valid_program())]);
    assert_eq!(again.admitted()[0].ledger_seq(), 5);
    assert_eq!(guards.ledger().entries()[4].at_ms, 250);
    assert_eq!(guards.ledger().entries()[0].at_ms, 100);
}

#[test]
fn blank_batch_blank_id_and_duplicates_are_rejected() {
    let (_clock, mut guards) = guards(7);
    let program = valid_program();
    let batch = guards.screen_before_best_of_n(
        "  ",
        &[
            candidate("c1", program.clone()),
            candidate("c1", program.clone()),
            candidate(" ", program.clone()),
            candidate(" ", program),
        ],
    );
    assert!(batch.admitted().is_empty());
    assert_eq!(batch.rejected().len(), 4);
    assert!(batch.rejected()[0].has_code(JG_BLANK_BATCH));
    assert!(!batch.rejected()[0].has_code(JG_DUPLICATE_CANDIDATE));
    assert_eq!(batch.rejected()[0].program_id(), Some("qa-cot"));
    assert!(batch.rejected()[1].has_code(JG_BLANK_BATCH));
    assert!(batch.rejected()[1].has_code(JG_DUPLICATE_CANDIDATE));
    assert!(batch.rejected()[2].has_code(JG_BLANK_CANDIDATE));
    assert!(!batch.rejected()[2].has_code(JG_DUPLICATE_CANDIDATE));
    assert!(batch.rejected()[3].has_code(JG_BLANK_CANDIDATE));
    assert!(!batch.rejected()[3].has_code(JG_DUPLICATE_CANDIDATE));

    let mut follow = Guardrails::new(ManualClock::new(8));
    let mut broken = valid_program();
    broken.program_id.clear();
    broken.predictors[0].demos = vec!["missing-demo".to_string()];
    let batch = follow.screen_before_best_of_n(
        "bo-ids",
        &[
            candidate("same", broken),
            candidate("same", valid_program()),
            candidate("", valid_program()),
        ],
    );
    assert!(batch.admitted().is_empty());
    let codes = |index: usize| {
        batch.rejected()[index]
            .reasons()
            .iter()
            .map(|reason| reason.code.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        codes(0),
        vec![
            ErrorCode::EmptyIdentifier.as_str(),
            ErrorCode::UnknownDemo.as_str(),
        ]
    );
    assert_eq!(codes(1), vec![JG_DUPLICATE_CANDIDATE]);
    assert_eq!(codes(2), vec![JG_BLANK_CANDIDATE]);
    assert_eq!(batch.rejected()[0].program_id(), Some(""));
}

#[test]
fn admitted_program_is_a_snapshot() {
    let (_clock, mut guards) = guards(3);
    let mut program = valid_program();
    let batch = guards.screen_before_best_of_n("bo-snap", &[candidate("c1", program.clone())]);
    program.program_id = "changed".to_string();
    assert_eq!(batch.admitted()[0].program().program_id, "qa-cot");
    let owned = batch.admitted()[0].clone().into_program();
    assert_eq!(owned.program_id, "qa-cot");
    parse_program(&to_json(&owned).expect("json")).expect("snapshot still valid");
}

#[test]
fn restored_ledger_does_not_mint_admissions() {
    let (clock, mut guards) = guards(11);
    let batch = guards.screen_before_best_of_n("bo-1", &[candidate("c1", valid_program())]);
    assert_eq!(batch.admitted().len(), 1);
    let saved = guards.ledger().to_json().expect("export");
    let restored = Ledger::from_json(&saved).expect("import");
    clock.set(12);
    let mut guards = Guardrails::restore(clock, restored);
    assert_eq!(guards.ledger().len(), 1);
    assert!(guards.ledger().entries()[0].seq == 1);

    let mut broken = valid_program();
    broken.signatures[0].outputs[0].name.clear();
    let batch = guards.screen_before_best_of_n("bo-2", &[candidate("c2", broken)]);
    assert!(batch.admitted().is_empty());
    assert_eq!(batch.rejected()[0].ledger_seq(), 2);
    assert!(batch.rejected()[0].has_code(ErrorCode::MissingFieldName.as_str()));
    assert_eq!(guards.ledger().entries()[1].at_ms, 12);
    assert_eq!(guards.ledger().len(), 2);
}

fn run(id: &str, failure: Option<FailureClass>, scored: bool) -> RunResult {
    RunResult {
        run_id: id.to_string(),
        failure,
        scored,
    }
}

#[test]
fn failure_codes_parse_only_on_exact_match() {
    assert_eq!(FailureClass::parse(E_INFRA), FailureClass::Infra);
    assert_eq!(FailureClass::parse(E_SUBJECT), FailureClass::Subject);
    assert_eq!(FailureClass::parse(E_SCORE), FailureClass::LowScore);
    assert_eq!(FailureClass::parse(E_LOW_SCORE), FailureClass::LowScore);
    assert_eq!(FailureClass::Infra.as_code(), E_INFRA);
    assert_eq!(FailureClass::Subject.as_code(), E_SUBJECT);
    assert_eq!(FailureClass::LowScore.as_code(), E_SCORE);
    assert_eq!(
        FailureClass::parse("E_INFRA_EXTRA"),
        FailureClass::Other("E_INFRA_EXTRA".to_string())
    );
    assert_eq!(
        FailureClass::parse("e_infra"),
        FailureClass::Other("e_infra".to_string())
    );
    assert_eq!(
        FailureClass::parse("E_SUBJECT\n"),
        FailureClass::Other("E_SUBJECT\n".to_string())
    );
}

#[test]
fn repair_allows_only_unscored_infra_and_subject() {
    let (clock, mut guards) = guards(42);
    let cases = [
        ("run-infra", Some(FailureClass::Infra), CrashCode::Infra),
        (
            "run-subject",
            Some(FailureClass::Subject),
            CrashCode::Subject,
        ),
        (
            "run-infra-text",
            Some(FailureClass::Other(E_INFRA.to_string())),
            CrashCode::Infra,
        ),
        (
            "run-subject-text",
            Some(FailureClass::Other(E_SUBJECT.to_string())),
            CrashCode::Subject,
        ),
    ];
    for (id, failure, code) in cases {
        let verdict = guards.judge_repair(&run(id, failure, false));
        let permit = verdict.permit().expect(id);
        assert!(verdict.denial().is_none());
        assert_eq!(permit.run_id(), id);
        assert_eq!(permit.code(), code);
        assert_eq!(permit.code().as_str(), code.as_str());
        assert_eq!(permit.code().to_string(), code.as_str());
    }
    assert!(guards
        .ledger()
        .entries()
        .iter()
        .all(|entry| entry.at_ms == 42));

    clock.set(-8);
    let later = guards.judge_repair(&run("run-later", Some(FailureClass::Infra), false));
    assert_eq!(later.permit().expect("later").code(), CrashCode::Infra);
    assert_eq!(guards.ledger().entries()[4].at_ms, -8);
    assert_eq!(guards.ledger().entries()[0].at_ms, 42);
    match &guards.ledger().entries()[4].event {
        GuardEvent::RepairAllow {
            run_id,
            code,
            scored,
        } => {
            assert_eq!(run_id, "run-later");
            assert_eq!(code, E_INFRA);
            assert!(!*scored);
        }
        other => panic!("应为放行 {other:?}"),
    }
    let loaded = Ledger::from_json(&guards.ledger().to_json().expect("json")).expect("reload");
    assert_eq!(loaded.entries(), guards.ledger().entries());
}

#[test]
fn low_score_is_always_denied() {
    let (_clock, mut guards) = guards(5);
    let cases = [
        ("low", Some(FailureClass::LowScore), true, Some(E_SCORE)),
        (
            "low-unscored",
            Some(FailureClass::LowScore),
            false,
            Some(E_SCORE),
        ),
        (
            "score-code",
            Some(FailureClass::Other(E_SCORE.to_string())),
            false,
            Some(E_SCORE),
        ),
        (
            "low-score-code",
            Some(FailureClass::parse(E_LOW_SCORE)),
            true,
            Some(E_SCORE),
        ),
        (
            "low-score-text",
            Some(FailureClass::Other(E_LOW_SCORE.to_string())),
            true,
            Some(E_LOW_SCORE),
        ),
    ];
    for (index, (id, failure, scored, ledger_code)) in cases.into_iter().enumerate() {
        let verdict = guards.judge_repair(&run(id, failure, scored));
        assert!(verdict.permit().is_none(), "{id}");
        let denial = verdict.denial().expect(id);
        assert_eq!(denial.reason_code(), JG_LOW_SCORE);
        assert_eq!(denial.message(), "低分类一律拒绝修复");
        assert_eq!(denial.ledger_seq(), u64::try_from(index).unwrap() + 1);
        match &guards.ledger().entries()[index].event {
            GuardEvent::RepairDeny {
                failure_code,
                scored: recorded,
                reason_code,
                ..
            } => {
                assert_eq!(failure_code.as_deref(), ledger_code);
                assert_eq!(*recorded, scored);
                assert_eq!(reason_code, JG_LOW_SCORE);
            }
            other => panic!("{id} 应拒绝 {other:?}"),
        }
    }
}

#[test]
fn non_crash_and_scored_crash_are_denied() {
    let (clock, mut guards) = guards(9);
    // 先记一条预筛，确认修复判定只追加、不改写已有行。
    let screened = guards.screen_before_best_of_n("bo", &[candidate("c1", valid_program())]);
    assert_eq!(screened.admitted()[0].ledger_seq(), 1);

    struct Case {
        id: &'static str,
        failure: Option<FailureClass>,
        scored: bool,
        reason: &'static str,
        message: &'static str,
        failure_code: Option<&'static str>,
    }
    let cases = [
        Case {
            id: "ok",
            failure: None,
            scored: false,
            reason: JG_NOT_A_CRASH,
            message: "没有崩溃码，拒绝修复",
            failure_code: None,
        },
        Case {
            id: "scored-ok",
            failure: None,
            scored: true,
            reason: JG_NOT_A_CRASH,
            message: "没有崩溃码，拒绝修复",
            failure_code: None,
        },
        Case {
            id: "timeout",
            failure: Some(FailureClass::Other("E_TIMEOUT".to_string())),
            scored: false,
            reason: JG_NOT_A_CRASH,
            message: "失败码 E_TIMEOUT 不是崩溃类，拒绝修复",
            failure_code: Some("E_TIMEOUT"),
        },
        Case {
            id: "lower",
            failure: Some(FailureClass::Other("e_infra".to_string())),
            scored: true,
            reason: JG_NOT_A_CRASH,
            message: "失败码 e_infra 不是崩溃类，拒绝修复",
            failure_code: Some("e_infra"),
        },
        Case {
            id: "padded",
            failure: Some(FailureClass::Other("E_INFRA ".to_string())),
            scored: false,
            reason: JG_NOT_A_CRASH,
            message: "失败码 E_INFRA  不是崩溃类，拒绝修复",
            failure_code: Some("E_INFRA "),
        },
        Case {
            id: "infra-scored",
            failure: Some(FailureClass::Infra),
            scored: true,
            reason: JG_SCORE_ON_CRASH,
            message: "崩溃码 E_INFRA 附带了评测分数，拒绝修复",
            failure_code: Some(E_INFRA),
        },
        Case {
            id: "subject-scored",
            failure: Some(FailureClass::Subject),
            scored: true,
            reason: JG_SCORE_ON_CRASH,
            message: "崩溃码 E_SUBJECT 附带了评测分数，拒绝修复",
            failure_code: Some(E_SUBJECT),
        },
        Case {
            id: "subject-text-scored",
            failure: Some(FailureClass::Other(E_SUBJECT.to_string())),
            scored: true,
            reason: JG_SCORE_ON_CRASH,
            message: "崩溃码 E_SUBJECT 附带了评测分数，拒绝修复",
            failure_code: Some(E_SUBJECT),
        },
        Case {
            id: " ",
            failure: Some(FailureClass::Infra),
            scored: false,
            reason: JG_BLANK_RUN,
            message: "run_id 为空，拒绝修复",
            failure_code: Some(E_INFRA),
        },
        Case {
            id: "",
            failure: Some(FailureClass::LowScore),
            scored: true,
            reason: JG_BLANK_RUN,
            message: "run_id 为空，拒绝修复",
            failure_code: Some(E_SCORE),
        },
        Case {
            id: "blank-code",
            failure: Some(FailureClass::Other(String::new())),
            scored: false,
            reason: JG_NOT_A_CRASH,
            message: "失败码为空，不是崩溃类，拒绝修复",
            failure_code: None,
        },
    ];

    for case in &cases {
        let verdict = guards.judge_repair(&run(case.id, case.failure.clone(), case.scored));
        assert!(verdict.permit().is_none(), "{}", case.id);
        let denial = verdict.denial().expect(case.id);
        assert_eq!(denial.reason_code(), case.reason, "{}", case.id);
        assert_eq!(denial.message(), case.message, "{}", case.id);
    }

    let entries = guards.ledger().entries();
    assert_eq!(entries[0].at_ms, 9);
    match &entries[0].event {
        GuardEvent::ScreenAdmit { candidate_id, .. } => assert_eq!(candidate_id, "c1"),
        other => panic!("预筛行被改写 {other:?}"),
    }
    for (offset, case) in cases.iter().enumerate() {
        let entry = &entries[offset + 1];
        assert_eq!(entry.at_ms, 9);
        assert_eq!(entry.seq, u64::try_from(offset + 2).unwrap());
        match &entry.event {
            GuardEvent::RepairDeny {
                run_id,
                failure_code,
                scored,
                reason_code,
                message,
            } => {
                assert_eq!(run_id, case.id);
                assert_eq!(failure_code.as_deref(), case.failure_code);
                assert_eq!(*scored, case.scored);
                assert_eq!(reason_code, case.reason);
                assert_eq!(message, case.message);
            }
            other => panic!("{} 应是拒绝 {other:?}", case.id),
        }
    }

    clock.advance(4);
    let allowed = guards.judge_repair(&run("after", Some(FailureClass::Subject), false));
    assert_eq!(allowed.permit().expect("subject").ledger_seq(), 13);
    assert_eq!(guards.ledger().entries()[0].at_ms, 9);
    assert_eq!(guards.ledger().entries()[12].at_ms, 13);
    let loaded = Ledger::from_json(&guards.ledger().to_json().expect("json")).expect("reload");
    assert_eq!(loaded.entries(), guards.ledger().entries());
}

#[test]
fn system_clock_reads_unix_milliseconds() {
    let now = SystemClock::new().now_ms();
    assert!(now > 1_700_000_000_000, "{now}");
}
