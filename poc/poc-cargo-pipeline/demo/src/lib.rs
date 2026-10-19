//! demo：分层 cargo 反馈流水线的实验台。
//! 一个 lib + 四类检查对象：proptest 属性、insta 快照、criterion 基准、miri UB。

pub mod parse_order;
pub use parse_order::parse_order;

/// 把比例钳制到 [0, 100]（整数百分点）。属性测试的主角：纯函数、无 I/O。
pub fn clamp_pct(pct: i64) -> u8 {
    pct.clamp(0, 100) as u8
}

/// 按百分比加价，饱和不溢出。proptest 关注点：任何输入都不 panic、结果单调。
/// 注意（proptest 实测抓到的坑）：u128 中间值必须饱和夹到 u64::MAX 再 `as u64`，
/// 裸 `as` 是截断，大 base 会绕回小值（最小反例 base=7718302959711109463, pct=239）。
pub fn apply_markup_cents(base_cents: u64, pct: u8) -> u64 {
    let add = (base_cents as u128 * pct as u128 / 100).min(u64::MAX as u128) as u64;
    base_cents.saturating_add(add)
}

/// UB 例子 A：调用方传入越界 index 时，unsafe 块写坏内存。
/// 实测发现：Rust 1.99 debug 构建的 unsafe 前置条件检查会在这里 panic 并 abort，
/// 即这一类 UB 原生测试就能抓到（详见 REPORT.md ③）。
pub fn oob_write(buf: &mut [u8; 4], idx: usize, v: u8) {
    unsafe {
        *buf.get_unchecked_mut(idx) = v;
    }
}

/// UB 例子 B：把非 0/1 的 u8 transmute 成 bool —— 非法值。
/// 原生 debug 构建不会报（x86 上"看起来正常"），只有 miri 能抓到。
/// 注意：clippy 的 transmute-int-to-bool 会在 -D warnings 门这里先拦一次
/// （静态 lint 与 miri 运行时的双层防御，见 REPORT.md ③）。
#[allow(clippy::transmute_int_to_bool)]
pub fn invalid_bool(b: u8) -> bool {
    unsafe { std::mem::transmute::<u8, bool>(b) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_basic() {
        assert_eq!(clamp_pct(-5), 0);
        assert_eq!(clamp_pct(50), 50);
        assert_eq!(clamp_pct(500), 100);
    }

    /// idx=4 越界。原生 debug：panic + abort；miri：out-of-bounds 报告。
    /// 标记 ignore：UB 演示不进常规绿跑，按需显式 `--ignored` 触发。
    #[test]
    #[ignore]
    fn miri_ub_write_oob() {
        let mut buf = [0u8; 4];
        oob_write(&mut buf, 4, 0xff);
    }

    /// b=2 是非法 bool 值。原生：静默通过；miri：invalid value 报告。
    #[test]
    #[ignore]
    fn miri_invalid_bool_value() {
        let b = invalid_bool(2);
        // 不 assert：原生下这个 UB "看起来能跑"，正是 miri 的价值证明。
        let _ = b;
    }
}
