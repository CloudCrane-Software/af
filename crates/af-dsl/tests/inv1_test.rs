//! INV-1 上下文完整性 + context.sections 语义校验验收测试（M2，提案 B §1.2/§5/§6）：
//! 来源可解析（E2301）· 配额为正（E2302，store 段 policy:none 记 0 例外；总量上限
//! 可配置）· LLM-bound 节点禁 reads（E2303）· 入边端口闭合（E2304）·
//! store 视图策略域（E2401）· arg ∈ 所在体形参表（E2402）· template vars 引用
//! （E2403），一次收齐。
//! 正例门槛：提案 B §4 六个 golden 样例全量（M1+M2）校验干净。

use af_dsl::{load_str, validate, validate_with, Document, Report, ValidateOptions};

fn run(src: &str) -> Report {
    let doc: Document = load_str(src).unwrap_or_else(|e| panic!("解析失败：{e}"));
    validate(&doc)
}

fn check(src: &str) -> Vec<&'static str> {
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

/// 最小文档骨架：meta.entry/output 指 n1；types 含 in_t（字段 q）与 a；
/// store s（message_log，writer n1）；nodes（自带尾换行的多行块）与 edges
/// （单行 flow 形，如 "[]" / "[{ from: p, ... }]"）由调用方给。
fn doc(nodes: &str, edges: &str) -> String {
    format!(
        "version: 1\n\
meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: {{ ref: a, from: n1 }}\n\
types:\n  - {{ name: in_t, schema: {{ type: object, required: [q], properties: {{ q: {{ type: string }} }} }} }}\n  - {{ name: a, schema: {{ type: object }} }}\n\
stores:\n  - {{ name: s, type: message_log, writer: n1, lifecycle: run, retention: {{ max_items: 8 }} }}\n\
nodes:\n{nodes}edges: {edges}\n"
    )
}

/// LLM-bound 消费节点（uses.model 在场即 LLM-bound）。
fn llm(n1_body: &str) -> String {
    doc(
        &format!("  - {{ id: n1, kind: task, uses: {{ model: route.main@1 }}, emits: {{ answer: a }}, {n1_body} }}\n"),
        "[]",
    )
}

// ---------- golden 门槛：六样例全量（M1+M2）校验干净 ----------

#[test]
fn six_golden_samples_pass_full_m2_validation() {
    for i in 1..=6 {
        let src = std::fs::read_to_string(format!("testdata/sample{i}.yaml")).unwrap();
        let doc: Document = load_str(&src).unwrap_or_else(|e| panic!("sample{i} 解析失败：{e}"));
        let r = validate(&doc);
        assert!(r.is_ok(), "sample{i} 不应违例：\n{r}");
        // 可配置总量上限路径在宽松上限下同样干净（六样例单节点配额总量 ≤ 7200）。
        let r2 = validate_with(
            &doc,
            &ValidateOptions {
                context_quota_cap: Some(100_000),
                ..Default::default()
            },
        );
        assert!(r2.is_ok(), "sample{i}（cap=100000）不应违例：\n{r2}");
    }
}

// ---------- 提案 B §5 的 INV-1 违规样例（机器验证过的 golden 拒绝夹具） ----------

#[test]
fn spec_inv1_fixture_triggers_exactly_e2301_e2303() {
    // 提案 B §5 INV-1 样例：section[1] 引用 meta.input 无的 requirement（E2301）+
    // LLM 节点直读 store（E2303）。其余全干净（写者唯一、配额正、无入边端口）。
    let src = std::fs::read_to_string("testdata/reject/inv1_e2301_e2303.yaml").unwrap();
    assert_eq!(check(&src), ["E2301", "E2303"]);
    // 错误信息逐字对齐提案 §5 预期错误风格。
    assert_eq!(
        msgs(&src),
        [
            (
                "E2301",
                "node 'solver' section[1] field 'requirement' not in meta.input schema (t)"
                    .to_string()
            ),
            (
                "E2303",
                "LLM-bound node 'solver' declares reads ['trajectory']（store 只能经 context.sections 进入 agent 上下文）".to_string()
            ),
        ]
    );
}

// ---------- E2301 来源不可解析 ----------

#[test]
fn input_field_not_in_schema_is_e2301() {
    let src = llm(
        "context: { sections: [\
{ from: input, fields: [ghost], policy: project, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2301"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2301",
            "node 'n1' section[0] field 'ghost' not in meta.input schema (in_t)".to_string()
        )]
    );
}

