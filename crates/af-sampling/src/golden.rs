//! 已知答案注入器（金标件）+ 拦截契约台账。
//!
//! 语义：
//! - 金标件是“已知答案”的测试探针：按率 `p` 混入真实件流，注入队列；
//!   其中 70% 是缺陷金标、30% 是干净金标。
//! - 拦截契约：**金标缺陷件被放行 → 告警计数 +1**。正常情况下缺陷金标
//!   必须被下游检验拦截；一旦放行说明抽样在漏检，告警即质量防线被穿透的信号。
//! - 干净金标被误拦（误报）单独计数，用于观察误报率，不进告警。

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::csp1::Verdict;
use crate::rng::Rng;
use crate::SamplingError;

/// 队列中流转的带标签物品。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tagged {
    Real { seq: u64, conforming: bool },
    Golden { seq: u64, defective: bool },
}

impl Tagged {
    pub fn is_golden(&self) -> bool {
        matches!(self, Tagged::Golden { .. })
    }
}

/// 注入器配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectorConfig {
    /// 注入率 p：每处理一件真实件前，以概率 p 注入一件金标件。
    pub p: f64,
    /// 金标件中缺陷占比（默认 0.7，其余为干净金标）。
    pub defect_share: f64,
}

impl InjectorConfig {
    pub fn validate(&self) -> Result<(), SamplingError> {
        if !(0.0 <= self.p && self.p <= 1.0) {
            return Err(SamplingError::InvalidInjectorConfig(format!(
                "p 必须在 [0, 1]，得到 {}",
                self.p
            )));
        }
        if !(0.0 <= self.defect_share && self.defect_share <= 1.0) {
            return Err(SamplingError::InvalidInjectorConfig(format!(
                "defect_share 必须在 [0, 1]，得到 {}",
                self.defect_share
            )));
        }
        Ok(())
    }
}

/// 注入器统计。
#[derive(Debug, Default, Clone)]
pub struct InjectorStats {
    pub injected: u64,
    pub defective: u64,
    pub clean: u64,
}

/// 已知答案注入器：把金标件按率注入队列。
#[derive(Debug, Clone)]
pub struct GoldenInjector {
    cfg: InjectorConfig,
    seq: u64,
    rng: Rng,
    stats: InjectorStats,
}

impl GoldenInjector {
    pub fn new(cfg: InjectorConfig, seed: u64) -> Result<Self, SamplingError> {
        cfg.validate()?;
        Ok(GoldenInjector {
            cfg,
            seq: 0,
            rng: Rng::new(seed),
            stats: InjectorStats::default(),
        })
    }

    pub fn stats(&self) -> &InjectorStats {
        &self.stats
    }

    /// 生产一件真实件：注入器可能先往队列塞一件金标件（以率 p），真实件随后入队。
    pub fn enqueue_real(&mut self, queue: &mut VecDeque<Tagged>, conforming: bool) {
        if self.rng.chance(self.cfg.p) {
            let defective = self.rng.chance(self.cfg.defect_share);
            queue.push_back(Tagged::Golden {
                seq: self.seq,
                defective,
            });
            self.seq += 1;
            self.stats.injected += 1;
            if defective {
                self.stats.defective += 1;
            } else {
                self.stats.clean += 1;
            }
        }
        queue.push_back(Tagged::Real {
            seq: self.seq,
            conforming,
        });
        self.seq += 1;
    }
}

/// 拦截契约台账：在放行点对每件物品对账。
#[derive(Debug, Default, Clone)]
pub struct GoldenLedger {
    /// ★ 告警计数：金标缺陷件被放行的次数（契约核心指标）。
    alarms: u64,
    /// 金标缺陷件被放行的总数（与 alarms 恒等，分离存放以显式表达契约）。
    defective_released: u64,
    /// 金标缺陷件被拦截的总数。
    defective_intercepted: u64,
    /// 干净金标被误拦（误报）。
    false_rejects: u64,
    /// 干净金标正常放行。
    clean_passed: u64,
}

impl GoldenLedger {
    /// 放行点对账：拿队列取出的物品与其检验结论对答案。
    pub fn observe(&mut self, item: &Tagged, v: &Verdict) {
        if let Tagged::Golden { defective, .. } = item {
            if *defective {
                if v.defect_caught {
                    self.defective_intercepted += 1;
                } else {
                    // 被放行（无论未抽中直通还是检验放行）→ 契约告警
                    self.defective_released += 1;
                    self.alarms += 1;
                }
            } else if v.defect_caught {
                self.false_rejects += 1;
            } else {
                self.clean_passed += 1;
            }
        }
    }

