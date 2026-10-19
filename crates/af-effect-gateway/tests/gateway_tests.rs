//! af-effect-gateway PoC 验收测试。
//!
//! 对应任务四条验收用例，另加窗口门控、外部失败可重试，
//! 墙钟回拨不影响幂等窗口，以及订单状态流转驱动意图生命周期。
//! 时序全部由 [`ManualClock`] 注入。

use af_effect_gateway::*;
use af_workorder::{
    BudgetVoucher, GraphVersion, LineBinding, OrderState, Priority, Sla, WorkOrder,
};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

const WALL0: i64 = 1_700_000_000_000;

fn manual_clock() -> ManualClock {
    ManualClock::new(0, WALL0)
}

fn temp_db(tag: &str) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let p = std::env::temp_dir().join(format!("af_eg_poc_{}_{}_{}.db", tag, std::process::id(), n));
    let _ = std::fs::remove_file(&p);
    p
}

/// 模拟外部调用的计数闭包：每被调一次计数 +1，返回带序号的伪回执。
fn counting_effect(counter: Arc<AtomicUsize>) -> impl Fn(&Value) -> Result<Value> {
    move |_payload| {
        let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(json!({ "txn_id": format!("T-{}", n), "provider": "mock-pay" }))
    }
}

fn call_count(counter: &AtomicUsize) -> usize {
    counter.load(Ordering::SeqCst)
}

fn key(workorder: &str, step: &str, seq: i64) -> IntentKey {
    IntentKey {
        workorder_id: workorder.to_string(),
        step_path: step.to_string(),
        seq,
    }
}

fn prepared_order(id: &str) -> WorkOrder {
    let mut wo = WorkOrder::new(
        id,
        GraphVersion::new("g-w3b"),
        Sla::new(None, Priority::Normal),
    );
    wo.bind_line(LineBinding {
        line_id: "line-A".into(),
        line_version: "lv-1".into(),
    })
    .unwrap();
    wo.attach_budget_voucher(BudgetVoucher {
        voucher_id: "bv-1".into(),
        amount: 1,
        signed: true,
    })
    .unwrap();
    wo
}

/// 既有幂等、fencing、撤回窗口用例都走已开工的订单，执行门控才放行。
fn in_progress_order(id: &str) -> WorkOrder {
    let mut wo = prepared_order(id);
    wo.release().unwrap();
    wo.start().unwrap();
    assert_eq!(wo.state(), OrderState::InProgress);
    wo
}

// ===========================================================================
// 用例①：崩溃恢复后同键同载荷重放 → 返回已存结果，不重发
// ===========================================================================
#[test]
fn t1_crash_recovery_same_key_same_payload_replays_stored_result() {
    let db = temp_db("t1");
    let counter = Arc::new(AtomicUsize::new(0));
    let k = key("WO-1001", "pay_vendor", 1);
    let payload = json!({ "amount": 12800, "to_account": "ACCT-A" });
    let clock = manual_clock();
    let wo = in_progress_order("WO-1001");

    // —— 第一次生命周期：declare → 窗口走到截止点 → execute（真实外呼一次）→ “崩溃” ——
    {
        let gw = Gateway::open_with_clock(&db, Duration::from_millis(50), clock.clone()).unwrap();
        assert_eq!(gw.current_epoch().unwrap(), 0, "新库纪元从 0 开始");
        let s = gw.attach().unwrap();
        let out = gw.declare_intent(&s, &k, &payload, &wo).unwrap();
        assert!(matches!(out, DeclareOutcome::Created { .. }));

        clock.advance(Duration::from_millis(50)); // 单调钟到达截止点，窗口关闭
        let exec = gw
            .execute(&s, &k, &wo, counting_effect(counter.clone()))
            .unwrap();
        assert!(!exec.is_replay(), "首次执行应是真实外呼");
        let r = exec.into_inner();
        assert_eq!(r["txn_id"], "T-1");
        assert_eq!(call_count(&counter), 1);
        assert!(matches!(
            gw.snapshot_intent(&k).unwrap().map(|s| s.state),
            Some(IntentState::Executed)
        ));
    } // drop == 崩溃：进程带着已落库的回执消失

    // —— 恢复：重新 open，resume 使纪元 +1，同键同载荷重放 ——
    {
        let gw = Gateway::open_with_clock(&db, Duration::from_millis(50), manual_clock()).unwrap();
        assert_eq!(gw.current_epoch().unwrap(), 0, "恢复后先看到崩溃前纪元");
        let s2 = gw.resume().unwrap();
        assert_eq!(s2.epoch, 1, "resume 必须使纪元 +1");

        // 重放 declare：同键同载荷 → 幂等接受
        let re = gw.declare_intent(&s2, &k, &payload, &wo).unwrap();
        assert!(matches!(re, DeclareOutcome::AlreadyDeclared { .. }));

        // 重放 execute：外部调用计数不变，返回已存回执
        let exec = gw
            .execute(&s2, &k, &wo, counting_effect(counter.clone()))
            .unwrap();
        assert!(exec.is_replay(), "有已存回执时必须重放而不是重发");
        match exec {
            ExecuteOutcome::Replayed {
                result,
                original_executed_at_ms,
                original_epoch,
            } => {
                assert_eq!(result["txn_id"], "T-1", "返回的是崩溃前那次的结果");
                assert_eq!(original_epoch, 0, "原回执是旧纪元执行落库的");
                assert!(original_executed_at_ms > 0);
            }
            ExecuteOutcome::RanFresh { .. } => panic!("不允许重发外部操作"),
        }
        assert_eq!(call_count(&counter), 1, "外部调用恰好发生一次");
    }

    let _ = std::fs::remove_file(&db);
}

