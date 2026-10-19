//! af-release-gate —— 投放端 WIP 闸门(CONWIP 语义)与首件渐放 PoC。
//!
//! - `gate`:核心闸门(标准审阅分钟计量的 WIP、上下限、夜间比例窗口、渐放)。
//! - `store`:在制队列与渐放游标的 SQLite 账本。首启建库，重入核对声明。
//! - `sim`:一天 1440 tick 的确定性模拟引擎。

mod clock;
pub mod gate;
pub mod sim;
mod store;

pub use clock::{Clock, ManualClock, SystemClock};
pub use gate::{
    AdmitError, CanaryCursor, CanaryPredicate, CanaryStatus, CompletedItem, ConfigError,
    EvalReport, GateConfig, GateError, GateEvent, InProcessItem, Item, MinuteOfDay, NightView,
    NightWindow, ReleaseGate, SharedGate, Ticket,
};
pub use sim::{SimConfig, SimReport};
pub use store::{DurableGate, OpenOutcome, PersistError, StorageProfile, SCHEMA_VERSION};
