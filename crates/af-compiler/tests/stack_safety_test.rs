//! 栈安全档位回归（红队 W2-C-21）：E1054 深度阈值 1000 按 release 档标定，
//! 而 cargo build/test 默认 debug 档 antlr4rust 递归帧大一个数量级——
//! af-dsl `cel::check` 已迁 256MB 大栈工作线程（W3-C16），af-compiler
//! lower 的 `cel_ref` 此前仍在调用方（8MB 主栈）上直接 `Program::compile`：
//! debug 档深约 59 即 SIGABRT（exit 134），「深>1000 前置拒」在真实运行
//! 档位形同虚设。本面以**子进程**驱动 `af-compiler-plan` 真实档位（进程内
//! 断言在红状态会把整个测试进程一起 abort，钉不住红）：
//! - 深 60（af-dsl 判定合法）必须走完 lower/bind，以 E9001 干净拒绝——
//!   绝不允许栈溢出崩进程；
//! - 深 1001 必须被 E1054 前置拒（阈值对照钉：阈值语义不随构建档漂移）。

use std::process::Command;

fn probe_doc(depth: usize) -> String {
    let deep = format!("{}steps.turns > 0{}", "(".repeat(depth), ")".repeat(depth));
    format!(
        r#"
version: 1
meta:
  name: stack_probe
  graph_version: "1.0"
  entry: n1
  input: i
  output: {{ ref: o, from: n1 }}
types:
  - {{ name: i, schema: {{ type: object }} }}
  - {{ name: o, schema: {{ type: object }} }}
stores: []
nodes:
  - id: n1
    kind: task
    uses: {{ model: zzz.missing@1 }}
    emits: {{ o: o }}
    route: {{ default: exit, rules: [] }}
edges: []
policies:
  stop:
    - when: "{deep}"
      outcome: completed
"#
    )
}

/// 跑 af-compiler-plan 子进程，返回 (退出码, stderr)。
fn run_plan(depth: usize) -> (Option<i32>, String) {
    let dir = std::env::temp_dir().join(format!(
        "af-compiler-stack-probe-d{depth}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("临时目录可建");
    let path = dir.join("probe.yaml");
    std::fs::write(&path, probe_doc(depth)).expect("探针文档可写");
    let out = Command::new(env!("CARGO_BIN_EXE_af-compiler-plan"))
        .arg(&path)
        .output()
        .expect("af-compiler-plan 子进程可启动");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let _ = std::fs::remove_dir_all(&dir);
    (out.status.code(), stderr)
}

fn assert_no_stack_overflow(code: Option<i32>, stderr: &str) {
    assert_ne!(
        code,
        Some(134),
        "exit 134 = SIGABRT 栈溢出崩进程（红队 W2-C-21）；stderr={stderr}"
    );
    assert!(
        !stderr.to_lowercase().contains("stack overflow"),
        "stderr 出现栈溢出字样（红队 W2-C-21）；stderr={stderr}"
    );
}

#[test]
fn deep60_dsl_legal_doc_must_not_abort_compiler_in_debug_tier() {
    let (code, stderr) = run_plan(60);
    assert_no_stack_overflow(code, &stderr);
    assert!(
        code == Some(1) && stderr.contains("E9001"),
        "深 60（af-dsl 判定合法）应走完 lower/bind 以 E9001 干净拒绝（uses 未登记），\
         实得 code={code:?} stderr={stderr}"
    );
}

#[test]
fn deep1001_is_rejected_by_e1054_threshold_regardless_of_build_tier() {
    let (code, stderr) = run_plan(1001);
    assert_no_stack_overflow(code, &stderr);
    assert!(
        code == Some(1) && stderr.contains("E1054"),
        "深 1001 应被 E1054 前置拒（阈值语义不随构建档漂移），\
         实得 code={code:?} stderr={stderr}"
    );
}
