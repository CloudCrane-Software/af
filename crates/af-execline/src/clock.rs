//! 时钟注入。ledger 时间戳与 MES `occurred_at` 都从这里取，测试可注固定值，
//! 重放得到同一时间戳（不读系统钟的路径全部可复现）。

use std::time::{SystemTime, UNIX_EPOCH};

/// Unix 毫秒时钟。
pub trait Clock {
    fn now_ms(&self) -> i64;
}

/// 系统时钟。生产路径用。
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0)
    }
}

/// 固定时钟。测试与重放用。
#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    ms: i64,
}

impl FixedClock {
    pub fn new(ms: i64) -> Self {
        Self { ms }
    }
}

impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        self.ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_returns_injected_value() {
        let clock = FixedClock::new(1_767_225_600_000);
        assert_eq!(clock.now_ms(), 1_767_225_600_000);
    }

    #[test]
    fn system_clock_is_plausible_unix_ms() {
        let now = SystemClock.now_ms();
        // 2020-01-01 之后的毫秒数；防止时钟注入错位（秒当毫秒等）。
        assert!(now > 1_577_836_800_000, "系统时钟异常: {now}");
    }
}
