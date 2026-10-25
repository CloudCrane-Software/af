//! G2b stage 3 测试：组件注册表装载 + IR 绑定 + E900x fail-loud + 逃逸注入点。
//!
//! 语料三分：①仓库正本 `af/packages`（真实组件清单——G2 门自证的同一对照
//! 面）；②golden 源 sample2/sample3（全字段投影断言）；③内联探针文档 +
//! 临时注册表（E900x 逐码精确断言）。RED 期全部红（桩 todo! panic），
//! GREEN 期全绿——先红后绿（TDD）。

use af_compiler::bind::{BindCode, BindViolation};
use af_compiler::components::{ComponentClass, ComponentRegistry, ComponentRegistryError};
use af_compiler::ir::{EffectClass, EscapeKind, ProvenanceSpec};
use af_compiler::{bind, lower};

// ============================================================
// 仓库正本注册表（G2 门自证的对照面）
// ============================================================

/// 仓库正本包根（与 af-dsl `registry_test.rs` 同口径）。
fn repo_packages_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../af/packages")
}

/// golden 语料源（af-compiler 入库副本，P2-L10 起单一事实源在本 crate）。
fn corpus_src(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata/src")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 {path:?}：{e}"))
}

fn repo_registry() -> ComponentRegistry {
    ComponentRegistry::load(repo_packages_root())
        .unwrap_or_else(|e| panic!("仓库正本 af/packages 组件清单应装载：{e}"))
}

/// 装载面：五份清单文件（根级 routes+扩容 + adaskit + autogen + sweng +
/// swarmkit）共 57 条声明（P2-L11 首批 7 + P2-L12 扩容 50——闸门 §⑤ 清款，
/// 逐名断言见 `registry_decl_test.rs`）；两次装载 digest 逐位一致（确定性）；
/// 版本表按名字可达。
#[test]
fn repo_components_registry_loads_deterministically() {
    let reg = repo_registry();
    assert_eq!(
        reg.component_count(),
        63,
        "P2-L11 首批 7 + P2-L12 扩容 50 + G4 reportkit 6（57→63，裁决§二同 commit）"
    );

    let again = repo_registry();
    assert_eq!(reg.digest(), again.digest(), "两次装载 digest 逐位一致");
    assert_eq!(reg.digest().len(), 64, "SHA-256 小写十六进制 64 字符");

    let main = reg.versions_of("route.main").expect("route.main 应注册");
    assert!(main.contains_key("1"));
    let locate = reg
        .versions_of("pkg.sweng.locate")
        .expect("pkg.sweng.locate 应注册");
    assert!(locate.contains_key("1"));
    assert!(
        reg.versions_of("pkg.nope.tool").is_none(),
        "未注册名不可达（fail-closed）"
    );
}

/// af-dsl 侧零感知：manifest.json 四包闭包不受 components.json 影响
/// （`package_count()==4` 是 af-dsl registry_test 的既有断言，此处复核
/// 组件清单文件不进图清单装载面——两套清单解耦的证据）。
#[test]
fn components_files_do_not_touch_graph_manifest_surface() {
    let graph_reg = af_dsl::registry::Registry::load(repo_packages_root())
        .expect("af-dsl 图清单装载不受 components.json 影响");
    assert_eq!(
        graph_reg.package_count(),
        5,
        "+reportkit（G4 终态，裁决§二）"
    );
    // G4 裁决§二：14 草稿闭包 ∪ G4 任务图白名单（Phase 1 +sweng
    // fix_swe_task、Phase 2 +reportkit paper_report；白名单常量正本在
    // af-dsl registry_test——计数同步与包落地同 commit）。
    assert_eq!(
        graph_reg.declared_graphs().len(),
        16,
        "14 草稿闭包 + sweng fix_swe_task + reportkit paper_report"
    );
}

// ============================================================
// golden 源绑定投影（sample2 / sample3）
// ============================================================

