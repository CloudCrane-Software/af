//! INV-2 写入串行化编译期判定验收测试（M1，提案 B §5）：
//! store 唯一写者（E2311 写者交叉核对）、map 体禁写共享槽（E2312）、
//! carried 覆盖（E2211），一次收齐。
//! 正例门槛：提案 B §4 六个 golden 样例全量 M1 校验干净。

use af_dsl::{load_str, validate, Document, Report};

fn check(src: &str) -> Vec<&'static str> {
    let doc: Document = load_str(src).unwrap_or_else(|e| panic!("解析失败：{e}"));
    validate(&doc).codes()
}

fn codes_of(r: &Report) -> Vec<&'static str> {
    r.codes()
}

// ---------- golden 门槛：六样例全量校验干净 ----------

#[test]
fn six_golden_samples_pass_full_m1_validation() {
    for i in 1..=6 {
        let src = std::fs::read_to_string(format!("testdata/sample{i}.yaml")).unwrap();
        let doc: Document = load_str(&src).unwrap_or_else(|e| panic!("sample{i} 解析失败：{e}"));
        let r = validate(&doc);
        assert!(r.is_ok(), "sample{i} 不应违例：\n{r}");
    }
}

// ---------- 提案 B §5 的 INV-2 违规样例（机器验证过的 golden 拒绝夹具） ----------

#[test]
fn spec_inv2_fixture_triggers_e2312_and_e2322() {
    // 提案 B §5 INV-2 样例：map 体 debater 写共享槽 transcript（writer 声明与之
    // 一致，故无 E2311；carried 已覆盖，故无 E2211）。安全点 debater（task）
    // 非验证面 → E2322 自 fix2（P2-L08）起实装——按 RESULT-M3 遗留表预案，
    // 断言由恰 [E2312] 同步改为 [E2312, E2322]（提案 §5 声明两码均触发）。
    let src = std::fs::read_to_string("testdata/reject/inv2_e2312_map_write.yaml").unwrap();
    assert_eq!(check(&src), ["E2312", "E2322"]);
}

// ---------- E2311 写者交叉核对 ----------

#[test]
fn two_writers_is_e2311() {
    let src = doc_with_store("n1", "  - { id: n1, kind: task, uses: { model: route.main@1 }, writes: [s], emits: { answer: a } }\n  - { id: n2, kind: task, writes: [s] }\n");
    assert_eq!(check(&src), ["E2311"]);
}

#[test]
fn sole_writer_differs_from_declared_is_e2311() {
    // 唯一实际写者 n2 ≠ 声明写者 n1。
    let src = doc_with_store("n1", "  - { id: n1, kind: task, uses: { model: route.main@1 }, emits: { answer: a } }\n  - { id: n2, kind: task, writes: [s] }\n");
    assert_eq!(check(&src), ["E2311"]);
}

