//! 评测产线验收：三态、冻结后拒绝改动、污染停机、四失败码、只追加账本。
//! 时刻全部来自 ManualClock，不睡眠。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use af_eval::{
    Catalog, EvalError, ExecSignals, FailureCode, ManualClock, OpenStatus, Phase, RunSpec,
    RunVerdict,
};

fn open(now_ms: i64) -> (Catalog, ManualClock) {
    let clock = ManualClock::new(now_ms);
    let (catalog, status) = Catalog::open_in_memory_with_clock("eval", clock.clone()).unwrap();
    assert_eq!(status, OpenStatus::Created);
    (catalog, clock)
}

fn spec<'a>(
    run_id: &'a str,
    version: u64,
    subject_version: &'a str,
    input: &'a [String],
    budget_ms: i64,
) -> RunSpec<'a> {
    RunSpec {
        run_id,
        lineage_id: "math",
        version,
        subject_id: "agent",
        subject_version,
        input,
        budget_ms,
    }
}

#[test]
fn draft_freezes_then_retires() {
    let (mut catalog, clock) = open(42);
    let created = catalog.create_benchmark("math").unwrap();
    assert_eq!(created.phase, Phase::Draft);
    assert_eq!(created.version, 1);
    assert_eq!(created.created_at_ms, 42);
    assert!(created.snapshot_hash.is_none());

    let err = catalog.retire("math", 1).unwrap_err();
    assert!(matches!(
        err,
        EvalError::IllegalTransition {
            phase: Phase::Draft,
            action: "退役",
        }
    ));
    let err = catalog.freeze("math", 1).unwrap_err();
    assert!(matches!(err, EvalError::EmptyBenchmark));
    assert_eq!(catalog.benchmark("math", 1).unwrap().phase, Phase::Draft);

    catalog.put_case("math", 1, "c2", "beta").unwrap();
    catalog.put_case("math", 1, "c1", "alpha").unwrap();
    catalog.put_case("math", 1, "c1", "alpha-2").unwrap();
    clock.advance(8);
    let frozen = catalog.freeze("math", 1).unwrap();
    assert_eq!(frozen.phase, Phase::Frozen);
    assert_eq!(frozen.frozen_at_ms, Some(50));
    assert_eq!(frozen.created_at_ms, 42);
    let hash = frozen.snapshot_hash.clone().unwrap();
    assert_eq!(hash.len(), 64);
    assert_eq!(
        frozen
            .cases
            .iter()
            .map(|case| (case.case_id.as_str(), case.payload.as_str()))
            .collect::<Vec<_>>(),
        vec![("c1", "alpha-2"), ("c2", "beta")]
    );

    let err = catalog.freeze("math", 1).unwrap_err();
    assert!(matches!(
        err,
        EvalError::IllegalTransition {
            phase: Phase::Frozen,
            action: "冻结",
        }
    ));

    clock.set(60);
    let retired = catalog.retire("math", 1).unwrap();
    assert_eq!(retired.phase, Phase::Retired);
    assert_eq!(retired.retired_at_ms, Some(60));
    assert_eq!(retired.snapshot_hash.as_deref(), Some(hash.as_str()));
    assert_eq!(retired.frozen_at_ms, Some(50));

    let err = catalog.retire("math", 1).unwrap_err();
    assert!(matches!(
        err,
        EvalError::IllegalTransition {
            phase: Phase::Retired,
            action: "退役",
        }
    ));
    let err = catalog
        .begin_run(&spec("late", 1, "rev-1", &[], 10))
        .unwrap_err();
    assert!(matches!(
        err,
        EvalError::NotFrozen {
            phase: Phase::Retired,
            ..
        }
    ));

    let forked = catalog.fork("math", 1).unwrap();
    assert_eq!(forked.version, 2);
    assert_eq!(forked.phase, Phase::Draft);
    assert_eq!(forked.cases, retired.cases);
    assert_eq!(forked.parent.unwrap().snapshot_hash, hash);
    let original = catalog.benchmark("math", 1).unwrap();
    assert_eq!(original.phase, Phase::Retired);
    assert_eq!(original.snapshot_hash.as_deref(), Some(hash.as_str()));
}

