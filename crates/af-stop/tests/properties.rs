//! af-stop 属性测试。语义以本 crate 的 RESULT.md 为准，只观察引擎、不改规则。
//!
//! 三组：
//! 1. 指纹相同则处置决策确定且一致。
//! 2. 规则优先级在任意注入顺序下判定不变。
//!    优先级：check 首败 > 指纹连击 > 普通错误阈值。
//! 3. 止损计数永不回退。
//!
//! 生成器只用四个工具名、四个错误码和 0..=8 的阈值。
//! proptest 缩小后的反例就是这些小值，可以直接读。

use af_stop::{
    Decision, EscalateReason, Event, Fingerprint, FingerprintAction, FingerprintEntry,
    FingerprintStore, StopEngine, StopPolicy, StopReason,
};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;

const TARGET_TOOL: &str = "search";
const TARGET_CODE: &str = "E_TIMEOUT";

/// 指纹库槽位。名字固定，反例里直接出现工具名而不是下标。
const SLOTS: &[(&str, &str)] = &[
    ("search", "E_TIMEOUT"),
    ("browser", "E_NAV"),
    ("deploy", "E_LOCKED"),
    ("db", "E_LOCK"),
];

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 256,
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        ..ProptestConfig::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FpAction {
    Stop,
    Escalate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OverrideSpec {
    /// `None` 表示沿用全局连击阈值。`Some(0)` 是白名单。
    limit: Option<u8>,
    action: FpAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Slot {
    tool: &'static str,
    code: &'static str,
    over: Option<OverrideSpec>,
}

#[derive(Debug, Clone)]
struct PolicySpec {
    streak_limit: u8,
    threshold: u8,
    over: Option<OverrideSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Noise {
    ToolOk {
        tool: &'static str,
    },
    Check {
        node: &'static str,
    },
    Generic {
        source: &'static str,
        code: &'static str,
    },
    OtherTool {
        tool: &'static str,
        code: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Segment {
    Streak {
        tool: &'static str,
        code: &'static str,
        repeats: u8,
    },
    Noise(Noise),
}

#[derive(Debug, Clone)]
struct StoreOrderCase {
    streak_limit: u8,
    threshold: u8,
    order_a: Vec<Slot>,
    order_b: Vec<Slot>,
    segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Replay {
    decisions: Vec<Decision>,
    count: usize,
    streak: Option<(String, String, usize)>,
}

fn arb_tool() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("search"), Just("browser"), Just("deploy"), Just("db")]
}

fn arb_code() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("E_TIMEOUT"),
        Just("E_LOCK"),
        Just("E_5XX"),
        Just("E_NAV")
    ]
}

fn arb_node() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("schema_check"), Just("format_check")]
}

fn arb_source() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("llm"), Just("net")]
}

fn arb_action() -> impl Strategy<Value = FpAction> {
    prop_oneof![Just(FpAction::Stop), Just(FpAction::Escalate)]
}

fn arb_override_spec() -> impl Strategy<Value = OverrideSpec> {
    (prop::option::of(0u8..=8), arb_action())
        .prop_map(|(limit, action)| OverrideSpec { limit, action })
}

fn arb_maybe_override() -> impl Strategy<Value = Option<OverrideSpec>> {
    prop_oneof![Just(None), arb_override_spec().prop_map(Some)]
}

fn arb_fp() -> impl Strategy<Value = Fingerprint> {
    (arb_tool(), arb_code()).prop_map(|(tool, code)| Fingerprint::new(tool, code))
}

fn arb_policy_spec() -> impl Strategy<Value = PolicySpec> {
    (0u8..=8, 0u8..=8, arb_maybe_override()).prop_map(|(streak_limit, threshold, over)| {
        PolicySpec {
            streak_limit,
            threshold,
            over,
        }
    })
}

