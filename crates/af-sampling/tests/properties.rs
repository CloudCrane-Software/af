//! 属性测试（proptest）。锁住 RESULT.md 已验证语义，不改状态机行为。
//!
//! 三组：
//! 1. 三不变式在任意注入序列下保持（转抽检 / 回全检 / f_min 托底）。
//! 2. 双水位升迁只在阈值穿越时发生（current 与 attained）。
//! 3. 金标注入不改谱系计数（谱系轨迹只由配方 + RunOutcome 序列决定）。
//!
//! 失败时 proptest 会打印缩到最小的输入。持久化回归文件关掉，反例只出现在测试输出里。

use std::collections::VecDeque;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use af_sampling::{
    Action, Csp1, Csp1Config, Engine, GoldenInjector, GoldenLedger, InjectorConfig,
    LedgerDeclaration, Lineage, ManualClock, OpenFork, Phase, Recipe, Rng as ItemRng, RunOutcome,
    SamplingLedger, Tagged,
};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

fn proptest_cfg() -> ProptestConfig {
    ProptestConfig {
        cases: 256,
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        ..ProptestConfig::default()
    }
}

fn bp_rate(bp: u32) -> f64 {
    bp as f64 / 10_000.0
}

fn milli(bp: u32) -> f64 {
    bp as f64 / 1000.0
}

fn lineage_rank(level: Lineage) -> i32 {
    match level {
        Lineage::Agent => 0,
        Lineage::Hybrid => 1,
        Lineage::Deterministic => 2,
    }
}

fn lineage_index(level: Lineage) -> usize {
    lineage_rank(level) as usize
}

// --- 任意注入序列：三不变式 -------------------------------------------------

struct ItemSeq {
    enabled: bool,
    i: u32,
    f_bp: u32,
    f_min_bp: u32,
    seed: u64,
    items: Vec<bool>,
    /// (在第几件之前应用；下标 ≥ 件数则在序列结束后应用) × 请求抽检率（万分比，可越界以覆盖钳制）。
    rate_edits_bp: Vec<(usize, u32)>,
}

impl fmt::Debug for ItemSeq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stream: String = self
            .items
            .iter()
            .map(|conforming| if *conforming { 'C' } else { 'D' })
            .collect();
        write!(
            f,
            "ItemSeq {{ enabled: {}, i: {}, f: {}/10000, f_min: {}/10000, seed: {}, items: \"{stream}\", rate_edits_bp: {:?} }}",
            self.enabled, self.i, self.f_bp, self.f_min_bp, self.seed, self.rate_edits_bp
        )
    }
}

fn arb_item_seq() -> impl Strategy<Value = ItemSeq> {
    (
        any::<bool>(),
        1u32..=32,
        1u32..=10_000,
        1u32..=10_000,
        any::<u64>(),
        proptest::collection::vec(any::<bool>(), 0..=40),
        proptest::collection::vec((0usize..=40, 0u32..=20_000), 0..=3),
    )
        .prop_map(|(enabled, i, a, b, seed, items, rate_edits_bp)| {
            let (f_min_bp, f_bp) = if a <= b { (a, b) } else { (b, a) };
            ItemSeq {
                enabled,
                i,
                f_bp,
                f_min_bp,
                seed,
                items,
                rate_edits_bp,
            }
        })
}

struct InvariantCursor {
    prev: Phase,
    /// 已检验连续合格件数。与 sim_1000 的复放一致：未检验件与相位切换本身不清零。
    streak: u32,
    to_sampling: u64,
    to_screening: u64,
    planted: u64,
}

fn rate_buckets(edits: &[(usize, u32)], len: usize) -> Vec<Vec<u32>> {
    let mut buckets = vec![Vec::new(); len + 1];
    for &(idx, bp) in edits {
        buckets[idx.min(len)].push(bp);
    }
    buckets
}

fn assert_f_min(machine: &Csp1, f_min: f64, at: &str) -> Result<(), TestCaseError> {
    let rate = machine.effective_rate();
    prop_assert!(
        rate >= f_min && rate <= 1.0,
        "不变式3 f_min 托底被打破 {}: effective={} f_min={}",
        at,
        rate,
        f_min
    );
    Ok(())
}

fn apply_rates(
    machine: &mut Csp1,
    rates_bp: &[u32],
    f_min: f64,
    at: &str,
) -> Result<(), TestCaseError> {
    for bp in rates_bp {
        let requested = bp_rate(*bp);
        let expect = requested.clamp(f_min, 1.0);
        let got = machine.set_rate(requested);
        prop_assert_eq!(
            got.to_bits(),
            expect.to_bits(),
            "set_rate 钳制偏离 {}: requested_bp={} got={} expect={}",
            at,
            bp,
            got,
            expect
        );
        assert_f_min(machine, f_min, at)?;
    }
    Ok(())
}

