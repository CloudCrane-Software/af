//! 影子对拍（af-bench 吸收方案 §4 / §5-P6）——同题新旧两路的一致性工具面。
//!
//! 三层组织（§4.1，成本递增、判据分级）：
//! - **L0 判分回放**（零模型、零容器）：既有 run 证据（判分三件套）在新旧两路
//!   判分调用点各自复判——旧路 = af-bench `swev.rs:205-210` 的硬编码直调
//!   （[`old_judge_call`]，语句逐字复制），新路 = [`crate::judge::invoke`]
//!   （locator 参数化）。判据：resolved/log_parsed（乃至 judge stdout 字节）
//!   逐位一致 100%。
//! - **L1 取证-判分链**（FakeRunner，零真容器/零模型）：新旧两路各自走
//!   `capture_diff → eval 执行 → judge`，比对取证字节/判定/note/指纹/结果行。
//!   编排于 tests/p6_shadow.rs（本模块供可复用的比较件）。
//! - **L2 端到端金丝雀**（真容器+真模型）：不在 P6 执行（编排者裁决）——本模块
//!   备其**入口与配置面**（[`L2Canary`]：新旧两路命令行 + §4.3 前置清单），
//!   #[ignore] 用例在 tests/p6_shadow.rs 在册。
//!
//! shadow-diff 三维度（§4.2）：结果行字段（确定性逐位/计量统计，[`diff_legacy_rows`]）/
//! 判分一致性（L0/L1 逐位））/ MES 事件序列（映射表比对，[`mes_mapping_table`] +
//! [`legacy_family_from_row`]——不做事件字节对拍，event_id 含账本序号必然不同）。
//!
//! 旧路活源基准 = tests/fixtures/p6-frozen-bench/（af-bench P5 完成点 1830371
//! 三文件的字节级冻结件——薄壳化后 af-bench 本体不再承载这些载荷）。

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::judge::JudgeLocator;

// ---- L0：判分回放 ----

/// 一份既有 run 证据（判分三件套；task_json 由题库根补齐，§4.1-L0）。
#[derive(Debug, Clone)]
pub struct L0Evidence {
    pub task_id: String,
    pub task_json: PathBuf,
    pub eval_log: PathBuf,
    pub agent_diff: PathBuf,
}

impl L0Evidence {
    /// 从历史 run 目录组装（eval.log/agent.diff 在 run 目录，task_json 在题库根）。
    pub fn from_run_dir(task_id: &str, data_root: &Path, run_dir: &Path) -> Self {
        let _ = (task_id, data_root, run_dir);
        unimplemented!("P6：L0Evidence::from_run_dir")
    }
}

/// 旧路判分调用（← af-bench swev.rs:205-210 语句逐字复制：硬编码绝对路径直
/// `Command`，无 locator、无 env 注入、不经 wrapper shell）。源文本与冻结件
/// 双向钉死（tests/p6_shadow.rs cmp_p6_*）。
pub fn old_judge_call(
    task_json: &Path,
    log: &Path,
    diff: &Path,
) -> std::io::Result<std::process::Output> {
    let _ = (task_json, log, diff);
    unimplemented!("P6：old_judge_call")
}

/// 一次判分调用的新旧两侧解读（判定值只从 judge stdout JSON 取，§3.2-3）。
#[derive(Debug, Clone)]
pub struct JudgeVerdict {
    /// judge 子进程是否成功退出（退出码 0）。
    pub exit_ok: bool,
    pub resolved: Option<bool>,
    pub log_parsed: Option<bool>,
    /// stdout 原文（逐位比对的载体）。
    pub stdout: String,
    /// 失败侧文案（exit_ok=false 时非空）。
    pub note: String,
}

/// 新路判分调用：`judge::invoke`（locator）+ stdout JSON 解读（与
/// `suite::swev::verify` 的读取面同源：resolved/log_parsed 两键）。
pub fn new_judge_call(locator: &JudgeLocator, ev: &L0Evidence) -> JudgeVerdict {
    let _ = (locator, ev);
    unimplemented!("P6：new_judge_call")
}