fn arb_event() -> impl Strategy<Value = Event> {
    prop_oneof![
        (arb_tool(), arb_code()).prop_map(|(tool, code)| Event::ToolError {
            tool: tool.to_string(),
            error_code: code.to_string(),
        }),
        arb_tool().prop_map(|tool| Event::ToolOk {
            tool: tool.to_string(),
        }),
        arb_node().prop_map(|node| Event::CheckFailed {
            node: node.to_string(),
        }),
        (arb_source(), arb_code()).prop_map(|(source, code)| Event::GenericError {
            source: source.to_string(),
            error_code: code.to_string(),
        }),
    ]
}

fn arb_distractor() -> impl Strategy<Value = Noise> {
    prop_oneof![
        arb_tool().prop_map(|tool| Noise::ToolOk { tool }),
        arb_node().prop_map(|node| Noise::Check { node }),
        (arb_source(), arb_code()).prop_map(|(source, code)| Noise::Generic { source, code }),
        (arb_tool(), arb_code())
            .prop_filter(
                "prefix must not be the target fingerprint search/E_TIMEOUT",
                |&(tool, code)| !(tool == TARGET_TOOL && code == TARGET_CODE),
            )
            .prop_map(|(tool, code)| Noise::OtherTool { tool, code }),
    ]
}

#[derive(Debug, Clone, Copy)]
enum RawSegment {
    Streak { slot: usize, repeats: u8 },
    Noise(Noise),
}

fn arb_raw_segment() -> impl Strategy<Value = RawSegment> {
    prop_oneof![
        (0..SLOTS.len(), 1u8..=8).prop_map(|(slot, repeats)| RawSegment::Streak { slot, repeats }),
        arb_distractor().prop_map(RawSegment::Noise),
    ]
}

fn arb_registry() -> impl Strategy<Value = Vec<Slot>> {
    (
        prop::collection::vec(arb_maybe_override(), SLOTS.len()..=SLOTS.len()),
        prop::collection::vec(any::<u8>(), SLOTS.len()..=SLOTS.len()),
    )
        .prop_map(|(overs, keys)| order_registered(&slots_from(&overs), &keys))
}

fn arb_store_order_case() -> impl Strategy<Value = StoreOrderCase> {
    (
        0u8..=8,
        0u8..=8,
        prop::collection::vec(arb_maybe_override(), SLOTS.len()..=SLOTS.len()),
        prop::collection::vec(any::<u8>(), SLOTS.len()..=SLOTS.len()),
        prop::collection::vec(any::<u8>(), SLOTS.len()..=SLOTS.len()),
        prop::collection::vec(arb_raw_segment(), 0..=8),
    )
        .prop_map(|(streak_limit, threshold, overs, keys_a, keys_b, raw)| {
            let slots = slots_from(&overs);
            let segments = raw
                .into_iter()
                .map(|seg| match seg {
                    RawSegment::Streak { slot, repeats } => Segment::Streak {
                        tool: slots[slot].tool,
                        code: slots[slot].code,
                        repeats,
                    },
                    RawSegment::Noise(noise) => Segment::Noise(noise),
                })
                .collect();
            StoreOrderCase {
                streak_limit,
                threshold,
                order_a: order_registered(&slots, &keys_a),
                order_b: order_registered(&slots, &keys_b),
                segments,
            }
        })
}

fn slots_from(overs: &[Option<OverrideSpec>]) -> Vec<Slot> {
    SLOTS
        .iter()
        .zip(overs.iter().copied())
        .map(|(&(tool, code), over)| Slot { tool, code, over })
        .collect()
}

/// 只登记有覆盖项的槽位，顺序由 `keys` 决定。同一组覆盖项的任意顺序都来自这里。
fn order_registered(slots: &[Slot], keys: &[u8]) -> Vec<Slot> {
    let mut idx: Vec<usize> = (0..slots.len())
        .filter(|&i| slots[i].over.is_some())
        .collect();
    idx.sort_by(|&i, &j| keys[i].cmp(&keys[j]).then(i.cmp(&j)));
    idx.into_iter().map(|i| slots[i]).collect()
}

