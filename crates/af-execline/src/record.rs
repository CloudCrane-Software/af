//! 结果行双轨制（af-bench 吸收方案 §1.4 / §5-P4）。
//!
//! 两轨同源于同一份 [`ExecutionFacts`]，不各自装配（防漂移）：
//! - **legacy `results.jsonl` 行原样保留**（swe-v 18 键，af-bench main.rs:861-890）：
//!   现役消费方 bench-mes.py 按行字段取值、看板沿用在飞口径——切流前不破坏。
//!   写入纪律 append + 每行 flush（main.rs:644-648, 891-895）由 [`RecordSink`]
//!   承载（文件实现与驱动层接线 = P5，方案 §5-P5）。
//! - **`af.plan.record.v1` 投影新增**（契约 §1.2 行契约；映射表 = 方案 §1.4）：
//!   `suite→plan 上下文 / task_id→item_id / reward→verdict / status→status /
//!   gate→gate / turns+model_calls+tokens+http_retries→usage /
//!   agent_wall_ms→wall_ms / verifier_note→note`；v1 行落
//!   `{{output_dir}}/records.jsonl`（与 legacy 分流）。
//!
//! 供给失败行（setup_failed/run_failed，契约 §6.2 F2 分类）只落 legacy 轨——
//! 方案 §1.4 映射表面向完整执行事实，失败行无 v1 投影（登记 RESULT-P4.md）。

use std::path::Path;

use af_harness::TokenTotals;
use serde_json::Value;

/// 结果行写入器（append + flush 语义，main.rs:644-648）。
///
/// 两方法对应两条分流：legacy `results.jsonl` 与 `af.plan.record.v1`
/// `{{output_dir}}/records.jsonl`。文件实现（双文件 sink）与驱动层接线是 P5
/// （方案 §5-P5"RecordSink 实现"）；P4 直调单元由调用方注入。
pub trait RecordSink {
    /// 落一行 legacy results.jsonl（writeln + flush）。
    fn append_legacy(&self, row: &str) -> Result<(), String>;
    /// 落一行 af.plan.record.v1 records.jsonl（writeln + flush）。
    fn append_v1(&self, row: &str) -> Result<(), String>;
}

/// P4 驱动层占位 sink：驱动面（driver.execute）尚未接线落行——P5 落位 RecordSink
/// 文件实现与 attempt 分配（方案 §5-P5）。误用即 fail-loud（Err），不静默丢行。
pub struct PendingRecordSink;

impl RecordSink for PendingRecordSink {
    fn append_legacy(&self, _row: &str) -> Result<(), String> {
        Err("驱动层 record sink 未接线（P5 落位）：经驱动执行的单元暂不可落行".into())
    }

    fn append_v1(&self, _row: &str) -> Result<(), String> {
        Err("驱动层 record sink 未接线（P5 落位）：经驱动执行的单元暂不可落行".into())
    }
}

/// 单题执行事实——两轨行的同源（方案 §1.4"防漂移"）。
/// 字段 = swe-v 18 键 legacy 行（af-bench main.rs:861-890）的并集。
#[derive(Debug, Clone)]
pub struct ExecutionFacts {
    pub suite: &'static str,
    pub task_id: String,
    pub profile: String,
    pub trial: bool,
    /// 判定（resolved，只从 judge stdout JSON 取）；None → 行内 null。
    pub reward: Option<bool>,
    /// RunStatus 六态映射（suite::status_label）。
    pub status: String,
    pub scaffold: &'static str,
    pub reasoning_tier: &'static str,
    pub run_dir: Option<String>,
    /// eval.log sha256（判分时计算，D-20 项 2）。
    pub eval_sha256: Option<String>,
    /// 完成度记账（ok/no_patch/test_edit）——记账不是门控（方案 §2.4）。
    pub gate: String,
    /// fix2：attempt 恒 1（审计 D5 关闭 gate 重试），字段保留供历史行兼容。
    pub attempts: u32,
    pub http_retries: u64,
    pub turns: u32,
    pub model_calls: u64,
    pub tokens: TokenTotals,
    pub agent_wall_ms: u64,
    /// 判分 note（截断发生在行装配，同 af-bench main.rs:889）。
    pub verifier_note: String,
}

/// 18 键 swe-v legacy 行（af-bench main.rs:861-890 原样装配）。
pub fn swev_legacy_row(facts: &ExecutionFacts) -> Value {
    let _ = facts;
    unimplemented!("P4 GREEN：18 键 legacy 行装配（main.rs:861-890 原样）")
}

/// `af.plan.record.v1` 投影（契约 §1.2 行形状 + 方案 §1.4 映射表）。
/// `plan_id/node_id` 来自订单上下文（order_id/line_id），`item_id` = task_id。
pub fn swev_v1_row(
    facts: &ExecutionFacts,
    plan_id: &str,
    node_id: &str,
    host_alias: &str,
    unit_id: &str,
    unit_version: &str,
) -> Value {
    let _ = (
        facts, plan_id, node_id, host_alias, unit_id, unit_version,
    );
    unimplemented!("P4 GREEN：af.plan.record.v1 投影（契约 §1.2 + 方案 §1.4 映射表）")
}

/// setup_failed 行（af-bench main.rs:656-667：字段齐备含 null，D-20）。
pub fn swev_setup_failed_row(task_id: &str, profile: &str, trial: bool, note: &str) -> Value {
    let _ = (task_id, profile, trial, note);
    unimplemented!("P4 GREEN：setup_failed 行装配（main.rs:656-667 原样）")
}

/// run_failed 行（af-bench main.rs:822-834：记实际目录，trail 留在现场供复核）。
pub fn swev_run_failed_row(
    task_id: &str,
    profile: &str,
    trial: bool,
    run_dir: &Path,
    http_retries: u64,
    note: &str,
) -> Value {
    let _ = (task_id, profile, trial, run_dir, http_retries, note);
    unimplemented!("P4 GREEN：run_failed 行装配（main.rs:822-834 原样）")
}
