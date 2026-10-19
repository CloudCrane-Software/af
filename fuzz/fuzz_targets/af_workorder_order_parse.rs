//! 从字节构造最小合法工单，走 JSON 解析和门控状态机。
//!
//! 不调用 [`af_workorder::Transition::from_state`]：`Cancel` 在该函数里是
//! `unreachable!`（起点不唯一）。状态机入口是 [`WorkOrder::transition`]。

#![no_main]

use af_workorder::{
    BudgetVoucher, GraphVersion, LineBinding, Priority, Sla, Transition, WorkArtifact, WorkOrder,
};
use chrono::{DateTime, TimeZone, Utc};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() <= 8 * 1024 {
        if let Ok(mut parsed) = serde_json::from_slice::<WorkOrder>(data) {
            let _ = parsed.id();
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
    drive(&mut order, data);

    if let Ok(bytes) = serde_json::to_vec(&order) {
        if let Ok(mut round_trip) = serde_json::from_slice::<WorkOrder>(&bytes) {
            drive(&mut round_trip, data);
        }
    }
});

fn drive(order: &mut WorkOrder, data: &[u8]) {
    let version = order.graph_version().clone();
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

    if order.graph_version() != &version {
        panic!("graph_version changed through the gate");
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
