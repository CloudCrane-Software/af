//! 任务包 P2-F01 验收测试：af-execline 订单生命周期 PoC。
//!
//! 覆盖验收标准四类：① 快乐路径全生命周期（ledger 可重载且序号连续，
//! MES 含 release/start/complete/close 四类）；② 门控拒绝（未 attach_voucher
//! 直接 release → 引擎 PreconditionsNotMet，驱动层转类型化错误不 panic）；
//! ③ close 拒绝（无合格终态工件）；④ 幂等重放（同 order_id 二次驱动不产生
//! 重复终态）。另附负向用例：未签名凭证点名、ledger 序号断层、执行单元失败
//! 可重试（ADR-007：测试含拒绝用例与预期错误码）。
//!
//! 引擎是真 af-engine 子进程（`EngineClient::locate` 定位/构建），不打桩。

use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use af_execline::{
    Clock, DriverConfig, EngineClient, EngineRejection, ExeclineError, ExecuteUnit, ExecContext,
    ExecOutput, FixedClock, MesFileWriter, OrderLedger, OrderLifecycleDriver, StubExecuteUnit,
};
use af_mes::MesEvent;
use af_workorder::{
    BudgetVoucher, GraphVersion, LineBinding, Missing, OrderState, Priority, Sla, StableId,
    WorkOrder,
};

/// 固定时钟时刻：2026-01-01T00:00:00Z。重放得到同一时间戳。
const FIXED_MS: i64 = 1_767_225_600_000;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace 根可解析")
}

fn target_dir() -> PathBuf {
    match std::env::var("CARGO_TARGET_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => workspace_root().join("target"),
    }
}

fn engine_client() -> EngineClient {
    EngineClient::locate(&workspace_root(), &target_dir())
        .expect("af-engine 二进制定位/构建失败：验收要求真引擎子进程，不打桩、不静默跳过")
}

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时钟")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("af-execline-it-{}-{}-{}", process::id(), tag, nanos));
    fs::create_dir_all(&dir).expect("创建测试 scratch 目录");
    dir
}

fn config(order_id: &str, out_dir: PathBuf) -> DriverConfig {
    DriverConfig {
        order_id: order_id.to_string(),
        graph_version: "g-poc-1".to_string(),
        sla: Sla::new(None, Priority::Normal),
        line: LineBinding {
            line_id: "line-poc".into(),
            line_version: "lv-1".into(),
        },
        voucher: BudgetVoucher {
            voucher_id: format!("bv-{order_id}"),
            amount: 500,
            signed: true,
        },
        out_dir,
        station_id: "st-poc".to_string(),
    }
}

fn open_driver(order_id: &str, tag: &str) -> (OrderLifecycleDriver, PathBuf) {
    let out = scratch(tag);
    let driver = OrderLifecycleDriver::open(
        config(order_id, out.clone()),
        engine_client(),
        Box::new(FixedClock::new(FIXED_MS)),
    )
    .expect("打开驱动");
    (driver, out)
}

fn stub_unit(order_id: &str) -> StubExecuteUnit {
    StubExecuteUnit::qualified_final(format!("fa-{order_id}"))
}

/// 总是失败的执行单元（F2 环境失败的可重试语义用）。
struct FailingUnit;

impl ExecuteUnit for FailingUnit {
    fn unit_id(&self) -> &str {
        "failing-unit"
    }

    fn execute(&self, _ctx: &ExecContext<'_>) -> Result<ExecOutput, ExeclineError> {
        Err(ExeclineError::UnitFailed {
            detail: "桩失败：模拟执行环境故障".into(),
        })
    }
}

fn ledger_records(out: &Path) -> Vec<af_execline::LedgerRecord> {
    OrderLedger::load(&out.join("ledger.jsonl")).expect("ledger 可重载")
}

fn mes_events(out: &Path) -> Vec<MesEvent> {
    MesFileWriter::load(&out.join("mes-events.jsonl")).expect("MES 事件可重载")
}

fn kinds(events: &[MesEvent]) -> Vec<&str> {
    let mut kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    kinds.sort_unstable();
    kinds
}

// ---------- ① 快乐路径 ----------

