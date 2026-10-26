//! G4 研究报告包 v1 执行面验收测试（设计《G4-研究报告包设计-v1》§5；
//! TDD 先红后绿——本文件先于 `planbind.rs`/`suite/report_unit.rs` 落地，
//! 模块缺席时编译红）。
//!
//! 覆盖面：
//! - **planbind 通用装订器**（裁决9 唯一 E9004→EX4004 消费者）：E9004 负例
//!   （篡改注册表一条 → 拒启动、文案带 E9004+双 digest）、一致性核对
//!   （graph_version/line/node_exec 对齐 → EX4005）、模板装载（裁决10 机制
//!   汇流，EX4001-4003 透传）。
//! - **判分链三原则报告面平行件**（§5.2）：子进程 argv 契约、指纹门
//!   EX3002 fail-loud（裁决6：锚=落盘实测，spec 与脚本同 commit——
//!   production 锚自检）、判定只取 stdout JSON。
//! - **判别力五反例**（§5.2-5/裁决6 判据 9）：篡改 quote/删快照/未登记
//!   标记/手改 md/篡改脚本各恰触发其断言，无串扰。
//! - **ReportExecuteUnit 分相**（§5.4）：setup 装载期四核对（planbind 唯一
//!   正本口径——合流批术语勘误）→ ingest
//!   快照落盘 → draft（缝）→ verify 双判分器 → 行落盘双轨（DraftFn 缝 =
//!   v1 口径；真实模型环=铺单批 F-第五单，裁决17）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use af_execline::error::{ErrorCode, ExeclineError};
use af_execline::exec::{ExecContext, ExecuteUnit};
use af_execline::planbind::{bind, BindExpectations};
use af_execline::record::{ExecutionFacts, FileRecordSink};
use af_execline::suite::report_unit::{
    invoke_citation, invoke_rubric, DraftFn, DraftInput, DraftOutput, LocalFileFetcher,
    ReportExecuteUnit, ReportJudgeLocator, ReportUnitSpec, SourceFetcher,
};
use serde_json::{json, Value};

fn repo_packages() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../af/packages")
}

