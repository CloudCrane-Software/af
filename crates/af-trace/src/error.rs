use crate::event::TraceEvent;

/// critical 写通道的错误。两种情况下事件都**原样退还**调用方（fail-closed），
/// 绝不静默丢弃——事件带回了它被分配到的 seq，调用方可重试或中止。
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TraceError {
    #[error("critical 缓冲已满（fail-closed），事件已退还调用方")]
    CriticalFull(TraceEvent),
    #[error("通道已关闭（shutdown 之后写入），事件已退还调用方")]
    Shutdown(TraceEvent),
}

impl TraceError {
    pub fn event(&self) -> &TraceEvent {
        match self {
            TraceError::CriticalFull(e) | TraceError::Shutdown(e) => e,
        }
    }

    pub fn into_event(self) -> TraceEvent {
        match self {
            TraceError::CriticalFull(e) | TraceError::Shutdown(e) => e,
        }
    }
}
