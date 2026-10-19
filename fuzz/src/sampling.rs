//! 从字节构造升迁配方和运行观测序列，调用 af-sampling 的公共入口。
//!
//! `window` 钳在 1..=32。`Engine::from_recipe_json` 对合法配方会
//! `Vec::with_capacity(window)`；不钳的话 fuzz 字节可以写出 `u32::MAX`，
//! 分配失败会 abort，冒烟会被当成崩溃。

use af_sampling::{Engine, RunOutcome, SamplingError};

/// 配方头长度。更短的输入只走拒绝路径。
const HEADER: usize = 8;
/// 一条 `RunOutcome` 占的字节数。
const EVENT_LEN: usize = 4;
/// 单次输入最多回放的观测条数。
const MAX_EVENTS: usize = 64;

/// 跑一条输入。不够构造配方时调用拒绝路径（非法 JSON → `Err`，不分配窗口）。
pub fn fuzz_one(data: &[u8]) {
    // 固定非法配方：window = 0 在校验阶段被拒，发生在 with_capacity 之前。
    let _ = std::hint::black_box(Engine::from_recipe_json(
        r#"{"window":0,"min_successes":0,"max_violations":0,"min_consistency":0,"clean_to_recover":1}"#,
    ));

    match decode(data) {
        Some((json, outcomes)) => {
            let _ = std::hint::black_box(drive(&json, &outcomes));
        }
        None => {
            let _ = std::hint::black_box(Engine::from_recipe_json(""));
        }
    }
}

/// 字节 →（配方 JSON，观测序列）。不足 8 字节返回 `None`。
pub fn decode(data: &[u8]) -> Option<(String, Vec<RunOutcome>)> {
    if data.len() < HEADER {
        return None;
    }
    let window = u32::from(data[0] % 32) + 1;
    let min_successes = u32::from(data[1]) % (window + 1);
    let max_violations = u32::from(u16::from_le_bytes([data[2], data[3]]));
    // 千分比，写出的十进制一定落在 [0, 1]。
    let consistency_milli = u32::from(data[4]) * 1000 / 255;
    let clean_to_recover = u32::from(data[5] % 32) + 1;
    let whole = consistency_milli / 1000;
    let frac = consistency_milli % 1000;

    let mut outcomes = Vec::new();
    for chunk in data[HEADER..]
        .as_chunks::<EVENT_LEN>()
        .0
        .iter()
        .take(MAX_EVENTS)
    {
        outcomes.push(RunOutcome {
            success: chunk[0] & 1 == 1,
            // u16 上限 × window(≤32) 落在 u32 内，调试溢出检查不会在求和处炸。
            violations: u32::from(u16::from_le_bytes([chunk[1], chunk[2]])),
            consistency: f64::from(chunk[3]) / 255.0,
        });
    }

    let json = format!(
        "{{\"window\":{window},\"min_successes\":{min_successes},\"max_violations\":{max_violations},\"min_consistency\":{whole}.{frac:03},\"clean_to_recover\":{clean_to_recover}}}"
    );
    Some((json, outcomes))
}

/// 配方注入 + 逐条 `record`。JSON 由 [`decode`] 构造，应当通过校验。
pub fn drive(json: &str, outcomes: &[RunOutcome]) -> Result<Engine, SamplingError> {
    let mut engine = Engine::from_recipe_json(json)?;
    for outcome in outcomes {
        std::hint::black_box(engine.record(*outcome));
    }
    std::hint::black_box(engine.current());
    std::hint::black_box(engine.attained());
    std::hint::black_box(engine.events().len());
    Ok(engine)
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_sampling::Lineage;

    #[test]
    fn min_legal_seed_promotes_one_level() {
        let data = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/corpus/af_sampling_invariant_input/min_recipe_one_event"
        ))
        .expect("corpus seed");
        let (json, outcomes) = decode(&data).expect("header");
        let engine = drive(&json, &outcomes).expect("recipe accepted");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(engine.recipe().window, 1);
        assert_eq!(engine.recipe().min_successes, 1);
        assert_eq!(engine.recipe().max_violations, 0);
        assert_eq!(engine.recipe().clean_to_recover, 1);
        assert_eq!(engine.events().len(), 1);
        assert_eq!(engine.current(), Lineage::Hybrid);
        assert_eq!(engine.attained(), Lineage::Hybrid);
        assert!(engine.current() <= engine.attained());
    }

    #[test]
    fn short_input_does_not_panic() {
        fuzz_one(&[]);
        fuzz_one(&[0, 1, 2]);
        fuzz_one(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
    }
}
