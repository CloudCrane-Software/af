//! 引擎级集成测试：每个用例起一个 af-engine 子进程，stdin 喂一行 JSON。
//! 等待子进程退出用 `wait_with_output`，不使用 sleep，也不读系统钟。

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use af_workorder::{
    decode_work_order_value, BudgetVoucher, Decision, DecisionArtifact, DecisionError, GateError,
    GraphVersion, LineBinding, OrderDecodeError, OrderState, Role, WorkArtifact, WorkOrder,
};
use serde_json::{json, Value};

/// 与引擎 `approve_decision` 使用同一条绑定：`decision_payload(graph_version)`。
fn bound_payload(order: &WorkOrder) -> String {
    order.decision_payload(order.graph_version().as_str())
}

fn created() -> WorkOrder {
    serde_json::from_value(json!({
        "id": "wo-1",
        "stable_id": "sid-f905c3b464315a0a05fe2682f64a93aa2194016459b62a5b554f8d60692f451f",
        "run_id": "run-eng-1",
        "state": "Created",
        "graph_version": "g-3",
        "line": null,
        "budget_voucher": null,
        "artifacts": [],
        "sla": {"deadline": "2026-11-01T00:00:00Z", "priority": "High"},
        "supersedes": null,
        "history": []
    }))
    .expect("Created 快照应能按 af-workorder 的 serde 还原")
}

fn line() -> LineBinding {
    LineBinding {
        line_id: "line-A".into(),
        line_version: "lv-7".into(),
    }
}

fn voucher(signed: bool) -> BudgetVoucher {
    BudgetVoucher {
        voucher_id: "bv-1".into(),
        amount: 1000,
        signed,
    }
}

fn artifact(id: &str, is_final: bool, qualified: bool) -> WorkArtifact {
    WorkArtifact {
        artifact_id: id.into(),
        is_final,
        qualified,
    }
}

fn bound() -> WorkOrder {
    let mut wo = created();
    wo.bind_line(line()).expect("Created 可绑产线");
    wo.attach_budget_voucher(voucher(true))
        .expect("Created 可登记凭证");
    wo
}

fn released() -> WorkOrder {
    let mut wo = bound();
    wo.release().expect("前置齐备应放行");
    wo
}

fn in_progress() -> WorkOrder {
    let mut wo = released();
    wo.start().expect("Released 可开工");
    wo
}

fn in_progress_with_final() -> WorkOrder {
    let mut wo = in_progress();
    wo.attach_artifact(artifact("fa-1", true, true))
        .expect("InProgress 可挂工件");
    wo
}

fn completed_bare() -> WorkOrder {
    let mut wo = in_progress();
    wo.complete().expect("InProgress 可完工");
    wo
}

fn ready_to_close() -> WorkOrder {
    let mut wo = in_progress_with_final();
    wo.complete().expect("InProgress 可完工");
    wo
}

fn approver() -> DecisionArtifact {
    let order = created();
    DecisionArtifact::seal(
        "user-approver",
        Role::Approver,
        Decision::Approve,
        &bound_payload(&order),
        "k-approver",
    )
}

fn reader() -> DecisionArtifact {
    let order = created();
    DecisionArtifact::seal(
        "user-reader",
        Role::Reader,
        Decision::Approve,
        &bound_payload(&order),
        "k-reader",
    )
}

/// cargo 1.99 在集成测试运行时注入二进制路径，不提供给 `env!`。
fn engine_bin() -> PathBuf {
    for key in ["CARGO_BIN_EXE_af_engine", "CARGO_BIN_EXE_af-engine"] {
        if let Ok(path) = std::env::var(key) {
            return PathBuf::from(path);
        }
    }
    let found: Vec<(String, String)> = std::env::vars()
        .filter(|(key, _)| {
            key.contains("BIN_EXE") || key.contains("af_engine") || key.contains("af-engine")
        })
        .collect();
    panic!("未找到 af-engine 二进制。相关环境变量: {found:?}");
}

fn invoke_raw(bytes: &[u8]) -> String {
    let mut child = Command::new(engine_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("启动 af-engine");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        stdin.write_all(bytes).expect("写入 stdin");
    }
    let output = child.wait_with_output().expect("等待 af-engine");
    let stdout = String::from_utf8(output.stdout).expect("stdout 应为 utf-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr 应为 utf-8");
    assert!(
        output.status.success(),
        "退出码应为 0，实际 {:?}，stdout={stdout:?}，stderr={stderr:?}",
        output.status.code()
    );
    assert!(
        stderr.is_empty(),
        "stderr 应为空，实际 {stderr}\nstdout={stdout}"
    );
    assert!(stdout.ends_with('\n'), "stdout 应以换行结束: {stdout:?}");
    assert_eq!(stdout.lines().count(), 1, "stdout 应恰好一行: {stdout:?}");
    stdout
}

fn parse_stdout(stdout: &str) -> Value {
    let value: Value = serde_json::from_str(stdout.trim_end())
        .unwrap_or_else(|e| panic!("stdout 不是 JSON: {e}\n{stdout}"));
    assert_eq!(value["engine_version"], env!("CARGO_PKG_VERSION"));
    match value["ok"].as_bool() {
        Some(true) => {
            assert!(value.get("order").is_some(), "放行应带 order: {value}");
            assert!(value.get("error").is_none(), "放行不带 error: {value}");
        }
        Some(false) => {
            assert!(value.get("error").is_some(), "拒绝应带 error: {value}");
            assert!(value.get("order").is_none(), "拒绝不带 order: {value}");
        }
        _ => panic!("ok 必须是 bool: {value}"),
    }
    value
}

