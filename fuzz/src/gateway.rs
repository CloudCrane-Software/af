//! 从字节构造幂等键、载荷和订单，走声明、快照、撤回与执行。
//!
//! 时间全部走 [`ManualClock`]，不 `sleep`，不读 [`af_effect_gateway::SystemClock`]。
//! [`Gateway::declare_intent`] 与 [`Gateway::execute`] 都递交当前 [`WorkOrder`]。
//! 声明成功时，快照的 `declared_order_state` 必须等于声明当时的 `order.state()`，
//! 之后订单再迁移也不改写这份快照。

use std::time::Duration;

use af_effect_gateway::{Gateway, IntentKey, ManualClock, Result as GwResult};
use af_workorder::{
    BudgetVoucher, GraphVersion, LineBinding, OrderState, Priority, Sla, WorkArtifact, WorkOrder,
};
use chrono::{DateTime, TimeZone, Utc};
use serde_json::{json, Value};

/// 跑一条输入。拒绝路径留在 `Result` 里，快照被改写才 panic。
pub fn fuzz_one(data: &[u8]) {
    let window_ms = u64::from(u16::from_le_bytes([
        data.first().copied().unwrap_or(0),
        data.get(1).copied().unwrap_or(0),
    ]));
    let clock = ManualClock::new(read_i64(data), read_i64(data.get(8..).unwrap_or_default()));
    // 墙钟回拨只动审计字段。窗口开关看单调钟。
    clock.shift_wall_ms(-60_000);

    let gateway =
        match Gateway::open_in_memory_with_clock(Duration::from_millis(window_ms), clock.clone()) {
            Ok(gateway) => gateway,
            Err(_) => return,
        };
    let session = match gateway.attach() {
        Ok(session) => session,
        Err(_) => return,
    };
    let _ = gateway.current_epoch();

    let payload = payload_from(data);
    let order_id = token(data, 32);
    let mut order = order_from(data, &order_id);

    // 幂等键上的 workorder 与递交的订单不是同一张：必须拒绝，不能 panic。
    if data.get(3).is_some_and(|b| b & 0x80 != 0) {
        let other = order_from(data, &format!("x{order_id}"));
        let foreign = key_from(data, 9, &order_id);
        let _ = gateway.declare_intent(&session, &foreign, &payload, &other);
        let _ = gateway.execute(&session, &foreign, &other, echo_payload);
    }

    let key_a = key_from(data, 0, &order_id);
    let key_b = key_from(data, 1, &order_id);
    let state_at_declare = order.state();
    let declared_a = gateway.declare_intent(&session, &key_a, &payload, &order);
    let declared_b = gateway.declare_intent(&session, &key_b, &payload, &order);
    let repeated = gateway.declare_intent(&session, &key_a, &payload, &order);
    if declared_a.is_ok() && declared_b.is_ok() {
        let snap_a = gateway.snapshot_intent(&key_a).ok().flatten();
        let snap_b = gateway.snapshot_intent(&key_b).ok().flatten();
        match (snap_a, snap_b) {
            (Some(a), Some(b)) => {
                if a.payload_sha256 != b.payload_sha256 {
                    panic!("same payload produced two fingerprints");
                }
                if a.declared_order_state != state_at_declare
                    || b.declared_order_state != state_at_declare
                    || a.order_id != order.id()
                    || b.order_id != order.id()
                {
                    panic!("declared_order_state is not the order state at declare time");
                }
            }
            _ => panic!("successful declare left no snapshot"),
        }
        if repeated.is_err() {
            panic!("same key and payload was rejected after a successful declare");
        }
        if let Ok(Some(again)) = gateway.snapshot_intent(&key_a) {
            if again.declared_order_state != state_at_declare || again.payload_sha256.is_empty() {
                panic!("repeat declare rewrote the declare-time snapshot");
            }
        }
    }

    // 同键异载荷：冲突，或首次声明失败时的新登记。都不该 panic，也不改已有快照。
    let frozen = gateway
        .snapshot_intent(&key_a)
        .ok()
        .flatten()
        .map(|snap| (snap.payload_sha256, snap.declared_order_state));
    let _ = gateway.declare_intent(&session, &key_a, &tweak(&payload, data), &order);
    assert_snapshot_frozen(&gateway, &key_a, frozen.as_ref());

    push_toward_in_progress(&mut order);
    assert_snapshot_frozen(&gateway, &key_a, frozen.as_ref());

    let key_c = key_from(data, 2, &order_id);
    let state_c = order.state();
    let declared_c = gateway.declare_intent(&session, &key_c, &payload, &order);
    if declared_c.is_ok() {
        match gateway.snapshot_intent(&key_c) {
            Ok(Some(snap))
                if snap.declared_order_state == state_c && snap.order_id == order.id() => {}
            Ok(Some(_)) => panic!("key snapshot does not match the order at declare time"),
            Ok(None) => panic!("declare succeeded but snapshot is missing"),
            Err(_) => {}
        }
    }

    clock.advance(Duration::from_millis(window_ms.saturating_add(1)));
    let _ = gateway.execute(&session, &key_c, &order, echo_payload);
    if declared_c.is_ok() {
        if let Ok(Some(snap)) = gateway.snapshot_intent(&key_c) {
            if snap.declared_order_state != state_c {
                panic!("execute rewrote declared_order_state");
            }
        }
    }

    let key_d = key_from(data, 3, &order_id);
    let _ = gateway.declare_intent(&session, &key_d, &payload, &order);
    let _ = gateway.withdraw_intent(&session, &key_d);
    let _ = gateway.record_receipt(&session, &key_d, &json!({"ok": true}));

    finish_or_cancel(&mut order, data);
    let _ = gateway.declare_intent(&session, &key_a, &payload, &order);
    assert_snapshot_frozen(&gateway, &key_a, frozen.as_ref());

    // 纪元推进后，旧 session 再声明应被 fencing 拒绝，而不是 panic。
    let _ = gateway.resume();
    let _ = gateway.declare_intent(&session, &key_a, &payload, &order);
    let _ = gateway.execute(&session, &key_c, &order, echo_payload);
}

