//! 把供给水位或预算余额写成 Bifrost 配置片段。
//!
//! 两层一起输出，请求时才叠加：
//! - **weight 层**：`providers.<name>.keys[].weight`。余额占合计的比例，量化成
//!   0–100 的百分整。Bifrost core 的 `WeightedRandom`（transports v1.6 仍走这条）
//!   用 `int(key.Weight * 100)` 分桶；和为 0 时改成均匀随机，和为负时 `rand.Intn` panic。
//!   这里用四舍五入（一半向上）写出已经落桶的整数，再做保底校验：主 key 为 0 时，
//!   至少一把备 key 必须大于 0，否则拒绝生成。
//! - **header 层**：`governance.routing_rules`。请求带 `x-af-pin-key: <key>` 时钉住该 key，
//!   盖过 weight 层。`targets[].weight` 与 `keys[].weight` 同名不同义：前者是这一条规则
//!   内部的命中概率，单目标钉住时恒为 [`PIN_TARGET_WEIGHT`]（加总为 1），不写预算比例，
//!   也不做 `int(w*100)`。
//!
//! 生成器是纯函数。唯一的时刻是调用方传入的 `now`：供给走 [`Meter::given`] 的克隆，
//! 不刷新调用方手里的树；账本没有时钟，`remaining_cents` 与 `now` 无关，`now` 仍写入片段。
//! 不读墙钟。

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::{BudgetLedger, Meter, Time};

/// 钉住某把 key 的请求头。Bifrost 比较前会把 header 名转成小写，这里已经是小写。
pub const PIN_HEADER: &str = "x-af-pin-key";

/// key 权重的百分刻度。1 表示 1%。
pub const PERCENT: u8 = 100;

/// 路由规则单目标的命中概率。不是 key 的百分权重。
pub const PIN_TARGET_WEIGHT: f64 = 1.0;

/// 拒绝生成配置的原因。拒绝时没有片段可下发。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BifrostError {
    /// 一把 key 都没有。
    #[error("no keys")]
    Empty,
    /// 名字会进入 CEL 字面量，只接受 token。
    #[error("key name {name} is not a token")]
    BadName { name: String },
    /// provider 会成为 `providers` 的对象键。
    #[error("provider {provider} is not a token")]
    BadProvider { provider: String },
    /// Bifrost 要求密钥写成 `env.VAR`，配置文件里不放原始密钥。
    #[error("key {name} value must be an env.VAR reference")]
    RawValue { name: String },
    /// 同名 key 会让 `x-af-pin-key` 指代不明。
    #[error("duplicate key name {name}")]
    DuplicateName { name: String },
    /// `keys[].weight` 只在同一 provider 的 key 之间比较。跨 provider 的比例是另一层 weight。
    #[error("providers differ: {first} vs {other}")]
    MixedProviders { first: String, other: String },
    /// 保底规则要有唯一的主 key。
    #[error("missing primary key")]
    MissingPrimary,
    /// 多于一把主 key。
    #[error("more than one primary key")]
    MultiplePrimary,
    /// 保底规则要有备 key。
    #[error("missing backup key")]
    MissingBackup,
    /// [`BudgetLedger::remaining_cents`] 为负。写成 weight 会让 V1.6 的 `rand.Intn` panic。
    #[error("key {name} balance {balance} is negative")]
    NegativeBalance { name: String, balance: i64 },
    /// 余额合计超出 `u128`。
    #[error("balance sum overflows")]
    BalanceOverflow,
    /// 量化之后主 key 为 0，备 key 也都为 0。交出去会退化成均匀随机。
    #[error("primary key {primary} weight is 0 and every backup weight is 0")]
    ZeroWeights { primary: String },
}

/// 主 key 耗尽时流量应落到备 key，而不是均匀随机。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRole {
    /// 主 key。量化权重为 0 时，备 key 必须还有正权重。
    Primary,
    /// 备 key。
    Backup,
}

/// 权重分子。两种来源都是“还剩多少”，单位各算各的，调用方不要混在同一组里。
#[derive(Debug, Clone)]
pub enum Waterline {
    /// 供给水位：[`Meter::given`] 的 `available`（[`crate::Count`]）。
    Supply(Meter),
    /// 预算余额：[`BudgetLedger::remaining_cents`]（美分）。账本不看时刻。
    Budget(BudgetLedger),
}

