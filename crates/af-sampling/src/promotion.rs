//! 三谱系升迁引擎：agent → hybrid → deterministic。
//!
//! 语义：
//! - [`Lineage`] 是严格有序的谱系阶梯：`Agent < Hybrid < Deterministic`，
//!   升/降一次只动一级，不跳级；`Deterministic` 是天花板。
//! - 判据结构（成功次数 / 违规数 / 一致性率）封装在 [`Recipe`] 里，
//!   **全部由外部注入**（JSON 配方，模拟配方下发），引擎不内置任何魔数。
//! - 熔断：一次硬失败（`success == false`）→ 立即自动降一级（Agent 触底），
//!   并清空评估窗口与连净计数。
//! - 回升：连续 `clean_to_recover` 次干净运行 → 向“已达成谱系”
//!   （[`Engine::attained`]，历史到过的最高谱系）回升一级；回升不会突破已达成谱系，
//!   冲击新谱系必须走判据窗口。
//! - 判据评估：滚动窗口积满 `window` 条运行记录后评估——
//!   成功次数 ≥ `min_successes`、违规总数 ≤ `max_violations`、平均一致性率 ≥
//!   `min_consistency` 三条同时满足才晋升一级并刷新已达成谱系；评估后窗口清空重来。

use serde::{Deserialize, Serialize};

use crate::SamplingError;

/// 三谱系，严格从便宜/呆板（agent）到便宜/确定（deterministic）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Lineage {
    Agent,
    Hybrid,
    Deterministic,
}

impl Lineage {
    pub const ALL: [Lineage; 3] = [Lineage::Agent, Lineage::Hybrid, Lineage::Deterministic];

    /// 升一级，到顶则原地不动。
    pub fn next(self) -> Option<Lineage> {
        match self {
            Lineage::Agent => Some(Lineage::Hybrid),
            Lineage::Hybrid => Some(Lineage::Deterministic),
            Lineage::Deterministic => None,
        }
    }

    /// 降一级，Agent 触底。
    pub fn prev(self) -> Option<Lineage> {
        match self {
            Lineage::Agent => None,
            Lineage::Hybrid => Some(Lineage::Agent),
            Lineage::Deterministic => Some(Lineage::Hybrid),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Lineage::Agent => "agent",
            Lineage::Hybrid => "hybrid",
            Lineage::Deterministic => "deterministic",
        }
    }
}

/// 升迁判据配方：**全部参数由外部传入**（JSON 注入）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    /// 判据评估窗口长度（条）。
    pub window: u32,
    /// 窗口内最少成功次数。
    pub min_successes: u32,
    /// 窗口内最大违规总数。
    pub max_violations: u32,
    /// 窗口内最小平均一致性率。
    pub min_consistency: f64,
    /// 熔断后连续干净多少次回升一级。
    pub clean_to_recover: u32,
}

impl Recipe {
    pub fn validate(&self) -> Result<(), SamplingError> {
        if self.window == 0 {
            return Err(SamplingError::InvalidRecipe("window 必须 >= 1".into()));
        }
        if self.clean_to_recover == 0 {
            return Err(SamplingError::InvalidRecipe(
                "clean_to_recover 必须 >= 1".into(),
            ));
        }
        if self.min_successes > self.window {
            return Err(SamplingError::InvalidRecipe(format!(
                "min_successes={} 超过窗口 {}，判据永不可能满足",
                self.min_successes, self.window
            )));
        }
        if !(0.0 <= self.min_consistency && self.min_consistency <= 1.0) {
            return Err(SamplingError::InvalidRecipe(format!(
                "min_consistency 必须在 [0, 1]，得到 {}",
                self.min_consistency
            )));
        }
        Ok(())
    }
}

/// 一次运行的观测结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunOutcome {
    /// 硬性成功/失败。false 触发熔断降级。
    pub success: bool,
    /// 本run违规数（软性，影响判据窗口；>0 还会打断连净计数）。
    pub violations: u32,
    /// 与金标/期望输出的一致性率 ∈ [0,1]。
    pub consistency: f64,
}

/// 引擎对一次 record 的处置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// 维持原级。
    Hold,
    /// 判据达成，晋升新一级（同时刷新已达成谱系）。
    Promoted(Lineage),
    /// 熔断降级。
    Demoted(Lineage),
    /// 连净回升一级（不超过已达成谱系）。
    Recovered(Lineage),
}

/// 升迁事件（带序号，供审计/测试复放）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub tick: u64,
    pub action: Action,
}

/// 升迁引擎。
#[derive(Debug, Clone)]
pub struct Engine {
    current: Lineage,
    attained: Lineage,
    recipe: Recipe,
    window: Vec<RunOutcome>,
    clean_streak: u32,
    events: Vec<Event>,
}

impl Engine {
    /// ★ 配方注入入口：判据全部来自外部 JSON，引擎零内置魔数。
    pub fn from_recipe_json(json: &str) -> Result<Self, SamplingError> {
        let recipe: Recipe = serde_json::from_str(json)?;
        recipe.validate()?;
        let window_cap = recipe.window as usize;
        Ok(Engine {
            current: Lineage::Agent,
            attained: Lineage::Agent,
            recipe,
            window: Vec::with_capacity(window_cap),
            clean_streak: 0,
            events: Vec::new(),
        })
    }

