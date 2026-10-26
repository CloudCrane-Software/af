//! CEL 保守子集静态判定（提案 B §1.4 + §6 E1051-E1054/E2203）。
//!
//! 四项判定（零手写解析器——两条集合包含 + 一次合成求值 + 一道前置闸）：
//! ⓪ 前置闸（→ E1054）：嵌套深度/字节长超限即拒，**不进解析**——cel-parser
//!    0.10.1（antlr4rust 递归下降）对深嵌套是栈溢出 SIGABRT，非 unwind
//!    panic，`catch_unwind` 兜不住（农田 C_CE15/16/19/B89 四样本实测；
//!    阈值 8MB 主栈标定：深 1000 优雅过、2000 起 abort）。
//! ① `Program::compile` 可解析（失败或解析器内部 panic → E1051）。cel-parser
//!    0.10.1 对尾部运算符/空输入等畸形串触发 antlr4rust 错误恢复路径的内部
//!    panic 而非返回 `ParseErrors`（af-graph `predicate.rs:11-14` 实测注记）；
//!    表达式串来自不可信文档，compile 与 execute 均以 `catch_unwind` 兜底归一。
//! ② `references().functions() ⊆ OPERATOR_WHITELIST`（→ E1052）：函数/方法
//!    调用一律拒绝；`in` 在 cel-parser 0.10.1 的 `functions()` 里登记为 `@in`
//!    （提案 §4.7 实测修正，白名单收 `@in` 不收 `_in_`）。条件表达式/负号/
//!    下标/宏不在保守子集内。**has() 宏例外补位**：cel-parser 把 has() 展开
//!    为 `Select{test:true}`，AST 无函数名、`functions()` 不登记——白名单面
//!    拦不住（农田 C_CE04），词法扫描补拦；all/exists/exists_one/map/filter
//!    展开为 Comprehension，内部 `@not_strictly_false` 调用已被 ② 拦。
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

/// 前置闸阈值（农田实测标定，ANOMALIES.md A-2：本机 8MB 主栈括号深 1000
/// 优雅 PASS、深 2000 起 SIGABRT；对齐稿 15 + 样例 6 的最长 CEL 表达式
/// 155 字节）：深度上限取实测过界 1000（>1000 拒），长度上限 64 KiB
/// （>65536 字节拒，崩溃样本 400KB/900KB 量级、常规表达式 155 字节，
/// 两侧各两个数量级余量）。
pub const MAX_NESTING_DEPTH: usize = 1000;
/// 同上——表达式字节长上限。
pub const MAX_EXPR_BYTES: usize = 64 * 1024;

/// 字符串字面量感知的一次线性词法扫描产出。
struct LexScan {
    /// 括号/方括号/花括号最大嵌套深度（字符串字面量内的括号不计）。
    max_depth: usize,
    /// `has` 标识符后随 `(`（含中间空白）——has() 宏调用形态。
    has_macro: bool,
}