// ===========================================================================
// 用例②：同键异载荷 → 拒绝并要求升级
// ===========================================================================
#[test]
fn t2_same_key_different_payload_rejected_requires_escalation() {
    let gw = Gateway::open_in_memory(Duration::from_millis(50)).unwrap();
    let s = gw.attach().unwrap();
    let k = key("WO-2002", "issue_cert", 3);
    let wo = in_progress_order("WO-2002");
    let p1 = json!({ "holder": "张三", "cert": "C-001" });
    let p2 = json!({ "holder": "李四", "cert": "C-001" }); // 同键，载荷不同

    gw.declare_intent(&s, &k, &p1, &wo).unwrap();

    let err = gw.declare_intent(&s, &k, &p2, &wo).unwrap_err();
    match &err {
        GatewayError::PayloadConflict {
            key,
            stored_sha256,
            incoming_sha256,
        } => {
            assert_eq!(key, &k.to_string());
            assert_ne!(stored_sha256, incoming_sha256);
            assert_eq!(stored_sha256.len(), 64, "sha256 十六进制长度");
        }
        other => panic!("期望 PayloadConflict，实际 {:?}", other),
    }
    // 错误信息必须点名“升级”，网关不替调用方二选一
    assert!(
        err.to_string().contains("升级"),
        "错误信息要写明需人工升级: {}",
        err
    );

    // 原意图不受影响：状态仍是 declared，指纹仍是 p1 的
    let snap = gw.snapshot_intent(&k).unwrap().unwrap();
    assert_eq!(snap.state, IntentState::Declared);
    assert_eq!(
        snap.payload_sha256,
        fingerprint_of(&p1),
        "存量意图的载荷指纹不能被覆盖"
    );

    // 同键同载荷仍幂等通过（对照）
    let ok = gw.declare_intent(&s, &k, &p1, &wo).unwrap();
    assert!(matches!(ok, DeclareOutcome::AlreadyDeclared { .. }));
}