fn invoke(request: &str) -> Value {
    parse_stdout(&invoke_line(request))
}

fn invoke_line(request: &str) -> String {
    let mut bytes = request.as_bytes().to_vec();
    if !request.ends_with('\n') {
        bytes.push(b'\n');
    }
    invoke_raw(&bytes)
}

fn request_line(op: &str, order: &WorkOrder, extra: Value) -> String {
    let mut body = json!({
        "op": op,
        "order": order,
    });
    let Value::Object(extra) = extra else {
        panic!("额外参数必须是 JSON 对象");
    };
    let obj = body.as_object_mut().expect("请求体是对象");
    for (key, value) in extra {
        obj.insert(key, value);
    }
    serde_json::to_string(&body).expect("请求可序列化")
}

fn call(op: &str, order: &WorkOrder, extra: Value) -> Value {
    invoke(&request_line(op, order, extra))
}

fn assert_same_order(got: &Value, expected: &WorkOrder) {
    assert_eq!(got["ok"], true);
    assert_eq!(
        got["order"],
        serde_json::to_value(expected).expect("订单可序列化")
    );
}

fn assert_gate(got: &Value, err: &GateError) {
    assert_eq!(got["ok"], false);
    assert_eq!(got["error"]["kind"], "Gate");
    assert_eq!(
        got["error"]["gate"],
        serde_json::to_value(err).expect("GateError 可序列化"),
        "gate 应是 af-workorder 的 serde 原样"
    );
}

fn assert_decision(got: &Value, err: &DecisionError) {
    assert_eq!(got["ok"], false);
    assert_eq!(got["error"]["kind"], "Decision");
    assert_eq!(
        got["error"]["decision"],
        serde_json::to_value(err).expect("DecisionError 可序列化"),
        "decision 应是 af-workorder 的 serde 原样"
    );
}

fn assert_bad(got: &Value, message_has: &str) {
    assert_eq!(got["ok"], false);
    assert_eq!(got["error"]["kind"], "BadRequest");
    let message = got["error"]["message"]
        .as_str()
        .expect("BadRequest.message 是字符串");
    assert!(
        message.contains(message_has),
        "message 应包含 {message_has}，实际 {message}"
    );
}

fn assert_engine_version(got: &Value) {
    let version = got["engine_version"]
        .as_str()
        .expect("engine_version 必须是字符串");
    assert_eq!(version, env!("CARGO_PKG_VERSION"));
    assert!(!version.is_empty(), "engine_version 必须非空");
}

fn assert_upgrade(got: &Value, min: &str) {
    assert_eq!(got["ok"], false);
    assert_eq!(got["error"]["kind"], "BadRequest");
    assert!(got.get("order").is_none(), "版本拒绝不带 order: {got}");
    assert_engine_version(got);
    let message = got["error"]["message"]
        .as_str()
        .expect("BadRequest.message 是字符串");
    assert!(message.contains("请升级引擎"), "应提示升级，实际 {message}");
    assert!(
        message.contains(min),
        "message 应包含下限 {min}，实际 {message}"
    );
    assert!(
        message.contains(env!("CARGO_PKG_VERSION")),
        "message 应包含本二进制版本，实际 {message}"
    );
}

fn assert_min_unparseable(got: &Value) {
    assert_eq!(got["ok"], false);
    assert_eq!(got["error"]["kind"], "BadRequest");
    assert!(got.get("order").is_none(), "解析失败不带 order: {got}");
    assert_engine_version(got);
    let message = got["error"]["message"]
        .as_str()
        .expect("BadRequest.message 是字符串");
    assert!(
        message.contains("无法解析"),
        "看不懂的下限应拒绝，实际 {message}"
    );
}

#[test]
fn bind_line_accepts_created_and_rejects_after_release() {
    let wo = created();
    let mut next = wo.clone();
    next.bind_line(line()).unwrap();
    let got = call("bind_line", &wo, json!({"line": line()}));
    assert_same_order(&got, &next);
    assert_eq!(
        serde_json::from_value::<WorkOrder>(got["order"].clone())
            .unwrap()
            .graph_version(),
        &GraphVersion::new("g-3")
    );

    let wo = released();
    let err = wo.clone().bind_line(line()).unwrap_err();
    let got = call("bind_line", &wo, json!({"line": line()}));
    assert_gate(&got, &err);
    assert_eq!(got["error"]["gate"]["MutationForbidden"]["op"], "bind_line");
}

#[test]
fn attach_voucher_accepts_created_and_rejects_after_release() {
    let wo = created();
    let mut next = wo.clone();
    next.attach_budget_voucher(voucher(true)).unwrap();
    let got = call("attach_voucher", &wo, json!({"voucher": voucher(true)}));
    assert_same_order(&got, &next);

    let wo = released();
    let err = wo.clone().attach_budget_voucher(voucher(true)).unwrap_err();
    let got = call("attach_voucher", &wo, json!({"voucher": voucher(true)}));
    assert_gate(&got, &err);
    assert_eq!(
        got["error"]["gate"]["MutationForbidden"]["op"],
        "attach_budget_voucher"
    );
}

