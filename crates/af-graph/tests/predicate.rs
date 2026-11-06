//! 内嵌谓词（CEL）求值封装的语义测试：正例 / 反例 / 错误传递。
//!
//! 依据 poc-cue/RESULT.md ④-6 + verify-load/REPORT.md ④-2：数据相关运行时谓词由 CEL
//! 在 Rust 内嵌求值。本文件验证 `Predicate` 的最小契约：compile + eval + 错误传递。

use af_graph::{Predicate, PredicateError};
use cel_interpreter::{Context, Value};
use std::collections::HashMap;

fn vars(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

// ---------- 正例：编译通过 + 求值为 true ----------

#[test]
fn compiles_and_evaluates_true() {
    let p = Predicate::compile("count > 0").expect("合法表达式应编译通过");
    assert_eq!(p.source(), "count > 0");
    let ok = p
        .eval(&vars(&[("count", Value::Int(3))]))
        .expect("求值应成功");
    assert!(ok, "count=3 应判 true");
}

#[test]
fn gate_like_predicate_over_event_data() {
    // 贴近图模板 gate 节点：对事件字段做阈值 + 采样数联合判定。
    let p = Predicate::compile("defect_ratio < 0.05 && samples >= 3").unwrap();
    let ok = p
        .eval(&vars(&[
            ("defect_ratio", Value::Float(0.02)),
            ("samples", Value::Int(10)),
        ]))
        .unwrap();
    assert!(ok, "0.02<0.05 且 10>=3 应判 true");
}

// ---------- 反例：求值为 false ----------

#[test]
fn evaluates_false() {
    let p = Predicate::compile("count > 0").unwrap();
    let ok = p
        .eval(&vars(&[("count", Value::Int(-1))]))
        .expect("求值应成功");
    assert!(!ok, "count=-1 应判 false");
}

// ---------- 错误传递：编译失败 / 求值失败 / 非布尔 ----------

#[test]
fn compile_error_native_is_passed_through() {
    // 畸形表达式（多出token/缺右括号）由 cel-parser 原生返回 ParseErrors → Compile。
    for src in ["(1 + 2", "1 2 3", "1 +* 2", "@bad"] {
        let err = Predicate::compile(src).unwrap_err();
        assert!(
            matches!(err, PredicateError::Compile(_)),
            "{src:?} 应报编译错误，实际: {err:?}"
        );
    }
}

#[test]
fn compile_panic_is_caught_and_passed_through() {
    // cel-parser 0.10.1 对"尾部运算符 / 空输入"会内部 panic（antlr4rust 错误恢复缺陷）。
    // 封装必须兜底归一为 Compile 而非让 panic 击穿调用方——否则畸形谓词串会击沉运行时。
    for src in ["1 +", "count >", "", "   "] {
        let err = Predicate::compile(src).unwrap_err();
        assert!(
            matches!(err, PredicateError::Compile(_)),
            "{src:?} 的解析器 panic 应被兜底归一为 Compile，实际: {err:?}"
        );
    }
}

#[test]
fn eval_error_undeclared_variable_is_passed_through() {
    // 未声明变量是求值期错误（CEL 解析期不查绑定），经 PredicateError::Eval 透传。
    let p = Predicate::compile("missing > 0").unwrap();
    let err = p.eval(&vars(&[])).unwrap_err();
    assert!(
        matches!(err, PredicateError::Eval(_)),
        "未声明变量应报求值错误，实际: {err:?}"
    );
}

#[test]
fn non_boolean_result_is_rejected() {
    // 谓词必须产出 bool；把算术表达式当条件用要被拒。
    let p = Predicate::compile("1 + 1").unwrap();
    let err = p.eval(&vars(&[])).unwrap_err();
    assert!(
        matches!(err, PredicateError::NotBoolean(_)),
        "非布尔结果应被拒，实际: {err:?}"
    );
}

#[test]
fn eval_with_reuses_native_context() {
    // eval_with：复用原生 cel Context，结果与 eval 一致（热路径零重复装订）。
    let p = Predicate::compile("count > 0").unwrap();
    let mut ctx = Context::default();
    ctx.add_variable_from_value("count", Value::Int(5));
    assert!(p.eval_with(&ctx).unwrap(), "count=5 应判 true");
}
