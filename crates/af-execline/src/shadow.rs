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
use std::process::Command;

use serde_json::{json, Value};

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
    /// 从历史 run 目录组装（eval.log/agent.diff 在 run 目录，task_json 在题库根
    /// —— af-bench main.rs:850 的 `data_root/<instance_id>.json` 口径）。
    pub fn from_run_dir(task_id: &str, data_root: &Path, run_dir: &Path) -> Self {
        Self {
            task_id: task_id.to_string(),
            task_json: data_root.join(format!("{task_id}.json")),
            eval_log: run_dir.join("eval.log"),
            agent_diff: run_dir.join("agent.diff"),
        }
    }
}

/// 旧路判分调用（← af-bench swev.rs:205-210 语句逐字复制：硬编码绝对路径直
/// `Command`，无 locator、无 env 注入、不经 wrapper shell）。源文本与冻结件
/// 双向钉死（tests/p6_shadow.rs cmp_p6_*）。
// `&log_path`/`&diff_path` 的取引用是**有意的**：源文本与冻结 swev.rs:205-210
// 逐字一致是对拍锚（cmp_p6_old_judge_call_matches_frozen_bench_source 钉死），
// 不是冗余借用。
#[allow(clippy::needless_borrows_for_generic_args)]
pub fn old_judge_call(
    task_json_path: &Path,
    log_path: &Path,
    diff_path: &Path,
) -> std::io::Result<std::process::Output> {
    Command::new("/home/pan-ding/swev-venv/bin/python")
        .arg("/home/pan-ding/workspace/agent黑灯工厂/tools/swev-judge.py")
        .arg(task_json_path)
        .arg(&log_path)
        .arg(&diff_path)
        .output()
}

/// 一次判分调用的新旧两侧解读（判定值只从 judge stdout JSON 取，§3.2-3：
/// 只读 resolved/log_parsed 两键，Rust 侧不做再解释）。
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

/// 从子进程 `Output` 解读（读取面与 `suite::swev::verify` 同源：swev.rs:212-231
/// 的 stdout JSON 两键读取；note 只为诊断，不参与对拍判据）。
fn read_verdict(output: &std::process::Output) -> JudgeVerdict {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() {
        return JudgeVerdict {
            exit_ok: false,
            resolved: None,
            log_parsed: None,
            stdout,
            note: format!(
                "judge 退出 {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(300)
                    .collect::<String>()
            ),
        };
    }
    let row: Value = match serde_json::from_str(&stdout) {
        Ok(row) => row,
        Err(err) => {
            let head: String = stdout.chars().take(200).collect();
            return JudgeVerdict {
                exit_ok: true,
                resolved: None,
                log_parsed: None,
                stdout,
                note: format!("judge 输出不是 JSON: {err}: {head}"),
            };
        }
    };
    JudgeVerdict {
        exit_ok: true,
        resolved: row.get("resolved").and_then(Value::as_bool),
        log_parsed: row.get("log_parsed").and_then(Value::as_bool),
        stdout,
        note: String::new(),
    }
}

/// 新路判分调用：`judge::invoke`（locator）+ stdout JSON 解读。
pub fn new_judge_call(locator: &JudgeLocator, ev: &L0Evidence) -> JudgeVerdict {
    match crate::judge::invoke(locator, &ev.task_json, &ev.eval_log, &ev.agent_diff) {
        Ok(output) => read_verdict(&output),
        Err(err) => JudgeVerdict {
            exit_ok: false,
            resolved: None,
            log_parsed: None,
            stdout: String::new(),
            note: format!("judge 启动失败: {err}"),
        },
    }
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
        self.old.resolved == self.new.resolved
    }
    pub fn log_parsed_agree(&self) -> bool {
        self.old.log_parsed == self.new.log_parsed
    }
    pub fn stdout_agree(&self) -> bool {
        self.old.stdout == self.new.stdout
    }
    /// 三项全齐（判据：逐位一致）。
    pub fn agree(&self) -> bool {
        self.resolved_agree() && self.log_parsed_agree() && self.stdout_agree()
    }
}

