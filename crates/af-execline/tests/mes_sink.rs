//! 消债 B②③ 验收测试：driver 正式 MES 写缝 + CH 读侧 event_id 对账。
//!
//! - B②（写缝上移）：[`af_execline::MesSink`] 是驱动层唯一 MES 出口——
//!   文件正本（mes-events.jsonl，权威记录）必写，CH 下游由驱动在对账面
//!   统一送批；订单面只声明目标（`MesChTarget`），不做写侧。
//! - B③（幂等升级）：补丁期的 `mes-ch-shipped.jsonl` marker 差量被 CH 读侧
//!   对账取代——ship 前按 `work_order_id` 查 `af_mes.mes_event`，已有
//!   event_id 跳过；送批 Parked = EX2003 fail-loud（真实落账，不接受
//!   "可能已写"）。真实 ClickHouse 不进 `cargo test`（af-mes 纪律），
//!   集成走 `af_mes::mock`；mingda 实测见 RESULT-消债ABC.md。

use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Arc, Mutex};

use af_execline::{
    DriverConfig, EngineClient, ExecContext, ExecOutput, ExeclineError, ExecuteUnit, FixedClock,
    MesChSink, MesChTarget, MesFileWriter, MesSink, MesSyncKind, MesSyncReport,
    OrderLifecycleDriver, StubExecuteUnit,
};
use af_mes::mock::MockClickHouse;
use af_mes::{Accept, ManualClock, MesEvent, MesWriter, WriterConfig};
use af_workorder::{BudgetVoucher, LineBinding, Priority, Sla};

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时钟")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "af-execline-mes-sink-{}-{}-{}",
        process::id(),
        tag,
        nanos
    ));
    fs::create_dir_all(&dir).expect("创建测试 scratch 目录");
    dir
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn event(id: &str, work_order: &str, kind: &str) -> af_mes::MesEvent {
    af_mes::MesEvent::new(work_order, 1_000, id, "line-a", "st-a", kind, 1)
}

/// 用独立 af-mes writer 往 mock CH 预置一行，并返回该行的 RowBinary 原始字节
/// （供读侧对账的 SELECT 响应回放——与 af-mes 自身测试同技法）。
fn seed_ch_row(mock: &MockClickHouse, dir: &Path, row: &af_mes::MesEvent) -> Vec<u8> {
    let outbox = dir.join(format!("seed-outbox-{}", row.event_id));
    let mut config = WriterConfig::new(mock.url(), &outbox);
    config.max_rows = 1;
    config.max_buffered = 1;
    let mut writer = MesWriter::open(config, std::sync::Arc::new(ManualClock::new(0))).unwrap();
    assert_eq!(
        writer.push(row.clone()).unwrap(),
        Accept::Sent { rows: 1 },
        "预置行必须真实入 mock"
    );
    mock.inserts()
        .iter()
        .find(|captured| captured.rows.contains(row))
        .unwrap_or_else(|| panic!("mock 应捕获预置行 {}", row.event_id))
        .raw_rowbinary
        .clone()
}

// ---------- B②：缝形态 ----------

/// 文件正本 sink（缺省形态）：append 落 JSONL；sync = no-op（FileStub 报告）；
/// path 指向 mes-events.jsonl。驱动 open() 缺省即此形态——既有冒烟路径零改动。
#[test]
fn file_sink_append_sync_noop_and_path() {
    let dir = Scratch(scratch("file-sink"));
    let path = dir.0.join("mes-events.jsonl");
    let mut sink = MesFileWriter::new(&path);
    sink.append(&event("e1", "sid-f", "release")).unwrap();
    let report = sink.sync().unwrap();
    assert_eq!(report.kind, MesSyncKind::FileStub);
    assert_eq!(report.shipped, 0, "文件 stub 无 CH 下游，对账面无差量可送");
    assert_eq!(sink.path(), path.as_path());
    assert_eq!(MesFileWriter::load(&path).unwrap().len(), 1);
}

