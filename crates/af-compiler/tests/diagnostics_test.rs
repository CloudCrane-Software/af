//! G2 机读诊断产物面测试（schema `af.diagnostics.v1`）。
//!
//! 三面：①逐码族产物用例——每类既有诊断（装载 E0xxx/E1xxx、校验 E2xxx、
//! 绑定 E900x）至少一条，核 code/severity/phase/相关对象标识逐字来自
//! 既有判定面；②负例——不该产诊断时产物里就是没有该条；③schema 稳定性
//! ——字段名/形状/枚举值固定（golden 字面量 + 键集断言 + 反序列化回环）。
//!
//! 纪律对照：产物面零判定零新码——所有断言的期望值均来自既有码表与
//! 既有 message（`bind_test.rs` / af-dsl 码表的同一口径）。

use af_compiler::bind::BindCode;
use af_compiler::components::ComponentRegistry;
use af_compiler::{
    bind, from_bind_violations, from_load_error, from_validate_report, lower, stamp_source_file,
    DiagLocation, Diagnostic, DiagnosticsFile, Phase, Severity, DIAGNOSTICS_SCHEMA,
};

// ============================================================
// 探针辅助（与 bind_test.rs 同款口径）
// ============================================================

/// 临时包根（测试自建自清）。
fn temp_root(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("af-compiler-g2diag-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write_components(root: &std::path::Path, json: &str) {
    std::fs::write(root.join("components.json"), json).unwrap();
}

/// 探针文档（与 bind_test.rs `probe_doc` 同一构造）：worker（model+tools）
/// → hatch（escape）；types/stores/policies 齐备以过 af-dsl 全量校验。
fn probe_doc(tools: &str) -> String {
    format!(
        r#"version: 1
meta:
  name: hatch_probe
  graph_version: "1.0"
  entry: worker
  input: probe_input
  output: {{ ref: answer, from: hatch }}
  intent: "G2b 绑定/E900x/逃逸注入探针"
types:
  - name: probe_input
    schema:
      type: object
      required: [x]
      properties: {{ x: {{ type: string }} }}
  - name: answer
    schema:
      type: object
      required: [y]
      properties: {{ y: {{ type: string }} }}
nodes:
  - id: worker
    kind: task
    uses: {{ model: route.main@1, tools: [{tools}] }}
    emits: {{ answer: answer }}
  - id: hatch
    kind: task
    escape:
      kind: dynamic_tool_registry
      impl_ref: packages/kit@1.0.0/hatches/registry.py
      reason: "动作词表运行时解析（测试探针）"
      owner: g2b-team
      share_budget_pct: 10
    consumes: {{ answer: answer }}
    emits: {{ action_space: answer }}
edges:
  - {{ from: worker, output: answer, to: hatch, input: answer }}
policies:
  budgets:
    - {{ id: cap, dimension: cents, scope: plan, cap: 100 }}
"#
    )
}

fn probe_ir(tools: &str) -> af_compiler::IrProgram {
    let doc = af_dsl::load_str(&probe_doc(tools)).unwrap_or_else(|e| panic!("探针装载失败：{e}"));
    let report = af_dsl::validate(&doc);
    assert!(report.is_ok(), "探针应过全量校验：{report}");
    lower(&doc).unwrap()
}

/// 校验探针：probe_doc 的边指向不存在的节点 `ghost`（E2001 悬空引用）。
fn dangling_edge_doc() -> String {
    probe_doc("\"pkg.kit.hammer@1\"").replace("to: hatch", "to: ghost")
}

// ============================================================
// ① 逐码族产物用例
// ============================================================

/// E0001（信封缺 version）→ `phase = load`，code/rule/node 逐字来自
/// af-dsl 既有判定面。
#[test]
fn load_envelope_e0001_projects_to_machine_entry() {
    let err = af_dsl::load_str("foo: bar").expect_err("缺 version 应信封拒绝");
    let diags = from_load_error(&err);
    assert_eq!(diags.len(), 1, "信封层单错短路：{diags:?}");
    let d = &diags[0];
    assert_eq!(d.code, "E0001");
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.phase, Phase::Load);
    assert_eq!(d.rule.as_deref(), Some("missing_version"));
    assert_eq!(d.node.as_deref(), Some("envelope"), "scope 逐字进 node 位");
    assert_eq!(d.reference, None, "非绑定面无引用串");
    assert_eq!(d.location, None, "库面不盖位置（CLI 面才知源路径）");
}