/// L0 批量回放：每份证据两路各判一次并配对（不中断——逐证据出结果；判据
/// 检查在调用方，失败侧 exit_ok=false 时 note 带诊断）。
pub fn l0_replay(evidences: &[L0Evidence], locator: &JudgeLocator) -> Vec<L0Pair> {
    evidences
        .iter()
        .map(|ev| {
            let old = match old_judge_call(&ev.task_json, &ev.eval_log, &ev.agent_diff) {
                Ok(output) => read_verdict(&output),
                Err(err) => JudgeVerdict {
                    exit_ok: false,
                    resolved: None,
                    log_parsed: None,
                    stdout: String::new(),
                    note: format!("judge 启动失败: {err}"),
                },
            };
            let new = new_judge_call(locator, ev);
            L0Pair {
                task_id: ev.task_id.clone(),
                old,
                new,
            }
        })
        .collect()
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
        self.deterministic_diffs.is_empty() && self.missing_keys_in_new.is_empty()
    }
}

/// 比对一对 legacy results 行（18 键 swe-v 口径；键序无关——serde_json 序列化
/// 两侧同序，字节一致由行序列化断言另证）。字段缺侧按 null 计（缺确定性字段
/// 即失配）。
pub fn diff_legacy_rows(old: &Value, new: &Value) -> RowDiff {
    let mut diff = RowDiff {
        rows_equal: old == new,
        ..RowDiff::default()
    };
    let old_obj = old.as_object();
    let new_obj = new.as_object();
    for field in DETERMINISTIC_ROW_FIELDS
        .iter()
        .chain(METERING_ROW_FIELDS.iter())
    {
        let (Some(o), Some(n)) = (old_obj, new_obj) else {
            continue;
        };
        let ov = o.get(*field).cloned().unwrap_or(Value::Null);
        let nv = n.get(*field).cloned().unwrap_or(Value::Null);
        if ov == nv {
            continue;
        }
        if METERING_ROW_FIELDS.contains(field) {
            diff.metering_diffs.push((*field).to_string());
        } else {
            diff.deterministic_diffs.push(format!("{field}: {ov}≠{nv}"));
        }
    }
    if let (Some(o), Some(n)) = (old_obj, new_obj) {
        for key in o.keys() {
            if !n.contains_key(key) {
                diff.missing_keys_in_new.push(key.clone());
            }
        }
        for key in n.keys() {
            if !o.contains_key(key) {
                diff.extra_keys_in_new.push(key.clone());
            }
        }
    }
    diff
}

/// 比对两份 results.jsonl（逐行配对；行数不等时以两行数较大值逐对出表——
/// 缺行侧按缺失键报，行数差本身即判不过）。
pub fn diff_results_files(old_path: &Path, new_path: &Path) -> Result<Vec<RowDiff>, String> {
    let read = |path: &Path| -> Result<Vec<Value>, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|err| format!("读 {} 失败: {err}", path.display()))?;
        text.lines()
            .filter(|line| !line.is_empty())
            .map(|line| {
                serde_json::from_str(line).map_err(|err| format!("{} 坏行: {err}", path.display()))
            })
            .collect()
    };
    let old = read(old_path)?;
    let new = read(new_path)?;
    let rows = old.len().max(new.len());
    let mut out = Vec::with_capacity(rows);
    for index in 0..rows {
        let empty = json!({});
        let o = old.get(index).unwrap_or(&empty);
        let n = new.get(index).unwrap_or(&empty);
        out.push(diff_legacy_rows(o, n));
    }
    Ok(out)
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
/// turns/tokens_*/wall_ms→单元 usage（行计量字段）。来源 = §4.2-3 与
/// tools/bench-mes.py:9, 98-131 的事件族取值面。
pub fn mes_mapping_table() -> Vec<MesMappingRow> {
    vec![
        MesMappingRow {
            legacy_event: "started",
            new_counterpart: "start",
            source: "调度侧 started ↔ driver start",
        },
        MesMappingRow {
            legacy_event: "done",
            new_counterpart: "complete",
            source: "调度侧 done/partial/failed ↔ driver complete",
        },
        MesMappingRow {
            legacy_event: "reward",
            new_counterpart: "verdict",
            source: "bench-mes reward ↔ 单元 verdict（行 reward）",
        },
        MesMappingRow {
            legacy_event: "tokens_input",
            new_counterpart: "usage.quota.input",
            source: "bench-mes tokens_input ↔ 行 tokens.input",
        },
        MesMappingRow {
            legacy_event: "tokens_output",
            new_counterpart: "usage.quota.output",
            source: "bench-mes tokens_output ↔ 行 tokens.output",
        },
        MesMappingRow {
            legacy_event: "cache_read",
            new_counterpart: "usage.quota.cache_read",
            source: "bench-mes cache_read ↔ 行 tokens.cache_read",
        },
        MesMappingRow {
            legacy_event: "turns",
            new_counterpart: "usage.turns",
            source: "bench-mes turns ↔ 行 turns",
        },
        MesMappingRow {
            legacy_event: "wall_ms",
            new_counterpart: "wall_ms",
            source: "bench-mes wall_ms ↔ 行 agent_wall_ms",
        },
    ]
}

