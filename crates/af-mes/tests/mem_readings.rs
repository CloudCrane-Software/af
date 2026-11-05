//! J2 消费腿契约测试：解析（真实行形）/查询（latest/window）/事件映射/门化（绿·红·fail-closed）。
//! 不连接真实 ClickHouse——本腿只读文件面，事件映射只到 `MesEvent` 行。

use af_mes::{
    MemDefenseError, MemDefenseGate, MemDefenseVerdict, MemReadings, MemReadingsLog, MesEvent,
    KIND_MEM_AVAILABLE_KB, KIND_SWAP_USED_KB, KIND_TMPFS_USED_B, MEM_DEFENSE_WORK_ORDER,
};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn scratch(name: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "af-mes-mem-readings-{name}-{}-{}",
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

fn write_log(dir: &PathBuf, lines: &str) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join("memory-readings.jsonl");
    fs::write(&path, lines).unwrap();
    path
}

/// keepalive 实际追加的最小行形（2026-11-05 13:47 真实行）。
const KEEPALIVE_ROW: &str = "{\"ts\":\"2026-11-05T13:47:38+0800\",\"epoch\":1793857658,\"src\":\"factory-keepalive\",\"tmpfs_used_b\":4641673216,\"mem_available_kb\":9996540,\"swap_used_kb\":8504104}";

/// mem-readings.sh 手跑全量行形：多 shmem/daemons/tmpfs_total/tmpfs_avail 字段——
/// 必须被忽略，三读数照收。
const FULL_ROW: &str = "{\"ts\":\"2026-11-05T13:40:11+0800\",\"epoch\":1793857211,\"src\":\"mem-readings\",\"tmpfs_total_b\":5368709120,\"tmpfs_used_b\":4641669120,\"tmpfs_avail_b\":727040000,\"mem_available_kb\":10044872,\"swap_total_kb\":16777212,\"swap_used_kb\":8498764,\"swap_free_kb\":8278448,\"shmem_kb\":4182664,\"daemons\":{\"zcode\":\"1234:567890\"}}";

#[test]
fn parses_real_row_shapes_and_ignores_extra_fields() {
    let dir = Scratch(scratch("parse"));
    let path = write_log(&dir.0, &format!("{KEEPALIVE_ROW}\n{FULL_ROW}\n"));
    let log = MemReadingsLog::open(&path).unwrap();
    assert_eq!(log.rows().len(), 2);
    assert!(log.bad_lines().is_empty());

    let keepalive = &log.rows()[0];
    assert_eq!(keepalive.epoch, 1793857658);
    assert_eq!(keepalive.src, "factory-keepalive");
    assert_eq!(keepalive.mem_available_kb, 9_996_540);
    assert_eq!(keepalive.swap_used_kb, 8_504_104);
    assert_eq!(keepalive.tmpfs_used_b, 4_641_673_216);

    let full = &log.rows()[1];
    assert_eq!(full.src, "mem-readings");
    // 全量行多出的字段不炸解析，三读数照收。
    assert_eq!(full.mem_available_kb, 10_044_872);
    assert_eq!(full.swap_used_kb, 8_498_764);
    assert_eq!(full.tmpfs_used_b, 4_641_669_120);
}

#[test]
fn missing_reading_makes_row_bad_and_all_bad_is_no_parseable() {
    let dir = Scratch(scratch("bad"));
    let path = write_log(
        &dir.0,
        // swap_used_kb 缺席——契约三读数缺一即整行作废。
        "{\"ts\":\"t\",\"epoch\":1,\"src\":\"s\",\"tmpfs_used_b\":3,\"mem_available_kb\":2}\n",
    );
    let err = MemReadingsLog::open(&path).unwrap_err();
    match err {
        MemDefenseError::NoParseableRows { bad_lines, .. } => assert_eq!(bad_lines, 1),
        other => panic!("应为 NoParseableRows，得 {other:?}"),
    }
}

#[test]
fn open_records_bad_line_detail_but_keeps_good_rows() {
    let dir = Scratch(scratch("mixed"));
    let path = write_log(&dir.0, &format!("{KEEPALIVE_ROW}\nnot-json\n{{}}\n"));
    let log = MemReadingsLog::open(&path).unwrap();
    assert_eq!(log.rows().len(), 1);
    assert_eq!(log.bad_lines().len(), 2);
    assert!(log.bad_lines()[0].starts_with("L2: "));
}

#[test]
fn open_unreadable_path_is_fail_closed_err() {
    let err = MemReadingsLog::open("/nonexistent/memory-readings.jsonl").unwrap_err();
    assert!(matches!(err, MemDefenseError::Unreadable { .. }));
}

#[test]
fn latest_picks_max_epoch_and_breaks_ties_by_append_order() {
    let dir = Scratch(scratch("latest"));
    let mk = |epoch: i64, avail: u64| {
        format!(
            "{{\"ts\":\"t\",\"epoch\":{epoch},\"src\":\"s\",\"tmpfs_used_b\":1,\"mem_available_kb\":{avail},\"swap_used_kb\":1}}"
        )
    };
    let path = write_log(
        &dir.0,
        &format!(
            "{}\n{}\n{}\n",
            mk(100, 111),
            mk(200, 222),
            mk(200, 333) // 同 epoch 并列：取文件中最后者（append 序）
        ),
    );
    let log = MemReadingsLog::open(&path).unwrap();
    let latest = log.latest().unwrap();
    assert_eq!(latest.epoch, 200);
    assert_eq!(latest.mem_available_kb, 333);
}

