//! 可注入时刻。
//!
//! 护栏放不放行只看候选和失败码，不看钟。
//! [`Clock::now_ms`] 只抄进账本，作为这一次判定的审计时刻。
//! 测试用 [`ManualClock`] 设定或推进，不睡眠。

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// 账本读取「现在」的来源。实现必须可以跨线程共享。
pub trait Clock: Send + Sync {
    /// Unix 毫秒。允许为负：调用方注入什么就落什么。
    fn now_ms(&self) -> i64;
}

/// 生产时钟。读 [`SystemTime`]。只用于审计时刻，不参与放行。
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl SystemClock {
    pub fn new() -> Self {
        Self
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(elapsed) => i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX),
            Err(_) => 0,
        }
    }
}

/// 测试与调用方注入的手动钟。克隆共享同一个计数器。
#[derive(Debug, Clone)]
pub struct ManualClock {
    millis: Arc<AtomicI64>,
}

impl ManualClock {
    pub fn new(now_ms: i64) -> Self {
        Self {
            millis: Arc::new(AtomicI64::new(now_ms)),
        }
    }

    /// 把时刻设成绝对值。
    pub fn set(&self, now_ms: i64) {
        self.millis.store(now_ms, Ordering::SeqCst);
    }

    /// 向前推进。`delta_ms == 0` 保持原值。
    pub fn advance(&self, delta_ms: u64) {
        let delta = i64::try_from(delta_ms).unwrap_or(i64::MAX);
        self.millis
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |cur| {
                Some(cur.saturating_add(delta))
            })
            .expect("advance 闭包返回 Some");
    }

    pub fn now_ms(&self) -> i64 {
        self.millis.load(Ordering::SeqCst)
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        ManualClock::now_ms(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_clock_set_and_advance_are_shared_by_clones() {
        let clock = ManualClock::new(-20);
        let clone = clock.clone();
        assert_eq!(Clock::now_ms(&clock), -20);
        clock.advance(5);
        assert_eq!(clone.now_ms(), -15);
        clone.set(40);
        assert_eq!(clock.now_ms(), 40);
        clock.advance(0);
        assert_eq!(clock.now_ms(), 40);
    }
}