/// 一把要写进 Bifrost 的 key，以及它的余额来源。
#[derive(Debug, Clone)]
pub struct KeySpec {
    /// 同时作为 `keys[].id`、`keys[].name` 和路由规则的 `key_id`。
    pub name: String,
    pub role: KeyRole,
    /// Bifrost provider 对象键，例如 `openai`。一组 key 必须相同。
    pub provider: String,
    /// `env.VAR` 引用。不接受原始密钥。
    pub value: String,
    /// 原样写入 `models`。空数组在 Bifrost v1.5 起是拒绝全部模型。
    pub models: Vec<String>,
    pub waterline: Waterline,
}

/// 可并进 Bifrost `config.json` 的片段。未知的 `generated_at` 会被 encoding/json 丢掉。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BifrostFragment {
    pub providers: BTreeMap<String, ProviderConfig>,
    pub governance: Governance,
    /// 调用方注入的时刻（整数秒）。预算路径不使用它计算权重，但仍记在这里。
    pub generated_at: Time,
}

/// 一个 provider 下的 key 列表。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProviderConfig {
    pub keys: Vec<ProviderKey>,
}

/// `keys[]` 的一项。`weight` 是百分整，不是路由目标的命中概率。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProviderKey {
    pub id: String,
    pub name: String,
    pub value: String,
    pub models: Vec<String>,
    /// 0–100。V1.6 选择器还会再乘 100，比例不变。
    pub weight: u8,
}

/// `governance` 对象。这里只放 header 钉住规则。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Governance {
    pub routing_rules: Vec<RoutingRule>,
}

/// 一条 CEL 路由规则。命中后按 `targets` 选目标，不再走 key 权重。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoutingRule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub cel_expression: String,
    pub targets: Vec<RoutingTarget>,
    pub fallbacks: Vec<String>,
    pub scope: String,
    /// 数字越小越先评估。钉住规则互斥，都用 0。
    pub priority: i32,
}

/// 路由目标。`weight` 是本规则内的命中概率，单目标时为 [`PIN_TARGET_WEIGHT`]。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoutingTarget {
    pub provider: String,
    pub key_id: String,
    pub weight: f64,
}

impl BifrostFragment {
    /// 按名字取 key 的百分权重。
    pub fn key_weight(&self, name: &str) -> Option<u8> {
        self.providers
            .values()
            .flat_map(|provider| provider.keys.iter())
            .find(|key| key.name == name)
            .map(|key| key.weight)
    }

    /// 钉住这把 key 的规则。
    pub fn pin_rule(&self, name: &str) -> Option<&RoutingRule> {
        self.governance
            .routing_rules
            .iter()
            .find(|rule| rule.targets.iter().any(|target| target.key_id == name))
    }
}

/// `balance / total * 100`，一半向上，结果落在 0–100。
///
/// `total == 0` 或 `balance == 0` 得 0。正好一半（余数的两倍等于分母）进一，
/// 不沿用 Go `int()` 的截断：截断会把 0.5% 收成 0，从而触发全零退化。
pub fn quantize_percent(balance: u128, total: u128) -> u8 {
    if total == 0 || balance == 0 {
        return 0;
    }
    let numer = balance.saturating_mul(u128::from(PERCENT));
    let quotient = numer / total;
    let remainder = numer % total;
    let rounded = if remainder.saturating_mul(2) >= total {
        quotient.saturating_add(1)
    } else {
        quotient
    };
    match u8::try_from(rounded.min(u128::from(PERCENT))) {
        Ok(percent) => percent,
        Err(_) => PERCENT,
    }
}