/// CH sink：append 只落文件正本；sync 读侧对账后把全部差量送 CH——
/// 报告只含 host:port（凭据零落回执）。
#[test]
fn ch_sink_ships_delta_and_report_has_no_credentials() {
    let mock = MockClickHouse::start();
    let dir = Scratch(scratch("ch-ship"));
    let target = MesChTarget::Url(mock.url().to_string());
    let mut sink =
        MesChSink::open(&target, &dir.0).expect("CH sink 打开（匿名 mock，无 outbox 残留）");
    sink.append(&event("sid:e1", "sid-c1", "release")).unwrap();
    sink.append(&event("sid:e2", "sid-c1", "close")).unwrap();

    let report = sink.sync().unwrap();
    assert_eq!(report.kind, MesSyncKind::ClickHouse);
    assert_eq!(report.events_total, 2);
    assert_eq!(report.already_in_ch, 0);
    assert_eq!(report.shipped, 2);
    assert_eq!(report.rows_delivered, 2);
    assert_eq!(report.outbox_batches, 0);
    assert_eq!(report.shipped_event_ids, vec!["sid:e1", "sid:e2"]);

    // 线格式：请求线只含 host:port，不出现凭据段（本例本就匿名——
    // 带凭据形态由 target 解析测试覆盖）。
    let host = mock.url().trim_start_matches("http://").to_string();
    assert_eq!(report.endpoint_host, host);

    // mock 侧：每行独立批（max_rows=1），全部被接受
    let inserts = mock.inserts();
    assert!(inserts.iter().all(|captured| captured.accepted));
    assert_eq!(inserts.len(), 2);
    assert_eq!(inserts[0].rows[0].event_id, "sid:e1");
    assert_eq!(inserts[1].rows[0].event_id, "sid:e2");
}

// ---------- B③：读侧 event_id 对账 ----------

/// 差量幂等：CH 已有 e1（SELECT 回放预置行）→ 只送 e2；SELECT 走
/// work_order_id 过滤（读侧对账面）。
#[test]
fn ch_sink_reconciles_by_event_id_and_ships_only_missing() {
    let mock = MockClickHouse::start();
    let dir = Scratch(scratch("ch-delta"));
    let raw_e1 = seed_ch_row(&mock, &dir.0, &event("sid:e1", "sid-r", "release"));
    mock.enqueue_query_body(raw_e1);

    let mut sink = MesChSink::open(&MesChTarget::Url(mock.url().to_string()), &dir.0).unwrap();
    sink.append(&event("sid:e1", "sid-r", "release")).unwrap();
    sink.append(&event("sid:e2", "sid-r", "start")).unwrap();
    let report = sink.sync().unwrap();
    assert_eq!(report.already_in_ch, 1, "e1 已在 CH，读侧认行");
    assert_eq!(report.shipped, 1, "只补差量 e2");
    assert_eq!(report.shipped_event_ids, vec!["sid:e2"]);

    let inserts = mock.inserts();
    let shipped = inserts
        .iter()
        .filter(|captured| {
            captured.accepted && captured.rows.iter().any(|r| r.event_id == "sid:e2")
        })
        .count();
    assert_eq!(shipped, 1, "e2 恰送一次");
    let shipped_e1 = inserts
        .iter()
        .filter(|captured| captured.rows.iter().any(|r| r.event_id == "sid:e1"))
        .count();
    assert_eq!(shipped_e1, 1, "e1 只有预置那一次，sink 不重发");

    let queries = mock.queries();
    let sql = queries.last().expect("SELECT 应已发出");
    assert!(sql.contains("work_order_id"), "对账查询按工单过滤: {sql}");
    // MergeTree 不带 FINAL（mingda CH 26.7 ILLEGAL_FINAL 实测，见 af-mes reader 文档）
    assert!(!sql.contains("FINAL"), "MergeTree 读不带 FINAL: {sql}");
}

/// 全量重叠：CH 已有全部事件 → 零送批、零插入（幂等上限形态）。
#[test]
fn ch_sink_full_overlap_ships_nothing() {
    let mock = MockClickHouse::start();
    let dir = Scratch(scratch("ch-full"));
    let e1 = event("sid:e1", "sid-full", "release");
    let e2 = event("sid:e2", "sid-full", "close");
    let raw_e1 = seed_ch_row(&mock, &dir.0, &e1);
    let raw_e2 = seed_ch_row(&mock, &dir.0, &e2);
    mock.enqueue_query_body([raw_e1, raw_e2].concat());

    let mut sink = MesChSink::open(&MesChTarget::Url(mock.url().to_string()), &dir.0).unwrap();
    sink.append(&e1).unwrap();
    sink.append(&e2).unwrap();
    let report = sink.sync().unwrap();
    assert_eq!(report.shipped, 0);
    assert_eq!(report.already_in_ch, 2);
    let inserts_before = mock.inserts().len();
    assert_eq!(inserts_before, 2, "只有两次预置插入，sink 全程零送批");
}

