//! P2-L08 fix2：受控循环三守卫校验（G2a 独立验证发现的三处缺口，
//! 报告：计划/研究组/af-compiler-G2a-独立验证-20261024.md 观察项 2/3）。
//!
//! 缺口与码位（提案 B §1.3 正本 + §6 码表）：
//! - E2002 `max_iterations` 必须 >0（值域——与 af-graph 共享码位，同族
//!   `af-graph/src/validate.rs` 的 LoopNotControlled 值域检查）；
//! - E2004 `safety_points` 必须非空（空集合——与 af-graph 共享码位；
//!   「缺安全点=拒绝编译」）；
//! - E2322 安全点必须指向验证面节点（§6 已分配码位、§5 inv2 夹具已预告；
//!   kind 三值承 af-graph `validate.rs` 三值语义原样：check→kind=check 节点、
//!   gate→kind=gate 节点、human→human 标记）；
//! - E2002 `carried` 每项至少一个上界（max_items/max_bytes，§1.3「至少一个
//!   上界（E2002）」）——与 E2211（carried 覆盖性）互补不重复。
//!
//! 反例逐码精确断言（防无关码噪声）+ 错误信息含节点路径定位 + 正例零误伤。

use af_dsl::{load_str, validate, Document};

const HEAD: &str = "version: 1\n\
meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: l1\n  input: in_t\n  output: { ref: a, from: l1 }\n\
types:\n  - { name: in_t, schema: { type: object, required: [q], properties: { q: { type: string } } } }\n  - { name: a, schema: { type: object } }\n";

const POLICIES: &str = "edges: []\npolicies: { budgets: [], stop: [], failures: [] }\n";

/// 最小 gate 节点（体内安全点的合法落点）。
const GATE: &str = "          - { id: g, kind: gate, assert: \"true\" }\n";

/// 受控循环骨架：stores 段、body 节点、max_iterations、carried 段、
/// safety_points 值均可注入（carried 段为空串 = 不声明）。
fn guard_doc(
    stores: &str,
    max_iterations: &str,
    carried: &str,
    safety_points: &str,
    body_nodes: &str,
) -> String {
    format!(
        "{HEAD}{stores}nodes:\n  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes:\n{body_nodes}        edges: []\n      max_iterations: {max_iterations}\n{carried}      safety_points: {safety_points}\n    emits: {{ answer: a }}\n{POLICIES}"
    )
}

fn check(src: &str) -> Vec<&'static str> {
    let doc: Document = load_str(src).unwrap_or_else(|e| panic!("解析失败：{e}"));
    validate(&doc).codes()
}

/// (code, message) 对（定位断言用）。
fn violations(src: &str) -> Vec<(&'static str, String)> {
    let doc: Document = load_str(src).unwrap_or_else(|e| panic!("解析失败：{e}"));
    validate(&doc)
        .violations
        .iter()
        .map(|v| (v.code.code(), v.message.clone()))
        .collect()
}

// ---------- ① E2002：max_iterations 必须 >0 ----------

#[test]
fn zero_max_iterations_is_e2002() {
    let src = guard_doc("", "0", "", "[ { node: g, kind: gate } ]", GATE);
    assert_eq!(check(&src), ["E2002"]);
}

#[test]
fn zero_max_iterations_message_carries_loop_path() {
    // 嵌套 loop：内层 l1.inner 迭代上限为 0，信息必须定位到内层路径。
    let src = format!(
        "{HEAD}nodes:\n  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - id: inner\n            kind: task\n            loop:\n              body:\n                nodes:\n                  - {{ id: gi, kind: gate, assert: \"true\" }}\n                edges: []\n              max_iterations: 0\n              safety_points: [ {{ node: gi, kind: gate }} ]\n            emits: {{ v: a }}\n{GATE}        edges: []\n      max_iterations: 2\n      safety_points: [ {{ node: g, kind: gate }} ]\n    emits: {{ answer: a }}\n{POLICIES}"
    );
    assert_eq!(check(&src), ["E2002"]);
    let vs = violations(&src);
    assert!(
        vs[0].1.contains("l1.inner"),
        "信息应含嵌套 loop 节点路径，实为：{}",
        vs[0].1
    );
}

// ---------- ②a E2004：safety_points 必须非空 ----------

#[test]
fn empty_safety_points_is_e2004() {
    let src = guard_doc("", "2", "", "[]", GATE);
    assert_eq!(check(&src), ["E2004"]);
}

