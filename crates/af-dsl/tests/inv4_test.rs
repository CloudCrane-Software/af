//! INV-4 评估器外部化验收测试（M3，提案 B §5/§8-5）：
//! 标注 `eval_input: true` 的 store，其读者（经 `reads` / `context.sections`
//! from:store 段引用它的全部节点）必须 ⊆ {该 store 的 writer, 绑定 uses.unit
//! 的 check 节点} → 越界即 E2331。误标只收紧不放松（§8-5：标记责任在作者）。
//! 正例门槛：提案 B §4 六个 golden 样例全量（M1+M2+M3）校验干净——特别是
//! 样例 4 AFlow 的 answers_ref（eval_input）仅由 check+unit 的 assess 读。

use af_dsl::{load_str, validate, Document, Report};

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

/// 基本骨架：answers_ref（eval_input，writer loader）+ loader 节点；
/// 额外节点与边由调用方给（loader 自带）。
fn doc(extra_nodes: &str, edges: &str) -> String {
    format!(
        "version: 1\n\
meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: loader\n  input: in_t\n  output: {{ ref: signal, from: loader }}\n\
types:\n  - {{ name: in_t, schema: {{ type: object, required: [q], properties: {{ q: {{ type: string }} }} }} }}\n  - {{ name: a, schema: {{ type: object, required: [text], properties: {{ text: {{ type: string }} }} }} }}\n\
stores:\n  - {{ name: answers_ref, type: collection, item: a, writer: loader, lifecycle: run, retention: {{ max_items: 64 }}, eval_input: true }}\n\
nodes:\n  - {{ id: loader, kind: task, uses: {{ tool: pkg.searchkit.corpus@1 }}, writes: [answers_ref], emits: {{ done: signal }} }}\n\
{extra_nodes}edges: {edges}\n"
    )
}

/// 读者节点模板（uses 子句与 kind 由调用方代入；READS_CLAUSE 可换读通道）。
const READER: &str = "  - { id: READER_ID, kind: READER_KINDREADER_USES, READS_CLAUSE consumes: { done: signal }, emits: { out: a } }\n";

fn reader(id: &str, kind: &str, uses: &str, reads: &str) -> String {
    READER
        .replace("READER_ID", id)
        .replace("READER_KIND", kind)
        .replace("READER_USES", uses)
        .replace("READS_CLAUSE", reads)
}

// ---------- golden 门槛：六样例全量（M1+M2+M3）校验干净 ----------

#[test]
fn six_golden_samples_pass_full_m3_validation() {
    for i in 1..=6 {
        let src = std::fs::read_to_string(format!("testdata/sample{i}.yaml")).unwrap();
        let doc: Document = load_str(&src).unwrap_or_else(|e| panic!("sample{i} 解析失败：{e}"));
        let r = validate(&doc);
        assert!(r.is_ok(), "sample{i} 不应违例：\n{r}");
    }
}

// ---------- 样例 4 AFlow 的 eval_input 用法（正例不误伤的关键面） ----------

#[test]
fn sample4_answers_ref_readers_stay_clean() {
    // answers_ref 读者仅 assess（check + uses.unit）→ 合法评估器；writer loader
    // 不读。experience（未标 eval_input）由 synth/best 自由读 → 不受 INV-4 约束。
    let src = std::fs::read_to_string("testdata/sample4.yaml").unwrap();
    let doc: Document = load_str(&src).unwrap_or_else(|e| panic!("{e}"));
    let r = validate(&doc);
    assert!(r.is_ok(), "sample4 不应违例：\n{r}");
    assert!(!r.codes().contains(&"E2331"));
}

// ---------- 提案 B §5 的 INV-4 违规样例（机器验证过的 golden 拒绝夹具） ----------

#[test]
fn spec_inv4_fixture_triggers_exactly_e2331_with_e2303() {
    // 提案 B §5 INV-4 样例：map 体候选 gen_a（LLM）reads 参考答案——兼踩
    // E2303（LLM 节点不该用 reads 直读）；评估器 assess（check+unit）与
    // writer loader 合法。§5 注明码集 = E2331[+E2303]。
    let src = std::fs::read_to_string("testdata/reject/inv4_e2331_eval_input.yaml").unwrap();
    assert_eq!(check(&src), ["E2303", "E2331"]);
    // 错误信息逐字对齐提案 §5 预期错误风格（节点以全路径标识，见 RESULT-M3
    // 偏差表——提案样例用裸 id，M1 E2312 已确立全路径先例）。
    assert_eq!(
        msgs(&src),
        [
            (
                "E2303",
                "LLM-bound node 'run_map.gen_a' declares reads ['answers_ref']（store 只能经 context.sections 进入 agent 上下文）".to_string()
            ),
            (
                "E2331",
                "store 'answers_ref' (eval_input) referenced by node 'run_map.gen_a' outside {writer 'loader', evaluator nodes}（评估输入对候选不可读）".to_string()
            ),
        ]
    );
}

// ---------- 反例 2：非 LLM 候选经 reads[] 读参考答案 ----------

#[test]
fn plain_task_reading_eval_input_store_is_e2331() {
    // uses.tool 而非 model：非 LLM-bound，无 E2303——纯 E2331。
    let src = doc(
        &reader(
            "thief",
            "task",
            ", uses: { tool: pkg.exfil@1 }",
            "reads: [answers_ref],",
        ),
        "[{ from: loader, output: done, to: thief, input: done }]",
    );
    assert_eq!(check(&src), ["E2331"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2331",
            "store 'answers_ref' (eval_input) referenced by node 'thief' outside {writer 'loader', evaluator nodes}（评估输入对候选不可读）".to_string()
        )]
    );
}

