//! 变更单:差异描述 + 排水/接力策略。
//!
//! 语义定位:工单的 graph_version 不可变,所以"改版本"不是改原单,而是
//! 生成一张绑定新版本的**后继工单**(`supersedes` 指回原单),原单封存留痕。

use serde::{Deserialize, Serialize};

use crate::error::ChangeError;
use crate::order::{GraphVersion, WorkOrder};
use crate::sla::Sla;

/// 排水策略:切版本时,旧版本上的在制品(in-flight)怎么清场。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrainStrategy {
    /// 不排水:不等在制,直接切(在制按接力策略处置)
    NoDrain,
    /// 全量排水:排空所有在制后才允许切换
    DrainAllInflight,
    /// 上限排水:在制数量降到 `max_inflight` 以下即放行
    DrainWithCap { max_inflight: u32 },
}

/// 接力策略:切换瞬间,新旧版本如何交接。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelayStrategy {
    /// 冷接力:旧版停稳 → 新版起跑(允许停机窗口)
    ColdHandover,
    /// 温接力:双跑交接,重叠预算 `overlap_budget`(秒)内新旧并存在制
    WarmHandover { overlap_budget: u32 },
    /// 热接力:在制不落地,直接由新版接管
    HotTakeover,
}

/// 变更单。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeOrder {
    pub change_id: String,
    /// 目标工单 id
    pub work_order_id: String,
    pub from_graph_version: GraphVersion,
    pub to_graph_version: GraphVersion,
    /// 差异描述(人类可读,必填)
    pub diff_summary: String,
    /// 差异涉及的图谱节点/路径(机器可核对)
    pub diff_paths: Vec<String>,
    pub drain: DrainStrategy,
    pub relay: RelayStrategy,
}

impl ChangeOrder {
    /// 变更单自身的完备性校验(与目标工单无关的规则):
    /// ① from ≠ to;② 差异描述非空;③ 排水/接力策略组合不矛盾。
    pub fn validate(&self) -> Result<(), ChangeError> {
        if self.from_graph_version == self.to_graph_version {
            return Err(ChangeError::FromEqualsTo(self.from_graph_version.clone()));
        }
        if self.diff_summary.trim().is_empty() {
            return Err(ChangeError::EmptyDiffSummary);
        }
        if matches!(self.drain, DrainStrategy::DrainAllInflight)
            && matches!(self.relay, RelayStrategy::HotTakeover)
        {
            return Err(ChangeError::ConflictingDrainRelay);
        }
        Ok(())
    }

    /// 是否适用于目标工单:指向同一张单、基于的 from 版本与工单当前绑定一致(防陈旧)、
    /// 且工单未到终态(终态封存,不接受变更)。
    pub fn applies_to(&self, order: &WorkOrder) -> bool {
        self.work_order_id == order.id()
            && &self.from_graph_version == order.graph_version()
            && !order.state().is_terminal()
    }

    /// 由变更单生成后继工单:Created 态、绑定 to_graph_version、supersedes 指回原单。
    /// 原工单不被触碰(graph_version 不可变的兑现)。
    pub fn spawn_successor(
        &self,
        new_id: impl Into<String>,
        sla: Sla,
    ) -> Result<WorkOrder, ChangeError> {
        self.validate()?;
        Ok(WorkOrder::successor(
            new_id,
            self.to_graph_version.clone(),
            sla,
            self.work_order_id.clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sla::Priority;

    fn change(from: &str, to: &str, drain: DrainStrategy, relay: RelayStrategy) -> ChangeOrder {
        ChangeOrder {
            change_id: "ch-1".into(),
            work_order_id: "wo-1".into(),
            from_graph_version: GraphVersion::new(from),
            to_graph_version: GraphVersion::new(to),
            diff_summary: "节点 N12 的判定阈值 0.8 → 0.9".into(),
            diff_paths: vec!["graph/N12".into()],
            drain,
            relay,
        }
    }

    #[test]
    fn serde_roundtrip_keeps_all_fields() {
        let c = change(
            "g-3",
            "g-4",
            DrainStrategy::DrainWithCap { max_inflight: 5 },
            RelayStrategy::WarmHandover {
                overlap_budget: 120,
            },
        );
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<ChangeOrder>(&json).unwrap(), c);
    }

    #[test]
    fn from_equals_to_is_rejected() {
        let err = change(
            "g-3",
            "g-3",
            DrainStrategy::NoDrain,
            RelayStrategy::ColdHandover,
        )
        .validate()
        .unwrap_err();
        assert!(matches!(err, ChangeError::FromEqualsTo(_)));
    }

    #[test]
    fn empty_diff_summary_is_rejected() {
        let mut c = change(
            "g-3",
            "g-4",
            DrainStrategy::NoDrain,
            RelayStrategy::ColdHandover,
        );
        c.diff_summary = "   ".into();
        assert_eq!(c.validate().unwrap_err(), ChangeError::EmptyDiffSummary);
    }

    #[test]
    fn drain_all_plus_hot_takeover_conflicts() {
        let err = change(
            "g-3",
            "g-4",
            DrainStrategy::DrainAllInflight,
            RelayStrategy::HotTakeover,
        )
        .validate()
        .unwrap_err();
        assert_eq!(err, ChangeError::ConflictingDrainRelay);
        // 其余组合合法
        assert!(change(
            "g-3",
            "g-4",
            DrainStrategy::DrainAllInflight,
            RelayStrategy::ColdHandover
        )
        .validate()
        .is_ok());
        assert!(change(
            "g-3",
            "g-4",
            DrainStrategy::NoDrain,
            RelayStrategy::HotTakeover
        )
        .validate()
        .is_ok());
    }

    #[test]
    fn spawn_successor_binds_new_version_and_supersedes() {
        let c = change(
            "g-3",
            "g-4",
            DrainStrategy::NoDrain,
            RelayStrategy::ColdHandover,
        );
        let successor = c
            .spawn_successor("wo-1b", Sla::new(None, Priority::High))
            .unwrap();
        assert_eq!(successor.graph_version().as_str(), "g-4");
        assert_eq!(successor.supersedes(), Some("wo-1"));
        assert_eq!(successor.state(), crate::state::OrderState::Created);
    }

    #[test]
    fn invalid_change_cannot_spawn_successor() {
        let c = change(
            "g-3",
            "g-3",
            DrainStrategy::NoDrain,
            RelayStrategy::ColdHandover,
        );
        assert!(c
            .spawn_successor("wo-1b", Sla::new(None, Priority::Low))
            .is_err());
    }
}
