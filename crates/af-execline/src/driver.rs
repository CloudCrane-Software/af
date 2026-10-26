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
use crate::mes::{MesFileWriter, MesSink, MesSyncReport};

/// 账本与事件文件名（out 目录布局）。
const LEDGER_FILE: &str = "ledger.jsonl";
/// MES 正本文件名（mes_ch 的 sink 同用此名）。
pub(crate) const MES_FILE: &str = "mes-events.jsonl";
const ARTIFACTS_DIR: &str = "artifacts";
/// run 目录根名（P5：attempt 目录在其下按布局分配）。
const RUNS_DIR: &str = "runs";

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
    /// ---- P4（方案 §2.2 ExecContext 扩展的驱动侧透传）----
    /// 执行宿主别名（缺省 anolis；换宿主=换 ctx）。
    pub host_alias: String,
    /// 题库根（单元从 `<data_root>/<task_id>.json` 装载任务包）。
    pub data_root: PathBuf,
    /// 目标题（swe-v = instance_id；换题=换 ctx）。
    pub task_id: String,
    /// 任务包计划面摘要钉死（红队 W2-C55：graph/report 预算帽全在
    /// plan.json——判据面钉死不覆盖计划面；门面 submit 时计算，凭证面
    /// 重放对账 + 单元执行前复核双门）。
    pub plan_sha256: Option<String>,
    /// 任务包判据面摘要钉死（红队 W1-C59①：判分真值源 fail_to_pass/
    /// pass_to_pass 全部来自任务包 JSON，此前无 digest 钉死——清空判据
    /// 即可让冻结判分链对零测试任务包返回 resolved=true）。门面 submit 时
    /// 计算，执行单元消费前核对，漂移即拒。None = 无期待（缺省，直调缝）。
    pub task_sha256: Option<String>,
    /// 试验标记（--trial 行口径，不进决策）。
    pub trial: bool,
    /// ---- P5（方案 §5-P5：attempt 目录由驱动层分配）----
    /// profile 名——attempt 目录命名的组成部分（`swev-<task>-<profile>-a<N>` /
    /// `tb-<task>-<profile>`）。与单元 spec 的 profile 双轨：组合根（P6 壳）
    /// 从同一 CLI 参数构造两者，保持同源（偏差登记 RESULT-P5.md）。
    pub profile: String,
    /// attempt 目录布局（套件差异；R8 覆写不对称如实保留）。
    pub attempt_layout: AttemptLayout,
}

/// attempt 目录布局（套件差异；方案 §5-P5 驱动层分配的命名策略）。
///
/// 两布局对照 af-bench 现状原样：
/// - swev 已 D-20 递增不覆写（main.rs:760-766 + 594-610 `next_attempt_index`）；
/// - TB 仍固定名 `remove_dir_all` 覆写（main.rs:1000-1002）——**R8 不对称如实
///   保留**：迁移不修（修了就不是迁移），P5 后独立决定是否对齐（方案 §6 R8）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptLayout {
    /// `swev-<task>-<profile>-a<N>`，N = 既有同前缀目录最大序号 +1，绝不删旧目录。
    SwevIncrement,
    /// `graph-<task>-<profile>-a<N>`，同 D-20 递增语义（图执行器；前缀区分
    /// 套件面，工件目录互不混置）。
    GraphIncrement,
    /// `tb-<task>-<profile>` 固定名，分配即 `remove_dir_all` 覆写（R8 保留）。
    TbOverwrite,
}