#[test]
fn empty_safety_points_message_carries_loop_path() {
    let src = guard_doc("", "2", "", "[]", GATE);
    let vs = violations(&src);
    assert_eq!(vs.len(), 1);
    assert_eq!(vs[0].0, "E2004");
    assert!(
        vs[0].1.contains("l1") && vs[0].1.contains("safety_points"),
        "信息应含节点路径与字段名，实为：{}",
        vs[0].1
    );
}

// ---------- ②b E2322：安全点必须指向验证面节点（三值原样） ----------

#[test]
fn safety_point_on_task_node_is_e2322() {
    // 提案 §5 inv2 场景同族：kind=gate 指向 task 节点。
    let body = "          - { id: w, kind: task, emits: { x: a } }\n";
    let src = guard_doc("", "2", "", "[ { node: w, kind: gate } ]", body);
    assert_eq!(check(&src), ["E2322"]);
}

#[test]
fn safety_point_kind_mismatch_is_e2322() {
    // 三值原样（承 af-graph）：声明 check 却指向 gate 节点 → 不符。
    let src = guard_doc("", "2", "", "[ { node: g, kind: check } ]", GATE);
    assert_eq!(check(&src), ["E2322"]);
}

#[test]
fn safety_point_human_on_plain_node_is_e2322() {
    // 声明 human 却指向无 human 标记的 gate 节点 → 不符。
    let src = guard_doc("", "2", "", "[ { node: g, kind: human } ]", GATE);
    assert_eq!(check(&src), ["E2322"]);
}

#[test]
fn safety_point_message_carries_paths() {
    let body = "          - { id: w, kind: task, emits: { x: a } }\n";
    let src = guard_doc("", "2", "", "[ { node: w, kind: gate } ]", body);
    let vs = violations(&src);
    assert_eq!(vs.len(), 1);
    assert_eq!(vs[0].0, "E2322");
    assert!(
        vs[0].1.contains("l1") && vs[0].1.contains("w"),
        "信息应含 loop 路径与安全点目标，实为：{}",
        vs[0].1
    );
}