// ===========================================================================
// 用例③：窗口内撤回 → 不发送
// ===========================================================================
#[test]
fn t3_withdraw_within_window_means_never_sent() {
    let counter = Arc::new(AtomicUsize::new(0));
    let clock = manual_clock();
    let gw = Gateway::open_in_memory_with_clock(Duration::from_millis(300), clock.clone()).unwrap();
    let s = gw.attach().unwrap();
    let wo = in_progress_order("WO-3003");
    let k = key("WO-3003", "send_sms", 1);
    let payload = json!({ "phone": "138xxxx", "text": "验证码" });

    let out = gw.declare_intent(&s, &k, &payload, &wo).unwrap();
    assert!(
        matches!(out, DeclareOutcome::Created { withdraw_deadline_ms } if withdraw_deadline_ms > 0)
    );

    // 窗口内撤回
    assert_eq!(
        gw.withdraw_intent(&s, &k).unwrap(),
        WithdrawOutcome::Withdrawn
    );
    assert_eq!(
        gw.withdraw_intent(&s, &k).unwrap(),
        WithdrawOutcome::AlreadyWithdrawn,
        "重复撤回幂等"
    );

    // execute 被拒，外部调用一次都没发生
    let err = gw
        .execute(&s, &k, &wo, counting_effect(counter.clone()))
        .unwrap_err();
    assert!(
        matches!(err, GatewayError::Withdrawn { .. }),
        "实际 {:?}",
        err
    );
    assert_eq!(call_count(&counter), 0, "撤回后绝不发送");

    // 对照组：另一个意图窗口已关才去撤回 → 撤回失败（窗口保护有效）
    let k2 = key("WO-3003", "send_sms", 2);
    gw.declare_intent(&s, &k2, &payload, &wo).unwrap();
    clock.advance(Duration::from_millis(300)); // 单调钟越过 300ms 窗口
    let err = gw.withdraw_intent(&s, &k2).unwrap_err();
    assert!(
        matches!(err, GatewayError::WithdrawWindowClosed { .. }),
        "窗口关闭后撤回必须失败: {:?}",
        err
    );
}

// ===========================================================================
// 用例④：旧纪元请求 → 拒绝
// ===========================================================================
#[test]
fn t4_stale_epoch_requests_rejected() {
    let counter = Arc::new(AtomicUsize::new(0));
    let db = temp_db("t4");
    let clock = manual_clock();
    let gw = Gateway::open_with_clock(&db, Duration::from_millis(30), clock.clone()).unwrap();
    let s_old = gw.attach().unwrap(); // epoch 0
    assert_eq!(s_old.epoch, 0);
    let wo = in_progress_order("WO-4004");
    let k = key("WO-4004", "dispatch_truck", 1);
    gw.declare_intent(&s_old, &k, &json!({"truck": "B-01"}), &wo)
        .unwrap();

    // 崩溃恢复：resume → 纪元 1
    let s_new = gw.resume().unwrap();
    assert_eq!(s_new.epoch, 1);

    // 旧纪元 session 发起的请求：declare / withdraw / execute 全拒
    for (op, res) in [
        (
            "declare",
            gw.declare_intent(&s_old, &k, &json!({"truck": "B-01"}), &wo)
                .map(|_| ()),
        ),
        ("withdraw", gw.withdraw_intent(&s_old, &k).map(|_| ())),
        (
            "execute",
            gw.execute(&s_old, &k, &wo, counting_effect(counter.clone()))
                .map(|_| ()),
        ),
        (
            "record_receipt",
            gw.record_receipt(&s_old, &k, &json!({"ok": true}))
                .map(|_| ()),
        ),
    ] {
        let err = res.unwrap_err();
        match &err {
            GatewayError::StaleEpoch {
                request_epoch,
                current_epoch,
            } => {
                assert_eq!(*request_epoch, 0);
                assert_eq!(*current_epoch, 1);
            }
            other => panic!("{} 期望 StaleEpoch，实际 {:?}", op, other),
        }
    }
    assert_eq!(call_count(&counter), 0, "旧纪元请求绝不触达外部调用");

    // 再次 resume：纪元继续 +1，上一个纪元的 session 也变旧
    let s_newer = gw.resume().unwrap();
    assert_eq!(s_newer.epoch, 2);
    let err = gw.withdraw_intent(&s_new, &k).unwrap_err();
    assert!(matches!(
        err,
        GatewayError::StaleEpoch {
            request_epoch: 1,
            current_epoch: 2
        }
    ));

    // 新纪元 session 可继续操作旧纪元声明的意图（崩溃恢复重放的路必须通）
    clock.advance(Duration::from_millis(30)); // 单调钟越过 30ms 窗口
    let exec = gw
        .execute(&s_newer, &k, &wo, counting_effect(counter.clone()))
        .unwrap();
    assert!(!exec.is_replay());
    assert_eq!(call_count(&counter), 1);

    let _ = std::fs::remove_file(&db);
}

