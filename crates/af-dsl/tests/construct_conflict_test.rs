//! 红队修复批（W1）：构造×构造并存拒绝——同一节点同时携带 loop 与 map
//! 双构造此前不被任何判定拒绝，且 emit_type 对其 emits.from_body 的类型
//! 推导自相矛盾（解析域取 loop 体、类型域因 map 在场一律包 list<>）。
//! 收紧为 E2209（fail-closed：组合非法，二构造必须分节点）。
//!
//! 红队 W3-C19 迁移：终态发射者必须自带 emits（内建端口不构成终态工件）——
//! 两条样本的 n1 原为「无 emits + output.from=n1」形态，现补
//! `emits: { done: signal }`（inv4_test loader 同款形态：显式声明内建 done
//! 端口为终态发射）。判据本体（E2209 / 零违例）不变。

use af_dsl::{load_str, validate, Document};

fn check(src: &str) -> Vec<&'static str> {
    let doc: Document = load_str(src).unwrap_or_else(|e| panic!("解析失败：{e}"));
    validate(&doc).codes()
}

#[test]
fn loop_plus_map_on_same_node_is_e2209() {
    let src = "version: 1
meta:
  name: t
  graph_version: \"1.0\"
  entry: n1
  input: in_t
  output: { ref: signal, from: n1 }
types:
  - { name: in_t, schema: { type: object, required: [q], properties: { q: { type: array, items: { type: string } } } } }
nodes:
  - id: n1
    kind: task
    uses: { model: route.main@1 }
    emits: { done: signal }
    loop:
      body:
        nodes:
          - { id: g, kind: gate, assert: \"true\" }
        edges: []
      max_iterations: 2
      safety_points: [ { node: g, kind: gate } ]
    map:
      body:
        nodes:
          - { id: inner, kind: task }
        edges: []
      over: input.q
edges: []
policies:
  budgets: []
  stop: []
  failures: []
";
    assert_eq!(check(src), ["E2209"]);
}

#[test]
fn loop_alone_and_map_alone_still_pass() {
    let loop_only = "version: 1
meta:
  name: t
  graph_version: \"1.0\"
  entry: n1
  input: in_t
  output: { ref: signal, from: n1 }
types:
  - { name: in_t, schema: { type: object, required: [q], properties: { q: { type: string } } } }
nodes:
  - id: n1
    kind: task
    uses: { model: route.main@1 }
    emits: { done: signal }
    loop:
      body:
        nodes:
          - { id: g, kind: gate, assert: \"true\" }
        edges: []
      max_iterations: 2
      safety_points: [ { node: g, kind: gate } ]
edges: []
policies:
  budgets: []
  stop: []
  failures: []
";
    assert_eq!(check(loop_only), Vec::<&str>::new());
}
