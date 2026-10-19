//! 语义层全链路。后端是本 crate 的 stub，不连接 CubeSandbox。

use af_credential::{attenuate, inspect, issue_root, ChildSpec, RootSpec};
use af_sandbox::{
    decide_concurrency, BackendExec, BudgetDecision, BudgetDimension, BudgetRefusal, Command,
    ConcurrencyBudget, ConcurrencyCharge, ConcurrencyRequest, CredentialScope, Image, ManualClock,
    Resources, SandboxError, SandboxHost, Session, StubHandle, SANDBOX_EXEC_TOOL,
};

const BOOT: u64 = 7;

fn world() -> (SandboxHost, StubHandle, ManualClock) {
    let stub = StubHandle::new();
    let clock = ManualClock::new(1_000);
    let host = SandboxHost::boot(stub.clone(), clock.clone(), BOOT);
    (host, stub, clock)
}

fn limits() -> Resources {
    Resources::new(250, 32 * 1024 * 1024, 5_000).unwrap()
}

fn image() -> Image {
    Image::new("example.invalid/tool:1").unwrap()
}

fn command() -> Command {
    Command::new(["tool", "run"]).unwrap()
}

fn issue(tools: &[&str], budget_cents: i64) -> biscuit_auth::Biscuit {
    issue_root(&RootSpec {
        tools: tools.iter().map(|tool| (*tool).to_string()).collect(),
        namespace: "line-a".into(),
        budget_cents,
    })
    .unwrap()
    .token
}

fn scope(token: &biscuit_auth::Biscuit, spend_cents: i64) -> CredentialScope<'_> {
    CredentialScope::new(token, "line-a", spend_cents)
}

fn charged(slots: u64) -> BudgetDecision {
    BudgetDecision::Charged(ConcurrencyCharge {
        dimension: BudgetDimension::SandboxConcurrency,
        slots,
        decided_at: 10,
        release_at: 80,
    })
}

fn refusal() -> BudgetDecision {
    BudgetDecision::Refused(BudgetRefusal {
        dimension: BudgetDimension::SandboxConcurrency,
        need: 1,
        available: 0,
        recovers_at: Some(40),
    })
}

fn open_session(host: &SandboxHost) -> Session {
    Session::open(host, image(), limits()).unwrap()
}

struct RecordingBudget {
    calls: u32,
    decision: BudgetDecision,
    slots: u64,
    now: u64,
    release_at: u64,
}

impl ConcurrencyBudget for RecordingBudget {
    fn decide(&mut self, request: &ConcurrencyRequest) -> BudgetDecision {
        self.calls += 1;
        self.slots = request.slots;
        self.now = request.now;
        self.release_at = request.release_at;
        assert_eq!(request.dimension, BudgetDimension::SandboxConcurrency);
        self.decision.clone()
    }
}

#[test]
fn open_forwards_injected_limits_and_clock() {
    let (host, stub, clock) = world();
    clock.set(1_234);
    let session = open_session(&host);
    assert_eq!(session.opened_at_ms().unwrap(), 1_234);
    assert_eq!(
        session.image().unwrap().reference(),
        "example.invalid/tool:1"
    );
    let resources = session.resources().unwrap();
    assert_eq!(resources.cpu_millicores(), 250);
    assert_eq!(resources.memory_bytes(), 32 * 1024 * 1024);
    assert_eq!(resources.timeout_ms(), 5_000);
    let opens = stub.open_log();
    assert_eq!(opens.len(), 1);
    assert_eq!(opens[0].image.reference(), "example.invalid/tool:1");
    assert_eq!(opens[0].resources, resources);
    assert_eq!(stub.exec_count(), 0);
}

