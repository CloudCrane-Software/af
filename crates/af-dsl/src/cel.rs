//! CEL 保守子集静态判定（提案 B §1.4 + §6 E1051-E1053/E2203）。
//!
//! 三项判定（零手写解析器，两条集合包含 + 一次合成求值）：
//! ① `Program::compile` 可解析（失败/解析器内部 panic → E1051，cel-parser
//!    0.10.1 对尾部运算符/空输入触发 antlr4rust 错误恢复 panic，承 af-graph
//!    `predicate.rs:11-14` 实测注记，`catch_unwind` 兜底归一）；
//! ② `references().functions() ⊆ OPERATOR_WHITELIST`（函数/方法调用一律拒绝
//!    → E1052；`in` 在 cel-parser 0.10.1 登记为 `@in`，实测修正入白名单）；
//! ③ `references().variables() ⊆ 位置词表`（→ E2203）且在合成绑定上下文上
//!    求值为布尔（→ E1053）。求值错误（NoSuchKey/未声明引用等）不判——
//!    合成绑定只承担布尔性判据，字段级词汇核对不在 §1.4 判定面内（见
//!    RESULT-M4 偏差表）。
//!
//! **RED 骨架**：判定逻辑在 GREEN 期实装；本文件先立 API 面与白名单常量
//! （白名单为 §1.4 逐字规格数据），使测试可编译可红。

use cel_interpreter::Value;
use std::collections::{BTreeMap, BTreeSet};

/// 算子白名单（§1.4 逐字；实测归纳自 cel-parser 0.10.1 `functions()` 命名，
/// `in` 登记为 `@in`）。条件表达式 `_?_:_`、负号 `-_`、下标 `_[_]`、宏
/// （all/exists/map/filter/has/exists_one）不在保守子集内 → 命中即 E1052。
pub const OPERATOR_WHITELIST: &[&str] = &[
    "_==_", "_!=_", "_<_", "_<=_", "_>_", "_>=_", "_&&_", "_||_", "!_", "_+_", "_-_", "_*_", "_/_",
    "_%_", "@in",
];

/// 一条 CEL 表达式静态判定的产出（全部判定通过时）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CelCheck {
    /// 表达式实际引用的根绑定名（`references().variables()`）。
    pub roots: BTreeSet<String>,
    pub ok: bool,
}

/// 判定失败：稳定码（E1051/E1052/E2203/E1053）+ 精确信息。
/// scope/site 由调用方（validate 管道）补齐。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CelError {
    pub code: crate::error::ErrorCode,
    pub detail: String,
}

/// 位置词表 + 合成绑定上下文：根名 → 合成 cel 值（按 §1.4 词汇表与文档
/// schema 建模；`validate/m4.rs` 在各 CEL 位点构造）。
#[derive(Debug, Clone, Default)]
pub struct CelEnv {
    entries: BTreeMap<String, Value>,
}

impl CelEnv {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一个允许根及其合成值（后写覆盖——词汇表绑定覆盖同名节点 id）。
    pub fn add(&mut self, name: &str, value: Value) {
        self.entries.insert(name.to_string(), value);
    }

    /// 位置词表（全部允许根，确定序）。
    pub fn roots(&self) -> BTreeSet<String> {
        self.entries.keys().cloned().collect()
    }
}

/// 三项静态判定。通过 → [`CelCheck`]（含引用根集）；失败 → 首个违例
/// （解析 → 算子 → 根 → 布尔，判定序固定）。
pub fn check(_source: &str, _env: &CelEnv) -> Result<CelCheck, CelError> {
    // RED 骨架：未实装——正例（合法表达式）恒过、反例（非法表达式）恒漏。
    Ok(CelCheck {
        roots: BTreeSet::new(),
        ok: true,
    })
}

/// JSON Schema → 合成 cel 值（`input`/`resume`/emit 端口的类型源）。
/// object → 逐属性递归映射；string/integer/number/boolean → 原子；array →
/// 单元素 list；无 type 信息 → Null（成员选择将 NoSuchKey → 求值错误不判）。
#[allow(dead_code)] // GREEN 期接线
pub(crate) fn synth_from_schema(_schema: &serde_json::Value) -> Value {
    Value::Null // RED 骨架
}
