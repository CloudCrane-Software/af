use demo::{apply_markup_cents, clamp_pct};
use proptest::prelude::*;

proptest! {
    /// 属性 1：clamp_pct 输出永远落在 [0,100]，且对已合法输入是恒等。
    #[test]
    fn clamp_pct_stays_in_range(pct in any::<i64>()) {
        let out = clamp_pct(pct);
        prop_assert!(out <= 100);
        let legal: u8 = (pct.rem_euclid(101)) as u8;
        prop_assert_eq!(clamp_pct(legal as i64), legal);
    }

    /// 属性 2：加价永不 panic、永不小于原价、pct 越大结果不减（单调性）。
    #[test]
    fn markup_monotonic_and_saturating(base in any::<u64>(), pct in any::<u8>()) {
        let out = apply_markup_cents(base, pct);
        prop_assert!(out >= base);
        let lower = apply_markup_cents(base, pct.saturating_sub(1));
        prop_assert!(out >= lower);
    }
}
