//! af-sampling 路径基准。只由 `cargo bench` 执行，不进入 `cargo test`。
//!
//! 不读墙钟，也不用 sleep。随机源是固定种子的 xorshift。
//! 计时窗口不含状态机构造。
//!
//! - `sampling_decision`：先用一件合格品进入 `Sampling`（窗外），再对连续合格品调用
//!   `Csp1::process`。抽检相位的热点是 `next_f64() < f`。合格品不会回全检，
//!   所以整批都停在这条判定上。`f` 低于 1 且不低于 `f_min`，两条分支都会走到。
//! - `lineage_promotion`：外部 JSON 配方与升迁单测相同（窗口 5、三判据、连净 3）。
//!   每个引擎从 Agent 记两窗干净运行，一级一级升到 Hybrid 再升到 Deterministic。
//!   计时只包住 `Engine::record`。`current <= attained` 在窗外复核。

use af_sampling::{Action, Csp1, Csp1Config, Engine, Lineage, Phase, Rng, RunOutcome};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::hint::black_box;
use std::time::{Duration, Instant};

const ITEMS: usize = 10_000;
const CLIMBS: usize = 256;
/// 与 `promotion` 单测同一份配方：干净窗口积满才晋升，一次只升一级。
const RECIPE_JSON: &str = r#"{
    "window": 5,
    "min_successes": 5,
    "max_violations": 0,
    "min_consistency": 0.98,
    "clean_to_recover": 3
}"#;

const CLEAN: RunOutcome = RunOutcome {
    success: true,
    violations: 0,
    consistency: 1.0,
};

fn elements(n: usize) -> u64 {
    u64::try_from(n).expect("count fits u64")
}

fn primed() -> (Csp1, Rng) {
    let mut csp = Csp1::new(Csp1Config {
        enabled: true,
        i: 1,
        f: 0.2,
        f_min: 0.05,
    })
    .expect("CSP-1 配置：f 不低于 f_min");
    let mut rng = Rng::new(2026);
    let verdict = csp.process(true, &mut rng);
    assert_eq!(verdict.phase_after, Phase::Sampling);
    assert!(csp.effective_rate() >= csp.config().f_min);
    (csp, rng)
}

/// 抽检相位上的判定次数。返回实际检验件数。
fn decide(csp: &mut Csp1, rng: &mut Rng) -> u64 {
    let mut inspected = 0u64;
    for _ in 0..ITEMS {
        let verdict = csp.process(black_box(true), rng);
        inspected += u64::from(verdict.inspected);
    }
    black_box(inspected)
}

fn sampling_decision(c: &mut Criterion) {
    let (csp0, rng0) = primed();
    let mut probe_csp = csp0.clone();
    let mut probe_rng = rng0.clone();
    let expected = decide(&mut probe_csp, &mut probe_rng);
    assert!(
        expected > 0 && expected < elements(ITEMS),
        "抽检率介于 0 和 1 之间，两条分支都要走到，实际检验 {expected}"
    );
    assert_eq!(probe_csp.phase(), Phase::Sampling);

    let mut group = c.benchmark_group("sampling_decision");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(elements(ITEMS)));
    group.bench_function("csp1_process_sampling_phase", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let (mut csp, mut rng) = (csp0.clone(), rng0.clone());
                let start = Instant::now();
                let inspected = decide(&mut csp, &mut rng);
                total += start.elapsed();
                assert_eq!(inspected, expected);
                assert_eq!(csp.phase(), Phase::Sampling);
            }
            total
        });
    });
    group.finish();
}

fn climb(engine: &mut Engine, steps: usize) {
    for _ in 0..steps {
        black_box(engine.record(black_box(CLEAN)));
    }
}

fn assert_climbed(engine: &Engine, window: usize, steps: usize) {
    assert_eq!(engine.current(), Lineage::Deterministic);
    assert_eq!(engine.attained(), Lineage::Deterministic);
    assert!(
        engine.current() <= engine.attained(),
        "current 不超过 attained"
    );
    let events = engine.events();
    assert_eq!(events.len(), steps);
    for (i, event) in events.iter().enumerate() {
        let want = if i + 1 == window {
            Action::Promoted(Lineage::Hybrid)
        } else if i + 1 == steps {
            Action::Promoted(Lineage::Deterministic)
        } else {
            Action::Hold
        };
        assert_eq!(
            event.tick,
            u64::try_from(i + 1).expect("tick fits u64"),
            "step {}",
            i + 1
        );
        assert_eq!(event.action, want, "step {}", i + 1);
    }
}

fn lineage_promotion(c: &mut Criterion) {
    let template = Engine::from_recipe_json(RECIPE_JSON).expect("升迁配方 JSON");
    let window = usize::try_from(template.recipe().window).expect("window fits usize");
    let steps = window.checked_mul(2).expect("两窗升迁步数");
    let mut probe = template.clone();
    climb(&mut probe, steps);
    assert_climbed(&probe, window, steps);

    let records = elements(CLIMBS) * elements(steps);
    let mut group = c.benchmark_group("lineage_promotion");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Elements(records));
    group.bench_function("agent_hybrid_deterministic", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let mut engines = Vec::with_capacity(CLIMBS);
                for _ in 0..CLIMBS {
                    engines.push(template.clone());
                }
                let start = Instant::now();
                for engine in &mut engines {
                    climb(engine, steps);
                }
                black_box(engines.len());
                total += start.elapsed();
                for engine in &engines {
                    assert_climbed(engine, window, steps);
                }
            }
            total
        });
    });
    group.finish();
}

criterion_group!(sampling_benches, sampling_decision, lineage_promotion);
criterion_main!(sampling_benches);
