//! 工单一等对象:状态机 + 门控 + graph_version 不可变绑定 + SLA。
//!
//! # graph_version 不可变绑定怎么落地
//! `graph_version` 是 `WorkOrder` 的**私有字段,只提供读取器 [`WorkOrder::graph_version`],
//! 全 crate 不存在任何 setter 或 `&mut` 通路**;需要换图版本的场景一律走
//! [`crate::change::ChangeOrder`] 生成"后继工单"(`supersedes`),原工单保持封存。
//! 因此"不可变"是 API 层面的编译期保证,不是运行期约定。

use chrono::{DateTime, Utc};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};

use crate::decision::{Decision, DecisionArtifact, Role};
use crate::error::{GateError, Missing, OrderDecodeError};
use crate::identity::{self, inspect_identity_fields, RunContext, StableId};
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
///
/// `stable_id` 在创建时写入,没有 setter。`run_id` 只由 [`WorkOrder::continue_as_new`] 更换。
///
/// serde 不给这两个字段填默认值。老载荷缺字段时,[`decode_work_order`] 与
/// `Deserialize` 都返回拒绝式信封,并点名缺的字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkOrder {
    id: String,
    /// 跨引擎 run 的稳定身份。创建时生成,continue-as-new 不改。
    stable_id: StableId,
    /// 当前引擎 run。continue-as-new 会换成新值;空串表示尚未绑定。
    run_id: String,
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

/// 与 [`WorkOrder`] 同形的读入结构。身份字段在进入这里之前已经由拒绝式信封查过,
/// 这里不再给 `stable_id` 或 `run_id` 加默认值。
#[derive(Debug, Deserialize)]
struct WorkOrderWire {
    id: String,
    stable_id: StableId,
    run_id: String,
    state: OrderState,
    graph_version: GraphVersion,
    line: Option<LineBinding>,
    budget_voucher: Option<BudgetVoucher>,
    artifacts: Vec<WorkArtifact>,
    sla: Sla,
    supersedes: Option<String>,
    history: Vec<HistoryRecord>,
}

impl WorkOrderWire {
    fn into_order(self) -> WorkOrder {
        WorkOrder {
            id: self.id,
            stable_id: self.stable_id,
            run_id: self.run_id,
            state: self.state,
            graph_version: self.graph_version,
            line: self.line,
            budget_voucher: self.budget_voucher,
            artifacts: self.artifacts,
            sla: self.sla,
            supersedes: self.supersedes,
            history: self.history,
        }
    }
}

/// 读工单 JSON。缺 `stable_id` 或 `run_id` 时返回拒绝式信封,不生成替补身份。
pub fn decode_work_order(text: &str) -> Result<WorkOrder, OrderDecodeError> {
    let value =
        serde_json::from_str(text).map_err(|err| OrderDecodeError::Malformed(err.to_string()))?;
    decode_value(value)
}

fn decode_value(value: serde_json::Value) -> Result<WorkOrder, OrderDecodeError> {
    if let Err(envelope) = inspect_identity_fields(&value) {
        return Err(OrderDecodeError::Rejected(envelope));
    }
    let wire: WorkOrderWire =
        serde_json::from_value(value).map_err(|err| OrderDecodeError::Invalid(err.to_string()))?;
    Ok(wire.into_order())
}

impl<'de> Deserialize<'de> for WorkOrder {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        decode_value(value).map_err(serde::de::Error::custom)
    }
}

impl WorkOrder {
    /// 新建工单:初始状态 Created,graph_version 在此一次性绑定。
    ///
    /// `stable_id` 由订单 id 在 [`crate::identity::STABLE_ID_DOMAIN`] 下生成。
    /// 不读系统熵,也不读引擎 run。初始 `run_id` 为空(尚未绑定)。
    pub fn new(id: impl Into<String>, graph_version: GraphVersion, sla: Sla) -> Self {
        let id = id.into();
        let stable_id = StableId::generate(&id);
        Self::assemble(
            id,
            stable_id,
            RunContext::detached(),
            graph_version,
            sla,
            None,
        )
    }

