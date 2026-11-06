//! 被 fuzz 的目标函数：解析 "base:pct" 订单串。
/// 返回 None 表示输入非法（fuzzer 应只把它当普通路径，不该崩）。
pub fn parse_order(s: &str) -> Option<(u64, u8)> {
    let mut it = s.splitn(2, ':');
    let base: u64 = it.next()?.parse().ok()?;
    let pct: u8 = it.next()?.parse().ok()?;
    // BUG（故意留的）：pct > 100 时应返回 None，这里 assert 崩给 fuzzer 抓。
    assert!(pct <= 100, "BUG: pct out of range: {pct}");
    Some((base, pct))
}
