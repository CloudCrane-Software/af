//! 可注入时刻。
//!
//! 执行器不读墙钟来决定下一轮。测试用 [`ManualClock`] 推进，不睡眠。
//! 计量的整数秒取 `wall_ms / 1000`，与 [`af_meter::Time`] 同单位。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// 调用方注入的时刻。实现必须可以跨线程共享。
pub trait Clock: Send + Sync {
    /// Unix 毫秒。测试里由调用方保证取值，生产实现读墙钟。
    fn wall_ms(&self) -> u64;

    /// 单调毫秒。网关的撤回窗口用它，不用墙钟。
    fn monotonic_ms(&self) -> u64;
}

/// 生产时钟。墙钟来自 [`SystemTime`]，单调读数用进程内递增的墙钟差，避免回绕。
#[derive(Debug, Clone)]
pub struct SystemClock {
    origin_mono: std::time::Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            origin_mono: std::time::Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn wall_ms(&self) -> u64 {
        wall_now()
    }

    fn monotonic_ms(&self) -> u64 {
        let elapsed = self.origin_mono.elapsed().as_millis();
        u64::try_from(elapsed).unwrap_or(u64::MAX)
    }
}

fn wall_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// 测试与调用方注入用的手动钟。克隆共享同一对计数器。
#[derive(Debug, Clone)]
pub struct ManualClock {
    wall_ms: Arc<AtomicU64>,
    mono_ms: Arc<AtomicU64>,
}

impl ManualClock {
    pub fn new(wall_ms: u64) -> Self {
        Self {
            wall_ms: Arc::new(AtomicU64::new(wall_ms)),
            mono_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 墙钟与单调钟一起前移。到 `u64::MAX` 饱和，不回绕。
    pub fn advance(&self, delta_ms: u64) {
        bump(&self.wall_ms, delta_ms);
        bump(&self.mono_ms, delta_ms);
    }

    pub fn set_wall(&self, wall_ms: u64) {
        self.wall_ms.store(wall_ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn wall_ms(&self) -> u64 {
        self.wall_ms.load(Ordering::SeqCst)
    }

    fn monotonic_ms(&self) -> u64 {
        self.mono_ms.load(Ordering::SeqCst)
    }
}

fn bump(cell: &AtomicU64, delta: u64) {
    cell.try_update(Ordering::SeqCst, Ordering::SeqCst, |cur| {
        Some(cur.saturating_add(delta))
    })
    .expect("advance 闭包返回 Some");
}

/// 把执行器的钟接到网关和 trace。两个 trait 的 `wall_ms` 单位不同，所以分开实现。
#[derive(Clone)]
pub(crate) struct SharedClock(pub Arc<dyn Clock>);

impl af_effect_gateway::Clock for SharedClock {
    fn monotonic_ms(&self) -> i64 {
        i64::try_from(self.0.monotonic_ms()).unwrap_or(i64::MAX)
    }

    fn wall_ms(&self) -> i64 {
        i64::try_from(self.0.wall_ms()).unwrap_or(i64::MAX)
    }
}

impl af_trace::Clock for SharedClock {
    fn wall_ms(&self) -> u64 {
        self.0.wall_ms()
    }
}