    /// 以已经存在的稳定身份建单,并绑到这次的引擎 run。
    ///
    /// 不重新生成 `stable_id`。continue-as-new 续接时,把上一轮载荷里的身份原样带进来。
    pub fn with_stable_id(
        id: impl Into<String>,
        graph_version: GraphVersion,
        sla: Sla,
        stable_id: StableId,
        run: RunContext,
    ) -> Self {
        Self::assemble(id.into(), stable_id, run, graph_version, sla, None)
    }

    /// 后继工单构造:由变更单生成,graph_version 直接绑新版本,并记录 supersede 关系。
    ///
    /// 后继单是一张新工单,`stable_id` 按新 id 重新生成,不继承原单身份。
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

    fn assemble(
        id: String,
        stable_id: StableId,
        run: RunContext,
        graph_version: GraphVersion,
        sla: Sla,
        supersedes: Option<String>,
    ) -> Self {
        Self {
            id,
            stable_id,
            run_id: run.into_run_id(),
            state: OrderState::Created,
            graph_version,
            line: None,
            budget_voucher: None,
            artifacts: Vec::new(),
            sla,
            supersedes,
            history: Vec::new(),
        }
    }

    // ---------- 只读访问 ----------

    pub fn id(&self) -> &str {
        &self.id
    }

    /// 稳定身份。没有对应的 setter。
    pub fn stable_id(&self) -> &StableId {
        &self.stable_id
    }

    /// 当前引擎 run。空串表示尚未绑定。
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn run_context(&self) -> RunContext {
        RunContext::new(self.run_id.clone())
    }

    /// 身份是否连续:只比较 `stable_id`,不比较 run、业务 id 或状态。
    pub fn same_identity(&self, other: &Self) -> bool {
        self.stable_id == other.stable_id
    }

    /// 决策工件的被签 payload。`stable_id` 在里面,`run_id` 不在。
    pub fn decision_payload(&self, subject: &str) -> String {
        identity::decision_payload(&self.stable_id, subject)
    }

    /// 签发决策工件。哈希覆盖 [`Self::decision_payload`],因此换 run 不改变哈希,换身份会。
    pub fn seal_decision(
        &self,
        actor_id: impl Into<String>,
        role: Role,
        decision: Decision,
        subject: &str,
        key_id: impl Into<String>,
    ) -> DecisionArtifact {
        let payload = self.decision_payload(subject);
        DecisionArtifact::seal(actor_id, role, decision, &payload, key_id)
    }

    /// 分片键。`shard_count` 是配方层的分片数,不存在本单上。
    /// 只由 `stable_id` 决定,换 run 不改变结果。
    pub fn shard_key(&self, shard_count: u32) -> Result<u32, crate::error::ShardError> {
        self.stable_id.shard_key(shard_count)
    }

