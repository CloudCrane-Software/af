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
    let vote = vmap(&[(
        "stats",
        vmap(&[("top_count", Value::Int(0)), ("answers", Value::Int(1))]),
    )]);
    let observation = vmap(&[("observation", vmap(&[("ok", Value::Bool(true))]))]);
    let resume = vmap(&[("approved", Value::Bool(true))]);
    let inputs_report_check = vmap(&[
        ("report", vmap(&[("ok", Value::Bool(true))])),
        ("check", vmap(&[("pass", Value::Bool(true))])),
    ]);
    let inputs_result = vmap(&[("result", vmap(&[("top_share_milli", Value::Int(0))]))]);
    let fixtures: &[(&str, Value)] = &[
        // s1/policies/stop[0]
        (
            "last_tool.first_line in ['FINISH', 'FINAL ANSWER']",
            last_tool_env(),
        ),
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
        (
            "budget.cents_spent >= budget.cents_cap * 9 / 10",
            budget_env(),
        ),
        // s5/format_check assert
        ("inputs.observation.ok == true", observation),
        // s5/format_check route rules[0]
        (
            "format_check.passed == false",
            vmap(&[("passed", Value::Bool(true))]),
        ),
        // s5/approve route rules[0]
        ("resume.approved == false", resume.clone()),
        // s5/loop exit_when
        ("resume.approved == true", resume),
        // s6/admit assert
        ("inputs.report.ok && inputs.check.pass", inputs_report_check),
        // s6/admit route rules[0]
        (
            "admit.passed == false",
            vmap(&[("passed", Value::Bool(true))]),
        ),
        // s6/acquire loop exit_when
        ("attempt.passed == true", attempt),
        // s6/explore loop exit_when
        (
            "budget.cents_spent >= budget.cents_cap * 9 / 10",
            budget_env(),
        ),
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
    assert_eq!(
        e.detail,
        "非算子函数/方法调用 \"contains\"（函数与方法调用一律拒绝）"
    );
}

// ---------- 反例 2：非算子函数 size（E1052，§4.7 反向对照） ----------

#[test]
fn size_function_rejected_as_e1052() {
    let e = err_of(
        "size(input.personas) > 0",
        &env(&[("input", vmap(&[("personas", Value::Int(0))]))]),
    );
    assert_eq!(e.code.code(), "E1052");
    assert_eq!(
        e.detail,
        "非算子函数/方法调用 \"size\"（函数与方法调用一律拒绝）"
    );
}

// ---------- 反例 3：保守子集外算子（三元 / 一元负号 → E1052） ----------

#[test]
fn ternary_and_negate_not_in_conservative_subset() {
    let e = err_of(
        "steps.turns > 1 ? true : false",
        &env(&[("steps", steps_env())]),
    );
    assert_eq!(e.code.code(), "E1052");
    assert_eq!(
        e.detail,
        "非算子函数/方法调用 \"_?_:_\"（函数与方法调用一律拒绝）"
    );

    let e = err_of("-(steps.turns) < 0", &env(&[("steps", steps_env())]));
    assert_eq!(e.code.code(), "E1052");
    assert_eq!(
        e.detail,
        "非算子函数/方法调用 \"-_\"（函数与方法调用一律拒绝）"
    );
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

    let e = err_of(
        "last_tool.first_line",
        &env(&[("last_tool", last_tool_env())]),
    );
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
    let e = err_of(
        "ghost.x && steps.turns + 1",
        &env(&[("steps", steps_env())]),
    );
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

// ---------- E1054 前置闸（农田修复批 P0-CEL，ANOMALIES.md A-2） ----------

/// 深度边界：>1000（农田实测 8MB 栈深 1000 优雅过/2000 abort，上限取实测
/// 过界值）在下潜解析前拒绝——超限侧在任何栈上都安全（闸先于 antlr）；
/// 边界内样本的完整解析须在大栈线程跑（antlr 递归下降每层吃数 KB 栈，
/// libtest 默认线程栈更小）。
fn on_big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(f)
        .expect("spawn 256MB-stack thread")
        .join()
        .expect("big-stack thread panicked")
}

