//! CEL 保守子集静态判定验收测试（M4，提案 B §1.4 + §4.7 + §6）：
//! 三项判定 = ① `Program::compile` 可解析（E1051）② `references().functions()
//! ⊆ 算子白名单`（E1052）③ `references().variables() ⊆ 位置词表`（E2203）且
//! 在合成绑定上下文上求值为布尔（E1053）。正例源 = §4.7 的 16 条实跑清单
//! （六个样例的全部 when/exit_when/assert 值）；反例 = §4.7 反向对照
//! （size 函数 / 未知根 / 非布尔）+ 方法调用 + 解析失败（含 cel-parser
//! panic 兜底）+ 保守子集外算子（三元/负号）。

use af_dsl::cel::{check, CelEnv, CelError, OPERATOR_WHITELIST};
use cel_interpreter::Value;
use std::collections::HashMap;

fn vmap(pairs: &[(&str, Value)]) -> Value {
    let mut hm: HashMap<String, Value> = HashMap::new();
    for (k, v) in pairs {
        hm.insert((*k).to_string(), v.clone());
    }
    hm.into()
}

fn env(pairs: &[(&str, Value)]) -> CelEnv {
    let mut e = CelEnv::new();
    for (k, v) in pairs {
        e.add(k, v.clone());
    }
    e
}

fn vstr(s: &str) -> Value {
    Value::String(s.to_string().into())
}

/// §1.4 词汇表建模：steps {turns,tool_free,tool_calls_last_turn,iterations}:Int。
fn steps_env() -> Value {
    vmap(&[
        ("turns", Value::Int(0)),
        ("tool_free", Value::Int(0)),
        ("tool_calls_last_turn", Value::Int(0)),
        ("iterations", Value::Int(0)),
    ])
}

/// §1.4 词汇表建模：budget {<dimension>_spent/<dimension>_cap}，8 维闭环
/// （BudgetDimension 词表）。
fn budget_env() -> Value {
    let mut pairs = Vec::new();
    for dim in [
        "turns",
        "model_calls",
        "tokens_in",
        "tokens_out",
        "tokens_total",
        "tool_calls",
        "cents",
        "clock_ms",
    ] {
        pairs.push((format!("{dim}_spent"), Value::Int(0)));
        pairs.push((format!("{dim}_cap"), Value::Int(1)));
    }
    let mut hm: HashMap<String, Value> = HashMap::new();
    for (k, v) in pairs {
        hm.insert(k, v);
    }
    hm.into()
}

fn last_tool_env() -> Value {
    vmap(&[
        ("name", vstr("")),
        ("first_line", vstr("FINISH")),
        ("ok", Value::Bool(true)),
    ])
}

fn err_of(source: &str, env: &CelEnv) -> CelError {
    check(source, env).expect_err("应被拒绝")
}

// ---------- 白名单常量 = §1.4 逐字（含 `in` → `@in` 实测修正） ----------

#[test]
fn operator_whitelist_locks_spec_fifteen() {
    assert_eq!(
        OPERATOR_WHITELIST,
        &[
            "_==_", "_!=_", "_<_", "_<=_", "_>_", "_>=_", "_&&_", "_||_", "!_", "_+_", "_-_",
            "_*_", "_/_", "_%_", "@in",
        ]
    );
}

// ---------- 正例：§4.7 的 16 条实跑清单（合成绑定全过三项判定） ----------