/// sample2（metagpt_sop）：全部 uses.* 引用解析 + effect_class/provenance
/// 回填 + AgentSpec 四元组可装载面（endpoint）。
#[test]
fn sample2_binds_all_uses_refs_with_first_class_fills() {
    let doc = af_dsl::load_str(&corpus_src("sample2.yaml")).unwrap();
    assert!(af_dsl::validate(&doc).is_ok());
    let ir = lower(&doc).unwrap();
    let reg = repo_registry();
    let report = bind(&ir, &reg);

    assert!(report.is_ok(), "sample2 应零违例：{:?}", report.violations);
    assert_eq!(report.registry_digest, reg.digest());

    // 工程师：三工具（解析后版本化引用，按引用字典序）+ effect_class 回填
    let eng = &report.bindings["build.engineer"];
    assert_eq!(eng.node_id, "engineer");
    let tools: Vec<(&str, EffectClass)> = eng
        .components
        .tools
        .iter()
        .map(|t| (t.reference.as_str(), t.effect_class.unwrap()))
        .collect();
    assert_eq!(
        tools,
        [
            ("pkg.sweng.edit@1", EffectClass::Persist),
            ("pkg.sweng.locate@1", EffectClass::Pure),
            ("pkg.sweng.view@1", EffectClass::Pure),
        ],
        "未版本化引用升格为 name@version；effect_class 注册表回填"
    );
    assert_eq!(eng.components.tools[0].class, ComponentClass::Tool);
    assert_eq!(
        eng.components.model_route.as_ref().unwrap().reference,
        "route.main@1"
    );
    assert!(eng.components.unit.is_none());
    assert!(eng.components.memory.is_empty());

    // AgentSpec 四元组可装载面：endpoint 实数据（model_route 声明原样）
    let agent = eng.agent.as_ref().expect("uses.model 节点应有 agent 投影");
    assert_eq!(agent.endpoint.model, "GLM-5.3");
    assert_eq!(
        agent.endpoint.base_url,
        "https://open.bigmodel.cn/api/anthropic"
    );
    assert_eq!(agent.endpoint.key_ref, "zhipu-coding-plan");

    // 规划师走廉价路由
    let planner = &report.bindings["planner"];
    assert_eq!(
        planner.components.model_route.as_ref().unwrap().reference,
        "route.cheap@1"
    );
    assert_eq!(
        planner.agent.as_ref().unwrap().endpoint.model,
        "GLM-5.3-Flash"
    );

    // QA 检查节点：判定单元绑定（kind=check 的评估器外部化）
    let qa = &report.bindings["build.qa"];
    let unit = qa.components.unit.as_ref().unwrap();
    assert_eq!(unit.reference, "pkg.sweng.test_unit@1");
    assert_eq!(unit.class, ComponentClass::Unit);
    assert!(qa.agent.is_none(), "无 uses.model 的节点无 agent 投影");

    // ADR-010 provenance 回填：pm 的 prd 端口声明 envelope<prd>
    let pm = &report.bindings["pm"];
    assert_eq!(
        pm.artifacts["prd"].provenance,
        Some(ProvenanceSpec {
            envelope_of: Some("prd".into()),
            eval_sha256: None
        }),
        "envelope<T> 声明 → provenance.envelope_of 回填"
    );

    // 非 LLM 节点（transform）组件面全空
    let apply = &report.bindings["build.apply"];
    assert!(apply.components.model_route.is_none());
    assert!(apply.components.tools.is_empty());
    assert!(apply.agent.is_none());
}

/// sample3（debate_kv）：嵌套体内节点（rounds.debate_map.debater）可绑定
/// 共享路由；无工具/单元引用的图零违例。
#[test]
fn sample3_binds_nested_body_node_to_shared_route() {
    let doc = af_dsl::load_str(&corpus_src("sample3.yaml")).unwrap();
    assert!(af_dsl::validate(&doc).is_ok());
    let ir = lower(&doc).unwrap();
    let report = bind(&ir, &repo_registry());
    assert!(report.is_ok(), "sample3 应零违例：{:?}", report.violations);

    let debater = &report.bindings["rounds.debate_map.debater"];
    assert_eq!(
        debater.components.model_route.as_ref().unwrap().reference,
        "route.main@1"
    );
    assert!(debater.agent.is_some());
    // 无逃逸样例：escapes 透传面为空映射
    assert!(report.escapes.is_empty());
}

// ============================================================
// 内联探针 + 临时注册表（E900x 逐码断言 + 逃逸注入）
// ============================================================