// ===========================================================================
// 补充⑤：窗口本身是发送闸门——未关不发送，关了才发
// ===========================================================================
#[test]
fn t5_execute_before_window_close_is_refused_then_allowed() {
    let counter = Arc::new(AtomicUsize::new(0));
    let clock = manual_clock();
    let gw = Gateway::open_in_memory_with_clock(Duration::from_millis(120), clock.clone()).unwrap();
    let s = gw.attach().unwrap();
    let wo = in_progress_order("WO-5005");
    let k = key("WO-5005", "charge_fee", 1);
    gw.declare_intent(&s, &k, &json!({"fee": 99}), &wo).unwrap();

    let err = gw
        .execute(&s, &k, &wo, counting_effect(counter.clone()))
        .unwrap_err();
    match &err {
        GatewayError::WindowStillOpen {
            deadline_ms,
            now_ms,
        } => {
            assert!(now_ms < deadline_ms, "窗口未关时单调 now 必须早于 deadline");
            assert!(*deadline_ms > 0);
        }
        other => panic!("期望 WindowStillOpen，实际 {:?}", other),
    }
    assert_eq!(call_count(&counter), 0, "窗口未关不发送");

    clock.advance(Duration::from_millis(120)); // 单调钟到达截止点
    let exec = gw
        .execute(&s, &k, &wo, counting_effect(counter.clone()))
        .unwrap();
    assert!(!exec.is_replay());
    assert_eq!(call_count(&counter), 1, "窗口关闭后正常发送一次");
}

// ===========================================================================
// 补充⑥：外部调用失败 → 意图保持 declared，可重试且不重复成功发送
// ===========================================================================
#[test]
fn t6_external_failure_keeps_declared_and_retry_succeeds() {
    let counter = Arc::new(AtomicUsize::new(0));
    let clock = manual_clock();
    let gw = Gateway::open_in_memory_with_clock(Duration::from_millis(20), clock.clone()).unwrap();
    let s = gw.attach().unwrap();
    let wo = in_progress_order("WO-6006");
    let k = key("WO-6006", "refund", 1);
    gw.declare_intent(&s, &k, &json!({"order": "D-9"}), &wo)
        .unwrap();
    clock.advance(Duration::from_millis(20));

    let c1 = counter.clone();
    let fail = move |_p: &Value| -> Result<Value> {
        c1.fetch_add(1, Ordering::SeqCst);
        external_err("mock 网关超时")
    };
    let err = gw.execute(&s, &k, &wo, fail).unwrap_err();
    assert!(matches!(err, GatewayError::ExternalCall(_)));
    assert_eq!(
        gw.snapshot_intent(&k).unwrap().unwrap().state,
        IntentState::Declared,
        "失败后必须保持 declared 才能重试"
    );

    // 重试成功：真实外呼第二次，回执落库
    let c2 = counter.clone();
    let ok_call = move |_p: &Value| -> Result<Value> {
        let n = c2.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(json!({ "txn_id": format!("T-{}", n) }))
    };
    let exec = gw.execute(&s, &k, &wo, ok_call).unwrap();
    assert!(!exec.is_replay());
    assert_eq!(exec.into_inner()["txn_id"], "T-2");
    assert_eq!(
        gw.snapshot_intent(&k).unwrap().unwrap().state,
        IntentState::Executed
    );

    // 再 execute：重放，不重发
    let again = gw
        .execute(&s, &k, &wo, counting_effect(counter.clone()))
        .unwrap();
    assert!(again.is_replay());
    assert_eq!(counter.load(Ordering::SeqCst), 2);
}