#[test]
fn exec_runs_when_whitelist_and_charge_agree() {
    let (host, stub, _) = world();
    let session = open_session(&host);
    let cmd = command();
    stub.script_exec(
        cmd.argv(),
        BackendExec {
            exit_code: 0,
            stdout: b"ok".to_vec(),
            stderr: b"warn".to_vec(),
            elapsed_ms: 15,
        },
    );
    let root = issue(&["sandbox:exec", "git"], 100);
    let budget = RecordingBudget {
        calls: 0,
        decision: charged(2),
        slots: 0,
        now: 0,
        release_at: 0,
    };
    let mut budget = budget;
    let decision = decide_concurrency(&mut budget, 10, 80);
    assert_eq!(budget.calls, 1);
    assert_eq!(budget.slots, 1);
    assert_eq!(budget.now, 10);
    assert_eq!(budget.release_at, 80);

    let output = session.exec(&cmd, &scope(&root, 10), &decision).unwrap();
    assert_eq!(budget.calls, 1, "exec 不再次询问预算钩子");
    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout, b"ok");
    assert_eq!(output.stderr, b"warn");
    assert_eq!(output.elapsed_ms, 15);
    assert_eq!(output.charge.slots, 2);
    assert_eq!(output.charge.decided_at, 10);
    assert_eq!(output.charge.release_at, 80);
    assert_eq!(output.charge.dimension, BudgetDimension::SandboxConcurrency);
    assert_eq!(stub.exec_count(), 1);
    let execs = stub.exec_log();
    assert_eq!(execs[0].argv, cmd.argv());
    assert_eq!(execs[0].resources, limits());
}

#[test]
fn nonzero_exit_is_a_finished_exec() {
    let (host, stub, _) = world();
    let session = open_session(&host);
    let cmd = command();
    stub.script_exec(
        cmd.argv(),
        BackendExec {
            exit_code: 7,
            stdout: Vec::new(),
            stderr: b"no".to_vec(),
            elapsed_ms: 1,
        },
    );
    let root = issue(&["sandbox:exec"], 100);
    let output = session.exec(&cmd, &scope(&root, 1), &charged(1)).unwrap();
    assert_eq!(output.exit_code, 7);
    assert_eq!(output.stderr, b"no");
}