/// 临时包根（根级 components.json 写入点；测试自建自清）。
fn temp_root(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("af-compiler-g2b-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write_components(root: &std::path::Path, json: &str) {
    std::fs::write(root.join("components.json"), json).unwrap();
}

/// 探针文档：worker（model+tools）→ hatch（escape）；types/stores/policies
/// 齐备以过 af-dsl 全量校验。
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

/// E9001：未注册引用（空注册表 = fail-closed，一切引用拒绝）。
#[test]
fn e9001_unregistered_reference_is_rejected() {
    let ir = probe_ir("\"pkg.kit.hammer@1\"");
    let report = bind(&ir, &ComponentRegistry::empty());
    assert!(!report.is_ok());
    assert!(
        report
            .violations
            .iter()
            .any(|v| v.code == BindCode::RefMissing
                && v.node == "worker"
                && v.reference == "pkg.kit.hammer@1"),
        "应逐条点名：{:?}",
        report.violations
    );
}

/// E9001：引用串不可解析（内嵌 '@'）同样按引用缺失拒绝（无法对照注册表）。
#[test]
fn e9001_malformed_reference_string_is_rejected() {
    let ir = probe_ir("\"bad@@ref\"");
    let report = bind(&ir, &ComponentRegistry::empty());
    assert!(report
        .violations
        .iter()
        .any(|v| v.code == BindCode::RefMissing && v.reference == "bad@@ref"));
}

/// E9002：名已注册、请求版本未声明。
#[test]
fn e9002_version_mismatch_is_rejected() {
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
    assert!(report
        .violations
        .iter()
        .any(|v| v.code == BindCode::VersionMismatch && v.reference == "pkg.kit.hammer@2"));
    let _ = std::fs::remove_dir_all(&root);
}

/// E9002：未版本化引用命中多版本声明（解析歧义按版本不符拒）。
#[test]
fn e9002_unversioned_ref_on_multi_version_name_is_rejected() {
    let root = temp_root("e9002-multi");
    write_components(
        &root,
        r#"{"components":[
            {"class":"tool","name":"pkg.kit.hammer","version":"1","effect_class":"persist"},
            {"class":"tool","name":"pkg.kit.hammer","version":"2","effect_class":"persist"}
        ]}"#,
    );
    let reg = ComponentRegistry::load(&root).unwrap();
    let ir = probe_ir("\"pkg.kit.hammer\"");
    let report = bind(&ir, &reg);
    assert!(report
        .violations
        .iter()
        .any(|v| v.code == BindCode::VersionMismatch && v.reference == "pkg.kit.hammer"));
    let _ = std::fs::remove_dir_all(&root);
}

/// E9003：类别不符（unit 组件被 uses.tools 引用——「unit 当 tool 用」）。
#[test]
fn e9003_class_mismatch_is_rejected() {
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
    assert!(report
        .violations
        .iter()
        .any(|v| v.code == BindCode::ClassMismatch
            && v.reference == "pkg.kit.judge@1"
            && v.node == "worker"));
    let _ = std::fs::remove_dir_all(&root);
}

/// 逃逸注入点：escape 构造 → BindReport.escapes（impl_ref 保留、记账字段
/// 透传）——绑定层注入点真实存在的直接证据。
#[test]
fn escape_injection_point_passes_escape_spec_through() {
    let root = temp_root("escape");
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
    assert!(
        report.is_ok(),
        "全注册探针应零违例：{:?}",
        report.violations
    );

    let esc = &report.escapes["hatch"];
    assert_eq!(esc.kind, Some(EscapeKind::DynamicToolRegistry));
    assert_eq!(
        esc.impl_ref, "packages/kit@1.0.0/hatches/registry.py",
        "impl_ref 保留"
    );
    assert_eq!(esc.reason, "动作词表运行时解析（测试探针）");
    assert_eq!(esc.owner, "g2b-team");
    assert_eq!(esc.share_budget_pct, 10, "记账字段透传");
    let _ = std::fs::remove_dir_all(&root);
}

/// 无组件引用的文档 × 空注册表：绑定通过（组件面全空非错误）。
#[test]
fn empty_registry_binds_doc_without_uses_refs() {
    let src = r#"version: 1
meta:
  name: bare
  graph_version: "1.0"
  entry: fold
  input: q
  output: { ref: answer, from: fold }
types:
  - name: q
    schema: { type: object, required: [x], properties: { x: { type: string } } }
  - name: answer
    schema: { type: object, required: [y], properties: { y: { type: string } } }
nodes:
  - id: fold
    kind: transform
    consumes: { q: q }
    ops:
      - { op: project, port: q, into: answer }
    emits: { answer: answer }
edges: []
policies:
  budgets:
    - { id: cap, dimension: cents, scope: plan, cap: 100 }
"#;
    let doc = af_dsl::load_str(src).unwrap();
    assert!(af_dsl::validate(&doc).is_ok());
    let ir = lower(&doc).unwrap();
    let report = bind(&ir, &ComponentRegistry::empty());
    assert!(report.is_ok());
    let fold = &report.bindings["fold"];
    assert!(fold.components.model_route.is_none());
    assert!(fold.components.tools.is_empty());
    assert!(fold.agent.is_none());
}

