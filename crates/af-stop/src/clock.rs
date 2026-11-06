//! 可注入时钟。
//!
//! 间隔与过期只读 [`Clock::monotonic_ms`]。[`Clock::wall_ms`] 是墙钟 Unix 毫秒，
//! 只写入账本里的绝对时刻，不参与时间窗开关。测试推进 [`ManualClock`]，不睡眠。

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// 账本用的时钟。实现必须可以跨线程共享。
///
/// 同一实例上 [`Clock::monotonic_ms`] 不得回退。墙钟允许回拨，所以过期判断不能读它。
pub trait Clock: Send + Sync {
    /// 单调毫秒。后一次读数不得小于前一次。
    fn monotonic_ms(&self) -> i64;

    /// 墙钟 Unix 毫秒。允许回拨。只用于落库审计。
    fn wall_ms(&self) -> i64;
}

/// 生产时钟。单调读数来自 [`Instant`]（相对本实例创建时刻），墙钟来自 [`SystemTime`]。
///
/// `Instant` 不能跨进程比较。账本打开时把读数折进库里只增不减的逻辑毫秒，
/// 时间窗比较用逻辑毫秒。
#[derive(Debug, Clone, Copy)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn monotonic_ms(&self) -> i64 {
        i64::try_from(self.origin.elapsed().as_millis()).unwrap_or(i64::MAX)
    }

    fn wall_ms(&self) -> i64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
            Err(_) => 0,
        }
    }
}

/// 测试与调用方注入用的手动钟。单调读数与墙钟分开走。
///
/// [`ManualClock::advance`] 让两只钟一起前移；[`ManualClock::shift_wall_ms`]
/// 只动墙钟（负值即回拨）。克隆共享同一对计数器。
#[derive(Debug, Clone)]
pub struct ManualClock {
    mono_ms: Arc<AtomicI64>,
    wall_ms: Arc<AtomicI64>,
}

impl ManualClock {
    pub fn new(mono_ms: i64, wall_ms: i64) -> Self {
        Self {
            mono_ms: Arc::new(AtomicI64::new(mono_ms)),
            wall_ms: Arc::new(AtomicI64::new(wall_ms)),
        }
    }

    /// 单调钟与墙钟一起前移。`Duration` 不能为负，因此单调读数不会回退。
    pub fn advance(&self, by: Duration) {
        let ms = i64::try_from(by.as_millis()).unwrap_or(i64::MAX);
        saturating_add(&self.mono_ms, ms);
        saturating_add(&self.wall_ms, ms);
    }

    /// 只移动墙钟。`delta_ms` 为负表示回拨，单调钟保持不动。
    pub fn shift_wall_ms(&self, delta_ms: i64) {
        saturating_add(&self.wall_ms, delta_ms);
    }
}

impl Clock for ManualClock {
    fn monotonic_ms(&self) -> i64 {
        self.mono_ms.load(Ordering::SeqCst)
    }

    fn wall_ms(&self) -> i64 {
        self.wall_ms.load(Ordering::SeqCst)
    }
}

fn saturating_add(cell: &AtomicI64, delta: i64) {
    cell.try_update(Ordering::SeqCst, Ordering::SeqCst, |cur| {
        Some(cur.saturating_add(delta))
    })
    .expect("advance closure returns Some");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_clock_advance_is_shared_and_wall_rewind_stays_off_the_monotonic_clock() {
        let clock = ManualClock::new(10, 5_000);
        let clone = clock.clone();
        assert_eq!(Clock::monotonic_ms(&clock), 10);
        assert_eq!(clock.wall_ms(), 5_000);

        clock.advance(Duration::from_millis(25));
        assert_eq!(clone.monotonic_ms(), 35);
        assert_eq!(clone.wall_ms(), 5_025);

        clock.advance(Duration::from_millis(0));
        assert_eq!(clock.monotonic_ms(), 35);

        clone.shift_wall_ms(-4_000);
        assert_eq!(clock.monotonic_ms(), 35);
        assert_eq!(clock.wall_ms(), 1_025);
    }

    #[test]
    fn system_clock_monotonic_does_not_go_backwards() {
        let clock = SystemClock::new();
        let first = clock.monotonic_ms();
        let second = clock.monotonic_ms();
        assert!(second >= first);
        assert!(clock.wall_ms() > 1_000_000_000_000);
    }
}
