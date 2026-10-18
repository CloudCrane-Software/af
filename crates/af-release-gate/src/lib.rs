//! af-release-gate —— 投放端 WIP 闸门(CONWIP 语义)与首件渐放 PoC。
//!
//! - `gate`:核心闸门(标准审阅分钟计量的 WIP、上下限、夜间比例窗口、渐放)。
//! - `sim`:一天 1440 tick 的确定性模拟引擎。

pub mod gate;
pub mod sim;

pub use gate::{
    AdmitError, CanaryPredicate, CanaryStatus, CompletedItem, ConfigError, EvalReport, GateConfig,
    GateError, GateEvent, Item, MinuteOfDay, NightWindow, ReleaseGate, SharedGate, Ticket,
};
pub use sim::{SimConfig, SimReport};