/// Parked fail-loud：CH 拒收（500）→ EX2003（真实落账纪律：不接受"可能已写"）；
/// outbox 已 park 至少一批（at-least-once 物证）。
#[test]
fn ch_sink_parked_batch_fails_loud_ex2003() {
    let mock = MockClickHouse::start();
    mock.fail_next_inserts(1);
    let dir = Scratch(scratch("ch-park"));
    let mut sink = MesChSink::open(&MesChTarget::Url(mock.url().to_string()), &dir.0).unwrap();
    sink.append(&event("sid:e1", "sid-p", "release")).unwrap();
    let err = sink.sync().unwrap_err();
    assert_eq!(err.code(), af_execline::error::ErrorCode::MesCh, "{err}");
    assert!(err.to_string().contains("EX2003"), "{err}");
    assert!(err.to_string().contains("park"), "{err}");
}

/// park 后重开收敛不双写：失败批经 outbox 重试入 CH 后，读侧对账认行
/// ——不再补送（F-首单 marker 方案会重写，读侧对账结构性修复）。
/// sync 语义：retry_outbox 先行（drain 停运期批）→ 再读侧 SELECT 对账 → 补差量。
#[test]
fn ch_sink_retry_after_park_converges_without_duplicate() {
    let mock = MockClickHouse::start();
    let dir = Scratch(scratch("ch-retry"));
    let e1 = event("sid:e1", "sid-rt", "release");
    let e2 = event("sid:e2", "sid-rt", "start");
    let raw_e1 = seed_ch_row(&mock, &dir.0, &e1);
    mock.enqueue_query_body(raw_e1.clone());

    // 第一轮：CH 拒收一次 → e2 park 进 outbox，EX2003 fail-loud
    let mut sink = MesChSink::open(&MesChTarget::Url(mock.url().to_string()), &dir.0).unwrap();
    sink.append(&e1).unwrap();
    sink.append(&e2).unwrap();
    mock.fail_next_inserts(1);
    let err = sink.sync().unwrap_err();
    assert!(err.to_string().contains("EX2003"), "{err}");
    // 失败批字节仍被 mock 记录（accepted=false）——取其 RowBinary 作二轮 SELECT 回放
    let raw_e2 = mock
        .inserts()
        .iter()
        .find(|captured| !captured.accepted && captured.rows.iter().any(|r| r.event_id == "sid:e2"))
        .expect("失败插入应被捕获")
        .raw_rowbinary
        .clone();
    drop(sink);

    // 第二轮（新实例同目录：outbox 载入 + 文件正本仍在，正本不重写）：
    // retry_outbox 把 e2 送入 → SELECT 回放 {e1,e2} → 零补送。
    mock.enqueue_query_body([raw_e1, raw_e2].concat());
    let mut sink2 = MesChSink::open(&MesChTarget::Url(mock.url().to_string()), &dir.0).unwrap();
    let report = sink2.sync().unwrap();
    assert_eq!(report.shipped, 0, "重试入账后读侧认行，不再补送");
    assert_eq!(report.already_in_ch, 2);

    // CH 终态：e1 恰一次（预置），e2 恰一次（outbox 重试）——零双写
    for id in ["sid:e1", "sid:e2"] {
        let times = mock
            .inserts()
            .iter()
            .filter(|captured| captured.accepted && captured.rows.iter().any(|r| r.event_id == id))
            .count();
        assert_eq!(times, 1, "{id} 在 CH 恰好一次");
    }
}

// ---------- B②：目标声明解析（订单面只留声明） ----------