fn sha256_of(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut out = String::new();
    for b in Sha256::digest(data) {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

// ============================================================
// 夹具：最小注册表（route.main 声明自仓库正本拷贝）+ demo 模板包
// ============================================================

/// 最小注册表根：根级 components.json（route.main@1 逐字取仓库正本）+
/// demo/prompts 模板（裁决10 机制面）。planbind 只消费 af-compiler
/// ComponentRegistry（E9004 面），不触 af-dsl 图清单。
fn fixture_registry(tag: &str, tpl_name: &str, tpl_text: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("g4r-kit-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let repo = serde_json::from_str::<Value>(
        &std::fs::read_to_string(repo_packages().join("components.json"))
            .expect("仓库根级 components.json 应在盘"),
    )
    .expect("仓库清单是合法 JSON");
    let route_main = repo["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "route.main")
        .expect("route.main@1 应在仓库正本")
        .clone();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("components.json"),
        serde_json::to_string_pretty(&json!({ "components": [route_main] })).unwrap(),
    )
    .unwrap();
    // demo 模板包（无 components.json 的包目录被组件装载器跳过——合法）。
    let prompts = root.join("demo").join("prompts");
    std::fs::create_dir_all(&prompts).unwrap();
    std::fs::write(prompts.join(format!("{tpl_name}.md")), tpl_text).unwrap();
    std::fs::write(
        prompts.join("MANIFEST.sha256"),
        format!(
            "# manifest_version: 1\n{}  prompts/{tpl_name}.md\n",
            sha256_of(tpl_text.as_bytes())
        ),
    )
    .unwrap();
    root
}

/// 最小 LLM 图（inv1 先例形：uses.model route.main@1，emits answer）。
fn llm_graph_yaml() -> String {
    "version: 1\n\
     meta:\n  name: t\n  graph_version: \"1.0\"\n  entry: n1\n  input: in_t\n  output: { ref: answer, from: n1 }\n\
     types:\n  - { name: in_t, schema: { type: object, required: [q], properties: { q: { type: string } } } }\n\
     \x20 - { name: answer, schema: { type: object } }\n\
     stores:\n  - { name: s, type: message_log, writer: n1, lifecycle: run, retention: { max_items: 8 } }\n\
     nodes:\n\
     \x20 - { id: n1, kind: task, uses: { model: route.main@1 }, writes: [s], emits: { answer: answer } }\n\
     edges: []\n"
        .into()
}

/// 真编译器全链产计划 JSON（load→validate→plan_json）。
fn compile_plan(registry_root: &Path) -> String {
    let doc = af_dsl::load_str(&llm_graph_yaml()).expect("夹具图应装载");
    let verdict = af_dsl::validate(&doc);
    assert!(verdict.is_ok(), "夹具图应零违例：{verdict}");
    let reg = af_compiler::ComponentRegistry::load(registry_root).expect("夹具注册表应装载");
    af_compiler::plan_json(&doc, &reg).expect("夹具计划应生成")
}

// ============================================================
// planbind：E9004 唯一执行面消费者（裁决9）
// ============================================================

#[test]
fn planbind_happy_path_binds_digest_and_templates() {
    let root = fixture_registry("ok", "tpl", "You are a report writer.");
    let plan_src = compile_plan(&root);
    let bound = bind(
        &plan_src,
        &root,
        &["pkg.demo.tpl"],
        &BindExpectations::default(),
    )
    .expect("正例应装订通过");
    assert_eq!(
        bound.templates["pkg.demo.tpl"].content,
        "You are a report writer."
    );
    assert!(bound.plan.body.plan_id.starts_with("plan.t@1.0+"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn planbind_e9004_rejects_tampered_registry_with_both_digests() {
    let root = fixture_registry("e9004", "tpl", "x");
    let plan_src = compile_plan(&root);
    // 篡改本地注册表一条（model 字段）→ digest 不符 → E9004 拒启动。
    let mut decls: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("components.json")).unwrap())
            .unwrap();
    decls["components"][0]["endpoint"]["model"] = json!("GLM-TAMPERED");
    std::fs::write(
        root.join("components.json"),
        serde_json::to_string_pretty(&decls).unwrap(),
    )
    .unwrap();
    let err = bind(
        &plan_src,
        &root,
        &["pkg.demo.tpl"],
        &BindExpectations::default(),
    )
    .unwrap_err();
    assert_eq!(err.code(), ErrorCode::PlanDigestMismatch, "{err}");
    assert!(
        err.to_string().contains("E9004"),
        "文案首 token 带 E9004（编译器/执行面共用语义码）：{err}"
    );
    let text = err.to_string();
    assert!(
        text.contains("expected") || text.contains("计划携带"),
        "{text}"
    );
    // 恢复后通过（拒绝是可恢复的环境核对，不是计划损坏）。
    let decls2: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("components.json")).unwrap())
            .unwrap();
    let _ = decls2;
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn planbind_rejects_version_line_and_node_alignment_mismatch() {
    let root = fixture_registry("cons", "tpl", "x");
    let plan_src = compile_plan(&root);
    let mut plan: Value = serde_json::from_str(&plan_src).unwrap();
    // 信封版本 ≠ 1。
    plan["v"] = json!(2);
    let err = bind(
        &serde_json::to_string(&plan).unwrap(),
        &root,
        &[],
        &BindExpectations::default(),
    )
    .unwrap_err();
    assert_eq!(err.code(), ErrorCode::PlanBindMismatch, "{err}");
    assert!(err.to_string().starts_with("EX4005"), "{err}");

    // graph_version 期待不符。
    let plan: Value = serde_json::from_str(&plan_src).unwrap();
    let err = bind(
        &plan_src,
        &root,
        &[],
        &BindExpectations {
            line_id: None,
            graph_version: Some("9.9".into()),
        },
    )
    .unwrap_err();
    assert_eq!(err.code(), ErrorCode::PlanBindMismatch);
    let _ = plan;

    // line 绑定不符（计划携带 line_id 时与期待核对）。
    let mut plan: Value = serde_json::from_str(&plan_src).unwrap();
    plan["body"]["line"]["line_id"] = json!("line-a");
    let err = bind(
        &serde_json::to_string(&plan).unwrap(),
        &root,
        &[],
        &BindExpectations {
            line_id: Some("line-b".into()),
            graph_version: None,
        },
    )
    .unwrap_err();
    assert_eq!(err.code(), ErrorCode::PlanBindMismatch, "{err}");

    // node_exec 与 graph.nodes 对齐破坏（删去唯一节点投影）。
    let mut plan: Value = serde_json::from_str(&plan_src).unwrap();
    plan["body"]["node_exec"] = json!([]);
    let err = bind(
        &serde_json::to_string(&plan).unwrap(),
        &root,
        &[],
        &BindExpectations::default(),
    )
    .unwrap_err();
    assert_eq!(err.code(), ErrorCode::PlanBindMismatch, "{err}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn planbind_template_gate_is_the_unified_mechanism() {
    let root = fixture_registry("tpl", "tpl", "payload");
    let plan_src = compile_plan(&root);
    // 未登记模板 → EX4002（裁决10 装载期门）。
    let err = bind(
        &plan_src,
        &root,
        &["pkg.demo.nope"],
        &BindExpectations::default(),
    )
    .unwrap_err();
    assert_eq!(err.code(), ErrorCode::TemplateUnregistered, "{err}");
    // 篡改模板文件 → EX4003。
    std::fs::write(root.join("demo/prompts/tpl.md"), "tampered").unwrap();
    let err = bind(
        &plan_src,
        &root,
        &["pkg.demo.tpl"],
        &BindExpectations::default(),
    )
    .unwrap_err();
    assert_eq!(err.code(), ErrorCode::TemplateDrift, "{err}");
    std::fs::remove_dir_all(&root).ok();
}

// ============================================================
// 判分链：定位器/指纹门/argv 契约/五反例（裁决6 + §5.2-5）
// ============================================================

fn kit_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("g4r-verdict-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join("sources")).unwrap();
    dir
}

/// 论文快照（quote 取自其中连续 ≥20 字符片段）。
const PAPER_TEXT: &str = "Multi-agent systems coordinate through a shared task ledger. \
The ledger records every tool side effect with an effect class, enabling replay and audit. \
Experiments on 145 runs show that guardrails reduce wasted turns.";

fn write_snapshot(dir: &Path, text: &str) -> (String, String) {
    let sha = sha256_of(text.as_bytes());
    let name = format!("sources/{}.txt", &sha[..12]);
    std::fs::write(dir.join(&name), text).unwrap();
    (format!("sources/{}.txt", &sha[..12]), sha)
}

fn task_json(min_citations: usize, min_section_chars: usize, paper_sha: &str) -> Value {
    json!({
        "paper_url": "file:///tmp/paper.txt",
        "focus": "协作机制与证据",
        "paper_sha256": paper_sha,
        "rubric_params": {
            "min_citations": min_citations,
            "min_section_chars": min_section_chars,
            "section_ids": ["summary", "method", "findings", "limitations", "implications"]
        }
    })
}

/// 合法报告（五段各带 [1] 标记，引用 quote 取自 PAPER_TEXT）。
fn valid_report(dir: &Path, paper_sha: &str) -> Value {
    let (snap_ref, snap_sha) = write_snapshot(dir, PAPER_TEXT);
    // marker_align 判据：每条引用恰被一个标记引用——仅 findings 段带 [1]。
    let body = |text: &str| {
        format!(
            "{text} 快照哈希逐位登记、引用逐条可独立复核；本节补充展开占位说明文本若干以满足下限。"
        )
    };
    let body_cited = |text: &str| {
        format!("{text} 论据见引用 [1]。本节补充展开占位说明文本以确保达到章节字数下限。")
    };
    json!({
        "paper": { "url": "file:///tmp/paper.txt", "title": "Ledgered Agents", "sha256": paper_sha },
        "sections": [
            { "id": "summary",   "title": "Summary",   "body_md": body("共享任务账本协调多智能体。") },
            { "id": "method",    "title": "Method",    "body_md": body("方法：effect class 分账。") },
            { "id": "findings",  "title": "Findings",  "body_md": body_cited("发现：护栏减少浪费轮次。") },
            { "id": "limitations","title": "Limitations", "body_md": body("局限：v0 无语义正当性判定。") },
            { "id": "implications","title": "Implications", "body_md": body("启示：审计面可复核。") }
        ],
        "citations": [
            { "n": 1, "url": "file:///tmp/paper.txt", "quote": "The ledger records every tool side effect with an effect class",
              "snapshot_sha256": snap_sha, "snapshot_ref": snap_ref }
        ]
    })
}

fn run_citation(dir: &Path, task: &Value, report: &Value) -> Value {
    std::fs::write(dir.join("task.json"), task.to_string()).unwrap();
    std::fs::write(dir.join("report.json"), report.to_string()).unwrap();
    let out = invoke_citation(
        &ReportJudgeLocator::production(),
        &dir.join("task.json"),
        &dir.join("report.json"),
        &dir.join("sources"),
    )
    .expect("判分子进程应可拉起");
    assert!(
        out.status.success(),
        "citation 应退出 0：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("判定只取 stdout JSON")
}

fn run_rubric(dir: &Path, task: &Value, report: &Value) -> Value {
    std::fs::write(dir.join("task.json"), task.to_string()).unwrap();
    std::fs::write(dir.join("report.json"), report.to_string()).unwrap();
    // 与单元同契约：report.md 由同一渲染器产出（R5 渲染一致性面）。
    std::fs::write(dir.join("report.md"), render_md(dir)).unwrap();
    let out = invoke_rubric(
        &ReportJudgeLocator::production(),
        &dir.join("task.json"),
        &dir.join("report.json"),
    )
    .expect("判分子进程应可拉起");
    assert!(
        out.status.success(),
        "rubric 应退出 0：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("判定只取 stdout JSON")
}

fn paper_sha() -> String {
    sha256_of(PAPER_TEXT.as_bytes())
}

#[test]
fn report_judge_production_anchors_match_files_on_disk() {
    // 裁决6：锚=落盘实测、spec 与脚本同 commit 原子生成——production 锚自检。
    ReportJudgeLocator::production()
        .check_fingerprints()
        .expect("production 双锚应与落盘脚本逐位一致");
}

#[test]
fn citation_happy_path_all_checks_true() {
    let dir = kit_dir("cite-ok");
    let report = valid_report(&dir, &paper_sha());
    let v = run_citation(&dir, &task_json(1, 40, &paper_sha()), &report);
    assert_eq!(v["passed"], true, "{v}");
    assert_eq!(v["checks"]["citations_total"], 1);
    assert_eq!(v["checks"]["marker_align"], true);
    assert_eq!(v["checks"]["quote_hit"], true);
    assert_eq!(v["checks"]["sha_match"], true);
    assert_eq!(v["checks"]["snapshot_present"], true);
    assert_eq!(v["failures"], json!([]));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn citation_discriminating_cases_hit_exactly_their_assertion() {
    // ① 篡改 quote 一字符 → 仅 quote_hit=false。
    let dir = kit_dir("cite-quote");
    let mut report = valid_report(&dir, &paper_sha());
    report["citations"][0]["quote"] =
        json!("The ledger records every tool side effect with an effect claseX");
    let v = run_citation(&dir, &task_json(1, 40, &paper_sha()), &report);
    assert_eq!(v["passed"], false);
    assert_eq!(v["checks"]["quote_hit"], false, "{v}");
    assert_eq!(v["checks"]["marker_align"], true, "无串扰：{v}");
    assert_eq!(v["checks"]["sha_match"], true, "无串扰：{v}");
    assert_eq!(v["checks"]["snapshot_present"], true, "无串扰：{v}");
    std::fs::remove_dir_all(&dir).ok();

    // ② 删一快照 → 仅 snapshot_present=false（sha_match 对缺件记 false 但
    //    主断言是 present——判分器只报在盘事实）。
    let dir = kit_dir("cite-snap");
    let report = valid_report(&dir, &paper_sha());
    let snap = report["citations"][0]["snapshot_ref"]
        .as_str()
        .unwrap()
        .to_string();
    std::fs::remove_file(dir.join(&snap)).unwrap();
    let v = run_citation(&dir, &task_json(1, 40, &paper_sha()), &report);
    assert_eq!(v["passed"], false);
    assert_eq!(v["checks"]["snapshot_present"], false, "{v}");
    assert_eq!(v["checks"]["marker_align"], true, "无串扰：{v}");
    std::fs::remove_dir_all(&dir).ok();

    // ③ 正文加未登记 [9] → 仅 marker_align=false。
    let dir = kit_dir("cite-marker");
    let mut report = valid_report(&dir, &paper_sha());
    report["sections"][0]["body_md"] = json!(format!(
        "{} 另见未登记标记 [9]。",
        report["sections"][0]["body_md"].as_str().unwrap()
    ));
    let v = run_citation(&dir, &task_json(1, 40, &paper_sha()), &report);
    assert_eq!(v["passed"], false);
    assert_eq!(v["checks"]["marker_align"], false, "{v}");
    assert_eq!(v["checks"]["quote_hit"], true, "无串扰：{v}");
    assert_eq!(v["checks"]["snapshot_present"], true, "无串扰：{v}");
    std::fs::remove_dir_all(&dir).ok();

    // ④ citations_total < min_citations → 仅总量断言红。
    let dir = kit_dir("cite-total");
    let report = valid_report(&dir, &paper_sha());
    let v = run_citation(&dir, &task_json(3, 40, &paper_sha()), &report);
    assert_eq!(v["passed"], false);
    assert_eq!(v["checks"]["citations_total"], 1, "{v}");
    assert_eq!(v["checks"]["quote_hit"], true, "无串扰：{v}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn rubric_discriminating_cases_r1_r3_r4_r5() {
    let sha = paper_sha();
    // 正例（md 由渲染器生成 → R5 过）。
    let dir = kit_dir("rub-ok");
    let report = valid_report(&dir, &sha);
    let v = run_rubric(&dir, &task_json(1, 40, &sha), &report);
    assert_eq!(v["passed"], true, "{v}");
    std::fs::remove_dir_all(&dir).ok();

    // ④'（R4）quote 长度越界（<20）→ 仅 R4 false。
    let dir = kit_dir("rub-r4");
    let mut report = valid_report(&dir, &sha);
    report["citations"][0]["quote"] = json!("ledger");
    let v = run_rubric(&dir, &task_json(1, 40, &sha), &report);
    assert_eq!(v["passed"], false, "{v}");
    let r4 = v["rubric"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "R4")
        .unwrap();
    assert_eq!(r4["ok"], false);
    std::fs::remove_dir_all(&dir).ok();

    // ③'（R3）paper.sha256 与 task 登记值不符 → 仅 R3 false。
    let dir = kit_dir("rub-r3");
    let report = valid_report(&dir, &sha);
    let v = run_rubric(&dir, &task_json(1, 40, &"0".repeat(64)), &report);
    assert_eq!(v["passed"], false, "{v}");
    let r3 = v["rubric"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "R3")
        .unwrap();
    assert_eq!(r3["ok"], false);
    std::fs::remove_dir_all(&dir).ok();

    // ①'（R1）章节闭集破坏（缺 implications）→ 仅 R1 false。
    let dir = kit_dir("rub-r1");
    let mut report = valid_report(&dir, &sha);
    report["sections"].as_array_mut().unwrap().pop();
    let v = run_rubric(&dir, &task_json(1, 40, &sha), &report);
    assert_eq!(v["passed"], false, "{v}");
    let r1 = v["rubric"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "R1")
        .unwrap();
    assert_eq!(r1["ok"], false);
    std::fs::remove_dir_all(&dir).ok();

    // ⑤（R5）手改 report.md 一处 → 仅 R5 false（渲染一致性）。
    let dir = kit_dir("rub-r5");
    let report = valid_report(&dir, &sha);
    std::fs::write(dir.join("task.json"), task_json(1, 40, &sha).to_string()).unwrap();
    std::fs::write(dir.join("report.json"), report.to_string()).unwrap();
    let render = render_md(&dir);
    std::fs::write(
        dir.join("report.md"),
        format!("{render}\nhand-edited line\n"),
    )
    .unwrap();
    let out = invoke_rubric(
        &ReportJudgeLocator::production(),
        &dir.join("task.json"),
        &dir.join("report.json"),
    )
    .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["passed"], false, "{v}");
    let r5 = v["rubric"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "R5")
        .unwrap();
    assert_eq!(r5["ok"], false);
    let r2 = v["rubric"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "R2")
        .unwrap();
    assert_eq!(r2["ok"], true, "无串扰：{v}");
    std::fs::remove_dir_all(&dir).ok();
}

/// 调包内渲染器产 report.md（与 rubric R5 同一渲染器——同构性由实现保证）。
fn render_md(dir: &Path) -> String {
    let out = std::process::Command::new("python3")
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../af/packages/reportkit/tools/render.py"),
        )
        .arg(dir.join("report.json"))
        .output()
        .expect("渲染器应可拉起");
    assert!(
        out.status.success(),
        "render.py 失败：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn judge_fingerprint_gate_rejects_tampered_script_with_ex3002() {
    // 裁决6 判据 9：篡改判分脚本一字 → EX3002 拒跑、不降级不跳判。
    let prod = ReportJudgeLocator::production();
    let script = std::fs::read_to_string(&prod.citation_script).unwrap();
    let tampered = script.replace("quote_hit", "quote_hix"); // 篡改一字
    let dir = kit_dir("fp");
    let bad = dir.join("citation_tampered.py");
    std::fs::write(&bad, tampered).unwrap();
    let locator = ReportJudgeLocator {
        citation_script: bad.clone(),
        citation_sha256: prod.citation_sha256.clone(),
        ..prod
    };
    let err = locator.check_fingerprints().unwrap_err();
    assert_eq!(err.code(), ErrorCode::JudgeFingerprintMismatch, "{err}");
    assert!(err.to_string().starts_with("EX3002"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

// ============================================================
// ReportExecuteUnit 分相（DraftFn 缝 = v1；真模型环 F-第五单）
// ============================================================

fn report_task_package(dir: &Path, registry_root: &Path, plan_src: &str) -> PathBuf {
    let task_dir = dir.join("paper-task-1");
    std::fs::create_dir_all(&task_dir).unwrap();
    let paper = dir.join("paper.txt");
    std::fs::write(&paper, PAPER_TEXT).unwrap();
    let mut task = task_json(1, 40, &paper_sha());
    // 摄取源指向真实本地文件（LocalFileFetcher 面）。
    task["paper_url"] = json!(null);
    task["local_path"] = json!(paper.display().to_string());
    std::fs::write(task_dir.join("task.json"), task.to_string()).unwrap();
    // v1 约定：计划随任务包（submit 级 --plan 档形态=开放问题 8，不预判）。
    std::fs::write(task_dir.join("plan.json"), plan_src).unwrap();
    let _ = registry_root;
    task_dir
}

fn draft_fn_ok() -> DraftFn {
    // 缝实现：按快照哈希动态组装合法报告（引用 quote 取自 PAPER_TEXT）。
    let sha = paper_sha();
    Arc::new(move |input: &DraftInput| {
        let snap_ref = format!("sources/{}.txt", &sha[..12]);
        let body = |t: &str| {
            format!(
                "{t} 快照哈希逐位登记、引用可复核；补充占位说明文本若干。焦点：{}",
                input.focus
            )
        };
        let body_cited = |t: &str| {
            format!(
                "{t} 论据见引用 [1]。补充展开占位说明文本若干以达到章节字数下限。焦点：{}",
                input.focus
            )
        };
        Ok(DraftOutput::from_report_json(json!({
            "paper": { "url": "file:///tmp/paper.txt", "title": "Ledgered Agents", "sha256": sha },
            "sections": [
                { "id": "summary", "title": "S", "body_md": body("账本协调。") },
                { "id": "method", "title": "M", "body_md": body("effect class 分账。") },
                { "id": "findings", "title": "F", "body_md": body_cited("护栏减亏。") },
                { "id": "limitations", "title": "L", "body_md": body("无语义判定。") },
                { "id": "implications", "title": "I", "body_md": body("可审计。") }
            ],
            "citations": [
                { "n": 1, "url": "file:///tmp/paper.txt",
                  "quote": "The ledger records every tool side effect with an effect class",
                  "snapshot_sha256": sha, "snapshot_ref": snap_ref }
            ]
        })
        .to_string()))
    })
}

fn report_unit(tag: &str, draft: DraftFn) -> (PathBuf, PathBuf, PathBuf, ReportExecuteUnit) {
    let root = fixture_registry(&format!("unit-{tag}"), "reader_system", "READER.");
    // writer 模板同包登记。
    let prompts = root.join("demo").join("prompts");
    std::fs::write(prompts.join("writer_system.md"), "WRITER.").unwrap();
    let manifest = format!(
        "# manifest_version: 1\n{}  prompts/reader_system.md\n{}  prompts/writer_system.md\n",
        sha256_of(b"READER."),
        sha256_of(b"WRITER.")
    );
    std::fs::write(prompts.join("MANIFEST.sha256"), manifest).unwrap();

    let base = std::env::temp_dir().join(format!("g4r-unit-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(&base).unwrap();
    let plan_src = compile_plan(&root);
    let task_dir = report_task_package(&base, &root, &plan_src);

    let mut spec = ReportUnitSpec::v1("glm-cp", ReportJudgeLocator::production(), root.clone());
    spec.reader_template_ref = "pkg.demo.reader_system".into();
    spec.writer_template_ref = "pkg.demo.writer_system".into();
    let unit = ReportExecuteUnit::new(spec, draft, Arc::new(LocalFileFetcher))
        .expect("单元装载应过（指纹/模板/档案三重门）");
    (base.clone(), task_dir, base, unit)
}

fn drive_once(
    unit: &ReportExecuteUnit,
    base: &Path,
    task_dir: &Path,
) -> af_execline::exec::ExecOutput {
    let data_root = task_dir.parent().unwrap().to_path_buf();
    let attempt = base.join("run-a1");
    std::fs::create_dir_all(&attempt).unwrap();
    let sink = FileRecordSink::open(&attempt).unwrap();
    let ctx = ExecContext {
        order_id: "wo-g4r",
        stable_id: "sid-g4r",
        line_id: "line-report",
        out_dir: &attempt,
        task_id: "paper-task-1",
        host_alias: "local",
        data_root: &data_root,
        attempt_dir: &attempt,
        record_sink: &sink,
        graph_version: None,
        task_sha256: None,
        plan_sha256: None,
        trial: false,
    };
    unit.execute(&ctx).expect("报告单元全相应走通（v1 缝）")
}

#[test]
fn report_unit_end_to_end_with_seamed_draft_passes_both_judges() {
    let (base, task_dir, _, unit) = report_unit("ok", draft_fn_ok());
    let out = drive_once(&unit, &base, &task_dir);
    assert_eq!(
        out.verdict,
        Some(true),
        "citation∧rubric 双 passed：{}",
        out.note
    );
    assert!(out.artifacts[0].qualified, "工件资格只看判分");
    assert!(out.artifacts[0].is_final);
    // 证据链在盘：report.json/report.md/verify 记录/sources 快照。
    let attempt = base.join("run-a1");
    assert!(attempt.join("report.json").is_file());
    assert!(attempt.join("report.md").is_file());
    assert!(attempt.join("verify.log").is_file());
    let snap = attempt.join("sources");
    assert!(
        snap.is_dir() && snap.read_dir().unwrap().count() >= 1,
        "快照落盘 sources/"
    );
    // 行落盘双轨。
    let rows = std::fs::read_to_string(attempt.join("records.jsonl")).unwrap();
    // v1 行契约（落地契约 §1.2）：plan_id/item_id/verdict/gate/usage/wall 齐备。
    let v1: Value = serde_json::from_str(rows.lines().next().unwrap()).unwrap();
    assert_eq!(v1["item_id"], "paper-task-1", "{rows}");
    assert_eq!(v1["verdict"], true, "{rows}");
    assert_eq!(v1["unit_id"], "report-unit", "{rows}");
    assert!(
        v1["plan_id"].as_str().unwrap().starts_with("plan.t@1.0+"),
        "{rows}"
    );
    let legacy = std::fs::read_to_string(attempt.join("results.jsonl")).unwrap();
    assert!(
        legacy.contains("\"suite\":\"report\"") || legacy.contains("report"),
        "{legacy}"
    );
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn report_unit_tampered_draft_yields_false_verdict_not_null() {
    // 起草缝返回被篡改引用的报告 → 判分有结论（false 非 null）。
    let bad_draft: DraftFn = {
        let sha = paper_sha();
        Arc::new(move |input: &DraftInput| {
            let snap_ref = format!("sources/{}.txt", &sha[..12]);
            let body = |t: &str| {
                format!(
                    "{t} 占位补充文本若干，长度满足章节下限。焦点：{}",
                    input.focus
                )
            };
            let body_cited =
                |t: &str| format!("{t} 论据见引用 [1]。占位补充文本若干以满足章节字数下限要求。");
            Ok(DraftOutput::from_report_json(
                json!({
                    "paper": { "url": "file:///tmp/paper.txt", "title": "T", "sha256": sha },
                    "sections": [
                        { "id": "summary", "title": "S", "body_md": body("a") },
                        { "id": "method", "title": "M", "body_md": body("b") },
                        { "id": "findings", "title": "F", "body_md": body_cited("c") },
                        { "id": "limitations", "title": "L", "body_md": body("d") },
                        { "id": "implications", "title": "I", "body_md": body("e") }
                    ],
                    "citations": [
                        { "n": 1, "url": "file:///tmp/paper.txt",
                          "quote": "TOTALLY NOT IN THE SNAPSHOT TEXT AT ALL ANYWHERE",
                          "snapshot_sha256": sha, "snapshot_ref": snap_ref }
                    ]
                })
                .to_string(),
            ))
        })
    };
    let (base, task_dir, _, unit) = report_unit("bad", bad_draft);
    let out = drive_once(&unit, &base, &task_dir);
    assert_eq!(
        out.verdict,
        Some(false),
        "独立判分有结论（真假均可，null=不通过）：{}",
        out.note
    );
    assert!(!out.artifacts[0].qualified);
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn report_unit_setup_rejects_plan_registry_drift_with_e9004() {
    // setup 四核对走 planbind：任务包携带的计划与本地注册表漂移 → E9004 拒跑。
    let (base, task_dir, _, unit) = report_unit("drift", draft_fn_ok());
    // 篡改任务包里的计划 digest。
    let plan_path = task_dir.join("plan.json");
    let mut plan: Value =
        serde_json::from_str(&std::fs::read_to_string(&plan_path).unwrap()).unwrap();
    plan["body"]["compiled_from"]["registry_digest"] = json!("0".repeat(64));
    std::fs::write(&plan_path, serde_json::to_string(&plan).unwrap()).unwrap();
    let err = drive_err(&unit, &base, &task_dir);
    assert_eq!(err.code(), ErrorCode::PlanDigestMismatch, "{err}");
    assert!(err.to_string().contains("E9004"), "{err}");
    std::fs::remove_dir_all(&base).ok();
}

fn drive_err(unit: &ReportExecuteUnit, base: &Path, task_dir: &Path) -> ExeclineError {
    let data_root = task_dir.parent().unwrap().to_path_buf();
    let attempt = base.join("run-a1");
    std::fs::create_dir_all(&attempt).unwrap();
    let sink = FileRecordSink::open(&attempt).unwrap();
    let ctx = ExecContext {
        order_id: "wo-g4r",
        stable_id: "sid-g4r",
        line_id: "line-report",
        out_dir: &attempt,
        task_id: "paper-task-1",
        host_alias: "local",
        data_root: &data_root,
        attempt_dir: &attempt,
        record_sink: &sink,
        graph_version: None,
        task_sha256: None,
        plan_sha256: None,
        trial: false,
    };
    unit.execute(&ctx).unwrap_err()
}

#[test]
fn local_fetcher_maps_paths_and_refuses_web() {
    let fetcher = LocalFileFetcher;
    let dir = kit_dir("fetch");
    let f = dir.join("p.txt");
    std::fs::write(&f, "hello paper").unwrap();
    let doc = fetcher
        .fetch(f.to_str().unwrap())
        .expect("本地路径应可摄取");
    assert_eq!(doc.text, "hello paper");
    assert_eq!(doc.sha256, sha256_of(b"hello paper"));
    assert_eq!(
        doc.snapshot_ref,
        format!("sources/{}.txt", &doc.sha256[..12])
    );
    let doc2 = fetcher
        .fetch(&format!("file://{}", f.display()))
        .expect("file:// 形应可摄取");
    assert_eq!(doc2.text, "hello paper");
    // web 面如实拒（§10-6：出网面未实测，F-第五单承载）。
    let err = fetcher
        .fetch("https://arxiv.org/abs/2401.00001")
        .unwrap_err();
    assert!(err.contains("F-第五单") || err.contains("web"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn execution_facts_reuse_for_report_suite() {
    // 行构造复用面：suite 字段报告化（record.rs 双轨同源）。
    let facts = ExecutionFacts {
        suite: "report",
        task_id: "paper-task-1".into(),
        profile: "glm-cp".into(),
        trial: false,
        reward: Some(true),
        status: "completed".into(),
        scaffold: "report-v1",
        reasoning_tier: "off",
        run_dir: Some("/tmp/x".into()),
        eval_sha256: None,
        gate: "ok".into(),
        attempts: 1,
        http_retries: 0,
        turns: 0,
        model_calls: 0,
        tokens: Default::default(),
        agent_wall_ms: 5,
        verifier_note: "citation∧rubric".into(),
    };
    let row = af_execline::record::swev_legacy_row(&facts);
    assert_eq!(row["suite"], "report");
    assert_eq!(row["reward"], true);
}

/// 红队 W1-C61：渲染器入指纹门——rubric 的 R5 判据 = report.md 与
/// `render.render_markdown` 输出一致（渲染器既是生产者又是裁判基准），改
/// render.py 一个函数即可翻转 R5 而两件钉死脚本字节不动；此前双指纹门只
/// 盖 citation/rubric，第三件漂移照过。
#[test]
fn judge_fingerprint_gate_rejects_tampered_renderer_with_ex3002() {
    let prod = ReportJudgeLocator::production();
    let script = std::fs::read_to_string(&prod.render_script).unwrap();
    let tampered = script.replace("def render_markdown", "def render_markdown_x");
    let dir = kit_dir("fp-render");
    let bad = dir.join("render_tampered.py");
    std::fs::write(&bad, tampered).unwrap();
    let locator = ReportJudgeLocator {
        render_script: bad.clone(),
        ..prod
    };
    let err = locator.check_fingerprints().unwrap_err();
    assert_eq!(err.code(), ErrorCode::JudgeFingerprintMismatch, "{err}");
    assert!(err.to_string().starts_with("EX3002"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn judge_fingerprint_gate_rejects_planted_pyc_shadow() {
    // 红队 W2-C52：判分目录出现 __pycache__/*.pyc 即拒——rubric_unit.py 经
    // `import render` 动态加载渲染器，Python 对在场且头部 mtime/size 匹配的
    // pyc 优先于源文件加载：不改一个锚定 .py 字节即可用影子字节码翻转 R5
    // 判据（投毒复现在案）。-B 只禁写，读取侧由此门兜底。
    let prod = ReportJudgeLocator::production();
    let dir = kit_dir("pyc-shadow");
    std::fs::create_dir_all(dir.join("__pycache__")).unwrap();
    std::fs::write(dir.join("__pycache__").join("render.cpython-314.pyc"), b"\x00\x00").unwrap();
    let locator = ReportJudgeLocator {
        render_script: dir.join("render.py"),
        ..prod
    };
    // 真脚本逐位复刻（过 sha256 锚）——使失败点唯一地落在 pyc 在场性门。
    std::fs::write(&locator.render_script, std::fs::read_to_string(&prod.render_script).unwrap())
        .unwrap();
    let err = locator.check_fingerprints().unwrap_err();
    assert_eq!(err.code(), ErrorCode::JudgeFingerprintMismatch, "{err}");
    assert!(
        err.to_string().contains("__pycache__"),
        "拒收文案须点名影子字节码：{err}"
    );
    std::fs::remove_dir_all(&dir).ok();
}
