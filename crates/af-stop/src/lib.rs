//! af-stop — 失败指纹止损规则引擎（PoC）
//!
//! 纯事件流处理：调用方按序 [`StopEngine::feed`] 事件，引擎返回 [`Decision`]。
//! 所有阈值与策略通过 [`StopPolicy`] 参数注入；指纹库结构 [`FingerprintStore`]
//! 为预留接口：默认空库（全部走全局默认参数），可按指纹登记覆盖项。
//!
//! 三条规则（按优先级从高到低，同一事件命中多条时只返回最高一条）：
//! 1. **指纹连击**：连续 N 次、指纹（工具名+错误码）完全相同的工具错误
//!    → `Stop(NoInfluence)`。任何其他事件（成功、不同指纹、check、普通错误）
//!    都会打断"连续"。
//! 2. **确定性节点首败**：check 型节点第一次失败 → `Stop(CheckFirstFail)`。
//! 3. **普通错误阈值**：工具错误 + 普通错误累计达到 M → `Escalate`。
//!
//! 对外 JSON 带版本字段 `v`（当前为 1）。读到缺失或非 1 的版本会返回错误，不会静默降级。
//! 指纹账本把「指纹 → 计数 / 处置 / 时间窗」落在 SQLite 里。三条规则的裁决仍只由本引擎决定：
//! 时间窗到期不会把计数清掉，也不会改裁决。
//!
//! # 示例
//! ```
//! use af_stop::{Decision, Event, StopEngine, StopPolicy, StopReason};
//!
//! let mut eng = StopEngine::new(StopPolicy::default()); // N=3, M=5
//! let err = || Event::ToolError { tool: "search".into(), error_code: "E_TIMEOUT".into() };
//! assert_eq!(eng.feed(err()), Decision::Continue);
//! assert_eq!(eng.feed(err()), Decision::Continue);
//! assert_eq!(eng.feed(err()), Decision::Stop { reason: StopReason::NoInfluence });
//! ```

mod clock;
mod error;
mod ledger;
mod wire;

pub use clock::{Clock, ManualClock, SystemClock};
pub use error::{Error, Result};
pub use ledger::{FingerprintLedger, FingerprintRecord, LedgerDeclaration, OpenStatus};
pub use wire::{decode_doc, decode_wire, encode_doc, encode_wire, Versioned, WIRE_VERSION};

use serde::de::Deserializer;
use serde::{Deserialize, Serialize, Serializer};
use std::collections::HashMap;

/// 失败指纹：工具名 + 错误码。两者都相同才视为同一指纹。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fingerprint {
    pub tool: String,
    pub error_code: String,
}

impl Fingerprint {
    pub fn new(tool: impl Into<String>, error_code: impl Into<String>) -> Self {
        Fingerprint {
            tool: tool.into(),
            error_code: error_code.into(),
        }
    }
}

/// 输入事件流。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    /// 工具调用失败，携带工具名与错误码（喂给规则①与规则③）。
    ToolError { tool: String, error_code: String },
    /// 工具调用成功（打断规则①的"连续"序列）。
    ToolOk { tool: String },
    /// 确定性节点（check 型）失败（规则②：首败即停）。
    CheckFailed { node: String },
    /// 非工具类普通错误（只喂给规则③）。
    GenericError { source: String, error_code: String },
}

/// Stop 的原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    /// 规则①：同一指纹重复到无影响（重复失败已不产生新信息）。
    NoInfluence,
    /// 规则②：确定性 check 节点首败。
    CheckFirstFail,
}

/// Escalate 的原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EscalateReason {
    /// 规则③：普通错误累计达到阈值。
    NormalErrorThreshold,
    /// 指纹库把某指纹的触发动作登记为升级（预留路径）。
    FingerprintPolicy,
}

/// 引擎裁决。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    Continue,
    Stop { reason: StopReason },
    Escalate { reason: EscalateReason },
}

impl Decision {
    pub fn is_continue(&self) -> bool {
        matches!(self, Decision::Continue)
    }
}

