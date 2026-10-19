//! af-eval —— 评测产线骨架。
//!
//! 五条纪律：
//! 1. 基准三态 `draft → frozen → retired`。冻结后内容不可变，改动是 fork 新版。
//! 2. 信息屏障：评测运行只能读冻结快照；当前配方对运行不可见（盲评）。
//!    运行输入与基准集精确相交则停机，不产生成绩。
//! 3. 四个失败码 `E_INFRA` / `E_SUBJECT` / `E_BENCH` / `E_TIMEOUT` 按固定优先级路由。
//! 4. 结果账本只追加。SQLite 用 WAL、`busy_timeout` 5000ms、`synchronous=NORMAL`、`foreign_keys=ON`。
//! 5. 每次运行绑定基准快照哈希和被评对象版本，两者写进账本行。
//!
//! 时刻来自 [`Clock`]。超时用注入时刻的差值，不睡眠。

mod clock;
mod error;
mod hash;
mod ledger;
mod phase;
mod route;
mod snapshot;
mod validate;

pub use clock::{Clock, ManualClock, SystemClock};
pub use error::{EvalError, Result};
pub use hash::{sha256, sha256_hex, SHA256_HEX_LEN, SHA256_LEN};
pub use ledger::{
    Benchmark, Catalog, Durability, OpenStatus, ParentPin, Recipe, RunRecord, RunSpec, RunVerdict,
    RunView, SCHEMA_VERSION,
};
pub use phase::Phase;
pub use route::{route_failure, Classified, ExecSignals, FailureCode};
pub use snapshot::{
    contamination_overlap, frozen_snapshot, snapshot_hash, BenchCase, FrozenSnapshot,
};
