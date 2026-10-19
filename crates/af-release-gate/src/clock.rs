//! 可注入墙钟。
//!
//! 投放闸的准入、完成、夜间窗口和渐放判定只看调用方传入的分钟
//! （[`crate::MinuteOfDay`]）。本钟不参与这些判定。
//!
//! 账本里的绝对时刻（`updated_wall_ms`）用 [`Clock::wall_ms`]，单位是 Unix 毫秒。
//! 测试用 [`ManualClock`] 固定这个戳，不睡眠。

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// 落库绝对时刻的来源。实现必须可以跨线程共享。
pub trait Clock: Send + Sync {
    /// Unix 毫秒。允许回拨。只用于落库，不用于间隔或过期判定。
    fn wall_ms(&self) -> i64;
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
    fn wall_ms(&self) -> i64 {
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
    pub fn new(wall_ms: i64) -> Self {
        Self {
            millis: Arc::new(AtomicI64::new(wall_ms)),
        }
    }

    /// 把墙钟设成绝对值。
    pub fn set(&self, wall_ms: i64) {
        self.millis.store(wall_ms, Ordering::SeqCst);
    }

    /// 向前推进。`delta_ms == 0` 保持原值。
    pub fn advance(&self, delta_ms: u64) {
        let delta = i64::try_from(delta_ms).unwrap_or(i64::MAX);
        self.millis
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |cur| {
                Some(cur.saturating_add(delta))
            })
            .expect("advance closure returns Some");
    }

    pub fn wall_ms(&self) -> i64 {
        self.millis.load(Ordering::SeqCst)
    }
}

impl Clock for ManualClock {
    fn wall_ms(&self) -> i64 {
        ManualClock::wall_ms(self)
    }
}