/// E1002（形状缺必填字段）→ `phase = load`（装载口 `load_str` 的形状层）。
#[test]
fn load_shape_e1002_projects_to_machine_entry() {
    let err = af_dsl::load_str("version: 1\nmeta:\n  name: probe\n")
        .expect_err("meta 缺必填字段应形状拒绝");
    let diags = from_load_error(&err);
    assert_eq!(diags.len(), 1, "形状层 serde 首错：{diags:?}");
    assert_eq!(diags[0].code, "E1002");
    assert_eq!(diags[0].phase, Phase::Load);
    assert_eq!(diags[0].rule.as_deref(), Some("missing_field"));
}

/// E2001（悬空引用）→ `phase = validate`，scope 进 node 位、rule 进
/// 规则名位。
#[test]
fn validate_e2001_projects_to_machine_entry() {
    let doc = af_dsl::load_str(&dangling_edge_doc()).unwrap();
    let report = af_dsl::validate(&doc);
    assert!(report.has_code(af_dsl::ErrorCode::DanglingReference));
    let diags = from_validate_report(&report);
    assert!(
        diags.iter().any(|d| d.code == "E2001"
            && d.phase == Phase::Validate
            && d.severity == Severity::Error
            && d.rule.as_deref() == Some("dangling_reference")
            && d.node.is_some()),
        "E2001 条目应逐字投影：{diags:?}"
    );
    assert!(
        diags.iter().all(|d| d.phase == Phase::Validate),
        "校验报告的条目全部 validate 相位：{diags:?}"
    );
}

/// E9001（引用缺失，空注册表 fail-closed）→ `phase = bind`，node/reference
/// 双标识逐字。
#[test]
fn bind_e9001_projects_to_machine_entry() {
    let ir = probe_ir("\"pkg.kit.hammer@1\"");
    let report = bind(&ir, &ComponentRegistry::empty());
    assert!(report
        .violations
        .iter()
        .any(|v| v.code == BindCode::RefMissing));
    let diags = from_bind_violations(&report.violations);
    let d = diags
        .iter()
        .find(|d| d.code == "E9001" && d.reference.as_deref() == Some("pkg.kit.hammer@1"))
        .expect("E9001（tool 引用）应在产物里");
    assert_eq!(d.phase, Phase::Bind);
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.node.as_deref(), Some("worker"));
    assert_eq!(d.reference.as_deref(), Some("pkg.kit.hammer@1"));
    assert_eq!(d.rule, None, "E900x 无规则名位（码即本体）");
    assert!(
        d.message.contains("pkg.kit.hammer"),
        "message 逐字：{}",
        d.message
    );
}

