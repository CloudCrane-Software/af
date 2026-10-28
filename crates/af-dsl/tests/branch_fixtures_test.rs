//! branches.yaml 分支覆盖夹具（G0-CI 机制批 · 子代理 D，2026-10-28）——
//! SPEC-1 §⑤「待补夹具」两处的最小可用集（登记见 ci/branches.yaml 同 id 条目）：
//!
//! ① `INV1-CTX-FROM-DEFAULT-SKIP`（validate/inv1.rs:333 `_ => {}` 兜底臂）：
//!    五词表外 from 值 → M2 整段跳过 + M1 check_vocab 出 E1004 的**成对**断言
//!    （正例走臂：跳过且仅 E1004；负例不被吞：词表内值由本臂 judged 出 E2301）。
//!    漂移闸语义：SECTION_FROMS（shape.rs 词表）与 inv1 match 臂若不同步（新增
//!    第六词只改一侧），词表内段会被兜底臂静默吞掉 → 负例由绿转红（复盘 C4
//!    升级⑤「按已想到的形状枚举」+ 语义行「新增第六词时此臂静默吞段」）。
//!
//! ② `M4-INLINE-PRIM-SYNTH`（validate/m4.rs:393 `PortType::Inline(p) => match
//!    p.r#type`）：内联原始型四臂逐臂合成默认值（String→""/Integer→0/
//!    Boolean→true/Number→0.0）的参数化夹具——正例钉值（谓词可求值为布尔即
//!    放行）、负例钉型（非布尔三臂裸绑定 → E1053 且点名结果型）。
//!    如实登记：Boolean 臂合成值为布尔，裸绑定恰是合法谓词——无 E1053 负形，
//!    该臂由正例钉死（合成错型则正例红），不编造第四条负例。
//!
//! 夹具↔分支对账由 ci/branches_check.py 每 CI 常跑执行（test 名须在
//! `cargo test --workspace -- --list` 名单内，缺席即红）。

use af_dsl::{load_str, validate, Document, Report};

fn run(src: &str) -> Report {
    let doc: Document = load_str(src).unwrap_or_else(|e| panic!("解析失败：{e}"));
    validate(&doc)
}

fn codes(src: &str) -> Vec<&'static str> {
    run(src).codes()
}

/// (code, message) 对（逐字断言用）。
fn msgs(src: &str) -> Vec<(&'static str, String)> {
    run(src)
        .violations
        .iter()
        .map(|v| (v.code.code(), v.message.clone()))
        .collect()
}

// ---------- ① INV1-CTX-FROM-DEFAULT-SKIP（inv1.rs:333 兜底臂） ----------------

/// 最小骨架：entry n1（task，emits answer:a）带单条 context.sections（调用方给）。
fn ctx_doc(section: &str) -> String {
    format!(
        "version: 1\n\
meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: {{ ref: a, from: n1 }}\n\
types:\n  - {{ name: in_t, schema: {{ type: object, required: [q], properties: {{ q: {{ type: string }} }} }} }}\n  - {{ name: a, schema: {{ type: object }} }}\n\
nodes:\n  - id: n1\n    kind: task\n    emits: {{ answer: a }}\n    context:\n      sections:\n        - {section}\n\
edges: []\n"
    )
}

#[test]
fn ctx_from_outside_vocab_skips_silently_and_shape_reports_e1004() {
    // 正例（走兜底臂）：from=recall 不在五词表 → inv1 整段跳过（`_ => {}` 不产
    // M2 码），违例仅 M1 check_vocab 的 E1004——「跳过」与「词表层定罪」同时
    // 在场，缺一侧即红。
    let src = ctx_doc("{ from: recall, quota_tokens: 100 }");
    assert_eq!(codes(&src), ["E1004"]);
    assert_eq!(
        msgs(&src),
        [(
            "E1004",
            "context.sections[0] 来源 \"recall\" 不在闭环词表 [\"template\", \
             \"input\", \"arg\", \"edge\", \"store\"]"
                .to_string()
        )]
    );
}