#[test]
fn sixteen_legal_fixtures_pass_all_three_judgments() {
    // (表达式, 允许根 + 合成值)。逐条对应 §4.7 检查 2 的 PASS 行。
    let qa = vmap(&[("passed", Value::Bool(true))]);
    let attempt = vmap(&[("passed", Value::Bool(true))]);
    let tally = vmap(&[("result", vmap(&[("top_share_milli", Value::Int(0))]))]);
    let vote = vmap(&[("stats", vmap(&[("top_count", Value::Int(0)), ("answers", Value::Int(1))]))]);
    let observation = vmap(&[("observation", vmap(&[("ok", Value::Bool(true))]))]);
    let resume = vmap(&[("approved", Value::Bool(true))]);
    let inputs_report_check =
        vmap(&[("report", vmap(&[("ok", Value::Bool(true))])), ("check", vmap(&[("pass", Value::Bool(true))]))]);
    let inputs_result = vmap(&[("result", vmap(&[("top_share_milli", Value::Int(0))]))]);
    let fixtures: &[(&str, Value)] = &[
        // s1/policies/stop[0]
        ("last_tool.first_line in ['FINISH', 'FINAL ANSWER']", last_tool_env()),
        // s1/policies/stop[1]
        ("steps.tool_free >= 2", steps_env()),
        // s2/qa route rules[0]
        ("qa.passed == false", qa.clone()),
        // s2/loop exit_when
        ("attempt.passed == true", attempt.clone()),
        // s3/converge_gate assert
        (
            "inputs.result.top_share_milli >= 0 && inputs.result.top_share_milli <= 1000",
            inputs_result.clone(),
        ),
        // s3/loop exit_when
        ("tally.result.top_share_milli >= 700", tally),
        // s3/vote route rules[0]
        ("vote.stats.top_count * 2 > vote.stats.answers", vote),
        // s4/loop exit_when
        ("budget.cents_spent >= budget.cents_cap * 9 / 10", budget_env()),
        // s5/format_check assert
        ("inputs.observation.ok == true", observation),
        // s5/format_check route rules[0]
        ("format_check.passed == false", vmap(&[("passed", Value::Bool(true))])),
        // s5/approve route rules[0]
        ("resume.approved == false", resume.clone()),
        // s5/loop exit_when
        ("resume.approved == true", resume),
        // s6/admit assert
        ("inputs.report.ok && inputs.check.pass", inputs_report_check),
        // s6/admit route rules[0]
        ("admit.passed == false", vmap(&[("passed", Value::Bool(true))])),
        // s6/acquire loop exit_when
        ("attempt.passed == true", attempt),
        // s6/explore loop exit_when
        ("budget.cents_spent >= budget.cents_cap * 9 / 10", budget_env()),
    ];
    assert_eq!(fixtures.len(), 16, "§4.7 清单恰 16 条");
    for (src, binding) in fixtures {
        let c = check(src, &env(&[(root_of(src), binding.clone())]))
            .unwrap_or_else(|e| panic!("{src} 应过三项判定：{e:?}"));
        assert!(c.ok, "{src}");
        assert_eq!(
            c.roots,
            std::iter::once(root_of(src).to_string()).collect::<std::collections::BTreeSet<_>>(),
            "{src} 的引用根集"
        );
    }
}

/// 夹具表达式的根绑定名（每条恰一个根，且都在表达式开头）。
fn root_of(expr: &str) -> &str {
    expr.split('.').next().unwrap_or(expr)
}

// ---------- 反例 1：方法调用（E1052） ----------

#[test]
fn method_call_rejected_as_e1052() {
    let e = err_of(
        "'FINISH'.contains(last_tool.first_line)",
        &env(&[("last_tool", last_tool_env())]),
    );
    assert_eq!(e.code.code(), "E1052");
    assert_eq!(e.detail, "非算子函数/方法调用 \"contains\"（函数与方法调用一律拒绝）");
}

// ---------- 反例 2：非算子函数 size（E1052，§4.7 反向对照） ----------

#[test]
fn size_function_rejected_as_e1052() {
    let e = err_of(
        "size(input.personas) > 0",
        &env(&[("input", vmap(&[("personas", Value::Int(0))]))]),
    );
    assert_eq!(e.code.code(), "E1052");
    assert_eq!(e.detail, "非算子函数/方法调用 \"size\"（函数与方法调用一律拒绝）");
}

// ---------- 反例 3：保守子集外算子（三元 / 一元负号 → E1052） ----------

