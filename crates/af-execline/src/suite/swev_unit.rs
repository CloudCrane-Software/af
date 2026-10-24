//! SwevExecuteUnit——swe-v 单题真实执行单元（af-bench 吸收方案 §5-P4，
//! `run_swev` 单题执行体的结构重组落点，方案 §1.1"结构重组"行）。
//!
//! 分相执行（§2.4 / main.rs:625-914 结构）：
//! `setup（任务装载+容器）→ run（AgentRunner 缝）→ gate（记账非门控）
//! → verify（P3 判分链）→ 行落盘 → cleanup（RAII guard，Drop → docker rm -f）`。
//!
//! 语句级对照清单（方案 §1.3，评审按此逐段比对）：
//! - 任务装载 ← main.rs:640（load_swev_tasks）+ task_json 路径 ← main.rs:850；
//! - setup ← main.rs:652（swev::setup，P2 已挂 bridge）；
//! - setup_failed 行 ← main.rs:654-677（字段齐备含 null）；
//! - bash 工具护栏 ← main.rs:700-735（exec + 反考古注入 + 重复命令 ≥3 拦截）；
//! - AgentSpec 组装 ← main.rs:736-759；采样覆写 ← main.rs:633-636；
//! - attempt 目录/新建不覆写 ← main.rs:698, 760-766（D-20：分配在调用方/驱动层）；
//! - run（AgentRunner 缝）+ http_retries ← main.rs:776-800；
//! - run_failed 行 ← main.rs:818-845（记实际目录 + cleanup）；
//! - agent_wall_ms ← main.rs:688, 849（run 相时长，不含 setup/verify）；
//! - gate 计算 ← main.rs:801-813（no_patch/test_edit，只记账）；
//! - verify ← main.rs:852-853（P3 swev::verify，指纹门+三件套+judge 子进程）；
//! - 18 键 legacy 行 + v1 投影 ← main.rs:861-895（双轨同源 ExecutionFacts）。
//!
//! 语义等价、机制升级两点（方案明文）：cleanup 由显式调用改 RAII guard
//! （§2.4 防新增早退路径漏清容器）；订单四步生命周期（bench_order）不搬——
//! P5 由 driver 引擎 op 承接。清理必达 + setup/run 失败行语义保留 + gate
//! 不接引擎 close 门，三条 §2.4 约束全部落位。

use std::sync::Arc;

use crate::error::ExeclineError;
use crate::exec::{ExecContext, ExecOutput, ExecuteUnit};
use crate::hostbridge::HostBridge;
use crate::judge::JudgeLocator;

/// swe-v 单元静态声明（方案 §2.2"换一档实验要改代码的东西进这里"，
/// 随 unit_version 版本化，对齐契约 §1.2 unit.json）。缺省值 = fix2 代际
/// af-bench 现值（§2.2 表列：采样覆写/两段提示词/Policy 250 步·$3/判分锚）。
#[derive(Debug, Clone)]
pub struct SwevUnitSpec {
    /// profile 名（→ EndpointSpec + key_cmd，P1 已迁 profile() 表）。
    pub profile: String,
    /// 采样覆写（main.rs:634-636）：温度（毫度）0。
    pub temperature_milli: u32,
    /// top_p（毫度）1.0 = 1000。
    pub top_p_milli: u32,
    /// max_tokens 32000（审计 D3：不吃协议默认）。
    pub max_tokens: u32,
    /// system 一行角色（main.rs:543）。
    pub system_prompt: &'static str,
    /// instance 纪律模板（main.rs:548-566）。
    pub instance_protocol: &'static str,
    /// Policy：1 turn = 1 模型调用，250 步（main.rs:748）。
    pub max_turns: u32,
    /// 完成信号词（main.rs:750）。
    pub completion_signal: &'static str,
    /// 连续无工具轮上限（main.rs:751）。
    pub tool_free_turn_limit: u32,
    /// 单题成本上限 $3（main.rs:787）。
    pub spend_cap_cents: i64,
    /// 工具单次开销（main.rs:788）。
    pub tool_spend_cents: i64,
    /// 结果代号（main.rs:871）。
    pub scaffold: &'static str,
    /// reasoning 档位显式声明（main.rs:872）。
    pub reasoning_tier: &'static str,
    /// 判分器定位与指纹锚（§3.2：装载期门 + verify 入口门双重防线）。
    pub judge: JudgeLocator,
    /// 单元版本（v1 行 unit_version；契约 §1.2 对齐）。
    pub unit_version: &'static str,
}

