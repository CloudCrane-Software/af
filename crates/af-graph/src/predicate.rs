//! 内嵌谓词求值（CEL）—— af-graph 的最小可用封装：compile + eval + 错误统一传递。
//!
//! 定位（`poc-cue/RESULT.md` ④-6、`verify-load/REPORT.md` ④-2）：CUE 只负责表达式串的
//! "存在性 + 参数闭合"静态校验；**数据相关的运行时谓词**（时间窗、连续越限、失败计数等
//! CUE 求值不了的条件）交给受限表达式引擎在 Rust 内嵌求值。本模块选 `cel-interpreter`
//! （crates.io 最新稳定 0.10.0）承担内嵌求值侧；rhai 等图灵完备脚本按设计纪律排除。
//!
//! 只做"最小可用"：把一条 CEL 表达式 compile 成 [`Predicate`]，随后可多次 `eval`；
//! 编译错误 / 求值错误 / 非布尔结果分别用 [`PredicateError`] 的三个变体向上传递，不 panic。
//!
//! **健壮性注记**：`cel-parser` 0.10.1 对"尾部运算符 / 空输入"等畸形串（如 `"1 +"`、
//! `"count >"`、`""`）会触发 antlr4rust 错误恢复路径的内部 panic，而非返回 `ParseErrors`
//! （实测见 `RESULT-UPDATE-T8.md` ③）。谓词串可能来自配方 / 配置等不可信来源，内嵌求值器
//! 必须不崩溃——故 compile / eval 均以 `catch_unwind` 兜底，把这类 panic 归一成错误向上传递。

use cel_interpreter::{Context, ExecutionError, Program, Value};
use std::collections::HashMap;
use std::fmt;

/// 谓词错误：编译期与求值期错误的统一传递。
#[derive(Debug, Clone)]
pub enum PredicateError {
    /// CEL 源码无法解析（compile 失败，含 cel-parser 内部 panic 的兜底归一）。携带报文。
    Compile(String),
    /// 求值失败：未声明引用 / 类型不符 / 除零 等，原样透传 cel 的 `ExecutionError`。
    Eval(ExecutionError),
    /// 表达式求值结果不是布尔值——谓词必须产出 bool（防止把任意表达式误当条件用）。
    NotBoolean(Value),
}

impl fmt::Display for PredicateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PredicateError::Compile(msg) => write!(f, "CEL 编译失败: {msg}"),
            PredicateError::Eval(e) => write!(f, "CEL 求值失败: {e}"),
            PredicateError::NotBoolean(v) => write!(f, "CEL 谓词结果非布尔: {v:?}"),
        }
    }
}

impl std::error::Error for PredicateError {}

/// 在 `catch_unwind` 下运行 `f`：cel 的内部 panic 归一为 `Err(())`，由调用方转成
/// `PredicateError`。见模块级"健壮性注记"。
fn run_captured<R>(f: impl FnOnce() -> R) -> Result<R, ()> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|_| ())
}

/// 一个已编译的内嵌谓词：compile 一次，eval 多次（编译与求值分离，契合高频判定热路径）。
#[derive(Debug)]
pub struct Predicate {
    program: Program,
    source: String,
}

impl Predicate {
    /// 编译一条 CEL 表达式。解析失败 → `PredicateError::Compile`（错误向上传递，不 panic）；
    /// cel-parser 的内部 panic 也在此兜底归一为 `Compile`。
    pub fn compile(source: &str) -> Result<Self, PredicateError> {
        match run_captured(|| Program::compile(source)) {
            Ok(Ok(program)) => Ok(Self {
                program,
                source: source.to_owned(),
            }),
            Ok(Err(e)) => Err(PredicateError::Compile(e.to_string())),
            Err(()) => Err(PredicateError::Compile(format!(
                "CEL 解析器内部 panic（cel-parser 0.10.1 对尾部运算符/空输入触发 antlr4rust 错误恢复 panic，已兜底归一）: {source:?}"
            ))),
        }
    }

    /// 原始表达式串（诊断 / 结构化日志用）。
    pub fn source(&self) -> &str {
        &self.source
    }

    /// 在给定变量绑定下求值，产出布尔判定。
    ///
    /// 便捷入口：内部构建一次性 `Context`。需要复用上下文（高性能热路径）用 [`eval_with`]。
    ///
    /// [`eval_with`]: Predicate::eval_with
    pub fn eval(&self, vars: &HashMap<String, Value>) -> Result<bool, PredicateError> {
        let mut ctx = Context::default();
        for (name, value) in vars {
            ctx.add_variable_from_value(name.as_str(), value.clone());
        }
        self.eval_with(&ctx)
    }

    /// 在原生 cel `Context` 下求值（复用已构建的上下文，避免重复装订变量）。
    ///
    /// - 求值错误（未声明变量 / 类型不符 / 除零 等）→ `PredicateError::Eval`（透传）。
    /// - 结果非布尔 → `PredicateError::NotBoolean`。
    /// - 求值器内部 panic → 兜底归一为 `PredicateError::Eval`（不击穿调用方）。
    pub fn eval_with(&self, ctx: &Context<'_>) -> Result<bool, PredicateError> {
        match run_captured(|| self.program.execute(ctx)) {
            Ok(Ok(Value::Bool(b))) => Ok(b),
            Ok(Ok(other)) => Err(PredicateError::NotBoolean(other)),
            Ok(Err(e)) => Err(PredicateError::Eval(e)),
            Err(()) => Err(PredicateError::Eval(ExecutionError::function_error(
                "<predicate>",
                "CEL 求值器内部 panic（已兜底归一）",
            ))),
        }
    }
}
