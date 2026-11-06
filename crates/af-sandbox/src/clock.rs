//! 可注入时钟。超时只比较调用方注入的毫秒读数，本 crate 不睡眠。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// 会话超时用的时刻源。
///
/// 生产侧把 [`SystemClock`] 交进来；测试交 [`ManualClock`]。
/// 执行路径只调用这个 trait，不另外读墙钟。
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

/// 生产时钟。读数是 Unix 毫秒。回拨会原样进入比较，调用方若不能接受回拨就不要用它。
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl SystemClock {
    pub fn new() -> Self {
        Self
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(elapsed) => u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
            Err(_) => 0,
        }
    }
}

/// 测试与调用方注入用的手动钟。克隆共享同一读数。
///
/// [`ManualClock::set`] 可以回拨。超时比较信任这个读数，不把它夹到单调。
#[derive(Debug, Clone)]
pub struct ManualClock {
    now_ms: Arc<AtomicU64>,
}

impl ManualClock {
    pub fn new(now_ms: u64) -> Self {
        Self {
            now_ms: Arc::new(AtomicU64::new(now_ms)),
        }
    }

    pub fn set(&self, now_ms: u64) {
        self.now_ms.store(now_ms, Ordering::Relaxed);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.load(Ordering::Relaxed)
    }
}