fn action_of(action: FpAction) -> FingerprintAction {
    match action {
        FpAction::Stop => FingerprintAction::StopNoInfluence,
        FpAction::Escalate => FingerprintAction::Escalate,
    }
}

fn entry_of(spec: OverrideSpec) -> FingerprintEntry {
    FingerprintEntry {
        streak_limit: spec.limit.map(usize::from),
        action: action_of(spec.action),
    }
}

fn policy_from_slots(streak_limit: u8, threshold: u8, slots: &[Slot]) -> StopPolicy {
    let mut store = FingerprintStore::new();
    for slot in slots {
        if let Some(over) = slot.over {
            store.register(Fingerprint::new(slot.tool, slot.code), entry_of(over));
        }
    }
    StopPolicy {
        fingerprint_streak_limit: usize::from(streak_limit),
        normal_error_threshold: usize::from(threshold),
        store,
    }
}

impl PolicySpec {
    fn apply(&self, fp: &Fingerprint) -> StopPolicy {
        let mut policy = StopPolicy {
            fingerprint_streak_limit: usize::from(self.streak_limit),
            normal_error_threshold: usize::from(self.threshold),
            store: FingerprintStore::new(),
        };
        if let Some(over) = self.over {
            policy.store.register(fp.clone(), entry_of(over));
        }
        policy
    }
}

fn effective_limit(spec: &PolicySpec) -> usize {
    match spec.over {
        Some(over) => over
            .limit
            .map(usize::from)
            .unwrap_or(usize::from(spec.streak_limit)),
        None => usize::from(spec.streak_limit),
    }
}

fn rule1_decision(action: FpAction) -> Decision {
    match action {
        FpAction::Stop => Decision::Stop {
            reason: StopReason::NoInfluence,
        },
        FpAction::Escalate => Decision::Escalate {
            reason: EscalateReason::FingerprintPolicy,
        },
    }
}

fn rule1_of(spec: &PolicySpec) -> Decision {
    match spec.over.map(|over| over.action) {
        Some(action) => rule1_decision(action),
        None => Decision::Stop {
            reason: StopReason::NoInfluence,
        },
    }
}

/// 纯连击第 `step` 拍（从 1 计）的裁决。
/// `limit == 0` 时规则①不触发。规则①先于规则③。
fn pure_streak_decision(step: usize, limit: usize, threshold: usize, rule1: &Decision) -> Decision {
    if limit > 0 && step >= limit {
        rule1.clone()
    } else if step >= threshold {
        Decision::Escalate {
            reason: EscalateReason::NormalErrorThreshold,
        }
    } else {
        Decision::Continue
    }
}

fn tool_event(fp: &Fingerprint) -> Event {
    Event::ToolError {
        tool: fp.tool.clone(),
        error_code: fp.error_code.clone(),
    }
}

fn target_event() -> Event {
    Event::ToolError {
        tool: TARGET_TOOL.to_string(),
        error_code: TARGET_CODE.to_string(),
    }
}

fn noise_event(noise: Noise) -> Event {
    match noise {
        Noise::ToolOk { tool } => Event::ToolOk {
            tool: tool.to_string(),
        },
        Noise::Check { node } => Event::CheckFailed {
            node: node.to_string(),
        },
        Noise::Generic { source, code } => Event::GenericError {
            source: source.to_string(),
            error_code: code.to_string(),
        },
        Noise::OtherTool { tool, code } => Event::ToolError {
            tool: tool.to_string(),
            error_code: code.to_string(),
        },
    }
}

fn flatten(segments: &[Segment]) -> Vec<Event> {
    let mut out = Vec::new();
    for seg in segments {
        match *seg {
            Segment::Streak {
                tool,
                code,
                repeats,
            } => {
                for _ in 0..repeats {
                    out.push(Event::ToolError {
                        tool: tool.to_string(),
                        error_code: code.to_string(),
                    });
                }
            }
            Segment::Noise(noise) => out.push(noise_event(noise)),
        }
    }
    out
}