fn assert_switch_laws(
    seq: &ItemSeq,
    cursor: &InvariantCursor,
    verdict: &af_sampling::Verdict,
    index: usize,
    conforming: bool,
) -> Result<(), TestCaseError> {
    let switched_to_sampling =
        cursor.prev == Phase::Screening && verdict.phase_after == Phase::Sampling;
    if switched_to_sampling {
        prop_assert!(
            cursor.streak >= seq.i,
            "不变式1 转抽检: 第 {} 件 Screening→Sampling 时连续已检合格 {} < i={}",
            index,
            cursor.streak,
            seq.i
        );
    }
    if cursor.prev == Phase::Screening && verdict.inspected && conforming && cursor.streak >= seq.i
    {
        prop_assert_eq!(
            verdict.phase_after,
            Phase::Sampling,
            "不变式1 转抽检: 第 {} 件连续已检合格已达 i={} 却停在 {:?}",
            index,
            seq.i,
            verdict.phase_after
        );
    }
    if cursor.prev == Phase::Screening && !conforming {
        prop_assert_eq!(
            verdict.phase_after,
            Phase::Screening,
            "不变式1 转抽检: 第 {} 件全检遇缺陷却转到了 {:?}",
            index,
            verdict.phase_after
        );
    }

    let switched_to_screening =
        cursor.prev == Phase::Sampling && verdict.phase_after == Phase::Screening;
    if switched_to_screening {
        prop_assert!(
            verdict.defect_caught && verdict.inspected && !conforming,
            "不变式2 回全检: 第 {} 件 Sampling→Screening 却不是抽中缺陷 verdict={:?} conforming={}",
            index,
            verdict,
            conforming
        );
    }
    if cursor.prev == Phase::Sampling && verdict.defect_caught {
        prop_assert_eq!(
            verdict.phase_after,
            Phase::Screening,
            "不变式2 回全检: 第 {} 件抽中缺陷却未回全检 verdict={:?}",
            index,
            verdict
        );
    }
    if cursor.prev == Phase::Sampling && !verdict.defect_caught {
        prop_assert_eq!(
            verdict.phase_after,
            Phase::Sampling,
            "不变式2 回全检: 第 {} 件未抽中缺陷却离开抽检 verdict={:?}",
            index,
            verdict
        );
    }
    Ok(())
}

fn assert_leak_laws(
    seq: &ItemSeq,
    cursor: &InvariantCursor,
    verdict: &af_sampling::Verdict,
    index: usize,
    conforming: bool,
) -> Result<(), TestCaseError> {
    if !seq.enabled {
        prop_assert_eq!(
            verdict.phase_after,
            cursor.prev,
            "关闭时第 {} 件相位不得变化",
            index
        );
        prop_assert!(
            !verdict.inspected && !verdict.defect_caught,
            "关闭时第 {} 件不得检验",
            index
        );
    }
    if seq.enabled && cursor.prev == Phase::Screening {
        prop_assert!(verdict.inspected, "全检相位第 {} 件必须检验", index);
        prop_assert!(!verdict.leaked_defect, "全检相位第 {} 件不允许漏检", index);
        if !conforming {
            prop_assert!(
                verdict.defect_caught,
                "全检相位第 {} 件缺陷必须被拦截",
                index
            );
        }
    }
    if verdict.inspected {
        prop_assert!(!verdict.leaked_defect, "已检验件不得记漏检 index={}", index);
    } else {
        prop_assert_eq!(
            verdict.leaked_defect,
            !conforming,
            "未检验件的漏检标记必须等于地面真值缺陷 index={}",
            index
        );
    }
    Ok(())
}

