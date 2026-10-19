//! 投放端 WIP 闸门(CONWIP 语义)。
//!
//! 核心模型:
//! - 队列按"标准审阅分钟"计量:每件(工单)带工时权重 `review_minutes`,
//!   WIP = 队列中所有已准入、未完成件的权重按队列顺序的左折叠。
//!   不保留增量累加器:分数权重上 `+=` / `-=` 与重求和会差 1 ULP。
//! - 上限 `wip_upper_minutes`:`try_admit` 在 `wip + weight > upper` 时拒绝(WipCap);
//!   `admit_blocking` 则阻塞等待,直到有件完成释放容量。
//! - 下限 `wip_lower_minutes`:WIP 跌破下限时触发 NeedRefill 事件(断粮预警),
//!   回到下限之上时触发 RefillSatisfied。下限不强制准入,只发信号。
//! - 夜间窗口:给定时间区间内按配置比例(ratio)确定性放行——信用累积制,
//!   每个请求 +ratio,信用 ≥1 时放行并扣 1,长期放行率 = ratio。
//! - 首件渐放(canary):新配置上线 → gated 状态;评估谓词通过前,该配置
//!   最多放行 N 件探针;谓词通过 → 全量放行(CanaryPromoted)。
//!
//! 策略评估顺序(固定):canary → 夜间窗口 → WIP 上限。
//! 工单入口 [`ReleaseGate::admit`] 在这三步之前先走工单自己的 release 门控，
//! 三步本身的顺序不改。

use std::cmp::Ordering;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use af_workorder::{GateError as OrderGateError, WorkOrder};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 一天内的第几分钟(0..=1439)。
pub type MinuteOfDay = u32;

/// 一件待审工单。`review_minutes` 是以"标准审阅分钟"计的工时权重。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
    pub config: String,
    pub review_minutes: f64,
}

/// 闸门参数(全部注入;可经 serde 从 JSON 反序列化)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateConfig {
    /// WIP 上限(标准审阅分钟)。
    pub wip_upper_minutes: f64,
    /// WIP 下限(断粮预警线,标准审阅分钟)。
    pub wip_lower_minutes: f64,
    /// 夜间窗口(可选)。
    #[serde(default)]
    pub night_window: Option<NightWindow>,
}

/// 夜间窗口:[start, end) 为一天内分钟;start > end 表示跨午夜(如 22:00→06:00)。
/// 窗口内只放行请求量乘 `ratio` 的比例(确定性信用制)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NightWindow {
    pub start: MinuteOfDay,
    pub end: MinuteOfDay,
    /// 放行比例,∈ (0, 1]。
    pub ratio: f64,
}