/// 生成 Bifrost 片段。
///
/// `now` 只传给供给的 [`Meter::given`]。传入的 [`KeySpec`] 不会被刷新或扣减。
pub fn render(keys: &[KeySpec], now: Time) -> Result<BifrostFragment, BifrostError> {
    validate(keys)?;
    let balances = read_balances(keys, now)?;
    let total = sum_balances(&balances)?;
    let weights: Vec<u8> = balances
        .iter()
        .map(|balance| quantize_percent(*balance, total))
        .collect();
    ensure_backup_survives(keys, &weights)?;

    let provider = keys[0].provider.clone();
    let description = format!(
        "header {PIN_HEADER} selects this key and overrides keys[].weight; target.weight is 1, not the key percent"
    );
    let mut provider_keys = Vec::with_capacity(keys.len());
    let mut routing_rules = Vec::with_capacity(keys.len());
    for (key, weight) in keys.iter().zip(weights) {
        provider_keys.push(ProviderKey {
            id: key.name.clone(),
            name: key.name.clone(),
            value: key.value.clone(),
            models: key.models.clone(),
            weight,
        });
        routing_rules.push(RoutingRule {
            id: format!("af-pin-{}", key.name),
            name: format!("pin {}", key.name),
            description: description.clone(),
            enabled: true,
            cel_expression: format!("headers[\"{PIN_HEADER}\"] == \"{}\"", key.name),
            targets: vec![RoutingTarget {
                provider: provider.clone(),
                key_id: key.name.clone(),
                weight: PIN_TARGET_WEIGHT,
            }],
            fallbacks: Vec::new(),
            scope: "global".to_string(),
            priority: 0,
        });
    }

    let mut providers = BTreeMap::new();
    providers.insert(
        provider,
        ProviderConfig {
            keys: provider_keys,
        },
    );
    Ok(BifrostFragment {
        providers,
        governance: Governance { routing_rules },
        generated_at: now,
    })
}

fn validate(keys: &[KeySpec]) -> Result<(), BifrostError> {
    if keys.is_empty() {
        return Err(BifrostError::Empty);
    }
    let provider = &keys[0].provider;
    let mut seen = BTreeSet::new();
    let mut primaries = 0usize;
    let mut backups = 0usize;
    for key in keys {
        if !is_token(&key.name) {
            return Err(BifrostError::BadName {
                name: key.name.clone(),
            });
        }
        if !is_token(&key.provider) {
            return Err(BifrostError::BadProvider {
                provider: key.provider.clone(),
            });
        }
        if !is_env_ref(&key.value) {
            return Err(BifrostError::RawValue {
                name: key.name.clone(),
            });
        }
        if key.provider != *provider {
            return Err(BifrostError::MixedProviders {
                first: provider.clone(),
                other: key.provider.clone(),
            });
        }
        if !seen.insert(key.name.clone()) {
            return Err(BifrostError::DuplicateName {
                name: key.name.clone(),
            });
        }
        match key.role {
            KeyRole::Primary => primaries += 1,
            KeyRole::Backup => backups += 1,
        }
    }
    if primaries == 0 {
        return Err(BifrostError::MissingPrimary);
    }
    if primaries > 1 {
        return Err(BifrostError::MultiplePrimary);
    }
    if backups == 0 {
        return Err(BifrostError::MissingBackup);
    }
    Ok(())
}

fn read_balances(keys: &[KeySpec], now: Time) -> Result<Vec<u128>, BifrostError> {
    keys.iter().map(|key| read_balance(key, now)).collect()
}

fn read_balance(key: &KeySpec, now: Time) -> Result<u128, BifrostError> {
    match &key.waterline {
        Waterline::Supply(meter) => Ok(u128::from(meter.clone().given(now).available)),
        Waterline::Budget(ledger) => {
            let cents = ledger.remaining_cents();
            match u64::try_from(cents) {
                Ok(cents) => Ok(u128::from(cents)),
                Err(_) => Err(BifrostError::NegativeBalance {
                    name: key.name.clone(),
                    balance: cents,
                }),
            }
        }
    }
}

fn sum_balances(balances: &[u128]) -> Result<u128, BifrostError> {
    let mut total = 0u128;
    for balance in balances {
        total = total
            .checked_add(*balance)
            .ok_or(BifrostError::BalanceOverflow)?;
    }
    Ok(total)
}

/// 主 key 量化成 0 时，备 key 里必须还有正权重。全 0 会让 V1.6 均匀随机。
fn ensure_backup_survives(keys: &[KeySpec], weights: &[u8]) -> Result<(), BifrostError> {
    let Some(primary_at) = keys.iter().position(|key| key.role == KeyRole::Primary) else {
        return Err(BifrostError::MissingPrimary);
    };
    if weights[primary_at] != 0 {
        return Ok(());
    }
    let backup_positive = keys
        .iter()
        .zip(weights)
        .any(|(key, weight)| key.role == KeyRole::Backup && *weight > 0);
    if backup_positive {
        Ok(())
    } else {
        Err(BifrostError::ZeroWeights {
            primary: keys[primary_at].name.clone(),
        })
    }
}