impl SwevUnitSpec {
    /// fix2 代际缺省（值 = af-bench main.rs 现值）。
    pub fn fix2(profile: impl Into<String>, judge: JudgeLocator) -> Self {
        Self {
            profile: profile.into(),
            temperature_milli: 0,
            top_p_milli: 1_000,
            max_tokens: 32_000,
            system_prompt: crate::suite::swev::SWEV_SYSTEM_PROMPT,
            instance_protocol: crate::suite::swev::SWEV_INSTANCE_PROTOCOL,
            max_turns: 250,
            completion_signal: "TASK_COMPLETE",
            tool_free_turn_limit: 3,
            spend_cap_cents: 300,
            tool_spend_cents: 1,
            scaffold: "v3",
            reasoning_tier: "off",
            judge,
            unit_version: "0.1.0",
        }
    }
}

/// swe-v 单题执行单元：持两个可注入缝构造（§2.3）——HostBridge（容器宿主）
/// 与 AgentRunner（模型执行）。Arc 形态见 agent.rs 文档（af-harness 'static
/// 约束）。
pub struct SwevExecuteUnit {
    #[allow(dead_code)] // RED 桩：GREEN 的 execute 消费三字段
    spec: SwevUnitSpec,
    #[allow(dead_code)]
    bridge: Arc<dyn HostBridge + Send + Sync>,
    #[allow(dead_code)]
    runner: Arc<dyn crate::agent::AgentRunner>,
}

impl std::fmt::Debug for SwevExecuteUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SwevExecuteUnit")
            .field("unit_id", &self.unit_id())
            .field("profile", &self.spec.profile)
            .finish_non_exhaustive()
    }
}

impl SwevExecuteUnit {
    /// 装载：判分指纹门（§3.2-2 双重防线的装载期一道，与 verify 入口门互备）
    /// + profile 可解析——都 fail-loud，拒在起容器/调模型之前。
    pub fn new(
        spec: SwevUnitSpec,
        bridge: Arc<dyn HostBridge + Send + Sync>,
        runner: Arc<dyn crate::agent::AgentRunner>,
    ) -> Result<Self, ExeclineError> {
        let _ = (&spec, &bridge, &runner);
        unimplemented!("P4 GREEN：装载期指纹门（EX3002 fail-loud）+ profile 可解析检查")
    }
}

impl ExecuteUnit for SwevExecuteUnit {
    fn unit_id(&self) -> &str {
        "swev-unit"
    }

    fn execute(&self, ctx: &ExecContext<'_>) -> Result<ExecOutput, ExeclineError> {
        let _ = ctx;
        unimplemented!("P4 GREEN：分相执行 setup→run→gate→verify→行落盘→cleanup(RAII)")
    }
}

/// bash 工具护栏包装（af-bench main.rs:710-734 原样语义）：
/// 反考古命中即在输出前注入护栏；重复命令计入最近 6 条窗口，第 3 次起拦截。
/// 抽为纯函数供单测与闭包共用（GREEN 落位）。
#[allow(dead_code)] // RED 桩：GREEN 的 bash 闭包消费
pub(crate) fn bash_guard_wrap(command: &str, out: String, recent: &mut Vec<String>) -> String {
    let _ = (command, out, recent);
    unimplemented!("P4 GREEN：反考古护栏注入 + 重复命令 ≥3 拦截（main.rs:710-734）")
}

/// 容器清理 RAII guard（§2.4"cleanup 必达"：Drop → tb.rs:294-296 的 docker rm -f，
/// 防新增早退路径漏清容器）。
#[allow(dead_code)] // RED 桩：GREEN 的 execute 构造
struct ContainerGuard {
    bridge: Arc<dyn HostBridge + Send + Sync>,
    env: Option<Arc<crate::suite::tb::TbEnv>>,
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        if let Some(env) = self.env.take() {
            env.cleanup(self.bridge.as_ref());
        }
    }
}
