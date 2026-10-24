//! 订单生命周期驱动：构造 → 引擎门控 → 执行 → 收尾，全程 ledger + MES 记账。
//!
//! 驱动层不持有任何门控规则——"能不能做"全部问 af-engine 子进程；本层只负责
//! 编排顺序、记账与幂等。每个 op 带幂等键 `{stable_id}/{op}`（落地契约 v0
//! §6.2 at-least-once + 幂等键）：ledger 里已有成功记录的 op 不二次调用引擎，
//! 同一 order_id 二次驱动只重放，不产生重复终态。
//!
//! 拒绝路径全部类型化：引擎的 `GateError` / `DecisionError` 原样包进
//! [`ExeclineError::EngineRejected`]，不 panic、不降级成字符串。

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

use af_workorder::{
    BudgetVoucher, GraphVersion, LineBinding, OrderState, Sla, StableId, WorkOrder,
};
use serde_json::{json, Map, Value};

use crate::clock::Clock;
use crate::engine::{EngineClient, EngineOutcome, EngineRejection};
use crate::error::ExeclineError;
use crate::exec::{ExecContext, ExecOutput, ExecuteUnit};
use crate::ledger::{LedgerRecord, OrderLedger};
use crate::mes::MesFileWriter;

/// 账本与事件文件名（out 目录布局）。
const LEDGER_FILE: &str = "ledger.jsonl";
const MES_FILE: &str = "mes-events.jsonl";
const ARTIFACTS_DIR: &str = "artifacts";

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
    done: BTreeSet<String>,
    /// 本次驱动累计的 MES 事件（`drive` 取出）。
    drive_mes: Vec<af_mes::MesEvent>,
    /// 本次驱动跳过的重放次数（`drive` 重置；用于报告 replayed）。
    replay_hits: usize,
}

impl OrderLifecycleDriver {
    /// 打开驱动。`out_dir` 下已有 ledger 则重载恢复（身份不符 = EX2002）。
    pub fn open(
        cfg: DriverConfig,
        client: EngineClient,
        clock: Box<dyn Clock>,
    ) -> Result<Self, ExeclineError> {
        std::fs::create_dir_all(&cfg.out_dir)?;
        let ledger = OrderLedger::open(&cfg.out_dir.join(LEDGER_FILE))?;
        // 账本身份必须与本单一致：异单账本不得冒充（EX2002）。
        if let Some(first) = ledger.records().first() {
            let expected = StableId::generate(&cfg.order_id);
            if first.stable_id != expected.as_str() {
                return Err(ExeclineError::LedgerCorrupt {
                    detail: format!(
                        "账本身份不符: 账本 stable_id={}，本单 {} 派生 {}，拒绝打开",
                        first.stable_id, cfg.order_id, expected
                    ),
                });
            }
        }
        // 恢复订单：取账本最近一条快照；空账本按确定性规则新建
        // （stable_id 只由 order_id 决定，重建与首驱逐字相同）。
        let order = ledger.current_order().cloned().unwrap_or_else(|| {
            WorkOrder::new(
                cfg.order_id.clone(),
                GraphVersion::new(cfg.graph_version.clone()),
                cfg.sla.clone(),
            )
        });
        let done = ledger
            .done_idem_keys()
            .into_iter()
            .map(str::to_string)
            .collect();
        let mes = MesFileWriter::new(cfg.out_dir.join(MES_FILE));
        Ok(Self {
            cfg,
            client,
            clock,
            order,
            ledger,
            mes,
            done,
            drive_mes: Vec::new(),
            replay_hits: 0,
        })
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
        let idem = self.idem_key("create_order");
        if self.done.contains(&idem) {
            return self.replay_skip(idem);
        }
        self.order = WorkOrder::new(
            self.cfg.order_id.clone(),
            GraphVersion::new(self.cfg.graph_version.clone()),
            self.cfg.sla.clone(),
        );
        self.append_record(
            "create_order",
            idem.clone(),
            true,
            None,
            Some(self.order.clone()),
        )?;
        self.done.insert(idem);
        Ok(())
    }

    /// 登记产线绑定（引擎 op `bind_line`）。
    pub fn bind_line(&mut self) -> Result<(), ExeclineError> {
        self.engine_step("bind_line", json!({ "line": self.cfg.line }))?;
        Ok(())
    }