// ===========================================================================
// 补充⑦：系统时钟回拨不影响幂等窗口判定
// ===========================================================================
#[test]
fn t7_wall_clock_rollback_does_not_affect_idempotency_window() {
    // 幂等窗口（撤回窗口）只由单调钟判定。
    // 墙钟前跳不得提前关窗；墙钟回拨不得延长窗口，也不得在重启后把窗口重新打开。
    let db = temp_db("t7");
    let clock = manual_clock();
    let window = Duration::from_millis(100);
    let gw = Gateway::open_with_clock(&db, window, clock.clone()).unwrap();
    let s = gw.attach().unwrap();
    let wo = in_progress_order("WO-7007");
    let counter = Arc::new(AtomicUsize::new(0));

    // A. 墙钟前跳 1 小时，单调钟不动：窗口仍开。
    let k_open = key("WO-7007", "hold", 1);
    gw.declare_intent(&s, &k_open, &json!({"n": 1}), &wo)
        .unwrap();
    clock.shift_wall_ms(3_600_000);
    let err = gw
        .execute(&s, &k_open, &wo, counting_effect(counter.clone()))
        .unwrap_err();
    match err {
        GatewayError::WindowStillOpen {
            deadline_ms,
            now_ms,
        } => {
            assert_eq!(now_ms, 0, "判定读数必须是单调逻辑毫秒，不是前跳后的墙钟");
            assert_eq!(deadline_ms, 100);
        }
        other => panic!("期望 WindowStillOpen，实际 {:?}", other),
    }
    assert_eq!(call_count(&counter), 0, "墙钟前跳不得提前发送");
    assert_eq!(
        gw.withdraw_intent(&s, &k_open).unwrap(),
        WithdrawOutcome::Withdrawn,
        "单调窗口未到点时仍可撤回"
    );

    // B. 单调钟走到截止点，同时墙钟回拨到声明之前：窗口必须已关。
    let k = key("WO-7007", "pay", 1);
    let declared = gw.declare_intent(&s, &k, &json!({"n": 2}), &wo).unwrap();
    assert!(matches!(
        declared,
        DeclareOutcome::Created {
            withdraw_deadline_ms: 100
        }
    ));
    clock.advance(Duration::from_millis(60));
    clock.shift_wall_ms(-7_200_000);
    let err = gw
        .execute(&s, &k, &wo, counting_effect(counter.clone()))
        .unwrap_err();
    match err {
        GatewayError::WindowStillOpen {
            deadline_ms,
            now_ms,
        } => {
            assert_eq!(now_ms, 60, "回拨后的墙钟不得把 now 拉回窗口起点");
            assert_eq!(deadline_ms, 100);
        }
        other => panic!("单调未到点不得放行 {:?}", other),
    }
    assert_eq!(call_count(&counter), 0);

    clock.advance(Duration::from_millis(40)); // 逻辑毫秒 100，窗口关闭
    let err = gw.withdraw_intent(&s, &k).unwrap_err();
    match err {
        GatewayError::WithdrawWindowClosed {
            deadline_ms, state, ..
        } => {
            assert_eq!(state, "declared");
            assert_eq!(deadline_ms, 100, "截止点是单调毫秒，不是 Unix 墙钟");
        }
        other => panic!("窗口关闭后撤回必须失败: {:?}", other),
    }

    // C. 进程重启。新单调钟从 0 起，墙钟停在 Unix 纪元（相对声明时的墙钟是回拨）。
    //    已提交的单调高水位已到 deadline，不得借墙钟把窗口重新打开。
    drop(gw);
    let restarted = ManualClock::new(0, 0);
    let gw = Gateway::open_with_clock(&db, window, restarted.clone()).unwrap();
    let s2 = gw.attach().unwrap();
    let exec = gw
        .execute(&s2, &k, &wo, counting_effect(counter.clone()))
        .unwrap();
    assert!(
        !exec.is_replay(),
        "窗口已按单调钟关闭，重启后应真实执行一次"
    );
    assert_eq!(call_count(&counter), 1);

    restarted.shift_wall_ms(-1_000_000);
    let again = gw
        .execute(&s2, &k, &wo, counting_effect(counter.clone()))
        .unwrap();
    assert!(again.is_replay(), "回执已在，墙钟再回拨也不得重发");
    assert_eq!(call_count(&counter), 1, "外部调用恰好一次");

    let _ = std::fs::remove_file(&db);
}