#[test]
fn frozen_content_rejects_edits_and_forks_a_new_version() {
    let (mut catalog, _) = open(1);
    catalog.create_benchmark("math").unwrap();
    catalog.put_case("math", 1, "c1", "alpha").unwrap();
    catalog.put_case("math", 1, "c2", "beta").unwrap();
    let frozen = catalog.freeze("math", 1).unwrap();
    let hash = frozen.snapshot_hash.clone().unwrap();

    let err = catalog.put_case("math", 1, "c1", "gamma").unwrap_err();
    assert!(matches!(
        err,
        EvalError::Sealed {
            phase: Phase::Frozen,
            ..
        }
    ));
    let err = catalog.remove_case("math", 1, "c2").unwrap_err();
    assert!(matches!(err, EvalError::Sealed { .. }));
    let err = catalog.put_case("math", 1, "c1", "alpha").unwrap_err();
    assert!(matches!(err, EvalError::Sealed { .. }));
    let still = catalog.benchmark("math", 1).unwrap();
    assert_eq!(still.snapshot_hash.as_deref(), Some(hash.as_str()));
    assert_eq!(still.cases.len(), 2);

    let forked = catalog.fork("math", 1).unwrap();
    assert_eq!(forked.version, 2);
    assert_eq!(forked.phase, Phase::Draft);
    assert!(forked.snapshot_hash.is_none());
    assert_eq!(forked.parent.as_ref().unwrap().version, 1);
    assert_eq!(forked.parent.as_ref().unwrap().snapshot_hash, hash);
    assert_eq!(forked.cases, still.cases);

    catalog.put_case("math", 2, "c1", "gamma").unwrap();
    let v1 = catalog.benchmark("math", 1).unwrap();
    assert_eq!(v1.phase, Phase::Frozen);
    assert_eq!(v1.cases[0].payload, "alpha");
    assert_eq!(v1.snapshot_hash.as_deref(), Some(hash.as_str()));

    let v2 = catalog.freeze("math", 2).unwrap();
    assert_eq!(v2.phase, Phase::Frozen);
    assert_ne!(v2.snapshot_hash.as_deref(), Some(hash.as_str()));

    let draft = {
        catalog.fork("math", 2).unwrap();
        catalog.benchmark("math", 3).unwrap()
    };
    assert_eq!(draft.phase, Phase::Draft);
    let err = catalog.fork("math", 3).unwrap_err();
    assert!(matches!(
        err,
        EvalError::IllegalTransition {
            phase: Phase::Draft,
            action: "fork",
        }
    ));
    let err = catalog
        .begin_run(&spec("draft-run", 3, "rev-9", &[], 10))
        .unwrap_err();
    assert!(matches!(
        err,
        EvalError::NotFrozen {
            phase: Phase::Draft,
            ..
        }
    ));

    // 退役看阶段，不要求这一版仍是谱系头。
    let retired = catalog.retire("math", 1).unwrap();
    assert_eq!(retired.phase, Phase::Retired);
    let err = catalog.put_case("math", 1, "c3", "nope").unwrap_err();
    assert!(matches!(
        err,
        EvalError::Sealed {
            phase: Phase::Retired,
            ..
        }
    ));
}

#[test]
fn contamination_hit_halts_and_miss_can_finish() {
    let (mut catalog, clock) = open(100);
    catalog.create_benchmark("math").unwrap();
    catalog.put_case("math", 1, "c1", "gold-1").unwrap();
    catalog.put_case("math", 1, "c2", "gold-2").unwrap();
    let frozen = catalog.freeze("math", 1).unwrap();
    let hash = frozen.snapshot_hash.clone().unwrap();

    let hit = vec!["notes".into(), "gold-2".into(), "gold-2".into()];
    let err = catalog
        .begin_run(&spec("hit", 1, "rev-4", &hit, 25))
        .unwrap_err();
    match err {
        EvalError::Contaminated { run_id, overlap } => {
            assert_eq!(run_id, "hit");
            assert_eq!(overlap, vec!["gold-2".to_owned()]);
        }
        other => panic!("expected contamination, got {other}"),
    }
    let halted = catalog.run_record("hit").unwrap();
    assert_eq!(halted.snapshot_hash, hash);
    assert_eq!(halted.subject_version, "rev-4");
    assert_eq!(halted.started_at_ms, 100);
    assert_eq!(halted.recorded_at_ms, Some(100));
    assert_eq!(
        halted.verdict,
        RunVerdict::Halted {
            overlap: vec!["gold-2".to_owned()]
        }
    );
    let err = catalog
        .finish_run("hit", ExecSignals::default())
        .unwrap_err();
    assert!(matches!(err, EvalError::RunClosed { .. }));

    let both = vec!["gold-2".into(), "gold-1".into()];
    let err = catalog
        .begin_run(&spec("both", 1, "rev-4", &both, 25))
        .unwrap_err();
    match err {
        EvalError::Contaminated { overlap, .. } => {
            assert_eq!(overlap, vec!["gold-1".to_owned(), "gold-2".to_owned()]);
        }
        other => panic!("expected contamination, got {other}"),
    }

    let near = vec!["gold".into(), "gold-1-extra".into()];
    let view = catalog
        .begin_run(&spec("near", 1, "rev-4", &near, 25))
        .unwrap();
    assert_eq!(view.snapshot.hash, hash);
    assert_eq!(view.snapshot.cases.len(), 2);
    clock.set(120);
    let finished = catalog.finish_run("near", ExecSignals::default()).unwrap();
    assert_eq!(finished.verdict, RunVerdict::Passed);
    assert_eq!(finished.snapshot_hash, hash);
    assert_eq!(finished.subject_version, "rev-4");
}

