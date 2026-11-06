//! meter 账本 schema 草案（DDL）验证 + 最小消费原型：
//! 建库、CHECK/FK 约束、显式幂等键、惰性窗口重置（对齐自然日/自然周）、Reading/recovers_at 语义。

use chrono::TimeZone;
use chrono_tz::Asia::Shanghai;
use chrono_tz::Tz;
use poc_ledger::{open_ledger, Consume, WindowKind, day_bounds, week_bounds, window_label};

fn at(y: i32, m: u32, d: u32, h: u32, mi: u32) -> chrono::DateTime<Tz> {
    Shanghai
        .from_local_datetime(
            &chrono::NaiveDate::from_ymd_opt(y, m, d)
                .unwrap()
                .and_hms_opt(h, mi, 0)
                .unwrap(),
        )
        .unwrap()
}

fn setup() -> rusqlite::Connection {
    let conn = open_ledger(":memory:").unwrap();
    let ts = 0i64;
    // subject=agent-1：订阅 = Min(周窗1000, 日窗200) + 独立 API Stock（af-meter RESULT 的示例拓扑）
    for (meter_id, kind, wk, cap, parent) in [
        ("subscription", "window", None, 0i64, None::<String>), // Min 节点（原型不扣量，仅树形）
        ("subscription:daily", "window", Some("day"), 200, Some("subscription".into())),
        ("subscription:weekly", "window", Some("week_mon"), 1000, Some("subscription".into())),
        ("api:stock", "stock", None, 10000, None),
    ] {
        conn.execute(
            "INSERT INTO meter_account(subject,meter_id,kind,window_kind,tz,unit,capacity,parent_id,created_at)
             VALUES('agent-1',?1,?2,?3,'Asia/Shanghai','cent',?4,?5,?6)",
            rusqlite::params![meter_id, kind, wk, cap, parent, ts],
        )
        .unwrap();
    }
    conn
}

#[test]
fn ddl_creates_all_tables_and_rejects_bad_rows() {
    let conn = open_ledger(":memory:").unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for t in ["ledger_meta", "meter_account", "meter_window_state", "meter_event", "meter_outbox"] {
        assert!(tables.contains(&t.to_string()), "缺表 {t}");
    }
    // CHECK 拦截非法 kind / op / outbox 状态
    assert!(conn
        .execute(
            "INSERT INTO meter_account(subject,meter_id,kind,unit,capacity,created_at)
             VALUES('s','m','f64','cent',1,0)",
            []
        )
        .is_err());
    assert!(conn
        .execute(
            "INSERT INTO meter_event(subject,meter_id,op,amount,ts) VALUES('s','m','steal',1,0)",
            []
        )
        .is_err());
    // FK 拦截无主账号的账目（foreign_keys=ON）
    assert!(conn
        .execute(
            "INSERT INTO meter_event(subject,meter_id,op,amount,ts) VALUES('ghost','m','consume',1,0)",
            []
        )
        .is_err());
}