fn echo_payload(payload: &Value) -> GwResult<String> {
    Ok(payload.to_string())
}

fn assert_snapshot_frozen(
    gateway: &Gateway,
    key: &IntentKey,
    frozen: Option<&(String, OrderState)>,
) {
    let Some((fingerprint, state)) = frozen else {
        return;
    };
    if let Ok(Some(snap)) = gateway.snapshot_intent(key) {
        if snap.payload_sha256 != *fingerprint || snap.declared_order_state != *state {
            panic!("later call rewrote the declare-time snapshot");
        }
    }
}

/// 按字节选择声明当时的订单态。产线与已签名凭证先挂上，后续 release 才过门控。
fn order_from(data: &[u8], id: &str) -> WorkOrder {
    let priority = match data.first().copied().unwrap_or(0) % 4 {
        0 => Priority::Low,
        1 => Priority::Normal,
        2 => Priority::High,
        _ => Priority::Critical,
    };
    let deadline = if data.get(1).is_some_and(|b| b & 1 == 1) {
        Some(injected_now(data))
    } else {
        None
    };
    let mut order = WorkOrder::new(
        id,
        GraphVersion::new(token(data.get(2..).unwrap_or_default(), 24)),
        Sla::new(deadline, priority),
    );
    let _ = order.bind_line(LineBinding {
        line_id: "line".into(),
        line_version: "lv".into(),
    });
    let _ = order.attach_budget_voucher(BudgetVoucher {
        voucher_id: "bv".into(),
        amount: 1,
        signed: true,
    });
    // 短输入默认 InProgress，声明和执行两条门控都走得到。
    let phase = data.get(4).copied().unwrap_or(2) % 6;
    if phase >= 1 {
        let _ = order.release();
    }
    if (2..=4).contains(&phase) {
        let _ = order.start();
    }
    if (3..=4).contains(&phase) {
        let _ = order.complete();
    }
    if phase == 4 {
        let _ = order.attach_artifact(WorkArtifact {
            artifact_id: "fa".into(),
            is_final: true,
            qualified: true,
        });
        let _ = order.close();
    }
    if phase == 5 {
        let _ = order.cancel();
    }
    order
}

fn push_toward_in_progress(order: &mut WorkOrder) {
    match order.state() {
        OrderState::Created => {
            let _ = order.release();
            let _ = order.start();
        }
        OrderState::Released => {
            let _ = order.start();
        }
        _ => {}
    }
}

fn finish_or_cancel(order: &mut WorkOrder, data: &[u8]) {
    push_toward_in_progress(order);
    if data.get(5).is_some_and(|b| b & 1 == 0) {
        let _ = order.cancel();
        return;
    }
    let _ = order.complete();
    let _ = order.attach_artifact(WorkArtifact {
        artifact_id: "fa".into(),
        is_final: true,
        qualified: true,
    });
    let _ = order.close();
}