#[test]
fn edge_port_not_incoming_is_e2301() {
    // n1 无任何入边，from:edge 段引用端口 ghost。
    let src = llm(
        "context: { sections: [\
{ from: edge, port: ghost, policy: project, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2301"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2301",
            "node 'n1' section[0] port 'ghost' not in incoming edge ports []".to_string()
        )]
    );
}

#[test]
fn store_not_declared_is_e2301() {
    let src = llm(
        "context: { sections: [\
{ from: store, store: ghost, policy: full, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2301"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2301",
            "node 'n1' section[0] store 'ghost' not declared in stores（store 名经作用域链解析：v0.1 = 顶层 stores 表）".to_string()
        )]
    );
}

#[test]
fn section_missing_source_field_is_e2301() {
    // 平铺联合体的每 from 必配来源字段：input.fields / edge.port / template.template_ref。
    let src = llm(
        "context: { sections: [\
{ from: input, quota_tokens: 100 }, \
{ from: edge, quota_tokens: 100 }, \
{ from: template, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2301", "E2301", "E2301"]);
    assert_eq!(
        msgs(&src),
        [
            ("E2301", "node 'n1' section[0] from 'input' 缺来源字段 fields[]".to_string()),
            ("E2301", "node 'n1' section[1] from 'edge' 缺来源字段 port".to_string()),
            (
                "E2301",
                "node 'n1' section[2] from 'template' 缺来源字段 template_ref".to_string()
            ),
        ]
    );
}

#[test]
fn store_section_missing_store_name_is_e2301_and_e2401() {
    // 缺 store 名（E2301）的同时也缺必填的视图策略（E2401）——一次收齐两码。
    let src = llm("context: { sections: [{ from: store, quota_tokens: 100 }] }");
    assert_eq!(check(&src), ["E2301", "E2401"]);
}

// ---------- E2302 配额非正（policy:none 记 0 例外；总量上限可配置） ----------

#[test]
fn quota_zero_is_e2302() {
    let src = llm(
        "context: { sections: [\
{ from: template, template_ref: pkg.t, quota_tokens: 0 }] }",
    );
    assert_eq!(check(&src), ["E2302"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2302",
            "node 'n1' section[0] quota_tokens = 0（须为正；例外仅 store 段 policy:none 记 0）"
                .to_string()
        )]
    );
}