/// E9002（版本不符）→ `phase = bind`，引用串原样。
#[test]
fn bind_e9002_projects_to_machine_entry() {
    let root = temp_root("e9002");
    write_components(
        &root,
        r#"{"components":[
            {"class":"tool","name":"pkg.kit.hammer","version":"1","effect_class":"persist"}
        ]}"#,
    );
    let reg = ComponentRegistry::load(&root).unwrap();
    let ir = probe_ir("\"pkg.kit.hammer@2\"");
    let report = bind(&ir, &reg);
    let diags = from_bind_violations(&report.violations);
    assert!(
        diags.iter().any(|d| d.code == "E9002"
            && d.phase == Phase::Bind
            && d.reference.as_deref() == Some("pkg.kit.hammer@2")),
        "E9002 应逐字投影：{diags:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// E9003（类别不符：unit 当 tool 用）→ `phase = bind`。
#[test]
fn bind_e9003_projects_to_machine_entry() {
    let root = temp_root("e9003");
    write_components(
        &root,
        r#"{"components":[
            {"class":"unit","name":"pkg.kit.judge","version":"1"}
        ]}"#,
    );
    let reg = ComponentRegistry::load(&root).unwrap();
    let ir = probe_ir("\"pkg.kit.judge@1\"");
    let report = bind(&ir, &reg);
    let diags = from_bind_violations(&report.violations);
    assert!(
        diags.iter().any(|d| d.code == "E9003"
            && d.phase == Phase::Bind
            && d.node.as_deref() == Some("worker")
            && d.reference.as_deref() == Some("pkg.kit.judge@1")),
        "E9003 应逐字投影：{diags:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

// ============================================================
// ② 负例（不该产诊断时产物里就是没有该条）
// ============================================================

/// 零违例校验报告 → 空产物条目（不是省略，是空数组）。
#[test]
fn clean_validate_report_projects_to_empty_diagnostics() {
    let doc = af_dsl::load_str(&probe_doc("\"pkg.kit.hammer@1\"")).unwrap();
    let report = af_dsl::validate(&doc);
    assert!(report.is_ok());
    assert!(from_validate_report(&report).is_empty());
}

/// 绑定零违例（model + tool 引用全部可解析）→ 空产物条目。注册表面与
/// `bind_test.rs` 逃逸用例同款（route.main model_route 全端点 + hammer）。
#[test]
fn clean_bind_projects_to_empty_diagnostics() {
    let root = temp_root("clean");
    write_components(
        &root,
        r#"{"components":[
            {"class":"model_route","name":"route.main","version":"1",
             "endpoint":{"protocol":"anthropic-messages","base_url":"https://open.bigmodel.cn/api/anthropic","model":"GLM-5.3","key_ref":"zhipu-coding-plan","reasoning_tier":"off"}},
            {"class":"tool","name":"pkg.kit.hammer","version":"1","effect_class":"pure"}
        ]}"#,
    );
    let reg = ComponentRegistry::load(&root).unwrap();
    let ir = probe_ir("\"pkg.kit.hammer@1\"");
    let report = bind(&ir, &reg);
    assert!(report.is_ok(), "探针应绑定零违例：{report:?}");
    assert!(from_bind_violations(&report.violations).is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

/// 成功路径产物本体：空数组 + schema 恒在场（空 ≠ 无文件 ≠ 无 schema）。
#[test]
fn empty_file_is_schema_plus_empty_array() {
    let file = DiagnosticsFile::empty();
    assert_eq!(file.schema, DIAGNOSTICS_SCHEMA);
    assert!(file.diagnostics.is_empty());
    let json = file.to_json();
    assert!(
        json.contains("\"diagnostics\": []"),
        "空数组显式在场：{json}"
    );
    assert!(json.contains("\"schema\": \"af.diagnostics.v1\""));
}

// ============================================================
// ③ schema 稳定性（字段名/形状/枚举值固定）
// ============================================================

/// golden 字面量：E9001 条目集（model + tool 双引用，产生序 = 解析序）
/// 的完整产物逐字节形态。任何字段名/序/枚举值变化都会碰这条断言——
/// schema 冻结面。
#[test]
fn schema_golden_literal_is_frozen() {
    let ir = probe_ir("\"pkg.kit.hammer@1\"");
    let report = bind(&ir, &ComponentRegistry::empty());
    let mut diagnostics = from_bind_violations(&report.violations);
    stamp_source_file(&mut diagnostics, "probe.yaml");
    let file = DiagnosticsFile {
        schema: DIAGNOSTICS_SCHEMA.to_string(),
        diagnostics,
    };
    let expected = r#"{
  "schema": "af.diagnostics.v1",
  "diagnostics": [
    {
      "code": "E9001",
      "severity": "error",
      "message": "组件 \"route.main\" 未注册（注册表共 0 条声明）",
      "phase": "bind",
      "location": {
        "file": "probe.yaml",
        "line": null,
        "column": null
      },
      "node": "worker",
      "reference": "route.main@1",
      "rule": null
    },
    {
      "code": "E9001",
      "severity": "error",
      "message": "组件 \"pkg.kit.hammer\" 未注册（注册表共 0 条声明）",
      "phase": "bind",
      "location": {
        "file": "probe.yaml",
        "line": null,
        "column": null
      },
      "node": "worker",
      "reference": "pkg.kit.hammer@1",
      "rule": null
    }
  ]
}
"#;
    assert_eq!(file.to_json(), expected, "schema 形状冻结（v1）");
}