#[test]
fn nesting_depth_over_limit_rejected_as_e1054() {
    let over = format!("{}1{}", "(".repeat(1001), ")".repeat(1001));
    let err = err_of(&over, &CelEnv::new());
    assert_eq!(err.code.code(), "E1054", "深 1001 应前置闸拒绝：{err:?}");
    // 边界内最深（1000）不触闸：32MB 栈上完整走 parse（结果 E1053 非布尔——
    // `(((1)))` 是 Int——恰恰证明它过了闸、进了判定管道）。
    let at_limit = format!("{}1{}", "(".repeat(1000), ")".repeat(1000));
    let code = on_big_stack(move || err_of(&at_limit, &CelEnv::new()).code.code().to_string());
    assert_ne!(
        code, "E1054",
        "深恰 1000 不触闸（应走到 E1053 非布尔）：{code}"
    );
    assert_eq!(
        code, "E1053",
        "深 1000 完整解析后按非布尔判（闸外无异常）：{code}"
    );
}

/// 长度边界：>64 KiB 拒（崩溃样本 400KB/900KB 量级、常规表达式 ≤155 字节，
/// 两侧各两个数量级余量）；恰在上限内的合法表达式（单字符串字面量比较，
/// antlr 单 token、栈安全）照常通过全判定。
#[test]
fn expr_bytes_over_limit_rejected_as_e1054() {
    let e = CelEnv::new();
    let over = format!(
        "\"{}\" != \"\"",
        "x".repeat(af_dsl::cel::MAX_EXPR_BYTES - 7)
    );
    assert!(over.len() == af_dsl::cel::MAX_EXPR_BYTES + 1);
    let err = err_of(&over, &e);
    assert_eq!(err.code.code(), "E1054", "超一字节应前置闸拒绝：{err:?}");

    let at_limit = format!(
        "\"{}\" != \"\"",
        "x".repeat(af_dsl::cel::MAX_EXPR_BYTES - 8)
    );
    assert!(at_limit.len() == af_dsl::cel::MAX_EXPR_BYTES);
    check(&at_limit, &e).unwrap_or_else(|err| panic!("恰在上限内应通过：{err:?}"));
}

/// 字符串字面量内的括号不计嵌套深度（字面量感知扫描）。
#[test]
fn brackets_inside_string_literals_do_not_count_depth() {
    let e = CelEnv::new();
    let src = format!("\"{}\" != \"\"", "(".repeat(2000));
    // 长度也在限内（2001 < 64KiB），字面量括号不计 → 过闸，解析比较为布尔。
    check(&src, &e).unwrap_or_else(|err| panic!("字面量内括号不计深：{err:?}"));
}

// ---------- ②' has() 宏补位（农田修复批 P1-HAS-MACRO，ANOMALIES.md B-3） ----------

/// has() 经 cel-parser 宏展开为 Select{test:true}，AST 不登记函数名——
/// functions() 白名单拦不住，词法补位拦（E1052，码位不变）。
#[test]
fn has_macro_rejected_as_e1052() {
    let e = env(&[("input", vmap(&[("q", vstr("x"))]))]);
    for src in [
        "has(input.q)",
        "has (input.q)",
        "input.q == \"has(x)\" && has(input.q)",
    ] {
        let err = err_of(src, &e);
        assert_eq!(err.code.code(), "E1052", "{src} 应拦宏：{err:?}");
    }
    // 对照：字符串字面量里的 "has(" 不是宏调用，不误伤。
    check("\"has(input.q)\" != \"\"", &e)
        .unwrap_or_else(|err| panic!("字面量内 has( 不误伤：{err:?}"));
    // 对照：标识符仅以 has 结尾（非裸 has）不是宏。
    check("input.q == \"x\"", &e).unwrap_or_else(|err| panic!("常规表达式不误伤：{err:?}"));
}

// ---------- 红队修复批（W1）：词法扫描跳过 CEL 注释 ----------