// ============================================================
// 注册表装载纪律（fail-loud 静态校验）
// ============================================================

#[test]
fn registry_rejects_tool_without_effect_class() {
    let root = temp_root("no-effect");
    write_components(
        &root,
        r#"{"components":[{"class":"tool","name":"pkg.kit.hammer","version":"1"}]}"#,
    );
    let err = ComponentRegistry::load(&root).unwrap_err();
    assert!(
        matches!(err, ComponentRegistryError::Invalid { .. }),
        "got: {err}"
    );
    assert!(
        err.to_string().contains("effect_class"),
        "点名缺失字段：{err}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn registry_rejects_model_route_without_endpoint() {
    let root = temp_root("no-endpoint");
    write_components(
        &root,
        r#"{"components":[{"class":"model_route","name":"route.main","version":"1"}]}"#,
    );
    let err = ComponentRegistry::load(&root).unwrap_err();
    assert!(err.to_string().contains("endpoint"), "点名缺失字段：{err}");
    let _ = std::fs::remove_dir_all(&root);
}

/// 端点纪律同 af-harness validate_endpoint：base_url 必须 http(s) 且无
/// userinfo；key_ref 引用名、不存明文、不以 sk- 开头。
#[test]
fn registry_rejects_endpoint_discipline_violations() {
    for (tag, endpoint) in [
        (
            "bad-url",
            r#"{"protocol":"anthropic-messages","base_url":"ftp://x","model":"m","key_ref":"K","reasoning_tier":"off"}"#,
        ),
        (
            "user-info",
            r#"{"protocol":"anthropic-messages","base_url":"https://u:p@x","model":"m","key_ref":"K","reasoning_tier":"off"}"#,
        ),
        (
            "sk-key",
            r#"{"protocol":"anthropic-messages","base_url":"https://x","model":"m","key_ref":"sk-live","reasoning_tier":"off"}"#,
        ),
    ] {
        let root = temp_root(tag);
        write_components(
            &root,
            &format!(
                r#"{{"components":[{{"class":"model_route","name":"route.main","version":"1","endpoint":{endpoint}}}]}}"#
            ),
        );
        let err = ComponentRegistry::load(&root).unwrap_err();
        assert!(
            matches!(err, ComponentRegistryError::Invalid { .. }),
            "{tag} 应装载即拒：{err}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}

/// 同名同版本跨文件重复 → 装载即拒（首声者路径在案）。
#[test]
fn registry_rejects_duplicate_refs_across_files() {
    let root = temp_root("dup");
    write_components(
        &root,
        r#"{"components":[{"class":"tool","name":"pkg.kit.hammer","version":"1","effect_class":"pure"}]}"#,
    );
    let pkg = root.join("somepkg");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(
        pkg.join("components.json"),
        r#"{"components":[{"class":"tool","name":"pkg.kit.hammer","version":"1","effect_class":"pure"}]}"#,
    )
    .unwrap();
    let err = ComponentRegistry::load(&root).unwrap_err();
    assert!(
        matches!(err, ComponentRegistryError::DuplicateComponentRef { .. }),
        "got: {err}"
    );
    assert!(err.to_string().contains("pkg.kit.hammer@1"));
    let _ = std::fs::remove_dir_all(&root);
}

/// name 内嵌 '@' → 装载即拒（组合引用分隔符歧义，同 manifest 规范）。
#[test]
fn registry_rejects_name_containing_at_sign() {
    let root = temp_root("at-name");
    write_components(
        &root,
        r#"{"components":[{"class":"tool","name":"bad@name","version":"1","effect_class":"pure"}]}"#,
    );
    let err = ComponentRegistry::load(&root).unwrap_err();
    assert!(
        matches!(err, ComponentRegistryError::Invalid { .. }),
        "got: {err}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// 违例一次收齐：两条未注册引用同时点名（不首错即停——绑定违例与装载
/// 错误范式不同，承 af-dsl Report）。
#[test]
fn violations_are_collected_not_first_stopped() {
    let ir = probe_ir("\"pkg.kit.hammer@1\", \"pkg.kit.saw@1\"");
    let report = bind(&ir, &ComponentRegistry::empty());
    let refs: Vec<&str> = report
        .violations
        .iter()
        .map(|v: &BindViolation| v.reference.as_str())
        .collect();
    assert!(
        refs.contains(&"pkg.kit.hammer@1") && refs.contains(&"pkg.kit.saw@1"),
        "一次收齐：{refs:?}"
    );
}
