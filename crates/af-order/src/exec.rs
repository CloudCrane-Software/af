//! submit 的执行参数面（正式 CLI 契约；消债 B①，原补丁台账-第二阶段 #B）。
//!
//! 分工一句话：**OrderSpec（JSON）管身份，CLI 旗标管执行**——`order_id/
//! line/budget/note` 是单身的稳定面（F-首单/二单的 7 行 spec 原样可复用），
//! 换题/换档/换预算顶是执行面的事，经 CLI 声明、逐次可变。
//!
//! - 三件套 `--task/--profile/--data-root` **同给**才走真实执行
//!   （`SwevExecuteUnit`：SshBridge + HarnessRunner + 冻结判分指纹门）；
//!   全不给 = **stub 冒烟档**（`StubExecuteUnit`：既有测试与冒烟面，
//!   非生产执行路径——保留为显式分档，不退役）；
//! - `--host/--max-turns/--spend-cap-cents` 预算硬顶注入：`--host` 缺省
//!   `anolis`；`--max-turns/--spend-cap-cents` 缺席 = 各单元自载缺省
//!   （swev = fix2 代际 250 轮/$3；graph = 节点 60 轮、总额归 plan
//!   cost_cap；report = 40 轮/$3——红队 W2-C33：CLI 缺省值不覆写单元
//!   缺省；F-首单/二单任务包口径 60/$1 为显式覆写）；
//! - `--mes-ch auto|url` 是**记账声明**，与执行面正交（冒烟档同样可声明）：
//!   只把 CH 目标转交 af-execline 驱动缝（[`MesChSink`] 读侧 event_id
//!   对账差量送批）——本 crate 零 CH 写侧。落在 `Command::Submit::mes_ch`，
//!   不在本结构（F-首单期它在补丁的 RealExec 里；正式化后按语义归位）。
//!
//! 向后兼容（消除条件原文："对 F-首单/二单已用旗标保持向后兼容，这两单
//! 证据链要可复现"）：旗标名、取值语义、缺省值与补丁期完全一致，
//! tests/exec_contract.rs 对两条已归档命令逐字钉死。

use std::path::PathBuf;

use af_execline::{JudgeLocator, ReportUnitSpec, SwevUnitSpec};

use crate::error::OrderError;

/// 真实执行路径的缺省宿主（与 af-bench --host 缺省同源）。
pub const DEFAULT_HOST: &str = "anolis";
/// 缺省轮数上限 = fix2 代际（af-bench main.rs:748）。
pub const DEFAULT_MAX_TURNS: u32 = 250;
/// 缺省单题成本上限（美分）= fix2 代际 $3（main.rs:787）。
pub const DEFAULT_SPEND_CAP_CENTS: i64 = 300;

/// submit 的真实执行参数（正式契约类型；F-首单期名 RealExec 沿用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealExec {
    /// 目标题（swe-v = instance_id；`<data_root>/<task>.json` 任务包）。
    pub task: String,
    /// 端点档案名（af-execline profile 表，如 glm-cp / step-5）。
    pub profile: String,
    /// 题库根。
    pub data_root: PathBuf,
    /// 容器宿主别名（anolis/local）。
    pub host: String,
    /// 轮数上限（红队 W2-C33：`None` = 用户未给 `--max-turns`——各单元
    /// 自载缺省保留，swev 250 / graph 60 / report 40；不得拿 CLI 缺省
    /// 250 无条件覆写，否则 graph/report 的帽被静默放宽 4-6 倍）。
    pub max_turns: Option<u32>,
    /// 单题成本上限（美分；`None` = 用户未给旗标——swev/report 用 $3
    /// 代际缺省，graph 退回 plan cost_cap 治理）。
    pub spend_cap_cents: Option<i64>,
}

/// submit 接受的全部执行面旗标（供 cli::split_flags 白名单；
/// `--mes-ch` 是记账声明，归 cli.rs 处理，不在此列）。
pub const EXEC_FLAGS: &[&str] = &[
    "--task",
    "--profile",
    "--data-root",
    "--host",
    "--max-turns",
    "--spend-cap-cents",
];