fn check_three_invariants(seq: &ItemSeq) -> Result<(), TestCaseError> {
    let f_min = bp_rate(seq.f_min_bp);
    let mut machine = match Csp1::new(Csp1Config {
        enabled: seq.enabled,
        i: seq.i,
        f: bp_rate(seq.f_bp),
        f_min,
    }) {
        Ok(machine) => machine,
        Err(err) => return Err(TestCaseError::fail(format!("合法配置被拒: {err}; {seq:?}"))),
    };
    assert_f_min(&machine, f_min, "构造后")?;

    let mut rng = ItemRng::new(seq.seed);
    let mut cursor = InvariantCursor {
        prev: machine.phase(),
        streak: 0,
        to_sampling: 0,
        to_screening: 0,
        planted: 0,
    };
    let buckets = rate_buckets(&seq.rate_edits_bp, seq.items.len());

    for (index, conforming) in seq.items.iter().copied().enumerate() {
        apply_rates(
            &mut machine,
            &buckets[index],
            f_min,
            &format!("第 {index} 件前"),
        )?;
        if !conforming {
            cursor.planted += 1;
        }
        let verdict = machine.process(conforming, &mut rng);
        if verdict.inspected && conforming {
            cursor.streak = cursor.streak.saturating_add(1);
        } else if verdict.inspected && !conforming {
            cursor.streak = 0;
        }
        assert_switch_laws(seq, &cursor, &verdict, index, conforming)?;
        if cursor.prev == Phase::Screening && verdict.phase_after == Phase::Sampling {
            cursor.to_sampling += 1;
        }
        if cursor.prev == Phase::Sampling && verdict.phase_after == Phase::Screening {
            cursor.to_screening += 1;
        }
        assert_leak_laws(seq, &cursor, &verdict, index, conforming)?;
        assert_f_min(&machine, f_min, &format!("第 {index} 件后"))?;
        cursor.prev = verdict.phase_after;
    }
    apply_rates(&mut machine, &buckets[seq.items.len()], f_min, "序列结束后")?;

    let stats = machine.stats();
    prop_assert_eq!(
        stats.defects_caught + stats.defects_leaked,
        cursor.planted,
        "缺陷守恒失败 caught={} leaked={} planted={}",
        stats.defects_caught,
        stats.defects_leaked,
        cursor.planted
    );
    prop_assert_eq!(
        stats.inspected + stats.passed_uninspected,
        seq.items.len() as u64,
        "检验+直通必须等于注入件数"
    );
    prop_assert_eq!(
        stats.to_sampling_switches,
        cursor.to_sampling,
        "转抽检次数不一致"
    );
    prop_assert_eq!(
        stats.to_screening_switches,
        cursor.to_screening,
        "回全检次数不一致"
    );
    if !seq.enabled {
        prop_assert_eq!(stats.defects_caught, 0, "关闭时不得拦截");
        prop_assert_eq!(stats.inspected, 0, "关闭时不得检验");
    }
    Ok(())
}

// --- 双水位：只在阈值穿越时升降 ---------------------------------------------

#[derive(Clone)]
struct OutcomeIn {
    success: bool,
    violations: u32,
    consistency_bp: u32,
}

impl fmt::Debug for OutcomeIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tag = if self.success { 'S' } else { 'F' };
        write!(
            f,
            "{tag} v={} c={}/1000",
            self.violations, self.consistency_bp
        )
    }
}

#[derive(Clone)]
struct PromoCase {
    window: u32,
    min_successes: u32,
    max_violations: u32,
    min_consistency_bp: u32,
    clean_to_recover: u32,
    outcomes: Vec<OutcomeIn>,
}

impl fmt::Debug for PromoCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PromoCase {{ window: {}, min_successes: {}, max_violations: {}, min_consistency: {}/1000, clean_to_recover: {}, outcomes: {:?} }}",
            self.window,
            self.min_successes,
            self.max_violations,
            self.min_consistency_bp,
            self.clean_to_recover,
            self.outcomes
        )
    }
}

fn arb_outcome() -> impl Strategy<Value = OutcomeIn> {
    (any::<bool>(), 0u32..=15, 0u32..=1000).prop_map(|(success, violations, consistency_bp)| {
        OutcomeIn {
            success,
            violations,
            consistency_bp,
        }
    })
}

fn arb_promo_case() -> impl Strategy<Value = PromoCase> {
    (1u32..=8, 0u32..=40, 0u32..=1000, 1u32..=6)
        .prop_flat_map(
            |(window, max_violations, min_consistency_bp, clean_to_recover)| {
                (
                    Just(window),
                    0..=window,
                    Just(max_violations),
                    Just(min_consistency_bp),
                    Just(clean_to_recover),
                    proptest::collection::vec(arb_outcome(), 0..=24),
                )
            },
        )
        .prop_map(
            |(
                window,
                min_successes,
                max_violations,
                min_consistency_bp,
                clean_to_recover,
                outcomes,
            )| {
                PromoCase {
                    window,
                    min_successes,
                    max_violations,
                    min_consistency_bp,
                    clean_to_recover,
                    outcomes,
                }
            },
        )
}

fn to_outcome(raw: &OutcomeIn) -> RunOutcome {
    RunOutcome {
        success: raw.success,
        violations: raw.violations,
        consistency: milli(raw.consistency_bp),
    }
}

fn recipe_json(case: &PromoCase) -> String {
    serde_json::json!({
        "window": case.window,
        "min_successes": case.min_successes,
        "max_violations": case.max_violations,
        "min_consistency": milli(case.min_consistency_bp),
        "clean_to_recover": case.clean_to_recover,
    })
    .to_string()
}

fn engine_from(case: &PromoCase) -> Result<Engine, TestCaseError> {
    let json = recipe_json(case);
    match Engine::from_recipe_json(&json) {
        Ok(engine) => Ok(engine),
        Err(err) => Err(TestCaseError::fail(format!("配方被拒: {err}; json={json}"))),
    }
}

struct Shadow {
    current: Lineage,
    attained: Lineage,
    window: Vec<RunOutcome>,
    clean_streak: u32,
}

struct Thresholds {
    hard_fail: bool,
    /// 连净次数跨过 clean_to_recover，且 current < attained。
    recover: bool,
    /// 窗口积满且三判据同时成立，且还有上一级。
    promote: bool,
    expected: Action,
}

