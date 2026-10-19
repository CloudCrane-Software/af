//! 可运行的门控演示:走一遍完整生命周期,并演示非法转换/门控缺失的错误文本。
//! 运行:cargo run -p af-workorder --example gate_demo

use af_workorder::{
    BudgetVoucher, GraphVersion, LineBinding, OrderState, Priority, Sla, Transition, WorkArtifact,
    WorkOrder,
};

fn main() {
    let mut wo = WorkOrder::new(
        "wo-demo",
        GraphVersion::new("g-3"),
        Sla::new(Some("2026-11-01T00:00:00Z".into()), Priority::High),
    );
    println!(
        "状态:{:?}(graph_version={})",
        wo.state(),
        wo.graph_version().as_str()
    );

    // ① 什么都不给就 release:门控列出全部缺失
    println!(
        "\n[release 未绑产线/未签凭证] → {}",
        wo.release().unwrap_err()
    );

    // ② 只绑产线,凭证未签名:点名哪张凭证没签
    wo.bind_line(LineBinding {
        line_id: "line-A".into(),
        line_version: "lv-7".into(),
    })
    .unwrap();
    wo.attach_budget_voucher(BudgetVoucher {
        voucher_id: "bv-9".into(),
        amount: 500,
        signed: false,
    })
    .unwrap();
    println!(
        "[凭证 bv-9 未签名]          → {}",
        wo.release().unwrap_err()
    );

    // ③ 跳步:created 直接 complete,状态机拒绝
    println!(
        "\n[created 跳步 complete]      → {}",
        wo.transition(Transition::Complete).unwrap_err()
    );

    // ④ 签发凭证后走完全程
    wo.attach_budget_voucher(BudgetVoucher {
        voucher_id: "bv-9".into(),
        amount: 500,
        signed: true,
    })
    .unwrap();
    wo.release().unwrap();
    println!("\nrelease ✅ → {:?}", wo.state());
    wo.start().unwrap();
    let mut wo_bare = wo.clone(); // 无工件副本,用来演示 close 门控
    wo.attach_artifact(WorkArtifact {
        artifact_id: "fa-1".into(),
        is_final: true,
        qualified: true,
    })
    .unwrap();
    wo.complete().unwrap();
    wo_bare.complete().unwrap();
    println!(
        "[close 无合格终态工件]       → {}",
        wo_bare.close().unwrap_err()
    );
    wo.close().unwrap();
    println!("close   ✅ → {:?}", wo.state());
    assert_eq!(wo.state(), OrderState::Closed);
    println!("\n履历:{:?}", wo.history());
}
