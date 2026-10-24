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
use serde_json::{json, Value};

/// note 截断（af-bench main.rs:666/834/889 的 `.chars().take(300)` 口径；
/// v1 契约 §1.2"截断至 ~300 字符"同源）。
fn clip300(note: &str) -> String {
    note.chars().take(300).collect()
}

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
    json!({
        "suite": facts.suite,
        "task_id": facts.task_id,
        "profile": facts.profile,
        "trial": facts.trial,
        "reward": facts.reward,
        "status": facts.status,
        // fix2（结果代号 fix2，审计 §6.A 对齐）：scaffold v3 = 提交式完成协议 +
        // instance 纪律 + 250 步/$3/温度0/top_p1/max_tokens 32000 + 无 gate 重试。
        // reasoning 档位按审计 §6.A.7 显式声明（保持 Off = mini 默认不发 thinking）。
        "scaffold": facts.scaffold,
        "reasoning_tier": facts.reasoning_tier,
        // D-20 项 1：实际 run 目录（重跑按序号递增，不再同名覆写）。
        "run_dir": facts.run_dir,
        // D-20 项 2：eval.log sha256（判分时计算，判分复核可逐字节对账）。
        "eval_sha256": facts.eval_sha256,
        "gate": facts.gate,
        // fix2：attempt 恒 1（审计 D5 关闭 gate 重试），字段保留供历史行兼容。
        "attempts": facts.attempts,
        "http_retries": facts.http_retries,
        "turns": facts.turns,
        "model_calls": facts.model_calls,
        "tokens": {
            "input": facts.tokens.input,
            "output": facts.tokens.output,
            "cache_read": facts.tokens.cache_read,
        },
        "agent_wall_ms": facts.agent_wall_ms,
        "verifier_note": clip300(&facts.verifier_note),
    })
}

/// `af.plan.record.v1` 投影（契约 §1.2 行形状 + 方案 §1.4 映射表）。
/// `plan_id/node_id` 来自订单上下文（order_id/line_id），`item_id` = task_id；
/// evidence_digest = eval_sha256（证据字节指纹，可空）。
pub fn swev_v1_row(
    facts: &ExecutionFacts,
    plan_id: &str,
    node_id: &str,
    host_alias: &str,
    unit_id: &str,
    unit_version: &str,
) -> Value {
    json!({
        "plan_id": plan_id,
        "node_id": node_id,
        "item_id": facts.task_id,
        "attempt": facts.attempts,
        "host_alias": host_alias,
        "unit_id": unit_id,
        "unit_version": unit_version,
        "status": facts.status,
        "verdict": facts.reward,
        "evidence": {
            "run_dir": facts.run_dir,
            "evidence_digest": facts.eval_sha256,
        },
        "gate": facts.gate,
        "usage": {
            "turns": facts.turns,
            "unit_calls": facts.model_calls,
            "http_retries": facts.http_retries,
            "quota": {
                "input": facts.tokens.input,
                "output": facts.tokens.output,
                "cache_read": facts.tokens.cache_read,
            },
        },
        "wall_ms": facts.agent_wall_ms,
        "note": clip300(&facts.verifier_note),
    })
}

/// setup_failed 行（af-bench main.rs:656-667：字段齐备含 null，D-20）。
pub fn swev_setup_failed_row(task_id: &str, profile: &str, trial: bool, note: &str) -> Value {
    json!({
        "suite": "swe-v",
        "task_id": task_id,
        "profile": profile,
        "trial": trial,
        "reward": null,
        "status": "setup_failed",
        // D-20：供给失败行也保持字段齐备（run 目录未建/eval 未跑 → null）。
        "run_dir": null,
        "eval_sha256": null,
        "verifier_note": clip300(note),
    })
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
    json!({
        "suite": "swe-v",
        "task_id": task_id,
        "profile": profile,
        "trial": trial,
        "reward": null,
        "status": "run_failed",
        "scaffold": "v3",
        // D-20 项 1：run_failed 行也记实际目录（trail 留在现场供复核）。
        "run_dir": run_dir.display().to_string(),
        "eval_sha256": null,
        "http_retries": http_retries,
        "verifier_note": clip300(note),
    })
}