fn criteria_met(window: &[RunOutcome], recipe: &Recipe) -> bool {
    if window.is_empty() {
        return false;
    }
    let successes = window.iter().filter(|row| row.success).count() as u32;
    let violations: u32 = window.iter().map(|row| row.violations).sum();
    let mean = window.iter().map(|row| row.consistency).sum::<f64>() / window.len() as f64;
    successes >= recipe.min_successes
        && violations <= recipe.max_violations
        && mean >= recipe.min_consistency
}

fn classify(
    shadow: &Shadow,
    recipe: &Recipe,
    out: RunOutcome,
) -> Result<Thresholds, TestCaseError> {
    if !out.success {
        let level = shadow.current.prev().unwrap_or(shadow.current);
        return Ok(Thresholds {
            hard_fail: true,
            recover: false,
            promote: false,
            expected: Action::Demoted(level),
        });
    }

    let streak = if out.violations == 0 {
        shadow.clean_streak + 1
    } else {
        0
    };
    if shadow.current < shadow.attained && streak >= recipe.clean_to_recover {
        let Some(up) = shadow.current.next() else {
            return Err(TestCaseError::fail(format!(
                "current {:?} < attained {:?} 却没有上一级",
                shadow.current, shadow.attained
            )));
        };
        return Ok(Thresholds {
            hard_fail: false,
            recover: true,
            promote: false,
            expected: Action::Recovered(up),
        });
    }

    let mut window = shadow.window.clone();
    window.push(out);
    let full = window.len() >= recipe.window as usize;
    let promote = full && criteria_met(&window, recipe) && shadow.current.next().is_some();
    let expected = if let Some(up) = shadow.current.next() {
        if promote {
            Action::Promoted(up)
        } else {
            Action::Hold
        }
    } else {
        Action::Hold
    };
    Ok(Thresholds {
        hard_fail: false,
        recover: false,
        promote,
        expected,
    })
}

fn apply_shadow(shadow: &mut Shadow, recipe: &Recipe, out: RunOutcome, action: Action) {
    match action {
        Action::Demoted(level) => {
            shadow.current = level;
            shadow.window.clear();
            shadow.clean_streak = 0;
        }
        Action::Recovered(level) => {
            shadow.window.push(out);
            shadow.current = level;
            shadow.clean_streak = 0;
        }
        Action::Promoted(level) => {
            shadow.current = level;
            shadow.attained = shadow.attained.max(level);
            shadow.window.clear();
            shadow.clean_streak = 0;
        }
        Action::Hold => {
            if out.violations == 0 {
                shadow.clean_streak += 1;
            } else {
                shadow.clean_streak = 0;
            }
            shadow.window.push(out);
            if shadow.window.len() >= recipe.window as usize {
                shadow.window.clear();
            }
        }
    }
}

fn assert_level_step(
    before: Lineage,
    after: Lineage,
    step: usize,
    what: &str,
) -> Result<(), TestCaseError> {
    let delta = (lineage_rank(after) - lineage_rank(before)).abs();
    prop_assert!(
        delta <= 1,
        "step {}: {} 跳级 {:?} -> {:?}",
        step,
        what,
        before,
        after
    );
    Ok(())
}