impl NightWindow {
    /// start == end 视为空窗口。
    pub fn contains(&self, now: MinuteOfDay) -> bool {
        if self.start == self.end {
            return false;
        }
        if self.start < self.end {
            now >= self.start && now < self.end
        } else {
            now >= self.start || now < self.end
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum AdmitError {
    /// WIP 上限:该件放不下了。
    #[error("WIP cap hit: wip={wip:.1} + weight={weight:.1} > upper={cap:.1}")]
    WipCap { wip: f64, weight: f64, cap: f64 },
    /// 夜间窗口按比例限流,本次信用不足。
    #[error("night window throttled: ratio={ratio}")]
    NightThrottled { ratio: f64 },
    /// 渐放(gated)阶段探针件数已满。
    #[error("canary probe cap reached for config {config:?}: cap={cap}")]
    CanaryProbeCap { config: String, cap: usize },
    /// 仅由阻塞式 admit 产生:等待超时。
    #[error("blocking admit timed out")]
    TimedOut,
}

/// 渐放相位。挂在 [`Permit`] 上，供 trace 记下本单是探针、已全量，还是不在渐放里。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RampPhase {
    /// 没有针对该图谱版本的渐放，按普通 CONWIP 放行。
    Ungated,
    /// 仍处于 gated。`index` 是本单探针序号（从 1 起），等于准入后的 released。
    Probe { index: usize, max_probes: usize },
    /// 谓词已通过，该版本全量放行。`probes_released` 停在晋升前的探针数。
    Open { probes_released: usize },
}

/// 投放闸放行凭证。
#[derive(Debug, Clone)]
pub struct Permit {
    pub order_id: String,
    /// 在制票据。`ticket.config` 是工单 `graph_version`。
    /// 释容用 `ticket.item_id` 调 [`ReleaseGate::complete`]，不要用工单自己的 `complete`。
    pub ticket: Ticket,
    pub ramp: RampPhase,
}

/// 订单 release 没能过闸。调用方工单保持进入 `admit` 之前的状态。
#[derive(Debug, Error, Clone, PartialEq)]
pub enum Reject {
    /// 状态机边或 release 门控没过。此时投放闸的在制、信用、渐放游标都不变。
    #[error("工单 release 被拒绝: {0}")]
    Order(OrderGateError),
    /// CONWIP：`wip + weight > upper`。精确等号不走这条。
    #[error("CONWIP 容量已满: wip={wip:.1} + weight={weight:.1} > upper={cap:.1}")]
    AtCapacity { wip: f64, weight: f64, cap: f64 },
    /// 该图谱版本的渐放仍是 gated，探针名额已用完，还没晋升。
    #[error("渐放相位未打开: config={config:?} released={released} cap={cap}")]
    RampPhaseNotOpen {
        config: String,
        released: usize,
        cap: usize,
    },
    /// 夜间窗口信用不足。规则与 [`AdmitError::NightThrottled`] 相同。
    #[error("夜间窗口限流: ratio={ratio}")]
    NightThrottled { ratio: f64 },
    /// 权重不是有限正数，请求本身不成立，不进闸。
    #[error("审阅分钟权重非法: {weight}")]
    BadWeight { weight: f64 },
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum ConfigError {
    #[error("wip_lower ({lower}) must be < wip_upper ({upper})")]
    LowerNotBelowUpper { lower: f64, upper: f64 },
    #[error("night ratio must be in (0, 1], got {0}")]
    BadNightRatio(f64),
    #[error("night window minute must be in [0, 1440), got {0}")]
    BadNightMinute(u32),
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum GateError {
    #[error("unknown item id {0} in queue")]
    UnknownItem(u64),
    #[error("canary already active for config {0:?}")]
    CanaryAlreadyActive(String),
}

/// 准入凭证。
#[derive(Debug, Clone)]
pub struct Ticket {
    pub item_id: u64,
    pub config: String,
    pub weight: f64,
    pub admitted_at: MinuteOfDay,
}

/// 完成一件时返回的信息。
#[derive(Debug, Clone)]
pub struct CompletedItem {
    pub ticket: Ticket,
    pub completed_at: MinuteOfDay,
}

/// 渐放评估报告:谓词以此为输入。
#[derive(Debug, Clone, PartialEq)]
pub struct EvalReport {
    pub config: String,
    pub probes_released: usize,
    pub probes_completed: usize,
    pub defects: u64,
}

pub type CanaryPredicate = Box<dyn Fn(&EvalReport) -> bool + Send>;

/// 闸门事件(供模拟/上游消费)。
#[derive(Debug, Clone, PartialEq)]
pub enum GateEvent {
    NeedRefill {
        at: MinuteOfDay,
        wip: f64,
        lower: f64,
    },
    RefillSatisfied {
        at: MinuteOfDay,
        wip: f64,
    },
    NightWindowEnter {
        at: MinuteOfDay,
    },
    NightWindowExit {
        at: MinuteOfDay,
    },
    CanaryPromoted {
        config: String,
        probes_released: usize,
        at: MinuteOfDay,
    },
}

struct QueuedItem {
    ticket: Ticket,
}

struct NightRuntime {
    credit: f64,
    requests: u64,
    admitted: u64,
}

struct CanaryState {
    config: String,
    max_probes: usize,
    predicate: CanaryPredicate,
    released: usize,
    completed: usize,
    defects: u64,
    open: bool,
}

/// 渐放状态快照(供观测)。
#[derive(Debug, Clone, PartialEq)]
pub struct CanaryStatus {
    pub config: String,
    pub open: bool,
    pub released: usize,
    pub max_probes: usize,
}

/// 渐放游标。晋升与封顶依赖 `completed` / `defects`，重启时和 `released` 一起原样带回。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanaryCursor {
    pub config: String,
    pub open: bool,
    pub released: usize,
    pub max_probes: usize,
    pub completed: usize,
    pub defects: u64,
}

/// 一件已准入、未完成的在制。顺序与准入队列一致。
#[derive(Debug, Clone, PartialEq)]
pub struct InProcessItem {
    pub id: u64,
    pub config: String,
    pub weight: f64,
    pub admitted_at: MinuteOfDay,
}

/// 夜间信用与上一次窗口观察。`prev_in_window == None` 表示还没有调用过准入。
#[derive(Debug, Clone, PartialEq)]
pub struct NightView {
    pub credit: f64,
    pub requests: u64,
    pub admitted: u64,
    pub prev_in_window: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RuntimeSnapshot {
    pub wip: f64,
    pub items: Vec<SnapshotItem>,
    pub night_credit: f64,
    pub night_requests: u64,
    pub night_admitted: u64,
    pub prev_in_window: Option<bool>,
    pub refill_active: bool,
    pub canary: Option<CanarySnapshot>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SnapshotItem {
    pub id: u64,
    pub config: String,
    pub weight: f64,
    pub admitted_at: MinuteOfDay,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CanarySnapshot {
    pub config: String,
    pub max_probes: usize,
    pub released: usize,
    pub completed: usize,
    pub defects: u64,
    pub open: bool,
}

pub(crate) enum RestoreError {
    PredicateMismatch,
    InconsistentWip,
}

/// 权重按给定顺序左折叠。在制队列是 WIP 的唯一真相，调用方与账本都用这个和。
///
/// 空序列得到 `+0.0`。`Iterator::sum` 对 `f64` 从 `-0.0` 加起，空和的位型与 `0.0` 不同。
pub(crate) fn sum_weights(weights: impl IntoIterator<Item = f64>) -> f64 {
    weights.into_iter().fold(0.0, |acc, weight| acc + weight)
}

pub struct ReleaseGate {
    cfg: GateConfig,
    queue: VecDeque<QueuedItem>,
    night: NightRuntime,
    canary: Option<CanaryState>,
    refill_active: bool,
    prev_in_window: Option<bool>,
    events: Vec<GateEvent>,
}

impl ReleaseGate {
    pub fn new(cfg: GateConfig) -> Result<Self, ConfigError> {
        // NaN 与 lower >= upper 都拒绝。`!(a < b)` 对 NaN 为真，partial_cmp 保持同一判定。
        if cfg.wip_lower_minutes.partial_cmp(&cfg.wip_upper_minutes) != Some(Ordering::Less) {
            return Err(ConfigError::LowerNotBelowUpper {
                lower: cfg.wip_lower_minutes,
                upper: cfg.wip_upper_minutes,
            });
        }
        if let Some(w) = &cfg.night_window {
            if !(w.ratio > 0.0 && w.ratio <= 1.0) {
                return Err(ConfigError::BadNightRatio(w.ratio));
            }
            for m in [w.start, w.end] {
                if m >= 1440 {
                    return Err(ConfigError::BadNightMinute(m));
                }
            }
        }
        Ok(Self {
            night: NightRuntime {
                credit: 1.0,
                requests: 0,
                admitted: 0,
            },
            cfg,
            queue: VecDeque::new(),
            canary: None,
            refill_active: false,
            prev_in_window: None,
            events: Vec::new(),
        })
    }

    pub fn config(&self) -> &GateConfig {
        &self.cfg
    }

    pub fn wip_minutes(&self) -> f64 {
        sum_weights(self.queue.iter().map(|queued| queued.ticket.weight))
    }

    pub fn in_process_count(&self) -> usize {
        self.queue.len()
    }

    /// WIP 是否已跌破下限(断粮预警)。
    pub fn refill_needed(&self) -> bool {
        self.wip_minutes() < self.cfg.wip_lower_minutes
    }

    /// `now` 是否处于夜间窗口(纯查询,不改状态)。
    pub fn in_night_window(&self, now: MinuteOfDay) -> bool {
        self.cfg
            .night_window
            .as_ref()
            .is_some_and(|w| w.contains(now))
    }

    pub fn canary_status(&self) -> Option<CanaryStatus> {
        self.canary.as_ref().map(|c| CanaryStatus {
            config: c.config.clone(),
            open: c.open,
            released: c.released,
            max_probes: c.max_probes,
        })
    }

    /// 渐放游标。没有开启过渐放时为 `None`。
    pub fn canary_cursor(&self) -> Option<CanaryCursor> {
        self.canary.as_ref().map(|c| CanaryCursor {
            config: c.config.clone(),
            open: c.open,
            released: c.released,
            max_probes: c.max_probes,
            completed: c.completed,
            defects: c.defects,
        })
    }

    /// 当前在制队列，队首在前。
    pub fn in_process(&self) -> Vec<InProcessItem> {
        self.queue
            .iter()
            .map(|queued| InProcessItem {
                id: queued.ticket.item_id,
                config: queued.ticket.config.clone(),
                weight: queued.ticket.weight,
                admitted_at: queued.ticket.admitted_at,
            })
            .collect()
    }

    /// 夜间信用与窗口观察。判定仍走 [`Self::try_admit`] 里的既有顺序。
    pub fn night_view(&self) -> NightView {
        NightView {
            credit: self.night.credit,
            requests: self.night.requests,
            admitted: self.night.admitted,
            prev_in_window: self.prev_in_window,
        }
    }

    pub fn drain_events(&mut self) -> Vec<GateEvent> {
        std::mem::take(&mut self.events)
    }

    /// 开启新配置的渐放:进入 gated 状态,谓词通过前该配置最多放 `max_probes` 件。
    pub fn begin_canary(
        &mut self,
        config: &str,
        max_probes: usize,
        predicate: CanaryPredicate,
    ) -> Result<(), GateError> {
        if let Some(c) = &self.canary {
            if !c.open {
                return Err(GateError::CanaryAlreadyActive(c.config.clone()));
            }
        }
        self.canary = Some(CanaryState {
            config: config.to_string(),
            max_probes,
            predicate,
            released: 0,
            completed: 0,
            defects: 0,
            open: false,
        });
        Ok(())
    }

    /// 直接注入在制(换日结转/热迁移等场景),不走准入策略;不衡上限(调用方保证)。
    pub fn seed_wip(&mut self, item: &Item) {
        self.queue.push_back(QueuedItem {
            ticket: Ticket {
                item_id: item.id,
                config: item.config.clone(),
                weight: item.review_minutes,
                admitted_at: 0,
            },
        });
    }

    fn sync_night_window(&mut self, now: MinuteOfDay) {
        let in_window = self.in_night_window(now);
        match self.prev_in_window {
            None => {
                // 首次观察:静默记录,不产生事件(计数器在构造时已初始化)。
                self.prev_in_window = Some(in_window);
            }
            Some(prev) if prev != in_window => {
                self.prev_in_window = Some(in_window);
                // 进窗口:信用重置(初始信用 1.0,保证窗口内第一个请求不被误杀);
                // 出窗口:计数清零,保证每个窗口的占比统计独立。
                self.night = NightRuntime {
                    credit: 1.0,
                    requests: 0,
                    admitted: 0,
                };
                let ev = if in_window {
                    GateEvent::NightWindowEnter { at: now }
                } else {
                    GateEvent::NightWindowExit { at: now }
                };
                self.events.push(ev);
            }
            _ => {}
        }
    }

    /// 尝试准入(非阻塞)。受三道策略约束,顺序:canary → 夜间窗口 → WIP 上限。
    pub fn try_admit(&mut self, item: &Item, now: MinuteOfDay) -> Result<Ticket, AdmitError> {
        self.sync_night_window(now);

        // 1) 渐放:gated 阶段该配置探针件数已满则拒绝。
        let probe_full = self
            .canary
            .as_ref()
            .is_some_and(|c| !c.open && c.config == item.config && c.released >= c.max_probes);
        if probe_full {
            let c = self.canary.as_ref().unwrap();
            return Err(AdmitError::CanaryProbeCap {
                config: c.config.clone(),
                cap: c.max_probes,
            });
        }

        // 2) 夜间窗口:确定性比例放行(信用制)。出窗计数重置见 sync_night_window。
        if self.in_night_window(now) {
            let w = self.cfg.night_window.as_ref().unwrap();
            self.night.requests += 1;
            self.night.credit += w.ratio;
            if self.night.credit >= 1.0 {
                self.night.credit -= 1.0;
                self.night.admitted += 1;
            } else {
                return Err(AdmitError::NightThrottled { ratio: w.ratio });
            }
        }

        // 3) WIP 上限:放入后不得超过。比较用的是当前队列的左折叠，入队后的和就是再加这一件。
        let wip = self.wip_minutes();
        if wip + item.review_minutes > self.cfg.wip_upper_minutes + 1e-9 {
            return Err(AdmitError::WipCap {
                wip,
                weight: item.review_minutes,
                cap: self.cfg.wip_upper_minutes,
            });
        }

        // 通过 → 准入。WIP 随队列重算，不在这里累加。
        let ticket = Ticket {
            item_id: item.id,
            config: item.config.clone(),
            weight: item.review_minutes,
            admitted_at: now,
        };
        self.queue.push_back(QueuedItem {
            ticket: ticket.clone(),
        });
        if let Some(c) = &mut self.canary {
            if !c.open && c.config == item.config {
                c.released += 1;
            }
        }
        self.check_refill_cleared(now);
        Ok(ticket)
    }

    /// 完成一件:释放容量;若属渐放配置,累积评估报告并跑谓词,通过则全量放行。
    pub fn complete(
        &mut self,
        item_id: u64,
        now: MinuteOfDay,
        defects: u64,
    ) -> Result<CompletedItem, GateError> {
        let idx = self.queue.iter().position(|q| q.ticket.item_id == item_id);
        let Some(idx) = idx else {
            return Err(GateError::UnknownItem(item_id));
        };
        let q = self.queue.remove(idx).expect("idx validated above");
        let completed = CompletedItem {
            ticket: q.ticket,
            completed_at: now,
        };

        let mut promoted = None;
        if let Some(c) = &mut self.canary {
            if !c.open && c.config == completed.ticket.config {
                c.completed += 1;
                c.defects += defects;
                let report = EvalReport {
                    config: c.config.clone(),
                    probes_released: c.released,
                    probes_completed: c.completed,
                    defects: c.defects,
                };
                if (c.predicate)(&report) {
                    c.open = true;
                    promoted = Some(c.config.clone());
                }
            }
        }
        if let Some(config) = promoted {
            let released = self.canary.as_ref().map_or(0, |c| c.released);
            self.events.push(GateEvent::CanaryPromoted {
                config,
                probes_released: released,
                at: now,
            });
        }
        self.check_refill_needed(now);
        Ok(completed)
    }

    /// WIP 跌破下限 → 边沿触发 NeedRefill。
    fn check_refill_needed(&mut self, now: MinuteOfDay) {
        let wip = self.wip_minutes();
        if !self.refill_active && wip < self.cfg.wip_lower_minutes {
            self.refill_active = true;
            self.events.push(GateEvent::NeedRefill {
                at: now,
                wip,
                lower: self.cfg.wip_lower_minutes,
            });
        }
    }

    /// WIP 回到下限之上 → RefillSatisfied。
    fn check_refill_cleared(&mut self, now: MinuteOfDay) {
        let wip = self.wip_minutes();
        if self.refill_active && wip >= self.cfg.wip_lower_minutes {
            self.refill_active = false;
            self.events
                .push(GateEvent::RefillSatisfied { at: now, wip });
        }
    }

    /// 夜间窗口计数快照(当前窗口内的请求/放行数,用于占比验证)。
    pub fn night_stats(&self) -> (u64, u64) {
        (self.night.requests, self.night.admitted)
    }

    pub(crate) fn runtime_snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            wip: self.wip_minutes(),
            items: self
                .queue
                .iter()
                .map(|queued| SnapshotItem {
                    id: queued.ticket.item_id,
                    config: queued.ticket.config.clone(),
                    weight: queued.ticket.weight,
                    admitted_at: queued.ticket.admitted_at,
                })
                .collect(),
            night_credit: self.night.credit,
            night_requests: self.night.requests,
            night_admitted: self.night.admitted,
            prev_in_window: self.prev_in_window,
            refill_active: self.refill_active,
            canary: self.canary.as_ref().map(|canary| CanarySnapshot {
                config: canary.config.clone(),
                max_probes: canary.max_probes,
                released: canary.released,
                completed: canary.completed,
                defects: canary.defects,
                open: canary.open,
            }),
        }
    }

    /// 用账本里的运行态覆盖空闸。不走准入策略，事件缓冲清空。
    pub(crate) fn restore_runtime(
        &mut self,
        snap: RuntimeSnapshot,
        predicate: Option<CanaryPredicate>,
    ) -> Result<(), RestoreError> {
        if snap.canary.is_some() != predicate.is_some() {
            return Err(RestoreError::PredicateMismatch);
        }
        let sum = sum_weights(snap.items.iter().map(|item| item.weight));
        if sum.to_bits() != snap.wip.to_bits() {
            return Err(RestoreError::InconsistentWip);
        }
        self.queue.clear();
        for item in snap.items {
            self.queue.push_back(QueuedItem {
                ticket: Ticket {
                    item_id: item.id,
                    config: item.config,
                    weight: item.weight,
                    admitted_at: item.admitted_at,
                },
            });
        }
        self.night = NightRuntime {
            credit: snap.night_credit,
            requests: snap.night_requests,
            admitted: snap.night_admitted,
        };
        self.prev_in_window = snap.prev_in_window;
        self.refill_active = snap.refill_active;
        self.events.clear();
        self.canary = match (snap.canary, predicate) {
            (None, None) => None,
            (Some(canary), Some(predicate)) => Some(CanaryState {
                config: canary.config,
                max_probes: canary.max_probes,
                predicate,
                released: canary.released,
                completed: canary.completed,
                defects: canary.defects,
                open: canary.open,
            }),
            _ => return Err(RestoreError::PredicateMismatch),
        };
        Ok(())
    }
}

/// 线程安全包装:阻塞式 admit(在上限时等待,直到有件完成释放容量)。
/// 共享 `ReleaseGate`,完成时 notify_all 唤醒等待者。
#[derive(Clone)]
pub struct SharedGate {
    inner: Arc<(Mutex<ReleaseGate>, Condvar)>,
}

impl SharedGate {
    pub fn new(gate: ReleaseGate) -> Self {
        Self {
            inner: Arc::new((Mutex::new(gate), Condvar::new())),
        }
    }

    pub fn try_admit(&self, item: &Item, now: MinuteOfDay) -> Result<Ticket, AdmitError> {
        let (lock, _) = &*self.inner;
        lock.lock()
            .expect("gate mutex poisoned")
            .try_admit(item, now)
    }

    /// 阻塞式准入:WipCap 时等待,其他错误立即返回。
    pub fn admit_blocking(
        &self,
        item: &Item,
        now: MinuteOfDay,
        timeout: Duration,
    ) -> Result<Ticket, AdmitError> {
        let deadline = Instant::now() + timeout;
        let (lock, cvar) = &*self.inner;
        let mut g = lock.lock().expect("gate mutex poisoned");
        loop {
            match g.try_admit(item, now) {
                Ok(t) => return Ok(t),
                Err(AdmitError::WipCap { .. }) => {} // 继续等
                Err(e) => return Err(e),
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(AdmitError::TimedOut);
            };
            let (g2, res) = cvar
                .wait_timeout(g, remaining)
                .expect("gate mutex poisoned");
            g = g2;
            if res.timed_out() {
                // 防御性最后再试一次(避免 notify 丢失的假超时);仍满则报超时。
                return match g.try_admit(item, now) {
                    Ok(t) => Ok(t),
                    Err(AdmitError::WipCap { .. }) => Err(AdmitError::TimedOut),
                    Err(e) => Err(e),
                };
            }
        }
    }

    pub fn complete(
        &self,
        item_id: u64,
        now: MinuteOfDay,
        defects: u64,
    ) -> Result<CompletedItem, GateError> {
        let (lock, cvar) = &*self.inner;
        let r = lock
            .lock()
            .expect("gate mutex poisoned")
            .complete(item_id, now, defects);
        cvar.notify_all(); // 完成一件释放容量,唤醒阻塞的 admit
        r
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut ReleaseGate) -> R) -> R {
        let (lock, _) = &*self.inner;
        let mut g = lock.lock().expect("gate mutex poisoned");
        f(&mut g)
    }
}

impl ReleaseGate {
    /// 把工单的 Created→Released 包进投放闸。
    ///
    /// 判定顺序：工单状态机与 release 门控（在副本上，不改调用方）→
    /// 审阅分钟权重 → 渐放 → 夜间窗口 → CONWIP 上限。
    /// 前两步失败时，在制、夜间信用、渐放游标都不变。
    /// 后三步走 [`Self::try_admit`]，顺序与直接投件相同。
    /// 全部通过后才把调用方工单迁到 Released，并返回带渐放相位的 [`Permit`]。
    ///
    /// `review_minutes` 与 [`Item::review_minutes`] 同一把尺子：
    /// `wip + weight > upper` 才拒绝，精确等号可入。
    /// `now` 由调用方注入，不读系统钟。
    /// 渐放配置键是工单的 `graph_version`。
    ///
    /// 这是投放闸侧的 release 入口。直接调工单的 `release` 不会经过本闸。
    pub fn admit(
        &mut self,
        order: &mut WorkOrder,
        review_minutes: f64,
        now: MinuteOfDay,
    ) -> Result<Permit, Reject> {
        let mut probe = order.clone();
        if let Err(err) = probe.release() {
            return Err(Reject::Order(err));
        }
        if !(review_minutes.is_finite() && review_minutes > 0.0) {
            return Err(Reject::BadWeight {
                weight: review_minutes,
            });
        }

        let item = Item {
            id: self.next_item_id(),
            config: order.graph_version().as_str().to_string(),
            review_minutes,
        };
        let ticket = match self.try_admit(&item, now) {
            Ok(ticket) => ticket,
            Err(AdmitError::WipCap { wip, weight, cap }) => {
                return Err(Reject::AtCapacity { wip, weight, cap });
            }
            Err(AdmitError::CanaryProbeCap { config, cap }) => {
                let released = self
                    .canary
                    .as_ref()
                    .map(|canary| canary.released)
                    .unwrap_or(0);
                return Err(Reject::RampPhaseNotOpen {
                    config,
                    released,
                    cap,
                });
            }
            Err(AdmitError::NightThrottled { ratio }) => {
                return Err(Reject::NightThrottled { ratio });
            }
            Err(AdmitError::TimedOut) => unreachable!("try_admit does not block"),
        };
        let ramp = self.ramp_phase(&ticket.config);

        if let Err(err) = order.release() {
            self.rollback_admit(ticket.item_id, now);
            return Err(Reject::Order(err));
        }

        Ok(Permit {
            order_id: order.id().to_string(),
            ticket,
            ramp,
        })
    }

    fn next_item_id(&self) -> u64 {
        self.queue
            .iter()
            .map(|queued| queued.ticket.item_id)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }

    fn ramp_phase(&self, config: &str) -> RampPhase {
        match self.canary.as_ref() {
            Some(canary) if canary.config == config && canary.open => RampPhase::Open {
                probes_released: canary.released,
            },
            Some(canary) if canary.config == config => RampPhase::Probe {
                index: canary.released,
                max_probes: canary.max_probes,
            },
            _ => RampPhase::Ungated,
        }
    }

    /// 副本门控已通过，正本 release 仍失败时收回刚刚入队的一件。
    /// 不走 [`Self::complete`]，避免把这次回滚记成探针完成或触发晋升。
    fn rollback_admit(&mut self, item_id: u64, now: MinuteOfDay) {
        let Some(idx) = self
            .queue
            .iter()
            .position(|queued| queued.ticket.item_id == item_id)
        else {
            return;
        };
        let Some(queued) = self.queue.remove(idx) else {
            return;
        };
        if let Some(canary) = &mut self.canary {
            if !canary.open && canary.config == queued.ticket.config && canary.released > 0 {
                canary.released -= 1;
            }
        }
        self.check_refill_needed(now);
    }
}