/// 驱动层 attempt 目录分配（方案 §5-P5：分配进驱动层，单元只收
/// `ctx.attempt_dir`；swev 单元的 run_id `-a<N>` 后缀即从该命名解析）。
/// 建目录是单元的事（swev/tb 单元各按 af-bench 原样 `create_dir_all`）。
pub fn allocate_attempt_dir(
    runs_root: &Path,
    task_id: &str,
    profile: &str,
    layout: &AttemptLayout,
) -> PathBuf {
    match layout {
        // D-20：既有同前缀目录最大序号 +1，绝不删除旧目录（main.rs:760-766）。
        AttemptLayout::SwevIncrement => {
            let attempt = crate::suite::next_attempt_index(runs_root, task_id, profile);
            runs_root.join(format!("swev-{task_id}-{profile}-a{attempt}"))
        }
        // 图执行器：同 D-20 递增语义，前缀 graph-（套件面区分）。
        AttemptLayout::GraphIncrement => {
            let attempt = crate::suite::next_attempt_index_with_prefix(
                runs_root,
                &format!("graph-{task_id}-{profile}-a"),
            );
            runs_root.join(format!("graph-{task_id}-{profile}-a{attempt}"))
        }
        // R8 不对称如实保留：固定名 + remove_dir_all 覆写（main.rs:1000-1002；
        // 删除失败按 af-bench 原样 `let _ =` 吞——建目录随后的 create_dir_all
        // 仍会因残件失败而显形，不静默改道）。
        AttemptLayout::TbOverwrite => {
            let dir = runs_root.join(format!("tb-{task_id}-{profile}"));
            let _ = std::fs::remove_dir_all(&dir);
            dir
        }
    }
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
    /// 驱动收尾的 MES 对账回执（B②③：文件 stub = no-op；CH sink = 读侧
    /// event_id 对账差量送批结果）。凭据零落（host:port only）。
    pub mes_sync: MesSyncReport,
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
    /// MES 写缝（B②③）：缺省文件正本；CH 下游经 [`Self::open_with_sink`] 注入。
    mes: Box<dyn MesSink>,
    /// 已成功完成的幂等键。
    done: BTreeSet<String>,
    /// 本次驱动累计的 MES 事件（`drive` 取出）。
    drive_mes: Vec<af_mes::MesEvent>,
    /// 本次驱动跳过的重放次数（`drive` 重置；用于报告 replayed）。
    replay_hits: usize,
    /// 运行实例判别量（红队 W2-C53 引入；W2-C-52 改源）：订单目录内持久
    /// 实例档案（`instance.id`，/dev/urandom 8 字节 16 hex，开单时铸造一次
    /// 随目录搬移）。event_id 原口径 = stable_id（order_id 纯函数）+ kind +
    /// ledger seq——同 order_id 跨 --state 根/跨产线复用即逐字节同 id，CH
    /// 侧按 event_id 去重的差量送批会互相吞掉对方记账事件（第二单
    /// shipped=0/already_in_ch=N 且零告警）。并入本判别量后跨根同 id 不再
    /// 相撞；W2-C-52 起判别量不再取路径哈希——canonicalize 只解 symlink
    /// 不解 bind-mount，mv/别名路径重放曾派生新实例、按新 id 补发正本已有
    /// 事件（同 (kind,seq) 双记进 CH 零告警）。
    instance: String,
}

/// 运行实例档案文件名（订单目录内，随目录搬移——实例身份不依赖可搬移路径，
/// 红队 W2-C-52）。
const INSTANCE_FILE: &str = "instance.id";

/// 载入或铸造运行实例判别量（红队 W2-C52）。
/// - 档案在位：读回并校验（16 位小写 hex，损坏 = EX2002）。
/// - 档案缺席且账本非空：实例身份不可知（目录被部分复制/档案被删）——
///   **拒绝打开**而非铸造新实例补发（补发 = 同 (kind,seq) 事件双记）。
/// - 档案缺席且账本为空（新单）：/dev/urandom 取 8 字节铸造，持久落盘
///   （随目录搬移，同根重放恒同 id）。
fn load_or_create_instance(out_dir: &Path, ledger_non_empty: bool) -> Result<String, ExeclineError> {
    use std::io::{Read, Write};
    use std::os::unix::fs::OpenOptionsExt;
    let path = out_dir.join(INSTANCE_FILE);
    if path.is_file() {
        let mut text = String::new();
        std::fs::File::open(&path)
            .and_then(|mut f| f.read_to_string(&mut text))
            .map_err(|err| ExeclineError::LedgerIo {
                detail: format!("实例档案读取失败 {}: {err}", path.display()),
            })?;
        let tag = text.trim();
        let well_formed = tag.len() == 16
            && tag
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if well_formed {
            return Ok(tag.to_string());
        }
        return Err(ExeclineError::LedgerCorrupt {
            detail: format!(
                "实例档案损坏: {} = {tag:?}（须 16 位小写 hex），拒绝打开",
                path.display()
            ),
        });
    }
    if ledger_non_empty {
        return Err(ExeclineError::LedgerCorrupt {
            detail: format!(
                "订单目录缺实例档案 {path} 且账本非空——运行实例身份不可知，拒绝打开\
                 （按路径派生新实例会把正本已有事件判「缺失」而补发，同 (kind,seq) 双记——\
                 红队 W2-C-52；恢复属运维面：确认目录来源后手工补写档案或弃用该副本）",
                path = path.display()
            ),
        });
    }
    let mut bytes = [0u8; 8];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|err| ExeclineError::LedgerIo {
            detail: format!("实例档案熵源不可用: {err}"),
        })?;
    let tag: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|err| ExeclineError::LedgerIo {
            detail: format!("实例档案落盘失败 {}: {err}", path.display()),
        })?;
    file.write_all(format!("{tag}\n").as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|err| ExeclineError::LedgerIo {
            detail: format!("实例档案写入失败 {}: {err}", path.display()),
        })?;
    Ok(tag)
}