#[test]
fn quota_zero_with_policy_none_on_store_is_legal() {
    // 提案 §7-5：`none` 段配额记 0 是词法变通——policy:none 的 store 段 quota 0 合法。
    let src = llm(
        "context: { sections: [\
{ from: store, store: s, policy: none, quota_tokens: 0 }] }",
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn quota_zero_with_policy_none_on_non_store_is_e2401_and_e2302() {
    // none 是 store 视图策略：input 段带 none → E2401（不适用）；例外不生效 → E2302。
    let src = llm(
        "context: { sections: [\
{ from: input, fields: [q], policy: none, quota_tokens: 0 }] }",
    );
    assert_eq!(check(&src), ["E2401", "E2302"]);
}

#[test]
fn total_quota_cap_is_configurable_and_default_off() {
    let src = std::fs::read_to_string("testdata/sample1.yaml").unwrap();
    let doc: Document = load_str(&src).unwrap_or_else(|e| panic!("{e}"));
    // 默认（§5 无总量规则）：不核对。
    assert!(validate(&doc).is_ok());
    // 可配置上限 1000 < solver 总量 7100（900+200+6000）→ E2302。
    let r = validate_with(
        &doc,
        &ValidateOptions {
            context_quota_cap: Some(1000),
            ..Default::default()
        },
    );
    assert_eq!(r.codes(), ["E2302"]);
    assert_eq!(
        r.violations[0].message,
        "node 'solver' context 配额总量 7100 超可配置上限 1000（policy:none 段按 0 计入）"
    );
    // 边界：总量恰等于上限不报（只有超过才报）。
    let r2 = validate_with(
        &doc,
        &ValidateOptions {
            context_quota_cap: Some(7100),
            ..Default::default()
        },
    );
    assert!(r2.is_ok(), "恰等上限不应报：\n{r2}");
}

// ---------- E2303 LLM-bound 节点禁 reads ----------

#[test]
fn llm_node_reads_store_is_e2303() {
    let src = llm("reads: [s]");
    assert_eq!(check(&src), ["E2303"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2303",
            "LLM-bound node 'n1' declares reads ['s']（store 只能经 context.sections 进入 agent 上下文）".to_string()
        )]
    );
}

#[test]
fn llm_node_reads_unknown_store_is_e2001_only() {
    // 抑制（承 M1 偏差⑨）：reads 全为未声明 store → E2001 已定罪，不重复报 E2303。
    let src = llm("reads: [ghost_s]");
    assert_eq!(check(&src), ["E2001"]);
}

#[test]
fn llm_node_reads_mixed_stores_lists_declared_only_in_e2303() {
    // 混合：未声明项归 E2001（refs 层先报），E2303 只列已声明项。
    let src = llm("reads: [s, ghost_s]");
    assert_eq!(check(&src), ["E2001", "E2303"]);
    assert_eq!(
        msgs(&src)[1],
        (
            "E2303",
            "LLM-bound node 'n1' declares reads ['s']（store 只能经 context.sections 进入 agent 上下文）".to_string()
        )
    );
}

#[test]
fn non_llm_node_reads_store_is_fine() {
    // 其余节点经 reads[] 读 store 是正道（提案 B §1.1 读侧裁决）。
    let src = doc(
        "  - { id: n1, kind: transform, reads: [s], ops: [ { op: project, port: s, into: answer } ], emits: { answer: a } }\n",
        "[]",
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

// ---------- E2304 入边端口恰被一个 section 引用 ----------

fn doc_with_in_edge(n1_body: &str) -> String {
    doc(
        &format!(
            "  - {{ id: prod, kind: task, emits: {{ out: a }} }}\n  - {{ id: n1, kind: task, uses: {{ model: route.main@1 }}, emits: {{ answer: a }}, {n1_body} }}\n"
        ),
        "[{ from: prod, output: out, to: n1, input: p }]",
    )
}

#[test]
fn uncovered_in_port_is_e2304() {
    let src = doc_with_in_edge(
        "context: { sections: [\
{ from: template, template_ref: pkg.t, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2304"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2304",
            "node 'n1' 入边端口 'p' 未被任何 context.sections 段引用（上下文 = 且仅 = sections 声明，无隐式注入）".to_string()
        )]
    );
}

#[test]
fn llm_node_without_context_and_in_edges_is_e2304() {
    // 实际消费但未声明：LLM-bound 节点有入边却连 context 都没有。
    let src = doc_with_in_edge("");
    assert_eq!(check(&src), ["E2304"]);
}

#[test]
fn in_port_referenced_twice_is_e2304() {
    let src = doc_with_in_edge(
        "context: { sections: [\
{ from: edge, port: p, policy: project, quota_tokens: 100 }, \
{ from: edge, port: p, policy: project, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2304"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2304",
            "node 'n1' 入边端口 'p' 被 2 个 section 引用（须恰被一个）".to_string()
        )]
    );
}

#[test]
fn non_llm_node_in_edges_without_context_is_clean() {
    // 非 LLM 节点不走 context（其读侧是 reads[]），入边不要求 section 覆盖。
    let src = doc(
        "  - { id: prod, kind: task, emits: { out: a } }\n  - { id: n1, kind: transform, ops: [ { op: project, port: p, into: answer } ], consumes: { p: a }, emits: { answer: a } }\n",
        "[{ from: prod, output: out, to: n1, input: p }]",
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn non_llm_node_with_context_in_port_not_sectioned_is_clean() {
    // 提案 B §5 INV-1 主语 = LLM-bound task 节点；§1 节点表：context「仅
    // LLM-bound task 可用（INV-1 判定对象）」。非 LLM 节点（check 判定单元）
    // 带 context 时 E2304 不判——其入边端口是程序性数据载荷（被验证对象），
    // 不是上下文装配面（G3 复核 2026-10-24：Agensh-DeLM verify/fail_scan）。
    let src = doc(
        "  - { id: prod, kind: task, emits: { out: a } }\n  - { id: n1, kind: check, uses: { unit: pkg.u.judge@1 }, consumes: { p: a }, emits: { report: a }, context: { sections: [\
{ from: template, template_ref: pkg.t, quota_tokens: 100 }] } }\n",
        "[{ from: prod, output: out, to: n1, input: p }]",
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

// ---------- E2401 store 视图策略域 ----------

#[test]
fn store_policy_missing_is_e2401() {
    let src = llm("context: { sections: [{ from: store, store: s, quota_tokens: 100 }] }");
    assert_eq!(check(&src), ["E2401"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2401",
            "node 'n1' section[0] from 'store' 缺视图策略（必填：full|latest_k|retrieve|summarize|none）".to_string()
        )]
    );
}

#[test]
fn store_policy_unknown_is_e2401() {
    let src = llm(
        "context: { sections: [\
{ from: store, store: s, policy: rolling, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2401"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2401",
            "node 'n1' section[0] policy 'rolling' 不在 store 视图策略词表（full|latest_k|retrieve|summarize|none）".to_string()
        )]
    );
}

#[test]
fn store_view_policy_on_input_section_is_e2401() {
    let src = llm(
        "context: { sections: [\
{ from: input, fields: [q], policy: full, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2401"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2401",
            "node 'n1' section[0] policy 'full' 不适用于 from 'input'（仅 project 或缺省）"
                .to_string()
        )]
    );
}