/// `MesChTarget::parse` + 端点/凭据解析（env MES_CH_URL > 0600 文件 > 报错；
/// URL 可含 user:pass@host，拆出转认证头，endpoint 与回执零凭据）。
#[test]
fn ch_target_parse_and_endpoint_resolution() {
    use af_execline::resolve_mes_ch_endpoint_with;

    assert_eq!(MesChTarget::parse("auto").unwrap(), MesChTarget::Auto);
    assert_eq!(
        MesChTarget::parse("http://mingda:8123").unwrap(),
        MesChTarget::Url("http://mingda:8123".into())
    );

    // auto：env 优先，其次 0600 文件内容，两者皆无 = 显式报错（不静默降级）
    let resolved = resolve_mes_ch_endpoint_with(
        &MesChTarget::Auto,
        Some("http://env-host:8123"),
        Some("http://file-host:8123"),
    )
    .unwrap();
    assert_eq!(resolved.endpoint, "http://env-host:8123");
    assert!(resolved.auth.is_none());
    let resolved = resolve_mes_ch_endpoint_with(
        &MesChTarget::Auto,
        None,
        Some("http://u1:p1@file-host:8123"),
    )
    .unwrap();
    assert_eq!(resolved.endpoint, "http://file-host:8123");
    assert_eq!(resolved.auth, Some(("u1".to_string(), "p1".to_string())));
    assert_eq!(resolved.host_only, "file-host:8123");
    let err = resolve_mes_ch_endpoint_with(&MesChTarget::Auto, None, None).unwrap_err();
    assert_eq!(err.code(), af_execline::error::ErrorCode::MesCh, "{err}");

    // 显式 URL：user:pass@host 拆分；无凭据段 = 匿名；非 http:// 拒绝
    let resolved = resolve_mes_ch_endpoint_with(
        &MesChTarget::Url("http://mesu:mesp@mingda:8123".into()),
        None,
        None,
    )
    .unwrap();
    assert_eq!(resolved.endpoint, "http://mingda:8123");
    assert_eq!(
        resolved.auth,
        Some(("mesu".to_string(), "mesp".to_string()))
    );
    let resolved =
        resolve_mes_ch_endpoint_with(&MesChTarget::Url("http://mingda:8123".into()), None, None)
            .unwrap();
    assert!(resolved.auth.is_none());
    let err = resolve_mes_ch_endpoint_with(&MesChTarget::Url("mingda:8123".into()), None, None)
        .unwrap_err();
    assert_eq!(err.code(), af_execline::error::ErrorCode::MesCh, "{err}");
    // 凭据段残缺（有 @ 无 :）显式报错
    let err = resolve_mes_ch_endpoint_with(
        &MesChTarget::Url("http://useronly@mingda:8123".into()),
        None,
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("user:pass"), "{err}");
}

// ---------- B②：驱动端到端消费（缝唯一出口） ----------

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace 根可解析")
}

fn engine_client() -> EngineClient {
    let target = match std::env::var("CARGO_TARGET_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => workspace_root().join("target"),
    };
    EngineClient::locate(&workspace_root(), &target)
        .expect("af-engine 二进制定位/构建失败（验收要求真引擎子进程）")
}

fn driver_config(order_id: &str, out_dir: PathBuf) -> DriverConfig {
    DriverConfig {
        task_sha256: None,
        plan_sha256: None,
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
        host_alias: "local".into(),
        data_root: PathBuf::new(),
        task_id: String::new(),
        trial: false,
        profile: "stub".into(),
        attempt_layout: af_execline::AttemptLayout::SwevIncrement,
    }
}