/// 指纹库单条登记项：对该指纹覆盖连击阈值与触发动作。
/// `streak_limit = Some(0)` 表示白名单——该指纹永不触发规则①。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FingerprintEntry {
    pub streak_limit: Option<usize>,
    pub action: FingerprintAction,
}

impl Default for FingerprintEntry {
    fn default() -> Self {
        FingerprintEntry {
            streak_limit: None,
            action: FingerprintAction::StopNoInfluence,
        }
    }
}

/// 指纹命中后的动作（默认停机 NoInfluence；可登记为升级）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FingerprintAction {
    StopNoInfluence,
    Escalate,
}

/// 指纹库（结构预留）：`指纹 → 登记项`。默认为空库，行为与"无指纹库"完全一致。
///
/// 序列化成按工具名、错误码排序的列表，避免 `HashMap` 的顺序进入对外文档。
/// 重复指纹在反序列化时拒绝，不静默覆盖。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FingerprintStore {
    entries: HashMap<Fingerprint, FingerprintEntry>,
}

impl FingerprintStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, fp: Fingerprint, entry: FingerprintEntry) {
        self.entries.insert(fp, entry);
    }

    pub fn get(&self, fp: &Fingerprint) -> Option<&FingerprintEntry> {
        self.entries.get(fp)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoreRow {
    fingerprint: Fingerprint,
    entry: FingerprintEntry,
}

impl Serialize for FingerprintStore {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut rows: Vec<StoreRow> = self
            .entries
            .iter()
            .map(|(fingerprint, entry)| StoreRow {
                fingerprint: fingerprint.clone(),
                entry: entry.clone(),
            })
            .collect();
        rows.sort_by(|left, right| {
            (&left.fingerprint.tool, &left.fingerprint.error_code)
                .cmp(&(&right.fingerprint.tool, &right.fingerprint.error_code))
        });
        rows.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FingerprintStore {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let rows = Vec::<StoreRow>::deserialize(deserializer)?;
        let mut entries = HashMap::with_capacity(rows.len());
        for row in rows {
            if entries.insert(row.fingerprint.clone(), row.entry).is_some() {
                return Err(serde::de::Error::custom(format!(
                    "duplicate fingerprint {}/{}",
                    row.fingerprint.tool, row.fingerprint.error_code
                )));
            }
        }
        Ok(FingerprintStore { entries })
    }
}

/// 注入的止损参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopPolicy {
    /// 规则①全局默认：连续 N 次相同指纹 → Stop(NoInfluence)。
    pub fingerprint_streak_limit: usize,
    /// 规则③：普通错误累计达到 M → Escalate。
    pub normal_error_threshold: usize,
    /// 指纹库结构预留。
    pub store: FingerprintStore,
}

