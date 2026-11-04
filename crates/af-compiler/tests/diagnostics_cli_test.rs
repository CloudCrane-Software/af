//! G2 机读产物面 CLI 测试：`af-compiler-plan --diagnostics-out <路径>`。
//!
//! 真跑 bin（`CARGO_BIN_EXE_af-compiler-plan`）四面：①失败路径（装载
//! 拒绝 / E900x 绑定拒绝）产物落盘且 code 逐字；②成功路径产物 = 空数组；
//! ③**默认行为不变**——不给旗标时：产物不落盘、退出码与 stderr 面不变；
//! ④旗标误用（缺值）退出码 2。
//!
//! 退出码契约（与 bin 文档一致）：管线失败 1；调用面错误 2；成功 0。
//! 空数组 ≠ 成功——无码操作性失败同落空数组，成败以退出码为准。

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_af-compiler-plan"))
}

fn crate_dir(p: &str) -> String {
    format!("{}/{p}", env!("CARGO_MANIFEST_DIR"))
}

fn temp_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("af-g2diag-cli-{tag}-{}.json", std::process::id()))
}

/// E0001 失败路径：装载拒绝 → 产物一条 E0001（phase=load，位置盖源路径），
/// stderr 与退出码与扩展前同面（stderr 点名 + exit 1）。
#[test]
fn cli_load_failure_writes_e0001_product() {
    let bad = temp_path("bad");
    let doc = temp_path("bad-yaml");
    std::fs::write(&doc, "foo: bar\n").unwrap();
    let out = bin()
        .arg("--diagnostics-out")
        .arg(&bad)
        .arg(&doc)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "装载拒绝退出码 1");
    assert!(String::from_utf8_lossy(&out.stderr).contains("装载拒绝"));
    let product: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&bad).unwrap()).unwrap();
    assert_eq!(product["schema"], "af.diagnostics.v1");
    assert_eq!(product["diagnostics"].as_array().unwrap().len(), 1);
    let d = &product["diagnostics"][0];
    assert_eq!(d["code"], "E0001");
    assert_eq!(d["severity"], "error");
    assert_eq!(d["phase"], "load");
    assert_eq!(d["location"]["file"], doc.to_str().unwrap());
    assert_eq!(
        d["location"]["line"],
        serde_json::Value::Null,
        "行缺省不编造"
    );
    let _ = std::fs::remove_file(&bad);
    let _ = std::fs::remove_file(&doc);
}

/// E9001 失败路径：无 --registry（空注册表 fail-closed）→ 产物含 E9001
/// （phase=bind、node/reference 逐字），exit 1。
#[test]
fn cli_bind_failure_writes_e9001_product() {
    let probe = r#"version: 1
meta:
  name: hatch_probe
  graph_version: "1.0"
  entry: worker
  input: probe_input
  output: { ref: answer, from: hatch }
  intent: "G2 机读产物面探针"
types:
  - name: probe_input
    schema:
      type: object
      required: [x]
      properties: { x: { type: string } }
  - name: answer
    schema:
      type: object
      required: [y]
      properties: { y: { type: string } }
nodes:
  - id: worker
    kind: task
    uses: { model: route.main@1, tools: ["pkg.kit.hammer@1"] }
    emits: { answer: answer }
  - id: hatch
    kind: task
    escape:
      kind: dynamic_tool_registry
      impl_ref: packages/kit@1.0.0/hatches/registry.py
      reason: "动作词表运行时解析（测试探针）"
      owner: g2b-team
      share_budget_pct: 10
    consumes: { answer: answer }
    emits: { action_space: answer }
edges:
  - { from: worker, output: answer, to: hatch, input: answer }
policies:
  budgets:
    - { id: cap, dimension: cents, scope: plan, cap: 100 }
"#;
    let doc = temp_path("probe-yaml");
    let product_path = temp_path("probe-out");
    std::fs::write(&doc, probe).unwrap();
    let out = bin()
        .arg("--diagnostics-out")
        .arg(&product_path)
        .arg(&doc)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "E900x 拒绝退出码 1");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("计划生成失败"), "stderr 面不变：{stderr}");
    let product: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&product_path).unwrap()).unwrap();
    let diags = product["diagnostics"].as_array().unwrap();
    assert!(!diags.is_empty(), "fail-closed 空注册表必产 E900x");
    let d = diags
        .iter()
        .find(|d| d["code"] == "E9001")
        .expect("E9001 应在产物（model 与 tool 引用均未注册）");
    assert_eq!(d["phase"], "bind");
    assert_eq!(d["node"], "worker");
    assert_eq!(d["reference"], "route.main@1");
    assert!(stdout_is_empty(&out), "失败路径不输出残缺计划");
    let _ = std::fs::remove_file(&doc);
    let _ = std::fs::remove_file(&product_path);
}

/// 成功路径（sample2 × 仓库正本注册表）→ 产物空数组 + schema 在场 +
/// stdout 计划 JSON 照常 + exit 0。
#[test]
fn cli_success_path_writes_empty_product() {
    let product_path = temp_path("ok");
    let out = bin()
        .arg("--registry")
        .arg(crate_dir("../../af/packages"))
        .arg("--diagnostics-out")
        .arg(&product_path)
        .arg(crate_dir("testdata/src/sample2.yaml"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "sample2 应全链成功：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with('{'), "stdout 仍是计划 JSON");
    let product: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&product_path).unwrap()).unwrap();
    assert_eq!(product["schema"], "af.diagnostics.v1");
    assert_eq!(
        product["diagnostics"],
        serde_json::json!([]),
        "成功路径零诊断条目"
    );
    let _ = std::fs::remove_file(&product_path);
}

/// **默认行为不变**：不给旗标 → 产物不落盘（文件不存在）、退出码与
/// stderr 面与扩展前一致（同一输入同退码）。
#[test]
fn cli_without_flag_writes_no_product_and_keeps_exit_code() {
    let doc = temp_path("noflag-yaml");
    std::fs::write(&doc, "foo: bar\n").unwrap();
    let untouched = temp_path("noflag-out");
    let out = bin().arg(&doc).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("装载拒绝"));
    assert!(
        !untouched.exists(),
        "不给旗标绝不写任何产物文件（默认面逐字节不变）"
    );
    let _ = std::fs::remove_file(&doc);
}

/// 旗标误用：缺值 → 退出码 2（调用面错误），不进入管线。
#[test]
fn cli_diagnostics_out_without_value_exits_2() {
    let doc = temp_path("noval-yaml");
    std::fs::write(&doc, "version: 1\n").unwrap();
    let out = bin()
        .arg("--diagnostics-out")
        .arg(&doc) // 被吞作产物路径
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "缺文档参数 → 用法错误");
    let _ = std::fs::remove_file(&doc);
}

fn stdout_is_empty(out: &std::process::Output) -> bool {
    out.stdout.is_empty()
}