/// 驱动消费 CH sink：一次 drive → 文件正本 4 行 + CH 恰收 4 行四类跃迁；
/// 二次 drive（重放）→ 读侧对账认行，零补送、零双写（跨驱记账孔闭合）。
#[test]
fn driver_consumes_ch_sink_end_to_end_and_replay_ships_nothing() {
    let mock = MockClickHouse::start();
    let dir = Scratch(scratch("driver-ch"));
    let order_id = "wo-mes-sink-e2e";
    let cfg = driver_config(order_id, dir.0.clone());
    let client = engine_client();
    let sink = MesChSink::open(&MesChTarget::Url(mock.url().to_string()), &dir.0).unwrap();
    let mut driver = OrderLifecycleDriver::open_with_sink(
        cfg,
        client,
        Box::new(FixedClock::new(1_767_225_600_000)),
        Box::new(sink),
    )
    .unwrap();
    let report = driver
        .drive(&StubExecuteUnit::qualified_final(format!("fa-{order_id}")))
        .unwrap();
    assert_eq!(report.mes_events.len(), 4);
    assert_eq!(report.mes_sync.kind, MesSyncKind::ClickHouse);
    assert_eq!(report.mes_sync.shipped, 4, "四类跃迁全量入 CH");
    let inserts = mock.inserts();
    let mut kinds: Vec<&str> = inserts
        .iter()
        .flat_map(|captured| captured.rows.iter().map(|row| row.kind.as_str()))
        .collect();
    kinds.sort_unstable();
    assert_eq!(kinds, vec!["close", "complete", "release", "start"]);

    // 二驱（重放）：先给 SELECT 回放 4 行（读侧对账认行依据）
    let raw_all: Vec<u8> = mock
        .inserts()
        .iter()
        .flat_map(|captured| captured.raw_rowbinary.iter().copied())
        .collect();
    mock.enqueue_query_body(raw_all);
    let cfg2 = driver_config(order_id, dir.0.clone());
    let sink2 = MesChSink::open(&MesChTarget::Url(mock.url().to_string()), &dir.0).unwrap();
    let mut driver2 = OrderLifecycleDriver::open_with_sink(
        cfg2,
        engine_client(),
        Box::new(FixedClock::new(1_767_225_600_000)),
        Box::new(sink2),
    )
    .unwrap();
    let report2 = driver2
        .drive(&StubExecuteUnit::qualified_final(format!("fa-{order_id}")))
        .unwrap();
    assert!(report2.replayed, "同单二驱为重放");
    assert_eq!(report2.mes_sync.shipped, 0, "读侧对账认行，零补送");
    assert_eq!(report2.mes_sync.already_in_ch, 4);

    let total_accepted: usize = mock
        .inserts()
        .iter()
        .filter(|captured| captured.accepted)
        .map(|captured| captured.rows.len())
        .sum();
    assert_eq!(total_accepted, 4, "CH 终态恰 4 行，零双写");
}

// ---------- MES-FAIL（修复批 2026-10-25）：失败路径收尾对账 guarantee ----------

/// 记账 sink 状态：append 的事件种类 + sync 调用次数。驱动持有
/// `Box<dyn MesSink>`，测试用共享 `Arc` 旁路检视（drop 驱动前即可断言）。
#[derive(Default)]
struct RecordingSinkState {
    appended_kinds: Vec<String>,
    sync_calls: usize,
}

/// 记账 sink：append 全收、sync 计数并回 no-op 报告（对账行为面由
/// 上面 CH 用例覆盖；本组用例只验驱动"失败也调 sync"的编排保证）。
struct RecordingSink {
    state: Arc<Mutex<RecordingSinkState>>,
}

impl MesSink for RecordingSink {
    fn append(&mut self, event: &MesEvent) -> Result<(), ExeclineError> {
        self.state
            .lock()
            .unwrap()
            .appended_kinds
            .push(event.kind.clone());
        Ok(())
    }

    fn sync(&mut self) -> Result<MesSyncReport, ExeclineError> {
        self.state.lock().unwrap().sync_calls += 1;
        Ok(MesSyncReport::file_stub())
    }

    fn path(&self) -> &Path {
        Path::new("recording-sink")
    }
}

/// 总是失败的执行单元（EX3001 路径用）。
struct FailingUnit;

impl ExecuteUnit for FailingUnit {
    fn unit_id(&self) -> &str {
        "mes-fail-unit"
    }

    fn execute(&self, _ctx: &ExecContext<'_>) -> Result<ExecOutput, ExeclineError> {
        Err(ExeclineError::UnitFailed {
            detail: "桩失败：MES-FAIL 失败路径用例".into(),
        })
    }
}