/// 旧路事件族从 legacy 行推导（bench-mes.py 视角：读行字段取值，
/// 事件族 = 行字段的投影；started/done 按 status 分类——setup_failed 供给面
/// 失败未开工、run_failed 开工未完，其余即完工地走完）。
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
    let status = row.get("status").and_then(Value::as_str).unwrap_or("");
    let started = status != "setup_failed";
    LegacyEventFamily {
        started,
        done: !matches!(status, "setup_failed" | "run_failed"),
        reward: row.get("reward").and_then(Value::as_bool),
        tokens_input: row.pointer("/tokens/input").and_then(Value::as_u64),
        tokens_output: row.pointer("/tokens/output").and_then(Value::as_u64),
        cache_read: row.pointer("/tokens/cache_read").and_then(Value::as_u64),
        turns: row.get("turns").and_then(Value::as_u64).map(|t| t as u32),
        wall_ms: row.get("agent_wall_ms").and_then(Value::as_u64),
    }
}

/// 新路 MES 事件（driver 四类 + 单元面）对 legacy 行的映射检查：
/// start/complete（started↔start、done↔complete）各恰 1 条、全部事件
/// work_order_id 收敛同一值（quantity=1 语义）、行内 reward/计量字段可被
/// 映射表覆盖。返回问题清单（空 = 通过）。
pub fn check_mes_against_row(events: &[af_mes::MesEvent], row: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    for kind in ["start", "complete"] {
        let hits = events.iter().filter(|event| event.kind == kind).count();
        match hits {
            1 => {}
            0 => problems.push(format!("MES 缺 {kind}（映射表 {kind} 无对应事件）")),
            n => problems.push(format!("MES {kind} 有 {n} 条（应恰 1 条）")),
        }
    }
    let ids: std::collections::BTreeSet<&str> = events
        .iter()
        .map(|event| event.work_order_id.as_str())
        .collect();
    if ids.len() > 1 {
        problems.push(format!(
            "MES work_order_id 不收敛（{} 个不同值: {ids:?}）",
            ids.len()
        ));
    }
    for event in events {
        if event.quantity != 1 {
            problems.push(format!(
                "MES {} quantity={} ≠ 1（quantity=1 语义不变）",
                event.kind, event.quantity
            ));
        }
    }
    // 行内可映射面：reward 与计量字段在册（映射表的"新侧落点"必须可寻址）。
    if row.get("reward").is_none() {
        problems.push("行缺 reward（reward→verdict 映射落点缺失）".into());
    }
    if row.pointer("/tokens/input").is_none() {
        problems.push("行缺 tokens.input（tokens_input→usage.quota.input 落点缺失）".into());
    }
    if row.get("agent_wall_ms").is_none() {
        problems.push("行缺 agent_wall_ms（wall_ms 映射落点缺失）".into());
    }
    problems
}

// ---- L2：端到端金丝雀钩子（§4.1-L2 / §4.3；本包不执行）----

/// 冻结旧路二进制指纹前 16 位（盘点 §② 与方案 §0 实测口径）。
pub const OLD_BIN_SHA256_PREFIX: &str = "7a87876c0a5541c0";