#[test]
fn ternary_and_negate_not_in_conservative_subset() {
    let e = err_of(
        "steps.turns > 1 ? true : false",
        &env(&[("steps", steps_env())]),
    );
    assert_eq!(e.code.code(), "E1052");
    assert_eq!(e.detail, "非算子函数/方法调用 \"_?_:\"（函数与方法调用一律拒绝）");

    let e = err_of("-(steps.turns) < 0", &env(&[("steps", steps_env())]));
    assert_eq!(e.code.code(), "E1052");
    assert_eq!(e.detail, "非算子函数/方法调用 \"-_\"（函数与方法调用一律拒绝）");
}

// ---------- 反例 4：未知根绑定（E2203，§4.7 反向对照） ----------

#[test]
fn unknown_root_rejected_as_e2203() {
    let e = err_of(
        "unknown_root.x == 1",
        &env(&[("steps", steps_env()), ("budget", budget_env())]),
    );
    assert_eq!(e.code.code(), "E2203");
    assert_eq!(e.detail, "根绑定 \"unknown_root\" 不在位置词表内");
}

// ---------- 反例 5：非布尔结果（E1053，§4.7 反向对照） ----------

#[test]
fn non_boolean_rejected_as_e1053() {
    let e = err_of("steps.turns + 1", &env(&[("steps", steps_env())]));
    assert_eq!(e.code.code(), "E1053");
    assert_eq!(e.detail, "谓词求值非布尔：结果为 int");

    let e = err_of("last_tool.first_line", &env(&[("last_tool", last_tool_env())]));
    assert_eq!(e.code.code(), "E1053");
    assert_eq!(e.detail, "谓词求值非布尔：结果为 string");
}

// ---------- 反例 6：解析失败 + cel-parser panic 兜底（E1051） ----------

#[test]
fn parse_failure_and_parser_panic_both_normalized_to_e1051() {
    // 尾部运算符/空输入触发 antlr4rust 错误恢复 panic（af-graph predicate.rs
    // 实测注记）——catch_unwind 兜底归一为 E1051，不击穿调用方。
    for src in ["1 +", "", "steps.turns >="] {
        let e = err_of(src, &env(&[("steps", steps_env())]));
        assert_eq!(e.code.code(), "E1051", "{src:?}");
    }
    // 普通解析失败（非 panic 路径）同样归 E1051。
    let e = err_of("a b c", &env(&[("a", Value::Bool(true))]));
    assert_eq!(e.code.code(), "E1051");
}

// ---------- 判定序：解析 → 算子 → 根 → 布尔（首个违例即返） ----------

#[test]
fn judgment_order_is_parse_then_operators_then_roots_then_boolean() {
    // 同时踩 E1052 与 E2203 → 只报 E1052（判定序靠前）。
    let e = err_of("size(ghost) > 0", &env(&[("steps", steps_env())]));
    assert_eq!(e.code.code(), "E1052");
    // 同时踩 E2203 与（对合法根才会显现的）E1053 → 只报 E2203。
    let e = err_of("ghost.x && steps.turns + 1", &env(&[("steps", steps_env())]));
    assert_eq!(e.code.code(), "E2203");
}

// ---------- 全部 15 白名单算子逐一放行（表达式过三项判定） ----------

#[test]
fn every_whitelisted_operator_is_accepted() {
    let e = env(&[
        ("a", Value::Int(1)),
        ("b", Value::Int(2)),
        ("s", vstr("x")),
        ("xs", Value::List(vec![vstr("x")].into())),
        ("t", Value::Bool(true)),
        ("f", Value::Bool(false)),
    ]);
    for src in [
        "a == b",
        "a != b",
        "a < b",
        "a <= b",
        "a > b",
        "a >= b",
        "t && f",
        "t || f",
        "!f",
        "a + b >= 0",
        "a - b <= 0",
        "a * b >= 0",
        "a / b >= 0",
        "a % b >= 0",
        "s in xs",
    ] {
        check(src, &e).unwrap_or_else(|err| panic!("{src} 应放行：{err:?}"));
    }
}
