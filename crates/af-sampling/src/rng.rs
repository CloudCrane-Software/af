//! 极简确定性 PRNG（xorshift64*），固定种子全程可复现，避免引入 rand 依赖。

/// 确定性伪随机数发生器。同一种子 → 同一序列（跨平台一致，仅用整数移位与乘法）。
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    /// 种子为 0 时用固定常数避开启始态 0（xorshift 全零态会卡死）。
    pub fn new(seed: u64) -> Self {
        Rng(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// [0, 1) 均匀浮点。
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// 以概率 p 返回 true。
    pub fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_same_seed_same_stream() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn zero_seed_does_not_stick() {
        let mut r = Rng::new(0);
        let v1 = r.next_u64();
        assert_ne!(v1, 0);
        assert_ne!(r.next_u64(), v1);
    }

    #[test]
    fn chance_bounds_and_spread() {
        let mut r = Rng::new(7);
        let hits = (0..10_000).filter(|_| r.chance(0.3)).count();
        // 二项分布 n=10000 p=0.3：均值 3000，标准差 ~45.8，±10 个标准差之外几乎不可能。
        assert!((2800..=3200).contains(&hits), "hits={hits}");
    }
}