/// MES-FAIL：close 被引擎拒收时，EX1006 照旧透传，但收尾 `mes.sync()`
/// 必须仍执行**恰一次**——已发生的 release/start/complete 事件的对账
/// 不能因失败被跳过（F-第三单探针 F3a2 实证：拒收路径 CH 读回 0 行，
/// 仅订单目录正本有事件）。拒收不打桩：不合格工件走真引擎 close 门。
#[test]
fn drive_close_rejected_still_runs_mes_sync_once() {
    let dir = Scratch(scratch("mes-fail-close"));
    let order_id = "wo-mes-fail-close";
    let state = Arc::new(Mutex::new(RecordingSinkState::default()));
    let mut driver = OrderLifecycleDriver::open_with_sink(
        driver_config(order_id, dir.0.clone()),
        engine_client(),
        Box::new(FixedClock::new(1_767_225_600_000)),
        Box::new(RecordingSink {
            state: state.clone(),
        }),
    )
    .unwrap();
    // is_final=true / qualified=false：complete 无前置门可过，close 门
    // 点名 NoQualifiedFinalArtifact 拒收——失败点在收尾对账之前最后一步。
    let unit = StubExecuteUnit {
        artifact_id: format!("fa-{order_id}"),
        is_final: true,
        qualified: false,
    };

    let err = match driver.drive(&unit) {
        Err(err) => err,
        Ok(_) => panic!("close 门应拒收不合格工件，drive 却成功了"),
    };
    match &err {
        ExeclineError::EngineRejected { op, .. } => assert_eq!(op, "close"),
        other => panic!("期望 EngineRejected(close)，实际 {other:?}"),
    }
    assert_eq!(err.code().as_str(), "EX1006", "{err}");

    let guard = state.lock().unwrap();
    assert_eq!(guard.sync_calls, 1, "失败路径收尾对账必须恰执行一次");
    let mut kinds = guard.appended_kinds.clone();
    kinds.sort_unstable();
    assert_eq!(
        kinds,
        vec!["complete", "release", "start"],
        "已发生事件恰为 release/start/complete（close 未发生）"
    );
}

/// MES-FAIL 同款·执行单元失败路径：EX3001 照旧透传，收尾 `mes.sync()`
/// 仍恰一次、已发生的 release/start 事件入 sink——失败点比 close 拒收
/// 更早（complete 都没到），验证 guarantee 覆盖管线全程而非仅收尾一步。
#[test]
fn drive_unit_failed_still_runs_mes_sync_once() {
    let dir = Scratch(scratch("mes-fail-unit"));
    let order_id = "wo-mes-fail-unit";
    let state = Arc::new(Mutex::new(RecordingSinkState::default()));
    let mut driver = OrderLifecycleDriver::open_with_sink(
        driver_config(order_id, dir.0.clone()),
        engine_client(),
        Box::new(FixedClock::new(1_767_225_600_000)),
        Box::new(RecordingSink {
            state: state.clone(),
        }),
    )
    .unwrap();

    let err = match driver.drive(&FailingUnit) {
        Err(err) => err,
        Ok(_) => panic!("失败单元应使 drive 返回 EX3001"),
    };
    assert_eq!(err.code().as_str(), "EX3001", "{err}");

    let guard = state.lock().unwrap();
    assert_eq!(guard.sync_calls, 1, "单元失败路径收尾对账必须恰执行一次");
    let mut kinds = guard.appended_kinds.clone();
    kinds.sort_unstable();
    assert_eq!(
        kinds,
        vec!["release", "start"],
        "已发生事件恰为 release/start（execute 之后皆未发生）"
    );
}

// ---------- 红队 W1-C35：emit_mes 一次性易失 → 重放按 event_id 补发 ----------

/// start 事件首写失败的 sink（包真 MesFileWriter——其余事件真落盘）。
struct FailStartOnceSink {
    inner: MesFileWriter,
    fixed: Arc<std::sync::atomic::AtomicBool>,
}

impl MesSink for FailStartOnceSink {
    fn append(&mut self, event: &MesEvent) -> Result<(), ExeclineError> {
        if event.kind == "start" && !self.fixed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(ExeclineError::MesCh {
                detail: "注入失败：start 首写失败（W1-C35 用例）".into(),
            });
        }
        self.inner.append(event)
    }
    fn sync(&mut self) -> Result<MesSyncReport, ExeclineError> {
        Ok(MesSyncReport::file_stub())
    }
    fn path(&self) -> &Path {
        self.inner.path()
    }
}

