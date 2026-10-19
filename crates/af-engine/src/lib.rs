//! af-engine — 无状态工单判定。
//!
//! 一次进程读 stdin 一行请求，向 stdout 写一行响应，然后退出。
//! 订单快照整单进、整单出。字段形状就是 [`af_workorder`] 派生的 serde，
//! 本 crate 不重定义工单、门控错误或决策错误。
//!
//! # 判定落点
//!
//! 必经引擎（编排层把这些判定交给本进程）：
//! `release`、`close`、`approve_decision`。
//!
//! 允许编排层本地执行（本进程仍然提供同名 op）：
//! `bind_line`、`attach_voucher`、`start`、`attach_artifact`、`complete`、`cancel`。
//!
//! 线协议、参数和错误形状见 crate 根目录的 `README.md`。
//!
//! 请求可以带 `engine_version_min`（semver）。本二进制版本低于它时拒绝，并提示升级。
//! 字段缺失表示不设下限。字段存在但无法解析时拒绝，不把看不懂的下限当成没有下限。
//! stable / canary 两份二进制如何并存、如何回滚，见 crate 根目录的 `VERSIONING.md`。

mod version;

use af_workorder::{
    BudgetVoucher, DecisionArtifact, DecisionError, GateError, LineBinding, WorkArtifact, WorkOrder,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// 编译进二进制的包版本。cargo 在编译时注入 `CARGO_PKG_VERSION`。
/// 运行时的同名环境变量不参与响应：进程被编排拉起时通常没有这个变量，
/// 版本必须跟这一份二进制走。
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize)]
struct Response {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    order: Option<WorkOrder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<WireError>,
    engine_version: &'static str,
}

/// 协议错误带 `kind`。领域错误把 af-workorder 的派生 serde 原样放在
/// `gate` 或 `decision` 里，不改写变体名和字段。
#[derive(Serialize)]
#[serde(tag = "kind")]
enum WireError {
    BadRequest { message: String },
    Gate { gate: GateError },
    Decision { decision: DecisionError },
}

enum Op {
    BindLine,
    AttachVoucher,
    Release,
    Start,
    AttachArtifact,
    Complete,
    Close,
    Cancel,
    ApproveDecision,
}

fn bad(message: impl Into<String>) -> WireError {
    WireError::BadRequest {
        message: message.into(),
    }
}

fn render(response: &Response) -> String {
    serde_json::to_string(response).expect("响应只含可序列化字段")
}

/// 把一行请求变成一行 JSON 响应（不含末尾换行）。
///
/// 成功体是 `{"ok":true,"order":...,"engine_version":...}`。
/// 失败体是 `{"ok":false,"error":...,"engine_version":...}`，两者不同时出现。
pub fn handle_line(line: &str) -> String {
    render(&eval(line))
}

/// stdin 本身读失败时的协议错误。调用方仍把它写到 stdout。
pub fn bad_request(message: &str) -> String {
    render(&fail(bad(message)))
}

fn ok_order(order: WorkOrder) -> Response {
    Response {
        ok: true,
        order: Some(order),
        error: None,
        engine_version: ENGINE_VERSION,
    }
}

fn fail(error: WireError) -> Response {
    Response {
        ok: false,
        order: None,
        error: Some(error),
        engine_version: ENGINE_VERSION,
    }
}

fn eval(line: &str) -> Response {
    match eval_inner(line) {
        Ok(order) => ok_order(order),
        Err(error) => fail(error),
    }
}

/// 检查顺序固定，命中即返回：空行 → JSON → 对象 → `engine_version_min`（字段存在时）
/// → op → 订单快照 → 参数 → 领域判定。
fn eval_inner(line: &str) -> Result<WorkOrder, WireError> {
    let line = line.trim();
    if line.is_empty() {
        return Err(bad("空请求"));
    }
    let value: Value = serde_json::from_str(line).map_err(|e| bad(format!("非法 JSON: {e}")))?;
    let obj = value
        .as_object()
        .ok_or_else(|| bad("请求必须是 JSON 对象"))?;
    enforce_min_version(obj)?;
    let op = parse_op(obj)?;
    let mut order = parse_order(obj)?;
    dispatch(op, &mut order, obj)?;
    Ok(order)
}

