//! 当前版本 1.2.0（声称 minor，实际含 4 处破坏性变更）。
pub fn apply_markup(base: u64, pct: u8) -> u64 {   // 1. 签名变了
    base + base * pct as u64 / 100
}

pub fn new_helper() -> bool {                       // 兼容新增
    true
}

pub struct Config {
    max: u8,                                        // 2. 字段转私有
}

pub enum Level {
    Low,
    Mid,                                            // 3. 新增枚举变体
    High,
}
// 4. clamp_pct 被整函数删除
