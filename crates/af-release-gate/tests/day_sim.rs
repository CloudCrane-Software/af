//! 一天流量模拟:验证 ①WIP 不超限 ②下限不断粮(NeedRefill 预警 + 不清零)
//! ③夜间比例限流 ④首件渐放。到达序列为手工构造(确定性,无随机)。
//!
//! 运营背景:审阅产能按小时注入(白天 5 标准审阅分钟/分,夜间 1.5),
//! 00:00 从昨日结转的 ~15 件在制起跑,夜间窗口 22:00→06:00 只放行 30%。

use af_release_gate::sim::run_day;
use af_release_gate::{EvalReport, GateConfig, Item, MinuteOfDay, NightWindow, SimConfig};

fn weight(id: u64) -> f64 {
    15.0 + ((id * 7) % 16) as f64 // 15~30 标准审阅分钟,均值 22.5
}

/// 手工构造的到达序列。
fn build_schedule() -> Vec<(MinuteOfDay, Item)> {
    let mut id = 1u64;
    let mut v: Vec<(MinuteOfDay, Item)> = Vec::new();
    let push = |v: &mut Vec<(MinuteOfDay, Item)>, m: MinuteOfDay, config: &str, id: &mut u64| {
        v.push((
            m,
            Item {
                id: *id,
                config: config.into(),
                review_minutes: weight(*id),
            },
        ));
        *id += 1;
    };

    // 夜一 [0,360) 每 7 分钟 1 件(52 件)——30% 限流,余量扣到清晨。
    for m in (0..360).step_by(7) {
        push(&mut v, m, "v1", &mut id);
    }
    // 早高峰 [360,480) 每 5 分钟 1 件(24 件)+ 清晨限流补量 → WIP 顶满上限。
    for m in (360..480).step_by(5) {
        push(&mut v, m, "v1", &mut id);
    }
    // 新配置 v2 渐放 [480,540):v2 以每 2 分钟 1 件突发上线(8 件,触发探针封顶),
    // v1 照常每 4 分钟 1 件(13 件)。两路合流后按分钟排序。
    let mut rollout: Vec<(MinuteOfDay, &str)> = Vec::new();
    for m in (480..496).step_by(2) {
        rollout.push((m, "v2"));
    }
    for m in (486..540).step_by(4) {
        rollout.push((m, "v1"));
    }
    rollout.sort_by_key(|(m, _)| *m);
    for (m, cfg) in rollout {
        push(&mut v, m, cfg, &mut id);
    }
    // 午间 [540,840) 每 6 分钟 1 件(50 件)——消耗早高峰积压。
    for m in (540..840).step_by(6) {
        push(&mut v, m, "v1", &mut id);
    }
    // 供给空窗 [840,975):无到达——验证下限预警(断粮前兆)且不清零。
    // 补给脉冲 [975,1035) 每 3 分钟 1 件(20 件)——把 WIP 拉回安全区。
    for m in (975..1035).step_by(3) {
        push(&mut v, m, "v1", &mut id);
    }
    // 午后稳态 [1035,1320) 按 4/5 分钟交替节奏(~63 件),供给≈产能。
    let mut m = 1035;
    let mut k = 0usize;
    while m < 1320 {
        push(&mut v, m, "v1", &mut id);
        m += if k % 2 == 0 { 4 } else { 5 };
        k += 1;
    }
    // 夜二 [1320,1440) 每 4 分钟 1 件(30 件)——夜间限流,余量留给次日清晨。
    for m in (1320..1440).step_by(4) {
        push(&mut v, m, "v1", &mut id);
    }
    v
}

fn capacity_by_hour() -> [f64; 24] {
    let mut c = [5.0; 24]; // 白天 5 标准审阅分钟/分
    for h in [22, 23, 0, 1, 2, 3, 4, 5] {
        c[h] = 1.5; // 夜间人力少
    }
    c
}