#[test]
fn unsigned_voucher_attaches_and_release_names_it() {
    let mut wo = created();
    wo.bind_line(line()).unwrap();
    let mut next = wo.clone();
    next.attach_budget_voucher(voucher(false)).unwrap();
    let got = call("attach_voucher", &wo, json!({"voucher": voucher(false)}));
    assert_same_order(&got, &next);

    let err = next.clone().release().unwrap_err();
    let got = call("release", &next, json!({}));
    assert_gate(&got, &err);
    assert_eq!(
        got["error"]["gate"]["PreconditionsNotMet"]["missing"],
        json!([{"BudgetVoucherNotSigned": {"voucher_id": "bv-1"}}])
    );
}

#[test]
fn release_accepts_prepared_order_and_lists_both_gaps() {
    let wo = bound();
    let mut next = wo.clone();
    next.release().unwrap();
    assert_same_order(&call("release", &wo, json!({})), &next);
    assert_eq!(next.state(), OrderState::Released);

    let wo = created();
    let err = wo.clone().release().unwrap_err();
    let got = call("release", &wo, json!({}));
    assert_gate(&got, &err);
    assert_eq!(
        got["error"]["gate"]["PreconditionsNotMet"]["missing"],
        json!(["LineVersionNotBound", "BudgetVoucherNotIssued"])
    );
}

#[test]
fn start_accepts_released_and_rejects_created() {
    let wo = released();
    let mut next = wo.clone();
    next.start().unwrap();
    assert_same_order(&call("start", &wo, json!({})), &next);
    assert_eq!(next.state(), OrderState::InProgress);

    let wo = created();
    let err = wo.clone().start().unwrap_err();
    let got = call("start", &wo, json!({}));
    assert_gate(&got, &err);
    assert_eq!(got["error"]["gate"]["IllegalTransition"]["from"], "Created");
    assert_eq!(
        got["error"]["gate"]["IllegalTransition"]["to"],
        "InProgress"
    );
    assert_eq!(
        got["error"]["gate"]["IllegalTransition"]["allowed"],
        json!(["Release", "Cancel"])
    );
}

#[test]
fn attach_artifact_accepts_in_progress_and_rejects_created() {
    let wo = in_progress();
    let mut next = wo.clone();
    next.attach_artifact(artifact("ia-1", false, true)).unwrap();
    let got = call(
        "attach_artifact",
        &wo,
        json!({"artifact": artifact("ia-1", false, true)}),
    );
    assert_same_order(&got, &next);

    let wo = created();
    let err = wo
        .clone()
        .attach_artifact(artifact("fa-1", true, true))
        .unwrap_err();
    let got = call(
        "attach_artifact",
        &wo,
        json!({"artifact": artifact("fa-1", true, true)}),
    );
    assert_gate(&got, &err);
    assert_eq!(
        got["error"]["gate"]["MutationForbidden"]["op"],
        "attach_artifact"
    );
}

#[test]
fn complete_accepts_in_progress_and_rejects_released() {
    let wo = in_progress();
    let mut next = wo.clone();
    next.complete().unwrap();
    assert_same_order(&call("complete", &wo, json!({})), &next);
    assert_eq!(next.state(), OrderState::Completed);

    let wo = released();
    let err = wo.clone().complete().unwrap_err();
    assert_gate(&call("complete", &wo, json!({})), &err);
}

#[test]
fn close_accepts_qualified_final_and_rejects_without_it() {
    let wo = ready_to_close();
    let mut next = wo.clone();
    next.close().unwrap();
    assert_same_order(&call("close", &wo, json!({})), &next);
    assert_eq!(next.state(), OrderState::Closed);

    let wo = completed_bare();
    let err = wo.clone().close().unwrap_err();
    let got = call("close", &wo, json!({}));
    assert_gate(&got, &err);
    assert_eq!(
        got["error"]["gate"]["PreconditionsNotMet"]["missing"],
        json!(["NoQualifiedFinalArtifact"])
    );
}

#[test]
fn close_from_in_progress_is_illegal_even_with_final_artifact() {
    let wo = in_progress_with_final();
    assert!(wo.artifacts().iter().any(|a| a.is_final && a.qualified));
    let err = wo.clone().close().unwrap_err();
    assert!(matches!(err, GateError::IllegalTransition { .. }));
    let got = call("close", &wo, json!({}));
    assert_gate(&got, &err);
    assert!(got["error"]["gate"].get("PreconditionsNotMet").is_none());
}

#[test]
fn cancel_accepts_created_and_rejects_completed() {
    let wo = created();
    let mut next = wo.clone();
    next.cancel().unwrap();
    assert_same_order(&call("cancel", &wo, json!({})), &next);
    assert_eq!(next.state(), OrderState::Cancelled);

    let wo = completed_bare();
    let err = wo.clone().cancel().unwrap_err();
    assert_gate(&call("cancel", &wo, json!({})), &err);
}

#[test]
fn approve_decision_accepts_approver_and_returns_same_order() {
    let wo = released();
    let payload = bound_payload(&wo);
    approver().validate_against(&payload).unwrap();
    let got = call(
        "approve_decision",
        &wo,
        json!({"artifact": approver(), "payload": payload}),
    );
    assert_same_order(&got, &wo);
    assert_eq!(got["order"]["state"], "Released");
}

#[test]
fn approve_decision_rejects_reader() {
    let wo = created();
    let payload = bound_payload(&wo);
    let art = reader();
    let err = art.validate_against(&payload).unwrap_err();
    assert!(matches!(err, DecisionError::PermissionDenied { .. }));
    let got = call(
        "approve_decision",
        &wo,
        json!({"artifact": art, "payload": payload}),
    );
    assert_decision(&got, &err);
}