/// L0 单证据对拍结果。
#[derive(Debug, Clone)]
pub struct L0Pair {
    pub task_id: String,
    pub old: JudgeVerdict,
    pub new: JudgeVerdict,
}

impl L0Pair {
    pub fn resolved_agree(&self) -> bool {
        let _ = &self.old;
        unimplemented!("P6：L0Pair::resolved_agree")
    }
    pub fn log_parsed_agree(&self) -> bool {
        let _ = &self.old;
        unimplemented!("P6：L0Pair::log_parsed_agree")
    }
    pub fn stdout_agree(&self) -> bool {
        let _ = &self.old;
        unimplemented!("P6：L0Pair::stdout_agree")
    }
    /// 三项全齐（判据：逐位一致）。
    pub fn agree(&self) -> bool {
        let _ = &self.old;
        unimplemented!("P6：L0Pair::agree")
    }
}

/// L0 批量回放：每份证据两路各判一次并配对（不中断——逐证据出结果）。
pub fn l0_replay(evidences: &[L0Evidence], locator: &JudgeLocator) -> Vec<L0Pair> {
    let _ = (evidences, locator);
    unimplemented!("P6：l0_replay")
}

// ---- shadow-diff 维度一：结果行字段（§4.2-1）----

/// 确定性字段：逐位比对，差异即失配。
pub const DETERMINISTIC_ROW_FIELDS: &[&str] = &[
    "suite",
    "task_id",
    "profile",
    "trial",
    "reward",
    "status",
    "gate",
    "scaffold",
    "reasoning_tier",
    "attempts",
];
/// 计量字段：统计比对不逐位（供给面抖动豁免，http_retries 差异如实记录）。
pub const METERING_ROW_FIELDS: &[&str] = &[
    "http_retries",
    "turns",
    "model_calls",
    "agent_wall_ms",
    "tokens",
];

/// 一对 legacy 行的字段级差异表。
#[derive(Debug, Clone, Default)]
pub struct RowDiff {
    /// 确定性字段失配清单（`字段: old≠new`）。
    pub deterministic_diffs: Vec<String>,
    /// 计量字段差异清单（记录不判死）。
    pub metering_diffs: Vec<String>,
    /// 键集差：new 行多出/缺失的键（新行订单字段差集单独核对，§4.2-1）。
    pub extra_keys_in_new: Vec<String>,
    pub missing_keys_in_new: Vec<String>,
    /// 行全等（Value 级）。
    pub rows_equal: bool,
}

impl RowDiff {
    /// 判据通过 = 确定性字段零失配 + 键集不缺。
    pub fn passes(&self) -> bool {
        let _: () = unimplemented!("P6：RowDiff::passes");
    }
}

/// 比对一对 legacy results 行（18 键 swe-v 口径；键序无关——serde_json 序列化
/// 两侧同序，字节一致由行序列化断言另证）。
pub fn diff_legacy_rows(old: &Value, new: &Value) -> RowDiff {
    let _ = (old, new);
    unimplemented!("P6：diff_legacy_rows")
}

/// 比对两份 results.jsonl（逐行配对；行数不等 = 全部行进 missing 报告）。
pub fn diff_results_files(old_path: &Path, new_path: &Path) -> Result<Vec<RowDiff>, String> {
    let _ = (old_path, new_path);
    unimplemented!("P6：diff_results_files")
}

// ---- shadow-diff 维度三：MES 事件序列映射表（§4.2-3 / R6）----

/// 旧路事件族 ←→ 新路对应物（映射表口径：不做事件字节对拍，event_id 含账本
/// 序号必然不同；判"映射后 kind 集合齐备 + 每类恰 1 条 + work_order_id 收敛"）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MesMappingRow {
    pub legacy_event: &'static str,
    pub new_counterpart: &'static str,
    pub source: &'static str,
}

/// 映射表：started↔start、done↔complete、reward→单元 reward（行 reward）、
/// turns/tokens_*/wall_ms→单元 usage（行计量字段）。
pub fn mes_mapping_table() -> Vec<MesMappingRow> {
    unimplemented!("P6：mes_mapping_table")
}