// ===========================================================================
// 补充⑧：订单状态流转驱动副作用意图生命周期
// ===========================================================================
#[test]
fn t8_order_state_transitions_drive_intent_lifecycle() {
    let counter = Arc::new(AtomicUsize::new(0));
    let clock = manual_clock();
    let gw = Gateway::open_in_memory_with_clock(Duration::from_millis(50), clock.clone()).unwrap();
    let s = gw.attach().unwrap();
    let mut wo = prepared_order("WO-8008");
    let k = key("WO-8008", "ship", 1);
    let payload = json!({ "sku": "A" });
    let declare_ok = &[OrderState::Released, OrderState::InProgress];
    let execute_ok = &[OrderState::InProgress];

    // Created：不能声明。拒绝要带上当前态和允许态，并且不落库。
    let err = gw.declare_intent(&s, &k, &payload, &wo).unwrap_err();
    assert_order_state_rejected(
        &err,
        IntentOrderOp::Declare,
        "WO-8008",
        OrderState::Created,
        declare_ok,
    );
    assert!(gw.snapshot_intent(&k).unwrap().is_none());

    // 键上的 workorder_id 必须就是这张订单。
    let other = in_progress_order("WO-OTHER");
    let err = gw.declare_intent(&s, &k, &payload, &other).unwrap_err();
    match err {
        GatewayError::WorkOrderMismatch {
            intent_workorder_id,
            presented_order_id,
        } => {
            assert_eq!(intent_workorder_id, "WO-8008");
            assert_eq!(presented_order_id, "WO-OTHER");
        }
        other => panic!("期望 WorkOrderMismatch，实际 {other:?}"),
    }

    wo.release().unwrap();
    assert_eq!(wo.state(), OrderState::Released);

    // Released：可以声明，快照记下订单 id 和当时的 Released。
    let created = gw.declare_intent(&s, &k, &payload, &wo).unwrap();
    assert!(matches!(
        created,
        DeclareOutcome::Created {
            withdraw_deadline_ms: 50
        }
    ));
    let snap = gw.snapshot_intent(&k).unwrap().unwrap();
    assert_eq!(snap.order_id, "WO-8008");
    assert_eq!(snap.declared_order_state, OrderState::Released);
    assert_eq!(snap.state, IntentState::Declared);

    // 同键异载荷仍是冲突，不因为订单态门控改写存量指纹。
    let err = gw
        .declare_intent(&s, &k, &json!({ "sku": "B" }), &wo)
        .unwrap_err();
    assert!(
        matches!(err, GatewayError::PayloadConflict { .. }),
        "{err:?}"
    );
    assert_eq!(
        gw.snapshot_intent(&k).unwrap().unwrap().payload_sha256,
        fingerprint_of(&payload)
    );

    // Released：窗口开着或关着都不能执行，也不是 WindowStillOpen。不发送。
    let err = gw
        .execute(&s, &k, &wo, counting_effect(counter.clone()))
        .unwrap_err();
    assert_order_state_rejected(
        &err,
        IntentOrderOp::Execute,
        "WO-8008",
        OrderState::Released,
        execute_ok,
    );
    assert_eq!(call_count(&counter), 0);

    clock.advance(Duration::from_millis(50)); // 单调钟到达截止点
    let err = gw
        .execute(&s, &k, &wo, counting_effect(counter.clone()))
        .unwrap_err();
    assert_order_state_rejected(
        &err,
        IntentOrderOp::Execute,
        "WO-8008",
        OrderState::Released,
        execute_ok,
    );
    assert_eq!(call_count(&counter), 0, "窗口关闭也不能在 Released 发送");

    // 递交另一张订单不能把这条意图发出去。
    let err = gw
        .execute(&s, &k, &other, counting_effect(counter.clone()))
        .unwrap_err();
    assert!(
        matches!(err, GatewayError::WorkOrderMismatch { .. }),
        "{err:?}"
    );
    assert_eq!(call_count(&counter), 0);

    // Released 下撤回窗口语义仍在：窗口内可撤回，走到截止点后不可撤回。
    let k_wd = key("WO-8008", "ship", 2);
    gw.declare_intent(&s, &k_wd, &payload, &wo).unwrap();
    assert_eq!(
        gw.withdraw_intent(&s, &k_wd).unwrap(),
        WithdrawOutcome::Withdrawn
    );
    let k_late = key("WO-8008", "ship", 3);
    gw.declare_intent(&s, &k_late, &payload, &wo).unwrap();
    clock.advance(Duration::from_millis(50));
    let err = gw.withdraw_intent(&s, &k_late).unwrap_err();
    assert!(
        matches!(err, GatewayError::WithdrawWindowClosed { .. }),
        "{err:?}"
    );

    // InProgress：同键同载荷幂等，声明快照仍是 Released。窗口已关，可以执行。
    wo.start().unwrap();
    assert_eq!(wo.state(), OrderState::InProgress);
    let again = gw.declare_intent(&s, &k, &payload, &wo).unwrap();
    assert!(matches!(again, DeclareOutcome::AlreadyDeclared { .. }));
    let snap = gw.snapshot_intent(&k).unwrap().unwrap();
    assert_eq!(snap.order_id, "WO-8008");
    assert_eq!(
        snap.declared_order_state,
        OrderState::Released,
        "声明时刻的状态快照不随后续迁移改写"
    );

    let exec = gw
        .execute(&s, &k, &wo, counting_effect(counter.clone()))
        .unwrap();
    assert!(!exec.is_replay());
    assert_eq!(call_count(&counter), 1);

    let err = gw
        .execute(&s, &k_wd, &wo, counting_effect(counter.clone()))
        .unwrap_err();
    assert!(matches!(err, GatewayError::Withdrawn { .. }), "{err:?}");
    assert_eq!(call_count(&counter), 1, "撤回的意图开工后也不发送");

    let replay = gw
        .execute(&s, &k, &wo, counting_effect(counter.clone()))
        .unwrap();
    assert!(replay.is_replay());
    assert_eq!(call_count(&counter), 1);

    // 开工后再声明一条，先不发送，然后完成订单。窗口仍开，但完成态必须拒绝执行。
    let k_pending = key("WO-8008", "ship", 4);
    gw.declare_intent(&s, &k_pending, &payload, &wo).unwrap();
    assert_eq!(
        gw.snapshot_intent(&k_pending)
            .unwrap()
            .unwrap()
            .declared_order_state,
        OrderState::InProgress
    );
    wo.complete().unwrap();
    assert_eq!(wo.state(), OrderState::Completed);

    let err = gw
        .execute(&s, &k_pending, &wo, counting_effect(counter.clone()))
        .unwrap_err();
    assert_order_state_rejected(
        &err,
        IntentOrderOp::Execute,
        "WO-8008",
        OrderState::Completed,
        execute_ok,
    );
    assert_eq!(call_count(&counter), 1);
    assert_eq!(
        gw.snapshot_intent(&k_pending).unwrap().unwrap().state,
        IntentState::Declared,
        "未发送的意图停在 declared"
    );

    // 已经发出的回执在订单完成后仍重放，不第二次外呼。
    let replay = gw
        .execute(&s, &k, &wo, counting_effect(counter.clone()))
        .unwrap();
    assert!(replay.is_replay());
    assert_eq!(call_count(&counter), 1);

    // Completed 后新声明和旧键再声明都拒绝，旧行保持原快照。
    let k_new = key("WO-8008", "ship", 5);
    let err = gw.declare_intent(&s, &k_new, &payload, &wo).unwrap_err();
    assert_order_state_rejected(
        &err,
        IntentOrderOp::Declare,
        "WO-8008",
        OrderState::Completed,
        declare_ok,
    );
    assert!(gw.snapshot_intent(&k_new).unwrap().is_none());

    let err = gw.declare_intent(&s, &k, &payload, &wo).unwrap_err();
    assert_order_state_rejected(
        &err,
        IntentOrderOp::Declare,
        "WO-8008",
        OrderState::Completed,
        declare_ok,
    );
    let snap = gw.snapshot_intent(&k).unwrap().unwrap();
    assert_eq!(snap.state, IntentState::Executed);
    assert_eq!(snap.order_id, "WO-8008");
    assert_eq!(snap.declared_order_state, OrderState::Released);

    // Cancelled 同样不在允许集合里。
    let mut cancelled = prepared_order("WO-8008-C");
    cancelled.release().unwrap();
    cancelled.cancel().unwrap();
    let k_c = key("WO-8008-C", "ship", 1);
    let err = gw
        .declare_intent(&s, &k_c, &payload, &cancelled)
        .unwrap_err();
    assert_order_state_rejected(
        &err,
        IntentOrderOp::Declare,
        "WO-8008-C",
        OrderState::Cancelled,
        declare_ok,
    );
    assert!(gw.snapshot_intent(&k_c).unwrap().is_none());
}