/// 字段名键集固定：顶层两键、条目八键、位置三键——多一个少一个都拒。
#[test]
fn schema_key_sets_are_frozen() {
    let file = DiagnosticsFile {
        schema: DIAGNOSTICS_SCHEMA.to_string(),
        diagnostics: vec![Diagnostic {
            code: "E2001".to_string(),
            severity: Severity::Error,
            message: "探针".to_string(),
            phase: Phase::Validate,
            location: Some(DiagLocation {
                file: "a.yaml".to_string(),
                line: None,
                column: None,
            }),
            node: Some("root".to_string()),
            reference: None,
            rule: Some("dangling_reference".to_string()),
        }],
    };
    let v: serde_json::Value = serde_json::from_str(&file.to_json()).unwrap();
    // 注：serde_json::Value 的键迭代是字典序（BTreeMap），与结构体序列化的
    // 声明序（golden 字面量所冻）不同——键集断言按集合比，序断言归 golden。
    let mut top: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    top.sort_unstable();
    assert_eq!(top, vec!["diagnostics", "schema"], "顶层键集固定");
    let entry = &v["diagnostics"][0];
    let mut keys: Vec<&str> = entry
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "code",
            "location",
            "message",
            "node",
            "phase",
            "reference",
            "rule",
            "severity"
        ],
        "条目键集固定"
    );
    let mut loc: Vec<&str> = entry["location"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    loc.sort_unstable();
    assert_eq!(loc, vec!["column", "file", "line"], "位置键集固定");
    assert_eq!(entry["severity"], serde_json::json!("error"), "枚举值固定");
    assert_eq!(entry["phase"], serde_json::json!("validate"), "枚举值固定");
}

/// 枚举全值域的序列化字面量固定（load/validate/bind + error）。
#[test]
fn enum_serialization_is_frozen() {
    assert_eq!(
        serde_json::to_string(&Severity::Error).unwrap(),
        "\"error\""
    );
    assert_eq!(serde_json::to_string(&Phase::Load).unwrap(), "\"load\"");
    assert_eq!(
        serde_json::to_string(&Phase::Validate).unwrap(),
        "\"validate\""
    );
    assert_eq!(serde_json::to_string(&Phase::Bind).unwrap(), "\"bind\"");
}

/// 反序列化回环：产物 JSON → 结构等值（消费侧按同一 schema 读回）。
#[test]
fn roundtrip_deserialize_matches() {
    let ir = probe_ir("\"pkg.kit.hammer@1\"");
    let report = bind(&ir, &ComponentRegistry::empty());
    let mut diagnostics = from_bind_violations(&report.violations);
    stamp_source_file(&mut diagnostics, "probe.yaml");
    let file = DiagnosticsFile {
        schema: DIAGNOSTICS_SCHEMA.to_string(),
        diagnostics,
    };
    let back: DiagnosticsFile = serde_json::from_str(&file.to_json()).unwrap();
    assert_eq!(back, file, "回环等值（schema 双向稳定）");
}

/// 位置盖章：无位置条目盖成 file 已知 + 行/列缺省；不覆盖已有位置、
/// 不造行/列。
#[test]
fn stamp_source_file_fills_file_only() {
    let mut diags = vec![Diagnostic {
        code: "E0001".to_string(),
        severity: Severity::Error,
        message: "探针".to_string(),
        phase: Phase::Load,
        location: None,
        node: Some("envelope".to_string()),
        reference: None,
        rule: Some("missing_version".to_string()),
    }];
    stamp_source_file(&mut diags, "doc.yaml");
    assert_eq!(
        diags[0].location,
        Some(DiagLocation {
            file: "doc.yaml".to_string(),
            line: None,
            column: None
        }),
        "行/列明确缺省（null），不编造"
    );
    // 已有位置不被覆盖。
    stamp_source_file(&mut diags, "other.yaml");
    assert_eq!(diags[0].location.as_ref().unwrap().file, "doc.yaml");
}

/// 确定性：同输入两次投影逐位一致（码表序 = 产生序，无重排）。
#[test]
fn projection_is_deterministic() {
    let doc = af_dsl::load_str(&dangling_edge_doc()).unwrap();
    let report = af_dsl::validate(&doc);
    let a = from_validate_report(&report);
    let b = from_validate_report(&report);
    assert_eq!(a, b);
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
}
