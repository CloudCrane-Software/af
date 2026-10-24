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

use af_execline::{
    DriverConfig, EngineClient, ExeclineError, FixedClock, MesChSink, MesChTarget, MesFileWriter,
    MesSink, MesSyncKind, OrderLifecycleDriver, StubExecuteUnit,
};
use af_mes::mock::MockClickHouse;
use af_mes::{Accept, EventFilter, ManualClock, MesReader, MesWriter, WriterConfig};
use af_workorder::{BudgetVoucher, GraphVersion, LineBinding, Priority, Sla};

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

    let mut sink = MesChSink::open(
        &MesChTarget::Url(mock.url().to_string()),
        &dir.0,
    )
    .unwrap();
    sink.append(&event("sid:e1", "sid-r", "release")).unwrap();
    sink.append(&event("sid:e2", "sid-r", "start")).unwrap();
    let report = sink.sync().unwrap();
    assert_eq!(report.already_in_ch, 1, "e1 已在 CH，读侧认行");
    assert_eq!(report.shipped, 1, "只补差量 e2");
    assert_eq!(report.shipped_event_ids, vec!["sid:e2"]);

    let inserts = mock.inserts();
    let shipped = inserts
        .iter()
        .filter(|captured| captured.accepted && captured.rows.iter().any(|r| r.event_id == "sid:e2"))
        .count();
    assert_eq!(shipped, 1, "e2 恰送一次");
    let shipped_e1 = inserts
        .iter()
        .filter(|captured| captured.rows.iter().any(|r| r.event_id == "sid:e1"))
        .count();
    assert_eq!(shipped_e1, 1, "e1 只有预置那一次，sink 不重发");

    let sql = mock.queries().last().expect("SELECT 应已发出");
    assert!(sql.contains("work_order_id"), "对账查询按工单过滤: {sql}");
    assert!(sql.contains("FINAL"), "读包装器口径: {sql}");
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

    let mut sink = MesChSink::open(
        &MesChTarget::Url(mock.url().to_string()),
        &dir.0,
    )
    .unwrap();
    sink.append(&e1).unwrap();
    sink.append(&e2).unwrap();
    let report = sink.sync().unwrap();
    assert_eq!(report.shipped, 0);
    assert_eq!(report.already_in_ch, 2);
    let inserts_before = mock.inserts().len();
    assert_eq!(
        inserts_before, 2,
        "只有两次预置插入，sink 全程零送批"
    );
}

/// Parked fail-loud：CH 拒收（500）→ EX2003（真实落账纪律：不接受"可能已写"）；
/// outbox 已 park 至少一批（at-least-once 物证）。
#[test]
fn ch_sink_parked_batch_fails_loud_ex2003() {
    let mock = MockClickHouse::start();
    mock.fail_next_inserts(1);
    let dir = Scratch(scratch("ch-park"));
    let mut sink = MesChSink::open(
        &MesChTarget::Url(mock.url().to_string()),
        &dir.0,
    )
    .unwrap();
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
    let mut sink = MesChSink::open(
        &MesChTarget::Url(mock.url().to_string()),
        &dir.0,
    )
    .unwrap();
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
    let mut sink2 = MesChSink::open(
        &MesChTarget::Url(mock.url().to_string()),
        &dir.0,
    )
    .unwrap();
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

    assert_eq!(MesChTarget::parse("auto"), Ok(MesChTarget::Auto));
    assert_eq!(
        MesChTarget::parse("http://mingda:8123"),
        Ok(MesChTarget::Url("http://mingda:8123".into()))
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
    let resolved = resolve_mes_ch_endpoint_with(&MesChTarget::Auto, None, Some("u1:p1@http-x"))
        .unwrap();
    assert_eq!(resolved.auth, Some(("u1".to_string(), "p1".to_string())));
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
    let resolved = resolve_mes_ch_endpoint_with(
        &MesChTarget::Url("http://mingda:8123".into()),
        None,
        None,
    )
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
    let sink = MesChSink::open(
        &MesChTarget::Url(mock.url().to_string()),
        &dir.0,
    )
    .unwrap();
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
    let mut kinds: Vec<&str> = mock
        .inserts()
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
    let sink2 = MesChSink::open(
        &MesChTarget::Url(mock.url().to_string()),
        &dir.0,
    )
    .unwrap();
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
