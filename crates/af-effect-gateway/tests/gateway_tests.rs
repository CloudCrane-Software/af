//! af-effect-gateway PoC 验收测试。
//!
//! 对应任务四条验收用例，另加两条补充（窗口门控、外部失败可重试）。

use af_effect_gateway::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn temp_db(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!(
        "af_eg_poc_{}_{}_{}.db",
        tag,
        std::process::id(),
        nanos
    ));
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

// ===========================================================================
// 用例①：崩溃恢复后同键同载荷重放 → 返回已存结果，不重发
// ===========================================================================
#[test]
fn t1_crash_recovery_same_key_same_payload_replays_stored_result() {
    let db = temp_db("t1");
    let counter = Arc::new(AtomicUsize::new(0));
    let k = key("WO-1001", "pay_vendor", 1);
    let payload = json!({ "amount": 12800, "to_account": "ACCT-A" });

    // —— 第一次生命周期：declare → 窗口 → execute（真实外呼一次）→ “崩溃” ——
    {
        let gw = Gateway::open(&db, Duration::from_millis(50)).unwrap();
        assert_eq!(gw.current_epoch().unwrap(), 0, "新库纪元从 0 开始");
        let s = gw.attach().unwrap();
        let out = gw.declare_intent(&s, &k, &payload).unwrap();
        assert!(matches!(out, DeclareOutcome::Created { .. }));

        sleep(Duration::from_millis(80)); // 等窗口关闭
        let exec = gw
            .execute(&s, &k, counting_effect(counter.clone()))
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
        let gw = Gateway::open(&db, Duration::from_millis(50)).unwrap();
        assert_eq!(gw.current_epoch().unwrap(), 0, "恢复后先看到崩溃前纪元");
        let s2 = gw.resume().unwrap();
        assert_eq!(s2.epoch, 1, "resume 必须使纪元 +1");

        // 重放 declare：同键同载荷 → 幂等接受
        let re = gw.declare_intent(&s2, &k, &payload).unwrap();
        assert!(matches!(re, DeclareOutcome::AlreadyDeclared { .. }));

        // 重放 execute：外部调用计数不变，返回已存回执
        let exec = gw
            .execute(&s2, &k, counting_effect(counter.clone()))
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
    let p1 = json!({ "holder": "张三", "cert": "C-001" });
    let p2 = json!({ "holder": "李四", "cert": "C-001" }); // 同键，载荷不同

    gw.declare_intent(&s, &k, &p1).unwrap();

    let err = gw.declare_intent(&s, &k, &p2).unwrap_err();
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
    let ok = gw.declare_intent(&s, &k, &p1).unwrap();
    assert!(matches!(ok, DeclareOutcome::AlreadyDeclared { .. }));
}

// ===========================================================================
// 用例③：窗口内撤回 → 不发送
// ===========================================================================
#[test]
fn t3_withdraw_within_window_means_never_sent() {
    let counter = Arc::new(AtomicUsize::new(0));
    let gw = Gateway::open_in_memory(Duration::from_millis(300)).unwrap(); // 长窗口
    let s = gw.attach().unwrap();
    let k = key("WO-3003", "send_sms", 1);
    let payload = json!({ "phone": "138xxxx", "text": "验证码" });

    let out = gw.declare_intent(&s, &k, &payload).unwrap();
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
        .execute(&s, &k, counting_effect(counter.clone()))
        .unwrap_err();
    assert!(
        matches!(err, GatewayError::Withdrawn { .. }),
        "实际 {:?}",
        err
    );
    assert_eq!(call_count(&counter), 0, "撤回后绝不发送");

    // 对照组：另一个意图窗口已关才去撤回 → 撤回失败（窗口保护有效）
    let k2 = key("WO-3003", "send_sms", 2);
    gw.declare_intent(&s, &k2, &payload).unwrap();
    sleep(Duration::from_millis(330)); // 越过 300ms 窗口
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
    let gw = Gateway::open(&db, Duration::from_millis(30)).unwrap();
    let s_old = gw.attach().unwrap(); // epoch 0
    assert_eq!(s_old.epoch, 0);
    let k = key("WO-4004", "dispatch_truck", 1);
    gw.declare_intent(&s_old, &k, &json!({"truck": "B-01"}))
        .unwrap();

    // 崩溃恢复：resume → 纪元 1
    let s_new = gw.resume().unwrap();
    assert_eq!(s_new.epoch, 1);

    // 旧纪元 session 发起的请求：declare / withdraw / execute 全拒
    for (op, res) in [
        (
            "declare",
            gw.declare_intent(&s_old, &k, &json!({"truck": "B-01"}))
                .map(|_| ()),
        ),
        ("withdraw", gw.withdraw_intent(&s_old, &k).map(|_| ())),
        (
            "execute",
            gw.execute(&s_old, &k, counting_effect(counter.clone()))
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
    sleep(Duration::from_millis(50)); // 越过 30ms 窗口
    let exec = gw
        .execute(&s_newer, &k, counting_effect(counter.clone()))
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
    let gw = Gateway::open_in_memory(Duration::from_millis(120)).unwrap();
    let s = gw.attach().unwrap();
    let k = key("WO-5005", "charge_fee", 1);
    gw.declare_intent(&s, &k, &json!({"fee": 99})).unwrap();

    let err = gw
        .execute(&s, &k, counting_effect(counter.clone()))
        .unwrap_err();
    match &err {
        GatewayError::WindowStillOpen {
            deadline_ms,
            now_ms: _,
        } => assert!(*deadline_ms > 0, "deadline 必须是未来时刻"),
        other => panic!("期望 WindowStillOpen，实际 {:?}", other),
    }
    assert_eq!(call_count(&counter), 0, "窗口未关不发送");

    sleep(Duration::from_millis(150)); // 越过窗口
    let exec = gw
        .execute(&s, &k, counting_effect(counter.clone()))
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
    let gw = Gateway::open_in_memory(Duration::from_millis(20)).unwrap();
    let s = gw.attach().unwrap();
    let k = key("WO-6006", "refund", 1);
    gw.declare_intent(&s, &k, &json!({"order": "D-9"})).unwrap();
    sleep(Duration::from_millis(40));

    let c1 = counter.clone();
    let fail = move |_p: &Value| -> Result<Value> {
        c1.fetch_add(1, Ordering::SeqCst);
        external_err("mock 网关超时")
    };
    let err = gw.execute(&s, &k, fail).unwrap_err();
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
    let exec = gw.execute(&s, &k, ok_call).unwrap();
    assert!(!exec.is_replay());
    assert_eq!(exec.into_inner()["txn_id"], "T-2");
    assert_eq!(
        gw.snapshot_intent(&k).unwrap().unwrap().state,
        IntentState::Executed
    );

    // 再 execute：重放，不重发
    let again = gw
        .execute(&s, &k, counting_effect(counter.clone()))
        .unwrap();
    assert!(again.is_replay());
    assert_eq!(counter.load(Ordering::SeqCst), 2);
}

// —— 测试辅助：直接用库里同样的指纹算法 ——
fn fingerprint_of(v: &Value) -> String {
    // 通过公开 API 间接构造：声明一个用 v 的意图再读回指纹
    let gw = Gateway::open_in_memory(Duration::from_millis(1)).unwrap();
    let s = gw.attach().unwrap();
    let k = key("FP", "fp", 0);
    gw.declare_intent(&s, &k, v).unwrap();
    gw.snapshot_intent(&k).unwrap().unwrap().payload_sha256
}