/// 从已拆分的 flag 表解析执行面：三件套全给 = 真实执行，全不给 = None
/// （冒烟档），半套 = OR1001。附属旗标（--host 等）不得脱离三件套单独出现。
pub fn parse_exec_flags(flags: &[(&str, String)]) -> Result<Option<RealExec>, OrderError> {
    let value = |name: &str| flags.iter().find(|(k, _)| *k == name).map(|(_, v)| v);
    let task = value("--task");
    let profile = value("--profile");
    let data_root = value("--data-root");
    match (task, profile, data_root) {
        (None, None, None) => {
            for name in ["--host", "--max-turns", "--spend-cap-cents"] {
                if value(name).is_some() {
                    return Err(OrderError::Usage {
                        detail: format!(
                            "{name} 依赖 --task/--profile/--data-root 三件套（不给三件套 = stub 冒烟档）"
                        ),
                    });
                }
            }
            Ok(None)
        }
        (Some(task), Some(profile), Some(data_root)) => Ok(Some(RealExec {
            task: task.clone(),
            profile: profile.clone(),
            data_root: PathBuf::from(data_root),
            host: value("--host")
                .cloned()
                .unwrap_or_else(|| DEFAULT_HOST.into()),
            // 红队 W2-C33：旗标缺席 = None（单元自载缺省保留），不得把
            // CLI 缺省值当用户意图覆写 graph/report 的 turns 帽。
            max_turns: parse_u32_flag(value("--max-turns"), "--max-turns")?,
            spend_cap_cents: {
                // 红队 W2-C40：负/零帽在 CLI 解析期前置拒绝——此前只拒非整数，
                // 负帽直到执行面凭证签发才被 af-harness 拒，彼时订单目录、
                // 账本 op 与 MES release/start 均已产生，失败代价远大于一次
                // 用法错误且留下卡 InProgress 的污染订单。max_turns=0 的
                // 零模型调用探针语义保留（spec-F4a3 同款），不在此禁。
                match parse_i64_flag(value("--spend-cap-cents"), "--spend-cap-cents")? {
                    None => None,
                    Some(cap) if cap <= 0 => {
                        return Err(OrderError::Usage {
                            detail: format!(
                                "--spend-cap-cents 必须 > 0（美分；零模型调用探针用 --max-turns 0）: {cap}"
                            ),
                        })
                    }
                    Some(cap) => Some(cap),
                }
            },
        })),
        _ => Err(OrderError::Usage {
            detail: "真实执行需要 --task/--profile/--data-root 三件同给（或全不给走 stub 冒烟档）"
                .into(),
        }),
    }
}

fn parse_u32_flag(raw: Option<&String>, name: &str) -> Result<Option<u32>, OrderError> {
    match raw {
        None => Ok(None),
        Some(text) => text
            .parse::<u32>()
            .map(Some)
            .map_err(|_| OrderError::Usage {
                detail: format!("{name} 不是非负整数: {text}"),
            }),
    }
}

fn parse_i64_flag(raw: Option<&String>, name: &str) -> Result<Option<i64>, OrderError> {
    match raw {
        None => Ok(None),
        Some(text) => text
            .parse::<i64>()
            .map(Some)
            .map_err(|_| OrderError::Usage {
                detail: format!("{name} 不是整数: {text}"),
            }),
    }
}

/// 真实执行单元 spec：fix2 代际基底 + 调用方预算硬顶覆写（红队 W2-C33：
/// 旗标未给 = 单元自载缺省——swev 的 fix2 缺省恰为 250/$3，与 CLI 代际
/// 缺省同值，此处 unwrap_or 为恒等兜底）+ 现产冻结判分锚（a292547f…，
/// 装载期/入口双重指纹门）。
pub fn swev_unit_spec_for(exec: &RealExec) -> SwevUnitSpec {
    let mut spec = SwevUnitSpec::fix2(exec.profile.clone(), JudgeLocator::production());
    spec.max_turns = exec.max_turns.unwrap_or(DEFAULT_MAX_TURNS);
    spec.spend_cap_cents = exec.spend_cap_cents.unwrap_or(DEFAULT_SPEND_CAP_CENTS);
    spec
}

// ============================================================
// --unit 单元路由（裁决7：--unit swev|report|stub，缺省保持现行为）
// ============================================================