fn replay(policy: StopPolicy, events: &[Event]) -> Replay {
    let mut eng = StopEngine::new(policy);
    let mut decisions = Vec::with_capacity(events.len());
    for event in events {
        decisions.push(eng.feed(event.clone()));
    }
    let streak = eng
        .current_streak()
        .map(|(fp, len)| (fp.tool.clone(), fp.error_code.clone(), len));
    Replay {
        decisions,
        count: eng.normal_error_count(),
        streak,
    }
}

fn counts_toward_threshold(event: &Event) -> bool {
    matches!(event, Event::ToolError { .. } | Event::GenericError { .. })
}

/// 连击只允许 +1，或在打断后落到空 / 新指纹的 1。不允许 5 → 3 这种部分回退。
fn next_streak(
    streak: &Option<(Fingerprint, usize)>,
    event: &Event,
) -> Option<(Fingerprint, usize)> {
    match event {
        Event::ToolError { tool, error_code } => {
            let fp = Fingerprint::new(tool.clone(), error_code.clone());
            let len = match streak {
                Some((cur, len)) if cur == &fp => len + 1,
                _ => 1,
            };
            Some((fp, len))
        }
        Event::ToolOk { .. } | Event::CheckFailed { .. } | Event::GenericError { .. } => None,
    }
}

fn registration_set(slots: &[Slot]) -> Vec<(&'static str, &'static str, Option<OverrideSpec>)> {
    let mut rows: Vec<_> = slots.iter().map(|s| (s.tool, s.code, s.over)).collect();
    rows.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    rows
}

/// 同一条有效规则的三种注入形态。
/// 形态 A 只在动作为 Stop 时存在：没有指纹库条目就表达不了 Escalate。
fn policies_for(limit: u8, threshold: u8, action: FpAction) -> Vec<(&'static str, StopPolicy)> {
    let fp = Fingerprint::new(TARGET_TOOL, TARGET_CODE);
    let mut out = Vec::new();
    if action == FpAction::Stop {
        out.push((
            "global",
            StopPolicy {
                fingerprint_streak_limit: usize::from(limit),
                normal_error_threshold: usize::from(threshold),
                store: FingerprintStore::new(),
            },
        ));
    }
    let mut stored_limit = FingerprintStore::new();
    stored_limit.register(
        fp.clone(),
        FingerprintEntry {
            streak_limit: Some(usize::from(limit)),
            action: action_of(action),
        },
    );
    out.push((
        "store-limit",
        StopPolicy {
            // 全局阈值故意高于任何前缀，连击只可能由指纹库触发。
            fingerprint_streak_limit: 64,
            normal_error_threshold: usize::from(threshold),
            store: stored_limit,
        },
    ));
    let mut inherit = FingerprintStore::new();
    inherit.register(
        fp,
        FingerprintEntry {
            streak_limit: None,
            action: action_of(action),
        },
    );
    out.push((
        "store-inherit",
        StopPolicy {
            fingerprint_streak_limit: usize::from(limit),
            normal_error_threshold: usize::from(threshold),
            store: inherit,
        },
    ));
    out
}

