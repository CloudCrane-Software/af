//! 一天流量的确定性 tick 模拟(验证不超限与下限不断粮)。
//!
//! 每个 tick(=1 分钟):
//! 1. 以 `capacity_per_tick` 标准审阅分钟的处理能力按 FIFO 推进在制件,完成者出队
//!    (闸门 `complete` 释放容量,并驱动渐放评估);
//! 2. 夜间窗口刚结束时,把被限流扣下的件放回待审队首(等价于"其余量等白天");
//! 3. 该分钟的到达件进入待审 backlog;
//! 4. 供给循环:只要 backlog 有件就尝试 `try_admit`,按拒绝原因分别处理
//!    (WipCap→本 tick 停;NightThrottled→扣到夜间暂存,出窗再放;CanaryProbeCap→停)。

use std::collections::VecDeque;

use crate::gate::{
    AdmitError, CanaryPredicate, GateConfig, GateEvent, Item, MinuteOfDay, ReleaseGate,
};

pub struct SimConfig {
    pub gate: GateConfig,
    /// 每小时每小时段(60 tick)可投入的标准审阅分钟/分钟——审阅人力按小时注入,
    /// 夜间人力少是现实约束(否则夜间限流必然结构性断粮)。
    pub capacity_by_hour: [f64; 24],
    /// 00:00 起跑时的结转在制(连续运营,不该从空闸开始)。
    pub start_inventory: Vec<Item>,
    /// 渐放 setup:(开始 tick, 配置名, 最多探针件数, 谓词)。
    pub canary: Option<(MinuteOfDay, String, usize, CanaryPredicate)>,
}

#[derive(Debug, Clone)]
pub struct TickRecord {
    pub tick: u32,
    pub wip: f64,
    pub in_process: usize,
    pub backlog: usize,
    pub night_deferred: usize,
    pub canary_parked: usize,
    pub admitted_now: u32,
    pub completed_now: u32,
}

#[derive(Debug, Default, Clone)]
pub struct RejectCounts {
    pub wip_cap: u64,
    pub night: u64,
    pub canary: u64,
}

#[derive(Debug, Clone)]
pub struct CanarySummary {
    pub config: String,
    pub promoted: bool,
    pub promoted_at: Option<MinuteOfDay>,
    pub probes_released_while_gated: usize,
}

#[derive(Debug, Default)]
pub struct SimReport {
    pub ticks: Vec<TickRecord>,
    pub admits: u64,
    pub completions: u64,
    pub rejects: RejectCounts,
    /// 处于夜间窗口期间的到达件数 / 放行件数(sim 口径:按到达计,不用重试计数)。
    pub night_window_arrivals: u64,
    pub night_window_admitted: u64,
    pub canary: Option<CanarySummary>,
    pub events: Vec<GateEvent>,
    pub max_wip: f64,
    /// wip == 0 的 tick 数(全程有供给,应为 0)。
    pub zero_wip_ticks: u32,
    /// wip < 下限的 tick 数。
    pub below_lower_ticks: u32,
    /// "有供给却断粮"次数:tick 末 wip < 下限 且 backlog 非空,
    /// 且本 tick 不是被夜间限流停住(其余停止原因都合法)。
    pub starved_despite_supply: u32,
    /// 渐放泊位上有件、backlog 已空、wip 仍在下限之下:渐放策略主动持量。
    pub canary_hold_starve: u32,
    pub work_completed_minutes: f64,
    pub end_backlog: usize,
}

struct SimItem {
    ticket_id: u64,
    weight: f64,
    remaining: f64,
}

