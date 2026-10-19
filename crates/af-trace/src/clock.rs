//! 可注入时刻。
//!
//! `ts_ms` 只作落盘审计，存的是墙钟毫秒。本 crate 没有间隔或过期判定。
//! 生产用 [`SystemClock`]，测试用 [`ManualClock`] 固定时刻，不睡眠。

use crate::event::now_ms;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// 事件时间戳的来源。实现必须可以跨线程共享。
pub trait Clock: Send + Sync {
    /// Unix 毫秒。允许回拨；调用方自己保证测试里的取值。
    fn wall_ms(&self) -> u64;
}

/// 生产时钟。读 [`SystemTime`]。
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl SystemClock {
    pub fn new() -> Self {
        Self
    }
}

impl Clock for SystemClock {
    fn wall_ms(&self) -> u64 {
        now_ms()
    }
}

/// 测试与调用方注入用的手动钟。克隆共享同一个计数器。
#[derive(Debug, Clone)]
pub struct ManualClock {
    millis: Arc<AtomicU64>,
}

impl ManualClock {
    pub fn new(now_ms: u64) -> Self {
        Self {
            millis: Arc::new(AtomicU64::new(now_ms)),
        }
    }

    /// 把时刻设成绝对值。
    pub fn set(&self, now_ms: u64) {
        self.millis.store(now_ms, Ordering::SeqCst);
    }

    /// 向前推进。到 `u64::MAX` 饱和，不回绕。
    pub fn advance(&self, delta_ms: u64) {
        self.millis
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |cur| {
                Some(cur.saturating_add(delta_ms))
            })
            .expect("advance 闭包返回 Some");
    }

    pub fn now_ms(&self) -> u64 {
        self.millis.load(Ordering::SeqCst)
    }
}

impl Clock for ManualClock {
    fn wall_ms(&self) -> u64 {
        ManualClock::now_ms(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_clock_set_and_advance_are_shared_by_clones() {
        let clock = ManualClock::new(10);
        let clone = clock.clone();
        assert_eq!(clock.wall_ms(), 10);
        clone.set(40);
        assert_eq!(clock.wall_ms(), 40);
        clock.advance(2);
        assert_eq!(clone.now_ms(), 42);
        clock.set(u64::MAX - 1);
        clock.advance(5);
        assert_eq!(clone.wall_ms(), u64::MAX);
    }
}
