//! 工单一等对象:状态机 + 门控 + graph_version 不可变绑定 + SLA。
//!
//! # graph_version 不可变绑定怎么落地
//! `graph_version` 是 `WorkOrder` 的**私有字段,只提供读取器 [`WorkOrder::graph_version`],
//! 全 crate 不存在任何 setter 或 `&mut` 通路**;需要换图版本的场景一律走
//! [`crate::change::ChangeOrder`] 生成"后继工单"(`supersedes`),原工单保持封存。
//! 因此"不可变"是 API 层面的编译期保证,不是运行期约定。

use serde::{Deserialize, Serialize};

use crate::error::{GateError, Missing};
use crate::sla::Sla;
use crate::state::{OrderState, Transition};

/// 图谱版本号。内部字段私有,只能整体构造、整体比较,防止被当作可变字符串改。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GraphVersion(String);

impl GraphVersion {
    pub fn new(v: impl Into<String>) -> Self {
        Self(v.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 产线版本绑定(release 门控条件之一)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineBinding {
    pub line_id: String,
    pub line_version: String,
}

/// 预算凭证(release 门控条件之一;`signed == true` 才算"已签发")。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetVoucher {
    pub voucher_id: String,
    pub amount: u64,
    pub signed: bool,
}

/// 工件。close 门控要求存在 `is_final && qualified` 的工件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkArtifact {
    pub artifact_id: String,
    /// 是否终态工件
    pub is_final: bool,
    /// 是否检验合格
    pub qualified: bool,
}

/// 状态机迁移履历(按发生顺序)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub from: OrderState,
    pub to: OrderState,
}

/// 工单。字段全部私有,只能经方法操作,保证所有变更都过门控。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkOrder {
    id: String,
    state: OrderState,
    graph_version: GraphVersion,
    line: Option<LineBinding>,
    budget_voucher: Option<BudgetVoucher>,
    artifacts: Vec<WorkArtifact>,
    sla: Sla,
    /// 本单是哪张单的后继(由变更单生成时填)
    supersedes: Option<String>,
    history: Vec<HistoryRecord>,
}

impl WorkOrder {
    /// 新建工单:初始状态 Created,graph_version 在此一次性绑定。
    pub fn new(id: impl Into<String>, graph_version: GraphVersion, sla: Sla) -> Self {
        Self {
            id: id.into(),
            state: OrderState::Created,
            graph_version,
            line: None,
            budget_voucher: None,
            artifacts: Vec::new(),
            sla,
            supersedes: None,
            history: Vec::new(),
        }
    }

    /// 后继工单构造:由变更单生成,graph_version 直接绑新版本,并记录 supersede 关系。
    pub fn successor(
        id: impl Into<String>,
        graph_version: GraphVersion,
        sla: Sla,
        supersedes: impl Into<String>,
    ) -> Self {
        Self {
            supersedes: Some(supersedes.into()),
            ..Self::new(id, graph_version, sla)
        }
    }

    // ---------- 只读访问 ----------

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn state(&self) -> OrderState {
        self.state
    }

    /// graph_version 唯一读取途径;无 setter,见模块文档。
    pub fn graph_version(&self) -> &GraphVersion {
        &self.graph_version
    }

    pub fn sla(&self) -> &Sla {
        &self.sla
    }

    pub fn line(&self) -> Option<&LineBinding> {
        self.line.as_ref()
    }

    pub fn budget_voucher(&self) -> Option<&BudgetVoucher> {
        self.budget_voucher.as_ref()
    }

    pub fn artifacts(&self) -> &[WorkArtifact] {
        &self.artifacts
    }

    pub fn supersedes(&self) -> Option<&str> {
        self.supersedes.as_deref()
    }

    pub fn history(&self) -> &[HistoryRecord] {
        &self.history
    }

    // ---------- 前置材料的登记(Created 阶段,release 之前) ----------