    /// 登记预算凭证（引擎 op `attach_voucher`）。
    pub fn attach_voucher(&mut self) -> Result<(), ExeclineError> {
        self.engine_step("attach_voucher", json!({ "voucher": self.cfg.voucher }))?;
        Ok(())
    }

    /// release 门（引擎 op `release`；必经引擎）。拒绝时返回类型化
    /// `EngineRejected`，携带 `Missing` 清单。
    pub fn release(&mut self) -> Result<(), ExeclineError> {
        if self.engine_step("release", json!({}))? {
            self.emit_mes("release", 1)?;
        }
        Ok(())
    }

    /// 开工（引擎 op `start`）。
    pub fn start(&mut self) -> Result<(), ExeclineError> {
        if self.engine_step("start", json!({}))? {
            self.emit_mes("start", 1)?;
        }
        Ok(())
    }

    /// 执行单元 + `attach_artifact`（引擎 op）。单元失败 = EX3001，
    /// 订单停在 InProgress（可重试；幂等键未落，重试不重复记账）。
    pub fn execute(&mut self, unit: &dyn ExecuteUnit) -> Result<ExecOutput, ExeclineError> {
        let idem = self.idem_key("attach_artifact");
        if self.done.contains(&idem) {
            self.replay_skip(idem)?;
            return Ok(ExecOutput {
                artifacts: self.order.artifacts().to_vec(),
                outputs: Vec::new(),
                note: "replay: 工件已登记，未重跑执行单元".to_string(),
            });
        }
        let artifacts_dir = self.cfg.out_dir.join(ARTIFACTS_DIR);
        std::fs::create_dir_all(&artifacts_dir)?;
        let ctx = ExecContext {
            order_id: &self.cfg.order_id,
            stable_id: self.order.stable_id().as_str(),
            line_id: &self.cfg.line.line_id,
            out_dir: &artifacts_dir,
        };
        let output = match unit.execute(&ctx) {
            Ok(output) => output,
            Err(err) => {
                // 执行失败入账（ok=false），幂等键不落——重试会真跑
                self.append_record(
                    "attach_artifact",
                    idem,
                    false,
                    Some(("unit_failed", err.to_string())),
                    None,
                )?;
                return Err(err);
            }
        };
        // 协议一次一个工件：逐个过引擎登记。任一被拒都入账并向上传播。
        self.attach_artifacts(&output.artifacts, &idem)?;
        self.append_record(
            "attach_artifact",
            idem.clone(),
            true,
            None,
            Some(self.order.clone()),
        )?;
        self.done.insert(idem);
        Ok(output)
    }

    /// 完工（引擎 op `complete`）。
    pub fn complete(&mut self) -> Result<(), ExeclineError> {
        if self.engine_step("complete", json!({}))? {
            self.emit_mes("complete", 1)?;
        }
        Ok(())
    }

    /// close 门（引擎 op `close`；必经引擎）。无合格终态工件被引擎拒绝。
    pub fn close(&mut self) -> Result<(), ExeclineError> {
        if self.engine_step("close", json!({}))? {
            self.emit_mes("close", 1)?;
        }
        Ok(())
    }

    /// 全生命周期驱动（幂等）：同一 order_id 二次驱动只重放已完成 op。
    pub fn drive(&mut self, unit: &dyn ExecuteUnit) -> Result<DriveReport, ExeclineError> {
        let seq_before = self.ledger.seq();
        self.drive_mes.clear();
        self.replay_hits = 0;
        self.create_order()?;
        self.bind_line()?;
        self.attach_voucher()?;
        self.release()?;
        self.start()?;
        let exec_output = self.execute(unit)?;
        self.complete()?;
        self.close()?;
        Ok(DriveReport {
            order: self.order.clone(),
            replayed: self.replay_hits > 0,
            appended: self.ledger.seq() - seq_before,
            mes_events: std::mem::take(&mut self.drive_mes),
            exec_output: Some(exec_output),
        })
    }

    // ---------- 内部 ----------