// ===========================================================================
// F2：同纪元两条连接并发 execute 同一意图，外呼只能发生一次
// ===========================================================================
#[test]
fn t9_same_epoch_concurrent_execute_calls_out_once() {
    let db = temp_db("t9");
    let clock = manual_clock();
    let window = Duration::from_millis(30);
    // 两条连接在推进钟之前打开，逻辑毫秒的起点相同，窗口才能一起关上。
    let gw_a = Gateway::open_with_clock(&db, window, clock.clone()).unwrap();
    let gw_b = Gateway::open_with_clock(&db, window, clock.clone()).unwrap();
    let s_a = gw_a.attach().unwrap();
    let s_b = gw_b.attach().unwrap();
    assert_eq!(s_a.epoch, s_b.epoch, "同库同纪元");
    let wo = in_progress_order("WO-F2");
    let k = key("WO-F2", "pay_vendor", 1);
    let payload = json!({ "amount": 1, "to_account": "ACCT-A" });
    gw_a.declare_intent(&s_a, &k, &payload, &wo).unwrap();
    clock.advance(window);

    let counter = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();
    for (gw, session) in [(gw_a, s_a), (gw_b, s_b)] {
        let barrier = Arc::clone(&barrier);
        let counter = Arc::clone(&counter);
        let k = k.clone();
        let wo = in_progress_order("WO-F2");
        handles.push(thread::spawn(move || {
            barrier.wait();
            gw.execute(&session, &k, &wo, move |_payload: &Value| {
                let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
                // 把“已读到 declared、尚未落回执”的窗口留住，让对端也能走进外呼。
                // 不睡眠：有界让出。抢占修好之后对端进不了这个闭包，循环会走完。
                let mut spins = 0;
                while counter.load(Ordering::SeqCst) < 2 && spins < 20_000 {
                    thread::yield_now();
                    spins += 1;
                }
                Ok(json!({ "txn_id": format!("T-{}", n), "provider": "mock-pay" }))
            })
        }));
    }

    let mut ran_fresh = 0usize;
    let mut others = Vec::new();
    for handle in handles {
        match handle.join().unwrap() {
            Ok(ExecuteOutcome::RanFresh { .. }) => ran_fresh += 1,
            other => others.push(format!("{other:?}")),
        }
    }
    assert_eq!(
        call_count(&counter),
        1,
        "F2: 同纪元双连接并发 execute 外呼计数必须为 1，其他结果 {others:?}"
    );
    assert_eq!(ran_fresh, 1, "F2: 恰好一方 RanFresh，其他结果 {others:?}");

    let gw = Gateway::open_with_clock(&db, window, clock).unwrap();
    let s = gw.attach().unwrap();
    let again = gw
        .execute(&s, &k, &wo, counting_effect(counter.clone()))
        .unwrap();
    assert!(again.is_replay(), "回执落库后只能重放");
    assert_eq!(call_count(&counter), 1, "重放不得再外呼");

    let _ = std::fs::remove_file(&db);
}

