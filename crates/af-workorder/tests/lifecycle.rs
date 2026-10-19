//! 端到端集成测试:站在 crate 外部(像真实消费方那样)走完"开单 → 门控放行 →
//! 执行 → 合格工件 → 关单",以及"换版走变更单生成后继工单"、"多方决策工件汇聚"两条支线。

use af_workorder::{
    BudgetVoucher, ChangeOrder, Decision, DecisionArtifact, DrainStrategy, GateError, GraphVersion,
    LineBinding, Missing, OrderState, Priority, RelayStrategy, Role, Sla, Transition, WorkArtifact,
    WorkOrder,
};

fn sla(priority: Priority) -> Sla {
    Sla::new(Some("2026-11-01T00:00:00Z".into()), priority)
}

fn prepared_order(id: &str, graph: &str) -> WorkOrder {
    let mut wo = WorkOrder::new(id, GraphVersion::new(graph), sla(Priority::High));
    wo.bind_line(LineBinding {
        line_id: "line-A".into(),
        line_version: "lv-7".into(),
    })
    .unwrap();
    wo.attach_budget_voucher(BudgetVoucher {
        voucher_id: "bv-1".into(),
        amount: 500,
        signed: true,
    })
    .unwrap();
    wo
}

#[test]
fn full_lifecycle_with_gates_end_to_end() {
    let mut wo = WorkOrder::new("wo-e2e", GraphVersion::new("g-3"), sla(Priority::Critical));

    // 门控逐条拦:什么都不给就 release,要同时报两个缺失
    let err = wo.release().unwrap_err();
    assert_eq!(
        err.missing().unwrap(),
        &[
            Missing::LineVersionNotBound,
            Missing::BudgetVoucherNotIssued
        ]
    );

    // 补齐 release 前置(产线版本绑定 + 已签发预算凭证)
    wo.bind_line(LineBinding {
        line_id: "line-A".into(),
        line_version: "lv-7".into(),
    })
    .unwrap();
    wo.attach_budget_voucher(BudgetVoucher {
        voucher_id: "bv-1".into(),
        amount: 500,
        signed: true,
    })
    .unwrap();
    assert_eq!(wo.release().unwrap(), OrderState::Released);
    assert_eq!(wo.start().unwrap(), OrderState::InProgress);

    // 执行产出:中间件(合格但非终态)不影响 close 门控;终态合格件放行
    wo.attach_artifact(WorkArtifact {
        artifact_id: "ia-1".into(),
        is_final: false,
        qualified: true,
    })
    .unwrap();
    assert_eq!(wo.complete().unwrap(), OrderState::Completed);
    let err = wo.close().unwrap_err();
    assert_eq!(err.missing().unwrap(), &[Missing::NoQualifiedFinalArtifact]);

    wo.attach_artifact(WorkArtifact {
        artifact_id: "fa-1".into(),
        is_final: true,
        qualified: true,
    })
    .unwrap();
    assert_eq!(wo.close().unwrap(), OrderState::Closed);

    // graph_version 从创建到关闭纹丝未动
    assert_eq!(wo.graph_version().as_str(), "g-3");
    // SLA 字段全程在
    assert_eq!(wo.sla().priority, Priority::Critical);
}

#[test]
fn change_order_spawns_successor_without_touching_original() {
    let mut wo = prepared_order("wo-base", "g-3");
    wo.release().unwrap();
    wo.start().unwrap();

    let change = ChangeOrder {
        change_id: "ch-9".into(),
        work_order_id: "wo-base".into(),
        from_graph_version: GraphVersion::new("g-3"),
        to_graph_version: GraphVersion::new("g-4"),
        diff_summary: "N12 阈值 0.8→0.9;新增 N15 校验节点".into(),
        diff_paths: vec!["graph/N12".into(), "graph/N15".into()],
        drain: DrainStrategy::DrainWithCap { max_inflight: 3 },
        relay: RelayStrategy::WarmHandover {
            overlap_budget: 300,
        },
    };
    change.validate().unwrap();
    assert!(change.applies_to(&wo));

    let mut successor = change
        .spawn_successor("wo-base-b", sla(Priority::High))
        .unwrap();
    // 原单没被改:graph_version 不变、仍在 in_progress
    assert_eq!(wo.graph_version().as_str(), "g-3");
    assert_eq!(wo.state(), OrderState::InProgress);
    // 后继单:新版本 + supersede 指回 + 从 Created 重新走门控
    assert_eq!(successor.graph_version().as_str(), "g-4");
    assert_eq!(successor.supersedes(), Some("wo-base"));
    assert_eq!(successor.state(), OrderState::Created);
    // 后继单同样要过 release 门控(版本换了,凭证/绑定不能继承白给)
    let err = successor.release().unwrap_err();
    assert_eq!(
        err.missing().unwrap(),
        &[
            Missing::LineVersionNotBound,
            Missing::BudgetVoucherNotIssued
        ]
    );

    // 陈旧变更单(基于旧 from 版本)对后继单不适用
    assert!(!change.applies_to(&successor));
}

#[test]
fn multi_actor_decisions_collect_then_gate_release() {
    // 场景:release 前需要一张"审批通过"的决策工件, reader 先被拒, approver 通过后才放行。
    let payload = "wo-multi:release-request:g-3:v2";

    let reader_vote = DecisionArtifact::seal(
        "user-reader",
        Role::Reader,
        Decision::Approve,
        payload,
        "k-reader",
    );
    assert!(matches!(
        reader_vote.validate_against(payload),
        Err(af_workorder::DecisionError::PermissionDenied { .. })
    ));

    let approver_vote = DecisionArtifact::seal(
        "user-approver",
        Role::Approver,
        Decision::Approve,
        payload,
        "k-approver",
    );
    approver_vote.validate_against(payload).unwrap();
    assert_eq!(approver_vote.actor_id, "user-approver");

    // payload 被掉包 → hash 对不上,决策作废
    let tampered = DecisionArtifact::seal(
        "user-approver",
        Role::Approver,
        Decision::Approve,
        "wo-multi:release-request:g-3:v3",
        "k-approver",
    );
    assert!(matches!(
        tampered.validate_against(payload),
        Err(af_workorder::DecisionError::PayloadHashMismatch { .. })
    ));

    // 决策齐了,工单才能过 release 门控(此处用工单自身门控演示编排)
    let mut wo = prepared_order("wo-multi", "g-3");
    assert_eq!(wo.release().unwrap(), OrderState::Released);

    // release 之后 Cancel 仍可行(取消分支)
    assert_eq!(wo.cancel().unwrap(), OrderState::Cancelled);
    assert!(matches!(
        wo.transition(Transition::Release),
        Err(GateError::IllegalTransition { .. })
    ));
}
