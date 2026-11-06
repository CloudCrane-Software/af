//! wo-engine —— 形态 A「薄壳」的 Rust 判定引擎：把 af-workorder 的状态机+门控包成独立 bin。
//!
//! 协议：stdin 一个 JSON 请求 → stdout 一个 JSON 应答；**每次调用无状态**，
//! 订单快照整体进、整体出。订单的 schema 由 af-workorder 类型的 serde 派生唯一决定，
//! Python 侧只透传不解释（薄壳的含义就在这：订单模型只有 Rust 一份）。
//!
//! 请求: {"op":"transition"|"bind_line"|"attach_voucher"|"attach_artifact",
//!        "order":{...快照...},
//!        "transition":"release"|"start"|"complete"|"close"|"cancel",   // op=transition 时
//!        "line":{...}, "voucher":{...}, "artifact":{...}}              // 对应登记类 op
//! 应答: {"ok":true, "order":{...新快照...}, "state":"Released", "engine_version":"v1"}
//!     | {"ok":false, "error":{"kind":"PreconditionsNotMet","detail":{...serde 原样...},
//!        "message":"门控拒绝 ..."}, "engine_version":"v1"}

use std::io::Read;

use af_workorder::{
    BudgetVoucher, GateError, LineBinding, Transition, WorkArtifact, WorkOrder,
};
use serde::Deserialize;

/// 引擎版本号：随每次响应回带，供实验观测"哪个版本的引擎做了判定"。
const ENGINE_VERSION: &str = "v1";

#[derive(Deserialize)]
struct Req {
    op: String,
    #[serde(default)]
    order: Option<serde_json::Value>,
    #[serde(default)]
    transition: Option<String>,
    #[serde(default)]
    line: Option<LineBinding>,
    #[serde(default)]
    voucher: Option<BudgetVoucher>,
    #[serde(default)]
    artifact: Option<WorkArtifact>,
}

/// 引擎内部错误载体：门控错误走 af-workorder 的 GateError；
/// 引擎自己的新前置条件（涟漪实验用）没有对应变体，走 Raw JSON（shape 与 GateError 对齐）。
enum Fail {
    Gate(GateError),
    Raw(serde_json::Value),
}

fn parse_transition(s: &str) -> Option<Transition> {
    match s {
        "release" => Some(Transition::Release),
        "start" => Some(Transition::Start),
        "complete" => Some(Transition::Complete),
        "close" => Some(Transition::Close),
        "cancel" => Some(Transition::Cancel),
        _ => None,
    }
}

fn gate_error_json(e: &GateError) -> serde_json::Value {
    serde_json::json!({
        "kind": match e {
            GateError::IllegalTransition { .. } => "IllegalTransition",
            GateError::PreconditionsNotMet { .. } => "PreconditionsNotMet",
            GateError::MutationForbidden { .. } => "MutationForbidden",
        },
        "detail": serde_json::to_value(e).unwrap_or(serde_json::Value::Null),
        "message": e.to_string(),
    })
}

fn respond(order: &WorkOrder, applied: Result<(), Fail>) {
    let out = match applied {
        Ok(()) => serde_json::json!({
            "ok": true,
            "order": order,
            "state": format!("{:?}", order.state()),
            "engine_version": ENGINE_VERSION,
        }),
        Err(Fail::Gate(e)) => serde_json::json!({
            "ok": false,
            "error": gate_error_json(&e),
            "engine_version": ENGINE_VERSION,
        }),
        Err(Fail::Raw(v)) => serde_json::json!({
            "ok": false,
            "error": v,
            "engine_version": ENGINE_VERSION,
        }),
    };
    println!("{out}");
}

fn main() {
    let mut buf = String::new();
    if std::io::stdin().read_to_string(&mut buf).is_err() {
        eprintln!("engine: 读 stdin 失败");
        std::process::exit(2);
    }
    let req: Req = match serde_json::from_str(&buf) {
        Ok(r) => r,
        Err(e) => {
            println!(
                "{}",
                serde_json::json!({
                    "ok": false,
                    "error": {"kind": "BadRequest", "detail": null, "message": format!("请求 JSON 不合法: {e}")},
                    "engine_version": ENGINE_VERSION,
                })
            );
            return;
        }
    };
    let mut order: WorkOrder = match req.order {
        Some(v) => match serde_json::from_value(v) {
            Ok(o) => o,
            Err(e) => {
                println!(
                    "{}",
                    serde_json::json!({
                        "ok": false,
                        "error": {"kind": "BadRequest", "detail": null, "message": format!("订单快照不合法: {e}")},
                        "engine_version": ENGINE_VERSION,
                    })
                );
                return;
            }
        },
        None => {
            println!(
                "{}",
                serde_json::json!({
                    "ok": false,
                    "error": {"kind": "BadRequest", "detail": null, "message": "缺少 order 快照"},
                    "engine_version": ENGINE_VERSION,
                })
            );
            return;
        }
    };

    let applied: Result<(), Fail> = match req.op.as_str() {
        "bind_line" => match req.line {
            Some(l) => order.bind_line(l).map_err(Fail::Gate),
            None => Err(bad_request("bind_line 需要 line")),
        },
        "attach_voucher" => match req.voucher {
            Some(v) => order.attach_budget_voucher(v).map_err(Fail::Gate),
            None => Err(bad_request("attach_voucher 需要 voucher")),
        },
        "attach_artifact" => match req.artifact {
            Some(a) => order.attach_artifact(a).map_err(Fail::Gate),
            None => Err(bad_request("attach_artifact 需要 artifact")),
        },
        "transition" => match req.transition.as_deref().and_then(parse_transition) {
            Some(t) => order.transition(t).map(|_| ()).map_err(Fail::Gate),
            None => Err(bad_request("transition 需要 release|start|complete|close|cancel")),
        },
        other => Err(bad_request(&format!("未知 op: {other}"))),
    };

    respond(&order, applied);
}

fn bad_request(message: &str) -> Fail {
    Fail::Raw(serde_json::json!({
        "kind": "BadRequest", "detail": null, "message": message,
    }))
}