    /// 幂等键：`{stable_id}/{op}`。stable_id 只由 order_id 决定，
    /// 因此同单重放得到同键；换单换键。
    fn idem_key(&self, op: &str) -> String {
        format!("{}/{op}", self.order.stable_id().as_str())
    }

    /// 一个引擎 op：幂等检查 → 子进程调用 → 记账。
    /// 返回 `true` = 本次真调了引擎（调用方据此发 MES）；`false` = 重放跳过。
    fn engine_step(&mut self, op: &str, params: Value) -> Result<bool, ExeclineError> {
        let idem = self.idem_key(op);
        if self.done.contains(&idem) {
            self.replay_skip(idem)?;
            return Ok(false);
        }
        let request = engine_request(&self.order, op, params)?;
        match self.client.call(&request)? {
            EngineOutcome::Accept(accept) => {
                self.order = accept.order;
                self.append_record(op, idem.clone(), true, None, Some(self.order.clone()))?;
                self.done.insert(idem);
                Ok(true)
            }
            EngineOutcome::Reject(rejection) => {
                let (kind, detail) = rejection_kind_and_detail(&rejection);
                self.append_record(op, idem, false, Some((kind, detail)), None)?;
                Err(ExeclineError::EngineRejected {
                    op: op.to_string(),
                    rejection,
                })
            }
        }
    }

    /// 重放审计行：不调引擎、不重复终态，只记"这步已做过"。
    fn replay_skip(&mut self, idem: String) -> Result<(), ExeclineError> {
        self.replay_hits += 1;
        self.append_record("replay_skip", idem, true, None, Some(self.order.clone()))
    }

    /// 逐个工件过引擎登记（协议一次一个工件）。任一被拒：入账 ok=false 并
    /// 返回类型化错误；ok=true 行与幂等键落账由调用方（`execute`）负责。
    fn attach_artifacts(
        &mut self,
        artifacts: &[af_workorder::WorkArtifact],
        idem: &str,
    ) -> Result<(), ExeclineError> {
        for artifact in artifacts {
            let request = engine_request(
                &self.order,
                "attach_artifact",
                json!({ "artifact": artifact }),
            )?;
            match self.client.call(&request)? {
                EngineOutcome::Accept(accept) => self.order = accept.order,
                EngineOutcome::Reject(rejection) => {
                    let (kind, detail) = rejection_kind_and_detail(&rejection);
                    self.append_record(
                        "attach_artifact",
                        idem.to_string(),
                        false,
                        Some((kind, detail)),
                        None,
                    )?;
                    return Err(ExeclineError::EngineRejected {
                        op: "attach_artifact".to_string(),
                        rejection,
                    });
                }
            }
        }
        Ok(())
    }

    /// 追加一行账本。`state_after` 取当前快照状态；拒绝行不带快照。
    fn append_record(
        &mut self,
        op: &str,
        idem_key: String,
        ok: bool,
        error: Option<(&str, String)>,
        order_after: Option<WorkOrder>,
    ) -> Result<(), ExeclineError> {
        let (error_kind, error_detail) = match error {
            Some((kind, detail)) => (Some(kind.to_string()), Some(detail)),
            None => (None, None),
        };
        let record = LedgerRecord {
            seq: 0,
            ts_ms: self.clock.now_ms(),
            order_id: self.cfg.order_id.clone(),
            stable_id: self.order.stable_id().as_str().to_string(),
            op: op.to_string(),
            idem_key,
            ok,
            state_after: Some(state_name(self.order.state())),
            error_kind,
            error_detail,
            order_after,
        };
        self.ledger.append(record)?;
        Ok(())
    }

    /// 发一条 MES 事件（stub writer 落盘）。event_id 用账本序号保唯一。
    /// 落盘失败 = 记账事故，向上传播不静默。
    fn emit_mes(&mut self, kind: &str, quantity: u64) -> Result<(), ExeclineError> {
        let stable_id = self.order.stable_id().as_str().to_string();
        let seq = self.ledger.seq();
        let event = af_mes::MesEvent::new(
            stable_id.clone(),
            self.clock.now_ms(),
            format!("{stable_id}:{kind}:{seq}"),
            self.cfg.line.line_id.clone(),
            self.cfg.station_id.clone(),
            kind,
            quantity,
        );
        self.mes.append(&event)?;
        self.drive_mes.push(event);
        Ok(())
    }
}

