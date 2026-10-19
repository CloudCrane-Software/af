//! af-sampling —— 抽样注入与升迁引擎（PoC）
//!
//! 三个机制合一：
//! 1. [`csp1`]：CSP-1 连续抽样状态机（全检 → 连续 i 件合格转抽检率 f → 遇缺陷回全检），
//!    f 受 f_min 托底，可整体关闭（enabled=false 时直通不检）。
//! 2. [`golden`]：已知答案注入器 —— 按率 p 往队列注入金标件（默认 70% 缺陷 / 30% 干净），
//!    统计拦截契约：金标缺陷件被放行 → 告警计数 +1。
//! 3. [`promotion`]：三谱系升迁引擎 agent → hybrid → deterministic，
//!    判据（成功次数/违规数/一致性率）全部由外部配方（JSON）注入；
//!    熔断：失败自动降一级；连续干净 N 次向已达成谱系回升一级。
//! 4. [`persist`]：抽样账本落 SQLite。谱系轨迹、当前水位、金标注入记录可重启读回。
//!    判据仍只走 [`Engine::record`]；恢复是重放轨迹，不是另一套升迁规则。

pub mod clock;
pub mod csp1;
pub mod golden;
pub mod persist;
pub mod promotion;
pub mod rng;

pub use clock::{Clock, ManualClock, SystemClock};
pub use csp1::{Csp1, Csp1Config, Csp1Stats, Phase, Verdict};
pub use golden::{GoldenInjector, GoldenLedger, InjectorConfig, InjectorStats, Tagged};
pub use persist::{
    GoldenDisposition, GoldenInjectionRecord, LedgerDeclaration, OpenFork, PersistedEvent,
    SamplingLedger, SCHEMA_VERSION,
};
pub use promotion::{Action, Engine, Event, Lineage, Recipe, RunOutcome};
pub use rng::Rng;

/// 本 crate 统一错误类型。
#[derive(Debug, thiserror::Error)]
pub enum SamplingError {
    #[error("CSP-1 配置非法: {0}")]
    InvalidCsp1Config(String),
    #[error("注入器配置非法: {0}")]
    InvalidInjectorConfig(String),
    #[error("升迁配方非法: {0}")]
    InvalidRecipe(String),
    #[error("升迁配方 JSON 解析失败: {0}")]
    RecipeJson(#[from] serde_json::Error),
    /// 重入时库内 schema 版本或关键参数与本次声明不一致。启动失败，不改库。
    #[error("抽样账本声明不符: {0}")]
    DeclarationMismatch(String),
    /// 库内水位、轨迹或金标记录自相矛盾。启动失败，不静默修复。
    #[error("抽样账本损坏，拒绝启动: {0}")]
    LedgerCorrupt(String),
    /// 调用方写入被拒绝（序号溢出、重复金标序号）。库与内存都不变。
    #[error("抽样账本拒绝写入: {0}")]
    Rejected(String),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
}