#[test]
fn comments_are_skipped_by_lex_scan() {
    // cel-parser 0.10.1 文法只收 // 行注释（CEL.g4：COMMENT : '//' …）；
    // 词法扫描（括号深度与 has() 宏补位）须与解析器同口径跳过——注释
    // 文本中的 has( 与括号不得计入（此前中文注释里 has( 误报 E1052、
    // 注释括号误计深度）。
    let e = env(&[("input", vmap(&[("q", vstr("x"))]))]);
    for src in [
        "true // nothing here",
        "true // has(input.q)",
        "true // (((((",
        "input.q == \"x\" // 中文注释里有 has( 与 (((( 括号",
    ] {
        check(src, &e).unwrap_or_else(|err| panic!("行注释内词法不误伤 {src:?}：{err:?}"));
    }
    // 对照：除号不是注释起点；字符串字面量里的 // 不是注释。
    check("1 / 2 == 0", &e).unwrap_or_else(|err| panic!("除号不误伤：{err:?}"));
    check("input.q == \"// has(\"", &e).unwrap_or_else(|err| panic!("字面量内 // 不误伤：{err:?}"));
    err_of("input.q == \"// has(\" && has(input.q)", &e);
    // 对照：真宏调用仍拦（E1052）；块注释不在文法内（解析必拒 E1051，
    // 词法面不得跳过——跳过即给深括号绕过 E1054 前置闸开门）。
    let err = err_of("has(input.q) // 尾注", &e);
    assert_eq!(err.code.code(), "E1052");
    let err = err_of("/* has(input.q) */ true", &e);
    assert_eq!(err.code.code(), "E1051");
}

// ---------- 红队修复批（W2）：has 宏不得被 // 行注释隔断绕闸 ----------

#[test]
fn has_macro_across_line_comment_is_still_rejected() {
    // 红队 W2-C24：`has //注释\n(...)`——词法扫描进入行注释时曾清掉
    // pending_has，注释后的 `(` 不再置 has_macro，保守子集闸被绕
    // （cel-parser 把注释作 HIDDEN 通道照常解析出 has 宏）。
    let e = env(&[("input", vmap(&[("q", vstr("x"))]))]);
    for src in [
        "has //comment\n(input.q)",
        "has//c\n(input.q)",
        "has // 中文注释\n(input.q)",
    ] {
        let err = err_of(src, &e);
        assert_eq!(err.code.code(), "E1052", "{src:?} 应拦宏：{err:?}");
    }
    // 对照：注释后的空白不改变判定（pending_has 跨越注释保持）。
    // 对照：除号不是注释——`has / x` 之后 ident 已清，不构成宏隔断误判面，
    // 真正的宏调用（无注释）仍由既有用例钉住。
    check("true // has(input.q)", &e)
        .unwrap_or_else(|err| panic!("注释文本内 has( 不误伤（W1-C23 口径不回退）：{err:?}"));
}

#[test]
fn deep_expression_parses_on_default_stack_thread() {
    // 红队 W3-C16：默认（小）栈线程上深 1000 表达式必须被解析/拒绝，而非
    // 栈溢出 SIGABRT——修复前 antlr4rust 递归下降跑在调用方栈上，debug 构建
    // 8MB 主栈深约 100 起崩（农田 C_CE29 深 1000 PASS 样本原依赖 256MB 大栈
    // 测试线程达成，生产 CLI 主线程无保护）。解析已改 64MB 大栈工作线程，
    // 本测试刻意不包大栈：深度 ≤1000 合法过、>1000 走 E1054 前置拒。
    let e = env(&[("input", vmap(&[("q", vstr("x"))]))]);
    // 深 1000（恰阈值内）：((((…1…)))) 语义 = 1，非布尔→E1053（不是崩溃）。
    let deep_ok = format!("{}1{}", "(".repeat(1000), ")".repeat(1000));
    let err = err_of(&deep_ok, &e);
    assert_ne!(
        err.code.code(),
        "E1051",
        "深 1000 不应解析失败（栈溢出会体现为 panic/abort）：{err:?}"
    );
    // 深 1200（阈值外）：前置闸 E1054。
    let deep_over = format!("{}1{}", "(".repeat(1200), ")".repeat(1200));
    let err = err_of(&deep_over, &e);
    assert_eq!(err.code.code(), "E1054", "深 1200 应被前置闸拒：{err:?}");
}