    pub fn current(&self) -> Lineage {
        self.current
    }

    /// 历史达成过的最高谱系：熔断只能把 current 打到 attained 以下，回升最多回到 attained。
    pub fn attained(&self) -> Lineage {
        self.attained
    }

    pub fn recipe(&self) -> &Recipe {
        &self.recipe
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn clean_streak(&self) -> u32 {
        self.clean_streak
    }

    /// 记录一次运行结果，返回处置动作。
    pub fn record(&mut self, out: RunOutcome) -> Action {
        let tick = self.events.len() as u64 + 1;
        let action = self.step(out);
        self.events.push(Event { tick, action });
        action
    }

    fn step(&mut self, out: RunOutcome) -> Action {
        // ── 熔断：硬失败立即自动降一级（Agent 触底），窗口与连净全部清零。
        if !out.success {
            self.current = self.current.prev().unwrap_or(self.current);
            self.window.clear();
            self.clean_streak = 0;
            return Action::Demoted(self.current);
        }

        // 连净计数：干净 = 成功且零违规；带违规的成功会打断连净。
        if out.violations == 0 {
            self.clean_streak += 1;
        } else {
            self.clean_streak = 0;
        }
        self.window.push(out);

        // ── 回升：连净 N 次 → 向已达成谱系爬一级（不突破 attained）。
        if self.current < self.attained && self.clean_streak >= self.recipe.clean_to_recover {
            if let Some(up) = self.current.next() {
                self.current = up;
                self.clean_streak = 0;
                return Action::Recovered(up);
            }
        }

        // ── 判据评估：窗口积满后按配方三判据裁决，评估完清空窗口。
        if self.window.len() >= self.recipe.window as usize {
            let successes = self.window.iter().filter(|r| r.success).count() as u32;
            let violations: u32 = self.window.iter().map(|r| r.violations).sum();
            let consistency =
                self.window.iter().map(|r| r.consistency).sum::<f64>() / self.window.len() as f64;
            let ok = successes >= self.recipe.min_successes
                && violations <= self.recipe.max_violations
                && consistency >= self.recipe.min_consistency;
            self.window.clear();
            if ok {
                if let Some(up) = self.current.next() {
                    self.current = up;
                    self.attained = self.attained.max(up);
                    self.clean_streak = 0;
                    return Action::Promoted(up);
                }
            }
        }

        Action::Hold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 标准测试配方：窗口 5、全成功、零违规、一致性 ≥ 0.98、连净 3 次回升。
    const RECIPE_JSON: &str = r#"{
        "window": 5,
        "min_successes": 5,
        "max_violations": 0,
        "min_consistency": 0.98,
        "clean_to_recover": 3
    }"#;

    fn clean() -> RunOutcome {
        RunOutcome {
            success: true,
            violations: 0,
            consistency: 1.0,
        }
    }
    fn fail() -> RunOutcome {
        RunOutcome {
            success: false,
            violations: 0,
            consistency: 0.0,
        }
    }
    fn soft(v: u32, c: f64) -> RunOutcome {
        RunOutcome {
            success: true,
            violations: v,
            consistency: c,
        }
    }

    /// ★ 配方注入：JSON 进、判据生效；坏 JSON / 非法参数被拒。
    #[test]
    fn recipe_is_injected_from_json_and_validated() {
        let e = Engine::from_recipe_json(RECIPE_JSON).unwrap();
        assert_eq!(
            e.recipe(),
            &Recipe {
                window: 5,
                min_successes: 5,
                max_violations: 0,
                min_consistency: 0.98,
                clean_to_recover: 3,
            }
        );
        assert_eq!(e.current(), Lineage::Agent);
        assert!(Engine::from_recipe_json("{ not json").is_err());
        assert!(Engine::from_recipe_json(r#"{"window":0,"min_successes":1,"max_violations":0,"min_consistency":0.9,"clean_to_recover":2}"#).is_err());
        assert!(Engine::from_recipe_json(r#"{"window":3,"min_successes":9,"max_violations":0,"min_consistency":0.9,"clean_to_recover":2}"#).is_err());
        assert!(Engine::from_recipe_json(r#"{"window":3,"min_successes":3,"max_violations":0,"min_consistency":1.5,"clean_to_recover":2}"#).is_err());
    }

    /// ★ 判据晋升：agent → hybrid → deterministic，一级一级来，到顶封住。
    #[test]
    fn promotes_by_criteria_level_by_level() {
        let mut e = Engine::from_recipe_json(RECIPE_JSON).unwrap();
        // 窗口 5 干净 → 晋 hybrid
        for k in 0..4 {
            assert_eq!(e.record(clean()), Action::Hold, "tick={k}");
        }
        assert_eq!(e.record(clean()), Action::Promoted(Lineage::Hybrid));
        assert_eq!(e.attained(), Lineage::Hybrid);
        // 再 5 干净 → 晋 deterministic（天花板）
        for k in 0..4 {
            assert_eq!(e.record(clean()), Action::Hold, "tick={k}");
        }
        assert_eq!(e.record(clean()), Action::Promoted(Lineage::Deterministic));
        // 在顶上继续干净：不再晋升
        for _ in 0..10 {
            assert_eq!(e.record(clean()), Action::Hold);
        }
        assert_eq!(e.current(), Lineage::Deterministic);
        assert_eq!(e.attained(), Lineage::Deterministic);
    }

    /// ★ 熔断：一次失败自动降一级；连净 N 次回升一级，且不突破已达成谱系。
    #[test]
    fn fuse_demotes_and_clean_streak_recovers_to_attained_only() {
        let mut e = Engine::from_recipe_json(RECIPE_JSON).unwrap();
        // 爬到 hybrid（5 干净）
        for _ in 0..5 {
            e.record(clean());
        }
        assert_eq!(e.current(), Lineage::Hybrid);
        // 失败一次 → 降回 agent（熔断）
        assert_eq!(e.record(fail()), Action::Demoted(Lineage::Agent));
        assert_eq!(e.current(), Lineage::Agent);
        assert_eq!(e.attained(), Lineage::Hybrid, "attained 不因熔断缩水");
        // 失败窗口被清空：之后 2 干净不应触发判据晋升（窗口要重积 5 条）
        e.record(clean());
        e.record(clean());
        // 连净 3 次 → 回升一级，只回到 hybrid（不越级到 deterministic）
        assert_eq!(e.record(clean()), Action::Recovered(Lineage::Hybrid));
        assert_eq!(e.current(), Lineage::Hybrid);
        // current == attained：继续连净不再触发回升；窗口已积 tick7/8/9 三条，续积
        assert_eq!(e.record(clean()), Action::Hold); // 窗口 4/5
        assert_eq!(e.record(clean()), Action::Promoted(Lineage::Deterministic));
        // 窗口 5/5 判据达成
    }

    /// ★ 判据会拒绝：违规超限 / 一致性率不足 → 窗口满也不晋升。
    #[test]
    fn criteria_refuse_violations_and_low_consistency() {
        // 违规超限：soft(1, 1.0) 不熔断但污染窗口
        let mut e = Engine::from_recipe_json(RECIPE_JSON).unwrap();
        for _ in 0..4 {
            e.record(clean());
        }
        assert_eq!(e.record(soft(1, 1.0)), Action::Hold); // 窗口满：violations=1 > max 0 → 拒
        assert_eq!(e.current(), Lineage::Agent);
        // 一致性率不足
        let mut e = Engine::from_recipe_json(RECIPE_JSON).unwrap();
        for _ in 0..4 {
            e.record(clean());
        }
        assert_eq!(e.record(soft(0, 0.5)), Action::Hold); // 平均一致性 (4+0.5)/5=0.9 < 0.98 → 拒
        assert_eq!(e.current(), Lineage::Agent);
        // 对照：同样 5 条干净高一致 → 晋升
        let mut e = Engine::from_recipe_json(RECIPE_JSON).unwrap();
        for _ in 0..5 {
            e.record(clean());
        }
        assert_eq!(e.current(), Lineage::Hybrid);
    }

    /// ★ 配方驱动行为：同一串结果，不同配方走出不同路径。
    #[test]
    fn different_recipes_different_paths_same_stream() {
        let stream = [clean(), clean(), clean(), clean(), clean(), fail(), clean()];
        // 宽配方：连净 1 次即可回升
        let mut loose = Engine::from_recipe_json(
            r#"{"window":3,"min_successes":3,"max_violations":0,"min_consistency":0.0,"clean_to_recover":1}"#,
        )
        .unwrap();
        // 严配方：连净 5 次才回升
        let mut strict = Engine::from_recipe_json(
            r#"{"window":3,"min_successes":3,"max_violations":0,"min_consistency":0.0,"clean_to_recover":5}"#,
        )
        .unwrap();
        for o in stream {
            loose.record(o);
            strict.record(o);
        }
        // 宽配方：5 干净 → hybrid；fail → agent；1 干净 → recovered hybrid
        assert_eq!(loose.current(), Lineage::Hybrid);
        assert_eq!(
            strict.current(),
            Lineage::Agent,
            "严配方连净 1 次远不够 5 次"
        );
    }

    /// ★ 事件审计：record 序列可复放。
    #[test]
    fn events_are_auditable() {
        let mut e = Engine::from_recipe_json(RECIPE_JSON).unwrap();
        for _ in 0..5 {
            e.record(clean());
        }
        e.record(fail());
        let ev = e.events();
        assert_eq!(ev.len(), 6);
        assert_eq!(ev[4].action, Action::Promoted(Lineage::Hybrid));
        assert_eq!(ev[4].tick, 5);
        assert_eq!(ev[5].action, Action::Demoted(Lineage::Agent));
        assert_eq!(ev[5].tick, 6);
    }
}