/// 单元选择（CLI `--unit` 值；Auto = 编译期缺省——现行为）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitChoice {
    /// 缺省：三件套在 → Swev；缺 → stub 冒烟（既有行为，零变化）。
    Auto,
    Swev,
    Report,
    /// 图执行器（F-第四单执行长缺口销项）：消费 <data_root>/<task>/plan.json。
    Graph,
    Stub,
}

/// 路由解析产物（run_submit 分派面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectedUnit {
    Stub,
    Swev,
    Report,
    Graph,
}

/// `--unit` 值解析（swev|report|stub；其余 OR1001）。
pub fn parse_unit_choice(raw: &str) -> Result<UnitChoice, OrderError> {
    match raw {
        "swev" => Ok(UnitChoice::Swev),
        "report" => Ok(UnitChoice::Report),
        "graph" => Ok(UnitChoice::Graph),
        "stub" => Ok(UnitChoice::Stub),
        other => Err(OrderError::Usage {
            detail: format!(
                "OR1001: 未知 --unit {other:?}（可选 swev | report | graph | stub；不给 = 现行为：三件套在=swev、缺=stub 冒烟）"
            ),
        }),
    }
}

/// 路由矩阵（裁决7）：Auto=现行为；swev/report 需三件套；stub 与三件套互斥。
pub fn resolve_unit(
    choice: UnitChoice,
    exec: Option<&RealExec>,
) -> Result<SelectedUnit, OrderError> {
    match (choice, exec) {
        (UnitChoice::Auto, Some(_)) => Ok(SelectedUnit::Swev),
        (UnitChoice::Auto, None) => Ok(SelectedUnit::Stub),
        (UnitChoice::Swev, Some(_))
        | (UnitChoice::Report, Some(_))
        | (UnitChoice::Graph, Some(_)) => Ok(match choice {
            UnitChoice::Swev => SelectedUnit::Swev,
            UnitChoice::Report => SelectedUnit::Report,
            UnitChoice::Graph => SelectedUnit::Graph,
            UnitChoice::Auto | UnitChoice::Stub => unreachable!("匹配臂已收"),
        }),
        (UnitChoice::Swev, None) | (UnitChoice::Report, None) | (UnitChoice::Graph, None) => {
            Err(OrderError::Usage {
                detail: "OR1001: --unit swev/report/graph 需要 --task/--profile/--data-root 三件套"
                    .into(),
            })
        }
        (UnitChoice::Stub, Some(_)) => Err(OrderError::Usage {
            detail: "OR1001: --unit stub 与执行三件套互斥（冒烟档不带执行参数）".into(),
        }),
        (UnitChoice::Stub, None) => Ok(SelectedUnit::Stub),
    }
}

/// report 单元的 registry_root 缺省（机器本地常量——judge.rs production 同款
/// R2 口径；任务包 task.json 的 registry_root 声明优先，F-第五单显式给出）。
pub const DEFAULT_REGISTRY_ROOT: &str = "/home/pan-ding/workspace/af-wt-g4/af/packages";