#[test]
fn safety_point_gate_on_gate_is_clean() {
    let src = guard_doc("", "2", "", "[ { node: g, kind: gate } ]", GATE);
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn safety_point_check_on_check_is_clean() {
    let body = "          - { id: c, kind: check, consumes: { x: a }, emits: { report: a } }\n";
    let src = guard_doc("", "2", "", "[ { node: c, kind: check } ]", body);
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn safety_point_human_on_human_node_is_clean() {
    // human 标记节点（sample5 approve 同型）作 human 安全点 → 合法。
    let body = "          - id: h\n            kind: task\n            human:\n              station: review-desk\n              role: reviewer\n              resume:\n                schema: { type: object }\n            emits: { decision: a }\n";
    let src = guard_doc("", "2", "", "[ { node: h, kind: human } ]", body);
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn dangling_safety_point_stays_e2107_only() {
    // 路径悬空是 E2107 域（refs）：E2322 不对不可解析目标重复定罪。
    let src = guard_doc("", "2", "", "[ { node: nope.deep, kind: gate } ]", GATE);
    assert_eq!(check(&src), ["E2107"]);
}

// ---------- ③ E2002：carried 每项至少一个上界 ----------

#[test]
fn unbounded_carried_is_e2002() {
    // store 声明完备（无 E2001/E2311/E2211），仅缺上界 → 恰一个 E2002。
    let stores = "stores:\n  - { name: s, type: kv, value: { type: string }, writer: n0, lifecycle: run, retention: { max_items: 8 } }\n";
    let src = format!(
        "{HEAD}{stores}nodes:\n  - {{ id: n0, kind: task, writes: [s], emits: {{ answer: a }} }}\n  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes:\n{GATE}        edges: []\n      max_iterations: 2\n      carried: [ {{ store: s }} ]\n      safety_points: [ {{ node: g, kind: gate }} ]\n    emits: {{ answer: a }}\n{POLICIES}"
    );
    assert_eq!(check(&src), ["E2002"]);
    let vs = violations(&src);
    assert!(
        vs[0].1.contains("l1") && vs[0].1.contains("\"s\""),
        "信息应含 loop 路径与 store 名，实为：{}",
        vs[0].1
    );
}

#[test]
fn carried_bytes_only_bound_is_clean() {
    // max_items/max_bytes 至少其一（承 af-graph is_bounded 口径）。
    let stores = "stores:\n  - { name: s, type: kv, value: { type: string }, writer: n0, lifecycle: run, retention: { max_items: 8 } }\n";
    let src = format!(
        "{HEAD}{stores}nodes:\n  - {{ id: n0, kind: task, writes: [s], emits: {{ answer: a }} }}\n  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes:\n{GATE}        edges: []\n      max_iterations: 2\n      carried: [ {{ store: s, max_bytes: 4096 }} ]\n      safety_points: [ {{ node: g, kind: gate }} ]\n    emits: {{ answer: a }}\n{POLICIES}"
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}

#[test]
fn unbounded_carried_and_uncovered_are_distinct_codes() {
    // 互补性：同一 store 既无上界（E2002）又未被覆盖（E2211）→ 两码并收，
    // 语义不同（值域 vs 覆盖性），不重复定罪也不互相吸收。
    let stores = "stores:\n  - { name: s, type: kv, value: { type: string }, writer: l1.b1, lifecycle: run, retention: { max_items: 8 } }\n";
    let src = format!(
        "{HEAD}{stores}nodes:\n  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes:\n          - {{ id: b1, kind: task, writes: [s], emits: {{ x: a }} }}\n{GATE}        edges: []\n      max_iterations: 2\n      carried: [ {{ store: ghost_s }} ]\n      safety_points: [ {{ node: g, kind: gate }} ]\n    emits: {{ answer: a }}\n{POLICIES}"
    );
    // ghost_s 未声明（E2001，refs 域）+ 无上界（E2002）+ s 写而未携带（E2211）。
    assert_eq!(check(&src), ["E2001", "E2002", "E2211"]);
}

// ---------- 一次收齐（三缺口并犯 + 多 loop） ----------

#[test]
fn three_guards_collected_in_one_report() {
    // 单 loop 三案并犯：0 迭代 + 空 safety_points + carried 无上界
    // → 码序确定：[E2002, E2004, E2002]（检查序：迭代上限 → 安全点非空 → 上界）。
    let stores = "stores:\n  - { name: s, type: kv, value: { type: string }, writer: n0, lifecycle: run, retention: { max_items: 8 } }\n";
    let src = format!(
        "{HEAD}{stores}nodes:\n  - {{ id: n0, kind: task, writes: [s], emits: {{ answer: a }} }}\n  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes: []\n        edges: []\n      max_iterations: 0\n      carried: [ {{ store: s }} ]\n      safety_points: []\n    emits: {{ answer: a }}\n{POLICIES}"
    );
    assert_eq!(check(&src), ["E2002", "E2004", "E2002"]);
}

#[test]
fn two_loops_both_reported() {
    let src = format!(
        "{HEAD}nodes:\n  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes:\n{GATE}        edges: []\n      max_iterations: 0\n      safety_points: [ {{ node: g, kind: gate }} ]\n    emits: {{ answer: a }}\n  - id: l2\n    kind: task\n    loop:\n      body:\n        nodes:\n{GATE}        edges: []\n      max_iterations: 0\n      safety_points: [ {{ node: g, kind: gate }} ]\n    emits: {{ v: a }}\n{POLICIES}"
    );
    assert_eq!(check(&src), ["E2002", "E2002"]);
    let vs = violations(&src);
    assert!(vs[0].1.contains("l1"), "首违例应定位 l1：{}", vs[0].1);
    assert!(vs[1].1.contains("l2"), "次违例应定位 l2：{}", vs[1].1);
}

// ---------- 正例总闸（零误伤） ----------

#[test]
fn fully_guarded_loop_is_clean() {
    let stores = "stores:\n  - { name: s, type: kv, value: { type: string }, writer: n0, lifecycle: run, retention: { max_items: 8 } }\n";
    let src = format!(
        "{HEAD}{stores}nodes:\n  - {{ id: n0, kind: task, writes: [s], emits: {{ answer: a }} }}\n  - id: l1\n    kind: task\n    loop:\n      body:\n        nodes:\n{GATE}        edges: []\n      max_iterations: 8\n      carried: [ {{ store: s, max_items: 64 }} ]\n      safety_points: [ {{ node: g, kind: gate }} ]\n      exit_when: \"g.passed == true\"\n    emits: {{ answer: a }}\n{POLICIES}"
    );
    assert_eq!(check(&src), Vec::<&str>::new());
}