    /// 绑定产线版本。仅 Created 态允许:release 门控要审的就是这份绑定,
    /// 已 release 后再改绑定等于绕过门控,直接拒绝。
    pub fn bind_line(&mut self, binding: LineBinding) -> Result<(), GateError> {
        if self.state != OrderState::Created {
            return Err(GateError::MutationForbidden {
                state: self.state,
                op: "bind_line",
            });
        }
        self.line = Some(binding);
        Ok(())
    }

    /// 登记预算凭证。凭证可以未签名(`signed=false`)——release 门控会把它报成缺失。
    /// 同样仅 Created 态允许。
    pub fn attach_budget_voucher(&mut self, voucher: BudgetVoucher) -> Result<(), GateError> {
        if self.state != OrderState::Created {
            return Err(GateError::MutationForbidden {
                state: self.state,
                op: "attach_budget_voucher",
            });
        }
        self.budget_voucher = Some(voucher);
        Ok(())
    }

    /// 挂工件。仅 InProgress / Completed 态允许(created/released 还没开工,不该有产出;
    /// 终态后封存)。
    pub fn attach_artifact(&mut self, artifact: WorkArtifact) -> Result<(), GateError> {
        if !matches!(self.state, OrderState::InProgress | OrderState::Completed) {
            return Err(GateError::MutationForbidden {
                state: self.state,
                op: "attach_artifact",
            });
        }
        self.artifacts.push(artifact);
        Ok(())
    }

    // ---------- 门控条件计算 ----------

    /// release 前置:① 已绑定产线版本;② 预算凭证已签发(存在且 signed)。
    /// 返回缺失清单,顺序固定(产线在前、凭证在后),保证错误可复现、可断言。
    fn release_missing(&self) -> Vec<Missing> {
        let mut missing = Vec::new();
        if self.line.is_none() {
            missing.push(Missing::LineVersionNotBound);
        }
        match &self.budget_voucher {
            None => missing.push(Missing::BudgetVoucherNotIssued),
            Some(v) if !v.signed => missing.push(Missing::BudgetVoucherNotSigned {
                voucher_id: v.voucher_id.clone(),
            }),
            Some(_) => {}
        }
        missing
    }

    /// close 前置:存在合格终态工件(is_final && qualified)。
    fn close_missing(&self) -> Vec<Missing> {
        if self.artifacts.iter().any(|a| a.is_final && a.qualified) {
            Vec::new()
        } else {
            vec![Missing::NoQualifiedFinalArtifact]
        }
    }

    // ---------- 状态迁移(统一入口) ----------

    /// 通用迁移入口:先查状态机边,再查门控;两条都过才落状态并记履历。
    pub fn transition(&mut self, t: Transition) -> Result<OrderState, GateError> {
        let from = self.state;
        if !from.can_apply(t) {
            return Err(GateError::IllegalTransition {
                from,
                to: t.to_state(),
                allowed: from.allowed_transitions().to_vec(),
            });
        }
        let missing = match t {
            Transition::Release => self.release_missing(),
            Transition::Close => self.close_missing(),
            _ => Vec::new(),
        };
        if !missing.is_empty() {
            return Err(GateError::PreconditionsNotMet {
                transition: t,
                missing,
            });
        }
        self.state = t.to_state();
        self.history.push(HistoryRecord {
            from,
            to: self.state,
        });
        Ok(self.state)
    }

    pub fn release(&mut self) -> Result<OrderState, GateError> {
        self.transition(Transition::Release)
    }

    pub fn start(&mut self) -> Result<OrderState, GateError> {
        self.transition(Transition::Start)
    }

    pub fn complete(&mut self) -> Result<OrderState, GateError> {
        self.transition(Transition::Complete)
    }

    pub fn close(&mut self) -> Result<OrderState, GateError> {
        self.transition(Transition::Close)
    }