proptest! {
    #![proptest_config(config())]

    /// 组 1：同一指纹反复注入，两台引擎的裁决、连击长度、普通错误计数都一致，
    /// 并且每一步都符合「达到阈值即触发、规则①先于规则③」。
    #[test]
    fn same_fingerprint_disposition_is_deterministic_and_consistent(
        spec in arb_policy_spec(),
        fp in arb_fp(),
        steps in 1usize..=12,
    ) {
        let limit = effective_limit(&spec);
        let rule1 = rule1_of(&spec);
        let mut first = StopEngine::new(spec.apply(&fp));
        let mut second = StopEngine::new(spec.apply(&fp));
        for step in 1..=steps {
            let event = tool_event(&fp);
            let left = first.feed(event.clone());
            let right = second.feed(event);
            let expected = pure_streak_decision(step, limit, usize::from(spec.threshold), &rule1);
            prop_assert_eq!(
                left.clone(),
                right,
                "两台引擎对同一指纹裁决不一致 step={} fp={:?} spec={:?}",
                step,
                fp,
                spec
            );
            prop_assert_eq!(
                left,
                expected,
                "纯连击裁决偏离已验证规则 step={} fp={:?} spec={:?}",
                step,
                fp,
                spec
            );
            prop_assert_eq!(
                first.normal_error_count(),
                step,
                "普通错误计数回退 step={}",
                step
            );
            prop_assert_eq!(
                second.normal_error_count(),
                step,
                "普通错误计数回退 step={}",
                step
            );
            prop_assert_eq!(
                first.current_streak(),
                Some((&fp, step)),
                "连击长度回退 step={}",
                step
            );
            prop_assert_eq!(
                second.current_streak(),
                Some((&fp, step)),
                "连击长度回退 step={}",
                step
            );
        }
    }

    /// 组 1：策略相同则处置相同。工具名和错误码的具体字符串不参与裁决，
    /// 只有「是不是同一指纹」和注入的阈值、动作参与。
    #[test]
    fn equivalent_fingerprints_share_the_same_disposition(
        spec in arb_policy_spec(),
        fp_a in arb_fp(),
        fp_b in arb_fp(),
        steps in 1usize..=12,
    ) {
        let mut first = StopEngine::new(spec.apply(&fp_a));
        let mut second = StopEngine::new(spec.apply(&fp_b));
        for step in 1..=steps {
            let left = first.feed(tool_event(&fp_a));
            let right = second.feed(tool_event(&fp_b));
            prop_assert_eq!(
                left,
                right,
                "相同策略下两条指纹的裁决不一致 step={} fp_a={:?} fp_b={:?} spec={:?}",
                step,
                fp_a,
                fp_b,
                spec
            );
            prop_assert_eq!(first.normal_error_count(), step);
            prop_assert_eq!(second.normal_error_count(), step);
            prop_assert_eq!(first.current_streak().map(|(_, len)| len), Some(step));
            prop_assert_eq!(second.current_streak().map(|(_, len)| len), Some(step));
        }
    }

    /// 组 2：同一组指纹库条目，按两种顺序登记，再喂同一段事件。
    /// 裁决、普通错误计数、连击状态都不变。
    #[test]
    fn rule_priority_invariant_under_store_injection_order(case in arb_store_order_case()) {
        prop_assert_eq!(
            registration_set(&case.order_a),
            registration_set(&case.order_b),
            "两种注入顺序登记的不是同一组指纹"
        );
        let events = flatten(&case.segments);
        let left = replay(
            policy_from_slots(case.streak_limit, case.threshold, &case.order_a),
            &events,
        );
        let right = replay(
            policy_from_slots(case.streak_limit, case.threshold, &case.order_b),
            &events,
        );
        prop_assert_eq!(
            left,
            right,
            "指纹库注入顺序改变了裁决 order_a={:?} order_b={:?} segments={:?}",
            case.order_a,
            case.order_b,
            case.segments
        );
    }

    /// 组 2：任意前缀、任意指纹库注入顺序之后，check 失败仍是最高优先级。
    /// 它同时打断连击，并且不计入普通错误。引擎不锁存，前缀里已经 Stop 也一样。
    #[test]
    fn check_priority_invariant_under_event_injection_order(
        streak_limit in 0u8..=8,
        threshold in 0u8..=8,
        registry in arb_registry(),
        prefix in prop::collection::vec(arb_event(), 0..=12),
        node in arb_node(),
    ) {
        let mut eng = StopEngine::new(policy_from_slots(streak_limit, threshold, &registry));
        for event in &prefix {
            eng.feed(event.clone());
        }
        let before = eng.normal_error_count();
        let decision = eng.feed(Event::CheckFailed {
            node: node.to_string(),
        });
        prop_assert_eq!(
            decision,
            Decision::Stop {
                reason: StopReason::CheckFirstFail,
            },
            "check 没有压过其他规则 streak_limit={} threshold={} registry={:?} prefix={:?} node={}",
            streak_limit,
            threshold,
            registry,
            prefix,
            node
        );
        prop_assert_eq!(eng.current_streak(), None, "check 没有打断连击");
        prop_assert_eq!(eng.normal_error_count(), before, "check 改写了普通错误计数");
    }

    /// 组 2：目标指纹的连击一旦达到有效阈值，处置就是指纹动作。
    /// 前缀可以是任意不含该指纹的事件（任意注入顺序），普通错误计数可以已经越过 M。
    /// 三种策略注入形态给出同一个结果。
    #[test]
    fn fingerprint_priority_invariant_under_event_injection_order(
        limit in 1u8..=6,
        threshold in 0u8..=8,
        action in arb_action(),
        prefix in prop::collection::vec(arb_distractor(), 0..=10),
    ) {
        let expected = rule1_decision(action);
        for (shape, policy) in policies_for(limit, threshold, action) {
            let mut eng = StopEngine::new(policy);
            for noise in &prefix {
                eng.feed(noise_event(*noise));
            }
            let mut decision = Decision::Continue;
            for step in 1..=usize::from(limit) {
                decision = eng.feed(target_event());
                if step < usize::from(limit) {
                    prop_assert!(
                        matches!(
                            decision,
                            Decision::Continue
                                | Decision::Escalate {
                                    reason: EscalateReason::NormalErrorThreshold,
                                }
                        ),
                        "未到连击阈值就提前处置 shape={} step={} decision={:?} limit={} threshold={} action={:?} prefix={:?}",
                        shape,
                        step,
                        decision,
                        limit,
                        threshold,
                        action,
                        prefix
                    );
                }
            }
            prop_assert_eq!(
                &decision,
                &expected,
                "连击没有压过普通阈值 shape={} limit={} threshold={} action={:?} prefix={:?}",
                shape,
                limit,
                threshold,
                action,
                prefix
            );
            prop_assert_eq!(
                eng.current_streak().map(|(_, len)| len),
                Some(usize::from(limit)),
                "目标连击长度回退 shape={} limit={} prefix={:?}",
                shape,
                limit,
                prefix
            );
        }
    }

    /// 组 3：普通错误计数沿任意事件流只增不减，并且等于工具错误 + 普通错误的个数。
    /// 连击长度在同一指纹上只 +1；成功、check、普通错误、不同指纹把它落到空或 1，不做部分回退。
    /// Stop / Escalate 都不把计数清掉（引擎不锁存，也不回滚）。
    #[test]
    fn stop_counter_never_regresses(
        streak_limit in 0u8..=8,
        threshold in 0u8..=8,
        registry in arb_registry(),
        events in prop::collection::vec(arb_event(), 0..=16),
    ) {
        let mut eng = StopEngine::new(policy_from_slots(streak_limit, threshold, &registry));
        let mut previous = 0usize;
        let mut expected = 0usize;
        let mut streak: Option<(Fingerprint, usize)> = None;
        for (index, event) in events.iter().enumerate() {
            let decision = eng.feed(event.clone());
            let count = eng.normal_error_count();
            prop_assert!(
                count >= previous,
                "止损计数回退 step={}: {} -> {} event={:?} decision={:?} registry={:?}",
                index,
                previous,
                count,
                event,
                decision,
                registry
            );
            if counts_toward_threshold(event) {
                expected += 1;
            }
            prop_assert_eq!(
                count,
                expected,
                "止损计数偏离事件个数 step={} event={:?} decision={:?}",
                index,
                event,
                decision
            );
            previous = count;
            streak = next_streak(&streak, event);
            let actual = eng.current_streak().map(|(fp, len)| (fp.clone(), len));
            prop_assert_eq!(
                actual,
                streak.clone(),
                "连击计数部分回退 step={} event={:?} decision={:?}",
                index,
                event,
                decision
            );
        }
    }
}
