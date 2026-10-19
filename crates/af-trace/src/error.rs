use crate::event::TraceEvent;

/// 写通道与落盘的错误。
///
/// 写路径上的三种错误都把事件退还给调用方（fail-closed），绝不静默丢弃。
/// 打开存储和对账失败不携带事件：启动即停，没有可退还的新事件。
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TraceError {
    #[error("critical 缓冲已满（fail-closed），事件已退还调用方")]
    CriticalFull(TraceEvent),
    #[error("通道已关闭（shutdown 之后写入），事件已退还调用方")]
    Shutdown(TraceEvent),
    /// 写盘失败。`events` 是已接受但未进入 durable 文件的事件，调用方决定重试或中止。
    #[error("critical 落盘失败（fail-closed）: {detail}")]
    PersistFailed {
        detail: String,
        events: Vec<TraceEvent>,
    },
    #[error("trace 存储打开失败: {0}")]
    Open(String),
    #[error("trace 声明与已存状态不符（fail-closed）: {0}")]
    DeclarationMismatch(String),
    #[error("trace 落盘与缺口计数不一致（fail-closed）: {0}")]
    Inconsistent(String),
}

impl TraceError {
    /// 写路径上被退还的第一件事件。打开或对账错误返回 `None`。
    pub fn event(&self) -> Option<&TraceEvent> {
        match self {
            TraceError::CriticalFull(event) | TraceError::Shutdown(event) => Some(event),
            TraceError::PersistFailed { events, .. } => events.first(),
            TraceError::Open(_)
            | TraceError::DeclarationMismatch(_)
            | TraceError::Inconsistent(_) => None,
        }
    }

    /// 与 [`TraceError::event`] 相同，但是取得所有权。
    pub fn into_event(self) -> Option<TraceEvent> {
        self.into_events().into_iter().next()
    }

    /// 写路径上被退还的全部事件。打开或对账错误得到空列表。
    pub fn into_events(self) -> Vec<TraceEvent> {
        match self {
            TraceError::CriticalFull(event) | TraceError::Shutdown(event) => vec![event],
            TraceError::PersistFailed { events, .. } => events,
            TraceError::Open(_)
            | TraceError::DeclarationMismatch(_)
            | TraceError::Inconsistent(_) => Vec::new(),
        }
    }
}
