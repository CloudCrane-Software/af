//! 订单状态机:created → released → in_progress → completed → closed,可 cancelled。
//!
//! 状态机只回答"这条边存不存在";前置条件门控(绑定产线版本/预算凭证/合格终态工件)
//! 在 [`crate::order::WorkOrder`] 中实现,两者分开:非法转换报 [`crate::error::GateError::IllegalTransition`],
//! 门控不满足报 [`crate::error::GateError::PreconditionsNotMet`]。

use serde::{Deserialize, Serialize};

/// 工单状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderState {
    Created,
    Released,
    InProgress,
    Completed,
    Closed,
    Cancelled,
}

/// 一条状态机边(带语义名的转换)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Transition {
    /// created → released,受门控:产线版本已绑定 + 预算凭证已签发
    Release,
    /// released → in_progress
    Start,
    /// in_progress → completed
    Complete,
    /// completed → closed,受门控:存在合格终态工件
    Close,
    /// created/released/in_progress → cancelled
    Cancel,
}

impl Transition {
    pub fn from_state(self) -> OrderState {
        match self {
            Transition::Release => OrderState::Created,
            Transition::Start => OrderState::Released,
            Transition::Complete => OrderState::InProgress,
            Transition::Close => OrderState::Completed,
            Transition::Cancel => unreachable!("Cancel 起点不止一个,由 allowed_transitions 决定"),
        }
    }

    pub fn to_state(self) -> OrderState {
        match self {
            Transition::Release => OrderState::Released,
            Transition::Start => OrderState::InProgress,
            Transition::Complete => OrderState::Completed,
            Transition::Close => OrderState::Closed,
            Transition::Cancel => OrderState::Cancelled,
        }
    }
}

impl OrderState {
    /// 合法出边。不在表内的 (from, to) 组合一律拒绝。
    pub fn allowed_transitions(self) -> &'static [Transition] {
        match self {
            OrderState::Created => &[Transition::Release, Transition::Cancel],
            OrderState::Released => &[Transition::Start, Transition::Cancel],
            OrderState::InProgress => &[Transition::Complete, Transition::Cancel],
            OrderState::Completed => &[Transition::Close],
            OrderState::Closed | OrderState::Cancelled => &[],
        }
    }

    pub fn can_apply(self, t: Transition) -> bool {
        // Cancel 的起点是三个状态,单独判断。
        if t == Transition::Cancel {
            return matches!(self, Created | Released | InProgress);
        }
        self.allowed_transitions().contains(&t)
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, OrderState::Closed | OrderState::Cancelled)
    }
}

use OrderState::{Created, InProgress, Released};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_table_is_exactly_the_asked_lifecycle() {
        assert_eq!(
            OrderState::Created.allowed_transitions(),
            &[Transition::Release, Transition::Cancel]
        );
        assert_eq!(
            OrderState::Released.allowed_transitions(),
            &[Transition::Start, Transition::Cancel]
        );
        assert_eq!(
            OrderState::InProgress.allowed_transitions(),
            &[Transition::Complete, Transition::Cancel]
        );
        assert_eq!(
            OrderState::Completed.allowed_transitions(),
            &[Transition::Close]
        );
        assert!(OrderState::Closed.allowed_transitions().is_empty());
        assert!(OrderState::Cancelled.allowed_transitions().is_empty());
    }

    #[test]
    fn cancel_allowed_only_before_completion() {
        assert!(Created.can_apply(Transition::Cancel));
        assert!(Released.can_apply(Transition::Cancel));
        assert!(InProgress.can_apply(Transition::Cancel));
        assert!(!OrderState::Completed.can_apply(Transition::Cancel));
        assert!(!OrderState::Closed.can_apply(Transition::Cancel));
        assert!(!OrderState::Cancelled.can_apply(Transition::Cancel));
    }

    #[test]
    fn terminal_states() {
        assert!(OrderState::Closed.is_terminal());
        assert!(OrderState::Cancelled.is_terminal());
        assert!(!Created.is_terminal());
    }
}
