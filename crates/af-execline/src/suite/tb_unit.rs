//! TbExecuteUnit——Terminal-Bench 单题执行单元（af-bench 吸收方案 §5-P5，
//! `run_tb` 单题执行体的结构重组落点，main.rs:916-1109）。
//!
//! 与 swe-v 单元的**原样差异**（af-bench 两执行体本就不同，迁移不合并）：
//! - 无采样覆写（main.rs:628-631 注释明言"只作用于 swe-v 线；TB/竞赛套件
//!   不动"——endpoint 原样用 profile 基底）；
//! - bash 工具**无护栏**（反考古/重复命令拦截是 run_swev 的 main.rs:710-734，
//!   TB 注册处 main.rs:963-970 无此包装）；
//! - user_message 即裸 prompt（无 instance 纪律模板，main.rs:1015）；
//! - run_id 无 `-a<N>` 后缀（main.rs:1013，直接 task_id）；
//! - max_turns 40（swe-v 250）、单题预算 $1（swe-v $3）（main.rs:981/1021-1027）；
//! - 判分 = 容器链（提 /app → build 验证器 → test.sh → reward.txt，
//!   tb.rs:224-292），不是 swev-judge 子进程——判分三件套契约不适用；
//! - legacy 行 12 键（main.rs:1068-1085），无 v1 投影（方案 §1.4 映射表面向
//!   swe-v 18 键行；TB v1 化留 P6 对拍口径独立决定，登记 RESULT-P5.md）。
//!
//! 语句级对照清单（方案 §1.3）：
//! - 任务装载 ← main.rs:921（load_tb_tasks）；失败行分类归 setup_failed
//!   （af-bench 原为批级 ? 中止；单元化口径同 P4 RESULT §3-5）；
//! - setup ← main.rs:933（TbEnv::setup，P2 已挂 bridge）；
//! - setup_failed 行 ← main.rs:935-957（infra 不占分母，单题继续=单元不 Err 外
//!   部语义由订单侧承接：行落盘 + EX3001 可重试）；
//! - bash 工具注册 ← main.rs:960-970（无护栏）；
//! - AgentSpec ← main.rs:971-992（TB_SYSTEM_PROMPT + 40 步 + 无覆写 endpoint）；
//! - run（AgentRunner 缝）+ run_failed 行 ← main.rs:1009-1057（cleanup 保留）；
//! - agent_wall_ms ← main.rs:1059；
//! - verify ← main.rs:1060（TbEnv::verify 容器链）；
//! - 12 键行 ← main.rs:1068-1085；
//! - cleanup ← main.rs:1035/1061（RAII guard，§2.4 cleanup 必达）。

use std::sync::Arc;

use crate::error::ExeclineError;
use crate::exec::{ExecContext, ExecOutput, ExecuteUnit};
use crate::hostbridge::HostBridge;

/// TB 单元静态声明（方案 §2.2"换一档实验要改代码的东西进这里"）。
/// 缺省值 = af-bench run_tb 现值（main.rs:975/981-991/1021-1027）。
#[derive(Debug, Clone)]
pub struct TbUnitSpec {
    /// profile 名（→ EndpointSpec + key_cmd；**不做采样覆写**，main.rs:972/628-631）。
    pub profile: String,
    /// system 一行角色（main.rs:975 = [`crate::suite::tb::TB_SYSTEM_PROMPT`]）。
    pub system_prompt: &'static str,
    /// Policy：40 步（main.rs:981，TB 无 250 步对齐）。
    pub max_turns: u32,
    /// 完成信号词（main.rs:983）。
    pub completion_signal: &'static str,
    /// 连续无工具轮上限（main.rs:984）。
    pub tool_free_turn_limit: u32,
    /// 单题成本上限 $1（main.rs:1021）。
    pub spend_cap_cents: i64,
    /// 工具单次开销（main.rs:1022）。
    pub tool_spend_cents: i64,
    /// 预算账本总额（main.rs:1025-1026：BudgetLedger::new(100)）。
    pub budget_ledger_cents: i64,
    /// 单元版本（契约 §1.2 对齐）。
    pub unit_version: &'static str,
}

impl TbUnitSpec {
    /// fix2 代际缺省（值 = af-bench run_tb 现值）。
    pub fn fix2(profile: impl Into<String>) -> Self {
        Self {
            profile: profile.into(),
            system_prompt: crate::suite::tb::TB_SYSTEM_PROMPT,
            max_turns: 40,
            completion_signal: "TASK_COMPLETE",
            tool_free_turn_limit: 3,
            spend_cap_cents: 100,
            tool_spend_cents: 1,
            budget_ledger_cents: 100,
            unit_version: "0.1.0",
        }
    }
}

/// TB 单题执行单元：持两个可注入缝（§2.3，形态同 swev 单元——Arc 约束见
/// agent.rs 文档）。无判分指纹门（TB 判分不是 swev-judge）。
pub struct TbExecuteUnit {
    spec: TbUnitSpec,
    bridge: Arc<dyn HostBridge + Send + Sync>,
    runner: Arc<dyn crate::agent::AgentRunner>,
}

impl std::fmt::Debug for TbExecuteUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TbExecuteUnit")
            .field("unit_id", &self.unit_id())
            .field("profile", &self.spec.profile)
            .finish_non_exhaustive()
    }
}

impl TbExecuteUnit {
    /// 装载：profile 可解析即过（fail-loud，拒在起容器/调模型之前）。
    pub fn new(
        spec: TbUnitSpec,
        bridge: Arc<dyn HostBridge + Send + Sync>,
        runner: Arc<dyn crate::agent::AgentRunner>,
    ) -> Result<Self, ExeclineError> {
        crate::profile::profile(&spec.profile).map_err(|err| ExeclineError::UnitFailed {
            detail: format!("装载失败：{err}"),
        })?;
        Ok(Self {
            spec,
            bridge,
            runner,
        })
    }
}

impl ExecuteUnit for TbExecuteUnit {
    fn unit_id(&self) -> &str {
        "tb-unit"
    }

    fn execute(&self, ctx: &ExecContext<'_>) -> Result<ExecOutput, ExeclineError> {
        let _ = ctx;
        unimplemented!("P5 GREEN 落位：TbExecuteUnit::execute")
    }
}