    /// 引擎 continue-as-new:换成新的 run。
    ///
    /// `stable_id`、`id`、`graph_version`、状态与履历都保持原值。
    /// 这不是状态机边,不记入 [`HistoryRecord`]。
    pub fn continue_as_new(&mut self, run: RunContext) -> &StableId {
        self.run_id = run.into_run_id();
        &self.stable_id
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

    /// 本单 SLA 是否已逾期。
    ///
    /// 比较的是建单时写入的绝对时刻 [`Sla::deadline`] 与调用方注入的 `now`。
    /// 不读系统钟,也不再问日历:班次规则只在解析 deadline 时生效。
    /// 恰在 deadline 不算逾期;未设 deadline 恒为 false。状态迁移不改变 deadline。
    pub fn is_overdue(&self, now: DateTime<Utc>) -> bool {
        self.sla.is_overdue(now)
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
        let deadline = chrono::DateTime::parse_from_rfc3339("2026-10-30T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        Sla::new(Some(deadline), Priority::Normal)
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

    #[test]
    fn order_overdue_uses_stored_absolute_deadline() {
        let deadline = chrono::DateTime::parse_from_rfc3339("2026-10-30T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let wo = WorkOrder::new(
            "wo-sla",
            GraphVersion::new("g-3"),
            Sla::new(Some(deadline), Priority::Normal),
        );
        assert_eq!(wo.sla().deadline, Some(deadline));
        assert!(!wo.is_overdue(deadline), "恰在时限不算逾期");
        assert!(wo.is_overdue(deadline + chrono::Duration::seconds(1)));
        assert!(!wo.is_overdue(deadline - chrono::Duration::seconds(1)));
    }

    #[test]
    fn stable_id_survives_run_change_and_the_lifecycle() {
        let mut wo = WorkOrder::new("wo-1", GraphVersion::new("g-3"), sla());
        let sid = wo.stable_id().clone();
        assert_eq!(sid, StableId::generate("wo-1"));
        assert!(wo.run_context().is_detached());
        assert_eq!(wo.graph_version().as_str(), "g-3");

        let carried = wo.continue_as_new(RunContext::new("run-a")).clone();
        assert_eq!(carried, sid);
        assert_eq!(wo.run_id(), "run-a");
        assert_eq!(wo.stable_id(), &sid);
        assert!(wo.history().is_empty(), "换 run 不是状态机边");

        wo.bind_line(line()).unwrap();
        wo.attach_budget_voucher(voucher(true)).unwrap();
        wo.release().unwrap();
        wo.start().unwrap();
        wo.attach_artifact(artifact("fa-1", true, true)).unwrap();
        wo.complete().unwrap();
        wo.close().unwrap();
        assert_eq!(wo.continue_as_new(RunContext::new("run-b")), &sid);
        assert_eq!(wo.run_id(), "run-b");
        assert_eq!(wo.stable_id(), &sid);
        assert_eq!(wo.graph_version().as_str(), "g-3");
        assert_eq!(wo.state(), OrderState::Closed);
        assert_eq!(wo.history().len(), 4);

        let other = WorkOrder::with_stable_id(
            "wo-other-label",
            GraphVersion::new("g-9"),
            sla(),
            sid.clone(),
            RunContext::new("run-c"),
        );
        assert!(wo.same_identity(&other));
        assert_ne!(wo.run_id(), other.run_id());
        assert_ne!(wo.id(), other.id());
        assert_eq!(wo.shard_key(64).unwrap(), other.shard_key(64).unwrap());
        assert_eq!(wo.shard_key(64).unwrap(), sid.shard_key(64).unwrap());

        let successor = WorkOrder::successor("wo-1b", GraphVersion::new("g-4"), sla(), "wo-1");
        assert_ne!(successor.stable_id(), &sid);
        assert_eq!(successor.stable_id(), &StableId::generate("wo-1b"));
        assert_eq!(successor.supersedes(), Some("wo-1"));
    }

    const LEGACY_ORDER: &str = r#"{
        "id": "wo-legacy",
        "state": "Created",
        "graph_version": "g-3",
        "line": null,
        "budget_voucher": null,
        "artifacts": [],
        "sla": {"deadline": "2026-10-30T00:00:00Z", "priority": "Normal"},
        "supersedes": null,
        "history": []
    }"#;

    #[test]
    fn legacy_payload_without_stable_id_is_a_rejection_envelope() {
        let err = decode_work_order(LEGACY_ORDER).unwrap_err();
        let crate::error::OrderDecodeError::Rejected(envelope) = err else {
            panic!("老载荷应走拒绝式信封");
        };
        assert_eq!(envelope.field, "stable_id");
        assert_eq!(envelope.kind, crate::error::RejectionKind::MissingField);
        let text = envelope.to_string();
        assert!(text.contains("拒绝式信封"), "{text}");
        assert!(text.contains("stable_id"), "{text}");
        let body = serde_json::to_value(&envelope).unwrap();
        assert_eq!(body["field"], "stable_id");
        assert_eq!(body["kind"], "missing_field");
        assert!(body["detail"].as_str().unwrap().contains("stable_id"));

        let direct = serde_json::from_str::<WorkOrder>(LEGACY_ORDER).unwrap_err();
        let direct = direct.to_string();
        assert!(direct.contains("拒绝式信封"), "{direct}");
        assert!(direct.contains("stable_id"), "{direct}");

        let mut current =
            serde_json::to_value(WorkOrder::new("wo-legacy", GraphVersion::new("g-3"), sla()))
                .unwrap();
        current.as_object_mut().unwrap().remove("stable_id");
        let err = decode_work_order(&current.to_string()).unwrap_err();
        match err {
            crate::error::OrderDecodeError::Rejected(envelope) => {
                assert_eq!(envelope.field, "stable_id");
            }
            other => panic!("去掉 stable_id 后应拒绝,实际 {other:?}"),
        }
    }
}