/// 字符串字面量与注释感知的词法扫描：跳过 `'…'`/`"…"` 字面量（含 `\`
/// 转义）与 `//` 行注释（cel-parser 0.10.1 文法唯一注释形：
/// `COMMENT : '//' (~'\n')* -> channel(HIDDEN)`——词法面与解析器同口径；
/// 此前注释文本里的 `has(` 误报 E1052、注释内括号误计深度，红队 W1-C23。
/// 块注释 `/* */` **不跳过**：文法不收（解析必拒 E1051），且若词法跳过
/// 而解析器按真词元下潜，深括号会绕过 E1054 前置闸直击 antlr4rust
/// SIGABRT 面——跳过即不安全），统计括号最大嵌套深度与 has() 宏调用
/// 形态。O(n) 单遍、零递归、零分配到路径热区之外——深/长表达式在此
/// 闸住，不进 antlr4rust 递归下降。
fn lex_scan(source: &str) -> LexScan {
    let mut out = LexScan {
        max_depth: 0,
        has_macro: false,
    };
    let mut depth: usize = 0;
    let mut in_quote: Option<char> = None;
    let mut escaped = false;
    let mut ident = String::new();
    // 上一个完整标识符恰为 `has`，等待确认后随 `(`（允许中间空白）。
    let mut pending_has = false;
    // `/` 悬置：等下一字符判 `//`（行注释）或按除号放行。
    let mut slash_pending = false;
    let mut line_comment = false;
    for c in source.chars() {
        if line_comment {
            if c == '\n' {
                line_comment = false;
            }
            continue;
        }
        if let Some(q) = in_quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                in_quote = None;
            }
            continue;
        }
        if slash_pending {
            slash_pending = false;
            if c == '/' {
                // 行注释：隐藏通道到行尾。pending_has/ident **跨越注释保持**
                // （红队 W2-C24）——此前 `/` 分支预清 pending_has，`has //c\n
                // (x)` 的注释后 `(` 不再置 has_macro，保守子集闸被绕（解析器
                // 把注释作 HIDDEN 通道照常解析出 has 宏）。保持后注释后的
                // `(` 仍能确认宏调用；注释文本自身的内容不计入。
                line_comment = true;
                continue;
            }
            // 非注释：`/` 是除号——宏调用形 `has (…` 被除号隔断不成立，
            // ident/pending_has 在此才清（此前预清即误伤注释形态）。
            ident.clear();
            pending_has = false;
        }
        match c {
            '\'' | '"' => {
                in_quote = Some(c);
                ident.clear();
                pending_has = false;
            }
            '(' | '[' | '{' => {
                if !ident.is_empty() {
                    pending_has = ident == "has";
                    ident.clear();
                }
                if pending_has {
                    out.has_macro = true;
                    pending_has = false;
                }
                depth += 1;
                out.max_depth = out.max_depth.max(depth);
            }
            ')' | ']' | '}' => {
                ident.clear();
                pending_has = false;
                depth = depth.saturating_sub(1);
            }
            '/' => {
                // `/` 悬置等下一字符判 `//`（注释）或除号——**不预清**
                // ident/pending_has（红队 W2-C24）：注释形态下 pending_has
                // 须跨越注释存活，除号形态在 slash_pending 分支才清。
                slash_pending = true;
            }
            c if c.is_alphanumeric() || c == '_' => {
                ident.push(c);
            }
            c if c.is_whitespace() => {
                // 标识符结束；`has` 悬置等 `(`，`has (x)` 与 `has(x)` 同判。
                if !ident.is_empty() {
                    pending_has = ident == "has";
                    ident.clear();
                }
            }
            _ => {
                ident.clear();
                pending_has = false;
            }
        }
    }
    out
}

/// 前置闸（E1054）：超限返回违例描述，否则 None。
fn limit_violation(source: &str, scan: &LexScan) -> Option<String> {
    if source.len() > MAX_EXPR_BYTES {
        return Some(format!(
            "表达式 {} 字节超上限 {}（64 KiB）——超长表达式在下潜解析前置闸拒绝",
            source.len(),
            MAX_EXPR_BYTES
        ));
    }
    if scan.max_depth > MAX_NESTING_DEPTH {
        return Some(format!(
            "括号嵌套深度 {} 超上限 {MAX_NESTING_DEPTH}——深嵌套在下潜解析前置闸拒绝\
             （栈溢出为 abort 非 unwind panic，catch_unwind 兜不住）",
            scan.max_depth
        ));
    }
    None
}

/// 四项静态判定。判定序固定：前置闸（E1054）→ 解析（E1051）→ 算子（E1052，
/// 含 has() 宏词法补位）→ 根（E2203）→ 布尔（E1053），首个违例即返
/// （§1.4 草图同款单错出口）。通过 → [`CelCheck`]。
pub fn check(source: &str, env: &CelEnv) -> Result<CelCheck, CelError> {
    // ⓪ 前置闸：深/长表达式不进 antlr4rust 递归下降（SIGABRT 面）。
    let scan = lex_scan(source);
    if let Some(detail) = limit_violation(source, &scan) {
        return Err(CelError {
            code: ErrorCode::CelLimitExceeded,
            detail,
        });
    }
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

    // ②' has() 宏补位：cel-parser 0.10.1 将 has() 展开为 Select{test:true}，
    //     AST 无函数名、functions() 不登记——白名单面漏网（农田 C_CE04），
    //     词法扫描补拦（字符串字面量内的 "has(" 不误伤）。
    if scan.has_macro {
        return Err(CelError {
            code: ErrorCode::CelNonSubset,
            detail: "宏调用 has(…)（宏不在保守子集内；has 经宏展开后 AST 不登记\
                     函数名，词法面拦）"
                .to_string(),
        });
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