/// L2 题集缺省值（§4.1-L2：确认集快标 3 + N 档 7 的折中首弹；从
/// af过题取证-20261024.md §1 表选取快标已确认 3 题 + glm 档 4 题）。
pub const L2_DEFAULT_TASKS: &[&str] = &[
    "astropy__astropy-12907",
    "django__django-11095",
    "matplotlib__matplotlib-22719",
    "pydata__xarray-2905",
    "pydata__xarray-3151",
    "pydata__xarray-4094",
    "pylint-dev__pylint-7277",
];

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
        let env_or =
            |key: &str, default: &str| std::env::var(key).unwrap_or_else(|_| default.to_string());
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let target_dir = std::env::var("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or(workspace_root.join("target"));
        Self {
            old_bin: PathBuf::from(env_or(
                "AF_SHADOW_OLD_BIN",
                "/tmp/threetest-bin/af-bench-g3-frozen",
            )),
            new_bin: PathBuf::from(env_or(
                "AF_SHADOW_NEW_BIN",
                target_dir
                    .join("debug")
                    .join("af-bench")
                    .to_str()
                    .unwrap_or("af-bench"),
            )),
            data_root: PathBuf::from(env_or("AF_SHADOW_DATA_ROOT", "/tmp/swev-s1")),
            tasks: env_or("AF_SHADOW_TASKS", &L2_DEFAULT_TASKS.join(","))
                .split(',')
                .map(|task| task.trim().to_string())
                .filter(|task| !task.is_empty())
                .collect(),
            profile: env_or("AF_SHADOW_PROFILE", "glm-cp"),
            host: env_or("AF_SHADOW_HOST", "local"),
            out_old: PathBuf::from(env_or("AF_SHADOW_OUT_OLD", "/tmp/shadow-l2/old")),
            out_new: PathBuf::from(env_or("AF_SHADOW_OUT_NEW", "/tmp/shadow-l2/new")),
        }
    }

    /// 旧路真执行命令行（冻结 af-bench CLI 参数面）。
    pub fn old_command_line(&self) -> String {
        self.command_line(&self.old_bin)
    }

    /// 新路真执行命令行（薄壳 af-bench，CLI 参数面与旧路逐字一致——薄壳化
    /// 契约"外部调用者无感"的可执行形态）。
    pub fn new_command_line(&self) -> String {
        self.command_line(&self.new_bin)
    }

    fn command_line(&self, bin: &Path) -> String {
        format!(
            "{} --suite swe-v --data-root {} --tasks {} --profile {} --host {} --out {}",
            bin.display(),
            self.data_root.display(),
            self.tasks.join(","),
            self.profile,
            self.host,
            if bin == self.old_bin {
                self.out_old.display()
            } else {
                self.out_new.display()
            }
        )
    }

    /// §4.3 前置清单：双锚口径 / 并发防撞（R3 错峰分宿主）/ 窗口避开 :17 cron
    /// 与在飞扩量批 / 运营者批预算 / 旧路二进制指纹核销。真跑前逐项人工核销。
    pub fn preflight_checklist(&self) -> Vec<String> {
        vec![
            "判分双锚口径：现产 a292547f…（JudgeLocator::production 默认锚）+ 历史 \
                 0338566c…（rsi-drill 冻结件）双锚并记（方案 §3.1/§4.3-R5）"
                .to_string(),
            format!(
                "旧路二进制指纹：sha256 前 16 位必须 = {OLD_BIN_SHA256_PREFIX}（冻结件 \
                 7a87876c…，盘点 §②）——不匹配即拒跑"
            ),
            "并发防撞 R3：新旧同宿主会互杀同名容器（setup 首步 docker rm -f）——\
             错峰起跑或分宿主（旧 anolis / 新 local）"
                .to_string(),
            "窗口：避开每小时 :17 cron（ADR-008:14）与在飞三测试扩量批（在飞盘点 §②）".to_string(),
            "预算：L2 = 模型+容器级（10 题双路 ≈ 双份单题成本上限）——运营者批预算/窗口".to_string(),
            "判据：硬门 = 新路三件套旧 judge 复判 100% 一致；软门 = resolved 一致率 \
                 ≥9/10，不一致逐题归因（模型采样/供给面 vs 判分链），归因不得落在判分链"
                .to_string(),
        ]
    }
}