fn assert_watermark_laws(
    before_current: Lineage,
    before_attained: Lineage,
    after_current: Lineage,
    after_attained: Lineage,
    action: Action,
    flags: &Thresholds,
    step: usize,
) -> Result<(), TestCaseError> {
    prop_assert!(
        after_current <= after_attained,
        "step {}: current {:?} 超过 attained {:?}",
        step,
        after_current,
        after_attained
    );
    prop_assert!(
        after_attained >= before_attained,
        "step {}: attained 回退 {:?} -> {:?}",
        step,
        before_attained,
        after_attained
    );
    assert_level_step(before_current, after_current, step, "current")?;
    assert_level_step(before_attained, after_attained, step, "attained")?;

    if after_current != before_current {
        prop_assert!(
            flags.hard_fail || flags.recover || flags.promote,
            "step {}: current {:?} -> {:?}，但没有阈值穿越",
            step,
            before_current,
            after_current
        );
    }
    if after_attained != before_attained {
        prop_assert!(
            flags.promote,
            "step {}: attained {:?} -> {:?}，但判据阈值未穿越",
            step,
            before_attained,
            after_attained
        );
    }
    if !flags.hard_fail && !flags.recover && !flags.promote {
        prop_assert_eq!(
            after_current,
            before_current,
            "step {}: 未穿越阈值但 current 变了",
            step
        );
        prop_assert_eq!(
            after_attained,
            before_attained,
            "step {}: 未穿越阈值但 attained 变了",
            step
        );
    }

    match action {
        Action::Demoted(level) => {
            prop_assert!(
                flags.hard_fail,
                "step {}: Demoted 发生在硬失败阈值之外",
                step
            );
            prop_assert_eq!(
                after_attained,
                before_attained,
                "step {}: 熔断改写了 attained",
                step
            );
            prop_assert_eq!(
                level,
                before_current.prev().unwrap_or(before_current),
                "step {}: 熔断不是降一级或触底",
                step
            );
            prop_assert_eq!(
                after_current,
                level,
                "step {}: Demoted 水位与动作不一致",
                step
            );
        }
        Action::Recovered(level) => {
            prop_assert!(
                flags.recover,
                "step {}: Recovered 发生在连净阈值穿越之外",
                step
            );
            prop_assert_eq!(
                after_attained,
                before_attained,
                "step {}: 回升改写了 attained",
                step
            );
            prop_assert!(
                level <= before_attained,
                "step {}: 回升 {:?} 越过 attained {:?}",
                step,
                level,
                before_attained
            );
            let Some(up) = before_current.next() else {
                return Err(TestCaseError::fail(format!(
                    "step {step}: Recovered 但 {before_current:?} 没有上一级"
                )));
            };
            prop_assert_eq!(level, up, "step {}: 回升跳级", step);
            prop_assert_eq!(
                after_current,
                level,
                "step {}: Recovered 水位与动作不一致",
                step
            );
        }
        Action::Promoted(level) => {
            prop_assert!(
                flags.promote,
                "step {}: Promoted 发生在判据阈值穿越之外",
                step
            );
            let Some(up) = before_current.next() else {
                return Err(TestCaseError::fail(format!(
                    "step {step}: Promoted 但 {before_current:?} 没有上一级"
                )));
            };
            prop_assert_eq!(level, up, "step {}: 晋升跳级", step);
            prop_assert_eq!(
                after_current,
                level,
                "step {}: Promoted 水位与动作不一致",
                step
            );
            prop_assert_eq!(
                after_attained,
                before_attained.max(level),
                "step {}: attained 未按晋升刷新",
                step
            );
        }
        Action::Hold => {
            prop_assert!(
                !flags.hard_fail && !flags.recover && !flags.promote,
                "step {}: 阈值已穿越但动作是 Hold (hard_fail={} recover={} promote={})",
                step,
                flags.hard_fail,
                flags.recover,
                flags.promote
            );
            prop_assert_eq!(
                after_current,
                before_current,
                "step {}: Hold 改了 current",
                step
            );
            prop_assert_eq!(
                after_attained,
                before_attained,
                "step {}: Hold 改了 attained",
                step
            );
        }
    }
    Ok(())
}

fn check_dual_watermark(case: &PromoCase) -> Result<(), TestCaseError> {
    let mut engine = engine_from(case)?;
    let recipe = engine.recipe().clone();
    let mut shadow = Shadow {
        current: Lineage::Agent,
        attained: Lineage::Agent,
        window: Vec::new(),
        clean_streak: 0,
    };
    prop_assert_eq!(engine.current(), Lineage::Agent, "起点必须是 agent");
    prop_assert_eq!(
        engine.attained(),
        Lineage::Agent,
        "已达成水位起点必须是 agent"
    );

    for (step, raw) in case.outcomes.iter().enumerate() {
        let out = to_outcome(raw);
        let flags = classify(&shadow, &recipe, out)?;
        let before_current = engine.current();
        let before_attained = engine.attained();
        let action = engine.record(out);
        prop_assert_eq!(
            action,
            flags.expected,
            "step {}: 双水位处置与阈值穿越不一致\n  before current={:?} attained={:?} streak={} window={:?}\n  hard_fail={} recover={} promote={}\n  outcome={:?}",
            step,
            before_current,
            before_attained,
            shadow.clean_streak,
            shadow.window,
            flags.hard_fail,
            flags.recover,
            flags.promote,
            out
        );
        assert_watermark_laws(
            before_current,
            before_attained,
            engine.current(),
            engine.attained(),
            action,
            &flags,
            step,
        )?;
        apply_shadow(&mut shadow, &recipe, out, action);
        prop_assert_eq!(
            engine.clean_streak(),
            shadow.clean_streak,
            "step {}: clean_streak 与阈值模型不一致 engine={} model={}",
            step,
            engine.clean_streak(),
            shadow.clean_streak
        );
        prop_assert_eq!(
            engine.current(),
            shadow.current,
            "step {}: current 与阈值模型不一致",
            step
        );
        prop_assert_eq!(
            engine.attained(),
            shadow.attained,
            "step {}: attained 与阈值模型不一致",
            step
        );
    }
    prop_assert_eq!(
        engine.events().len(),
        case.outcomes.len(),
        "事件数应等于注入的运行条数"
    );
    prop_assert!(
        engine.current() <= engine.attained(),
        "终态 current {:?} 超过 attained {:?}",
        engine.current(),
        engine.attained()
    );
    Ok(())
}

// --- 金标注入不改谱系计数 ---------------------------------------------------

