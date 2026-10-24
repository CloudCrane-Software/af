//! AgentRunner——模型执行缝（af-bench 吸收方案 §2.3 两个可注入缝之二，P4 落位）。
//!
//! 真实现 [`HarnessRunner`] = `Opened::open` + `Opened::run(Launch)`
//! （af-harness run.rs:215-236）+ key 紧邻现取与 401 重取器（af-bench
//! main.rs:768-773；KeyRefresher 类型 run.rs:85）。测试注入 FakeRunner 返回
//! 固定 RunReport——单测零模型调用、零凭证消耗（方案 §2.3）。
//!
//! 与 HostBridge（`&dyn` 注入）不同，本缝持 `Arc<dyn … + Send + Sync>`：
//! af-harness ToolRegistry 的工具函数体钉死 `'static + Send + Sync`
//! （run.rs:53-55），bash 工具闭包必须整体可注册——桥接引用无法带进注册闭包，
//! 缝的持有形态随之升为 Arc（偏差登记 RESULT-P4.md §3）。
//!
//! 失败路径保留 http_retries（af-bench main.rs:819/833：runner 失败行的
//! http_retries 仍取自 opened.http_retries()），故 Err 侧是结构化
//! [`AgentRunFailure`] 而非裸 String。

use std::path::Path;
use std::sync::Arc;

use af_harness::{AgentSpec, RunReport};

/// bash 工具体：命令文本 → 工具输出文本（单元侧护栏包装后的执行闭包；
/// af-bench main.rs:704-735 的语义在单元内，本缝只收成品）。
pub type BashFn = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// 缝输入：单元控制的全部模型执行材料（af-bench main.rs:736-798 的可搬运面）。
/// 凭证（root）与 harness 侧账面工单由真实现自持——那是订单门/harness 材料，
/// 不是单元参数（方案 §2.2 凭证纪律同款解读）。
pub struct AgentLaunch<'a> {
    /// run 目录（真实现 `Opened::open` 的落点，main.rs:773）。
    pub attempt_dir: &'a Path,
    /// run 标识（main.rs:778：`{instance_id}-a{attempt}`）。
    pub run_id: String,
    /// 首条 user 消息（main.rs:775：prompt + instance 模板）。
    pub user_message: String,
    /// 四元组规格（main.rs:736-759：采样覆写后的 endpoint + 提示词 + 工具声明 + policy）。
    pub spec: AgentSpec,
    /// bash 工具体（含反考古/重复命令护栏，main.rs:704-735）。
    pub bash: BashFn,
    /// 密钥解析命令（真实现紧邻 run 现取 + 401 重取；FakeRunner 零凭证消耗）。
    pub key_cmd: &'static str,
    /// 单题成本上限（美分，main.rs:787：swe-v 300 / TB 100）。
    pub spend_cap_cents: i64,
    /// 工具单次开销上限（main.rs:788）。
    pub tool_spend_cents: i64,
    /// 预算账本总额（main.rs:790：BudgetLedger::new(300)）。
    pub budget_ledger_cents: i64,
}

/// 成功侧产出：harness 报告 + HTTP 层退避重试计数（main.rs:800）。
pub struct AgentRunOutcome {
    pub report: RunReport,
    pub http_retries: u64,
}

/// 失败侧：错误文本（进 run_failed 行 verifier_note，main.rs:834 口径）
/// + 失败前累计的 http_retries（main.rs:819/833）。
pub struct AgentRunFailure {
    pub message: String,
    pub http_retries: u64,
}

/// 模型执行缝（方案 §2.3 原语义：`run(launch) → Result<RunReport, String>`，
/// Err 侧升结构化以保 http_retries）。
pub trait AgentRunner {
    fn run(&self, launch: AgentLaunch<'_>) -> Result<AgentRunOutcome, AgentRunFailure>;
}

/// 真实现：af-harness 包装器（main.rs:681-687 root + 768-798 key/Launch/registry
/// + opened.run 语句级对照，见 swev_unit 文档）。
///
/// 根凭证在 `run()` 内逐题签发——与 af-bench 同粒度（main.rs:681-686 在任务
/// 循环体内、每题一签；`issue_root` 每次新 KeyPair，文档明言跨重启不保根）。
/// `RootCredential` 类型未从 af-credential 导出，逐题签发也免去了持有它的
/// 类型命名问题（持有跨 run 的根须依赖 open/rotate 体系，无此需求）。
#[derive(Debug, Default)]
pub struct HarnessRunner;

impl HarnessRunner {
    /// 兼容构造（单元结构；保留命名构造面供调用方与 FakeRunner 对称使用）。
    pub fn new() -> Result<Self, String> {
        Ok(Self)
    }
}

impl AgentRunner for HarnessRunner {
    fn run(&self, launch: AgentLaunch<'_>) -> Result<AgentRunOutcome, AgentRunFailure> {
        let _ = launch;
        unimplemented!("P4 GREEN：Opened::open + opened.run(Launch) + key 紧邻现取/401 重取")
    }
}