/// 组一行引擎请求：op + 整单快照 + 该 op 的参数。
fn engine_request(order: &WorkOrder, op: &str, params: Value) -> Result<Value, ExeclineError> {
    let order_value =
        serde_json::to_value(order).map_err(|err| ExeclineError::EngineBadResponse {
            detail: format!("订单快照序列化失败: {err}"),
        })?;
    let mut request = Map::new();
    request.insert("op".to_string(), json!(op));
    request.insert("order".to_string(), order_value);
    if let Value::Object(extra) = params {
        request.extend(extra);
    }
    Ok(Value::Object(request))
}

/// 拒绝的分类标签与可读详情（入账用；类型化拒绝仍由 EngineRejection 承载）。
fn rejection_kind_and_detail(rejection: &EngineRejection) -> (&'static str, String) {
    match rejection {
        EngineRejection::BadRequest { .. } => ("bad_request", rejection.to_string()),
        EngineRejection::RejectionEnvelope { .. } => ("rejection_envelope", rejection.to_string()),
        EngineRejection::Gate { .. } => ("gate", rejection.to_string()),
        EngineRejection::Decision { .. } => ("decision", rejection.to_string()),
    }
}

/// 状态名的线格式（OrderState 的 serde 名：Created/Released/...）。
fn state_name(state: OrderState) -> String {
    serde_json::to_value(state)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{state:?}"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use af_workorder::{OrderState::*, Priority};

    fn order() -> WorkOrder {
        WorkOrder::new(
            "wo-unit",
            GraphVersion::new("g-1"),
            Sla::new(None, Priority::Normal),
        )
    }

    /// 请求线形状：op + 整单快照 + 该 op 参数合并；快照可解析回同身份订单。
    #[test]
    fn engine_request_merges_op_order_and_params() {
        let request = engine_request(
            &order(),
            "bind_line",
            json!({ "line": { "line_id": "l", "line_version": "v" } }),
        )
        .expect("组请求");
        let obj = request.as_object().expect("请求是对象");
        assert_eq!(obj["op"], json!("bind_line"));
        assert!(obj.contains_key("line"), "op 参数应合并进顶层");
        assert!(!obj.contains_key("voucher"), "未传的参数不得出现");
        let decoded: WorkOrder = serde_json::from_value(obj["order"].clone()).expect("快照可解析");
        assert_eq!(decoded.stable_id(), order().stable_id());
        assert_eq!(decoded.state(), Created);
    }

    /// 无参数的 op（release/start/complete/close）只带 op + order。
    #[test]
    fn engine_request_without_params_is_minimal() {
        let request = engine_request(&order(), "release", json!({})).expect("组请求");
        let obj = request.as_object().expect("请求是对象");
        assert_eq!(obj.len(), 2, "只应有 op 与 order 两个键");
        assert_eq!(obj["op"], json!("release"));
    }

    /// 状态名用 OrderState 的 serde 名（ledger state_after 的线格式）。
    #[test]
    fn state_name_matches_serde_names() {
        assert_eq!(state_name(Created), "Created");
        assert_eq!(state_name(Released), "Released");
        assert_eq!(state_name(InProgress), "InProgress");
        assert_eq!(state_name(Completed), "Completed");
        assert_eq!(state_name(Closed), "Closed");
        assert_eq!(state_name(Cancelled), "Cancelled");
    }

    /// 拒绝的入账标签：gate/bad_request/...，与 EngineRejection 变体对应。
    #[test]
    fn rejection_labels_match_kinds() {
        let gate = EngineRejection::Gate {
            gate: crate::engine::GateRejection::PreconditionsNotMet {
                transition: af_workorder::Transition::Release,
                missing: vec![af_workorder::Missing::BudgetVoucherNotIssued],
            },
        };
        let (kind, detail) = rejection_kind_and_detail(&gate);
        assert_eq!(kind, "gate");
        assert!(detail.contains("BudgetVoucherNotIssued"), "{detail}");

        let (kind, _) = rejection_kind_and_detail(&EngineRejection::BadRequest {
            message: "缺少字段 op".into(),
        });
        assert_eq!(kind, "bad_request");
    }
}
