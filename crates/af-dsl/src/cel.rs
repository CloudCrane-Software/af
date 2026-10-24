//! CEL 保守子集静态判定（提案 B §1.4 + §6 E1051-E1053/E2203）。
//!
//! 三项判定（零手写解析器——两条集合包含 + 一次合成求值）：
//! ① `Program::compile` 可解析（失败或解析器内部 panic → E1051）。cel-parser
//!    0.10.1 对尾部运算符/空输入等畸形串触发 antlr4rust 错误恢复路径的内部
//!    panic 而非返回 `ParseErrors`（af-graph `predicate.rs:11-14` 实测注记）；
//!    表达式串来自不可信文档，compile 与 execute 均以 `catch_unwind` 兜底归一。
//! ② `references().functions() ⊆ OPERATOR_WHITELIST`（→ E1052）：函数/方法
//!    调用一律拒绝；`in` 在 cel-parser 0.10.1 的 `functions()` 里登记为 `@in`
//!    （提案 §4.7 实测修正，白名单收 `@in` 不收 `_in_`）。条件表达式/负号/
//!    下标/宏不在保守子集内。
//! ③ `references().variables() ⊆ 位置词表`（→ E2203）且在合成绑定上下文上
//!    求值为布尔（→ E1053，承 af-graph `PredicateError::NotBoolean`）。
//!
//! 求值错误（NoSuchKey/未声明引用/类型不符等）**不判**：合成绑定只承担布尔性
//! 判据——`Ok(非 Bool)` 才是确定性的非布尔证据；字段级词汇核对（如
//! `steps.不存在字段`）不在 §1.4 判定面内（见 RESULT-M4 偏差表）。

use crate::error::ErrorCode;
use cel_interpreter::{Context, Program, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// 算子白名单（§1.4 逐字；实测归纳自 cel-parser 0.10.1 `functions()` 命名）。
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
    pub code: ErrorCode,
    pub detail: String,
}

impl CelError {
    /// 面向违例信息的完整子句（码位语义前缀 + detail）。
    pub fn message(&self) -> String {
        match self.code {
            ErrorCode::CelNonSubset => format!("非保守子集：{}", self.detail),
            _ => self.detail.clone(),
        }
    }
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

    /// 登记一个允许根及其合成值（后写覆盖——词汇表绑定覆盖同名节点 id，
    /// 词汇表名在可见位点上是保留字，见 RESULT-M4 偏差表）。
    pub fn add(&mut self, name: &str, value: Value) {
        self.entries.insert(name.to_string(), value);
    }

    /// 位置词表（全部允许根，确定序）。
    pub fn roots(&self) -> BTreeSet<String> {
        self.entries.keys().cloned().collect()
    }

    /// 取出全部绑定（词汇表覆盖层用）。
    pub(crate) fn into_entries(self) -> BTreeMap<String, Value> {
        self.entries
    }
}

/// `catch_unwind` 兜底（承 af-graph `predicate.rs:45-47` 的健壮性纪律）。
fn run_captured<R>(f: impl FnOnce() -> R) -> Result<R, ()> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|_| ())
}

/// 三项静态判定。判定序固定：解析（E1051）→ 算子（E1052）→ 根（E2203）→
/// 布尔（E1053），首个违例即返（§1.4 草图同款单错出口）。通过 → [`CelCheck`]。
pub fn check(source: &str, env: &CelEnv) -> Result<CelCheck, CelError> {
    // ① 解析：Err → 编译错误；panic → antlr4rust 错误恢复路径，兜底归一。
    let program = match run_captured(|| Program::compile(source)) {
        Ok(Ok(p)) => p,
        Ok(Err(e)) => {
            return Err(CelError {
                code: ErrorCode::CelParse,
                detail: format!("解析失败：{e}"),
            })
        }
        Err(()) => {
            return Err(CelError {
                code: ErrorCode::CelParse,
                detail: format!(
                    "解析失败：cel-parser 内部 panic（尾部运算符/空输入触发 antlr4rust \
                     错误恢复 panic，已兜底归一）：{source:?}"
                ),
            })
        }
    };
    let refs = program.references();

    // ② 函数 ⊆ 白名单（HashSet 迭代序不定——排序保首个违例确定性）。
    let mut funcs = refs.functions();
    funcs.sort_unstable();
    for f in funcs {
        if !OPERATOR_WHITELIST.contains(&f) {
            return Err(CelError {
                code: ErrorCode::CelNonSubset,
                detail: format!("非算子函数/方法调用 {f:?}（函数与方法调用一律拒绝）"),
            });
        }
    }

    // ③ 根 ⊆ 位置词表。
    let mut vars = refs.variables();
    vars.sort_unstable();
    for v in &vars {
        if !env.entries.contains_key(*v) {
            return Err(CelError {
                code: ErrorCode::CelUnknownRoot,
                detail: format!("根绑定 {v:?} 不在位置词表内"),
            });
        }
    }

    // ③' 合成绑定上求值为布尔；求值错误不判（见模块级注记）。
    let mut ctx = Context::default();
    for (name, value) in &env.entries {
        ctx.add_variable_from_value(name.as_str(), value.clone());
    }
    if let Ok(Ok(value)) = run_captured(|| program.execute(&ctx)) {
        if !matches!(value, Value::Bool(_)) {
            return Err(CelError {
                code: ErrorCode::CelNotBoolean,
                detail: format!("谓词求值非布尔：结果为 {}", kind_name(&value)),
            });
        }
    }

    Ok(CelCheck {
        roots: vars.into_iter().map(str::to_owned).collect(),
        ok: true,
    })
}

/// cel 值型名（违例信息用；不依赖上游 Display 文案与 feature 面）。
fn kind_name(v: &Value) -> &'static str {
    match v {
        Value::Int(_) => "int",
        Value::UInt(_) => "uint",
        Value::Float(_) => "float",
        Value::String(_) => "string",
        Value::Bytes(_) => "bytes",
        Value::Bool(_) => "bool",
        Value::List(_) => "list",
        Value::Map(_) => "map",
        Value::Function(..) => "function",
        Value::Null => "null",
        // Duration/Timestamp 等随上游 feature 面开合的变体：不逐一点名。
        _ => "value",
    }
}

/// 合成 cel Map（字段名 → 值）。
pub(crate) fn vmap(pairs: impl IntoIterator<Item = (String, Value)>) -> Value {
    let hm: HashMap<String, Value> = pairs.into_iter().collect();
    hm.into()
}

/// JSON Schema → 合成 cel 值（`input`/`resume`/emit 端口的类型源）。
/// object → 逐属性递归；string/integer/number/boolean → 原子；array → 单元素
/// list（成员选择与 `@in` 可求值）；无 type 信息 → Null（成员选择将 NoSuchKey
/// → 求值错误不判，不产噪声）。v0.1 无 `$ref`（§8-2），不处理。
pub(crate) fn synth_from_schema(schema: &serde_json::Value) -> Value {
    match schema.get("type").and_then(|t| t.as_str()) {
        Some("object") => vmap(
            schema
                .get("properties")
                .and_then(|p| p.as_object())
                .map(|props| props.iter().map(|(k, v)| (k.clone(), synth_from_schema(v))))
                .into_iter()
                .flatten(),
        ),
        Some("string") => Value::String(String::new().into()),
        Some("integer") => Value::Int(0),
        Some("number") => Value::Float(0.0),
        Some("boolean") => Value::Bool(true),
        Some("array") => Value::List(
            vec![schema
                .get("items")
                .map(synth_from_schema)
                .unwrap_or(Value::Null)]
            .into(),
        ),
        _ => Value::Null,
    }
}