#[test]
fn failure_codes_stay_distinct_and_persist() {
    let (mut catalog, clock) = open(1_000);
    catalog.create_benchmark("math").unwrap();
    catalog.put_case("math", 1, "c1", "alpha").unwrap();
    catalog.freeze("math", 1).unwrap();

    let cases = [
        (
            "infra",
            ExecSignals {
                infra: true,
                bench: true,
                subject: true,
            },
            1_010,
            RunVerdict::Failed(FailureCode::Infra),
        ),
        (
            "bench",
            ExecSignals {
                infra: false,
                bench: true,
                subject: true,
            },
            1_010,
            RunVerdict::Failed(FailureCode::Bench),
        ),
        (
            "subject",
            ExecSignals {
                infra: false,
                bench: false,
                subject: true,
            },
            1_010,
            RunVerdict::Failed(FailureCode::Subject),
        ),
        (
            "timeout",
            ExecSignals {
                infra: true,
                bench: true,
                subject: true,
            },
            1_050,
            RunVerdict::Failed(FailureCode::Timeout),
        ),
        ("pass", ExecSignals::default(), 1_049, RunVerdict::Passed),
    ];

    for (run_id, signals, now, want) in cases {
        clock.set(1_000);
        catalog
            .begin_run(&spec(run_id, 1, "rev-7", &[], 50))
            .unwrap();
        clock.set(now);
        let record = catalog.finish_run(run_id, signals).unwrap();
        assert_eq!(record.verdict, want, "{run_id}");
        assert_eq!(record.subject_version, "rev-7");
        assert_eq!(record.started_at_ms, 1_000);
        assert_eq!(record.recorded_at_ms, Some(now));
        assert_eq!(record.budget_ms, 50);
        assert_eq!(
            record.snapshot_hash,
            catalog.benchmark("math", 1).unwrap().snapshot_hash.unwrap()
        );
    }

    clock.set(1_000);
    catalog
        .begin_run(&spec("back", 1, "rev-7", &[], 50))
        .unwrap();
    clock.set(999);
    let err = catalog
        .finish_run("back", ExecSignals::default())
        .unwrap_err();
    assert!(matches!(
        err,
        EvalError::ClockRegressed {
            started_at_ms: 1_000,
            now_ms: 999
        }
    ));
    assert_eq!(
        catalog.run_record("back").unwrap().verdict,
        RunVerdict::InProgress
    );
    clock.set(1_020);
    let record = catalog
        .finish_run(
            "back",
            ExecSignals {
                bench: true,
                ..ExecSignals::default()
            },
        )
        .unwrap();
    assert_eq!(record.verdict, RunVerdict::Failed(FailureCode::Bench));
}

#[test]
fn append_only_ledger_rejects_update_and_delete() {
    let (mut catalog, clock) = open(10);
    catalog.create_benchmark("math").unwrap();
    catalog.put_case("math", 1, "c1", "alpha").unwrap();
    catalog.freeze("math", 1).unwrap();
    catalog
        .begin_run(&spec("run-1", 1, "rev-1", &[], 40))
        .unwrap();
    clock.set(20);
    let before = catalog
        .finish_run(
            "run-1",
            ExecSignals {
                subject: true,
                ..ExecSignals::default()
            },
        )
        .unwrap();
    assert_eq!(before.verdict, RunVerdict::Failed(FailureCode::Subject));

    let err = catalog
        .begin_run(&spec("run-1", 1, "rev-9", &[], 40))
        .unwrap_err();
    assert!(matches!(err, EvalError::RunExists { .. }), "{err}");
    let err = catalog
        .begin_run(&spec("zero", 1, "rev-1", &[], 0))
        .unwrap_err();
    assert!(matches!(err, EvalError::BadBudget), "{err}");

    let err = catalog
        .update_run_subject_version("run-1", "hacked")
        .unwrap_err();
    assert!(matches!(err, EvalError::AppendOnly), "{err}");
    let err = catalog.update_run_outcome("run-1").unwrap_err();
    assert!(matches!(err, EvalError::AppendOnly), "{err}");
    let err = catalog.delete_run("run-1").unwrap_err();
    assert!(matches!(err, EvalError::AppendOnly), "{err}");

    let after = catalog.run_record("run-1").unwrap();
    assert_eq!(after, before);
    assert_eq!(after.subject_version, "rev-1");
    assert_eq!(after.verdict, RunVerdict::Failed(FailureCode::Subject));
}