fn assert_order_state_rejected(
    err: &GatewayError,
    op: IntentOrderOp,
    order_id: &str,
    state: OrderState,
    allowed: &[OrderState],
) {
    match err {
        GatewayError::OrderStateRejected {
            op: got_op,
            order_id: got_id,
            state: got_state,
            allowed: got_allowed,
        } => {
            assert_eq!(*got_op, op);
            assert_eq!(got_id, order_id);
            assert_eq!(*got_state, state);
            assert_eq!(*got_allowed, allowed);
        }
        other => panic!("期望 OrderStateRejected，实际 {other:?}"),
    }
    let text = err.to_string();
    assert!(
        text.contains(op_name(op)) && text.contains(&format!("{state:?}")),
        "错误文本要点明操作和状态: {text}"
    );
}

fn op_name(op: IntentOrderOp) -> &'static str {
    match op {
        IntentOrderOp::Declare => "declare",
        IntentOrderOp::Execute => "execute",
    }
}

// —— 测试辅助：直接用库里同样的指纹算法 ——
fn fingerprint_of(v: &Value) -> String {
    // 通过公开 API 间接构造：声明一个用 v 的意图再读回指纹
    let gw = Gateway::open_in_memory(Duration::from_millis(1)).unwrap();
    let s = gw.attach().unwrap();
    let k = key("FP", "fp", 0);
    let wo = in_progress_order("FP");
    gw.declare_intent(&s, &k, v, &wo).unwrap();
    gw.snapshot_intent(&k).unwrap().unwrap().payload_sha256
}
