//! CSP-1 连续抽样状态机（Dodge 连续抽样方案）。
//!
//! 语义：
//! - `Screening`（全检）：逐件检验；连续 `i` 件合格 → 转 `Sampling`。
//! - `Sampling`（抽检）：每件以比率 `f` 抽中检验；抽中且不合格 → 回 `Screening`；
//!   未抽中的件直接放行（若是缺陷件即漏检，仿真中地面真值已知，用于统计漏检）。
//! - `f` 的有效值恒 ≥ `f_min`：构造期校验拒绝 `f < f_min`，运行期 `set_rate` 把
//!   新速率钳制在 `[f_min, 1.0]`，因此“f 不低于 f_min”是结构性不变式。
//! - `enabled = false` 时整体关闭：一切直通不检，相位不变。

use crate::rng::Rng;
use crate::SamplingError;
use serde::{Deserialize, Serialize};

/// 抽样相位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// 全检（筛选）。
    Screening,
    /// 抽检。
    Sampling,
}

/// CSP-1 配置。参数整体可注入（模拟配方下发）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Csp1Config {
    /// 是否启用整台机器；false = 直通不检。
    pub enabled: bool,
    /// 连续合格件数阈值 i（全检阶段连续 i 件合格才允许转抽检）。
    pub i: u32,
    /// 抽检率 f ∈ (0, 1]。
    pub f: f64,
    /// 抽检率下限 f_min，任何运行期调整不得低于它。
    pub f_min: f64,
}

impl Csp1Config {
    pub fn validate(&self) -> Result<(), SamplingError> {
        if self.i == 0 {
            return Err(SamplingError::InvalidCsp1Config(format!(
                "i 必须 >= 1，得到 {}",
                self.i
            )));
        }
        if !(self.f_min > 0.0 && self.f_min <= 1.0) {
            return Err(SamplingError::InvalidCsp1Config(format!(
                "f_min 必须在 (0, 1]，得到 {}",
                self.f_min
            )));
        }
        if !(self.f_min <= self.f && self.f <= 1.0) {
            return Err(SamplingError::InvalidCsp1Config(format!(
                "f={} 不得低于 f_min={} 且不得超过 1.0",
                self.f, self.f_min
            )));
        }
        Ok(())
    }
}

/// 单件处理结论（仿真层已知地面真值，供统计与契约校验）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verdict {
    /// 本件是否被实际检验。
    pub inspected: bool,
    /// 检验且不合格（被拦截）。
    pub defect_caught: bool,
    /// 未检验且实际是缺陷件（漏检放行；仅仿真可判定）。
    pub leaked_defect: bool,
    /// 处理完本件后的相位。
    pub phase_after: Phase,
}

/// 过程统计。
#[derive(Debug, Default, Clone)]
pub struct Csp1Stats {
    pub inspected: u64,
    pub passed_uninspected: u64,
    pub defects_caught: u64,
    pub defects_leaked: u64,
    /// Screening → Sampling 切换次数。
    pub to_sampling_switches: u64,
    /// Sampling → Screening 切换次数。
    pub to_screening_switches: u64,
    /// 处于抽检相位时经过的总件数。
    pub sampling_phase_items: u64,
    /// 抽检相位中实际被检验的件数（经验抽检率的分子）。
    pub sampling_phase_inspected: u64,
}

impl Csp1Stats {
    /// 抽检相位内的经验抽检率（伯努利波动，仅统计展示用；结构性保证靠钳制）。
    pub fn empirical_sampling_rate(&self) -> f64 {
        if self.sampling_phase_items == 0 {
            0.0
        } else {
            self.sampling_phase_inspected as f64 / self.sampling_phase_items as f64
        }
    }
}

/// CSP-1 状态机。
#[derive(Debug, Clone)]
pub struct Csp1 {
    cfg: Csp1Config,
    phase: Phase,
    streak: u32,
    stats: Csp1Stats,
}

