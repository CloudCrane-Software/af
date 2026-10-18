//! SLA 字段:时限(deadline)+ 优先级(priority)。
//! deadline 用 RFC3339 字符串,避免为 PoC 引入 chrono/time 依赖;
//! [`Sla::is_overdue`] 的字典序比较只对"同格式 UTC(Z 后缀)时间戳"成立,见方法文档。

use serde::{Deserialize, Serialize};

/// 优先级,Ord 顺序即高低顺序(Low < Normal < High < Critical)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

/// 工单 SLA。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sla {
    /// 完成时限,RFC3339(建议 UTC + `Z` 后缀);None 表示未设时限。
    pub deadline: Option<String>,
    pub priority: Priority,
}

impl Sla {
    pub fn new(deadline: Option<String>, priority: Priority) -> Self {
        Self { deadline, priority }
    }

    /// 按字典序比较判断是否已过时限。
    ///
    /// # 前提(明确收窄的假设)
    /// `now` 与 `deadline` 都是同格式、同长度的 RFC3339 UTC 时间戳(如
    /// `2026-10-19T08:00:00Z`)。该前提下字典序 == 时间序;跨时区偏移格式不保证。
    /// 无 deadline 恒为 false。
    pub fn is_overdue(&self, now: &str) -> bool {
        match &self.deadline {
            Some(d) => d.as_str() < now,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_orders_low_to_critical() {
        assert!(Priority::Low < Priority::Normal);
        assert!(Priority::Normal < Priority::High);
        assert!(Priority::High < Priority::Critical);
    }

    #[test]
    fn overdue_uses_lexicographic_compare_on_rfc3339_utc() {
        let sla = Sla::new(Some("2026-10-19T08:00:00Z".into()), Priority::High);
        assert!(sla.is_overdue("2026-10-19T08:00:01Z"));
        assert!(!sla.is_overdue("2026-10-19T07:59:59Z"));
        assert!(!sla.is_overdue("2026-10-19T08:00:00Z"), "恰在时限不算超时");
    }

    #[test]
    fn no_deadline_never_overdue() {
        let sla = Sla::new(None, Priority::Low);
        assert!(!sla.is_overdue("2999-01-01T00:00:00Z"));
    }

    #[test]
    fn sla_serde_roundtrip() {
        let sla = Sla::new(Some("2026-10-20T00:00:00Z".into()), Priority::Critical);
        let json = serde_json::to_string(&sla).unwrap();
        assert_eq!(serde_json::from_str::<Sla>(&json).unwrap(), sla);
    }
}
