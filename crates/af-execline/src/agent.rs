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

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use af_harness::{AgentSpec, BudgetBinding, Launch, Opened, RunReport, ToolRegistry};
use af_meter::BudgetLedger;
use af_stop::StopPolicy;
use af_workorder::{GraphVersion, Priority, Sla, WorkOrder};
use serde_json::Value;

use crate::profile::resolve_key;

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
    /// 语句级对照（af-bench main.rs）：
    /// - 根凭证逐题签发 ← main.rs:681-686（issue_root：bash/bench/10000 cents）；
    /// - bash 工具注册（command 参数提取 ← main.rs:703-710；护栏在单元侧闭包）；
    /// - harness 侧账面工单 ← WorkOrder::new(run_id)（main.rs:687 经 bench_order
    ///   的 new 部分；四步生命周期按方案 §1.1 不搬，P5 由 driver 引擎 op 承接——
    ///   af-harness 只查 id 非空，run.rs:221-223）；
    /// - key 紧邻现取 + 401 重取器 ← main.rs:768-772（OAuth 会被 sidecar 轮换，
    ///   构建期可能几十分钟；长题 >1h 撞令牌寿命时端点 401 重跑 key_cmd 一次）；
    /// - Launch 组装 ← main.rs:776-798（vars 空/namespace bench/StopPolicy 缺省/
    ///   Breakpoint::None）；
    /// - http_retries 双路径累计 ← main.rs:800/819。
    fn run(&self, launch: AgentLaunch<'_>) -> Result<AgentRunOutcome, AgentRunFailure> {
        let root = af_credential::issue_root(&af_credential::RootSpec {
            tools: vec!["bash".to_string()],
            namespace: "bench".into(),
            budget_cents: 10_000,
        })
        .map_err(|err| AgentRunFailure {
            message: format!("issue root: {err}"),
            http_retries: 0,
        })?;
        // bash 工具体：单元已在闭包内做护栏（main.rs:710-734），此处只做
        // harness 胶水——command 参数提取与 Value 包装（main.rs:704-710/734）。
        let bash = launch.bash.clone();
        let mut registry = ToolRegistry::new();
        registry.register("bash", move |call| {
            let command = call
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            Ok(Value::String(bash(&command)))
        });
        // OAuth 对会被 sidecar 等并发方轮换：构建期可能几十分钟，key 必须紧邻
        // run 现取（401 竞态实证，main.rs:767）。
        let key = resolve_key(launch.key_cmd).map_err(|err| AgentRunFailure {
            message: err,
            http_retries: 0,
        })?;
        // 401 重取器：长题 >1h 撞 mcode 令牌寿命时，harness 端点 401 会重跑本
        // 命令换新 key 重试一次（main.rs:769-772）。
        let key_cmd = launch.key_cmd;
        let key_refresh: Option<af_harness::KeyRefresher> =
            Some(Arc::new(move || resolve_key(key_cmd)));
        let opened = Opened::open(launch.attempt_dir).map_err(|err| AgentRunFailure {
            message: format!("open run: {err}"),
            http_retries: 0,
        })?;
        let order = WorkOrder::new(
            launch.run_id.clone(),
            GraphVersion::new("bench"),
            Sla::new(None, Priority::Normal),
        );
        let result = opened.run(Launch {
            spec: launch.spec,
            run_id: launch.run_id,
            vars: BTreeMap::new(),
            user_message: launch.user_message,
            key: af_harness::EndpointKey::new(key),
            key_refresh,
            parent: &root.token,
            public_key: &root.keypair.public(),
            namespace: "bench".into(),
            spend_cap_cents: launch.spend_cap_cents,
            tool_spend_cents: launch.tool_spend_cents,
            budget: BudgetBinding::Cents {
                dimension: "llm.cents".into(),
                ledger: BudgetLedger::new(launch.budget_ledger_cents),
                spend_cap_cents: launch.spend_cap_cents,
            },
            order: &order,
            tools: registry,
            stop_policy: StopPolicy::default(),
            breakpoint: af_harness::Breakpoint::None,
        });
        match result {
            Ok(report) => Ok(AgentRunOutcome {
                report,
                http_retries: opened.http_retries(),
            }),
            Err(err) => Err(AgentRunFailure {
                message: err.to_string(),
                http_retries: opened.http_retries(),
            }),
        }
    }
}
