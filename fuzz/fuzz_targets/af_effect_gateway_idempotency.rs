//! 从字节构造幂等键和载荷，走声明、指纹观测、撤回与执行。
//!
//! 时间全部走 [`ManualClock`]，不 `sleep`，不读 [`af_effect_gateway::SystemClock`]。
//! 载荷指纹没有单独的公共函数，入口是 [`Gateway::declare_intent`]，
//! 观测面是 [`Gateway::snapshot_intent`] 的 `payload_sha256`。

#![no_main]

use std::time::Duration;

use af_effect_gateway::{Gateway, IntentKey, ManualClock, Result as GwResult};
use libfuzzer_sys::fuzz_target;
use serde_json::{json, Value};

fuzz_target!(|data: &[u8]| {
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
    let key_a = key_from(data, 0);
    let key_b = key_from(data, 1);

    let declared_a = gateway.declare_intent(&session, &key_a, &payload);
    let declared_b = gateway.declare_intent(&session, &key_b, &payload);
    // 同键同载荷再声明一次：幂等接受，不改指纹。
    let _ = gateway.declare_intent(&session, &key_a, &payload);
    if declared_a.is_ok() && declared_b.is_ok() {
        match (
            gateway.snapshot_intent(&key_a),
            gateway.snapshot_intent(&key_b),
        ) {
            (Ok(Some(a)), Ok(Some(b))) if a.payload_sha256 != b.payload_sha256 => {
                panic!("same payload produced two fingerprints");
            }
            _ => {}
        }
    }

    // 同键异载荷：冲突或（首次声明失败时）新登记。都不该 panic。
    let _ = gateway.declare_intent(&session, &key_a, &tweak(&payload, data));

    let key_c = key_from(data, 2);
    let _ = gateway.declare_intent(&session, &key_c, &payload);
    clock.advance(Duration::from_millis(window_ms.saturating_add(1)));
    let _ = gateway.execute(&session, &key_c, |payload: &Value| -> GwResult<String> {
        Ok(payload.to_string())
    });

    let key_d = key_from(data, 3);
    let _ = gateway.declare_intent(&session, &key_d, &payload);
    let _ = gateway.withdraw_intent(&session, &key_d);
    let _ = gateway.record_receipt(&session, &key_d, &json!({"ok": true}));

    // 纪元推进后，旧 session 再声明应被 fencing 拒绝，而不是 panic。
    let _ = gateway.resume();
    let _ = gateway.declare_intent(&session, &key_a, &payload);
});

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

fn key_from(data: &[u8], seq_delta: i64) -> IntentKey {
    IntentKey {
        workorder_id: token(data, 32),
        step_path: token(data.get(1..).unwrap_or_default(), 32),
        seq: read_i64(data).wrapping_add(seq_delta),
    }
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