#[test]
fn window_returns_tail_in_file_order() {
    let dir = Scratch(scratch("window"));
    let mut text = String::new();
    for epoch in 1..=10 {
        text.push_str(&format!(
            "{{\"ts\":\"t\",\"epoch\":{epoch},\"src\":\"s\",\"tmpfs_used_b\":1,\"mem_available_kb\":{epoch},\"swap_used_kb\":1}}\n"
        ));
    }
    let path = write_log(&dir.0, &text);
    let log = MemReadingsLog::open(&path).unwrap();
    let window = log.window(3);
    assert_eq!(window.len(), 3);
    assert_eq!(window[0].epoch, 8);
    assert_eq!(window[2].epoch, 10);
    // 窗口超宽不炸。
    assert_eq!(log.window(99).len(), 10);
}

fn sample_row() -> MemReadings {
    MemReadings {
        ts: "2026-11-05T13:47:38+0800".into(),
        epoch: 1793857658,
        src: "factory-keepalive".into(),
        tmpfs_used_b: 4_641_673_216,
        mem_available_kb: 9_996_540,
        swap_used_kb: 8_504_104,
    }
}

#[test]
fn to_events_maps_three_readings_into_mes_event_surface() {
    let row = sample_row();
    let events: [MesEvent; 3] = row.to_events("factory-host");

    // 一行的三读数 = 三个事件，映射稳定（确定性，无 rand64）。
    let again = row.to_events("factory-host");
    assert_eq!(events, again);

    let by_kind = |k: &str| events.iter().find(|e| e.kind == k).unwrap();
    for e in &events {
        assert_eq!(e.work_order_id, MEM_DEFENSE_WORK_ORDER);
        assert_eq!(e.occurred_at, 1_793_857_658_000); // epoch 秒 → 毫秒
        assert_eq!(e.line_id, "factory-host");
        assert_eq!(e.station_id, "factory-keepalive");
        assert_eq!(
            e.event_id,
            format!("{}:1793857658:{}", MEM_DEFENSE_WORK_ORDER, e.kind)
        );
    }
    assert_eq!(by_kind(KIND_MEM_AVAILABLE_KB).quantity, 9_996_540);
    assert_eq!(by_kind(KIND_SWAP_USED_KB).quantity, 8_504_104);
    assert_eq!(by_kind(KIND_TMPFS_USED_B).quantity, 4_641_673_216);

    // kind 闭集恰为三读数名。
    let mut kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    kinds.sort();
    assert_eq!(
        kinds,
        vec![KIND_MEM_AVAILABLE_KB, KIND_SWAP_USED_KB, KIND_TMPFS_USED_B]
    );
}

#[test]
fn gate_passes_when_all_three_within_thresholds() {
    let gate = MemDefenseGate {
        min_mem_available_kb: 1_000_000,
        max_swap_used_kb: 12_000_000,
        max_tmpfs_used_b: 5_368_709_120,
    };
    let verdict = gate.judge(&sample_row());
    assert_eq!(verdict, MemDefenseVerdict::Pass);
    assert!(verdict.is_pass());
}

#[test]
fn gate_breaches_each_line_and_reports_detail() {
    let base = sample_row();
    let gate = MemDefenseGate {
        min_mem_available_kb: 9_996_541, // 高于实测 → mem 线破
        max_swap_used_kb: 8_504_103,     // 低于实测 → swap 线破
        max_tmpfs_used_b: 4_641_673_215, // 低于实测 → tmpfs 线破
    };
    let MemDefenseVerdict::Breach(breach) = gate.judge(&base) else {
        panic!("三线全破必须 Breach");
    };
    assert_eq!(breach.len(), 3);
    assert!(breach.iter().any(|b| b.contains("mem_available_kb")));
    assert!(breach.iter().any(|b| b.contains("swap_used_kb")));
    assert!(breach.iter().any(|b| b.contains("tmpfs_used_b")));

    // 单线破也必红（三线联防，不取多数票）。
    let swap_only = MemDefenseGate {
        min_mem_available_kb: 1,
        max_swap_used_kb: base.swap_used_kb - 1,
        max_tmpfs_used_b: u64::MAX,
    };
    assert!(!swap_only.judge(&base).is_pass());
}

#[test]
fn judge_latest_is_fail_closed_on_unreadable_log() {
    let gate = MemDefenseGate {
        min_mem_available_kb: 1,
        max_swap_used_kb: u64::MAX,
        max_tmpfs_used_b: u64::MAX,
    };
    let err = MemReadingsLog::open("/nonexistent/memory-readings.jsonl").unwrap_err();
    // 读数面不可证 ⇒ Err 上抛，调用方不得当绿（fail-closed 两态夹具之一）。
    let _ = gate;
    assert!(matches!(err, MemDefenseError::Unreadable { .. }));
}

#[test]
fn judge_latest_gates_real_parsed_log_end_to_end() {
    let dir = Scratch(scratch("e2e"));
    let path = write_log(&dir.0, &format!("{KEEPALIVE_ROW}\n{FULL_ROW}\n"));
    let log = MemReadingsLog::open(&path).unwrap();
    // 宽门：真实行必绿。
    let loose = MemDefenseGate {
        min_mem_available_kb: 1,
        max_swap_used_kb: u64::MAX,
        max_tmpfs_used_b: u64::MAX,
    };
    assert!(loose.judge_latest(&log).unwrap().is_pass());
    // 紧门（swap 上界压到实测之下）：同一读数面必红——门化在 af-mes 侧生效。
    let tight = MemDefenseGate {
        min_mem_available_kb: 1,
        max_swap_used_kb: 8_504_103,
        max_tmpfs_used_b: u64::MAX,
    };
    assert!(!tight.judge_latest(&log).unwrap().is_pass());
}