#[test]
fn zero_writer_is_tolerated_in_m1() {
    // 声明写者不写、无人写：非串行化违例，M1 不报（RESULT-M1 偏差表登记）。
    let src = doc_with_store(
        "n1",
        "  - { id: n1, kind: task, uses: { model: route.main@1 }, emits: { answer: a } }\n",
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn writer_declared_as_body_path_matches() {
    // 声明写者是点号路径（loop 体内节点）且交叉核对一致 → 干净。
    let src = doc_with_store(
        "l1.b1",
        "  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - { id: b1, kind: task, writes: [s], emits: { x: a } }\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      carried: [ { store: s, max_items: 4 } ]\n      safety_points: [ { node: g, kind: gate } ]\n    emits: { answer: { from_body: b1.x } }\n",
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

// ---------- E2211 carried 覆盖 ----------

#[test]
fn uncovered_outer_store_written_in_body_is_e2211() {
    let src = doc_with_store(
        "l1.b1",
        "  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - { id: b1, kind: task, writes: [s], emits: { x: a } }\n          - { id: g, kind: gate, assert: \"true\" }\n        edges: []\n      max_iterations: 2\n      safety_points: [ { node: g, kind: gate } ]\n    emits: { answer: { from_body: b1.x } }\n",
    );
    assert_eq!(check(&src), ["E2211"]);
}

#[test]
fn nested_body_write_requires_outer_carried_too() {
    // 嵌套体（map 体内）写入也计入外层 loop 的 carried 覆盖义务；map 体写共享槽
    // 同时踩 E2312（写者声明为该 map 体路径，交叉核对一致 → 无 E2311）。
    let src = [
        "version: 1",
        "meta:",
        "  name: t",
        "  graph_version: \"1.0\"",
        "  entry: l1",
        "  input: in_t",
        "  output: { ref: a, from: l1 }",
        "types:",
        "  - { name: in_t, schema: { type: object, required: [arr], properties: { arr: { type: array, items: { type: string } } } } }",
        "  - { name: a, schema: { type: object } }",
        "stores:",
        "  - { name: s, type: kv, value: { type: string }, writer: l1.mp.d, lifecycle: run, retention: { max_items: 8 } }",
        "nodes:",
        "  - id: l1",
        "    kind: task",
        "    loop:",
        "      body:",
        "        nodes:",
        "          - id: mp",
        "            kind: task",
        "            map:",
        "              over: input.arr",
        "              body:",
        "                nodes:",
        "                  - { id: d, kind: task, writes: [s], emits: { x: a } }",
        "                edges: []",
        "          - { id: b2, kind: task, emits: { y: a } }",
        "          - { id: g, kind: gate, assert: \"true\" }",
        "        edges: []",
        "      max_iterations: 2",
        "      safety_points: [ { node: g, kind: gate } ]",
        "    emits: { answer: { from_body: b2.y } }",
        "edges: []",
        "policies:",
        "  budgets: []",
        "  stop: []",
        "  failures: []",
    ]
    .join("\n")
        + "\n";
    assert_eq!(check(&src), ["E2312", "E2211"]);
}

// ---------- 一次收齐 ----------

#[test]
fn all_inv2_codes_collected_in_one_report() {
    // 同文档三案并犯：m 多写者（E2311）· p 由 map 体写（E2312）·
    // p、c 均未被外层 carried 覆盖（E2211×2）。顺序 = store 表序 + carried 表序，确定。
    let src = collect_all_doc();
    let doc: Document = load_str(&src).unwrap_or_else(|e| panic!("{e}"));
    let r = validate(&doc);
    assert_eq!(codes_of(&r), ["E2311", "E2312", "E2211", "E2211"]);
}

// ---------- 夹具构造 ----------

fn doc_with_store(writer: &str, nodes: &str) -> String {
    let entry = if nodes.contains("id: l1") { "l1" } else { "n1" };
    format!(
        "version: 1\n\
meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: {entry}\n  input: in_t\n  output: {{ ref: a, from: {entry} }}\n\
types:\n  - {{ name: in_t, schema: {{ type: object, required: [q], properties: {{ q: {{ type: string }} }} }} }}\n  - {{ name: a, schema: {{ type: object }} }}\n\
stores:\n  - {{ name: s, type: kv, value: {{ type: string }}, writer: {writer}, lifecycle: run, retention: {{ max_items: 8 }} }}\n\
nodes:\n{nodes}\
edges: []\n\
policies:\n  budgets: []\n  stop: []\n  failures: []\n"
    )
}

fn collect_all_doc() -> String {
    // 输入带 array 字段（供 map.over）。
    [
        "version: 1",
        "meta:",
        "  name: t",
        "  graph_version: \"1.0\"",
        "  entry: n1",
        "  input: in_t",
        "  output: { ref: a, from: n1 }",
        "types:",
        "  - { name: in_t, schema: { type: object, required: [arr], properties: { arr: { type: array, items: { type: string } } } } }",
        "  - { name: a, schema: { type: object } }",
        "stores:",
        "  - { name: m, type: kv, value: { type: string }, writer: n1, lifecycle: run, retention: { max_items: 8 } }",
        "  - { name: p, type: kv, value: { type: string }, writer: l1.mp.d, lifecycle: run, retention: { max_items: 8 } }",
        "  - { name: c, type: kv, value: { type: string }, writer: l1.b9, lifecycle: run, retention: { max_items: 8 } }",
        "nodes:",
        "  - { id: n1, kind: task, uses: { model: route.main@1 }, writes: [m], emits: { answer: a } }",
        "  - { id: n2, kind: task, writes: [m] }",
        "  - id: l1",
        "    kind: task",
        "    loop:",
        "      body:",
        "        nodes:",
        "          - id: mp",
        "            kind: task",
        "            map:",
        "              over: input.arr",
        "              body:",
        "                nodes:",
        "                  - { id: d, kind: task, writes: [p], emits: { x: a } }",
        "                edges: []",
        "          - { id: b9, kind: task, writes: [c], emits: { z: a } }",
        "          - { id: g, kind: gate, assert: \"true\" }",
        "        edges: []",
        "      max_iterations: 2",
        "      safety_points: [ { node: g, kind: gate } ]",
        "    emits: { answer: { from_body: b9.z } }",
        "edges: []",
        "policies:",
        "  budgets: []",
        "  stop: []",
        "  failures: []",
    ]
    .join("\n")
        + "\n"
}

// ---------- 红队修复批（W1）：E2311 单写者不匹配分支文案 ----------

#[test]
fn sole_writer_mismatch_message_says_mismatch_not_multi() {
    // 唯一实际写者 ≠ 声明写者：文案应说「写者与声明不符」（写者集合恰一，
    // 断言「不唯一」与事实不符、误导排障方向）；码仍为 E2311。
    let src = doc_with_store(
        "n1",
        "  - { id: n1, kind: task, uses: { model: route.main@1 }, emits: { answer: a } }\n  - { id: n2, kind: task, writes: [s] }\n",
    );
    let doc: Document = load_str(&src).unwrap_or_else(|e| panic!("解析失败：{e}"));
    let r = validate(&doc);
    assert_eq!(codes_of(&r), ["E2311"]);
    let msg = &r.violations[0].message;
    assert!(
        msg.contains("写者与声明不符"),
        "文案应为「写者与声明不符」：{msg}"
    );
    assert!(
        !msg.contains("不唯一"),
        "单写者分支不得断言「不唯一」：{msg}"
    );
}

#[test]
fn multi_writer_message_still_says_not_sole() {
    let src = doc_with_store(
        "n1",
        "  - { id: n1, kind: task, uses: { model: route.main@1 }, writes: [s], emits: { answer: a } }\n  - { id: n2, kind: task, writes: [s] }\n",
    );
    let doc: Document = load_str(&src).unwrap_or_else(|e| panic!("解析失败：{e}"));
    let r = validate(&doc);
    assert_eq!(codes_of(&r), ["E2311"]);
    assert!(r.violations[0].message.contains("不唯一"));
}