#[test]
fn happy_path_full_lifecycle_ledger_and_mes() {
    let (mut driver, out) = open_driver("wo-happy-1", "happy");
    let unit = stub_unit("wo-happy-1");

    let report = driver.drive(&unit).expect("全生命周期应走通");

    // 终态与履历
    assert_eq!(report.order.state(), OrderState::Closed);
    assert!(!report.replayed, "首驱不是重放");
    assert_eq!(report.order.history().len(), 4, "release/start/complete/close 四条边");
    assert_eq!(report.order.artifacts().len(), 1, "执行桩工件已登记");
    // stable_id 按 af-workorder 派生规则（域 af-workorder/stable-id/v1\0 || order_id）
    assert_eq!(
        report.order.stable_id(),
        &StableId::generate("wo-happy-1"),
        "stable_id 与 af-workorder::StableId::generate 同规则"
    );
    // 判定走真引擎子进程
    assert!(
        driver.engine_bin().ends_with("af-engine"),
        "引擎二进制路径: {}",
        driver.engine_bin().display()
    );

    // ledger：可重载且序号连续，全部关键 op 在账
    let records = ledger_records(&out);
    let seqs: Vec<u64> = records.iter().map(|r| r.seq).collect();
    let expected: Vec<u64> = (1..=records.len() as u64).collect();
    assert_eq!(seqs, expected, "序号必须从 1 连续");
    for op in [
        "create_order",
        "bind_line",
        "attach_voucher",
        "release",
        "start",
        "attach_artifact",
        "complete",
        "close",
    ] {
        assert!(
            records.iter().any(|r| r.op == op && r.ok),
            "ledger 缺少成功记录: {op}"
        );
    }
    assert_eq!(
        records.iter().filter(|r| r.op == "close" && r.ok).count(),
        1,
        "close 成功记录唯一"
    );
    // 每条成功记录都带整单快照——账本自成恢复源
    assert!(records
        .iter()
        .filter(|r| r.ok)
        .all(|r| r.order_after.is_some()));

    // MES：release/start/complete/close 四类齐全，work_order_id 收敛到 stable_id（ADR-004）
    let events = mes_events(&out);
    assert_eq!(
        kinds(&events),
        vec!["close", "complete", "release", "start"],
        "MES 事件恰为四类跃迁"
    );
    assert!(
        events
            .iter()
            .all(|e| e.work_order_id == report.order.stable_id().as_str()),
        "work_order_id 必须是 stable_id，不是 factory- 旧约定"
    );
    assert!(events.iter().all(|e| e.line_id == "line-poc"));
    assert!(events.iter().all(|e| e.station_id == "st-poc"));
    assert!(events.iter().all(|e| e.occurred_at == FIXED_MS));

    // 工件文件落盘
    assert!(
        out.join("artifacts").join("fa-wo-happy-1.json").is_file(),
        "执行桩工件文件应在 out/artifacts/ 下"
    );
}

// ---------- ② 门控拒绝：未 attach_voucher 直接 release ----------

#[test]
fn release_without_voucher_is_typed_gate_rejection_not_panic() {
    let (mut driver, out) = open_driver("wo-gate-1", "gate-no-voucher");
    driver.create_order().unwrap();
    driver.bind_line().unwrap();

    let err = driver.release().unwrap_err();
    match &err {
        ExeclineError::EngineRejected { op, rejection } => {
            assert_eq!(op, "release");
            match rejection {
                EngineRejection::Gate { gate } => assert_eq!(
                    gate.missing().unwrap(),
                    &[Missing::BudgetVoucherNotIssued],
                    "门控必须类型化点名缺预算凭证"
                ),
                other => panic!("期望 Gate 拒绝，实际 {other:?}"),
            }
        }
        other => panic!("期望 EngineRejected，实际 {other:?}"),
    }
    assert_eq!(err.code().as_str(), "EX1006", "稳定错误码");
    assert_eq!(driver.state(), OrderState::Created, "拒绝后状态不得跃迁");

    // 拒绝也入账（审计），且不带快照
    let records = ledger_records(&out);
    let rejected = records
        .iter()
        .find(|r| r.op == "release" && !r.ok)
        .expect("拒绝的 release 应入账");
    assert_eq!(rejected.error_kind.as_deref(), Some("gate"));
    assert!(rejected.order_after.is_none());
}

#[test]
fn release_with_unsigned_voucher_names_the_voucher() {
    let out = scratch("gate-unsigned");
    let mut cfg = config("wo-gate-2", out.clone());
    cfg.voucher.signed = false;
    let mut driver = OrderLifecycleDriver::open(
        cfg,
        engine_client(),
        Box::new(FixedClock::new(FIXED_MS)),
    )
    .expect("打开驱动");
    driver.create_order().unwrap();
    driver.bind_line().unwrap();
    driver.attach_voucher().unwrap();

    let err = driver.release().unwrap_err();
    match &err {
        ExeclineError::EngineRejected { rejection, .. } => match rejection {
            EngineRejection::Gate { gate } => assert_eq!(
                gate.missing().unwrap(),
                &[Missing::BudgetVoucherNotSigned {
                    voucher_id: "bv-wo-gate-2".into()
                }],
                "未签名凭证必须点名凭证号"
            ),
            other => panic!("期望 Gate 拒绝，实际 {other:?}"),
        },
        other => panic!("期望 EngineRejected，实际 {other:?}"),
    }
}