#[test]
fn policy_on_template_section_is_e2401() {
    let src = llm(
        "context: { sections: [\
{ from: template, template_ref: pkg.t, policy: project, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2401"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2401",
            "node 'n1' section[0] policy 'project' 不适用于 from 'template'（template 段无 policy）".to_string()
        )]
    );
}

#[test]
fn latest_k_without_args_k_is_e2401() {
    let src = llm(
        "context: { sections: [\
{ from: store, store: s, policy: latest_k, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2401"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2401",
            "node 'n1' section[0] policy 'latest_k' 参数 args.k 缺失或非正整数".to_string()
        )]
    );
}

#[test]
fn latest_k_with_zero_k_is_e2401() {
    let src = llm(
        "context: { sections: [\
{ from: store, store: s, policy: latest_k, args: { k: 0 }, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2401"]);
}

#[test]
fn retrieve_without_component_is_e2401() {
    let src = llm(
        "context: { sections: [\
{ from: store, store: s, policy: retrieve, args: { k: 3 }, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2401"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2401",
            "node 'n1' section[0] policy 'retrieve' 缺 component（组件引用）".to_string()
        )]
    );
}

#[test]
fn retrieve_without_args_k_is_e2401() {
    let src = llm(
        "context: { sections: [\
{ from: store, store: s, policy: retrieve, component: pkg.sim@1, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2401"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2401",
            "node 'n1' section[0] policy 'retrieve' 参数 args.k 缺失或非正整数".to_string()
        )]
    );
}

#[test]
fn summarize_without_component_is_e2401() {
    let src = llm(
        "context: { sections: [\
{ from: store, store: s, policy: summarize, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2401"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2401",
            "node 'n1' section[0] policy 'summarize' 缺 component（组件引用）".to_string()
        )]
    );
}

// ---------- E2402 arg ∈ 所在体形参表 ----------

#[test]
fn arg_not_in_formals_is_e2402() {
    let src = llm(
        "context: { sections: [\
{ from: arg, name: ghost, policy: project, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2402"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2402",
            "node 'n1' section[0] arg 'ghost' 不在所在体形参表（可见形参：[]）".to_string()
        )]
    );
}

#[test]
fn map_implicit_item_not_visible_at_root_is_e2402() {
    // map 的隐式形参 item 只在 map 体内可见——根图不是体。
    let src = llm(
        "context: { sections: [\
{ from: arg, name: item, policy: project, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2402"]);
}

#[test]
fn arg_resolution_walks_scope_chain_outward() {
    // map 体（隐式 item）> loop 体（args {x}）：leaf 同时可见 item 与 x。
    // 形参可见性沿作用域链向外收集（loop args + map 隐式 item）。
    let src = [
        "version: 1",
        "meta:",
        "  name: t",
        "  graph_version: \"1.0\"",
        "  entry: n1",
        "  input: in_t",
        "  output: { ref: a, from: n1.lp.leaf }",
        "types:",
        "  - { name: in_t, schema: { type: object, required: [arr], properties: { arr: { type: array, items: { type: string } } } } }",
        "  - { name: a, schema: { type: object } }",
        "stores:",
        "  - { name: s, type: message_log, writer: n1, lifecycle: run, retention: { max_items: 8 } }",
        "nodes:",
        "  - id: n1",
        "    kind: task",
        "    map:",
        "      over: input.arr",
        "      body:",
        "        nodes:",
        "          - id: lp",
        "            kind: task",
        "            loop:",
        "              args: { x: input.arr }",
        "              body:",
        "                nodes:",
        "                  - { id: leaf, kind: task, uses: { model: route.main@1 }, emits: { v: a }, context: { sections: [",
        "                      { from: arg, name: item, policy: project, quota_tokens: 10 },",
        "                      { from: arg, name: x, policy: project, quota_tokens: 10 }] } }",
        "                  - { id: g, kind: gate, assert: \"true\" }",
        "                edges: []",
        "              max_iterations: 2",
        "              safety_points: [ { node: g, kind: gate } ]",
        "        edges: []",
        "edges: []",
    ]
    .join("\n")
        + "\n";
    assert_eq!(check(&src), Vec::<&str>::new());
}