/// report 单元材料面（v1 约定：任务包目录 `<data_root>/<task>/` 携
/// task.json + plan.json——submit 级 `--plan` 档形态=开放问题 8，不预判；
/// registry_root 取 task.json 声明，缺省机器本地常量）。
pub fn report_unit_for(exec: &RealExec) -> Result<ReportUnitSpec, OrderError> {
    let task_dir = exec.data_root.join(&exec.task);
    let task_json = task_dir.join("task.json");
    let raw = std::fs::read_to_string(&task_json).map_err(|err| OrderError::Usage {
        detail: format!(
            "OR1001: report 任务包缺 task.json（约定布局 {}/task.json）: {err}",
            task_dir.display()
        ),
    })?;
    let task: serde_json::Value = serde_json::from_str(&raw).map_err(|err| OrderError::Usage {
        detail: format!("OR1001: task.json 不是合法 JSON: {err}"),
    })?;
    if !task_dir.join("plan.json").is_file() {
        return Err(OrderError::Usage {
            detail: format!(
                "OR1001: report 任务包缺 plan.json（v1 约定：计划随任务包；submit 级 --plan 档形态=开放问题 8 待裁决）: {}",
                task_dir.display()
            ),
        });
    }
    let registry_root = task
        .get("registry_root")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_REGISTRY_ROOT));
    // 红队 W2-C54：--max-turns/--spend-cap-cents 接入 report 臂（此前只 swev
    // 臂覆写，report/graph 两臂完全不读这两个旗标，订单输出 JSON 却把它们
    // 当预算回显）。spec.spend_cap_cents 即环级总帽（RingBudgets 取
    // min(spec, plan cost_cap)）——旗标语义 = 本单总预算，与 graph 臂一致。
    // 红队 W2-C33：仅用户显式给旗标时覆写——单元 v1 缺省（turns 40 /
    // spend 300）不在场时保留，不被 CLI 缺省值静默放宽。
    let mut spec = ReportUnitSpec::v1(
        exec.profile.clone(),
        af_execline::ReportJudgeLocator::production(),
        registry_root,
    );
    if let Some(turns) = exec.max_turns {
        spec.max_turns = turns;
    }
    if let Some(cap) = exec.spend_cap_cents {
        spec.spend_cap_cents = cap;
    }
    Ok(spec)
}

// ============================================================
// --unit graph 图执行器材料面（F-第四单执行长缺口销项）
// ============================================================

/// graph 单元材料面（v1 约定：任务包目录 `<data_root>/<task>/` 携
/// task.json + plan.json——与 report 同款约定；submit 级 `--plan` 档形态 =
/// 开放问题 8 已裁留 v0.2；registry_root 取 task.json 声明，缺省机器本地常量）。
pub fn graph_unit_for(exec: &RealExec) -> Result<af_execline::GraphUnitSpec, OrderError> {
    let task_dir = exec.data_root.join(&exec.task);
    let task_json = task_dir.join("task.json");
    let raw = std::fs::read_to_string(&task_json).map_err(|err| OrderError::Usage {
        detail: format!(
            "OR1001: graph 任务包缺 task.json（约定布局 {}/task.json）: {err}",
            task_dir.display()
        ),
    })?;
    let task: serde_json::Value = serde_json::from_str(&raw).map_err(|err| OrderError::Usage {
        detail: format!("OR1001: task.json 不是合法 JSON: {err}"),
    })?;
    if !task_dir.join("plan.json").is_file() {
        return Err(OrderError::Usage {
            detail: format!(
                "OR1001: graph 任务包缺 plan.json（v1 约定：计划随任务包；submit 级 --plan 档形态=开放问题 8 已裁留 v0.2）: {}",
                task_dir.display()
            ),
        });
    }
    let registry_root = task
        .get("registry_root")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_REGISTRY_ROOT));
    // 红队 W2-C54：graph 臂接线——max_turns = 节点级 turns 缺省覆写；
    // spend_cap_cents = 订单级总预算（plan 无 cost_cap 时作总闸，N 节点
    // 收敛到同一总额；plan 有 cost_cap 时以 plan 为准、旗标仅作无计划
    // 形态的兜底）。红队 W2-C33：仅用户显式给旗标时覆写——单元 v1 缺省
    // （节点 turns 60 / 订单总帽 None=plan cost_cap 治理）不在场时保留，
    // 不被 CLI 缺省值静默放宽 4-6 倍。
    let mut spec = af_execline::GraphUnitSpec::v1(exec.profile.clone(), registry_root);
    // 红队 W2-C51：graph 臂与 swev/report 两臂同源注入现产判分锚——
    // GraphUnitSpec.judge 此前恒 None（装配面三层里唯一没接线的一层），
    // 任何含 pkg.sweagent.swebench_eval 检查节点的图计划经 CLI 提交必然
    // 在分派面 fail-loud 拒载，图臂唯一走冻结判分链的单元从下单入口
    // 永不可达（已交付图单全部改走 pkg.sweng.test_unit 侧证）。
    spec.judge = Some(af_execline::JudgeLocator::production());
    if let Some(turns) = exec.max_turns {
        spec.default_node_turns = turns;
    }
    spec.order_spend_cap_cents = exec.spend_cap_cents;
    Ok(spec)
}