/// 旧路事件族从 legacy 行推导（bench-mes.py 视角：读行字段取值，
/// tools/bench-mes.py:9, 98-131 的事件族 = 行字段的投影）。
#[derive(Debug, Clone, Default)]
pub struct LegacyEventFamily {
    pub started: bool,
    pub done: bool,
    pub reward: Option<bool>,
    pub tokens_input: Option<u64>,
    pub tokens_output: Option<u64>,
    pub cache_read: Option<u64>,
    pub turns: Option<u32>,
    pub wall_ms: Option<u64>,
}

pub fn legacy_family_from_row(row: &Value) -> LegacyEventFamily {
    let _ = row;
    unimplemented!("P6：legacy_family_from_row")
}

/// 新路 MES 事件（driver 四类 + 单元面）对 legacy 行的映射检查：
/// start/complete（started↔start、done↔complete）各恰 1 条、全部事件
/// work_order_id 收敛同一值、行内 reward/计量字段可被映射表覆盖。
/// 返回问题清单（空 = 通过）。
pub fn check_mes_against_row(events: &[af_mes::MesEvent], row: &Value) -> Vec<String> {
    let _ = (events, row);
    unimplemented!("P6：check_mes_against_row")
}

// ---- L2：端到端金丝雀钩子（§4.1-L2 / §4.3；本包不执行）----

/// 冻结旧路二进制指纹前 16 位（盘点 §② 与方案 §0 实测口径）。
pub const OLD_BIN_SHA256_PREFIX: &str = "7a87876c0a5541c0";

/// L2 金丝雀配置面：新旧两路命令行 + 前置清单。真容器/真模型执行属后续
/// F-首单任务包（编排者裁决）；本结构是入口与配置的就绪件。
#[derive(Debug, Clone)]
pub struct L2Canary {
    /// 旧路二进制（冻结 af-bench；env `AF_SHADOW_OLD_BIN`，缺省盘点在位值）。
    pub old_bin: PathBuf,
    /// 新路二进制（薄壳 af-bench；env `AF_SHADOW_NEW_BIN`，缺省 workspace 构建值）。
    pub new_bin: PathBuf,
    /// 题库根（env `AF_SHADOW_DATA_ROOT`，缺省历史战役值 /tmp/swev-s1）。
    pub data_root: PathBuf,
    /// 题集（env `AF_SHADOW_TASKS` 逗号表；§4.1-L2 建议确认集快标 3 + N 档 7）。
    pub tasks: Vec<String>,
    /// 档案（env `AF_SHADOW_PROFILE`，缺省 glm-cp）。
    pub profile: String,
    /// 宿主（env `AF_SHADOW_HOST`，缺省 local——R3 分宿主防互杀）。
    pub host: String,
    /// 旧路输出根。
    pub out_old: PathBuf,
    /// 新路输出根。
    pub out_new: PathBuf,
}

impl L2Canary {
    /// 从环境组装（缺省值 = 本机现产口径；全部可覆盖）。
    pub fn from_env() -> Self {
        unimplemented!("P6：L2Canary::from_env")
    }

    /// 旧路真执行命令行（冻结 af-bench CLI 参数面）。
    pub fn old_command_line(&self) -> String {
        let _ = &self.old_bin;
        unimplemented!("P6：L2Canary::old_command_line")
    }

    /// 新路真执行命令行（薄壳 af-bench，CLI 参数面与旧路逐字一致——薄壳化
    /// 契约"外部调用者无感"的可执行形态）。
    pub fn new_command_line(&self) -> String {
        let _ = &self.new_bin;
        unimplemented!("P6：L2Canary::new_command_line")
    }

    /// §4.3 前置清单：双锚口径 / 并发防撞（R3 错峰分宿主）/ 窗口避开 :17 cron
    /// 与在飞扩量批 / 运营者批预算。真跑前逐项人工核销。
    pub fn preflight_checklist(&self) -> Vec<String> {
        let _ = &self.old_bin;
        unimplemented!("P6：L2Canary::preflight_checklist")
    }
}