#[test]
fn day_traffic_respects_caps_and_never_starves() {
    let schedule = build_schedule();
    let total_work: f64 = schedule.iter().map(|(_, i)| i.review_minutes).sum();
    // 昨日结转:15 件在制(≈337 分钟),连续运营不从空闸起跑。
    let start_inventory: Vec<Item> = (1..=15)
        .map(|i| Item {
            id: 10_000 + i,
            config: "v1".into(),
            review_minutes: weight(i),
        })
        .collect();
    let seed_count = start_inventory.len();

    let report = run_day(
        &schedule,
        SimConfig {
            gate: GateConfig {
                wip_upper_minutes: 400.0,
                wip_lower_minutes: 120.0,
                night_window: Some(NightWindow {
                    start: 1320,
                    end: 360,
                    ratio: 0.3,
                }),
            },
            capacity_by_hour: capacity_by_hour(),
            start_inventory,
            canary: Some((
                480,
                "v2".to_string(),
                3,
                // 评估谓词:前 2 件探针干净完成才算评估通过(模拟首轮质检)。
                Box::new(|r: &EvalReport| r.probes_completed >= 2 && r.defects == 0),
            )),
        },
    );

    // ---- 摘要(作为证据)----
    let cap_total: f64 = capacity_by_hour().iter().sum::<f64>() * 60.0;
    println!(
        "items={} total_work={:.0}min capacity={:.0}min util={:.1}%",
        schedule.len(),
        total_work,
        cap_total,
        100.0 * total_work / cap_total
    );
    println!(
        "admits={} completions={} work_done={:.0} rejects={:?}",
        report.admits, report.completions, report.work_completed_minutes, report.rejects
    );
    println!(
        "max_wip={:.1} zero_wip_ticks={} below_lower_ticks={} starved_despite_supply={} canary_hold_starve={}",
        report.max_wip, report.zero_wip_ticks, report.below_lower_ticks,
        report.starved_despite_supply, report.canary_hold_starve
    );
    println!(
        "night: arrivals={} admitted={} ratio={:.3}",
        report.night_window_arrivals,
        report.night_window_admitted,
        report.night_window_admitted as f64 / report.night_window_arrivals as f64
    );
    println!("canary: {:?}", report.canary);
    println!("events: {:?}", report.events);
    println!(
        "end_backlog={} end_wip={:.1}",
        report.end_backlog, report.ticks[1439].wip
    );
    for t in (0..1440).step_by(60) {
        let r = &report.ticks[t];
        println!(
            "{:02}:{:02} wip={:6.1} inproc={:3} backlog={:3} deferred={:3} parked={}",
            t / 60,
            t % 60,
            r.wip,
            r.in_process,
            r.backlog,
            r.night_deferred,
            r.canary_parked
        );
    }
    // 空窗期细看(证据)
    for t in (900..1000).step_by(10) {
        let r = &report.ticks[t];
        println!(
            "  t={} wip={:6.1} inproc={:3} backlog={:3} admitted_now={} completed_now={}",
            t, r.wip, r.in_process, r.backlog, r.admitted_now, r.completed_now
        );
    }

    // ---- 断言 ----
    // ① WIP 不超限:全天从未突破上限,且峰值日真的撞过上限(否则上限未被验证)。
    assert!(
        report.max_wip <= 400.0 + 1e-6,
        "WIP exceeded upper: {}",
        report.max_wip
    );
    assert!(report.rejects.wip_cap > 0, "峰值日应当真的撞过上限");
    // ② 下限不断粮:全程不清零;有预警;无"有供给却断粮"。
    assert_eq!(report.zero_wip_ticks, 0, "审阅站清零 = 断粮");
    assert_eq!(report.starved_despite_supply, 0);
    assert_eq!(report.canary_hold_starve, 0);
    assert!(report.below_lower_ticks > 0, "构造的供给空窗应触发下限预警");
    assert!(report
        .events
        .iter()
        .any(|e| matches!(e, af_release_gate::GateEvent::NeedRefill { .. })));
    // ③ 夜间比例:两个窗口合计,放行量 ≈ 到达量 × 0.3(初始信用 ±余量)。
    let arr = report.night_window_arrivals as f64;
    let adm = report.night_window_admitted as f64;
    assert!(adm <= 0.3 * arr + 2.0, "夜间放行超比例: {adm}/{arr}");
    assert!(adm >= 0.3 * arr - 4.0, "夜间限流过紧: {adm}/{arr}");
    assert!(report.rejects.night > 0, "夜间限流应真的拒绝过");
    // ④ 渐放:gated 期最多 3 件探针,首个干净完成即晋升,泊位清空。
    let cs = report.canary.as_ref().unwrap();
    assert!(cs.promoted, "谓词应通过并晋升");
    assert_eq!(cs.probes_released_while_gated, 3);
    let at = cs.promoted_at.expect("promoted_at");
    assert!((480..600).contains(&at), "晋升应在 rollout 后不久: {at}");
    assert!(report.rejects.canary > 0, "渐放封顶应真的拒绝过第 4 件");
    // 账目守恒:准入数 + 结转数 = 完成数 + 期末在制。
    assert_eq!(
        report.admits + seed_count as u64,
        report.completions + report.ticks[1439].in_process as u64
    );
}