// ---------- ③ close 拒绝：无合格终态工件 ----------

#[test]
fn close_without_qualified_final_artifact_is_rejected() {
    let (mut driver, out) = open_driver("wo-gate-3", "gate-close");
    driver.create_order().unwrap();
    driver.bind_line().unwrap();
    driver.attach_voucher().unwrap();
    driver.release().unwrap();
    driver.start().unwrap();
    // 不执行任何单元：无工件直接 complete
    driver.complete().unwrap();

    let err = driver.close().unwrap_err();
    match &err {
        ExeclineError::EngineRejected { op, rejection } => {
            assert_eq!(op, "close");
            match rejection {
                EngineRejection::Gate { gate } => assert_eq!(
                    gate.missing().unwrap(),
                    &[Missing::NoQualifiedFinalArtifact],
                    "close 门必须点名缺合格终态工件"
                ),
                other => panic!("期望 Gate 拒绝，实际 {other:?}"),
            }
        }
        other => panic!("期望 EngineRejected，实际 {other:?}"),
    }
    assert_eq!(err.code().as_str(), "EX1006");
    assert_eq!(driver.state(), OrderState::Completed, "拒绝后停在 Completed");

    // 补上合格终态工件后放行
    let output = driver.execute(&stub_unit("wo-gate-3")).unwrap();
    assert_eq!(output.artifacts.len(), 1);
    driver.close().expect("补齐工件后 close 应放行");
    assert_eq!(driver.state(), OrderState::Closed);

    // close 全程只有一条成功记录
    let records = ledger_records(&out);
    assert_eq!(
        records.iter().filter(|r| r.op == "close" && r.ok).count(),
        1
    );
}

// ---------- ④ 幂等重放 ----------

#[test]
fn second_drive_replays_without_duplicate_terminal() {
    let (mut driver, out) = open_driver("wo-idem-1", "idem");
    let unit = stub_unit("wo-idem-1");
    let first = driver.drive(&unit).expect("首驱走通");
    assert!(!first.replayed);
    assert_eq!(first.order.state(), OrderState::Closed);
    let history_after_first = first.order.history().len();
    drop(driver);

    // 同一 order_id、同一 out_dir：二次驱动
    let mut driver2 = OrderLifecycleDriver::open(
        config("wo-idem-1", out.clone()),
        engine_client(),
        Box::new(FixedClock::new(FIXED_MS)),
    )
    .expect("二次打开驱动（ledger 重载恢复）");
    let second = driver2.drive(&unit).expect("二次驱动应幂等放行");

    assert!(second.replayed, "二次驱动必须标记为重放");
    assert_eq!(second.order.state(), OrderState::Closed, "终态不变");
    assert_eq!(
        second.order.history().len(),
        history_after_first,
        "不得产生重复跃迁"
    );
    assert!(
        second.mes_events.is_empty(),
        "重放不得重复发 MES 事件"
    );

    // 账本证据：close 成功记录仍唯一；出现 replay_skip；序号连续
    let records = ledger_records(&out);
    assert_eq!(
        records.iter().filter(|r| r.op == "close" && r.ok).count(),
        1,
        "终态不得重复入账"
    );
    assert!(
        records.iter().any(|r| r.op == "replay_skip"),
        "重放应有 replay_skip 审计行"
    );
    let seqs: Vec<u64> = records.iter().map(|r| r.seq).collect();
    let expected: Vec<u64> = (1..=records.len() as u64).collect();
    assert_eq!(seqs, expected);

    // MES 事件不重复
    assert_eq!(mes_events(&out).len(), 4, "重放后 MES 仍只有四类各一");

    // 工件文件不重写（内容仍是首驱产物）
    let artifact_file = out.join("artifacts").join("fa-wo-idem-1.json");
    assert!(artifact_file.is_file());
}

// ---------- 负向：执行单元失败可重试（F2 环境失败） ----------