impl Default for StopPolicy {
    fn default() -> Self {
        StopPolicy {
            fingerprint_streak_limit: 3,
            normal_error_threshold: 5,
            store: FingerprintStore::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct EngineState {
    /// 当前"连续相同指纹错误"的序列：None 表示上一事件不是工具错误。
    streak: Option<(Fingerprint, usize)>,
    /// 规则③计数器（工具错误 + 普通错误）。
    normal_errors: usize,
}

/// 止损引擎：持有注入的策略与流式状态，逐事件裁决。
#[derive(Debug, Clone)]
pub struct StopEngine {
    policy: StopPolicy,
    state: EngineState,
}

impl StopEngine {
    pub fn new(policy: StopPolicy) -> Self {
        StopEngine {
            policy,
            state: EngineState {
                streak: None,
                normal_errors: 0,
            },
        }
    }

    /// 处理一个事件，返回裁决。
    ///
    /// 注意：引擎不记忆"已停机"状态——调用方收到 `Stop` 后应停止喂数据；
    /// 若继续喂，check 失败会再次返回 Stop，其余事件按规则继续裁决。
    pub fn feed(&mut self, event: Event) -> Decision {
        match event {
            // 规则②：确定性节点（check 型）首败即停，优先级最高。
            Event::CheckFailed { .. } => {
                self.state.streak = None;
                Decision::Stop {
                    reason: StopReason::CheckFirstFail,
                }
            }
            Event::ToolError { tool, error_code } => {
                let fp = Fingerprint { tool, error_code };
                // 更新连击序列：与上一指纹相同则累加，否则重置为该指纹 ×1。
                let streak_len = match &mut self.state.streak {
                    Some((cur, len)) if *cur == fp => {
                        *len += 1;
                        *len
                    }
                    _ => {
                        self.state.streak = Some((fp.clone(), 1));
                        1
                    }
                };
                self.state.normal_errors += 1;

                // 规则①：先查指纹库覆盖项，没有则用全局 N。N=0 视为白名单。
                let entry = self.policy.store.get(&fp);
                let limit = entry
                    .and_then(|e| e.streak_limit)
                    .unwrap_or(self.policy.fingerprint_streak_limit);
                if limit > 0 && streak_len >= limit {
                    return match entry.map(|e| e.action.clone()) {
                        Some(FingerprintAction::Escalate) => Decision::Escalate {
                            reason: EscalateReason::FingerprintPolicy,
                        },
                        _ => Decision::Stop {
                            reason: StopReason::NoInfluence,
                        },
                    };
                }

                // 规则③：普通错误累计阈值（与规则①同事件都命中时，①优先）。
                if self.state.normal_errors >= self.policy.normal_error_threshold {
                    return Decision::Escalate {
                        reason: EscalateReason::NormalErrorThreshold,
                    };
                }
                Decision::Continue
            }
            // 其余事件：打断"连续"，普通错误计入阈值。
            Event::ToolOk { .. } => {
                self.state.streak = None;
                Decision::Continue
            }
            Event::GenericError { .. } => {
                self.state.streak = None;
                self.state.normal_errors += 1;
                if self.state.normal_errors >= self.policy.normal_error_threshold {
                    return Decision::Escalate {
                        reason: EscalateReason::NormalErrorThreshold,
                    };
                }
                Decision::Continue
            }
        }
    }

    /// 只读观测：当前连击序列（指纹与长度）。
    pub fn current_streak(&self) -> Option<(&Fingerprint, usize)> {
        self.state.streak.as_ref().map(|(fp, len)| (fp, *len))
    }

    /// 只读观测：普通错误累计计数。
    pub fn normal_error_count(&self) -> usize {
        self.state.normal_errors
    }

    /// 导出可落盘的引擎状态。连击长度与普通错误计数原样带出，不在这里清零。
    pub fn snapshot(&self) -> EngineSnapshot {
        EngineSnapshot {
            v: WIRE_VERSION,
            streak: self
                .state
                .streak
                .as_ref()
                .map(|(fp, len)| (fp.clone(), u64::try_from(*len).unwrap_or(u64::MAX))),
            normal_errors: u64::try_from(self.state.normal_errors).unwrap_or(u64::MAX),
        }
    }

    /// 用一份快照恢复引擎。`v` 不是 1，或连击长度为 0，都拒绝。
    pub fn from_snapshot(policy: StopPolicy, snap: EngineSnapshot) -> Result<Self> {
        if snap.v != WIRE_VERSION {
            return Err(Error::UnknownVersion {
                found: snap.v,
                expected: WIRE_VERSION,
            });
        }
        let streak = match snap.streak {
            None => None,
            Some((fingerprint, len)) => {
                if len == 0 {
                    return Err(Error::Corrupt("连击长度为 0".into()));
                }
                let len = usize::try_from(len)
                    .map_err(|_| Error::Corrupt("连击长度超出 usize".into()))?;
                Some((fingerprint, len))
            }
        };
        let normal_errors = usize::try_from(snap.normal_errors)
            .map_err(|_| Error::Corrupt("普通错误计数超出 usize".into()))?;
        Ok(StopEngine {
            policy,
            state: EngineState {
                streak,
                normal_errors,
            },
        })
    }
}

/// 引擎流式状态的对外快照。`v` 必须为 1。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineSnapshot {
    #[serde(deserialize_with = "wire::require_v1")]
    pub v: u32,
    pub streak: Option<(Fingerprint, u64)>,
    pub normal_errors: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_err(tool: &str, code: &str) -> Event {
        Event::ToolError {
            tool: tool.into(),
            error_code: code.into(),
        }
    }

    // ---- 规则①：连续 N 次相同指纹 → Stop(NoInfluence) ----

    #[test]
    fn rule1_fingerprint_streak_stops_at_nth() {
        let mut eng = StopEngine::new(StopPolicy::default()); // N=3
        assert_eq!(
            eng.feed(tool_err("search", "E_TIMEOUT")),
            Decision::Continue
        );
        assert_eq!(
            eng.feed(tool_err("search", "E_TIMEOUT")),
            Decision::Continue
        );
        assert_eq!(
            eng.feed(tool_err("search", "E_TIMEOUT")),
            Decision::Stop {
                reason: StopReason::NoInfluence
            }
        );
        assert_eq!(
            eng.current_streak(),
            Some((&Fingerprint::new("search", "E_TIMEOUT"), 3))
        );
    }

    #[test]
    fn rule1_different_error_code_is_different_fingerprint() {
        // 调高 M，隔离规则③，只看指纹连击
        let mut eng = StopEngine::new(StopPolicy {
            normal_error_threshold: 100,
            ..Default::default()
        });
        // 3 组"2 连 + 换指纹打断"：E_TIMEOUT 最长连击只有 2 < 3，永不 Stop
        for _ in 0..3 {
            assert_eq!(
                eng.feed(tool_err("search", "E_TIMEOUT")),
                Decision::Continue
            );
            assert_eq!(
                eng.feed(tool_err("search", "E_TIMEOUT")),
                Decision::Continue
            );
            // 同工具不同错误码：指纹不同，打断连击
            assert_eq!(
                eng.feed(tool_err("search", "E_RATE_LIMIT")),
                Decision::Continue
            );
        }
    }

    #[test]
    fn rule1_success_breaks_the_streak() {
        let mut eng = StopEngine::new(StopPolicy {
            normal_error_threshold: 100,
            ..Default::default()
        });
        assert_eq!(
            eng.feed(tool_err("search", "E_TIMEOUT")),
            Decision::Continue
        );
        assert_eq!(
            eng.feed(tool_err("search", "E_TIMEOUT")),
            Decision::Continue
        );
        assert_eq!(
            eng.feed(Event::ToolOk {
                tool: "search".into()
            }),
            Decision::Continue
        );
        // 成功之后从头重新数：2 次仍 Continue，第 3 次（连续）才 Stop
        assert_eq!(
            eng.feed(tool_err("search", "E_TIMEOUT")),
            Decision::Continue
        );
        assert_eq!(
            eng.feed(tool_err("search", "E_TIMEOUT")),
            Decision::Continue
        );
        assert_eq!(
            eng.feed(tool_err("search", "E_TIMEOUT")),
            Decision::Stop {
                reason: StopReason::NoInfluence
            }
        );
    }

    #[test]
    fn rule1_alternating_tools_never_triggers() {
        // 不误触：不同工具交替错误，指纹永远不连续
        let mut eng = StopEngine::new(StopPolicy {
            normal_error_threshold: 100, // 调高 M，隔离规则③
            ..StopPolicy::default()
        });
        for i in 0..20 {
            let tool = if i % 2 == 0 { "search" } else { "browser" };
            assert_eq!(
                eng.feed(tool_err(tool, "E_TIMEOUT")),
                Decision::Continue,
                "i={i}"
            );
        }
    }

    // ---- 规则②：确定性节点（check 型）首败即停 ----

    #[test]
    fn rule2_check_first_failure_stops_immediately() {
        let mut eng = StopEngine::new(StopPolicy::default());
        assert_eq!(
            eng.feed(Event::CheckFailed {
                node: "schema_check".into()
            }),
            Decision::Stop {
                reason: StopReason::CheckFirstFail
            }
        );
    }

    // ---- 规则③：普通错误达到阈值 → Escalate ----

    #[test]
    fn rule3_generic_errors_escalate_at_threshold() {
        let mut eng = StopEngine::new(StopPolicy {
            normal_error_threshold: 3,
            ..Default::default()
        });
        assert_eq!(
            eng.feed(Event::GenericError {
                source: "llm".into(),
                error_code: "E_5XX".into()
            }),
            Decision::Continue
        );
        assert_eq!(
            eng.feed(Event::GenericError {
                source: "llm".into(),
                error_code: "E_TIMEOUT".into()
            }),
            Decision::Continue
        );
        // 不同 source/code 互不打断彼此（规则③只数总量），第 3 个到达阈值
        assert_eq!(
            eng.feed(Event::GenericError {
                source: "llm".into(),
                error_code: "E_5XX".into()
            }),
            Decision::Escalate {
                reason: EscalateReason::NormalErrorThreshold
            }
        );
        // 之后继续错误继续升级
        assert_eq!(
            eng.feed(Event::GenericError {
                source: "llm".into(),
                error_code: "E_5XX".into()
            }),
            Decision::Escalate {
                reason: EscalateReason::NormalErrorThreshold
            }
        );
    }

    #[test]
    fn rule3_tool_errors_also_count_toward_threshold() {
        let mut eng = StopEngine::new(StopPolicy {
            normal_error_threshold: 3,
            ..Default::default()
        });
        assert_eq!(eng.feed(tool_err("a", "X")), Decision::Continue); // 计数 1
        assert_eq!(eng.feed(tool_err("b", "Y")), Decision::Continue); // 计数 2（指纹不同，不触发①）
        assert_eq!(eng.normal_error_count(), 2);
        // 第 3 个错误到达阈值 → Escalate（工具错误同样计入规则③）
        assert_eq!(
            eng.feed(tool_err("a", "X")),
            Decision::Escalate {
                reason: EscalateReason::NormalErrorThreshold
            }
        );
    }

    // ---- 优先级：同一事件同时命中，更具体的规则胜出 ----

    #[test]
    fn fingerprint_stop_wins_over_threshold_escalate() {
        // N=3、M=3：第 3 个相同指纹错误同时满足①③，应返回 Stop 而非 Escalate
        let mut eng = StopEngine::new(StopPolicy {
            normal_error_threshold: 3,
            ..Default::default()
        });
        assert_eq!(eng.feed(tool_err("db", "E_LOCK")), Decision::Continue);
        assert_eq!(eng.feed(tool_err("db", "E_LOCK")), Decision::Continue);
        assert_eq!(
            eng.feed(tool_err("db", "E_LOCK")),
            Decision::Stop {
                reason: StopReason::NoInfluence
            }
        );
    }

    // ---- 指纹库结构预留：按指纹覆盖参数 ----

    #[test]
    fn store_override_whitelisted_fingerprint_never_stops() {
        // 调高 M，隔离规则③
        let mut policy = StopPolicy {
            normal_error_threshold: 100,
            ..Default::default()
        };
        policy.store.register(
            Fingerprint::new("deploy", "E_LOCKED"),
            FingerprintEntry {
                streak_limit: Some(0),
                ..Default::default()
            },
        );
        let mut eng = StopEngine::new(policy);
        for _ in 0..10 {
            assert_eq!(eng.feed(tool_err("deploy", "E_LOCKED")), Decision::Continue);
        }
    }

    #[test]
    fn store_override_lower_limit_and_escalate_action() {
        let mut policy = StopPolicy::default();
        policy.store.register(
            Fingerprint::new("deploy", "E_LOCKED"),
            FingerprintEntry {
                streak_limit: Some(1),
                action: FingerprintAction::Escalate,
            },
        );
        let mut eng = StopEngine::new(policy);
        // 首个该指纹错误即触发（覆盖 N=1），且动作为升级
        assert_eq!(
            eng.feed(tool_err("deploy", "E_LOCKED")),
            Decision::Escalate {
                reason: EscalateReason::FingerprintPolicy
            }
        );
        // 其他指纹不受影响，仍走全局 N=3
        assert_eq!(eng.feed(tool_err("other", "E_X")), Decision::Continue);
        assert_eq!(eng.feed(tool_err("other", "E_X")), Decision::Continue);
        assert_eq!(
            eng.feed(tool_err("other", "E_X")),
            Decision::Stop {
                reason: StopReason::NoInfluence
            }
        );
    }

    // ---- 综合场景 ----

    #[test]
    fn mixed_stream_end_to_end() {
        // 交替错误(不触发①) → 累计到 M-1 → check 首败直接 Stop(②优先级最高)
        let mut eng = StopEngine::new(StopPolicy {
            normal_error_threshold: 4,
            ..Default::default()
        });
        assert_eq!(
            eng.feed(tool_err("search", "E_TIMEOUT")),
            Decision::Continue
        );
        assert_eq!(eng.feed(tool_err("browser", "E_NAV")), Decision::Continue);
        assert_eq!(
            eng.feed(Event::ToolOk {
                tool: "search".into()
            }),
            Decision::Continue
        );
        assert_eq!(
            eng.feed(tool_err("search", "E_TIMEOUT")),
            Decision::Continue
        );
        // 已累计 3 个错误 < M=4，此时 check 首败 → Stop（而不是等下一次错误 Escalate）
        assert_eq!(
            eng.feed(Event::CheckFailed {
                node: "format_check".into()
            }),
            Decision::Stop {
                reason: StopReason::CheckFirstFail
            }
        );
    }

    #[test]
    fn fingerprint_store_wire_is_sorted_and_rejects_duplicates() {
        let mut first = FingerprintStore::new();
        first.register(
            Fingerprint::new("b", "2"),
            FingerprintEntry {
                streak_limit: Some(1),
                action: FingerprintAction::Escalate,
            },
        );
        first.register(Fingerprint::new("a", "1"), FingerprintEntry::default());
        let mut second = FingerprintStore::new();
        second.register(Fingerprint::new("a", "1"), FingerprintEntry::default());
        second.register(
            Fingerprint::new("b", "2"),
            FingerprintEntry {
                streak_limit: Some(1),
                action: FingerprintAction::Escalate,
            },
        );
        let text_a = encode_wire(&first).unwrap();
        let text_b = encode_wire(&second).unwrap();
        assert_eq!(text_a, text_b);
        let back: FingerprintStore = decode_wire(&text_a).unwrap();
        assert_eq!(back, first);

        let duplicated = r#"[
            {"fingerprint":{"tool":"a","error_code":"1"},"entry":{"streak_limit":null,"action":"StopNoInfluence"}},
            {"fingerprint":{"tool":"a","error_code":"1"},"entry":{"streak_limit":null,"action":"Escalate"}}
        ]"#;
        let err = serde_json::from_str::<FingerprintStore>(duplicated).unwrap_err();
        assert!(err.to_string().contains("duplicate"), "{err}");
    }

    #[test]
    fn snapshot_roundtrip_continues_decisions_and_rejects_unknown_version() {
        let policy = StopPolicy {
            normal_error_threshold: 100,
            ..StopPolicy::default()
        };
        let mut live = StopEngine::new(policy.clone());
        assert!(live.feed(tool_err("search", "E_TIMEOUT")).is_continue());
        assert!(live.feed(tool_err("search", "E_TIMEOUT")).is_continue());
        let snap = live.snapshot();
        let text = encode_doc(&snap).unwrap();
        let restored_snap: EngineSnapshot = decode_doc(&text).unwrap();
        assert_eq!(restored_snap, snap);
        let mut restored = StopEngine::from_snapshot(policy.clone(), restored_snap).unwrap();
        let next = tool_err("search", "E_TIMEOUT");
        assert_eq!(restored.feed(next.clone()), live.feed(next));
        assert_eq!(restored.normal_error_count(), 3);
        assert_eq!(restored.current_streak().map(|(_, len)| len), Some(3));

        let err = StopEngine::from_snapshot(
            policy,
            EngineSnapshot {
                v: 2,
                streak: None,
                normal_errors: 0,
            },
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                Error::UnknownVersion {
                    found: 2,
                    expected: 1
                }
            ),
            "{err}"
        );
        let bad_json =
            decode_doc::<EngineSnapshot>(r#"{"v":2,"streak":null,"normal_errors":0}"#).unwrap_err();
        assert!(
            matches!(
                bad_json,
                Error::UnknownVersion {
                    found: 2,
                    expected: 1
                }
            ),
            "{bad_json}"
        );
    }
}