#[test]
fn blind_run_reads_frozen_snapshot_not_current_recipe() {
    let (mut catalog, _) = open(7);
    catalog
        .set_current_recipe("optim", "SECRET-RECIPE-BODY")
        .unwrap();
    catalog.create_benchmark("math").unwrap();
    catalog.put_case("math", 1, "c1", "visible-case").unwrap();
    let frozen = catalog.freeze("math", 1).unwrap();
    catalog.fork("math", 1).unwrap();
    catalog
        .put_case("math", 2, "c1", "SECRET-RECIPE-BODY")
        .unwrap();

    let view = catalog
        .begin_run(&spec("blind", 1, "rev-2", &[], 15))
        .unwrap();
    assert_eq!(view.snapshot.hash, frozen.snapshot_hash.unwrap());
    assert_eq!(view.snapshot.cases[0].payload, "visible-case");
    assert_eq!(view.subject_version, "rev-2");
    let err = view.current_recipe().unwrap_err();
    assert!(matches!(err, EvalError::InformationBarrier { .. }));
    let err = catalog.recipe_during_run("blind").unwrap_err();
    assert!(matches!(
        err,
        EvalError::InformationBarrier { ref run_id } if run_id == "blind"
    ));
    let rendered = format!("{view:?}");
    assert!(!rendered.contains("SECRET-RECIPE-BODY"));

    catalog
        .set_current_recipe("optim", "SECRET-RECIPE-NEXT")
        .unwrap();
    let err = catalog.recipe_during_run("blind").unwrap_err();
    assert!(matches!(err, EvalError::InformationBarrier { .. }));
    assert_eq!(catalog.current_recipe().unwrap().body, "SECRET-RECIPE-NEXT");
    let again = catalog.run_view("blind").unwrap();
    assert_eq!(again.snapshot.cases[0].payload, "visible-case");
    assert!(!format!("{again:?}").contains("SECRET-RECIPE-NEXT"));
    assert_eq!(
        catalog
            .benchmark("math", 1)
            .unwrap()
            .snapshot_hash
            .as_deref(),
        Some(again.snapshot.hash.as_str())
    );
}

#[test]
fn reopen_replays_hash_subject_version_and_halt() {
    let tmp = TempDb::new();
    let clock = ManualClock::new(80);
    let hash = {
        let (mut catalog, status) =
            Catalog::open_with_clock(tmp.path(), "eval", clock.clone()).unwrap();
        assert_eq!(status, OpenStatus::Created);
        catalog.create_benchmark("math").unwrap();
        catalog.put_case("math", 1, "c1", "gold-1").unwrap();
        let frozen = catalog.freeze("math", 1).unwrap();
        let hash = frozen.snapshot_hash.clone().unwrap();
        let err = catalog
            .begin_run(&spec("hit", 1, "rev-8", &["gold-1".into()], 10))
            .unwrap_err();
        assert!(matches!(err, EvalError::Contaminated { .. }));
        clock.set(85);
        catalog
            .begin_run(&spec("ok", 1, "rev-8", &["notes".into()], 10))
            .unwrap();
        clock.set(90);
        catalog.finish_run("ok", ExecSignals::default()).unwrap();
        hash
    };
    clock.set(1_000);
    let (catalog, status) = Catalog::open_with_clock(tmp.path(), "eval", clock).unwrap();
    assert_eq!(status, OpenStatus::AlreadyDeclared);
    let bench = catalog.benchmark("math", 1).unwrap();
    assert_eq!(bench.phase, Phase::Frozen);
    assert_eq!(bench.snapshot_hash.as_deref(), Some(hash.as_str()));
    assert_eq!(bench.frozen_at_ms, Some(80));
    let halted = catalog.run_record("hit").unwrap();
    assert_eq!(halted.snapshot_hash, hash);
    assert_eq!(halted.subject_version, "rev-8");
    assert!(matches!(halted.verdict, RunVerdict::Halted { .. }));
    let passed = catalog.run_record("ok").unwrap();
    assert_eq!(passed.snapshot_hash, hash);
    assert_eq!(passed.subject_version, "rev-8");
    assert_eq!(passed.verdict, RunVerdict::Passed);
    assert_eq!(passed.recorded_at_ms, Some(90));
}

struct TempDb {
    path: PathBuf,
}

impl TempDb {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("af-eval-pipe-{}-{n}.db", std::process::id()));
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let path = if suffix.is_empty() {
                self.path.clone()
            } else {
                PathBuf::from(format!("{}{suffix}", self.path.display()))
            };
            let _ = std::fs::remove_file(path);
        }
    }
}
