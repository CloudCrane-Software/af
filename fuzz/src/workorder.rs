//! 从字节构造最小合法工单，走 JSON 解析和门控状态机。
//!
//! 不调用 [`af_workorder::Transition::from_state`]：`Cancel` 在该函数里是
//! `unreachable!`（起点不唯一）。状态机入口是 [`WorkOrder::transition`]。
//! 老载荷缺 `stable_id` / `run_id` 时 [`decode_work_order`] 返回拒绝式信封，
//! 这里不补默认身份。`continue_as_new` 只换 `run_id`。

use af_workorder::{
    decode_work_order, BudgetVoucher, GraphVersion, LineBinding, Priority, RunContext, Sla,
    StableId, Transition, WorkArtifact, WorkOrder,
};
use chrono::{DateTime, TimeZone, Utc};

/// 跑一条输入。解析失败就跳过；身份或 `graph_version` 被改写才 panic。
pub fn fuzz_one(data: &[u8]) {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = decode_work_order(text);
    }

    if data.len() <= 8 * 1024 {
        if let Ok(mut parsed) = serde_json::from_slice::<WorkOrder>(data) {
            let _ = parsed.id();
            let _ = parsed.stable_id();
            let _ = parsed.run_id();
            let _ = parsed.sla().deadline;
            drive(&mut parsed, data);
        }
    }

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
        token(data, 32),
        GraphVersion::new(token(data.get(2..).unwrap_or_default(), 24)),
        Sla::new(deadline, priority),
    );
    let generated = StableId::generate(order.id());
    if order.stable_id() != &generated {
        panic!("WorkOrder::new stable_id is not generate(id)");
    }
    if !order.run_id().is_empty() {
        panic!("new order is not detached from an engine run");
    }
    drive(&mut order, data);

    let bytes = match serde_json::to_vec(&order) {
        Ok(bytes) => bytes,
        Err(_) => return,
    };
    if let Ok(mut round_trip) = serde_json::from_slice::<WorkOrder>(&bytes) {
        if round_trip.stable_id() != order.stable_id() || round_trip.run_id() != order.run_id() {
            panic!("json round trip dropped identity");
        }
        drive(&mut round_trip, data);
    }
    if let Ok(text) = std::str::from_utf8(&bytes) {
        if let Ok(decoded) = decode_work_order(text) {
            if decoded.stable_id() != order.stable_id() || decoded.run_id() != order.run_id() {
                panic!("decode_work_order dropped identity");
            }
        }
    }
}

fn drive(order: &mut WorkOrder, data: &[u8]) {
    let version = order.graph_version().clone();
    let sid = order.stable_id().clone();
    let identity = order.clone();
    let steps = [
        Transition::Release,
        Transition::Start,
        Transition::Complete,
        Transition::Close,
        Transition::Cancel,
    ];

    // 具名入口各走一次，错误留在 Result 里。
    let before = order.state();
    let named = match data.first().copied().unwrap_or(0) % 5 {
        0 => order.release(),
        1 => order.start(),
        2 => order.complete(),
        3 => order.close(),
        _ => order.cancel(),
    };
    if named.is_err() && order.state() != before {
        panic!("named transition mutated state on error: {before:?}");
    }

    for i in 0..12 {
        let b = data.get(i).copied().unwrap_or(0);
        if b & 0b0001 != 0 {
            let _ = order.bind_line(LineBinding {
                line_id: token(data, 24),
                line_version: token(data.get(i..).unwrap_or_default(), 16),
            });
        }
        if b & 0b0010 != 0 {
            let _ = order.attach_budget_voucher(BudgetVoucher {
                voucher_id: token(data, 16),
                amount: u64::from(b),
                signed: b & 0b1000 != 0,
            });
        }
        if b & 0b0100 != 0 {
            let _ = order.attach_artifact(WorkArtifact {
                artifact_id: format!("a{i}"),
                is_final: b & 0b1000 != 0,
                qualified: b & 0b1_0000 != 0,
            });
        }
        let t = steps[(b as usize) % steps.len()];
        let state_before = order.state();
        if order.transition(t).is_err() && order.state() != state_before {
            panic!("rejected transition mutated state: {state_before:?}");
        }
        let _ = order.is_overdue(injected_now(data.get(i..).unwrap_or_default()));
        let _ = order.state().allowed_transitions();
        let _ = order.state().can_apply(t);
        let _ = order.state().is_terminal();
        let _ = order.history().len();
        let _ = t.to_state();
    }

    let run = token(data, 16);
    let _ = order.continue_as_new(RunContext::new(run.clone()));
    if order.run_id() != run {
        panic!("continue_as_new did not install the injected run");
    }
    if order.stable_id() != &sid || !order.same_identity(&identity) {
        panic!("stable_id changed through the gate");
    }
    if order.graph_version() != &version {
        panic!("graph_version changed through the gate");
    }
    let _ = order.shard_key(0);
    if let Ok(key) = order.shard_key(64) {
        if order.shard_key(64).ok() != Some(key) {
            panic!("shard_key changed without an identity change");
        }
    }
}

/// 调用方注入的时刻。钳在 chrono 可表示的秒范围内，不读系统钟。
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
    use af_workorder::OrderDecodeError;

    #[test]
    fn f3_stable_id_survives_gate_and_continue_as_new() {
        let mut order = WorkOrder::new(
            "wo-1",
            GraphVersion::new("g-1"),
            Sla::new(None, Priority::Normal),
        );
        assert_eq!(order.stable_id(), &StableId::generate("wo-1"));
        assert!(order.run_id().is_empty());
        drive(&mut order, b"\x01\x02\x03\x04");
        assert_eq!(order.stable_id(), &StableId::generate("wo-1"));
        assert_eq!(order.run_id(), "\u{1}\u{2}\u{3}\u{4}");
        assert!(order.shard_key(0).is_err());
        let key = order.shard_key(64).unwrap();
        assert_eq!(order.shard_key(64).unwrap(), key);
    }

    #[test]
    fn f3_missing_stable_id_is_rejected_without_a_default() {
        let err = decode_work_order(r#"{"id":"wo-1","state":"Created"}"#).unwrap_err();
        match err {
            OrderDecodeError::Rejected(envelope) => assert_eq!(envelope.field, "stable_id"),
            other => panic!("expected rejection envelope, got {other}"),
        }
    }

    #[test]
    fn f3_sample_inputs_do_not_panic() {
        fuzz_one(&[]);
        fuzz_one(b"{");
        fuzz_one(br#"{"id":"wo"}"#);
        fuzz_one(&[0xff; 32]);
    }
}
