//! 给定账本或供给水位，生成器吐出的 Bifrost 配置片段。
//!
//! 快照在 `tests/snapshots/*.snap`，直接入库。更新时用 `INSTA_UPDATE=always`，
//! 不走 `cargo insta review`。普通 `cargo test` 只比对。
//!
//! 三份金标：预算余额 800:200、日窗耗尽后的主 key 权重 0、注入日界后水位回来。

use af_meter::bifrost::{render, KeyRole, KeySpec, Waterline, PIN_HEADER, PIN_TARGET_WEIGHT};
use af_meter::demo::{API_STOCK, DAILY_CAP, DAY, WEEK, WEEKLY_CAP};
use af_meter::{BudgetLedger, Meter};

fn spec(name: &str, role: KeyRole, waterline: Waterline) -> KeySpec {
    let upper = name.to_ascii_uppercase();
    KeySpec {
        name: name.to_string(),
        role,
        provider: "openai".to_string(),
        value: format!("env.AF_KEY_{upper}"),
        models: vec!["*".to_string()],
        waterline,
    }
}

fn budget_keys() -> Vec<KeySpec> {
    let mut primary = BudgetLedger::new(1_000);
    primary.try_spend(200, 1_000).unwrap();
    let mut backup = BudgetLedger::new(500);
    backup.try_spend(300, 500).unwrap();
    assert_eq!(primary.remaining_cents(), 800);
    assert_eq!(backup.remaining_cents(), 200);
    vec![
        spec("primary", KeyRole::Primary, Waterline::Budget(primary)),
        spec("backup", KeyRole::Backup, Waterline::Budget(backup)),
    ]
}

fn exhausted_subscription() -> Meter {
    let mut meter = Meter::min(vec![
        Meter::window(WEEK, WEEKLY_CAP, 0),
        Meter::window(DAY, DAILY_CAP, 0),
    ]);
    meter.try_consume(DAILY_CAP, 0).unwrap();
    meter
}

fn supply_keys() -> Vec<KeySpec> {
    vec![
        spec(
            "subscription",
            KeyRole::Primary,
            Waterline::Supply(exhausted_subscription()),
        ),
        spec(
            "api",
            KeyRole::Backup,
            Waterline::Supply(Meter::stock(API_STOCK)),
        ),
    ]
}

/// 每条钉住规则的目标权重是 1。这不是 key 的百分整。
fn assert_pins(fragment: &af_meter::bifrost::BifrostFragment) {
    assert!(!fragment.governance.routing_rules.is_empty());
    for rule in &fragment.governance.routing_rules {
        assert!(rule.cel_expression.contains(PIN_HEADER));
        assert_eq!(rule.targets.len(), 1);
        assert_eq!(
            rule.targets[0].weight.to_bits(),
            PIN_TARGET_WEIGHT.to_bits()
        );
        assert!(fragment.key_weight(&rule.targets[0].key_id).is_some());
    }
}

#[test]
fn budget_balances_80_20() {
    let now = 1_700_000_000;
    let fragment = render(&budget_keys(), now).unwrap();
    assert_eq!(fragment.generated_at, now);
    assert_eq!(fragment.key_weight("primary"), Some(80));
    assert_eq!(fragment.key_weight("backup"), Some(20));
    assert_pins(&fragment);
    insta::assert_json_snapshot!("budget_balances_80_20", fragment);
}

#[test]
fn supply_exhausted_primary_weight_zero() {
    let mut probe = exhausted_subscription();
    assert_eq!(probe.given(DAY - 1).available, 0, "日窗耗尽且尚未到日界");
    let now = DAY - 1;
    let fragment = render(&supply_keys(), now).unwrap();
    assert_eq!(fragment.generated_at, now);
    assert_eq!(fragment.key_weight("subscription"), Some(0));
    assert_eq!(fragment.key_weight("api"), Some(100));
    let pin = fragment.pin_rule("subscription").unwrap();
    assert_eq!(pin.targets[0].weight.to_bits(), PIN_TARGET_WEIGHT.to_bits());
    assert_pins(&fragment);
    insta::assert_json_snapshot!("supply_exhausted_primary_weight_zero", fragment);
}

#[test]
fn supply_recovers_on_injected_day_boundary() {
    let mut probe = exhausted_subscription();
    assert_eq!(probe.given(DAY - 1).available, 0);
    assert_eq!(
        probe.given(DAY).available,
        DAILY_CAP,
        "注入日界后水位回到日窗容量，周窗剩余不单独进权重"
    );
    let now = DAY;
    let fragment = render(&supply_keys(), now).unwrap();
    assert_eq!(fragment.generated_at, now);
    assert_eq!(fragment.key_weight("subscription"), Some(2));
    assert_eq!(fragment.key_weight("api"), Some(98));
    assert_pins(&fragment);
    insta::assert_json_snapshot!("supply_recovers_on_injected_day_boundary", fragment);
}