impl Csp1 {
    /// 构造并校验配置；`f < f_min` 在此即被拒绝。
    pub fn new(cfg: Csp1Config) -> Result<Self, SamplingError> {
        cfg.validate()?;
        Ok(Csp1 {
            cfg,
            phase: Phase::Screening,
            streak: 0,
            stats: Csp1Stats::default(),
        })
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn stats(&self) -> &Csp1Stats {
        &self.stats
    }

    pub fn config(&self) -> &Csp1Config {
        &self.cfg
    }

    /// 当前有效抽检率。不变式：恒 ≥ f_min（enabled=false 时无意义，仍返回钳制值）。
    pub fn effective_rate(&self) -> f64 {
        self.cfg.f.max(self.cfg.f_min)
    }

    /// 运行期调整抽检率，返回钳制后的有效值；任何人想压到 f_min 以下都会被顶回。
    pub fn set_rate(&mut self, f: f64) -> f64 {
        self.cfg.f = f.clamp(self.cfg.f_min, 1.0);
        self.cfg.f
    }

    /// 送入一件物品。`conforming` 是地面真值；抽中与否由 `rng` 以当前有效率决定。
    pub fn process(&mut self, conforming: bool, rng: &mut Rng) -> Verdict {
        if !self.cfg.enabled {
            self.stats.passed_uninspected += 1;
            if !conforming {
                self.stats.defects_leaked += 1;
            }
            return Verdict {
                inspected: false,
                defect_caught: false,
                leaked_defect: !conforming,
                phase_after: self.phase,
            };
        }
        match self.phase {
            Phase::Screening => {
                self.stats.inspected += 1;
                let mut v = Verdict {
                    inspected: true,
                    defect_caught: false,
                    leaked_defect: false,
                    phase_after: Phase::Screening,
                };
                if conforming {
                    self.streak += 1;
                    if self.streak >= self.cfg.i {
                        self.phase = Phase::Sampling;
                        self.streak = 0;
                        self.stats.to_sampling_switches += 1;
                        v.phase_after = Phase::Sampling;
                    }
                } else {
                    self.streak = 0;
                    self.stats.defects_caught += 1;
                    v.defect_caught = true;
                }
                v
            }
            Phase::Sampling => {
                self.stats.sampling_phase_items += 1;
                if rng.next_f64() < self.effective_rate() {
                    self.stats.inspected += 1;
                    self.stats.sampling_phase_inspected += 1;
                    if conforming {
                        Verdict {
                            inspected: true,
                            defect_caught: false,
                            leaked_defect: false,
                            phase_after: Phase::Sampling,
                        }
                    } else {
                        self.stats.defects_caught += 1;
                        self.phase = Phase::Screening;
                        self.streak = 0;
                        self.stats.to_screening_switches += 1;
                        Verdict {
                            inspected: true,
                            defect_caught: true,
                            leaked_defect: false,
                            phase_after: Phase::Screening,
                        }
                    }
                } else {
                    self.stats.passed_uninspected += 1;
                    let leaked = !conforming;
                    if leaked {
                        self.stats.defects_leaked += 1;
                    }
                    Verdict {
                        inspected: false,
                        defect_caught: false,
                        leaked_defect: leaked,
                        phase_after: Phase::Sampling,
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    fn cfg(i: u32, f: f64, f_min: f64) -> Csp1Config {
        Csp1Config {
            enabled: true,
            i,
            f,
            f_min,
        }
    }

    #[test]
    fn rejects_f_below_f_min_at_construction() {
        assert!(Csp1::new(cfg(5, 0.01, 0.05)).is_err());
        assert!(Csp1::new(cfg(0, 0.2, 0.05)).is_err());
        assert!(Csp1::new(cfg(5, 1.5, 0.05)).is_err());
        assert!(Csp1::new(cfg(5, 0.2, 0.05)).is_ok());
    }

    #[test]
    fn set_rate_is_clamped_to_f_min() {
        let mut m = Csp1::new(cfg(5, 0.2, 0.05)).unwrap();
        assert_eq!(m.set_rate(0.001), 0.05); // 想压到 f_min 以下 → 被顶回
        assert_eq!(m.set_rate(2.0), 1.0); // 超过 1 → 封顶
        assert_eq!(m.set_rate(0.4), 0.4); // 正常区间原样接受
        assert!(m.effective_rate() >= m.config().f_min);
    }

    #[test]
    fn switches_to_sampling_after_i_consecutive_conforming() {
        let i = 4;
        let mut m = Csp1::new(cfg(i, 0.5, 0.05)).unwrap();
        let mut r = Rng::new(1);
        for k in 1..i {
            let v = m.process(true, &mut r);
            assert_eq!(m.phase(), Phase::Screening, "第 {k} 件不应切换");
            assert!(v.inspected);
        }
        let v = m.process(true, &mut r); // 第 i 件
        assert_eq!(v.phase_after, Phase::Sampling);
        assert_eq!(m.phase(), Phase::Sampling);
        assert_eq!(m.stats().to_sampling_switches, 1);
    }

    #[test]
    fn streak_resets_on_defect_during_screening() {
        let i = 3;
        let mut m = Csp1::new(cfg(i, 0.5, 0.05)).unwrap();
        let mut r = Rng::new(1);
        m.process(true, &mut r);
        m.process(true, &mut r);
        let v = m.process(false, &mut r); // 缺陷打断连击
        assert!(v.defect_caught);
        // 重新数 3 件才允许切换
        for _ in 0..i - 1 {
            assert_eq!(m.process(true, &mut r).phase_after, Phase::Screening);
        }
        assert_eq!(m.process(true, &mut r).phase_after, Phase::Sampling);
    }

    #[test]
    fn defect_caught_in_sampling_returns_to_screening() {
        let mut m = Csp1::new(cfg(1, 1.0, 0.05)).unwrap(); // f=1：抽检即全检
        let mut r = Rng::new(1);
        assert_eq!(m.process(true, &mut r).phase_after, Phase::Sampling);
        let v = m.process(false, &mut r); // 抽检相位抽中缺陷
        assert!(v.inspected && v.defect_caught);
        assert_eq!(v.phase_after, Phase::Screening);
        assert_eq!(m.stats().to_screening_switches, 1);
    }

    #[test]
    fn disabled_bypasses_everything() {
        let mut m = Csp1::new(Csp1Config {
            enabled: false,
            i: 5,
            f: 0.2,
            f_min: 0.05,
        })
        .unwrap();
        let mut r = Rng::new(3);
        for k in 0..100 {
            let defective = k % 10 == 0; // 10% 缺陷
            let v = m.process(!defective, &mut r);
            assert!(!v.inspected && !v.defect_caught);
            assert_eq!(v.leaked_defect, defective);
        }
        assert_eq!(m.stats().inspected, 0);
        assert_eq!(m.stats().defects_leaked, 10);
    }

    /// ★ 本 PoC 指定验证之一：模拟 1000 件序列（埋缺陷位），
    /// 验证“连续合格后转抽检、遇缺陷回全检、f 不低于 f_min”。
    #[test]
    fn sim_1000_items_planted_defects() {
        const N: usize = 1000;
        const I: u32 = 15;
        const F: f64 = 0.2;
        const F_MIN: f64 = 0.05;

        let mut m = Csp1::new(cfg(I, F, F_MIN)).unwrap();
        let mut r = Rng::new(2026);
        let mut def = Rng::new(99); // 缺陷位发生器，与抽检决策解耦

        // 埋缺陷位：约 6% 随机缺陷 + 第 500~529 件一段 30 连续缺陷“质量事故”。
        let is_defective = |k: usize, def: &mut Rng| -> bool {
            (500..=529).contains(&k) || def.chance(0.06)
        };

        // 复放用的“已检验连续合格”计数器，用于核验切换条件。
        let mut my_streak: u32 = 0;
        let mut prev_phase = Phase::Screening;
        let mut planted = 0u64;

        for k in 0..N {
            let bad = is_defective(k, &mut def);
            if bad {
                planted += 1;
            }
            let v = m.process(!bad, &mut r);

            // 先维护复放计数器（与机器同步：当前件计入后才判定切换）：
            // 只有被检验的合格件推进连击，被检验的缺陷件清零。
            if v.inspected && !bad {
                my_streak = my_streak.saturating_add(1);
            } else if v.inspected && bad {
                my_streak = 0;
            }

            // 不变式 1：任何 Screening→Sampling 切换，必须此前已有 >= I 件“已检验连续合格”。
            if prev_phase == Phase::Screening && v.phase_after == Phase::Sampling {
                assert!(
                    my_streak >= I,
                    "第 {k} 件切换到抽检时连续合格只有 {my_streak} < {I}"
                );
            }
            // 不变式 2：任何 Sampling→Screening 切换，必须恰好由“抽中缺陷”触发。
            if prev_phase == Phase::Sampling && v.phase_after == Phase::Screening {
                assert!(v.defect_caught, "第 {k} 件回全检却不是抽中缺陷");
            }
            // 不变式 3：全检相位不允许漏检；漏检只能发生在抽检相位的未抽中件上。
            if v.inspected {
                assert!(!v.leaked_defect);
            } else {
                assert_eq!(v.leaked_defect, bad);
            }
            prev_phase = v.phase_after;
        }

        let s = m.stats();
        // 缺陷守恒：埋下的缺陷 = 拦截 + 漏检。
        assert_eq!(
            s.defects_caught + s.defects_leaked,
            planted,
            "caught={} leaked={} planted={planted}",
            s.defects_caught,
            s.defects_leaked
        );
        // 存在实际的相位往返（机器真的在两种相位间工作过）。
        assert!(s.to_sampling_switches >= 2, "switches={}", s.to_sampling_switches);
        assert!(s.to_screening_switches >= 1);
        // f 不低于 f_min：名义不变式 + 经验抽检率也在 f_min 之上（F=0.2 对 0.05 余量极大）。
        assert!(F >= F_MIN);
        let emp = s.empirical_sampling_rate();
        assert!(emp >= F_MIN, "经验抽检率 {emp} 低于 f_min={F_MIN}");
        // “质量事故”段（500~529 连续缺陷）必须把机器打回全检：事故段内至少一次回切。
        assert!(s.defects_caught > planted / 2);
        println!(
            "sim1000: planted={planted} caught={} leaked={} to_sampling={} to_screening={} emp_rate={emp:.3}",
            s.defects_caught, s.defects_leaked, s.to_sampling_switches, s.to_screening_switches
        );
    }

    /// 把有效抽检率钳到 f_min 后长跑，统计上逼近 f_min（结构性下限的可观测面）。
    #[test]
    fn long_run_sampling_rate_hovers_at_f_min_when_clamped() {
        let mut m = Csp1::new(cfg(1, 0.05, 0.05)).unwrap(); // i=1：第一件合格即入抽检相位
        let mut r0 = Rng::new(5);
        m.process(true, &mut r0); // 转入抽检相位
        assert_eq!(m.phase(), Phase::Sampling);
        m.set_rate(0.000001); // 试图压穿 f_min → 钳回 0.05
        assert_eq!(m.effective_rate(), 0.05);
        let mut r = Rng::new(11);
        for _ in 0..10_000 {
            m.process(true, &mut r);
        }
        let emp = m.stats().empirical_sampling_rate();
        assert!(
            (0.04..=0.06).contains(&emp),
            "经验抽检率 {emp} 偏离 f_min=0.05 过远"
        );
    }
}
