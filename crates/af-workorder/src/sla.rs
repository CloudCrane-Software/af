//! SLA:绝对截止时刻 + 优先级。
//!
//! [`Sla::deadline`] 是 `Option<DateTime<Utc>>`。它由注入的 [`af_calendar::Calendar`]
//! 按班次规则从"下单时刻 + 承诺工时"解析出来([`Sla::from_calendar`]),再归一到 UTC。
//! 订单逾期判定只比较这个绝对时刻:不再比较钟面,也不再比较 RFC3339 字符串。
//!
//! 字段从 `Option<String>` **一次性切换**,不另留一份 deprecated 字符串:
//! `DateTime<Utc>` 的 serde 线格式就是 RFC3339(UTC 用 `Z`),旧载荷
//! `"deadline":"2026-10-20T00:00:00Z"` 能直接反序列化;带非零偏移的文本会折成同一绝对时刻。
//! 并存两个字段会让"判定读哪一个"不唯一。

use chrono::{DateTime, Duration, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use af_calendar::{Calendar, CalendarError};

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
    /// 完成时限的绝对时刻(UTC)。由日历班次规则解析产出;None 表示未设时限。
    pub deadline: Option<DateTime<Utc>>,
    pub priority: Priority,
}

impl Sla {
    /// 放入已经是绝对时刻的截止。`deadline` 本身应来自 [`Self::from_calendar`]
    /// 或是已归一的 UTC;本构造器不再接受 RFC3339 字符串。
    pub fn new(deadline: Option<DateTime<Utc>>, priority: Priority) -> Self {
        Self { deadline, priority }
    }

    /// 用注入的日历把"下单时刻 + 承诺有效工时"解析成绝对截止,再存成 UTC。
    ///
    /// `placed_at` 保持调用方时区(与 `Calendar::sla_deadline` 一致);返回前一律
    /// `with_timezone(Utc)`,之后的逾期判定与输入时区无关。
    /// 时长语义原样交给日历:零时长 = 下一个可开工时刻;负时长返回
    /// [`CalendarError::NegativeDuration`];视界耗尽返回 [`CalendarError::HorizonExceeded`]。
    /// 日历配置需已由调用方 `validate()`(本方法不重复校验)。
    pub fn from_calendar<Tz>(
        calendar: &Calendar,
        placed_at: DateTime<Tz>,
        promised_working: Duration,
        priority: Priority,
    ) -> Result<Self, CalendarError>
    where
        Tz: TimeZone + Clone,
    {
        let deadline = calendar
            .sla_deadline(placed_at, promised_working)?
            .with_timezone(&Utc);
        Ok(Self {
            deadline: Some(deadline),
            priority,
        })
    }

    /// 绝对时刻判定是否已过时限。
    ///
    /// `now` 由调用方注入,本方法不读系统钟,也不再问日历(班次规则只在解析 deadline 时生效)。
    /// 恰在 deadline 不算逾期(`now > deadline` 才算);无 deadline 恒为 false。
    pub fn is_overdue(&self, now: DateTime<Utc>) -> bool {
        self.deadline.is_some_and(|d| d < now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(rfc3339: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(rfc3339)
            .unwrap()
            .with_timezone(&Utc)
    }

    /// 旧实现的判定式。单独成函数,避免断言被当成常量比较。
    fn legacy_lexicographic_overdue(deadline: &str, now: &str) -> bool {
        deadline < now
    }

    #[test]
    fn priority_orders_low_to_critical() {
        assert!(Priority::Low < Priority::Normal);
        assert!(Priority::Normal < Priority::High);
        assert!(Priority::High < Priority::Critical);
    }

    #[test]
    fn overdue_compares_absolute_instants() {
        let sla = Sla::new(Some(utc("2026-10-19T08:00:00Z")), Priority::High);
        assert!(sla.is_overdue(utc("2026-10-19T08:00:01Z")));
        assert!(!sla.is_overdue(utc("2026-10-19T07:59:59Z")));
        assert!(
            !sla.is_overdue(utc("2026-10-19T08:00:00Z")),
            "恰在时限不算逾期"
        );

        // 10:59+08 = 02:59Z,绝对时刻更早。旧字典序把 "03:00:00Z" 与 "10:59:00+08:00" 比,会误判逾期。
        let sla = Sla::new(Some(utc("2026-10-26T03:00:00Z")), Priority::High);
        let now = utc("2026-10-26T10:59:00+08:00");
        assert!(now < sla.deadline.unwrap());
        assert!(!sla.is_overdue(now));
        assert!(
            legacy_lexicographic_overdue("2026-10-26T03:00:00Z", "2026-10-26T10:59:00+08:00"),
            "护栏:旧字典序确实会把这个时刻判成已逾期"
        );
    }

    #[test]
    fn no_deadline_never_overdue() {
        let sla = Sla::new(None, Priority::Low);
        assert!(!sla.is_overdue(utc("2999-01-01T00:00:00Z")));
    }

    #[test]
    fn sla_serde_roundtrip_accepts_legacy_rfc3339() {
        let sla = Sla::new(Some(utc("2026-10-20T00:00:00Z")), Priority::Critical);
        let json = serde_json::to_string(&sla).unwrap();
        assert!(
            json.contains("2026-10-20T00:00:00Z"),
            "UTC 线格式保持带 Z 的 RFC3339:{json}"
        );
        assert_eq!(serde_json::from_str::<Sla>(&json).unwrap(), sla);

        // 旧载荷:deadline 仍是字符串。Z 与非零偏移都折成同一个绝对时刻。
        let legacy_z = r#"{"deadline":"2026-10-20T00:00:00Z","priority":"Critical"}"#;
        assert_eq!(serde_json::from_str::<Sla>(legacy_z).unwrap(), sla);
        let legacy_offset = r#"{"deadline":"2026-10-20T08:00:00+08:00","priority":"Critical"}"#;
        assert_eq!(serde_json::from_str::<Sla>(legacy_offset).unwrap(), sla);

        let none = Sla::new(None, Priority::Low);
        assert_eq!(
            serde_json::from_str::<Sla>(&serde_json::to_string(&none).unwrap()).unwrap(),
            none
        );
    }

    #[test]
    fn from_calendar_rejects_negative_duration() {
        let cal = Calendar::weekday_8x5_shanghai();
        let placed = utc("2026-10-23T08:00:00Z");
        let err =
            Sla::from_calendar(&cal, placed, Duration::seconds(-1), Priority::Low).unwrap_err();
        assert_eq!(err, CalendarError::NegativeDuration { seconds: -1 });
    }
}