    pub fn alarms(&self) -> u64 {
        self.alarms
    }
    pub fn defective_released(&self) -> u64 {
        self.defective_released
    }
    pub fn defective_intercepted(&self) -> u64 {
        self.defective_intercepted
    }
    pub fn false_rejects(&self) -> u64 {
        self.false_rejects
    }
    pub fn clean_passed(&self) -> u64 {
        self.clean_passed
    }
    /// 看过的金标件总数。
    pub fn golden_seen(&self) -> u64 {
        self.defective_intercepted + self.defective_released + self.false_rejects + self.clean_passed
    }
    /// 金标分类一致率：(缺陷被拦 + 干净放行) / 金标总数。
    pub fn consistency(&self) -> f64 {
        if self.golden_seen() == 0 {
            return 1.0;
        }
        (self.defective_intercepted + self.clean_passed) as f64 / self.golden_seen() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csp1::{Csp1, Csp1Config};

    /// 跑一条产线：注入器 → 队列 → CSP-1 → 台账，返回 (注入器统计, 台账)。
    fn run_line(csp_cfg: Csp1Config, p: f64, n_real: usize, seed: u64) -> (InjectorStats, GoldenLedger) {
        let mut inj = GoldenInjector::new(InjectorConfig { p, defect_share: 0.7 }, seed).unwrap();
        let mut m = Csp1::new(csp_cfg).unwrap();
        let mut r = Rng::new(seed.wrapping_add(1));
        let mut def = Rng::new(seed.wrapping_add(2));
        let mut ledger = GoldenLedger::default();
        let mut queue = VecDeque::new();
        for k in 0..n_real {
            inj.enqueue_real(&mut queue, !def.chance(0.06));
            while let Some(item) = queue.pop_front() {
                let conforming = match &item {
                    Tagged::Real { conforming, .. } => *conforming,
                    Tagged::Golden { defective, .. } => !*defective,
                };
                let v = m.process(conforming, &mut r);
                ledger.observe(&item, &v);
            }
            let _ = k;
        }
        (inj.stats().clone(), ledger)
    }

    #[test]
    fn rejects_bad_injector_config() {
        assert!(GoldenInjector::new(InjectorConfig { p: 1.5, defect_share: 0.7 }, 1).is_err());
        assert!(GoldenInjector::new(InjectorConfig { p: 0.1, defect_share: 2.0 }, 1).is_err());
        assert!(GoldenInjector::new(InjectorConfig { p: 0.1, defect_share: 0.7 }, 1).is_ok());
    }

    /// ★ 注入率与 70/30 构成。
    #[test]
    fn injects_at_rate_p_with_70_30_mix() {
        let n = 10_000;
        let (stats, _) = run_line(
            Csp1Config { enabled: true, i: 10, f: 0.5, f_min: 0.05 },
            0.1,
            n,
            7,
        );
        // 注入率 ≈ 0.1（n=10000，σ≈30，±10σ 足够宽）
        assert!(
            (900..=1100).contains(&stats.injected),
            "injected={}",
            stats.injected
        );
        // 缺陷占比 ≈ 0.7（σ≈14）
        let share = stats.defective as f64 / stats.injected as f64;
        assert!(
            (0.65..=0.75).contains(&share),
            "defect_share={share} defective={} clean={}",
            stats.defective,
            stats.clean
        );
        assert_eq!(stats.defective + stats.clean, stats.injected);
    }

    /// ★ 拦截契约：抽检必然漏检 → 金标缺陷件放行 → 告警计数增加，且 alarms == defective_released。
    #[test]
    fn alarm_fires_when_golden_defect_released() {
        let (_, ledger) = run_line(
            Csp1Config { enabled: true, i: 20, f: 0.1, f_min: 0.02 },
            0.1,
            1000,
            42,
        );
        assert!(ledger.defective_intercepted() > 0, "应当有被拦下的缺陷金标");
        assert!(ledger.alarms() > 0, "f=0.1 的抽检不可能拦住所有缺陷金标");
        assert_eq!(
            ledger.alarms(),
            ledger.defective_released(),
            "告警数必须恒等于放行的缺陷金标数"
        );
        // 一致性：金标缺陷一半以上被拦（全检相位拦截 + 抽检拦截）
        assert!(ledger.consistency() > 0.5, "consistency={}", ledger.consistency());
        println!(
            "contract(f=0.1): alarms={} intercepted={} released={} false_rejects={} consistency={:.3}",
            ledger.alarms(),
            ledger.defective_intercepted(),
            ledger.defective_released(),
            ledger.false_rejects(),
            ledger.consistency()
        );
    }

    /// ★ 全检（f=1 或 i 拉满永远全检）下契约零违例：所有缺陷金标被拦，告警为 0。
    #[test]
    fn no_alarm_under_full_screening() {
        let (_, ledger) = run_line(
            Csp1Config { enabled: true, i: u32::MAX, f: 1.0, f_min: 0.05 },
            0.1,
            1000,
            42,
        );
        assert_eq!(ledger.alarms(), 0);
        assert_eq!(ledger.false_rejects(), 0);
        // 全检之下所有金标缺陷都必须被拦下（放行数为 0）
        assert_eq!(ledger.defective_released(), 0);
        assert!(ledger.defective_intercepted() > 50);
        assert_eq!(ledger.consistency(), 1.0);
    }

    /// p=0 时一件金标都不注入。
    #[test]
    fn zero_p_injects_nothing() {
        let (stats, ledger) = run_line(
            Csp1Config { enabled: true, i: 5, f: 0.2, f_min: 0.05 },
            0.0,
            500,
            9,
        );
        assert_eq!(stats.injected, 0);
        assert_eq!(ledger.golden_seen(), 0);
        assert_eq!(ledger.alarms(), 0);
    }
}