    pub fn cancel(&mut self) -> Result<OrderState, GateError> {
        self.transition(Transition::Cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sla::Priority;

    fn sla() -> Sla {
        Sla::new(Some("2026-10-30T00:00:00Z".into()), Priority::Normal)
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

    #[test]
    fn full_happy_path_reaches_closed() {
        let mut wo = WorkOrder::new("wo-1", GraphVersion::new("g-3"), sla());
        assert_eq!(wo.state(), OrderState::Created);
        wo.bind_line(line()).unwrap();
        wo.attach_budget_voucher(voucher(true)).unwrap();
        assert_eq!(wo.release().unwrap(), OrderState::Released);
        assert_eq!(wo.start().unwrap(), OrderState::InProgress);
        wo.attach_artifact(artifact("fa-1", true, true)).unwrap();
        assert_eq!(wo.complete().unwrap(), OrderState::Completed);
        assert_eq!(wo.close().unwrap(), OrderState::Closed);
        assert!(wo.state().is_terminal());
        assert_eq!(wo.history().len(), 4);
        assert_eq!(
            wo.history()[0],
            HistoryRecord {
                from: OrderState::Created,
                to: OrderState::Released
            }
        );
    }

    #[test]
    fn release_without_anything_lists_both_missing_in_fixed_order() {
        let mut wo = WorkOrder::new("wo-2", GraphVersion::new("g-3"), sla());
        let err = wo.release().unwrap_err();
        assert_eq!(
            err.missing().unwrap(),
            &[
                Missing::LineVersionNotBound,
                Missing::BudgetVoucherNotIssued
            ]
        );
        // 状态没被改动
        assert_eq!(wo.state(), OrderState::Created);
    }

    #[test]
    fn release_with_line_but_no_voucher_reports_only_voucher() {
        let mut wo = WorkOrder::new("wo-3", GraphVersion::new("g-3"), sla());
        wo.bind_line(line()).unwrap();
        let err = wo.release().unwrap_err();
        assert_eq!(err.missing().unwrap(), &[Missing::BudgetVoucherNotIssued]);
    }

    #[test]
    fn release_with_voucher_only_reports_only_line() {
        let mut wo = WorkOrder::new("wo-4", GraphVersion::new("g-3"), sla());
        wo.attach_budget_voucher(voucher(true)).unwrap();
        let err = wo.release().unwrap_err();
        assert_eq!(err.missing().unwrap(), &[Missing::LineVersionNotBound]);
    }

    #[test]
    fn release_with_unsigned_voucher_names_the_voucher() {
        let mut wo = WorkOrder::new("wo-5", GraphVersion::new("g-3"), sla());
        wo.bind_line(line()).unwrap();
        wo.attach_budget_voucher(voucher(false)).unwrap();
        let err = wo.release().unwrap_err();
        assert_eq!(
            err.missing().unwrap(),
            &[Missing::BudgetVoucherNotSigned {
                voucher_id: "bv-1".into()
            }]
        );
        // 错误文本可读,给出缺什么
        let text = format!("{err}");
        assert!(text.contains("bv-1"), "错误文本应含凭证号:{text}");
    }

    #[test]
    fn close_requires_final_and_qualified_artifact() {
        let mut wo = WorkOrder::new("wo-6", GraphVersion::new("g-3"), sla());
        wo.bind_line(line()).unwrap();
        wo.attach_budget_voucher(voucher(true)).unwrap();
        wo.release().unwrap();
        wo.start().unwrap();

        // 执行中不能直接 close:InProgress → Closed 不是状态机边(先于门控被拒)
        assert!(matches!(
            wo.close(),
            Err(GateError::IllegalTransition { .. })
        ));

        // ① Completed 但无任何工件 → close 门控报缺合格终态工件
        wo.complete().unwrap();
        let err = wo.close().unwrap_err();
        assert_eq!(err.missing().unwrap(), &[Missing::NoQualifiedFinalArtifact]);

        // ② 终态但不合格,不行
        wo.attach_artifact(artifact("fa-x", true, false)).unwrap();
        let err = wo.close().unwrap_err();
        assert_eq!(err.missing().unwrap(), &[Missing::NoQualifiedFinalArtifact]);

        // ③ 合格但非终态,仍不行
        wo.attach_artifact(artifact("ia-1", false, true)).unwrap();
        assert_eq!(
            wo.close().unwrap_err().missing().unwrap(),
            &[Missing::NoQualifiedFinalArtifact]
        );

        // ④ 补上合格终态工件,放行
        wo.attach_artifact(artifact("fa-1", true, true)).unwrap();
        assert_eq!(wo.close().unwrap(), OrderState::Closed);
    }

    #[test]
    fn illegal_transitions_are_rejected_as_illegal_not_missing() {
        let mut wo = WorkOrder::new("wo-7", GraphVersion::new("g-3"), sla());
        // created → in_progress:状态机没这条边
        match wo.transition(Transition::Start) {
            Err(GateError::IllegalTransition { from, to, .. }) => {
                assert_eq!((from, to), (OrderState::Created, OrderState::InProgress));
            }
            other => panic!("期望 IllegalTransition,实际 {other:?}"),
        }
        // created → closed 直接跳过整个生命周期,也不行
        assert!(matches!(
            wo.transition(Transition::Close),
            Err(GateError::IllegalTransition { .. })
        ));
        // 完成后不可取消
        wo.bind_line(line()).unwrap();
        wo.attach_budget_voucher(voucher(true)).unwrap();
        wo.release().unwrap();
        wo.start().unwrap();
        wo.attach_artifact(artifact("fa-1", true, true)).unwrap();
        wo.complete().unwrap();
        assert!(matches!(
            wo.cancel(),
            Err(GateError::IllegalTransition { .. })
        ));
    }

    #[test]
    fn cancel_works_from_active_states_and_freezes_order() {
        let mut wo = WorkOrder::new("wo-8", GraphVersion::new("g-3"), sla());
        wo.cancel().unwrap();
        assert_eq!(wo.state(), OrderState::Cancelled);
        // 终态后任何迁移都拒绝
        for t in [Transition::Release, Transition::Start, Transition::Cancel] {
            assert!(matches!(
                wo.transition(t),
                Err(GateError::IllegalTransition { .. })
            ));
        }
        // 终态后也不许再挂工件
        assert!(matches!(
            wo.attach_artifact(artifact("x", true, true)),
            Err(GateError::MutationForbidden { .. })
        ));
    }

    #[test]
    fn precondition_materials_can_only_be_registered_in_created() {
        let mut wo = WorkOrder::new("wo-9", GraphVersion::new("g-3"), sla());
        wo.bind_line(line()).unwrap();
        wo.attach_budget_voucher(voucher(true)).unwrap();
        wo.release().unwrap();
        // released 后想换绑定/换凭证:拒绝,防止绕过门控
        assert!(matches!(
            wo.bind_line(line()),
            Err(GateError::MutationForbidden {
                op: "bind_line",
                ..
            })
        ));
        assert!(matches!(
            wo.attach_budget_voucher(voucher(true)),
            Err(GateError::MutationForbidden {
                op: "attach_budget_voucher",
                ..
            })
        ));
    }

    #[test]
    fn graph_version_never_changes_through_any_operation() {
        let mut wo = WorkOrder::new("wo-10", GraphVersion::new("g-3"), sla());
        let before = wo.graph_version().clone();
        wo.bind_line(line()).unwrap();
        wo.attach_budget_voucher(voucher(true)).unwrap();
        wo.release().unwrap();
        wo.start().unwrap();
        wo.attach_artifact(artifact("fa-1", true, true)).unwrap();
        wo.complete().unwrap();
        wo.close().unwrap();
        // API 只有读取器;全流程后版本仍等于创建时绑定的版本
        assert_eq!(*wo.graph_version(), before);
        assert_eq!(wo.graph_version().as_str(), "g-3");
    }
}