/// C-35：MES 写失败的跃迁此前永久缺席账本（幂等键已落、重放 skip 不再发、
/// 对账真值源恰是没写进去的文件）。修复后：重放路径按 event_id 对账正本，
/// 缺则按该 op 首次成功记录的序号补发——二驱后四类跃迁齐、无重复。
#[test]
fn replay_backfills_mes_event_lost_by_first_write_failure() {
    let dir = Scratch(scratch("w1-c35"));
    let order_id = "wo-mes-w1c35";
    let fixed = Arc::new(std::sync::atomic::AtomicBool::new(false));

    // 一驱：start 的 MES 首写失败 → drive 报错；release 事件已入正本。
    {
        let sink = FailStartOnceSink {
            inner: MesFileWriter::new(dir.0.join("mes-events.jsonl")),
            fixed: fixed.clone(),
        };
        let mut driver = OrderLifecycleDriver::open_with_sink(
            driver_config(order_id, dir.0.clone()),
            engine_client(),
            Box::new(FixedClock::new(1_767_225_600_000)),
            Box::new(sink),
        )
        .unwrap();
        let err = match driver.drive(&StubExecuteUnit::qualified_final(format!("fa-{order_id}"))) {
            Err(err) => err,
            Ok(_) => panic!("start 首写失败应上抛"),
        };
        assert!(err.to_string().contains("W1-C35"), "{err}");
    }
    let kinds1: Vec<String> = MesFileWriter::load(&dir.0.join("mes-events.jsonl"))
        .unwrap()
        .into_iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(kinds1, vec!["release".to_string()], "start 写失败缺席正本");

    // 二驱（重放，sink 修好）：start 按账本序号补发，complete/close 照发。
    fixed.store(true, std::sync::atomic::Ordering::SeqCst);
    {
        let mut driver = OrderLifecycleDriver::open(
            driver_config(order_id, dir.0.clone()),
            engine_client(),
            Box::new(FixedClock::new(1_767_225_600_000)),
        )
        .unwrap();
        driver
            .drive(&StubExecuteUnit::qualified_final(format!("fa-{order_id}")))
            .expect("二驱应走通（重放 + 补发）");
    }
    let events = MesFileWriter::load(&dir.0.join("mes-events.jsonl")).unwrap();
    let mut kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    kinds.sort_unstable();
    assert_eq!(
        kinds,
        vec!["close", "complete", "release", "start"],
        "四类跃迁齐（start 补发），无重复"
    );
    // event_id 全局无重复（补发 id = 原 op 记录序号）。
    let mut ids: Vec<&str> = events.iter().map(|e| e.event_id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 4, "补发不产生重复 event_id");
}

/// 红队 W1-C63：同 event_id 异 line_id = 身份面冲突——event_id 无 line
/// 判别量（stable_id+kind+seq），同 order_id 跨线/跨根的两单事件逐字节同
/// id；此前差量按 event_id 去重会互相吞账（正本 4 条、CH 永少一笔且无
/// 告警）。修复后 sync fail-loud 拒并。
#[test]
fn ch_sink_same_event_id_different_line_fails_loud() {
    let mock = MockClickHouse::start();
    let dir = Scratch(scratch("w1-c63"));
    // CH 已有该 event_id，记账于 line-A。
    let ch_row = af_mes::MesEvent::new(
        "sid-dup".to_string(),
        1_767_225_600_000,
        "sid-dup:release:4".to_string(),
        "line-A".to_string(),
        "af-order-cli".to_string(),
        "release",
        1,
    );
    let raw = seed_ch_row(&mock, &dir.0, &ch_row);
    mock.enqueue_query_body(raw);

    let mut sink = MesChSink::open(&MesChTarget::Url(mock.url().to_string()), &dir.0).unwrap();
    // 本正本：同一 event_id，记账于 line-B（跨线复用同 order_id）。
    let local = af_mes::MesEvent::new(
        "sid-dup".to_string(),
        1_767_225_600_001,
        "sid-dup:release:4".to_string(),
        "line-B".to_string(),
        "af-order-cli".to_string(),
        "release",
        1,
    );
    sink.append(&local).unwrap();
    let err = sink.sync().unwrap_err();
    assert!(err.to_string().contains("身份面冲突"), "{err}");
    assert!(err.to_string().contains("line-A"), "{err}");
    assert!(err.to_string().contains("line-B"), "{err}");
}