struct InjectSpec {
    enabled: bool,
    i: u32,
    f_bp: u32,
    f_min_bp: u32,
    p_bp: u32,
    defect_share_bp: u32,
    inj_seed: u64,
    csp_seed: u64,
    reals: Vec<bool>,
}

impl fmt::Debug for InjectSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stream: String = self
            .reals
            .iter()
            .map(|conforming| if *conforming { 'C' } else { 'D' })
            .collect();
        write!(
            f,
            "InjectSpec {{ enabled: {}, i: {}, f: {}/10000, f_min: {}/10000, p: {}/1000, defect_share: {}/1000, inj_seed: {}, csp_seed: {}, reals: \"{stream}\" }}",
            self.enabled,
            self.i,
            self.f_bp,
            self.f_min_bp,
            self.p_bp,
            self.defect_share_bp,
            self.inj_seed,
            self.csp_seed
        )
    }
}

struct GoldenCase {
    promo: PromoCase,
    inject: InjectSpec,
}

impl fmt::Debug for GoldenCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GoldenCase {{ promo: {:?}, inject: {:?} }}",
            self.promo, self.inject
        )
    }
}

fn arb_inject_spec() -> impl Strategy<Value = InjectSpec> {
    (
        any::<bool>(),
        1u32..=24,
        1u32..=10_000,
        1u32..=10_000,
        0u32..=1000,
        0u32..=1000,
        any::<u64>(),
        any::<u64>(),
        proptest::collection::vec(any::<bool>(), 0..=16),
    )
        .prop_map(
            |(enabled, i, a, b, p_bp, defect_share_bp, inj_seed, csp_seed, reals)| {
                let (f_min_bp, f_bp) = if a <= b { (a, b) } else { (b, a) };
                InjectSpec {
                    enabled,
                    i,
                    f_bp,
                    f_min_bp,
                    p_bp,
                    defect_share_bp,
                    inj_seed,
                    csp_seed,
                    reals,
                }
            },
        )
}

fn arb_golden_case() -> impl Strategy<Value = GoldenCase> {
    (arb_promo_case(), arb_inject_spec()).prop_map(|(promo, inject)| GoldenCase { promo, inject })
}

#[derive(PartialEq, Eq)]
struct LineageSig {
    current: Lineage,
    attained: Lineage,
    clean_streak: u32,
    actions: Vec<Action>,
    /// 每次 record 之后所处谱系的次数：agent, hybrid, deterministic。
    visits: [u32; 3],
}

impl fmt::Debug for LineageSig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "LineageSig {{ current: {:?}, attained: {:?}, clean_streak: {}, visits: {{agent: {}, hybrid: {}, deterministic: {}}}, actions: {:?} }}",
            self.current,
            self.attained,
            self.clean_streak,
            self.visits[0],
            self.visits[1],
            self.visits[2],
            self.actions
        )
    }
}

fn signature(engine: &Engine) -> Result<LineageSig, TestCaseError> {
    let mut replay = Lineage::Agent;
    let mut visits = [0u32; 3];
    let mut actions = Vec::with_capacity(engine.events().len());
    for event in engine.events() {
        replay = match event.action {
            Action::Hold => replay,
            Action::Promoted(level) | Action::Demoted(level) | Action::Recovered(level) => level,
        };
        visits[lineage_index(replay)] += 1;
        actions.push(event.action);
    }
    prop_assert_eq!(
        replay,
        engine.current(),
        "动作回放谱系与 current() 不一致 actions={:?}",
        actions
    );
    Ok(LineageSig {
        current: engine.current(),
        attained: engine.attained(),
        clean_streak: engine.clean_streak(),
        actions,
        visits,
    })
}

fn replay_lineage(case: &PromoCase) -> Result<LineageSig, TestCaseError> {
    let mut engine = engine_from(case)?;
    for raw in &case.outcomes {
        engine.record(to_outcome(raw));
    }
    signature(&engine)
}

struct InjectEcho {
    injected: u64,
    defective: u64,
    clean: u64,
    golden_seen: u64,
    alarms: u64,
    released: u64,
    intercepted: u64,
    false_rejects: u64,
}

fn ground_truth(item: &Tagged) -> bool {
    match item {
        Tagged::Real { conforming, .. } => *conforming,
        Tagged::Golden { defective, .. } => !*defective,
    }
}