impl OrderLifecycleDriver {
    /// 打开驱动（缺省 MES 文件正本 sink；冒烟/无 CH 下游形态）。
    /// `out_dir` 下已有 ledger 则重载恢复（身份不符 = EX2002）。
    pub fn open(
        cfg: DriverConfig,
        client: EngineClient,
        clock: Box<dyn Clock>,
    ) -> Result<Self, ExeclineError> {
        let mes_file = cfg.out_dir.join(MES_FILE);
        Self::open_with_sink(cfg, client, clock, Box::new(MesFileWriter::new(mes_file)))
    }

    /// 打开驱动并注入 MES 写缝（B②③ 正式缝：CH 下游 =
    /// [`crate::mes_ch::MesChSink`]，订单面只声明目标不做写侧）。
    pub fn open_with_sink(
        cfg: DriverConfig,
        client: EngineClient,
        clock: Box<dyn Clock>,
        mes: Box<dyn MesSink>,
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
        // 红队 W2-C-30：CLI/spec 声明的 line 与订单快照的**不可变绑定**不符
        // = 先拒绝打开（EX2002 账本身份不符族）——此前 bind_line 幂等跳过、
        // 订单保持首绑旧线，而 ExecContext/MES 事件取 cfg 新线，不一致静默
        // 入账（trace 工具只核 work_order_id 不核 line）。LineBinding 两字段
        // （line_id/line_version）整绑同比对；未绑新单（line=None）不核，
        // 由 bind_line 落 cfg 声明。
        if let Some(bound) = order.line() {
            if bound != &cfg.line {
                return Err(ExeclineError::LedgerCorrupt {
                    detail: format!(
                        "产线绑定不符: 订单账本绑定 line_id={} line_version={}，本次 CLI/spec 声明 \
                         line_id={} line_version={}（产线绑定随订单一次性入账、不可经重放换线——\
                         红队 W2-C-30）",
                        bound.line_id, bound.line_version, cfg.line.line_id, cfg.line.line_version
                    ),
                });
            }
        }
        let done = ledger
            .done_idem_keys()
            .into_iter()
            .map(str::to_string)
            .collect();
        // 运行实例判别量（红队 W2-C52）：持久实例档案，随目录搬移；档案缺席
        // 且账本非空 = fail-loud 拒绝（不铸造新实例补发）。
        let instance = load_or_create_instance(&cfg.out_dir, !ledger.records().is_empty())?;
        Ok(Self {
            instance,
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

    /// 订单快照绑定的产线 id（红队 W2-C-30）：ExecContext 与 MES 事件一律取
    /// **订单快照的不可变绑定**（与 graph_version 取 self.order 同源对称，
    /// cfg 由门面每次 submit 重读、重放期可能与首绑漂移）；未绑新单
    /// （bind_line 未落账）回落 cfg 声明。禁止 MES/记录行以 cfg 线冒充订单
    /// 实际绑定线。
    fn bound_line_id(&self) -> &str {
        self.order
            .line()
            .map(|binding| binding.line_id.as_str())
            .unwrap_or(&self.cfg.line.line_id)
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
        } else {
            self.emit_mes_if_missing("release", 1)?;
        }
        Ok(())
    }

    /// 开工（引擎 op `start`）。
    pub fn start(&mut self) -> Result<(), ExeclineError> {
        if self.engine_step("start", json!({}))? {
            self.emit_mes("start", 1)?;
        } else {
            self.emit_mes_if_missing("start", 1)?;
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
                verdict: None,
                record: None,
                evidence: Vec::new(),
            });
        }
        let artifacts_dir = self.cfg.out_dir.join(ARTIFACTS_DIR);
        std::fs::create_dir_all(&artifacts_dir)?;
        // P5 接线（方案 §5-P5）：attempt 目录由驱动层分配（swev D-20 递增 /
        // TB 固定名覆写按布局，R8 保留）；RecordSink 文件实现落本单 out_dir
        // （results.jsonl / records.jsonl 双轨，append 跨单续写）。重放路径
        // （上方）不触这两步——单元不重跑、行不重落。准备失败按单元失败
        // 入账（ok=false、幂等键不落，重试真跑）。
        let attempt_dir = allocate_attempt_dir(
            &self.cfg.out_dir.join(RUNS_DIR),
            &self.cfg.task_id,
            &self.cfg.profile,
            &self.cfg.attempt_layout,
        );
        let sink = match crate::record::FileRecordSink::open(&self.cfg.out_dir) {
            Ok(sink) => sink,
            Err(detail) => {
                let err = ExeclineError::UnitFailed { detail };
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
        // P4 扩展字段（方案 §2.2）：宿主/题库/题目/试验标记从 DriverConfig 透传。
        let ctx = ExecContext {
            order_id: &self.cfg.order_id,
            stable_id: self.order.stable_id().as_str(),
            // 红队 W2-C-30：line_id 与下方 graph_version 同源——订单快照的
            // 不可变绑定（此前取 cfg 旗标，重放换 --line 时与本单账面漂移）。
            line_id: self.bound_line_id(),
            out_dir: &artifacts_dir,
            task_id: &self.cfg.task_id,
            host_alias: &self.cfg.host_alias,
            data_root: &self.cfg.data_root,
            attempt_dir: &attempt_dir,
            record_sink: &sink,
            trial: self.cfg.trial,
            // 红队 W1-C32：WorkOrder 一次性绑定的 graph_version 进执行上下文
            // ——graph/report 单元装订时与 plan.compiled_from.graph_version
            // 核对（此前期待分支是死代码，「一次性绑定」有账无核）。
            // 红队 W2-C37：期待源取**订单快照的不可变绑定**
            // （self.order.graph_version()），不再取 cfg 值——cfg 由门面每次
            // submit 从当前 plan.json 重读，与之比对等于自指（换计划图版本
            // 重放时期待值随新计划同步变化、恒真）；订单快照值在账本恢复时
            // 保持首绑，换版重放即期待(旧) vs 计划(新) 不符而被拒。
            graph_version: Some(self.order.graph_version().as_str()),
            task_sha256: self.cfg.task_sha256.as_deref(),
            plan_sha256: self.cfg.plan_sha256.as_deref(),
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
        } else {
            self.emit_mes_if_missing("complete", 1)?;
        }
        Ok(())
    }

    /// close 门（引擎 op `close`；必经引擎）。无合格终态工件被引擎拒绝。
    pub fn close(&mut self) -> Result<(), ExeclineError> {
        if self.engine_step("close", json!({}))? {
            self.emit_mes("close", 1)?;
        } else {
            self.emit_mes_if_missing("close", 1)?;
        }
        Ok(())
    }

    /// 全生命周期驱动（幂等）：同一 order_id 二次驱动只重放已完成 op。
    ///
    /// 收尾对账 guarantee（MES-FAIL 修复，2026-10-25）：`mes.sync()` 无论
    /// 管线成败都执行——对账以**已发生事件**为准（close 被拒收/执行单元
    /// 失败时，release/start/complete 已发事件同样须对账入 CH，不留只存
    /// 在于正本的缺口）。管线失败时原错误优先返回；sync 自身再失败，原
    /// 错误不吞，sync 失败追加记账一行（失败真相由 ledger 承担）。
    pub fn drive(&mut self, unit: &dyn ExecuteUnit) -> Result<DriveReport, ExeclineError> {
        let seq_before = self.ledger.seq();
        self.drive_mes.clear();
        self.replay_hits = 0;
        let inner = self.drive_inner(unit);
        // 收尾对账面（B②③）——无条件执行：CH sink = retry_outbox + 读侧
        // event_id 对账 + 差量送批，以已发生事件为准；Parked = EX2003
        // fail-loud——失败时订单止于管线实际到达的状态（happy = Closed；
        // close 被拒 = Completed；单元失败 = InProgress），重跑按实际状态
        // 续走、读侧对账只补差量；文件 sink = no-op（冒烟路径行为不变）。
        let sync = self.mes.sync();
        match (inner, sync) {
            (Ok(exec_output), Ok(mes_sync)) => Ok(DriveReport {
                order: self.order.clone(),
                replayed: self.replay_hits > 0,
                appended: self.ledger.seq() - seq_before,
                mes_events: std::mem::take(&mut self.drive_mes),
                mes_sync,
                exec_output: Some(exec_output),
            }),
            // 管线失败 = 返回值；sync 若也失败只追加记账（原错误优先，
            // 不吞不覆盖）。追加本身失败（账本已在坏态）无从再记——
            // 原错误已携带返回，静默不掩盖任何新事实。
            (Err(orig), sync_result) => {
                if let Err(sync_err) = sync_result {
                    let _ = self.append_record(
                        "mes_sync",
                        self.idem_key("mes_sync"),
                        false,
                        Some(("mes_sync_failed", sync_err.to_string())),
                        None,
                    );
                }
                Err(orig)
            }
            // 管线成功而 sync 失败：与既有语义一致 fail-loud（EX2003）。
            (Ok(_), Err(sync_err)) => Err(sync_err),
        }
    }

    /// `drive` 的管线本体（create → … → close）。收尾对账由 [`Self::drive`]
    /// 的外层 guarantee 统一承担，本函数不触 sync——失败可在任一步上行。
    fn drive_inner(&mut self, unit: &dyn ExecuteUnit) -> Result<ExecOutput, ExeclineError> {
        self.create_order()?;
        self.bind_line()?;
        self.attach_voucher()?;
        self.release()?;
        self.start()?;
        let exec_output = self.execute(unit)?;
        self.complete()?;
        self.close()?;
        Ok(exec_output)
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

    /// 发一条 MES 事件（经写缝：文件正本必写，CH 下游在对账面统一）。
    /// event_id 用账本序号保唯一（首发 = 该 op 成功记录刚落账的序号）。
    /// 写失败 = 记账事故，向上传播不静默。
    fn emit_mes(&mut self, kind: &str, quantity: u64) -> Result<(), ExeclineError> {
        let seq = self.ledger.seq();
        self.emit_mes_seq(kind, quantity, seq)
    }

    fn emit_mes_seq(&mut self, kind: &str, quantity: u64, seq: u64) -> Result<(), ExeclineError> {
        let stable_id = self.order.stable_id().as_str().to_string();
        let event = af_mes::MesEvent::new(
            stable_id.clone(),
            self.clock.now_ms(),
            // 红队 W2-C53：instance 判别量殿后——同 order_id 跨 state 根/跨
            // 产线复用不再产生逐字节相同的 event_id（CH 差量按 id 去重，相撞
            // 即互吞记账事件且无告警）。
            format!("{stable_id}:{kind}:{seq}:{}", self.instance),
            // 红队 W2-C-30：事件线 = 订单快照不可变绑定（此前取 cfg 旗标，
            // 换线重放时 MES 印新线而账本订单绑旧线、静默入账）。
            self.bound_line_id().to_string(),
            self.cfg.station_id.clone(),
            kind,
            quantity,
        );
        self.mes.append(&event)?;
        self.drive_mes.push(event);
        Ok(())
    }

    /// 重放路径的 MES 对账补发（红队 W1-C35）：引擎 op 已成功（幂等键在
    /// 账本）但正本文件缺该 event_id——此前 emit_mes 写失败错误上抛、事件
    /// 两处皆无，重放 skip 即永久缺席 MES 账面（对账真值源恰是那份没写
    /// 进去的文件，sync 也救不回）。按 event_id 对账正本，缺则按**该 op
    /// 首次成功记录的序号**补发（重放期间 replay_skip 行会推高账本 seq，
    /// 不得用当前 seq 重构 id），幂等。
    ///
    /// 红队 W2-C11：对账按逻辑键（stable_id + kind + seq）兼容两种落盘
    /// 形态——W2-C53 引入 instance 判别量前的存量正本 event_id 是三段式
    /// `{stable_id}:{kind}:{seq}`，只按新四段式精确串匹配认不出旧 id，
    /// 重放任一修复前订单会对同单四跃迁各再补一条新 id，本地正本翻倍且
    /// 被 sync 当差量再送 CH（MergeTree 无去重）。新格式在场**或**旧三段
    /// 式在场，均视为已发。
    fn emit_mes_if_missing(&mut self, kind: &str, quantity: u64) -> Result<(), ExeclineError> {
        let Some(seq) = self
            .ledger
            .records()
            .iter()
            .rev()
            .find(|r| r.op == kind && r.ok)
            .map(|r| r.seq)
        else {
            return Ok(());
        };
        let stable_id = self.order.stable_id().as_str().to_string();
        let event_id = format!("{stable_id}:{kind}:{seq}:{}", self.instance);
        let legacy_event_id = format!("{stable_id}:{kind}:{seq}");
        if self.mes.has_event(&event_id)? || self.mes.has_event(&legacy_event_id)? {
            return Ok(());
        }
        self.emit_mes_seq(kind, quantity, seq)
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