// ---------- 反例 3：LLM 候选经 context.sections from:store 读参考答案 ----------

#[test]
fn section_reference_to_eval_input_store_is_e2331() {
    // reads 之外的另一条读通道：from:store 段（无 reads[] → 无 E2303；
    // 入边端口 done 已被 from:edge 段恰一个引用 → 无 E2304）。
    let solver = "  - id: solver\n    kind: task\n    uses: { model: route.main@1 }\n    consumes: { done: signal }\n    emits: { answer: a }\n    context:\n      sections:\n        - { from: edge, port: done, quota_tokens: 50 }\n        - { from: store, store: answers_ref, policy: latest_k, args: { k: 4 }, quota_tokens: 120 }\n";
    let src = doc(
        solver,
        "[{ from: loader, output: done, to: solver, input: done }]",
    );
    assert_eq!(check(&src), ["E2331"]);
    assert_eq!(
        msgs(&src),
        [(
            "E2331",
            "store 'answers_ref' (eval_input) referenced by node 'solver' outside {writer 'loader', evaluator nodes}（评估输入对候选不可读）".to_string()
        )]
    );
}

// ---------- 反例 4：check 未绑 uses.unit 不是评估器 ----------

#[test]
fn check_without_unit_reading_eval_input_store_is_e2331() {
    // §5：评估器 = kind=check 且绑定 uses.unit；缺 unit 的 check 越界读。
    let src = doc(
        &reader("assessor", "check", "", "reads: [answers_ref],"),
        "[{ from: loader, output: done, to: assessor, input: done }]",
    );
    assert_eq!(check(&src), ["E2331"]);
}

// ---------- 反例 5：gate 绑 unit 也不是评估器（须 kind=check） ----------

#[test]
fn gate_with_unit_reading_eval_input_store_is_e2331() {
    // §5 集合字面：{writer, 绑 uses.unit 的 check 节点}——gate 属验证面但
    // 非评估器（评估器外部化要求 kind=check + uses.unit 严格双条件）。
    let src = doc(
        &reader(
            "guard",
            "gate",
            ", uses: { unit: pkg.guard@1 }",
            "reads: [answers_ref],",
        ),
        "[{ from: loader, output: done, to: guard, input: done }]",
    );
    assert_eq!(check(&src), ["E2331"]);
}

// ---------- 正例：合法读者不误伤 ----------

#[test]
fn writer_reading_own_eval_input_store_is_clean() {
    let src = doc("", "[]").replace(
        "writes: [answers_ref], emits: { done: signal }",
        "writes: [answers_ref], reads: [answers_ref], emits: { done: signal }",
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn check_with_unit_reading_eval_input_store_is_clean() {
    // 样例 4 assess 模式：check + uses.unit 是唯一合法候选外读者。
    let src = doc(
        &reader(
            "assess",
            "check",
            ", uses: { unit: pkg.searchkit.assess@1 }",
            "reads: [answers_ref],",
        ),
        "[{ from: loader, output: done, to: assess, input: done }]",
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn non_eval_input_store_is_freely_readable() {
    // 未标 eval_input 的 store 不受 INV-4 约束（样例 4 experience 模式：
    // 变异器可读分数）。§8-5：不标则完全不查（标记责任在作者）。
    let src = doc(
        &reader("synth", "task", ", uses: { tool: pkg.variator@1 }", "reads: [experience],"),
        "[{ from: loader, output: done, to: synth, input: done }]",
    )
    .replace(
        "  - { name: answers_ref, type: collection, item: a, writer: loader, lifecycle: run, retention: { max_items: 64 }, eval_input: true }",
        "  - { name: answers_ref, type: collection, item: a, writer: loader, lifecycle: run, retention: { max_items: 64 }, eval_input: true }\n  - { name: experience, type: collection, item: a, writer: loader, lifecycle: run, retention: { max_items: 20 } }",
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

// ---------- 一次收齐与抑制 ----------

#[test]
fn two_offenders_report_two_e2331_in_path_order() {
    // solver（sections 通道，无入边）与 thief（reads 通道）并犯 → 两条
    // E2331，顺序 = 节点索引序（BTree 路径序：solver < thief）。
    let solver = "  - id: solver\n    kind: task\n    uses: { model: route.main@1 }\n    emits: { answer: a }\n    context:\n      sections:\n        - { from: store, store: answers_ref, policy: latest_k, args: { k: 4 }, quota_tokens: 120 }\n";
    let nodes = format!(
        "{solver}{}",
        reader(
            "thief",
            "task",
            ", uses: { tool: pkg.exfil@1 }",
            "reads: [answers_ref],"
        )
    );
    let src = doc(
        &nodes,
        "[{ from: loader, output: done, to: thief, input: done }]",
    );
    assert_eq!(check(&src), ["E2331", "E2331"]);
}

#[test]
fn dangling_writer_suppresses_e2331_noise() {
    // writer 路径悬空已由 refs 层 E2001 定罪；INV-4 对该 store 跳过
    // （合法读者集合无从建立，双码只产噪声——承 M1 偏差⑨纪律）。
    let src = doc(
        &reader(
            "thief",
            "task",
            ", uses: { tool: pkg.exfil@1 }",
            "reads: [answers_ref],",
        ),
        "[{ from: loader, output: done, to: thief, input: done }]",
    )
    .replace(
        "writer: loader, lifecycle: run, retention: { max_items: 64 }, eval_input: true",
        "writer: ghost, lifecycle: run, retention: { max_items: 64 }, eval_input: true",
    );
    assert_eq!(check(&src), ["E2001"]);
}