#[test]
fn missing_tool_does_not_reach_backend() {
    let (host, stub, _) = world();
    let session = open_session(&host);
    let root = issue(&["git"], 100);
    let err = session
        .exec(&command(), &scope(&root, 10), &charged(1))
        .unwrap_err();
    match err {
        SandboxError::ToolDenied {
            tool, fact_present, ..
        } => {
            assert_eq!(tool, SANDBOX_EXEC_TOOL);
            assert!(!fact_present);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(stub.exec_count(), 0);
}

#[test]
fn attenuated_token_keeps_fact_but_exec_is_denied() {
    let (host, stub, _) = world();
    let session = open_session(&host);
    let root = issue(&["sandbox:exec", "git"], 100);
    let child = attenuate(
        &root,
        &ChildSpec {
            tools: vec!["git".into()],
            spend_cap_cents: 100,
        },
    )
    .unwrap();
    let caps = inspect(&child).unwrap();
    assert!(
        caps.tools.contains("sandbox:exec"),
        "衰减不能删掉事实，tools={:?}",
        caps.tools
    );
    let err = session
        .exec(
            &command(),
            &CredentialScope::new(&child, "line-a", 10),
            &charged(1),
        )
        .unwrap_err();
    match err {
        SandboxError::ToolDenied {
            tool, fact_present, ..
        } => {
            assert_eq!(tool, SANDBOX_EXEC_TOOL);
            assert!(fact_present);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(stub.exec_count(), 0);
}

#[test]
fn namespace_mismatch_and_spend_cap_are_denials() {
    let (host, stub, _) = world();
    let session = open_session(&host);
    let root = issue(&["sandbox:exec"], 100);
    let wrong_ns = CredentialScope::new(&root, "line-b", 10);
    let err = session
        .exec(&command(), &wrong_ns, &charged(1))
        .unwrap_err();
    assert!(matches!(
        err,
        SandboxError::ToolDenied {
            fact_present: true,
            ..
        }
    ));

    let over = scope(&root, 101);
    let err = session.exec(&command(), &over, &charged(1)).unwrap_err();
    assert!(matches!(
        err,
        SandboxError::ToolDenied {
            fact_present: true,
            ..
        }
    ));
    assert_eq!(stub.exec_count(), 0);

    session
        .exec(&command(), &scope(&root, 100), &charged(1))
        .unwrap();
    assert_eq!(stub.exec_count(), 1);
}

#[test]
fn budget_refusal_is_echoed_and_not_charged_here() {
    let (host, stub, _) = world();
    let session = open_session(&host);
    let root = issue(&["sandbox:exec"], 100);
    let decision = refusal();
    let err = session
        .exec(&command(), &scope(&root, 10), &decision)
        .unwrap_err();
    match err {
        SandboxError::BudgetRejected { decision: echoed } => assert_eq!(echoed, decision),
        other => panic!("{other:?}"),
    }
    assert_eq!(stub.exec_count(), 0);
}

#[test]
fn wrong_dimension_and_zero_slots_do_not_run() {
    let (host, stub, _) = world();
    let session = open_session(&host);
    let root = issue(&["sandbox:exec"], 100);
    let scope = scope(&root, 10);
    for decision in [
        BudgetDecision::Charged(ConcurrencyCharge {
            dimension: BudgetDimension::Other("calls"),
            slots: 1,
            decided_at: 1,
            release_at: 2,
        }),
        charged(0),
    ] {
        let err = session.exec(&command(), &scope, &decision).unwrap_err();
        match err {
            SandboxError::BudgetRejected { decision: echoed } => assert_eq!(echoed, decision),
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(stub.exec_count(), 0);
}

#[test]
fn sandbox_down_on_open_and_exec() {
    let (host, stub, _) = world();
    stub.set_available(false);
    let err = Session::open(&host, image(), limits()).unwrap_err();
    assert!(matches!(err, SandboxError::SandboxDown));
    assert!(stub.open_log().is_empty());

    stub.set_available(true);
    let session = open_session(&host);
    stub.set_available(false);
    let root = issue(&["git"], 100);
    let err = session
        .exec(&command(), &scope(&root, 10), &refusal())
        .unwrap_err();
    assert!(
        matches!(err, SandboxError::SandboxDown),
        "不可用优先于凭证和预算，实际 {err:?}"
    );
    assert_eq!(stub.exec_count(), 0);
    assert!(session.close().is_err());
    stub.set_available(true);
    session.close().unwrap();
    assert_eq!(stub.close_count(), 1);
    assert_eq!(stub.session_count(), 0);
}

#[test]
fn clock_timeout_does_not_call_backend() {
    let (host, stub, clock) = world();
    let session = open_session(&host);
    clock.set(1_000 + 5_000);
    let root = issue(&["sandbox:exec"], 100);
    let err = session
        .exec(&command(), &scope(&root, 10), &charged(1))
        .unwrap_err();
    assert_eq!(
        match_timeout(err),
        (5_000, 5_000),
        "打开于 1000，上限 5000，此刻 6000"
    );
    assert_eq!(stub.exec_count(), 0);
    clock.set(5_999);
    session
        .exec(&command(), &scope(&root, 10), &charged(1))
        .unwrap();
    assert_eq!(stub.exec_count(), 1);
}

#[test]
fn reported_elapsed_at_the_cap_discards_output() {
    let (host, stub, _) = world();
    let session = open_session(&host);
    let cmd = command();
    stub.script_exec(
        cmd.argv(),
        BackendExec {
            exit_code: 0,
            stdout: b"late".to_vec(),
            stderr: Vec::new(),
            elapsed_ms: 5_000,
        },
    );
    let root = issue(&["sandbox:exec"], 100);
    let err = session
        .exec(&cmd, &scope(&root, 10), &charged(1))
        .unwrap_err();
    assert_eq!(match_timeout(err), (5_000, 5_000));
    assert_eq!(stub.exec_count(), 1);
}

#[test]
fn resume_fences_old_handles_and_allows_a_new_session() {
    let (host, stub, _) = world();
    let old = open_session(&host);
    assert_eq!(old.epoch(), 0);
    assert_eq!(host.resume().unwrap(), 1);
    assert_eq!(stub.retire_count(), 1);
    assert_eq!(stub.session_count(), 0);
    let root = issue(&["sandbox:exec"], 100);
    let err = old
        .exec(&command(), &scope(&root, 10), &charged(1))
        .unwrap_err();
    assert!(matches!(
        err,
        SandboxError::StaleEpoch {
            request_epoch: 0,
            current_epoch: 1,
            request_boot: BOOT,
            current_boot: BOOT,
        }
    ));
    assert!(matches!(
        old.image().unwrap_err(),
        SandboxError::StaleEpoch { .. }
    ));
    assert!(matches!(
        old.close().unwrap_err(),
        SandboxError::StaleEpoch { .. }
    ));
    assert_eq!(stub.exec_count(), 0);
    assert_eq!(stub.close_count(), 0);

    let new_session = open_session(&host);
    assert_eq!(new_session.epoch(), 1);
    new_session
        .exec(&command(), &scope(&root, 10), &charged(1))
        .unwrap();
    assert_eq!(stub.exec_count(), 1);
    assert_eq!(host.resume().unwrap(), 2);
    assert_eq!(stub.retire_count(), 2);
}

#[test]
fn resume_while_down_still_kills_handles() {
    let (host, stub, _) = world();
    let old = open_session(&host);
    stub.set_available(false);
    let err = host.resume().unwrap_err();
    assert!(matches!(err, SandboxError::SandboxDown));
    assert_eq!(host.current_epoch().unwrap(), 1);
    assert_eq!(stub.retire_count(), 0);
    assert_eq!(stub.session_count(), 1);
    let root = issue(&["sandbox:exec"], 100);
    let err = old
        .exec(&command(), &scope(&root, 10), &charged(1))
        .unwrap_err();
    assert!(matches!(err, SandboxError::StaleEpoch { .. }));
    assert_eq!(stub.exec_count(), 0);
}

#[test]
fn backend_restart_invalidates_handles_without_resume() {
    let (host, stub, _) = world();
    let old = open_session(&host);
    let handle = old.handle();
    stub.restart();
    let root = issue(&["sandbox:exec"], 100);
    let err = old
        .exec(&command(), &scope(&root, 10), &charged(1))
        .unwrap_err();
    assert!(matches!(
        err,
        SandboxError::StaleEpoch {
            request_epoch: 0,
            current_epoch: 1,
            ..
        }
    ));
    let err = old
        .exec(&command(), &scope(&root, 10), &charged(1))
        .unwrap_err();
    assert!(matches!(
        err,
        SandboxError::StaleEpoch {
            current_epoch: 1,
            ..
        }
    ));
    assert!(matches!(
        Session::reattach(&host, handle).unwrap_err(),
        SandboxError::StaleEpoch { .. }
    ));
    assert_eq!(stub.exec_count(), 0);
    let new_session = open_session(&host);
    assert_eq!(new_session.epoch(), 1);
    new_session
        .exec(&command(), &scope(&root, 10), &charged(1))
        .unwrap();
    assert_eq!(stub.exec_count(), 1);
}

#[test]
fn reattach_does_not_adopt_a_session_the_host_did_not_open() {
    let (host, stub, clock) = world();
    let session = open_session(&host);
    let handle = session.handle();
    let root = issue(&["sandbox:exec"], 100);

    let other_boot = SandboxHost::boot(stub.clone(), clock.clone(), BOOT + 1);
    let err = Session::reattach(&other_boot, handle).unwrap_err();
    assert!(matches!(
        err,
        SandboxError::StaleEpoch {
            request_boot: BOOT,
            current_boot,
            ..
        } if current_boot == BOOT + 1
    ));

    let same_boot = SandboxHost::boot(stub.clone(), ManualClock::new(0), BOOT);
    let err = Session::reattach(&same_boot, handle).unwrap_err();
    assert!(matches!(
        err,
        SandboxError::SessionGone { session_id } if session_id == handle.id
    ));
    assert_eq!(stub.session_count(), 1);
    assert_eq!(stub.exec_count(), 0);

    let again = Session::reattach(&host, handle).unwrap();
    again
        .exec(&command(), &scope(&root, 10), &charged(1))
        .unwrap();
    assert_eq!(stub.exec_count(), 1);
}

#[test]
fn close_is_idempotent_and_blocks_later_use() {
    let (host, stub, _) = world();
    let session = open_session(&host);
    session.close().unwrap();
    session.close().unwrap();
    assert_eq!(stub.close_count(), 1);
    assert_eq!(stub.session_count(), 0);
    let root = issue(&["sandbox:exec"], 100);
    assert!(matches!(
        session
            .exec(&command(), &scope(&root, 10), &charged(1))
            .unwrap_err(),
        SandboxError::SessionClosed { .. }
    ));
    assert!(matches!(
        session.collect(&[]).unwrap_err(),
        SandboxError::SessionClosed { .. }
    ));
    assert!(matches!(
        Session::reattach(&host, session.handle()).unwrap_err(),
        SandboxError::SessionClosed { .. }
    ));
    assert_eq!(stub.exec_count(), 0);
    assert_eq!(stub.collect_count(), 0);
}

#[test]
fn stale_epoch_wins_over_a_bad_credential_and_a_down_backend() {
    let (host, stub, _) = world();
    let session = open_session(&host);
    host.resume().unwrap();
    stub.set_available(false);
    let root = issue(&["git"], 100);
    let err = session
        .exec(&command(), &scope(&root, 10), &refusal())
        .unwrap_err();
    assert!(matches!(err, SandboxError::StaleEpoch { .. }));
    assert_eq!(stub.exec_count(), 0);
}

#[test]
fn deadline_wins_over_a_bad_credential_when_the_backend_is_up() {
    let (host, stub, clock) = world();
    let session = open_session(&host);
    clock.set(1_000 + 5_000);
    let root = issue(&["git"], 100);
    let err = session
        .exec(&command(), &scope(&root, 10), &charged(1))
        .unwrap_err();
    assert!(matches!(err, SandboxError::TimedOut { .. }));
    assert_eq!(stub.exec_count(), 0);
}

#[test]
fn down_backend_wins_over_the_deadline() {
    let (host, stub, clock) = world();
    let session = open_session(&host);
    clock.set(1_000 + 5_000);
    stub.set_available(false);
    let root = issue(&["sandbox:exec"], 100);
    let err = session
        .exec(&command(), &scope(&root, 10), &charged(1))
        .unwrap_err();
    assert!(matches!(err, SandboxError::SandboxDown));
}

#[test]
fn collect_returns_requested_bytes_and_fails_closed_on_a_miss() {
    let (host, stub, _) = world();
    let session = open_session(&host);
    stub.insert_artifact("/out/a", b"alpha".to_vec());
    stub.insert_artifact("/out/b", b"beta".to_vec());
    let specs = [
        af_sandbox::ArtifactSpec::new("/out/b").unwrap(),
        af_sandbox::ArtifactSpec::new("/out/a").unwrap(),
    ];
    let got = session.collect(&specs).unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].path, "/out/b");
    assert_eq!(got[0].bytes, b"beta");
    assert_eq!(got[1].bytes, b"alpha");
    assert!(session.collect(&[]).unwrap().is_empty());

    let missing = [af_sandbox::ArtifactSpec::new("/missing").unwrap()];
    let err = session.collect(&missing).unwrap_err();
    assert!(matches!(
        err,
        SandboxError::ArtifactMissing { path } if path == "/missing"
    ));
    assert_eq!(stub.collect_count(), 3);

    stub.set_available(false);
    assert!(matches!(
        session.collect(&specs).unwrap_err(),
        SandboxError::SandboxDown
    ));
}

#[test]
fn collect_on_a_restarted_backend_is_stale() {
    let (host, stub, _) = world();
    let session = open_session(&host);
    stub.insert_artifact("/out/a", b"alpha".to_vec());
    stub.restart();
    let specs = [af_sandbox::ArtifactSpec::new("/out/a").unwrap()];
    assert!(matches!(
        session.collect(&specs).unwrap_err(),
        SandboxError::StaleEpoch { .. }
    ));
    assert_eq!(stub.collect_count(), 0);
}

fn match_timeout(err: SandboxError) -> (u64, u64) {
    match err {
        SandboxError::TimedOut {
            timeout_ms,
            elapsed_ms,
        } => (timeout_ms, elapsed_ms),
        other => panic!("{other:?}"),
    }
}
