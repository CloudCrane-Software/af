//! lowering 错误路径：输入违反「已过全量校验」前提时返回 [`LowerError`]
//! （绝不 panic）——route 归一化违例（E2102 面）与空构造体（E1007 面，
//! af-dsl M1 不查、lowering 兜底）。夹具为内联 YAML（非 golden 样例），
//! 不经 common 模块。

use af_compiler::LowerError;

#[test]
fn unvalidated_route_violation_is_reported_not_panicked() {
    let src = r#"
version: 1
meta:
  name: t_route
  graph_version: "1.0"
  entry: n
  input: i
  output: { ref: a, from: n }
types:
  - { name: i, schema: { type: object } }
  - { name: a, schema: { type: object } }
stores: []
nodes:
  - id: n
    kind: task
    emits: { a: a }
    route:
      default: ghost          # E2102 面：goto 既非出边目标也非 exit
      rules: []
edges: []
policies: {}
"#;
    let doc = af_dsl::load_str(src).expect("形状应可解析");
    assert!(
        !af_dsl::validate(&doc).is_ok(),
        "前提核对：该文档确实被 stage 1 语义层拒绝"
    );
    let err = af_compiler::lower(&doc).expect_err("未过校验输入应 Err 不 panic");
    assert!(
        matches!(&err, LowerError::RouteUnnormalized { node, .. } if node == "n"),
        "实为 {err:?}"
    );
}

#[test]
fn empty_construct_body_is_reported_not_panicked() {
    let src = r#"
version: 1
meta:
  name: t_empty
  graph_version: "1.0"
  entry: n
  input: i
  output: { ref: a, from: n }
types:
  - { name: i, schema: { type: object } }
  - { name: a, schema: { type: object } }
stores: []
nodes:
  - id: n
    kind: task
    loop:
      body: { nodes: [], edges: [] }   # E1007 面：空体（af-dsl M1 不查）
      max_iterations: 3
      safety_points: []
    emits: { a: a }
edges: []
policies: {}
"#;
    let doc = af_dsl::load_str(src).expect("形状应可解析");
    let err = af_compiler::lower(&doc).expect_err("空体应 Err 不 panic");
    assert!(
        matches!(&err, LowerError::EmptyBody { construct } if construct == "loop@n"),
        "实为 {err:?}"
    );
}
