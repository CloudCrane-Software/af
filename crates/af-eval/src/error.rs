//! 评测产线的失败。该停的路径都从这里返回，不降级成一次“通过”。

use crate::phase::Phase;

pub type Result<T> = std::result::Result<T, EvalError>;

/// 调用方能区分的拒绝原因。
#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    /// 标识、正文或预算不满足本 crate 的形状约束。
    #[error("字段 {field} 不合法")]
    Invalid { field: &'static str },

    /// 谱系已经有过 `created`。新版本走 fork，不另起一条同名谱系。
    #[error("谱系 {lineage} 已存在")]
    LineageExists { lineage: String },

    /// 没有这个谱系版本。
    #[error("基准 {lineage}@{version} 不存在")]
    BenchmarkMissing { lineage: String, version: u64 },

    /// 当前阶段不允许这个动作（例如 draft 直接退役、冻结后再冻结）。
    #[error("阶段 {phase} 不允许{action}")]
    IllegalTransition { phase: Phase, action: &'static str },

    /// 离开 draft 之后用例不能原地改。改动要 fork 新版。
    #[error("{lineage}@{version} 处于 {phase}，内容不可变")]
    Sealed {
        lineage: String,
        version: u64,
        phase: Phase,
    },

    /// 没有用例的基准不能冻结。空集会使污染交集恒为空。
    #[error("冻结需要至少一个用例")]
    EmptyBenchmark,

    /// draft 里没有这道用例。
    #[error("用例 {case_id} 不存在")]
    CaseMissing { case_id: String },

    /// 评测运行拒绝读取 draft 或 retired。
    #[error("{lineage}@{version} 处于 {phase}，评测只能读取冻结快照")]
    NotFrozen {
        lineage: String,
        version: u64,
        phase: Phase,
    },

    /// 盲评：运行路径上看不到当前配方，只读也不行。
    #[error("信息屏障：运行 {run_id} 不可见当前配方")]
    InformationBarrier { run_id: String },

    /// 运行输入与基准集相交。运行已停机，没有成绩。
    #[error("污染停机：运行 {run_id} 的输入与基准集相交")]
    Contaminated {
        run_id: String,
        overlap: Vec<String>,
    },

    /// 账本里还没有配方事件。
    #[error("配方尚未登记")]
    RecipeMissing,

    /// 同一 `run_id` 已经绑定过。
    #[error("运行 {run_id} 已存在")]
    RunExists { run_id: String },

    /// 没有这条运行。
    #[error("运行 {run_id} 不存在")]
    RunMissing { run_id: String },

    /// 结果已经落盘，不能再裁决一次。
    #[error("运行 {run_id} 已结束")]
    RunClosed { run_id: String },

    /// 还没有结果行。结束之前不能改写结果。
    #[error("运行 {run_id} 尚未结束")]
    RunOpen { run_id: String },

    /// 预算不是正的毫秒数。
    #[error("预算必须为正毫秒")]
    BadBudget,

    /// 注入时钟相对开始时刻回退。不发明一个裁决。
    #[error("时钟回退：开始 {started_at_ms}，现在 {now_ms}")]
    ClockRegressed { started_at_ms: i64, now_ms: i64 },

    /// UPDATE / DELETE 被只追加触发器拒绝。行保持原样。
    #[error("账本只追加，拒绝改写")]
    AppendOnly,

    /// 重入时 `ledger_id` 或 schema 与本次声明不一致。不改库。
    #[error("账本声明不符：{detail}")]
    DeclarationMismatch { detail: String },

    /// `user_version` 不是本账本声明的版本。
    #[error("未知的账本 schema 版本 {found}，期望 {expected}")]
    UnknownSchema { found: i64, expected: i64 },

    /// `user_version` 仍是 0，但库里已经有表。不把半截库当成首启。
    #[error("user_version 为 0 但表已存在，拒绝当成首启")]
    TornInitialization,

    /// 表或只追加触发器与本 crate 写下的集合不一致。
    #[error("账本结构漂移：{detail}")]
    SchemaDrift { detail: String },

    /// 已落盘的事件序列违反不变量。不静默修好。
    #[error("账本损坏：{detail}")]
    Corrupt { detail: String },

    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl EvalError {
    pub(crate) fn invalid(field: &'static str) -> Self {
        Self::Invalid { field }
    }
}