/// CEL 字面量与 provider 对象键用的 token：字母或数字开头，其余只含 `-` `_` `.`。
fn is_token(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

fn is_env_ref(value: &str) -> bool {
    value.strip_prefix("env.").is_some_and(is_token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::{API_STOCK, DAILY_CAP, DAY, WEEK, WEEKLY_CAP};
    use crate::Meter;

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

    fn stock(n: u64) -> Waterline {
        Waterline::Supply(Meter::stock(n))
    }

    fn budget(budget_cents: i64, spent_cents: i64) -> Waterline {
        Waterline::Budget(BudgetLedger {
            budget_cents,
            spent_cents,
        })
    }

    fn pair(primary: Waterline, backup: Waterline) -> Vec<KeySpec> {
        vec![
            spec("primary", KeyRole::Primary, primary),
            spec("backup", KeyRole::Backup, backup),
        ]
    }

    #[test]
    fn quantize_percent_rounds_half_up() {
        assert_eq!(quantize_percent(0, 0), 0);
        assert_eq!(quantize_percent(0, 10), 0);
        assert_eq!(quantize_percent(1, 1), 100);
        assert_eq!(quantize_percent(1, 2), 50);
        // 100/3 = 33.333… → 33；200/3 = 66.666… → 67。
        assert_eq!(quantize_percent(1, 3), 33);
        assert_eq!(quantize_percent(2, 3), 67);
        // 正好一半向上：0.5% → 1，99.5% → 100。Go 的 int() 会把 0.5 截成 0。
        assert_eq!(quantize_percent(1, 200), 1);
        assert_eq!(quantize_percent(199, 200), 100);
        // 低于一半则舍去。
        assert_eq!(quantize_percent(1, 201), 0);
        assert_eq!(quantize_percent(1, 10_000), 0);
    }

    #[test]
    fn half_up_is_not_renormalized_to_one_hundred() {
        let fragment = render(&pair(stock(1), stock(199)), 0).unwrap();
        assert_eq!(fragment.key_weight("primary"), Some(1));
        assert_eq!(fragment.key_weight("backup"), Some(100));
        let sum: u32 = fragment
            .providers
            .values()
            .flat_map(|provider| provider.keys.iter())
            .map(|key| u32::from(key.weight))
            .sum();
        assert_eq!(sum, 101);
    }

    #[test]
    fn budget_ratio_uses_remaining_cents() {
        let mut primary = BudgetLedger::new(1_000);
        primary.try_spend(200, 1_000).unwrap();
        let mut backup = BudgetLedger::new(500);
        backup.try_spend(300, 500).unwrap();
        assert_eq!(primary.remaining_cents(), 800);
        assert_eq!(backup.remaining_cents(), 200);

        let fragment = render(
            &pair(Waterline::Budget(primary), Waterline::Budget(backup)),
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(fragment.generated_at, 1_700_000_000);
        assert_eq!(fragment.key_weight("primary"), Some(80));
        assert_eq!(fragment.key_weight("backup"), Some(20));
    }

    #[test]
    fn negative_remaining_is_refused_before_any_fragment() {
        let err = render(&pair(budget(-5, 0), stock(10)), 0).unwrap_err();
        assert_eq!(
            err,
            BifrostError::NegativeBalance {
                name: "primary".to_string(),
                balance: -5,
            }
        );
    }

    #[test]
    fn spent_above_budget_is_negative_remaining_and_refused() {
        // remaining_cents 是普通减法，只有 i64 下溢才停在 MIN。10 - 20 = -10，不是 0。
        let ledger = BudgetLedger {
            budget_cents: 10,
            spent_cents: 20,
        };
        assert_eq!(ledger.remaining_cents(), -10);
        let err = render(&pair(budget(10, 20), stock(4)), 7).unwrap_err();
        assert_eq!(
            err,
            BifrostError::NegativeBalance {
                name: "primary".to_string(),
                balance: -10,
            }
        );
    }

    #[test]
    fn exact_exhaustion_is_zero_weight_not_a_negative() {
        let fragment = render(&pair(budget(10, 10), stock(4)), 7).unwrap();
        assert_eq!(fragment.key_weight("primary"), Some(0));
        assert_eq!(fragment.key_weight("backup"), Some(100));
        assert_eq!(fragment.generated_at, 7);
        let err = render(&pair(budget(10, 10), budget(3, 3)), 7).unwrap_err();
        assert_eq!(
            err,
            BifrostError::ZeroWeights {
                primary: "primary".to_string(),
            }
        );
    }

    #[test]
    fn all_zero_weights_are_refused() {
        let err = render(&pair(stock(0), stock(0)), 0).unwrap_err();
        assert_eq!(
            err,
            BifrostError::ZeroWeights {
                primary: "primary".to_string(),
            }
        );
    }

    #[test]
    fn primary_zero_with_positive_backup_is_kept() {
        let fragment = render(&pair(stock(0), stock(5)), 3).unwrap();
        assert_eq!(fragment.key_weight("primary"), Some(0));
        assert_eq!(fragment.key_weight("backup"), Some(100));
        assert_eq!(fragment.generated_at, 3);
    }

    #[test]
    fn rounding_collapse_to_all_zero_is_refused() {
        // 201 份相等余额，每份 100/201 < 0.5，四舍五入后全是 0。
        // 原始余额都是正的，但量化结果会让 V1.6 均匀随机，因此拒绝。
        let mut keys = Vec::with_capacity(201);
        keys.push(spec("primary", KeyRole::Primary, stock(1)));
        for index in 0..200 {
            keys.push(spec(&format!("b{index}"), KeyRole::Backup, stock(1)));
        }
        let err = render(&keys, 0).unwrap_err();
        assert_eq!(
            err,
            BifrostError::ZeroWeights {
                primary: "primary".to_string(),
            }
        );
    }

    #[test]
    fn one_positive_backup_keeps_a_zero_primary() {
        let mut keys = pair(stock(0), stock(0));
        keys.push(spec("spare", KeyRole::Backup, stock(8)));
        let fragment = render(&keys, 1).unwrap();
        assert_eq!(fragment.key_weight("primary"), Some(0));
        assert_eq!(fragment.key_weight("backup"), Some(0));
        assert_eq!(fragment.key_weight("spare"), Some(100));
    }

    #[test]
    fn injected_now_refreshes_a_clone_and_leaves_the_supply() {
        let mut subscription = Meter::min(vec![
            Meter::window(WEEK, WEEKLY_CAP, 0),
            Meter::window(DAY, DAILY_CAP, 0),
        ]);
        subscription.try_consume(DAILY_CAP, 0).unwrap();
        let untouched = subscription.clone();

        let mut probe = subscription.clone();
        assert_eq!(probe.given(DAY - 1).available, 0);
        // 日窗在注入的日界重置；周窗仍绑着剩余 800，水位是 Min，不是周窗本身。
        assert_eq!(probe.given(DAY).available, DAILY_CAP);

        let keys = pair(Waterline::Supply(subscription.clone()), stock(API_STOCK));
        let exhausted = render(&keys, DAY - 1).unwrap();
        assert_eq!(exhausted.generated_at, DAY - 1);
        assert_eq!(exhausted.key_weight("primary"), Some(0));
        assert_eq!(exhausted.key_weight("backup"), Some(100));

        let restored = render(&keys, DAY).unwrap();
        assert_eq!(restored.generated_at, DAY);
        assert_eq!(restored.key_weight("primary"), Some(2));
        assert_eq!(restored.key_weight("backup"), Some(98));

        match &keys[0].waterline {
            Waterline::Supply(meter) => assert_eq!(
                serde_json::to_value(meter).unwrap(),
                serde_json::to_value(&untouched).unwrap(),
                "生成器不得刷新调用方的供给树"
            ),
            Waterline::Budget(_) => panic!("应是供给"),
        }
    }

    #[test]
    fn budget_records_the_injected_instant_without_changing_the_ratio() {
        let keys = pair(budget(100, 0), budget(100, 0));
        let at_zero = render(&keys, 0).unwrap();
        let at_max = render(&keys, u64::MAX).unwrap();
        assert_eq!(at_zero.key_weight("primary"), Some(50));
        assert_eq!(at_max.key_weight("backup"), Some(50));
        assert_eq!(at_zero.generated_at, 0);
        assert_eq!(at_max.generated_at, u64::MAX);
        assert_eq!(at_zero.key_weight("primary"), at_max.key_weight("primary"));
    }

    #[test]
    fn header_pin_weight_is_not_the_key_percent() {
        let fragment = render(&pair(stock(0), stock(5)), 9).unwrap();
        let primary = fragment.pin_rule("primary").unwrap();
        assert_eq!(primary.targets.len(), 1);
        assert_eq!(
            primary.targets[0].weight.to_bits(),
            PIN_TARGET_WEIGHT.to_bits()
        );
        assert_eq!(primary.targets[0].key_id, "primary");
        assert_eq!(primary.targets[0].provider, "openai");
        assert_eq!(primary.priority, 0);
        assert!(primary.enabled);
        assert_eq!(primary.scope, "global");
        assert!(primary.fallbacks.is_empty());
        assert!(primary.cel_expression.contains(PIN_HEADER));
        assert!(primary.cel_expression.contains("primary"));
        assert!(primary.description.contains(PIN_HEADER));
        assert_ne!(u32::from(fragment.key_weight("backup").unwrap()), 1);

        let json = serde_json::to_value(&fragment).unwrap();
        let key_weight = &json["providers"]["openai"]["keys"][0]["weight"];
        let target_weight = &json["governance"]["routing_rules"][0]["targets"][0]["weight"];
        assert!(
            key_weight.is_u64(),
            "key weight 应是百分整，实际 {key_weight}"
        );
        assert!(
            target_weight.is_f64(),
            "路由目标 weight 应是命中概率，实际 {target_weight}"
        );
        let text = serde_json::to_string(&fragment).unwrap();
        assert!(text.contains("\"weight\":0"));
        assert!(text.contains("\"weight\":100"));
        assert!(text.contains("\"weight\":1.0"));
    }

    #[test]
    fn role_is_not_list_position() {
        let keys = vec![
            spec("backup", KeyRole::Backup, stock(1)),
            spec("primary", KeyRole::Primary, stock(0)),
        ];
        let fragment = render(&keys, 4).unwrap();
        assert_eq!(fragment.providers["openai"].keys[0].name, "backup");
        assert_eq!(fragment.providers["openai"].keys[0].weight, 100);
        assert_eq!(fragment.providers["openai"].keys[1].name, "primary");
        assert_eq!(fragment.providers["openai"].keys[1].weight, 0);
        assert_eq!(fragment.governance.routing_rules[0].id, "af-pin-backup");
    }

    #[test]
    fn shape_errors_precede_balance_checks() {
        assert_eq!(render(&[], 0).unwrap_err(), BifrostError::Empty);
        assert_eq!(
            render(&[spec("primary", KeyRole::Primary, stock(1))], 0).unwrap_err(),
            BifrostError::MissingBackup
        );
        assert_eq!(
            render(&[spec("backup", KeyRole::Backup, stock(1))], 0).unwrap_err(),
            BifrostError::MissingPrimary
        );

        let mut two_primary = pair(stock(1), stock(1));
        two_primary[1].role = KeyRole::Primary;
        two_primary[1].name = "other".to_string();
        two_primary[1].value = "env.AF_KEY_OTHER".to_string();
        assert_eq!(
            render(&two_primary, 0).unwrap_err(),
            BifrostError::MultiplePrimary
        );

        let mut dup = pair(stock(1), stock(1));
        dup[1].name = "primary".to_string();
        assert_eq!(
            render(&dup, 0).unwrap_err(),
            BifrostError::DuplicateName {
                name: "primary".to_string(),
            }
        );

        let mut bad_name = pair(stock(1), stock(1));
        bad_name[0].name = "a\"b".to_string();
        assert!(matches!(
            render(&bad_name, 0).unwrap_err(),
            BifrostError::BadName { .. }
        ));

        let mut raw = pair(stock(1), stock(1));
        raw[0].value = "sk-live".to_string();
        assert_eq!(
            render(&raw, 0).unwrap_err(),
            BifrostError::RawValue {
                name: "primary".to_string(),
            }
        );

        let mut mixed = pair(stock(1), stock(1));
        mixed[1].provider = "anthropic".to_string();
        assert_eq!(
            render(&mixed, 0).unwrap_err(),
            BifrostError::MixedProviders {
                first: "openai".to_string(),
                other: "anthropic".to_string(),
            }
        );
    }
}
