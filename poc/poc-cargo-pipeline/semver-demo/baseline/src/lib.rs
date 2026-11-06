//! 基线版本 0.1.0：公开 API。
pub fn clamp_pct(pct: i64) -> u8 {
    pct.clamp(0, 100) as u8
}

pub fn apply_markup(base: u64) -> u64 {
    base + base / 10
}

pub struct Config {
    pub max: u8,
}

pub enum Level {
    Low,
    High,
}