#[test]
fn failed_unit_keeps_order_in_progress_and_retry_completes() {
    let (mut driver, out) = open_driver("wo-retry-1", "retry");
    driver.create_order().unwrap();
    driver.bind_line().unwrap();
    driver.attach_voucher().unwrap();
    driver.release().unwrap();
    driver.start().unwrap();

    let err = driver.execute(&FailingUnit).unwrap_err();
    assert_eq!(err.code().as_str(), "EX3001", "执行单元失败稳定码");
    assert_eq!(driver.state(), OrderState::InProgress, "失败后停在 InProgress");
    let records = ledger_records(&out);
    assert!(
        records.iter().any(|r| r.op == "attach_artifact" && !r.ok),
        "失败的 attach_artifact 入账"
    );

    // 重试：同一驱动换合格单元，走完剩余生命周期
    let output = driver.execute(&stub_unit("wo-retry-1")).expect("重试应放行");
    assert_eq!(output.artifacts.len(), 1);
    driver.complete().unwrap();
    driver.close().unwrap();
    assert_eq!(driver.state(), OrderState::Closed);

    let records = ledger_records(&out);
    assert_eq!(
        records
            .iter()
            .filter(|r| r.op == "attach_artifact" && r.ok)
            .count(),
        1,
        "成功的 attach_artifact 唯一"
    );
    assert_eq!(
        records.iter().filter(|r| r.op == "close" && r.ok).count(),
        1
    );
}

// ---------- 负向：ledger 序号断层 ----------

#[test]
fn ledger_seq_gap_is_rejected_as_corrupt() {
    let out = scratch("ledger-gap");
    let path = out.join("ledger.jsonl");
    let order = WorkOrder::new("wo-gap", GraphVersion::new("g-poc-1"), Sla::new(None, Priority::Normal));
    let base = af_execline::LedgerRecord {
        seq: 0,
        ts_ms: FIXED_MS,
        order_id: "wo-gap".into(),
        stable_id: StableId::generate("wo-gap").as_str().to_string(),
        op: "create_order".into(),
        idem_key: format!("{}/create_order", StableId::generate("wo-gap").as_str()),
        ok: true,
        state_after: Some("Created".into()),
        error_kind: None,
        error_detail: None,
        order_after: Some(order),
    };
    let mut text = String::new();
    for seq in [1u64, 3] {
        let mut record = base.clone();
        record.seq = seq;
        text.push_str(&serde_json::to_string(&record).unwrap());
        text.push('\n');
    }
    fs::write(&path, text).unwrap();

    let err = OrderLedger::load(&path).unwrap_err();
    assert_eq!(err.code().as_str(), "EX2002", "序号断层 = 账本损坏");
    assert!(err.to_string().contains("序号不连续"), "{err}");

    // 身份不一致同样拒绝（换 order_id 的账本不能冒充）
    let mut foreign = base.clone();
    foreign.seq = 1;
    foreign.order_id = "wo-other".into();
    foreign.stable_id = StableId::generate("wo-other").as_str().to_string();
    foreign.idem_key = format!("{}/create_order", StableId::generate("wo-other").as_str());
    fs::write(&path, format!("{}\n", serde_json::to_string(&foreign).unwrap())).unwrap();
    let err = OrderLedger::load(&path).unwrap_err();
    assert_eq!(err.code().as_str(), "EX2002");
}

// ---------- 负向：ledger 身份与订单不符时驱动拒绝打开 ----------

#[test]
fn driver_open_rejects_ledger_of_another_order() {
    let out = scratch("foreign-ledger");
    let path = out.join("ledger.jsonl");
    let foreign_id = "wo-foreign";
    let order = WorkOrder::new(foreign_id, GraphVersion::new("g-poc-1"), Sla::new(None, Priority::Normal));
    let record = af_execline::LedgerRecord {
        seq: 1,
        ts_ms: FIXED_MS,
        order_id: foreign_id.into(),
        stable_id: StableId::generate(foreign_id).as_str().to_string(),
        op: "create_order".into(),
        idem_key: format!("{}/create_order", StableId::generate(foreign_id).as_str()),
        ok: true,
        state_after: Some("Created".into()),
        error_kind: None,
        error_detail: None,
        order_after: Some(order),
    };
    fs::write(&path, format!("{}\n", serde_json::to_string(&record).unwrap())).unwrap();

    let err = OrderLifecycleDriver::open(
        config("wo-mine", out.clone()),
        engine_client(),
        Box::new(FixedClock::new(FIXED_MS)),
    )
    .unwrap_err();
    assert_eq!(err.code().as_str(), "EX2002", "异单账本不得冒充");
}