fn replay_with_injection(case: &GoldenCase) -> Result<(LineageSig, InjectEcho), TestCaseError> {
    let mut engine = engine_from(&case.promo)?;
    let spec = &case.inject;
    let mut injector = match GoldenInjector::new(
        InjectorConfig {
            p: milli(spec.p_bp),
            defect_share: milli(spec.defect_share_bp),
        },
        spec.inj_seed,
    ) {
        Ok(injector) => injector,
        Err(err) => {
            return Err(TestCaseError::fail(format!(
                "注入器配置被拒: {err}; {spec:?}"
            )))
        }
    };
    let mut csp = match Csp1::new(Csp1Config {
        enabled: spec.enabled,
        i: spec.i,
        f: bp_rate(spec.f_bp),
        f_min: bp_rate(spec.f_min_bp),
    }) {
        Ok(csp) => csp,
        Err(err) => {
            return Err(TestCaseError::fail(format!(
                "CSP 配置被拒: {err}; {spec:?}"
            )))
        }
    };
    let mut csp_rng = ItemRng::new(spec.csp_seed);
    let mut ledger = GoldenLedger::default();
    let mut queue = VecDeque::new();

    let steps = spec.reals.len().max(case.promo.outcomes.len());
    for step in 0..steps {
        if let Some(conforming) = spec.reals.get(step).copied() {
            injector.enqueue_real(&mut queue, conforming);
            while let Some(item) = queue.pop_front() {
                let verdict = csp.process(ground_truth(&item), &mut csp_rng);
                ledger.observe(&item, &verdict);
            }
        }
        if let Some(raw) = case.promo.outcomes.get(step) {
            engine.record(to_outcome(raw));
        }
    }

    let stats = injector.stats();
    let echo = InjectEcho {
        injected: stats.injected,
        defective: stats.defective,
        clean: stats.clean,
        golden_seen: ledger.golden_seen(),
        alarms: ledger.alarms(),
        released: ledger.defective_released(),
        intercepted: ledger.defective_intercepted(),
        false_rejects: ledger.false_rejects(),
    };
    let sig = signature(&engine)?;
    Ok((sig, echo))
}

fn check_golden_does_not_change_lineage(case: &GoldenCase) -> Result<(), TestCaseError> {
    let pure = replay_lineage(&case.promo)?;
    let (mixed, echo) = replay_with_injection(case)?;

    prop_assert_eq!(
        echo.defective + echo.clean,
        echo.injected,
        "金标构成不守恒 defective={} clean={} injected={}",
        echo.defective,
        echo.clean,
        echo.injected
    );
    prop_assert_eq!(
        echo.golden_seen,
        echo.injected,
        "台账看到的金标 {} != 注入 {}",
        echo.golden_seen,
        echo.injected
    );
    prop_assert_eq!(
        echo.alarms,
        echo.released,
        "告警 {} != 放行缺陷金标 {}",
        echo.alarms,
        echo.released
    );
    prop_assert_eq!(
        echo.intercepted + echo.released,
        echo.defective,
        "缺陷金标不守恒 intercepted={} released={} defective={}",
        echo.intercepted,
        echo.released,
        echo.defective
    );
    prop_assert_eq!(echo.false_rejects, 0, "干净金标被误拦");

    let visit_sum = mixed.visits.iter().sum::<u32>();
    prop_assert_eq!(
        visit_sum as usize,
        case.promo.outcomes.len(),
        "谱系计数之和 {} 被金标注入 {} 件改写（运行条数 {}）",
        visit_sum,
        echo.injected,
        case.promo.outcomes.len()
    );
    prop_assert_eq!(
        mixed.visits,
        pure.visits,
        "金标注入改变了谱系计数 injected={} defective={} clean={}\n  pure={:?}\n  mixed={:?}",
        echo.injected,
        echo.defective,
        echo.clean,
        &pure,
        &mixed
    );
    prop_assert_eq!(
        mixed,
        pure,
        "金标注入改变了谱系签名 injected={} defective={} clean={}",
        echo.injected,
        echo.defective,
        echo.clean
    );
    Ok(())
}

// --- 重启不丢水位 -----------------------------------------------------------

struct TempDb {
    path: PathBuf,
}

impl TempDb {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("af-sampling-prop-{}-{n}.db", std::process::id()));
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let mut wal = self.path.clone().into_os_string();
        wal.push("-wal");
        let _ = std::fs::remove_file(wal);
        let mut shm = self.path.clone().into_os_string();
        shm.push("-shm");
        let _ = std::fs::remove_file(shm);
    }
}

fn arb_restart_case() -> impl Strategy<Value = (PromoCase, usize)> {
    arb_promo_case().prop_flat_map(|case| {
        let n = case.outcomes.len();
        (Just(case), 0..=n)
    })
}

fn assert_ledger_matches(
    disk: &SamplingLedger,
    oracle: &Engine,
    split: usize,
) -> Result<(), TestCaseError> {
    prop_assert_eq!(
        disk.current(),
        oracle.current(),
        "current 在重启点 split={} 丢失",
        split
    );
    prop_assert_eq!(
        disk.attained(),
        oracle.attained(),
        "attained 在重启点 split={} 丢失",
        split
    );
    prop_assert_eq!(
        disk.clean_streak(),
        oracle.clean_streak(),
        "clean_streak 在重启点 split={} 丢失",
        split
    );
    prop_assert_eq!(
        disk.events(),
        oracle.events(),
        "谱系轨迹在重启点 split={} 丢失",
        split
    );
    prop_assert_eq!(
        disk.pending_window(),
        oracle.pending_window(),
        "判据窗口在重启点 split={} 丢失",
        split
    );
    Ok(())
}