// ---------- E2403 template vars 引用悬空 ----------

#[test]
fn template_vars_input_field_dangling_is_e2403() {
    let src = llm(
        "context: { sections: [\
{ from: template, template_ref: pkg.t, vars: { x: input.nope }, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2403"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2403",
            "node 'n1' section[0] vars.x 引用 'input.nope' 悬空：字段 'nope' 不在 meta.input schema（in_t）".to_string()
        )]
    );
}

#[test]
fn template_vars_arg_dangling_is_e2403() {
    let src = llm(
        "context: { sections: [\
{ from: template, template_ref: pkg.t, vars: { x: arg.nope }, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2403"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2403",
            "node 'n1' section[0] vars.x 引用 'arg.nope' 悬空：形参 'nope' 不在所在体形参表（可见形参：[]）".to_string()
        )]
    );
}

#[test]
fn template_vars_literal_and_valid_refs_are_clean() {
    // 字面量（非 input./arg. 前缀）不判；input.q 可解析。
    let src = llm(
        "context: { sections: [\
{ from: template, template_ref: pkg.t, vars: { v: b, q2: input.q }, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn template_vars_empty_ref_is_e2403() {
    let src = llm(
        "context: { sections: [\
{ from: template, template_ref: pkg.t, vars: { a: 'input.', b: 'arg.' }, quota_tokens: 100 }] }",
    );
    assert_eq!(check(&src), ["E2403", "E2403"]);
    assert_eq!(
        msgs(&src),
        [
            (
                "E2403",
                "node 'n1' section[0] vars.a 引用 'input.' 悬空（input. 后须跟字段名）".to_string()
            ),
            (
                "E2403",
                "node 'n1' section[0] vars.b 引用 'arg.' 悬空（arg. 后须跟形参名）".to_string()
            ),
        ]
    );
}

// ---------- 一次收齐 ----------

#[test]
fn all_context_codes_collected_in_one_report() {
    // 四节点并犯七类：n1 E2301（input 悬空字段）· n2 E2401+E2302+E2303+E2304
    // （坏策略 + 零配额 + LLM 直读 + 入边端口 out 未覆盖）· n3 E2402+E2403
    // （arg 悬空 + vars 悬空）。顺序 = 节点路径序（n1<n2<n3<prod）× 节点内
    // 段序 → E2303 → E2304，确定。
    let src = [
        "version: 1",
        "meta:",
        "  name: t",
        "  graph_version: \"1.0\"",
        "  entry: n1",
        "  input: in_t",
        "  output: { ref: a, from: n1 }",
        "types:",
        "  - { name: in_t, schema: { type: object, required: [q], properties: { q: { type: string } } } }",
        "  - { name: a, schema: { type: object } }",
        "stores:",
        "  - { name: s, type: message_log, writer: n1, lifecycle: run, retention: { max_items: 8 } }",
        "nodes:",
        "  - { id: prod, kind: task, emits: { out: a } }",
        "  - { id: n1, kind: task, uses: { model: route.main@1 }, emits: { answer: a }, context: { sections: [",
        "      { from: input, fields: [ghost], policy: project, quota_tokens: 100 }] } }",
        "  - { id: n2, kind: task, uses: { model: route.main@1 }, reads: [s], emits: { answer: a }, context: { sections: [",
        "      { from: store, store: s, policy: weird, quota_tokens: 0 }] } }",
        "  - { id: n3, kind: task, uses: { model: route.main@1 }, emits: { answer: a }, context: { sections: [",
        "      { from: arg, name: ghost, policy: project, quota_tokens: 10 },",
        "      { from: template, template_ref: pkg.t, vars: { v: arg.ghost2 }, quota_tokens: 10 }] } }",
        "edges:",
        "  - { from: prod, output: out, to: n2, input: out }",
    ]
    .join("\n")
        + "\n";
    assert_eq!(
        check(&src),
        ["E2301", "E2401", "E2302", "E2303", "E2304", "E2402", "E2403"]
    );
}
