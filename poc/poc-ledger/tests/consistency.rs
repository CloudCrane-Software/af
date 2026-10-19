//! 跨库一致性边界与补偿写法（分库形态的核心代价）：
//! - 证明边界：meter.db 与 gateway.db 两个文件的两次独立落库之间没有原子性——
//!   中间"崩溃"必然留下分裂状态（一边有账、一边无回执）。
//! - 补偿写法：outbox（本库事务内"业务行+信箱行"原子落库 → 对端幂等落回执 → 标 done），
//!   三个崩溃点逐一注入，恢复函数收敛到"对端恰好一条回执、真实执行恰好一次"。

use std::cell::Cell;

use poc_ledger::{open_ledger, outbox_recover, GATEWAY_RECEIPT_SQL};

struct TwoDbs {
    dir: String,
}

impl TwoDbs {
    fn new(tag: &str) -> Self {
        let dir = format!("{}/outbox_{tag}", env!("CARGO_TARGET_TMPDIR"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TwoDbs { dir }
    }
    fn meter(&self) -> rusqlite::Connection {
        let c = open_ledger(&format!("{}/meter.db", self.dir)).unwrap();
        // FK 前置：被计量的账号先注册（真实系统里这是开户流程）
        c.execute(
            "INSERT OR IGNORE INTO meter_account(subject,meter_id,kind,window_kind,unit,capacity,created_at)
             VALUES('agent-1','subscription:daily','window','day','cent',200,0)",
            [],
        )
        .unwrap();
        c
    }
    fn gateway(&self) -> rusqlite::Connection {
        let c = open_ledger(&format!("{}/gateway.db", self.dir)).unwrap();
        c.execute_batch(GATEWAY_RECEIPT_SQL).unwrap();
        c
    }
}

fn gateway_receipt_count(g: &rusqlite::Connection) -> i64 {
    g.query_row("SELECT COUNT(*) FROM effect_receipt", [], |r| r.get(0)).unwrap()
}

/// 发起库事务：业务账目 + outbox 信箱行，同一事务原子提交。
fn prepare_with_outbox(m: &rusqlite::Connection, amount: i64, idem: &str) -> rusqlite::Result<()> {
    m.execute_batch("BEGIN IMMEDIATE")?;
    let r = (|| -> rusqlite::Result<()> {
        m.execute(
            "INSERT INTO meter_event(subject,meter_id,op,amount,window_start,available_after,idempotency_key,ts)
             VALUES('agent-1','subscription:daily','consume',?1,NULL,NULL,?2,0)",
            rusqlite::params![amount, idem],
        )?;
        m.execute(
            "INSERT INTO meter_outbox(id,payload,created_at) VALUES(?1,?2,0)",
            rusqlite::params![idem, format!(r#"{{"db":"gateway","intent_id":"{idem}","amount":{amount}}}"#)],
        )?;
        Ok(())
    })();
    match r {
        Ok(()) => m.execute_batch("COMMIT")?,
        Err(e) => {
            let _ = m.execute_batch("ROLLBACK");
            return Err(e);
        }
    }
    Ok(())
}

/// 对端幂等应用：同 intent_id 主键重放不产生第二条（"真实执行恰好一次"的落点）。
fn apply_to_gateway(g: &rusqlite::Connection, id: &str, payload: &str, counter: &Cell<i64>) -> rusqlite::Result<()> {
    let n = g.execute(
        "INSERT INTO effect_receipt(intent_id,payload,applied_at)
         VALUES(?1,?2,0) ON CONFLICT(intent_id) DO NOTHING",
        rusqlite::params![id, payload],
    )?;
    counter.set(counter.get() + n as i64); // n=1 才是真实执行；幂等重放 n=0
    Ok(())
}

#[test]
fn boundary_two_files_have_no_atomicity_across_autocommit_writes() {
    let env = TwoDbs::new("boundary");
    let m = env.meter();
    let g = env.gateway();
    // 第一步：meter.db 落账（已提交）
    prepare_with_outbox(&m, 5, "wo-crash:step1").unwrap();
    // ——此处注入崩溃：进程死在第二步（gateway 落回执）之前——
    drop(m);
    drop(g);
    // 重启后观测：meter 有账、outbox pending、gateway 无回执 = 分裂态（这就是"跨文件无事务"的边界）
    let m = env.meter();
    let g = env.gateway();
    let events: i64 =
        m.query_row("SELECT COUNT(*) FROM meter_event WHERE idempotency_key='wo-crash:step1'", [], |r| r.get(0)).unwrap();
    let pending: i64 =
        m.query_row("SELECT COUNT(*) FROM meter_outbox WHERE state='pending'", [], |r| r.get(0)).unwrap();
    assert_eq!(events, 1, "meter 侧已提交");
    assert_eq!(pending, 1, "信箱行与账目同事务落库（补偿的支点）");
    assert_eq!(gateway_receipt_count(&g), 0, "gateway 侧一无所有：跨文件无原子性");
}

#[test]
fn crash_point_a_recover_replays_pending_outbox_exactly_once() {
    // 崩溃点 A：prepare 之后、gateway 写入之前死掉
    let env = TwoDbs::new("a");
    let m = env.meter();
    let g = env.gateway();
    prepare_with_outbox(&m, 5, "wo-a:step1").unwrap();
    assert_eq!(gateway_receipt_count(&g), 0);
    let counter = Cell::new(0i64);
    let mut apply = |id: &str, payload: &str| apply_to_gateway(&g, id, payload, &counter);
    let recovered = outbox_recover(&m, &mut apply).unwrap();
    assert_eq!(recovered, 1);
    assert_eq!(gateway_receipt_count(&g), 1);
    assert_eq!(counter.get(), 1, "真实执行恰好一次");
    let pending: i64 = m.query_row("SELECT COUNT(*) FROM meter_outbox WHERE state='pending'", [], |r| r.get(0)).unwrap();
    assert_eq!(pending, 0);
}

#[test]
fn crash_point_b_replay_after_unmarked_gateway_write_is_idempotent() {
    // 崩溃点 B：gateway 已写入、outbox 尚未标 done 时死掉 → 恢复会重放同一 intent
    let env = TwoDbs::new("b");
    let m = env.meter();
    let g = env.gateway();
    prepare_with_outbox(&m, 5, "wo-b:step1").unwrap();
    let counter = Cell::new(0i64);
    apply_to_gateway(&g, "wo-b:step1", "{\"db\":\"gateway\"}", &counter).unwrap();
    assert_eq!(counter.get(), 1);
    // ——注入崩溃：done 未标，重启——
    let counter2 = Cell::new(0i64);
    let mut apply = |id: &str, payload: &str| apply_to_gateway(&g, id, payload, &counter2);
    let recovered = outbox_recover(&m, &mut apply).unwrap();
    assert_eq!(recovered, 1, "pending 行被恢复流程收敛");
    assert_eq!(counter2.get(), 0, "重放被对端主键幂等吞掉：不再真实执行");
    assert_eq!(gateway_receipt_count(&g), 1, "回执恒一条，不因重放翻倍");
    let done: i64 = m.query_row("SELECT COUNT(*) FROM meter_outbox WHERE state='done'", [], |r| r.get(0)).unwrap();
    assert_eq!(done, 1);
}

#[test]
fn all_crash_points_converge_to_exactly_one_receipt() {
    // 收敛性：三个崩溃点注入后各自进入恢复循环，终态恒一
    for tag in ["p1", "p2", "p3"] {
        let env = TwoDbs::new(tag);
        let m = env.meter();
        let g = env.gateway();
        let idem = format!("wo-{tag}:step1");
        prepare_with_outbox(&m, 5, &idem).unwrap();
        let counter = Cell::new(0i64);
        match tag {
            // p1：prepare 后即崩（最常见）
            "p1" => {}
            // p2：gateway 已写、done 未标即崩
            "p2" => apply_to_gateway(&g, &idem, "{\"db\":\"gateway\"}", &counter).unwrap(),
            // p3：完整跑完恢复流程后"二次重启"（恢复幂等性）
            "p3" => {
                let mut apply =
                    |id: &str, payload: &str| apply_to_gateway(&g, id, payload, &counter);
                assert_eq!(outbox_recover(&m, &mut apply).unwrap(), 1);
            }
            _ => unreachable!(),
        }
        // 恢复循环（重启后 reconcile，多跑几遍模拟反复重试）
        for _ in 0..3 {
            let mut apply = |id: &str, payload: &str| apply_to_gateway(&g, id, payload, &counter);
            let _ = outbox_recover(&m, &mut apply).unwrap();
        }
        let events: i64 = m.query_row("SELECT COUNT(*) FROM meter_event", [], |r| r.get(0)).unwrap();
        let receipts = gateway_receipt_count(&g);
        let done: i64 = m.query_row("SELECT COUNT(*) FROM meter_outbox WHERE state='done'", [], |r| r.get(0)).unwrap();
        assert_eq!(events, 1, "tag={tag}: 账目恰一条");
        assert_eq!(receipts, 1, "tag={tag}: 回执恰一条");
        assert_eq!(done, 1, "tag={tag}: 信箱收敛为 done");
    }
}