fn payload_from(data: &[u8]) -> Value {
    if !data.is_empty() && data.len() <= 4 * 1024 {
        if let Ok(value) = serde_json::from_slice::<Value>(data) {
            return value;
        }
    }
    json!({
        "k": data.first().copied().unwrap_or(0),
        "n": data.len() as u64,
    })
}

fn tweak(payload: &Value, data: &[u8]) -> Value {
    json!({
        "payload": payload,
        "n": data.len() as u64,
    })
}

fn key_from(data: &[u8], seq_delta: i64, order_id: &str) -> IntentKey {
    IntentKey {
        workorder_id: order_id.to_string(),
        step_path: token(data.get(1..).unwrap_or_default(), 32),
        seq: read_i64(data).wrapping_add(seq_delta),
    }
}

fn injected_now(data: &[u8]) -> DateTime<Utc> {
    let secs = read_i64(data).clamp(-62_135_596_800, 253_402_300_799);
    Utc.timestamp_opt(secs, 0)
        .single()
        .unwrap_or(DateTime::UNIX_EPOCH)
}

fn read_i64(data: &[u8]) -> i64 {
    let mut buf = [0u8; 8];
    let n = data.len().min(buf.len());
    buf[..n].copy_from_slice(&data[..n]);
    i64::from_le_bytes(buf)
}

fn token(data: &[u8], max: usize) -> String {
    let n = data.len().min(max);
    String::from_utf8_lossy(&data[..n]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_effect_gateway::{GatewayError, IntentOrderOp};

    fn clock() -> ManualClock {
        ManualClock::new(0, 1_700_000_000_000)
    }

    #[test]
    fn f3_released_snapshot_survives_start_and_execute_waits_for_in_progress() {
        let clock = clock();
        let gw =
            Gateway::open_in_memory_with_clock(Duration::from_millis(20), clock.clone()).unwrap();
        let session = gw.attach().unwrap();
        let mut order = order_from(&[1, 0, 0, 0, 1], "WO-F3");
        assert_eq!(order.state(), OrderState::Released);
        let key = key_from(b"WO-F3", 0, "WO-F3");
        let payload = json!({"sku": "A"});
        gw.declare_intent(&session, &key, &payload, &order).unwrap();
        let snap = gw.snapshot_intent(&key).unwrap().unwrap();
        assert_eq!(snap.order_id, "WO-F3");
        assert_eq!(snap.declared_order_state, OrderState::Released);

        // 窗口开着也先看订单态：Released 不是 WindowStillOpen。
        let err = gw
            .execute(&session, &key, &order, echo_payload)
            .unwrap_err();
        assert!(matches!(
            err,
            GatewayError::OrderStateRejected {
                op: IntentOrderOp::Execute,
                state: OrderState::Released,
                ..
            }
        ));

        order.start().unwrap();
        assert_eq!(order.state(), OrderState::InProgress);
        gw.declare_intent(&session, &key, &payload, &order).unwrap();
        let snap = gw.snapshot_intent(&key).unwrap().unwrap();
        assert_eq!(
            snap.declared_order_state,
            OrderState::Released,
            "声明时刻的状态快照不随后续迁移改写"
        );

        clock.advance(Duration::from_millis(20));
        let ran = gw.execute(&session, &key, &order, echo_payload).unwrap();
        assert!(!ran.is_replay());
        let snap = gw.snapshot_intent(&key).unwrap().unwrap();
        assert_eq!(snap.declared_order_state, OrderState::Released);
        assert_eq!(snap.order_id, "WO-F3");
    }

    #[test]
    fn f3_created_declare_does_not_persist() {
        let gw = Gateway::open_in_memory_with_clock(Duration::from_millis(1), clock()).unwrap();
        let session = gw.attach().unwrap();
        let order = order_from(&[0, 0, 0, 0, 0], "WO-NEW");
        assert_eq!(order.state(), OrderState::Created);
        let key = key_from(b"WO-NEW", 0, "WO-NEW");
        let err = gw
            .declare_intent(&session, &key, &json!({}), &order)
            .unwrap_err();
        assert!(matches!(
            err,
            GatewayError::OrderStateRejected {
                op: IntentOrderOp::Declare,
                state: OrderState::Created,
                ..
            }
        ));
        assert!(gw.snapshot_intent(&key).unwrap().is_none());
    }

    #[test]
    fn f3_sample_inputs_do_not_panic() {
        fuzz_one(&[]);
        fuzz_one(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
        fuzz_one(&[0xff; 64]);
        fuzz_one(br#"{"k":1,"n":2}"#);
    }
}