/// 下限门在领域判定之前。版本不够时不返回订单错误，避免旧二进制用旧规则把请求判过去。
fn enforce_min_version(obj: &Map<String, Value>) -> Result<(), WireError> {
    let Some(value) = obj.get("engine_version_min") else {
        return Ok(());
    };
    let Some(min) = value.as_str() else {
        return Err(bad("字段 engine_version_min 必须是字符串"));
    };
    match version::check(ENGINE_VERSION, min) {
        Ok(()) => Ok(()),
        Err(version::VersionGate::TooOld) => Err(bad(format!(
            "引擎版本 {ENGINE_VERSION} 低于 engine_version_min {min}，请升级引擎"
        ))),
        Err(version::VersionGate::BadMin(reason)) => {
            Err(bad(format!("字段 engine_version_min 无法解析: {reason}")))
        }
        Err(version::VersionGate::BadEngine) => Err(bad("引擎版本无法解析，拒绝判定")),
    }
}

fn parse_op(obj: &Map<String, Value>) -> Result<Op, WireError> {
    let op = match obj.get("op") {
        None => return Err(bad("缺少字段 op")),
        Some(Value::String(s)) => s.as_str(),
        Some(_) => return Err(bad("字段 op 必须是字符串")),
    };
    match op {
        "bind_line" => Ok(Op::BindLine),
        "attach_voucher" => Ok(Op::AttachVoucher),
        "release" => Ok(Op::Release),
        "start" => Ok(Op::Start),
        "attach_artifact" => Ok(Op::AttachArtifact),
        "complete" => Ok(Op::Complete),
        "close" => Ok(Op::Close),
        "cancel" => Ok(Op::Cancel),
        "approve_decision" => Ok(Op::ApproveDecision),
        other => Err(bad(format!("未知 op: {other}"))),
    }
}

fn parse_order(obj: &Map<String, Value>) -> Result<WorkOrder, WireError> {
    let Some(value) = obj.get("order") else {
        return Err(bad("缺少字段 order"));
    };
    WorkOrder::deserialize(value).map_err(|e| bad(format!("订单快照无法反序列化: {e}")))
}

fn parse_field<T: DeserializeOwned>(obj: &Map<String, Value>, field: &str) -> Result<T, WireError> {
    let Some(value) = obj.get(field) else {
        return Err(bad(format!("缺少字段 {field}")));
    };
    T::deserialize(value).map_err(|e| bad(format!("字段 {field} 类型错误: {e}")))
}

fn gate(err: GateError) -> WireError {
    WireError::Gate { gate: err }
}

fn decision(err: DecisionError) -> WireError {
    WireError::Decision { decision: err }
}

fn dispatch(op: Op, order: &mut WorkOrder, obj: &Map<String, Value>) -> Result<(), WireError> {
    match op {
        Op::BindLine => {
            let line: LineBinding = parse_field(obj, "line")?;
            order.bind_line(line).map_err(gate)
        }
        Op::AttachVoucher => {
            let voucher: BudgetVoucher = parse_field(obj, "voucher")?;
            // 线名字是 attach_voucher。库方法仍叫 attach_budget_voucher，
            // MutationForbidden 里的 op 字符串保持库的原文。
            order.attach_budget_voucher(voucher).map_err(gate)
        }
        Op::Release => order.release().map(|_| ()).map_err(gate),
        Op::Start => order.start().map(|_| ()).map_err(gate),
        Op::AttachArtifact => {
            let artifact: WorkArtifact = parse_field(obj, "artifact")?;
            order.attach_artifact(artifact).map_err(gate)
        }
        Op::Complete => order.complete().map(|_| ()).map_err(gate),
        Op::Close => order.close().map(|_| ()).map_err(gate),
        Op::Cancel => order.cancel().map(|_| ()).map_err(gate),
        Op::ApproveDecision => {
            let artifact: DecisionArtifact = parse_field(obj, "artifact")?;
            let payload: String = parse_field(obj, "payload")?;
            // 工单上没有决策字段，release 门控也不读决策。校验通过后原样送回快照。
            artifact.validate_against(&payload).map_err(decision)?;
            Ok(())
        }
    }
}
