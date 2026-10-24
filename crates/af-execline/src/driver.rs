//! 订单生命周期驱动：构造 → 引擎门控 → 执行 → 收尾，全程 ledger + MES 记账。
//!
//! 驱动层不持有任何门控规则——"能不能做"全部问 af-engine 子进程；本层只负责
//! 编排顺序、记账与幂等。每个 op 带幂等键 `{stable_id}/{op}`（落地契约 v0
//! §6.2 at-least-once + 幂等键）：ledger 里已有成功记录的 op 不二次调用引擎，
//! 同一 order_id 二次驱动只重放，不产生重复终态。
//!
//! 拒绝路径全部类型化：引擎的 `GateError` / `DecisionError` 原样包进
//! [`ExeclineError::EngineRejected`]，不 panic、不降级成字符串。

use std::fmt;
use std::path::{Path, PathBuf};

use af_workorder::{BudgetVoucher, GraphVersion, LineBinding, OrderState, Sla, WorkOrder};

use crate::clock::Clock;
use crate::engine::EngineClient;
use crate::error::ExeclineError;
use crate::exec::{ExecContext, ExecOutput, ExecuteUnit};
use crate::ledger::{LedgerRecord, OrderLedger};
use crate::mes::MesFileWriter;

/// MES 记账的生命周期事件类型。绑定/凭证/执行登记不单独发事件——
/// 事件账本与订单 ledger 分工：ledger 记全部 op，MES 记产线关心的四类跃迁。
pub const MES_TRANSITION_KINDS: &[&str] = &["release", "start", "complete", "close"];

/// 驱动配置。一次驱动一份；同 order_id 复用同一 `out_dir` 即重放。
pub struct DriverConfig {
    pub order_id: String,
    /// 与 `WorkOrder::graph_version` 一次性绑定，之后不可变。
    pub graph_version: String,
    pub sla: Sla,
    /// release 门材料①（仅 Created 态可登记）。
    pub line: LineBinding,
    /// release 门材料②（仅 Created 态可登记；未签名会在 release 门被点名）。
    pub voucher: BudgetVoucher,
    /// 账本根目录：`ledger.jsonl` / `mes-events.jsonl` / `artifacts/`。
    pub out_dir: PathBuf,
    /// MES 事件的 station_id。
    pub station_id: String,
}

/// 一次 `drive` 的报告。
pub struct DriveReport {
    /// 驱动后的整单快照（终态 Closed）。
    pub order: WorkOrder,
    /// 本次驱动是否存在重放（ledger 里已有成功记录的 op）。
    pub replayed: bool,
    /// 本次驱动追加的 ledger 行数。
    pub appended: u64,
    /// 本次驱动产出的 MES 事件（纯重放时为空）。
    pub mes_events: Vec<af_mes::MesEvent>,
    /// 执行单元产出（重放时由订单快照重建，产出文件不重写）。
    pub exec_output: Option<ExecOutput>,
}

/// 订单生命周期驱动。
pub struct OrderLifecycleDriver {
    cfg: DriverConfig,
    client: EngineClient,
    clock: Box<dyn Clock>,
    order: WorkOrder,
    ledger: OrderLedger,
    mes: MesFileWriter,
    /// 已成功完成的幂等键。
    done: std::collections::BTreeSet<String>,
    /// 本次驱动累计的 MES 事件（`drive` 取出）。
    drive_mes: Vec<af_mes::MesEvent>,
}

impl OrderLifecycleDriver {
    /// 打开驱动。`out_dir` 下已有 ledger 则重载恢复（身份不符 = EX2002）。
    pub fn open(
        _cfg: DriverConfig,
        _client: EngineClient,
        _clock: Box<dyn Clock>,
    ) -> Result<Self, ExeclineError> {
        unimplemented!("RED：OrderLifecycleDriver::open 待最小实现")
    }

    /// 当前整单快照。
    pub fn order(&self) -> &WorkOrder {
        &self.order
    }

    /// 当前状态。
    pub fn state(&self) -> OrderState {
        self.order.state()
    }

    /// 真引擎二进制路径（证明判定走子进程，不打桩）。
    pub fn engine_bin(&self) -> &Path {
        self.client.bin()
    }

    pub fn ledger_path(&self) -> &Path {
        self.ledger.path()
    }

    pub fn mes_path(&self) -> &Path {
        self.mes.path()
    }

    /// 构造订单（本地，无引擎调用）：写入初始快照。
    pub fn create_order(&mut self) -> Result<(), ExeclineError> {
        unimplemented!("RED：create_order 待最小实现")
    }

    /// 登记产线绑定（引擎 op `bind_line`）。
    pub fn bind_line(&mut self) -> Result<(), ExeclineError> {
        unimplemented!("RED：bind_line 待最小实现")
    }

    /// 登记预算凭证（引擎 op `attach_voucher`）。
    pub fn attach_voucher(&mut self) -> Result<(), ExeclineError> {
        unimplemented!("RED：attach_voucher 待最小实现")
    }

    /// release 门（引擎 op `release`；必经引擎）。拒绝时返回类型化
    /// `EngineRejected`，携带 `Missing` 清单。
    pub fn release(&mut self) -> Result<(), ExeclineError> {
        unimplemented!("RED：release 待最小实现")
    }

    /// 开工（引擎 op `start`）。
    pub fn start(&mut self) -> Result<(), ExeclineError> {
        unimplemented!("RED：start 待最小实现")
    }

    /// 执行单元 + `attach_artifact`（引擎 op）。单元失败 = EX3001，
    /// 订单停在 InProgress（可重试；幂等键未落，重试不重复记账）。
    pub fn execute(&mut self, _unit: &dyn ExecuteUnit) -> Result<ExecOutput, ExeclineError> {
        unimplemented!("RED：execute 待最小实现")
    }

    /// 完工（引擎 op `complete`）。
    pub fn complete(&mut self) -> Result<(), ExeclineError> {
        unimplemented!("RED：complete 待最小实现")
    }

    /// close 门（引擎 op `close`；必经引擎）。无合格终态工件被引擎拒绝。
    pub fn close(&mut self) -> Result<(), ExeclineError> {
        unimplemented!("RED：close 待最小实现")
    }

    /// 全生命周期驱动（幂等）：同一 order_id 二次驱动只重放已完成 op。
    pub fn drive(&mut self, _unit: &dyn ExecuteUnit) -> Result<DriveReport, ExeclineError> {
        unimplemented!("RED：drive 待最小实现")
    }
}

impl fmt::Debug for OrderLifecycleDriver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OrderLifecycleDriver")
            .field("order_id", &self.cfg.order_id)
            .field("state", &self.order.state())
            .field("ledger", &self.ledger.path())
            .field("engine_bin", &self.client.bin())
            .finish_non_exhaustive()
    }
}