#[test]
fn ctx_from_in_vocab_is_judged_not_swallowed_by_fallback() {
    // 负例（不被兜底臂吞）：from=template ∈ 五词表 → 由 template 臂 judged（缺
    // template_ref → E2301）且不触 E1004——词表两侧（SECTION_FROMS ↔ inv1
    // match 臂）若漂移（词表收词、臂没收），本段会被静默吞掉 → 由绿转红。
    let src = ctx_doc("{ from: template, quota_tokens: 100 }");
    assert_eq!(codes(&src), ["E2301"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2301",
            "node 'n1' section[0] from 'template' 缺来源字段 template_ref".to_string()
        )]
    );
    // 同文书反证：词表内合法段（带 template_ref）由 template 臂处理放行——
    // 既不被兜底臂吞掉（无码），也不被误判（无 E2301）。
    let src_ok = ctx_doc("{ from: template, template_ref: pkg.t, quota_tokens: 100 }");
    assert!(
        codes(&src_ok).is_empty(),
        "template 臂应处理合法段并放行：\n{}",
        run(&src_ok)
    );
}

// ---------- ② M4-INLINE-PRIM-SYNTH（m4.rs:393 内联原始型四臂） ----------------

/// 最小骨架：n0（task，emits 一个内联原始型端口 p）→ n2（gate，consumes 同型
/// 端口 p，assert 引用 inputs.p 合成值）。边类型相等核对（refs EdgePortType-
/// Mismatch）要求 consumes 声明与生产者 emit 同型，故两侧同给 `{ type: K }`。
fn inline_doc(prim: &str, assert_expr: &str) -> String {
    format!(
        "version: 1\n\
meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: {{ ref: a, from: n1 }}\n\
types:\n  - {{ name: in_t, schema: {{ type: object, required: [q], properties: {{ q: {{ type: string }} }} }} }}\n  - {{ name: a, schema: {{ type: object }} }}\n\
nodes:\n  - {{ id: n0, kind: task, emits: {{ p: {{ type: {prim} }} }} }}\n  - {{ id: n1, kind: task, emits: {{ answer: a }} }}\n  - id: n2\n    kind: gate\n    consumes: {{ p: {{ type: {prim} }} }}\n    emits: {{ checked: a }}\n    assert: \"{assert_expr}\"\n\
edges:\n  - {{ from: n0, output: p, to: n2, input: p }}\n\
policies: {{ budgets: [], stop: [], failures: [] }}\n"
    )
}

#[test]
fn inline_primitive_ports_synthesize_kind_defaults() {
    // 正例（逐臂钉值）：内联原始型端口按 PrimitiveKind 合成默认值并进入
    // inputs 绑定（assert_env → synth_port）——谓词按合成值可求值为布尔即放行。
    // 任一臂合成缺失/错值 → 对应行由绿转红（复盘 C4 升级①逐臂矩阵）。
    let cases = [
        ("string", "inputs.p == ''"),
        ("integer", "inputs.p == 0"),
        ("number", "inputs.p == 0.0"),
        ("boolean", "inputs.p"),
    ];
    for (prim, expr) in cases {
        let src = inline_doc(prim, expr);
        assert!(
            codes(&src).is_empty(),
            "内联 {prim} 臂合成默认值应放行 {expr:?}：\n{}",
            run(&src)
        );
    }
}

#[test]
fn inline_primitive_non_boolean_synth_is_e1053() {
    // 负例（逐臂钉型）：非布尔三臂的合成值作裸谓词 → E1053 且点名结果型
    // （string/int/float）——合成错型（如 Integer 臂产出字符串）即红。
    // Boolean 臂无此负形（合成值为布尔，裸绑定即合法谓词）——由正例钉死，
    // 此处不列第四条，避免编造（见文件头注）。
    let cases = [
        ("string", "string"),
        ("integer", "int"),
        ("number", "float"),
    ];
    for (prim, kind) in cases {
        let src = inline_doc(prim, "inputs.p");
        assert_eq!(
            codes(&src),
            ["E1053"],
            "内联 {prim} 臂裸绑定应判 E1053：\n{}",
            run(&src)
        );
        assert_eq!(
            msgs(&src),
            [(
                "E1053",
                format!("node 'n2' assert CEL 谓词求值非布尔：结果为 {kind}")
            )]
        );
    }
}