/// 跑一天(1440 tick)。`schedule` 必须按分钟升序(手工构造的到达序列)。
pub fn run_day(schedule: &[(MinuteOfDay, Item)], cfg: SimConfig) -> SimReport {
    assert!(
        schedule.windows(2).all(|w| w[0].0 <= w[1].0),
        "schedule must be sorted by minute"
    );
    let SimConfig {
        gate: gate_cfg,
        capacity_by_hour,
        start_inventory,
        canary,
    } = cfg;
    let mut canary_setup = canary;
    let mut gate = ReleaseGate::new(gate_cfg).expect("valid gate config");
    // 换日结转:直接注入在制(调用方保证不超上限),并纳入模拟的处理列表。
    let mut in_process: Vec<SimItem> = Vec::new();
    for item in &start_inventory {
        gate.seed_wip(item);
        in_process.push(SimItem {
            ticket_id: item.id,
            weight: item.review_minutes,
            remaining: item.review_minutes,
        });
    }
    let mut canary_summary = canary_setup.as_ref().map(|(_, name, _, _)| CanarySummary {
        config: name.clone(),
        promoted: false,
        promoted_at: None,
        probes_released_while_gated: 0,
    });
    let canary_name = canary_summary.as_ref().map(|c| c.config.clone());
    let mut canary_promoted = false;

    let mut by_minute: Vec<Vec<&Item>> = vec![Vec::new(); 1440];
    for (m, item) in schedule {
        by_minute[*m as usize].push(item);
    }

    let mut backlog: VecDeque<&Item> = VecDeque::new();
    let mut night_deferred: VecDeque<&Item> = VecDeque::new();
    // 渐放泊位:被 CanaryProbeCap 扣住的新配置件。晋升后放回 backlog 队首。
    let mut canary_parked: VecDeque<&Item> = VecDeque::new();
    let mut was_in_window = gate.in_night_window(0);
    let lower = gate.config().wip_lower_minutes;
    let upper = gate.config().wip_upper_minutes;

    let mut report = SimReport {
        max_wip: 0.0,
        ..Default::default()
    };

    for tick in 0..1440u32 {
        let now = tick;
        let mut admitted_now = 0u32;
        let mut completed_now = 0u32;
        let mut stopped_by_night = false;

        // canary 开启(按值取走谓词)。
        if let Some((_, name, max_probes, predicate)) = canary_setup.take_if(|(s, ..)| *s == now) {
            gate.begin_canary(&name, max_probes, predicate)
                .expect("begin canary");
        }

        // 1) 处理:FIFO 倾注产能,完成者出闸(释放容量/驱动渐放)。
        let mut budget = capacity_by_hour[(now / 60) as usize];
        let mut done: Vec<(u64, f64)> = Vec::new();
        for it in in_process.iter_mut() {
            if budget <= 1e-9 {
                break;
            }
            let take = it.remaining.min(budget);
            it.remaining -= take;
            budget -= take;
            if it.remaining <= 1e-9 {
                done.push((it.ticket_id, it.weight));
            }
        }
        in_process.retain(|it| it.remaining > 1e-9);
        for (id, weight) in done {
            // 手工构造的干净流量:完成件零缺陷。
            gate.complete(id, now, 0)
                .expect("completed item known to gate");
            completed_now += 1;
            report.completions += 1;
            report.work_completed_minutes += weight;
        }

        // 捕获事件(渐放晋升等);晋升时泊位件放回 backlog 队首。
        for ev in gate.drain_events() {
            if let GateEvent::CanaryPromoted {
                config,
                probes_released,
                at,
            } = &ev
            {
                canary_promoted = true;
                if let Some(cs) = &mut canary_summary {
                    if *config == cs.config {
                        cs.promoted = true;
                        cs.promoted_at = Some(*at);
                        cs.probes_released_while_gated = *probes_released;
                    }
                }
                let mut merged: VecDeque<&Item> = canary_parked.drain(..).collect();
                merged.extend(backlog.drain(..));
                backlog = merged;
            }
            report.events.push(ev);
        }

        // 2) 夜间窗口刚结束 → 暂存件放回队首(保持原到达顺序)。
        let in_window = gate.in_night_window(now);
        if was_in_window && !in_window {
            let mut merged: VecDeque<&Item> = night_deferred.drain(..).collect();
            merged.extend(backlog.drain(..));
            backlog = merged;
        }
        was_in_window = in_window;

        // 3) 到达入 backlog;统计窗口内到达(先计数再 drain)。
        //    rollout 中的新配置件不进普通 backlog,直接进渐放泊位(专门通道,
        //    探针不被存量积压淹没;晋升后随泊位放回)。
        if in_window {
            report.night_window_arrivals += by_minute[now as usize].len() as u64;
        }
        for item in by_minute[now as usize].drain(..) {
            let to_parked =
                !canary_promoted && canary_name.as_deref() == Some(item.config.as_str());
            if to_parked {
                canary_parked.push_back(item);
            } else {
                backlog.push_back(item);
            }
        }

        // 4) 供给循环:先试渐放泊位(gated 的新配置件),再按 FIFO 供给 backlog。
        //    泊位优先是有意的:探针应尽快进入评估,不被积压淹没(PoC 简化,见 RESULT)。
        loop {
            let Some(front) = canary_parked.front().copied() else {
                break;
            };
            let probe = front.clone();
            match gate.try_admit(&probe, now) {
                Ok(_t) => {
                    canary_parked.pop_front();
                    in_process.push(SimItem {
                        ticket_id: probe.id,
                        weight: probe.review_minutes,
                        remaining: probe.review_minutes,
                    });
                    admitted_now += 1;
                    report.admits += 1;
                    if in_window {
                        report.night_window_admitted += 1;
                    }
                }
                Err(AdmitError::CanaryProbeCap { .. }) => {
                    report.rejects.canary += 1;
                    break; // 仍 gated,本 tick 探针位已满
                }
                Err(AdmitError::WipCap { .. }) => break, // 无空位,下个 tick 再试
                Err(AdmitError::NightThrottled { .. }) => {
                    canary_parked.pop_front();
                    night_deferred.push_back(front);
                    report.rejects.night += 1;
                    stopped_by_night = true;
                    break;
                }
                Err(AdmitError::TimedOut) => unreachable!(),
            }
        }
        loop {
            let Some(front) = backlog.front().copied() else {
                break;
            };
            let probe = front.clone();
            match gate.try_admit(&probe, now) {
                Ok(_t) => {
                    backlog.pop_front();
                    in_process.push(SimItem {
                        ticket_id: probe.id,
                        weight: probe.review_minutes,
                        remaining: probe.review_minutes,
                    });
                    admitted_now += 1;
                    report.admits += 1;
                    if in_window {
                        report.night_window_admitted += 1;
                    }
                }
                Err(AdmitError::WipCap { .. }) => {
                    report.rejects.wip_cap += 1;
                    break; // 本 tick 无空位,下个 tick 完成后再试
                }
                Err(AdmitError::NightThrottled { .. }) => {
                    // 本窗口配额用尽:件扣到夜间暂存,不再重复请求(不烧信用)。
                    backlog.pop_front();
                    night_deferred.push_back(front);
                    report.rejects.night += 1;
                    stopped_by_night = true;
                }
                Err(AdmitError::CanaryProbeCap { .. }) => {
                    // 新配置在 gated 期被扣住,停泊位等待;其余流量照常。
                    backlog.pop_front();
                    canary_parked.push_back(front);
                    report.rejects.canary += 1;
                }
                Err(AdmitError::TimedOut) => unreachable!("try_admit never times out"),
            }
        }

        // 5) 观测。
        let wip = gate.wip_minutes();
        if wip <= 1e-9 {
            report.zero_wip_ticks += 1;
        }
        if wip < lower {
            report.below_lower_ticks += 1;
            if !backlog.is_empty() {
                if !stopped_by_night {
                    // 有供给、也未被夜间限流挡住,却仍在下限之下:闸门失职。
                    report.starved_despite_supply += 1;
                }
            } else if !canary_parked.is_empty() {
                // 渐放策略主动持量(backlog 已空仍不放泊位件)。
                report.canary_hold_starve += 1;
            }
        }
        report.max_wip = report.max_wip.max(wip);
        debug_assert!(wip <= upper + 1e-6, "wip {wip} exceeded upper {upper}");
        report.ticks.push(TickRecord {
            tick: now,
            wip,
            in_process: gate.in_process_count(),
            backlog: backlog.len(),
            night_deferred: night_deferred.len(),
            canary_parked: canary_parked.len(),
            admitted_now,
            completed_now,
        });
    }

    if let Some(cs) = &mut canary_summary {
        // 全天未晋升时,取闸门终态的探针数。
        if !cs.promoted {
            if let Some(st) = gate.canary_status() {
                cs.probes_released_while_gated = st.released;
            }
        }
    }
    report.canary = canary_summary;
    report.end_backlog = backlog.len() + night_deferred.len() + canary_parked.len();
    report
}
