//! mock HTTP：RowBinary 批格式、双阈值、失败进 outbox 再重试。
//! 不连接真实 ClickHouse。时刻只用 ManualClock。

use af_mes::mock::MockClickHouse;
use af_mes::{
    Accept, Clock, EventFilter, ManualClock, MesError, MesEvent, MesReader, MesWriter,
    WriterConfig, DEFAULT_MAX_ROWS, SCHEMA_STATEMENTS,
};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn scratch(label: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "af-mes-{label}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    path
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn event(id: &str, at: i64) -> MesEvent {
    MesEvent::new("WO", at, id, "L", "S", "start", 3)
}

fn expected_rowbinary(event: &MesEvent) -> Vec<u8> {
    let mut raw = Vec::new();
    push_str(&mut raw, &event.work_order_id);
    raw.extend_from_slice(&event.occurred_at.to_le_bytes());
    push_str(&mut raw, &event.event_id);
    push_str(&mut raw, &event.line_id);
    push_str(&mut raw, &event.station_id);
    push_str(&mut raw, &event.kind);
    raw.extend_from_slice(&event.quantity.to_le_bytes());
    raw
}

fn push_str(buf: &mut Vec<u8>, text: &str) {
    assert!(text.len() < 128, "测试字符串走单字节 LEB128");
    buf.push(u8::try_from(text.len()).unwrap());
    buf.extend_from_slice(text.as_bytes());
}

fn assert_rowbinary_insert(captured: &af_mes::mock::CapturedInsert, rows: &[MesEvent]) {
    assert!(
        captured.query.contains("FORMAT RowBinary"),
        "query={}",
        captured.query
    );
    assert!(
        !captured.query.contains("RowBinaryWithNamesAndTypes"),
        "query={}",
        captured.query
    );
    assert!(captured.decompress_flag, "插入必须带 decompress=1");
    assert!(captured.lz4_framed, "body 必须是 lz4 帧（magic 0x82）");
    assert_eq!(captured.rows, rows);
    let mut expected = Vec::new();
    for row in rows {
        expected.extend(expected_rowbinary(row));
    }
    assert_eq!(captured.raw_rowbinary, expected);
}

#[test]
fn row_threshold_sends_one_rowbinary_batch() {
    let mock = MockClickHouse::start();
    let dir = Scratch(scratch("rows"));
    let clock = Arc::new(ManualClock::new(1_000));
    let mut config = WriterConfig::new(mock.url(), &dir.0);
    config.max_rows = 3;
    config.max_buffered = 3;
    config.max_delay = Duration::from_secs(60);
    let mut writer = MesWriter::open(config, clock).unwrap();

    let rows = vec![event("a", 10), event("b", 11), event("c", 12)];
    assert!(matches!(
        writer.push(rows[0].clone()).unwrap(),
        Accept::Buffered { pending: 1 }
    ));
    assert!(writer.poll().unwrap() == Accept::Buffered { pending: 1 });
    assert!(mock.inserts().is_empty());
    writer.push(rows[1].clone()).unwrap();
    assert!(mock.inserts().is_empty());
    let accepted = writer.push(rows[2].clone()).unwrap();
    assert_eq!(accepted, Accept::Sent { rows: 3 });
    assert_eq!(writer.pending_rows(), 0);

    let inserts = mock.inserts();
    assert_eq!(inserts.len(), 1);
    assert!(inserts[0].accepted);
    assert_rowbinary_insert(&inserts[0], &rows);
}

#[test]
fn delay_flushes_before_row_threshold_on_injected_clock() {
    let mock = MockClickHouse::start();
    let dir = Scratch(scratch("delay"));
    let clock = Arc::new(ManualClock::new(5_000));
    let mut config = WriterConfig::new(mock.url(), &dir.0);
    config.max_rows = DEFAULT_MAX_ROWS;
    config.max_delay = Duration::from_millis(500);
    let mut writer = MesWriter::open(config, Arc::clone(&clock) as Arc<dyn Clock>).unwrap();

    let first = event("a", 5_000);
    assert!(matches!(
        writer.push(first.clone()).unwrap(),
        Accept::Buffered { .. }
    ));
    clock.advance(499);
    assert_eq!(writer.poll().unwrap(), Accept::Buffered { pending: 1 });
    assert!(mock.inserts().is_empty());

    clock.advance(1);
    assert_eq!(writer.poll().unwrap(), Accept::Sent { rows: 1 });
    let inserts = mock.inserts();
    assert_eq!(inserts.len(), 1);
    assert_rowbinary_insert(&inserts[0], std::slice::from_ref(&first));

    let second = event("b", 5_500);
    clock.advance(10_000);
    let accepted = writer.push(second.clone()).unwrap();
    assert_eq!(accepted, Accept::Buffered { pending: 1 });
    let inserts = mock.inserts();
    assert_eq!(
        inserts.len(),
        1,
        "新行自己的延迟还没走完，不能跟已发送的批绑在一起"
    );
    assert_eq!(inserts[0].rows, vec![first]);
    assert_eq!(writer.pending_rows(), 1);
    let _ = second;
}

#[test]
fn due_batch_is_sent_before_the_new_row_is_appended() {
    let mock = MockClickHouse::start();
    let dir = Scratch(scratch("split"));
    let clock = Arc::new(ManualClock::new(0));
    let mut config = WriterConfig::new(mock.url(), &dir.0);
    config.max_rows = 100;
    config.max_delay = Duration::from_millis(10);
    let mut writer = MesWriter::open(config, Arc::clone(&clock) as Arc<dyn Clock>).unwrap();
    writer.push(event("old", 1)).unwrap();
    clock.advance(10);
    let accepted = writer.push(event("new", 2)).unwrap();
    assert_eq!(accepted, Accept::Buffered { pending: 1 });
    let inserts = mock.inserts();
    assert_eq!(inserts.len(), 1);
    assert_eq!(inserts[0].rows[0].event_id, "old");
    assert_eq!(writer.pending_rows(), 1);
}

#[test]
fn fifty_thousand_rows_flush_as_one_batch() {
    let mock = MockClickHouse::start();
    let dir = Scratch(scratch("50k"));
    let clock = Arc::new(ManualClock::new(0));
    let mut config = WriterConfig::new(mock.url(), &dir.0);
    config.max_delay = Duration::from_secs(3600);
    assert_eq!(config.max_rows, 50_000);
    let mut writer = MesWriter::open(config, clock).unwrap();
    for index in 0..49_999 {
        let id = format!("e{index}");
        let accepted = writer.push(event(&id, index)).unwrap();
        assert!(
            matches!(accepted, Accept::Buffered { .. }),
            "第 {index} 行不该发送"
        );
    }
    assert!(mock.inserts().is_empty());
    let accepted = writer.push(event("last", 49_999)).unwrap();
    assert_eq!(accepted, Accept::Sent { rows: 50_000 });
    let inserts = mock.inserts();
    assert_eq!(inserts.len(), 1);
    assert!(inserts[0].accepted);
    assert!(inserts[0].decompress_flag);
    assert!(inserts[0].lz4_framed);
    assert!(inserts[0].query.contains("FORMAT RowBinary"));
    assert!(!inserts[0].query.contains("RowBinaryWithNamesAndTypes"));
    assert_eq!(inserts[0].rows.len(), 50_000);
    assert_eq!(inserts[0].rows[0].event_id, "e0");
    assert_eq!(inserts[0].rows[49_999].event_id, "last");
    assert_eq!(inserts[0].rows[49_999].occurred_at, 49_999);
}

#[test]
fn failed_insert_is_parked_and_retried_without_loss() {
    let mock = MockClickHouse::start();
    mock.fail_next_inserts(1);
    let dir = Scratch(scratch("retry"));
    let clock = Arc::new(ManualClock::new(0));
    let mut config = WriterConfig::new(mock.url(), &dir.0);
    config.max_rows = 2;
    config.max_buffered = 2;
    config.max_delay = Duration::from_secs(60);
    let mut writer = MesWriter::open(config, clock).unwrap();
    let rows = vec![event("a", 1), event("b", 2)];
    writer.push(rows[0].clone()).unwrap();
    let accepted = writer.push(rows[1].clone()).unwrap();
    assert_eq!(accepted, Accept::Parked { rows: 2 });
    assert_eq!(writer.pending_rows(), 0);
    assert_eq!(writer.outbox_batches(), 1);
    assert!(writer.last_delivery_error().is_some());

    let failed = mock.inserts();
    assert_eq!(failed.len(), 1);
    assert!(!failed[0].accepted);
    assert_rowbinary_insert(&failed[0], &rows);

    drop(writer);
    let clock = Arc::new(ManualClock::new(0));
    let mut config = WriterConfig::new(mock.url(), &dir.0);
    config.max_rows = 2;
    config.max_buffered = 2;
    let mut writer = MesWriter::open(config, clock).unwrap();
    assert_eq!(writer.outbox_batches(), 1);
    assert_eq!(writer.retry_outbox().unwrap(), 1);
    assert_eq!(writer.outbox_batches(), 0);

    let inserts = mock.inserts();
    assert_eq!(inserts.len(), 2);
    assert!(inserts[1].accepted);
    assert_rowbinary_insert(&inserts[1], &rows);
}

#[test]
fn newer_batch_waits_behind_an_unsent_outbox_file() {
    let mock = MockClickHouse::start();
    mock.fail_next_inserts(2);
    let dir = Scratch(scratch("order"));
    let clock = Arc::new(ManualClock::new(0));
    let mut config = WriterConfig::new(mock.url(), &dir.0);
    config.max_rows = 1;
    config.max_buffered = 1;
    config.max_outbox_batches = 4;
    config.max_delay = Duration::from_secs(60);
    let mut writer = MesWriter::open(config, clock).unwrap();

    assert_eq!(
        writer.push(event("a", 1)).unwrap(),
        Accept::Parked { rows: 1 }
    );
    assert_eq!(
        writer.push(event("b", 2)).unwrap(),
        Accept::Parked { rows: 1 }
    );
    assert_eq!(writer.outbox_batches(), 2);
    let during_outage = mock.inserts();
    assert_eq!(during_outage.len(), 2);
    assert!(during_outage.iter().all(|insert| {
        !insert.accepted && insert.rows.len() == 1 && insert.rows[0].event_id == "a"
    }));

    assert_eq!(writer.retry_outbox().unwrap(), 2);
    assert_eq!(writer.outbox_batches(), 0);
    let inserts = mock.inserts();
    assert_eq!(inserts.len(), 4);
    assert!(inserts[2].accepted && inserts[2].rows[0].event_id == "a");
    assert!(inserts[3].accepted && inserts[3].rows[0].event_id == "b");
    assert_rowbinary_insert(&inserts[2], &[event("a", 1)]);
    assert_rowbinary_insert(&inserts[3], &[event("b", 2)]);
}

#[test]
fn outbox_saturation_returns_backpressure_and_keeps_the_row() {
    let mock = MockClickHouse::start();
    mock.fail_next_inserts(10);
    let dir = Scratch(scratch("bp"));
    let clock = Arc::new(ManualClock::new(0));
    let mut config = WriterConfig::new(mock.url(), &dir.0);
    config.max_rows = 1;
    config.max_buffered = 1;
    config.max_outbox_batches = 1;
    config.max_delay = Duration::from_secs(60);
    let mut writer = MesWriter::open(config, clock).unwrap();
    let parked = writer.push(event("kept", 1)).unwrap();
    assert_eq!(parked, Accept::Parked { rows: 1 });
    let rejected = event("nope", 2);
    let err = writer.push(rejected.clone()).unwrap_err();
    match err {
        MesError::Backpressure {
            outbox_batches,
            outbox_limit,
            reason,
            ..
        } => {
            assert_eq!(outbox_batches, 1);
            assert_eq!(outbox_limit, 1);
            assert_eq!(reason, "outbox 已满");
        }
        other => panic!("期望背压，得到 {other}"),
    }
    assert_eq!(writer.pending_rows(), 0);
    assert_eq!(writer.outbox_batches(), 1);
    let inserts = mock.inserts();
    assert_eq!(inserts.len(), 1);
    assert_eq!(inserts[0].rows, vec![event("kept", 1)]);
    assert!(!inserts[0].rows.iter().any(|row| row.event_id == "nope"));
}

#[test]
fn outbox_write_failure_keeps_the_batch_in_memory() {
    let mock = MockClickHouse::start();
    mock.fail_next_inserts(5);
    let dir = Scratch(scratch("disk"));
    let clock = Arc::new(ManualClock::new(0));
    let mut config = WriterConfig::new(mock.url(), &dir.0);
    config.max_rows = 1;
    config.max_buffered = 1;
    let mut writer = MesWriter::open(config, clock).unwrap();
    fs::remove_dir_all(&dir.0).unwrap();
    fs::write(&dir.0, b"not a directory").unwrap();
    let err = writer.push(event("mem", 7)).unwrap_err();
    match err {
        MesError::OutboxPersist { rows, .. } => assert_eq!(rows, 1),
        other => panic!("期望 outbox 落盘失败，得到 {other}"),
    }
    assert_eq!(writer.pending_rows(), 1);
    assert_eq!(writer.outbox_batches(), 0);
}

#[test]
fn apply_schema_sends_ddl_constants() {
    let mock = MockClickHouse::start();
    let dir = Scratch(scratch("ddl"));
    let clock = Arc::new(ManualClock::new(0));
    let writer = MesWriter::open(WriterConfig::new(mock.url(), &dir.0), clock).unwrap();
    writer.apply_schema().unwrap();
    let queries = mock.queries();
    assert_eq!(queries.len(), SCHEMA_STATEMENTS.len());
    for (got, expect) in queries.iter().zip(SCHEMA_STATEMENTS) {
        assert_eq!(got, expect);
        assert!(!got.to_ascii_lowercase().contains("rand64"));
        assert!(!got.to_ascii_lowercase().contains("optimize"));
    }
}

#[test]
fn reader_sends_select_final_and_decodes_rowbinary() {
    let mock = MockClickHouse::start();
    let dir = Scratch(scratch("read"));
    let clock = Arc::new(ManualClock::new(0));
    let mut config = WriterConfig::new(mock.url(), &dir.0);
    config.max_rows = 1;
    config.max_buffered = 1;
    let mut writer = MesWriter::open(config, clock).unwrap();
    let row = MesEvent::new("WO'1", 42, "e", "line", "st", "done", 9);
    assert_eq!(writer.push(row.clone()).unwrap(), Accept::Sent { rows: 1 });
    let captured = mock.inserts();
    mock.enqueue_query_body(captured[0].raw_rowbinary.clone());

    let reader = MesReader::open(mock.url(), "af_mes").unwrap();
    let filter = EventFilter {
        work_order_id: Some("WO'1".into()),
        ..EventFilter::default()
    };
    let got = reader.fetch_events(&filter).unwrap();
    assert_eq!(got, vec![row]);

    let queries = mock.queries();
    let sql = queries.last().expect("select");
    assert!(sql.contains("FROM mes_event FINAL"), "{sql}");
    assert!(sql.contains("work_order_id = 'WO\\'1'"), "{sql}");
    assert!(!sql.to_ascii_lowercase().contains("optimize"));
    assert!(!sql.to_ascii_lowercase().contains("rand64"));
}