#[test]
fn approve_decision_rejects_hash_mismatch() {
    let wo = created();
    let payload = bound_payload(&wo);
    // 请求 payload 已是这张订单的现算值，工件却签在另一段字符串上。
    let art = DecisionArtifact::seal(
        "user-approver",
        Role::Approver,
        Decision::Approve,
        "不是这张订单的 payload",
        "k-approver",
    );
    let err = art.validate_against(&payload).unwrap_err();
    assert!(matches!(err, DecisionError::PayloadHashMismatch { .. }));
    let got = call(
        "approve_decision",
        &wo,
        json!({"artifact": art, "payload": payload}),
    );
    assert_decision(&got, &err);
}

/// 发现 F5：工件与自带 payload 互相一致时，换一张订单仍然被接受。
/// 引擎必须按订单现算 `decision_payload`（stable_id + graph_version），拒绝这种请求。
/// 换 `run_id` 不改变绑定，同一工件仍然通过。
#[test]
fn approve_decision_rejects_matching_artifact_on_the_wrong_order() {
    let victim = created();
    let payload = bound_payload(&victim);
    let artifact = DecisionArtifact::seal(
        "user-approver",
        Role::Approver,
        Decision::Approve,
        &payload,
        "k-approver",
    );
    artifact
        .validate_against(&payload)
        .expect("工件对这份 payload 成立");

    let mut other_id = serde_json::to_value(&victim).unwrap();
    other_id["stable_id"] = json!("sid-other");
    let other_id: WorkOrder = serde_json::from_value(other_id).unwrap();
    assert_ne!(bound_payload(&other_id), payload);
    let got = call(
        "approve_decision",
        &other_id,
        json!({"artifact": artifact, "payload": payload}),
    );
    assert_bad(&got, "payload 与订单现算的 decision_payload 不一致");

    let mut other_graph = serde_json::to_value(&victim).unwrap();
    other_graph["graph_version"] = json!("g-9");
    let other_graph: WorkOrder = serde_json::from_value(other_graph).unwrap();
    assert_ne!(bound_payload(&other_graph), payload);
    let got = call(
        "approve_decision",
        &other_graph,
        json!({"artifact": artifact, "payload": payload}),
    );
    assert_bad(&got, "payload 与订单现算的 decision_payload 不一致");

    // 同一张订单上，调用方改口一段自洽但不属于本单的 payload，同样拒绝。
    let foreign = "wo-1:release-request:g-3";
    let foreign_art = DecisionArtifact::seal(
        "user-approver",
        Role::Approver,
        Decision::Approve,
        foreign,
        "k-approver",
    );
    foreign_art.validate_against(foreign).unwrap();
    let got = call(
        "approve_decision",
        &victim,
        json!({"artifact": foreign_art, "payload": foreign}),
    );
    assert_bad(&got, "payload 与订单现算的 decision_payload 不一致");

    let mut other_run = serde_json::to_value(&victim).unwrap();
    other_run["run_id"] = json!("run-CAN-other");
    let other_run: WorkOrder = serde_json::from_value(other_run).unwrap();
    assert_eq!(bound_payload(&other_run), payload);
    let got = call(
        "approve_decision",
        &other_run,
        json!({"artifact": artifact, "payload": payload}),
    );
    assert_same_order(&got, &other_run);
}