fn reopen_ledger(
    path: &Path,
    decl: &LedgerDeclaration,
    clock: &ManualClock,
    oracle: &Engine,
    split: usize,
) -> Result<SamplingLedger, TestCaseError> {
    let (ledger, fork) = SamplingLedger::open(path, decl, clock.clone())
        .map_err(|err| TestCaseError::fail(format!("重开失败 split={split}: {err}")))?;
    prop_assert_eq!(
        fork,
        OpenFork::AlreadyDeclared,
        "重开必须走重入 split={}",
        split
    );
    assert_ledger_matches(&ledger, oracle, split)?;
    Ok(ledger)
}

fn check_restart_keeps_watermark(case: &PromoCase, split: usize) -> Result<(), TestCaseError> {
    let decl = LedgerDeclaration {
        recipe_json: recipe_json(case),
        injector: InjectorConfig {
            p: 0.0,
            defect_share: 0.7,
        },
    };
    // 固定注入时刻。若落盘改读墙钟，轨迹上的毫秒不会是 10000。
    let clock = ManualClock::new(10_000);
    let tmp = TempDb::new();
    let mut oracle = engine_from(case)?;
    let (mut disk, fork) = SamplingLedger::open(tmp.path(), &decl, clock.clone())
        .map_err(|err| TestCaseError::fail(format!("首启失败: {err}")))?;
    prop_assert_eq!(fork, OpenFork::Created, "空库必须是首启");

    for (index, raw) in case.outcomes.iter().enumerate() {
        if index == split {
            drop(disk);
            disk = reopen_ledger(tmp.path(), &decl, &clock, &oracle, split)?;
        }
        let outcome = to_outcome(raw);
        let expect = oracle.record(outcome);
        let got = disk.record(outcome).map_err(|err| {
            TestCaseError::fail(format!("step {index} 落盘失败 split={split}: {err}"))
        })?;
        prop_assert_eq!(
            got,
            expect,
            "step {} 重启后续判与未重启不一致 split={}",
            index,
            split
        );
    }
    drop(disk);
    let disk = reopen_ledger(tmp.path(), &decl, &clock, &oracle, split)?;
    let traj = disk
        .trajectory()
        .map_err(|err| TestCaseError::fail(format!("读轨迹失败 split={split}: {err}")))?;
    prop_assert_eq!(
        traj.len(),
        oracle.events().len(),
        "轨迹条数 split={}",
        split
    );
    for (row, event) in traj.iter().zip(oracle.events()) {
        prop_assert_eq!(row.tick, event.tick, "tick split={}", split);
        prop_assert_eq!(row.action, event.action, "action split={}", split);
        prop_assert_eq!(
            row.current_after,
            oracle_lineage_after(&oracle, event.tick),
            "轨迹水位 split={}",
            split
        );
        prop_assert_eq!(
            row.recorded_at_ms,
            10_000,
            "绝对时刻必须来自注入钟 split={}",
            split
        );
    }
    Ok(())
}

fn oracle_lineage_after(oracle: &Engine, tick: u64) -> Lineage {
    let mut level = Lineage::Agent;
    for event in oracle.events() {
        if let Action::Promoted(next) | Action::Demoted(next) | Action::Recovered(next) =
            event.action
        {
            level = next;
        }
        if event.tick == tick {
            break;
        }
    }
    level
}

proptest! {
    #![proptest_config(proptest_cfg())]

    /// 三不变式在任意注入序列下保持：转抽检必有 i 连检合格，回全检必为抽中缺陷，有效抽检率不低于 f_min。
    #[test]
    fn three_invariants_hold_for_any_injection_sequence(seq in arb_item_seq()) {
        check_three_invariants(&seq)?;
    }

    /// 双水位升迁只在阈值穿越时发生：硬失败才降，连净跨过 clean_to_recover 且低于 attained 才回升，窗口三判据成立才晋升。
    #[test]
    fn dual_watermark_moves_only_on_threshold_crossing(case in arb_promo_case()) {
        check_dual_watermark(&case)?;
    }

    /// 金标注入不改谱系计数：同一配方与 RunOutcome 序列，旁边注入任意金标都不改变 current/attained/动作/各谱系停留次数。
    #[test]
    fn golden_injection_does_not_change_lineage_counts(case in arb_golden_case()) {
        check_golden_does_not_change_lineage(&case)?;
    }

    /// 重启不丢水位：任意配方与结果序列，在任意一步落盘重开后，水位、窗口、轨迹与未重启的引擎一致，后续处置也一致。
    #[test]
    fn restart_does_not_drop_watermark((case, split) in arb_restart_case()) {
        check_restart_keeps_watermark(&case, split)?;
    }
}
