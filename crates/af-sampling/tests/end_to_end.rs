//! 端到端：三机制串联。
//!
//! 真实件流（埋缺陷位）→ 金标注入器按率 p 注入队列 → CSP-1 相位机逐件处理 →
//! 放行点台账对账（金标缺陷放行 → 告警）。每 100 件真实件汇总一次运行结果
//! （成功=该批零告警、违规=告警数、一致性=金标分类一致率），喂给升迁引擎，
//! 判据配方由外部 JSON 注入。全部固定种子，可复现。

use std::collections::VecDeque;

use af_sampling::{
    Action, Csp1, Csp1Config, Engine, GoldenInjector, GoldenLedger, InjectorConfig, Lineage, Rng,
    RunOutcome, Tagged,
};

const N_REAL: usize = 1400;
const BATCH: usize = 100;

#[test]
fn end_to_end_inject_sample_promote() {
    // ── 外部注入的判据配方（模拟配方下发）：窗口 4、零告警才成功、一致性 ≥ 0.95、连净 2 次回升
    let recipe_json = r#"{
        "window": 4,
        "min_successes": 4,
        "max_violations": 0,
        "min_consistency": 0.95,
        "clean_to_recover": 2
    }"#;
    let mut engine = Engine::from_recipe_json(recipe_json).unwrap();

    // CSP-1：i=15，f=0.5，f_min=0.05
    let mut csp = Csp1::new(Csp1Config {
        enabled: true,
        i: 15,
        f: 0.5,
        f_min: 0.05,
    })
    .unwrap();
    // 注入器：p=0.04，缺陷金标占 70%
    let mut injector = GoldenInjector::new(
        InjectorConfig {
            p: 0.04,
            defect_share: 0.7,
        },
        777,
    )
    .unwrap();

    let mut item_rng = Rng::new(2026); // 缺陷位
    let mut csp_rng = Rng::new(888); // 抽检决策
    let mut queue = VecDeque::new();
    let mut ledger = GoldenLedger::default();

    let mut alarms_total = 0u64;
    let mut last_alarms = 0u64;
    // 分批一致性用的台账快照
    let mut snap_intercepted = 0u64;
    let mut snap_clean_passed = 0u64;
    let mut snap_seen = 0u64;
    let mut trace = String::new();

    for k in 0..N_REAL {
        // 真实件：约 8% 缺陷，外加第 600~624 件一段 25 连续缺陷“质量事故”
        let defective = (600..=624).contains(&k) || item_rng.chance(0.08);
        // 注入器可能先塞金标件，真实件随后；队列由 CSP-1 排干
        injector.enqueue_real(&mut queue, !defective);
        while let Some(item) = queue.pop_front() {
            let conforming = match &item {
                Tagged::Real { conforming, .. } => *conforming,
                Tagged::Golden { defective, .. } => !*defective,
            };
            let v = csp.process(conforming, &mut csp_rng);
            ledger.observe(&item, &v);
        }

        if (k + 1) % BATCH == 0 {
            // 每 100 件真实件：把台账增量折算成一次运行结果喂给升迁引擎
            let batch_alarms = ledger.alarms() - last_alarms;
            last_alarms = ledger.alarms();
            alarms_total += batch_alarms;
            // 该批金标一致率：增量（拦截的缺陷金标 + 正常放行的干净金标）/ 增量金标数
            let seen = ledger.golden_seen() - snap_seen;
            let correct = (ledger.defective_intercepted() - snap_intercepted)
                + (ledger.clean_passed() - snap_clean_passed);
            snap_seen = ledger.golden_seen();
            snap_intercepted = ledger.defective_intercepted();
            snap_clean_passed = ledger.clean_passed();
            let batch_consistency = if seen == 0 {
                1.0
            } else {
                correct as f64 / seen as f64
            };
            let outcome = RunOutcome {
                success: batch_alarms == 0,
                violations: batch_alarms as u32,
                consistency: batch_consistency,
            };
            let action = engine.record(outcome);
            trace.push_str(&format!(
                "batch{}: alarms={batch_alarms} consistency={batch_consistency:.2} -> {:?} @ {}\n",
                (k + 1) / BATCH,
                action,
                engine.current().name()
            ));
        }
    }

    // ── 契约与守恒断言
    assert_eq!(
        ledger.alarms(),
        ledger.defective_released(),
        "告警==放行缺陷金标"
    );
    assert_eq!(alarms_total, ledger.alarms(), "分批告警加总==台账总告警");
    assert_eq!(
        ledger.defective_intercepted() + ledger.defective_released(),
        injector.stats().defective,
        "缺陷金标=拦截+放行（守恒）"
    );
    assert_eq!(ledger.false_rejects(), 0, "干净金标不应被误拦");
    assert!(ledger.alarms() > 0, "f=0.5 抽检 + 缺陷流必有金标缺陷放行");
    assert!(
        csp.stats().to_screening_switches >= 1,
        "事故段必须把机器打回全检"
    );
    assert!(
        engine.attained() >= engine.current(),
        "current 永不超过 attained"
    );

    println!("── ledger ──");
    println!(
        "golden: seen={} defective={} clean={}\nintercepted={} released={} alarms={} consistency={:.3}",
        ledger.golden_seen(),
        injector.stats().defective,
        injector.stats().clean,
        ledger.defective_intercepted(),
        ledger.defective_released(),
        ledger.alarms(),
        ledger.consistency()
    );
    println!("── csp ──");
    let s = csp.stats();
    println!(
        "inspected={} passed_uninspected={} caught={} leaked={} to_sampling={} to_screening={} emp_rate={:.3} final_phase={:?}",
        s.inspected,
        s.passed_uninspected,
        s.defects_caught,
        s.defects_leaked,
        s.to_sampling_switches,
        s.to_screening_switches,
        s.empirical_sampling_rate(),
        csp.phase()
    );
    println!("── promotion ──");
    println!("{trace}");
    println!(
        "final: current={} attained={} events={}",
        engine.current().name(),
        engine.attained().name(),
        engine.events().len()
    );

    // 固定种子下的确定性终态（回归锚点，数值来自首次运行观察后钉死）。
    assert_eq!(engine.events().len(), 14);
    assert_eq!(engine.current(), Lineage::Agent);
    assert_eq!(
        engine.attained(),
        Lineage::Hybrid,
        "batch9 判据晋 hybrid、batch13 连净回升、batch14 熔断触底"
    );
    assert_eq!(ledger.alarms(), 8);
    // 四类处置动作都真实发生过
    let actions: Vec<_> = engine.events().iter().map(|e| e.action).collect();
    assert!(actions.contains(&Action::Demoted(Lineage::Agent)));
    assert!(actions.contains(&Action::Hold));
    assert!(actions.contains(&Action::Promoted(Lineage::Hybrid)));
    assert!(actions.contains(&Action::Recovered(Lineage::Hybrid)));
}