#[test]
fn unknown_op_and_wrong_case_are_bad_request() {
    let got = invoke(r#"{"op":"frob"}"#);
    assert_bad(&got, "未知 op: frob");

    let got = invoke(r#"{"op":"Release","order":{"state":"Flying"}}"#);
    assert_bad(&got, "未知 op: Release");
}

#[test]
fn bad_json_empty_and_non_object_are_bad_request() {
    assert_bad(&invoke(""), "空请求");
    assert_bad(&invoke("   \n"), "空请求");
    assert_bad(&invoke("{"), "非法 JSON");
    assert_bad(&invoke("not-json"), "非法 JSON");
    assert_bad(&invoke("null"), "请求必须是 JSON 对象");
    assert_bad(&invoke("[]"), "请求必须是 JSON 对象");
    assert_bad(&invoke("1"), "请求必须是 JSON 对象");
    assert_bad(&invoke(r#""release""#), "请求必须是 JSON 对象");
}

#[test]
fn field_type_and_snapshot_errors_are_bad_request() {
    let got = call(
        "attach_voucher",
        &created(),
        json!({"voucher": {"voucher_id":"bv-1","amount":"1000","signed":true}}),
    );
    assert_bad(&got, "字段 voucher 类型错误");

    let got = call("bind_line", &created(), json!({"line": null}));
    assert_bad(&got, "字段 line 类型错误");

    let mut order = serde_json::to_value(created()).unwrap();
    order["state"] = json!("Flying");
    let got = invoke(&serde_json::to_string(&json!({"op":"release","order": order})).unwrap());
    assert_bad(&got, "订单快照无法反序列化");

    let got = invoke(r#"{"op":1}"#);
    assert_bad(&got, "字段 op 必须是字符串");
}

/// 拒绝式信封必须带着 field 与 detail 出去。压成 BadRequest.message 会丢掉结构。
#[test]
fn rejection_envelope_keeps_field_and_detail() {
    let mut missing_id = serde_json::to_value(created()).unwrap();
    missing_id.as_object_mut().unwrap().remove("stable_id");
    assert_rejection_passthrough("release", &missing_id);

    let mut blank_id = serde_json::to_value(created()).unwrap();
    blank_id["stable_id"] = json!("");
    assert_rejection_passthrough("release", &blank_id);

    // 前置齐备的单加上两个未知字段。若被静默丢掉,release 会成功并把有损快照送回去。
    let mut injected = serde_json::to_value(bound()).unwrap();
    injected["injected_beta"] = json!("keep-me");
    injected["injected_alpha"] = json!(1);
    let got = assert_rejection_passthrough("release", &injected);
    let field = got["error"]["field"].as_str().expect("field 是字符串");
    assert!(field.contains("injected_alpha"), "{got}");
    assert!(field.contains("injected_beta"), "{got}");
    assert_eq!(field, "injected_alpha, injected_beta");
}

fn assert_rejection_passthrough(op: &str, order: &Value) -> Value {
    let err = decode_work_order_value(order.clone()).expect_err("库应拒绝这份快照");
    let OrderDecodeError::Rejected(envelope) = err else {
        panic!("库应给出拒绝式信封,实际 {err:?}");
    };
    let got =
        invoke(&serde_json::to_string(&json!({"op": op, "order": order})).expect("请求可序列化"));
    assert_eq!(got["ok"], false, "{got}");
    assert!(got.get("order").is_none(), "拒绝不能带回有损订单: {got}");
    assert_eq!(got["error"]["kind"], "RejectionEnvelope", "{got}");
    assert!(
        got["error"].get("message").is_none(),
        "拒绝式信封不能压成 message: {got}"
    );
    assert_eq!(got["error"]["field"], envelope.field, "{got}");
    assert_eq!(got["error"]["detail"], envelope.detail, "{got}");
    got
}

#[test]
fn missing_op_order_and_params_are_bad_request() {
    assert_bad(&invoke("{}"), "缺少字段 op");
    assert_bad(&invoke(r#"{"op":"release"}"#), "缺少字段 order");
    assert_bad(&call("bind_line", &created(), json!({})), "缺少字段 line");
    assert_bad(
        &call("attach_voucher", &created(), json!({})),
        "缺少字段 voucher",
    );
    assert_bad(
        &call("attach_artifact", &in_progress(), json!({})),
        "缺少字段 artifact",
    );
    assert_bad(
        &call(
            "approve_decision",
            &created(),
            json!({"artifact": approver()}),
        ),
        "缺少字段 payload",
    );
    assert_bad(
        &call(
            "approve_decision",
            &created(),
            json!({"payload": bound_payload(&created())}),
        ),
        "缺少字段 artifact",
    );
}

#[test]
fn lifecycle_is_stateless_across_processes() {
    let mut local = created();

    let mut next = local.clone();
    next.bind_line(line()).unwrap();
    let remote: WorkOrder =
        serde_json::from_value(call("bind_line", &local, json!({"line": line()}))["order"].clone())
            .unwrap();
    assert_eq!(remote, next);
    local = remote;

    let mut next = local.clone();
    next.attach_budget_voucher(voucher(true)).unwrap();
    let remote: WorkOrder = serde_json::from_value(
        call("attach_voucher", &local, json!({"voucher": voucher(true)}))["order"].clone(),
    )
    .unwrap();
    assert_eq!(remote, next);
    local = remote;

    let mut next = local.clone();
    next.release().unwrap();
    let remote: WorkOrder =
        serde_json::from_value(call("release", &local, json!({}))["order"].clone()).unwrap();
    assert_eq!(remote, next);
    local = remote;

    let mut next = local.clone();
    next.start().unwrap();
    let remote: WorkOrder =
        serde_json::from_value(call("start", &local, json!({}))["order"].clone()).unwrap();
    assert_eq!(remote, next);
    local = remote;

    let mut next = local.clone();
    next.attach_artifact(artifact("ia-1", false, true)).unwrap();
    let remote: WorkOrder = serde_json::from_value(
        call(
            "attach_artifact",
            &local,
            json!({"artifact": artifact("ia-1", false, true)}),
        )["order"]
            .clone(),
    )
    .unwrap();
    assert_eq!(remote, next);
    local = remote;

    let mut next = local.clone();
    next.complete().unwrap();
    let remote: WorkOrder =
        serde_json::from_value(call("complete", &local, json!({}))["order"].clone()).unwrap();
    assert_eq!(remote, next);
    local = remote;

    let err = local.clone().close().unwrap_err();
    assert_gate(&call("close", &local, json!({})), &err);
    assert_eq!(local.state(), OrderState::Completed);

    let mut next = local.clone();
    next.attach_artifact(artifact("fa-1", true, true)).unwrap();
    let remote: WorkOrder = serde_json::from_value(
        call(
            "attach_artifact",
            &local,
            json!({"artifact": artifact("fa-1", true, true)}),
        )["order"]
            .clone(),
    )
    .unwrap();
    assert_eq!(remote, next);
    local = remote;

    let mut next = local.clone();
    next.close().unwrap();
    let remote: WorkOrder =
        serde_json::from_value(call("close", &local, json!({}))["order"].clone()).unwrap();
    assert_eq!(remote, next);
    assert_eq!(remote.state(), OrderState::Closed);
    assert_eq!(remote.graph_version().as_str(), "g-3");
    assert_eq!(remote.history().len(), 4);
    assert_eq!(remote.sla().priority, af_workorder::Priority::High);
}

#[test]
fn engine_version_ignores_runtime_env() {
    let mut child = Command::new(engine_bin())
        .env("CARGO_PKG_VERSION", "9.9.9")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("启动 af-engine");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        stdin.write_all(b"{}\n").expect("写入");
    }
    let output = child.wait_with_output().expect("等待");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: Value = serde_json::from_str(stdout.trim_end()).unwrap();
    assert_eq!(value["engine_version"], env!("CARGO_PKG_VERSION"));
    assert_ne!(value["engine_version"], "9.9.9");
    assert_eq!(value["engine_version"], "0.1.0");
}

#[test]
fn engine_version_min_accepts_current_lower_and_prerelease() {
    assert_eq!(env!("CARGO_PKG_VERSION"), "0.1.0");
    let wo = bound();
    let mut next = wo.clone();
    next.release().unwrap();
    for min in [
        env!("CARGO_PKG_VERSION"),
        "0.0.0",
        "0.1.0-rc.1",
        "0.1.0+build.7",
    ] {
        let got = call(
            "release",
            &wo,
            json!({"engine_version_min": min, "tag": "ignored"}),
        );
        assert_same_order(&got, &next);
        assert_engine_version(&got);
    }

    // 下限满足时，领域错误仍按原样返回。
    let wo = created();
    let err = wo.clone().release().unwrap_err();
    let got = call("release", &wo, json!({"engine_version_min": "0.0.1"}));
    assert_gate(&got, &err);
    assert_engine_version(&got);
}

#[test]
fn engine_version_min_rejects_when_engine_is_older_or_min_is_unusable() {
    assert_eq!(env!("CARGO_PKG_VERSION"), "0.1.0");
    let wo = bound();
    let mut would_release = wo.clone();
    would_release.release().unwrap();

    for min in ["0.1.1", "0.2.0", "1.0.0", "99.0.0"] {
        let got = call("release", &wo, json!({"engine_version_min": min}));
        assert_upgrade(&got, min);
    }

    // 版本不够时停在下限门，不进入订单解析，也不返回领域错误。
    let got =
        invoke(r#"{"op":"release","engine_version_min":"99.0.0","order":{"state":"Flying"}}"#);
    assert_upgrade(&got, "99.0.0");
    let message = got["error"]["message"].as_str().unwrap();
    assert!(
        !message.contains("反序列化") && !message.contains("Preconditions"),
        "版本拒绝不应混入领域错误: {message}"
    );

    let got = invoke(r#"{"op":"release","engine_version_min":"99.0.0"}"#);
    assert_upgrade(&got, "99.0.0");
    assert!(
        !got["error"]["message"]
            .as_str()
            .unwrap()
            .contains("缺少字段"),
        "版本拒绝应先于缺字段"
    );

    for raw in [
        r#"{"op":"release","engine_version_min":"latest","order":{}}"#,
        r#"{"op":"release","engine_version_min":"","order":{}}"#,
        r#"{"op":"release","engine_version_min":"0.1","order":{}}"#,
        r#"{"op":"release","engine_version_min":"v0.1.0","order":{}}"#,
        r#"{"op":"release","engine_version_min":"01.0.0","order":{}}"#,
        r#"{"op":"release","engine_version_min":"0.1.0-01","order":{}}"#,
    ] {
        assert_min_unparseable(&invoke(raw));
    }

    assert_bad(
        &invoke(r#"{"engine_version_min":1}"#),
        "字段 engine_version_min 必须是字符串",
    );
    assert_bad(
        &invoke(r#"{"op":"release","engine_version_min":null}"#),
        "字段 engine_version_min 必须是字符串",
    );
    assert_bad(
        &invoke(r#"{"engine_version_min":["0.1.0"]}"#),
        "字段 engine_version_min 必须是字符串",
    );
}

#[test]
fn engine_version_field_is_always_present() {
    let samples = [
        invoke(&request_line("release", &bound(), json!({}))),
        invoke(&request_line(
            "release",
            &bound(),
            json!({"engine_version_min": env!("CARGO_PKG_VERSION")}),
        )),
        invoke(r#"{"op":"frob"}"#),
        invoke(""),
        invoke("[]"),
        invoke(r#"{"op":"release","engine_version_min":"99.0.0"}"#),
        invoke(r#"{"engine_version_min":"not-a-version"}"#),
        invoke(r#"{"engine_version_min":1}"#),
    ];
    for got in &samples {
        assert_engine_version(got);
        assert!(got["ok"].is_boolean(), "带版本的响应仍有 ok: {got}");
    }
    assert_eq!(samples[0]["ok"], true);
    assert_eq!(samples[2]["ok"], false);
    assert_eq!(samples[5]["ok"], false);
}

#[test]
fn one_line_only_and_eof_without_newline() {
    let got = parse_stdout(&invoke_raw(br#"{"op":"frob"}"#));
    assert_bad(&got, "未知 op: frob");

    let got = parse_stdout(&invoke_raw(b"{\"op\":\"frob\"}\n{\"op\":\"release\"}\n"));
    assert_bad(&got, "未知 op: frob");
}

fn contract_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/contract")
}

fn samples() -> Vec<(&'static str, String)> {
    let flying = {
        let mut order = serde_json::to_value(created()).unwrap();
        order["state"] = json!("Flying");
        serde_json::to_string(&json!({"op":"release","order": order})).unwrap()
    };
    vec![
        (
            "bind_line_ok",
            request_line("bind_line", &created(), json!({"line": line()})),
        ),
        (
            "bind_line_forbidden",
            request_line("bind_line", &released(), json!({"line": line()})),
        ),
        (
            "attach_voucher_ok",
            request_line(
                "attach_voucher",
                &created(),
                json!({"voucher": voucher(true)}),
            ),
        ),
        (
            "attach_voucher_forbidden",
            request_line(
                "attach_voucher",
                &released(),
                json!({"voucher": voucher(true)}),
            ),
        ),
        ("release_ok", request_line("release", &bound(), json!({}))),
        (
            "release_missing",
            request_line("release", &created(), json!({})),
        ),
        (
            "release_unsigned",
            request_line(
                "release",
                &{
                    let mut wo = created();
                    wo.bind_line(line()).unwrap();
                    wo.attach_budget_voucher(voucher(false)).unwrap();
                    wo
                },
                json!({}),
            ),
        ),
        ("start_ok", request_line("start", &released(), json!({}))),
        (
            "start_illegal",
            request_line("start", &created(), json!({})),
        ),
        (
            "attach_artifact_ok",
            request_line(
                "attach_artifact",
                &in_progress(),
                json!({"artifact": artifact("ia-1", false, true)}),
            ),
        ),
        (
            "attach_artifact_forbidden",
            request_line(
                "attach_artifact",
                &created(),
                json!({"artifact": artifact("fa-1", true, true)}),
            ),
        ),
        (
            "complete_ok",
            request_line("complete", &in_progress(), json!({})),
        ),
        (
            "complete_illegal",
            request_line("complete", &released(), json!({})),
        ),
        (
            "close_ok",
            request_line("close", &ready_to_close(), json!({})),
        ),
        (
            "close_missing",
            request_line("close", &completed_bare(), json!({})),
        ),
        (
            "close_illegal",
            request_line("close", &in_progress_with_final(), json!({})),
        ),
        ("cancel_ok", request_line("cancel", &created(), json!({}))),
        (
            "cancel_illegal",
            request_line("cancel", &completed_bare(), json!({})),
        ),
        (
            "approve_decision_ok",
            request_line(
                "approve_decision",
                &released(),
                json!({"artifact": approver(), "payload": bound_payload(&released())}),
            ),
        ),
        (
            "approve_decision_denied",
            request_line(
                "approve_decision",
                &created(),
                json!({"artifact": reader(), "payload": bound_payload(&created())}),
            ),
        ),
        ("unknown_op", r#"{"op":"frob"}"#.to_string()),
        ("order_bad_state", flying),
        ("order_missing_stable_id", {
            let mut order = serde_json::to_value(created()).unwrap();
            order.as_object_mut().unwrap().remove("stable_id");
            serde_json::to_string(&json!({"op":"release","order": order})).unwrap()
        }),
        ("order_unknown_fields", {
            let mut order = serde_json::to_value(bound()).unwrap();
            order["injected_beta"] = json!("keep-me");
            order["injected_alpha"] = json!(1);
            serde_json::to_string(&json!({"op":"release","order": order})).unwrap()
        }),
        (
            "voucher_amount_type",
            request_line(
                "attach_voucher",
                &created(),
                json!({"voucher": {"voucher_id":"bv-1","amount":"1000","signed":true}}),
            ),
        ),
        ("missing_order", r#"{"op":"release"}"#.to_string()),
        ("op_not_string", r#"{"op":1}"#.to_string()),
        ("not_object", "[]".to_string()),
    ]
}

#[test]
fn contract_samples_match_engine() {
    let dir = contract_dir();
    let samples = samples();
    let mut seen_ops = Vec::new();
    for (name, request) in &samples {
        let req_path = dir.join(format!("{name}.request.json"));
        let resp_path = dir.join(format!("{name}.response.json"));
        let on_disk = fs::read_to_string(&req_path)
            .unwrap_or_else(|e| panic!("读取 {}：{e}", req_path.display()));
        assert!(
            !on_disk.trim().contains('\n'),
            "{name}.request.json 必须是单行 JSON"
        );
        // 语义比较（不逐字节）：serde_json 的 preserve_order 由 cedar-policy-core 经 feature
        // 统一开关，工作区/单 crate 构建键序不同——契约等价性按 Value 判定，键序不作契约。
        let disk_req: serde_json::Value = serde_json::from_str(on_disk.trim())
            .unwrap_or_else(|e| panic!("{name}.request.json 不是合法 JSON: {e}"));
        let built_req: serde_json::Value = serde_json::from_str(request)
            .unwrap_or_else(|e| panic!("{name} 构造串不是合法 JSON: {e}"));
        assert_eq!(disk_req, built_req, "{name} 请求样例与构造不一致");
        let expected = fs::read_to_string(&resp_path)
            .unwrap_or_else(|e| panic!("读取 {}：{e}", resp_path.display()));
        assert!(
            !expected.trim().contains('\n'),
            "{name}.response.json 必须是单行，与 stdout 一致"
        );
        let actual = invoke_line(request);
        let actual_v: serde_json::Value = serde_json::from_str(actual.trim())
            .unwrap_or_else(|e| panic!("{name} stdout 不是合法 JSON: {e}"));
        let expected_v: serde_json::Value = serde_json::from_str(expected.trim())
            .unwrap_or_else(|e| panic!("{name}.response.json 不是合法 JSON: {e}"));
        assert_eq!(actual_v, expected_v, "{name} stdout 与样例不一致");
        parse_stdout(&actual);
        if let Some(op) = serde_json::from_str::<Value>(request)
            .ok()
            .and_then(|v| v.get("op")?.as_str().map(str::to_string))
        {
            seen_ops.push(op);
        }
    }
    for op in [
        "bind_line",
        "attach_voucher",
        "release",
        "start",
        "attach_artifact",
        "complete",
        "close",
        "cancel",
        "approve_decision",
    ] {
        assert!(
            seen_ops.iter().any(|seen| seen == op),
            "契约样例缺少 op {op}"
        );
    }

    let order_path = dir.join("work_order.created.json");
    let order: Value = serde_json::from_str(
        &fs::read_to_string(&order_path).unwrap_or_else(|e| panic!("读取工单样例: {e}")),
    )
    .unwrap();
    assert_eq!(order, serde_json::to_value(created()).unwrap());
}

/// `VERSIONING.md`「操作步骤」代码块里的命令。干跑只校验它们存在。
const CHANNEL_STEPS: &[&str] = &[
    r#"cargo build -p af-engine --bin af-engine"#,
    r#"mkdir -p -- "$ENGINE_CHANNEL_DIR/bins" "$ENGINE_CHANNEL_DIR/pins""#,
    r#"cp -f -- "$ENGINE_BIN" "$ENGINE_CHANNEL_DIR/bins/$ENGINE_BUILD""#,
    r#"ln -sfn -- "$ENGINE_CHANNEL_DIR/bins/$ENGINE_BUILD" "$ENGINE_CHANNEL_DIR/canary""#,
    r#"readlink -f -- "$ENGINE_CHANNEL_DIR/canary""#,
    r#"test -x "$ENGINE_CHANNEL_DIR/canary""#,
    r#"rm -f -- "$ENGINE_CHANNEL_DIR/canary""#,
    r#"test ! -e "$ENGINE_CHANNEL_DIR/canary""#,
];

fn operational_steps(doc: &str) -> Vec<String> {
    let mut in_section = false;
    let mut in_fence = false;
    let mut steps = Vec::new();
    for line in doc.lines() {
        if !in_section {
            if line == "## 操作步骤" {
                in_section = true;
            }
            continue;
        }
        if !in_fence {
            if line == "```sh" {
                in_fence = true;
            } else if line.starts_with("## ") {
                break;
            }
            continue;
        }
        if line == "```" {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        steps.push(trimmed.to_string());
    }
    steps
}

fn assert_command_exists(cmd: &str) {
    let output = Command::new("bash")
        .args(["-c", r#"command -v "$1""#, "_", cmd])
        .output()
        .unwrap_or_else(|e| panic!("command -v {cmd}: {e}"));
    let stdout = String::from_utf8(output.stdout).expect("command -v stdout utf-8");
    let stderr = String::from_utf8(output.stderr).expect("command -v stderr utf-8");
    assert!(
        output.status.success(),
        "命令 {cmd} 不存在，stderr={stderr}"
    );
    assert!(!stdout.trim().is_empty(), "command -v {cmd} 没有给出位置");
}

#[test]
fn versioning_steps_dry_run_checks_commands_exist() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let doc = fs::read_to_string(manifest.join("VERSIONING.md")).expect("读取 VERSIONING.md");
    for phrase in [
        "EngineChannel",
        "新规则先发 canary",
        "该 run 全程用同版本",
        "摘除 canary 指针",
        "stable",
        "canary",
        "scripts/engine-channel-dry-run.sh",
    ] {
        assert!(doc.contains(phrase), "VERSIONING.md 应写明 {phrase}");
    }

    let steps = operational_steps(&doc);
    assert_eq!(
        steps,
        CHANNEL_STEPS
            .iter()
            .map(|step| (*step).to_string())
            .collect::<Vec<_>>()
    );
    for step in &steps {
        let cmd = step.split_whitespace().next().expect("步骤有命令名");
        assert!(
            !cmd.contains('|') && !cmd.contains('&') && !cmd.contains(';'),
            "操作步骤保持单命令，便于干跑: {step}"
        );
        assert_command_exists(cmd);
    }

    let scratch = std::env::temp_dir().join(format!("af-engine-w4f-{}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch);
    assert!(
        !scratch.exists(),
        "干跑开始前通道目录应不存在: {}",
        scratch.display()
    );

    let script = manifest.join("scripts/engine-channel-dry-run.sh");
    let output = Command::new("bash")
        .arg(&script)
        .env("ENGINE_CHANNEL_DIR", &scratch)
        .output()
        .expect("启动干跑脚本");
    let stdout = String::from_utf8(output.stdout).expect("干跑 stdout utf-8");
    let stderr = String::from_utf8(output.stderr).expect("干跑 stderr utf-8");
    assert!(
        output.status.success(),
        "干跑应成功，stderr={stderr}\nstdout={stdout}"
    );
    assert!(stderr.is_empty(), "干跑 stderr 应为空: {stderr}");
    for step in CHANNEL_STEPS {
        assert!(stdout.contains(step), "干跑输出应列出步骤 {step}\n{stdout}");
    }
    assert!(stdout.contains("dry-run: not-executed"), "{stdout}");
    assert!(stdout.contains("dry-run: ok"), "{stdout}");
    assert!(!stdout.contains("MISSING"), "干跑发现缺失命令: {stdout}");
    assert!(
        !scratch.exists(),
        "干跑不得创建通道目录 {}",
        scratch.display()
    );
}