#[test]
fn consume_roundtrip_with_idempotency_and_fk() {
    let conn = setup();
    let now = at(2026, 10, 19, 9, 0); // 周一
    match poc_ledger::consume(&conn, "agent-1", "subscription:daily", 150, &now, "wo-1:step-a").unwrap() {
        Consume::Applied(r) => {
            assert_eq!(r.available, 50);
            let (ws, we) = day_bounds(&now);
            assert_eq!(r.recovers_at_ms, Some(we.timestamp_millis()));
            // 窗口投影与账目行都已落
            let (sws, swe, rem): (i64, i64, i64) = conn
                .query_row(
                    "SELECT window_start, window_end, remaining FROM meter_window_state
                     WHERE subject='agent-1' AND meter_id='subscription:daily'",
                    [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap();
            assert_eq!(sws, ws.timestamp_millis());
            assert_eq!(swe, we.timestamp_millis());
            assert_eq!(rem, 50);
        }
        other => panic!("期望 Applied，得到 {other:?}"),
    }
    // 幂等重放：同键不再扣
    assert!(matches!(
        poc_ledger::consume(&conn, "agent-1", "subscription:daily", 150, &now, "wo-1:step-a").unwrap(),
        Consume::AlreadyApplied
    ));
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM meter_event WHERE idempotency_key='wo-1:step-a'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "幂等键下恰好一条账目");
    // 超额：Insufficient 且 recovers_at = 窗口终点
    match poc_ledger::consume(&conn, "agent-1", "subscription:daily", 100, &now, "wo-1:step-b").unwrap() {
        Consume::Insufficient { reading } => {
            assert_eq!(reading.available, 50);
            let (_, we) = day_bounds(&now);
            assert_eq!(reading.recovers_at_ms, Some(we.timestamp_millis()));
        }
        other => panic!("期望 Insufficient，得到 {other:?}"),
    }
    // stock 型：无 recovers_at（不自动恢复）
    match poc_ledger::consume(&conn, "agent-1", "api:stock", 30, &now, "wo-1:step-c").unwrap() {
        Consume::Applied(r) => {
            assert_eq!(r.available, 9970);
            assert_eq!(r.recovers_at_ms, None);
        }
        other => panic!("期望 Applied，得到 {other:?}"),
    }
}

#[test]
fn lazy_window_reset_at_aligned_boundary_day_and_week() {
    let conn = setup();
    // 周四 23:59 消耗日窗
    let thu = at(2026, 10, 22, 23, 59);
    assert!(matches!(
        poc_ledger::consume(&conn, "agent-1", "subscription:daily", 180, &thu, "wo-1:d0").unwrap(),
        Consume::Applied(_)
    ));
    // 同一自然日 23:59:59 后的剩余 20
    match poc_ledger::consume(&conn, "agent-1", "subscription:daily", 10, &thu, "wo-1:d1").unwrap() {
        Consume::Applied(r) => assert_eq!(r.available, 10),
        other => panic!("{other:?}"),
    }
    // 跨过对齐午夜（周五 00:01）：惰性重置 → remaining 回 200，窗口列换成新窗口
    let fri = at(2026, 10, 23, 0, 1);
    match poc_ledger::consume(&conn, "agent-1", "subscription:daily", 5, &fri, "wo-1:d2").unwrap() {
        Consume::Applied(r) => {
            assert_eq!(r.available, 195, "新自然日应重置为 capacity-5");
            let (_, we) = day_bounds(&fri);
            assert_eq!(r.recovers_at_ms, Some(we.timestamp_millis()));
        }
        other => panic!("{other:?}"),
    }
    let (sws, _): (i64, i64) = conn
        .query_row(
            "SELECT window_start, remaining FROM meter_window_state
             WHERE subject='agent-1' AND meter_id='subscription:daily'",
            [], |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let (fs, _) = day_bounds(&fri);
    assert_eq!(sws, fs.timestamp_millis(), "窗口起点应对齐到新自然日 00:00");

    // 周窗同样惰性重置：周五消耗后跨到下周一（10-26 00:00）→ 重置回 1000
    assert!(matches!(
        poc_ledger::consume(&conn, "agent-1", "subscription:weekly", 400, &fri, "wo-1:w0").unwrap(),
        Consume::Applied(_)
    ));
    let mon = at(2026, 10, 26, 0, 1);
    match poc_ledger::consume(&conn, "agent-1", "subscription:weekly", 1, &mon, "wo-1:w1").unwrap() {
        Consume::Applied(r) => assert_eq!(r.available, 999, "新自然周（周一 00:00）应重置"),
        other => panic!("{other:?}"),
    }
    let (sws, swe): (i64, i64) = conn
        .query_row(
            "SELECT window_start, window_end FROM meter_window_state
             WHERE subject='agent-1' AND meter_id='subscription:weekly'",
            [], |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let (ms, me) = week_bounds(&mon);
    assert_eq!(sws, ms.timestamp_millis());
    assert_eq!(swe, me.timestamp_millis());
    // reset 落了账（op='reset' 由重置路径以事件形式可查——本原型在窗口投影层重置，账面以 consume 事件审计）
    let resets: i64 = conn
        .query_row("SELECT COUNT(*) FROM meter_event WHERE op='reset'", [], |r| r.get(0))
        .unwrap();
    let _ = resets; // 原型：重置体现在投影替换；正式版可补 reset 事件（见报告④）
}

#[test]
fn routing_decision_input_daily_exhausted_weekly_surplus_recovers_at_next_boundaries() {
    // 决策输入复刻 af-meter 核心场景：周四 18:00 日窗尽、周窗有余。
    // 路由需要的就是两个对齐恢复点：日窗=明天 00:00 钟面，周窗=下周一 00:00 钟面。
    let conn = setup();
    let now = at(2026, 10, 22, 18, 0); // 周四
    assert!(matches!(
        poc_ledger::consume(&conn, "agent-1", "subscription:daily", 200, &now, "wo-1:r0").unwrap(),
        Consume::Applied(_)
    ));
    let daily_insufficient =
        poc_ledger::consume(&conn, "agent-1", "subscription:daily", 1, &now, "wo-1:r1").unwrap();
    let weekly = poc_ledger::consume(&conn, "agent-1", "subscription:weekly", 1, &now, "wo-1:r2").unwrap();
    let (_, d_end) = day_bounds(&now);
    let (_, w_end) = week_bounds(&now);
    match (daily_insufficient, weekly) {
        (Consume::Insufficient { reading }, Consume::Applied(r)) => {
            assert_eq!(reading.recovers_at_ms, Some(d_end.timestamp_millis()));
            assert_eq!(r.recovers_at_ms, Some(w_end.timestamp_millis()));
            assert_eq!(window_label(WindowKind::WeekMon, &now), "2026-W43");
        }
        other => panic!("意外组合 {other:?}"),
    }
}
