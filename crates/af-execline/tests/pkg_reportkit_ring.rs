//! F-第五单（报告环接线）验收测试——真实模型环 [`draft_fn_real_ring`]。
//!
//! TDD 先红后绿（模块缺席时编译红——G4 pkg_reportkit.rs 同手法）：
//! - **Fake 模型环驱动**（AgentRunner 缝注入 FakeRingRunner）：reader→writer
//!   双节点经 `uses.model` 档位真组装（采样覆写/policy/工具注册/预算绑定
//!   逐面断言），envelope 校验按 **paper_report 真计划 types 表**驱动；
//! - **cite_check 反馈环**：writer 首轮坏 quote → 判分器（真子进程）失败
//!   文案回喂 → 次轮修复 → 双判分过——plan `study` 循环的执行面兑现；
//! - **摄取面**：LocalFileFetcher 的 PDF 抽取（pdftotext，设计 §2.3）+
//!   ArxivWebFetcher 域白名单（离线负例）+ html_to_text 纯函数；
//! - **真模型冒烟**（`#[ignore]`：glm-cp 现取，需网络与密钥）。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use af_execline::agent::{AgentLaunch, AgentRunFailure, AgentRunOutcome, AgentRunner};
use af_execline::error::ExeclineError;
use af_execline::exec::{ExecContext, ExecuteUnit};
use af_execline::record::FileRecordSink;
use af_execline::suite::report_unit::{
    draft_fn_real_ring, html_to_text, validate_envelope, ArxivWebFetcher, LocalFileFetcher,
    ReportExecuteUnit, ReportJudgeLocator, ReportUnitSpec, RingTypes, SourceFetcher,
};
use af_harness::{RunReport, RunStatus, TokenTotals};
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

const PAPER_TEXT: &str = "Multi-agent systems coordinate through a shared task ledger. \
The ledger records every tool side effect with an effect class, enabling replay and audit. \
Experiments on 145 runs show that guardrails reduce wasted turns.";

fn paper_sha() -> String {
    sha256_of(PAPER_TEXT.as_bytes())
}

// ============================================================
// 真计划夹具：paper_report@1.0 全链编译（真注册表 = 仓库 af/packages）
// ============================================================

/// 真编译器全链产 paper_report 计划（load→validate→plan_json；registry_root
/// 必须用仓库正本——E9004 digest 与计划携带值一致才过 planbind）。
fn real_report_plan() -> String {
    let yaml =
        std::fs::read_to_string(repo_packages().join("reportkit/graphs/paper_report@1.0.afb"))
            .expect("paper_report@1.0.afb 应在包内");
    let doc = af_dsl::load_str(&yaml).expect("paper_report 图应装载");
    let verdict = af_dsl::validate(&doc);
    assert!(verdict.is_ok(), "paper_report 图应零违例：{verdict}");
    let reg = af_compiler::ComponentRegistry::load(repo_packages()).expect("仓库注册表应装载");
    af_compiler::plan_json(&doc, &reg).expect("paper_report 计划应生成")
}

/// 报告任务包（v1 约定布局 <data_root>/<task>/{task.json, plan.json}）。
fn ring_task_package(base: &Path, plan_src: &str) -> PathBuf {
    let task_dir = base.join("paper-task-1");
    std::fs::create_dir_all(&task_dir).unwrap();
    let paper = base.join("paper.txt");
    std::fs::write(&paper, PAPER_TEXT).unwrap();
    std::fs::write(
        task_dir.join("task.json"),
        json!({
            "local_path": paper.display().to_string(),
            "focus": "协作机制与证据链",
            "paper_sha256": paper_sha(),
            "rubric_params": {
                "min_citations": 1,
                "min_section_chars": 40,
                "section_ids": ["summary", "method", "findings", "limitations", "implications"]
            }
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(task_dir.join("plan.json"), plan_src).unwrap();
    task_dir
}

fn ring_unit(tag: &str, runner: Arc<dyn AgentRunner>) -> (PathBuf, PathBuf, ReportExecuteUnit) {
    let base = std::env::temp_dir().join(format!("g5-ring-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(&base).unwrap();
    let plan_src = real_report_plan();
    let task_dir = ring_task_package(&base, &plan_src);
    let spec = ReportUnitSpec::v1("glm-cp", ReportJudgeLocator::production(), repo_packages());
    let unit = ReportExecuteUnit::new(
        spec.clone(),
        draft_fn_real_ring(runner, spec),
        Arc::new(LocalFileFetcher),
    )
    .expect("真环单元装载应过三重门（真计划+真模板+真锚）");
    (base, task_dir, unit)
}

fn drive(unit: &ReportExecuteUnit, base: &Path, task_dir: &Path) -> af_execline::ExecOutput {
    let data_root = task_dir.parent().unwrap().to_path_buf();
    let attempt = base.join("run-a1");
    std::fs::create_dir_all(&attempt).unwrap();
    let sink = FileRecordSink::open(&attempt).unwrap();
    let ctx = ExecContext {
        order_id: "wo-g5",
        stable_id: "sid-g5",
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
    unit.execute(&ctx).expect("真环单元全相应走通")
}

fn drive_err(unit: &ReportExecuteUnit, base: &Path, task_dir: &Path) -> ExeclineError {
    let data_root = task_dir.parent().unwrap().to_path_buf();
    let attempt = base.join("run-a1");
    std::fs::create_dir_all(&attempt).unwrap();
    let sink = FileRecordSink::open(&attempt).unwrap();
    let ctx = ExecContext {
        order_id: "wo-g5",
        stable_id: "sid-g5",
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

// ============================================================
// FakeRingRunner：按节点形状分派，驱动真工具注册/提交通路
// ============================================================

/// 缝输入形状（断言真环的 AgentLaunch 组装面——swev FakeRunner 同手法）。
#[derive(Clone, Debug)]
struct CapturedRingLaunch {
    run_id: String,
    attempt_dir: PathBuf,
    user_message: String,
    system: String,
    tool_names: Vec<String>,
    extra_tool_names: Vec<String>,
    completion_signals: Vec<String>,
    max_turns: u32,
    per_turn: u64,
    tool_free_turn_limit: u32,
    temperature_milli: Option<u32>,
    top_p_milli: Option<u32>,
    max_tokens: Option<u32>,
    spend_cap_cents: i64,
    tool_spend_cents: i64,
    budget_ledger_cents: i64,
}

impl CapturedRingLaunch {
    fn is_reader(&self) -> bool {
        self.tool_names.iter().any(|t| t == "submit_notes")
    }
}

/// 合法笔记（锚点取自 PAPER_TEXT 连续原文）。
fn good_notes() -> Value {
    json!([
        { "point": "护栏在 145 个 run 上减少浪费轮次",
          "anchor": "Experiments on 145 runs show that guardrails reduce wasted turns." },
        { "point": "账本按 effect class 记每个工具副作用",
          "anchor": "The ledger records every tool side effect with an effect class" }
    ])
}

/// writer 报告构造（quote 来自 PAPER_TEXT；bad_quote = 快照外文本）。
/// 章节正文 ≥60 字符（夹具 task min_section_chars=40 留余量）。
fn writer_report(quote: &str) -> Value {
    let sha = paper_sha();
    let snap_ref = format!("sources/{}.txt", &sha[..12]);
    let body = |t: &str| {
        format!(
            "{t} 快照逐位登记、引用可独立复核；本节补充展开占位说明文本若干 \
             以满足章节字数下限并保持结构完整。"
        )
    };
    let body_cited = |t: &str| {
        format!(
            "{t} 论据见引用 [1]。本节补充展开占位说明文本若干以满足章节字数下限要求，\
             并保持与引用标记的逐条对齐。"
        )
    };
    json!({
        "paper": { "url": "file:///tmp/paper.txt", "title": "Ledgered Agents", "sha256": sha },
        "sections": [
            { "id": "summary",   "title": "S", "body_md": body("账本协调多智能体。") },
            { "id": "method",    "title": "M", "body_md": body("effect class 分账。") },
            { "id": "findings",  "title": "F", "body_md": body_cited("护栏减亏。") },
            { "id": "limitations","title": "L", "body_md": body("无语义判定。") },
            { "id": "implications","title": "I", "body_md": body("可审计。") }
        ],
        "citations": [
            { "n": 1, "url": "file:///tmp/paper.txt", "quote": quote,
              "snapshot_sha256": sha, "snapshot_ref": snap_ref }
        ]
    })
}

fn good_quote() -> &'static str {
    "The ledger records every tool side effect with an effect class"
}

/// 真环 run 报告（Completed + 用量），fake 经 extra_tools 真调用提交通路。
struct FakeRingRunner {
    /// reader 提交的笔记载荷。
    notes: Value,
    /// writer 各轮报告载荷（第 i 轮取 writer_reports[min(i, len-1)]）。
    writer_reports: Vec<Value>,
    launches: Mutex<Vec<CapturedRingLaunch>>,
}

impl FakeRingRunner {
    fn happy() -> Arc<Self> {
        Arc::new(Self {
            notes: good_notes(),
            writer_reports: vec![writer_report(good_quote())],
            launches: Mutex::new(Vec::new()),
        })
    }

    fn captured(&self) -> Vec<CapturedRingLaunch> {
        self.launches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl AgentRunner for FakeRingRunner {
    fn run(&self, launch: AgentLaunch<'_>) -> Result<AgentRunOutcome, AgentRunFailure> {
        let names: Vec<String> = launch.spec.toolset.iter().map(|t| t.name.clone()).collect();
        let extra: Vec<String> = launch.extra_tools.iter().map(|t| t.name.clone()).collect();
        let is_reader = names.iter().any(|t| t == "submit_notes");
        let call_extra = |want: &str, payload: Value| -> Result<(), String> {
            let binding = launch
                .extra_tools
                .iter()
                .find(|t| t.name == want)
                .ok_or_else(|| format!("extra_tools 缺 {want}: {extra:?}"))?;
            (binding.handler)(&payload).map(|_| ())
        };
        let (turns, model_calls, tokens) = if is_reader {
            call_extra("submit_notes", json!({ "notes": self.notes })).map_err(|err| {
                AgentRunFailure {
                    message: err,
                    http_retries: 0,
                }
            })?;
            (
                2u32,
                2u64,
                TokenTotals {
                    input: 1000,
                    output: 200,
                    cache_read: 0,
                },
            )
        } else {
            let prior = self.captured().iter().filter(|l| !l.is_reader()).count();
            let report = self
                .writer_reports
                .get(prior)
                .unwrap_or(self.writer_reports.last().expect("至少一版 writer 报告"));
            call_extra("submit_report", json!({ "report": report })).map_err(|err| {
                AgentRunFailure {
                    message: err,
                    http_retries: 0,
                }
            })?;
            (
                1u32,
                1u64,
                TokenTotals {
                    input: 500,
                    output: 400,
                    cache_read: 0,
                },
            )
        };
        let signal = launch
            .spec
            .policy
            .stop
            .completion_signals
            .first()
            .cloned()
            .unwrap_or_default();
        let captured = CapturedRingLaunch {
            run_id: launch.run_id.clone(),
            attempt_dir: launch.attempt_dir.to_path_buf(),
            user_message: launch.user_message.clone(),
            system: launch.spec.context_template.system.clone(),
            tool_names: names,
            extra_tool_names: extra,
            completion_signals: launch.spec.policy.stop.completion_signals.clone(),
            max_turns: launch.spec.policy.max_turns,
            per_turn: launch.spec.policy.budget_ref.per_turn,
            tool_free_turn_limit: launch.spec.policy.stop.tool_free_turn_limit,
            temperature_milli: launch.spec.endpoint.temperature_milli,
            top_p_milli: launch.spec.endpoint.top_p_milli,
            max_tokens: launch.spec.endpoint.max_tokens,
            spend_cap_cents: launch.spend_cap_cents,
            tool_spend_cents: launch.tool_spend_cents,
            budget_ledger_cents: launch.budget_ledger_cents,
        };
        self.launches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(captured);
        Ok(AgentRunOutcome {
            report: RunReport {
                run_id: launch.run_id.clone(),
                status: RunStatus::Completed { signal },
                turns,
                model_calls,
                effect_replays: 0,
                effect_fresh: 0,
                transcript: Vec::new(),
                whitelist: Vec::new(),
                tokens,
            },
            http_retries: 1,
        })
    }
}

// ============================================================
// ① Fake 环 happy：reader→writer 驱动 + 组装面 + envelope + 双判分 + 行落盘
// ============================================================

#[test]
fn ring_fake_reader_writer_drive_passes_envelope_and_judges() {
    let runner = FakeRingRunner::happy();
    let (base, task_dir, unit) = ring_unit("ok", runner.clone());
    let out = drive(&unit, &base, &task_dir);
    assert_eq!(out.verdict, Some(true), "双判分过：{}", out.note);

    let launches = runner.captured();
    assert_eq!(launches.len(), 2, "恰 reader+writer 两节点：{launches:?}");
    let (reader, writer) = (&launches[0], &launches[1]);
    assert!(reader.is_reader(), "首节点 = reader");
    assert_eq!(reader.run_id, "report-reader");
    assert_eq!(writer.run_id, "report-writer-i1");

    // 组装面（uses.model 档位真组装）：模板来自真包装载、采样覆写、
    // policy（llm.cents 每轮 1 分）、完成信号、预算绑定。
    let repo_reader_tpl =
        std::fs::read_to_string(repo_packages().join("reportkit/prompts/reader_system.md"))
            .unwrap();
    let repo_writer_tpl =
        std::fs::read_to_string(repo_packages().join("reportkit/prompts/writer_system.md"))
            .unwrap();
    assert_eq!(
        reader.system, repo_reader_tpl,
        "reader system = pkg.reportkit.reader_system"
    );
    assert_eq!(
        writer.system, repo_writer_tpl,
        "writer system = pkg.reportkit.writer_system"
    );
    for l in [&launches[0], &launches[1]] {
        assert_eq!(l.temperature_milli, Some(0), "采样覆写 temperature=0");
        assert_eq!(l.top_p_milli, Some(1000));
        assert_eq!(l.max_tokens, Some(32000));
        assert_eq!(l.per_turn, 1, "llm.cents 每 turn 1 分（swev 同款预算记账）");
        assert_eq!(l.completion_signals, vec!["REPORT_DONE".to_string()]);
        // 红队 W1-C33：环级共享预算账本——reader 拿环级满帽（300¢ = min
        // (spec 帽, plan cost_cap)），writer 拿剩余额度（扣 reader 已用
        // 2¢——此前每成员各拿满份帽，图级上限 4×300¢）。
        assert_eq!(l.tool_spend_cents, 1);
        assert!(l.budget_ledger_cents > 0, "预算账本绑定在场");
    }
    assert_eq!(
        launches[0].spend_cap_cents, 300,
        "reader 拿环级满帽（W1-C33 共享账本）"
    );
    assert_eq!(
        launches[1].spend_cap_cents, 298,
        "writer 拿剩余额度（环级帽 300 − reader 已用 2¢，W1-C33）"
    );
    assert!(
        launches[1].spend_cap_cents < launches[0].spend_cap_cents,
        "环内后续成员帽须收敛到剩余额度"
    );
    assert_eq!(reader.max_turns, 40, "spec v1 图面口径 turns 40");
    assert_eq!(reader.tool_free_turn_limit, 3);

    // 工具注册面：reader 携 web_fetch+submit_notes；writer 携 submit_report。
    assert!(
        reader.tool_names.contains(&"web_fetch".to_string()),
        "{:?}",
        reader.tool_names
    );
    assert!(reader.tool_names.contains(&"submit_notes".to_string()));
    assert_eq!(writer.tool_names, vec!["submit_report".to_string()]);
    assert!(reader.extra_tool_names.contains(&"web_fetch".to_string()));
    assert!(reader
        .extra_tool_names
        .contains(&"submit_notes".to_string()));
    assert!(writer
        .extra_tool_names
        .contains(&"submit_report".to_string()));

    // 节点 run 目录分立（harness.db 不互踩）。
    assert!(
        reader.attempt_dir.ends_with("ring/reader"),
        "{}",
        reader.attempt_dir.display()
    );
    assert!(
        writer.attempt_dir.ends_with("ring/writer-i1"),
        "{}",
        writer.attempt_dir.display()
    );

    // writer 上下文：paper 值逐字 + 笔记边（notes_in）+ 快照登记表 + 下限。
    let sha = paper_sha();
    assert!(writer.user_message.contains(&sha), "writer 见 paper.sha256");
    assert!(
        writer
            .user_message
            .contains(&format!("sources/{}.txt", &sha[..12])),
        "writer 见快照登记表（snapshot_ref）"
    );
    assert!(
        writer.user_message.contains("护栏在 145 个 run"),
        "writer 见 reader 笔记（notes 边）"
    );
    assert!(
        writer.user_message.contains("min_citations"),
        "writer 见引用下限"
    );
    // reader 上下文：焦点 + 论文快照截头。
    assert!(
        reader.user_message.contains("协作机制与证据链"),
        "reader 见焦点"
    );
    assert!(
        reader.user_message.contains("shared task ledger"),
        "reader 见论文正文"
    );

    // 行落盘：环用量进 v1 行（usage 双节点聚合）。
    let attempt = base.join("run-a1");
    let rows = std::fs::read_to_string(attempt.join("records.jsonl")).unwrap();
    let v1: Value = serde_json::from_str(rows.lines().next().unwrap()).unwrap();
    assert_eq!(v1["usage"]["turns"], 3, "reader 2 + writer 1：{rows}");
    assert_eq!(v1["usage"]["unit_calls"], 3);
    assert_eq!(v1["usage"]["quota"]["input"], 1500);
    assert_eq!(v1["usage"]["quota"]["output"], 600);
    assert_eq!(v1["usage"]["http_retries"], 2, "两节点各 1 次");
    assert_eq!(v1["verdict"], true);
    assert!(
        v1["plan_id"]
            .as_str()
            .unwrap()
            .starts_with("plan.paper_report@1.0+"),
        "真 paper_report 计划：{rows}"
    );
    // 环工件在盘：sources ledger（reader artifact_contract 记账）+ 各轮草稿。
    assert!(attempt.join("ring/sources_ledger.json").is_file());
    assert!(attempt.join("ring/draft-i1.json").is_file());
    std::fs::remove_dir_all(&base).ok();
}

// ============================================================
// ② envelope 负例：按真计划 types 表拒绝
// ============================================================

#[test]
fn ring_envelope_rejects_notes_missing_anchor() {
    let runner = Arc::new(FakeRingRunner {
        notes: json!([{ "point": "缺锚点的笔记" }]),
        writer_reports: vec![writer_report(good_quote())],
        launches: Mutex::new(Vec::new()),
    });
    let (base, task_dir, unit) = ring_unit("env-note", runner);
    let err = drive_err(&unit, &base, &task_dir);
    let text = err.to_string();
    assert!(
        text.contains("note_item"),
        "envelope 文案点名计划类型：{text}"
    );
    assert!(text.contains("anchor"), "缺失字段点名：{text}");
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn ring_envelope_rejects_report_missing_snapshot_ref() {
    let mut report = writer_report(good_quote());
    report["citations"][0]
        .as_object_mut()
        .unwrap()
        .remove("snapshot_ref");
    let runner = Arc::new(FakeRingRunner {
        notes: good_notes(),
        writer_reports: vec![report],
        launches: Mutex::new(Vec::new()),
    });
    let (base, task_dir, unit) = ring_unit("env-report", runner);
    let err = drive_err(&unit, &base, &task_dir);
    let text = err.to_string();
    assert!(
        text.contains("research_report"),
        "envelope 文案点名计划类型：{text}"
    );
    assert!(
        text.contains("snapshot_ref"),
        "五字段 required 兑现：{text}"
    );
    std::fs::remove_dir_all(&base).ok();
}

// ============================================================
// ③ cite_check 反馈环：坏 quote → 判分失败回喂 → 次轮修复
// ============================================================

#[test]
fn ring_cite_feedback_drives_second_writer_round() {
    let runner = Arc::new(FakeRingRunner {
        notes: good_notes(),
        writer_reports: vec![
            writer_report("TOTALLY NOT IN THE SNAPSHOT TEXT AT ALL ANYWHERE"),
            writer_report(good_quote()),
        ],
        launches: Mutex::new(Vec::new()),
    });
    let (base, task_dir, unit) = ring_unit("cite-loop", runner.clone());
    let out = drive(&unit, &base, &task_dir);
    assert_eq!(out.verdict, Some(true), "次轮修复后双判分过：{}", out.note);

    let launches = runner.captured();
    let writers: Vec<&CapturedRingLaunch> = launches.iter().filter(|l| !l.is_reader()).collect();
    assert_eq!(
        writers.len(),
        2,
        "study 环两轮 writer（max_iterations 3 内）"
    );
    assert!(writers[0].attempt_dir.ends_with("ring/writer-i1"));
    assert!(writers[1].attempt_dir.ends_with("ring/writer-i2"));
    assert!(
        writers[1]
            .user_message
            .contains("quote not found in snapshot"),
        "cite_check 失败文案回喂次轮：{}",
        writers[1].user_message
    );
    // 两轮草稿均在盘（证据不覆写）。
    let attempt = base.join("run-a1");
    assert!(attempt.join("ring/draft-i1.json").is_file());
    assert!(attempt.join("ring/draft-i2.json").is_file());
    std::fs::remove_dir_all(&base).ok();
}

// ============================================================
// ④ 摄取面：PDF 抽取 + arXiv 白名单 + html_to_text
// ============================================================

/// 最小合法 PDF（单页单行文本——pdftotext 实测可抽）。
fn minimal_pdf() -> Vec<u8> {
    let stream =
        b"BT /F1 12 Tf 72 720 Td (Minimal research paper snapshot for the report ring.) Tj ET";
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec(),
        format!("<< /Length {} >>\nstream\n", stream.len())
            .into_bytes()
            .iter()
            .cloned()
            .chain(stream.iter().cloned())
            .chain(b"\nendstream".to_vec())
            .collect(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];
    for (i, body) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in &offsets {
        out.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            objs.len() + 1,
            xref
        )
        .as_bytes(),
    );
    out
}

#[test]
fn pdf_extract_via_pdftotext_lands_snapshot() {
    // 设计 §2.3：pdf_extract 走快照契约（url=file://、text=pdftotext 抽取、
    // sha256=抽取文本指纹、snapshot_ref=sources/<sha12>.txt）。
    let dir = std::env::temp_dir().join(format!("g5-ring-pdf-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let pdf = dir.join("paper.pdf");
    std::fs::write(&pdf, minimal_pdf()).unwrap();

    let doc = LocalFileFetcher
        .fetch(pdf.to_str().unwrap())
        .expect("本地 PDF 应可抽取");
    assert!(
        doc.text.contains("Minimal research paper snapshot"),
        "pdftotext 抽取文本：{}",
        doc.text
    );
    assert_eq!(doc.url, format!("file://{}", pdf.display()));
    let text_norm = doc.text.trim_end().to_string();
    assert_eq!(
        doc.sha256,
        sha256_of(text_norm.as_bytes()),
        "指纹=规范化抽取文本"
    );
    assert_eq!(
        doc.snapshot_ref,
        format!("sources/{}.txt", &doc.sha256[..12])
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn arxiv_web_fetcher_whitelist_refuses_non_arxiv_offline() {
    // 域白名单（设计 §2.2：v1 限 arxiv.org 域）——离线负例，零网络。
    for url in [
        "https://example.com/abs/2401.00001",
        "https://evil.arxiv.org.attacker.io/abs/2401.00001",
    ] {
        let err = ArxivWebFetcher.fetch(url).unwrap_err();
        assert!(
            err.contains("arxiv") || err.contains("白名单"),
            "白名单拒绝文案：{url} → {err}"
        );
    }
    // 明文 http 拒绝（协议面先于白名单）。
    let err = ArxivWebFetcher
        .fetch("http://export.arxiv.org/abs/2401.00001")
        .unwrap_err();
    assert!(err.contains("https"), "明文 http 拒绝：{err}");
    // file:// 走本地面，不进 web 面。
    assert!(ArxivWebFetcher.fetch("file:///tmp/x.txt").is_err());
}

/// 红队 W2-C31：重定向逐跳 host 复核——curl 不再 -L 裸跟，白名单必须在
/// **每一跳**（含重定向落点）成立；本白名单外目标拒绝落盘。
#[test]
fn arxiv_web_fetcher_revalidates_every_redirect_hop_host() {
    // 白名单内三 host 均可作重定向落点。
    for url in [
        "https://export.arxiv.org/abs/2401.00001",
        "https://arxiv.org/abs/2401.00001",
        "https://www.arxiv.org/abs/2401.00001",
    ] {
        assert!(
            ArxivWebFetcher::check_hop(url).is_ok(),
            "白名单内落点应放行：{url}"
        );
    }
    // 白名单外/仿冒域/明文 http 一律拒。
    for (url, why) in [
        (
            "https://attacker.example.com/abs/2401.00001",
            "白名单外主机",
        ),
        ("https://arxiv.org.evil.io/abs/2401.00001", "仿冒域"),
        ("https://evil-arxiv.org/abs/2401.00001", "形近域"),
        ("http://export.arxiv.org/abs/2401.00001", "明文 http"),
    ] {
        let err = ArxivWebFetcher::check_hop(url).unwrap_err();
        assert!(
            err.contains("白名单") || err.contains("https"),
            "{why}：{url} → {err}"
        );
    }
}

/// 红队 W2-C31：相对 Location 解析——绝对 https 原样、根相对并入现 host、
/// 非 https 绝对目标拒绝（协议面不因重定向放宽）。
#[test]
fn arxiv_web_fetcher_resolves_relative_redirect_locations() {
    let base = "https://export.arxiv.org/abs/2401.00001v1";
    assert_eq!(
        ArxivWebFetcher::resolve_redirect(base, "https://arxiv.org/abs/2401.00001").unwrap(),
        "https://arxiv.org/abs/2401.00001",
        "绝对 https Location 原样"
    );
    assert_eq!(
        ArxivWebFetcher::resolve_redirect(base, "/abs/2401.00002").unwrap(),
        "https://export.arxiv.org/abs/2401.00002",
        "根相对 Location 并入现 host"
    );
    let err = ArxivWebFetcher::resolve_redirect(base, "http://arxiv.org/abs/2401.00001")
        .unwrap_err();
    assert!(err.contains("https"), "非 https 绝对 Location 拒绝：{err}");
}

#[test]
fn html_to_text_strips_scripts_tags_and_entities() {
    let html = "<html><head><title>T</title><style>body{hidden}</style>\
                <script>secret()</script></head>\
                <body><h1>Attention Is All You Need</h1><p>A &amp; B &lt;tags&gt;</p></body></html>";
    let text = html_to_text(html);
    assert!(text.contains("Attention Is All You Need"), "{text}");
    assert!(text.contains("A & B <tags>"), "实体解码：{text}");
    assert!(!text.contains("<h1>"), "标签剥除：{text}");
    assert!(!text.contains("hidden"), "style 块剥除：{text}");
    assert!(!text.contains("secret"), "script 块剥除：{text}");
}

// ============================================================
// ⑤ envelope 校验器直测（真计划 types 表驱动）
// ============================================================

#[test]
fn envelope_validator_follows_real_plan_types() {
    let plan: Value = serde_json::from_str(&real_report_plan()).unwrap();
    let types = plan["body"]["types"].as_object().unwrap().clone();
    let map: std::collections::BTreeMap<String, Value> = types.into_iter().collect();
    let ring_types =
        RingTypes::from_plan_types(&map).expect("真计划应带 note_item/research_report 类型");

    // note_item：point+anchor 双 required。
    assert!(validate_envelope(&ring_types.note_item, &good_notes()[0]).is_ok());
    let mut missing = good_notes()[0].clone();
    missing.as_object_mut().unwrap().remove("anchor");
    let err = validate_envelope(&ring_types.note_item, &missing).unwrap_err();
    assert!(err.contains("anchor"), "{err}");

    // research_report：五字段 citation required（裁决4）。
    let good = writer_report(good_quote());
    assert!(validate_envelope(&ring_types.research_report, &good).is_ok());
    let mut bad = good.clone();
    bad["citations"][0]
        .as_object_mut()
        .unwrap()
        .remove("snapshot_sha256");
    let err = validate_envelope(&ring_types.research_report, &bad).unwrap_err();
    assert!(err.contains("snapshot_sha256"), "{err}");
    // 章节缺 body_md 也拒。
    let mut bad2 = good;
    bad2["sections"][0]
        .as_object_mut()
        .unwrap()
        .remove("body_md");
    assert!(validate_envelope(&ring_types.research_report, &bad2).is_err());
}

#[test]
fn ring_types_reject_plan_without_report_types() {
    // 计划缺类型 → 环拒跑（不静默用内置兜底）。
    let empty = std::collections::BTreeMap::new();
    let err = RingTypes::from_plan_types(&empty).unwrap_err();
    assert!(err.contains("note_item") || err.contains("类型"), "{err}");
}

// ============================================================
// ⑥ 真模型冒烟（glm-cp 现取；原样 cargo test 不跑——需网络+密钥）
// ============================================================

#[test]
#[ignore = "真模型冒烟：glm-cp 现取（网络+密钥）；cargo test --ignored --test pkg_reportkit_ring real_model_ring_smoke_minimal_plan"]
fn real_model_ring_smoke_minimal_plan() {
    // 最小 plan：真 paper_report 图（单 reader + 单 writer + cite_check 环），
    // 短文产出（min_section_chars=40、min_citations=1）。
    let runner = Arc::new(af_execline::HarnessRunner::new().expect("HarnessRunner"));
    let (base, task_dir, unit) = ring_unit("smoke", runner);
    let started = std::time::Instant::now();
    let out = drive(&unit, &base, &task_dir);
    let attempt = base.join("run-a1");
    println!(
        "冒烟 verdict={:?} note={} wall={}ms\nreport.md:\n{}",
        out.verdict,
        out.note,
        started.elapsed().as_millis(),
        std::fs::read_to_string(attempt.join("report.md")).unwrap_or_default()
    );
    assert_eq!(out.verdict, Some(true), "真模型环双判分过：{}", out.note);
    let report: Value =
        serde_json::from_str(&std::fs::read_to_string(attempt.join("report.json")).unwrap())
            .unwrap();
    assert!(
        report["citations"].as_array().map(Vec::len).unwrap_or(0) >= 1,
        "至少一条可核验引用"
    );
    let rows = std::fs::read_to_string(attempt.join("records.jsonl")).unwrap();
    let v1: Value = serde_json::from_str(rows.lines().next().unwrap()).unwrap();
    assert!(
        v1["usage"]["turns"].as_u64().unwrap() >= 2,
        "真模型轮次>0：{rows}"
    );
    assert!(
        v1["usage"]["unit_calls"].as_u64().unwrap() >= 2,
        "真模型调用>0：{rows}"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// 红队 W1-C33：环级预算熔断——reader + writer 轮共享单一预算账本
///（min(spec 帽, plan cost_cap)），账本耗尽即拒起下一成员；此前每成员
/// 各拿满份帽（图级上限 4×300¢）。
#[test]
fn ring_budget_is_shared_across_members_and_circuit_breaks() {
    let runner = FakeRingRunner::happy();
    let base = std::env::temp_dir().join(format!("g5-ring-w1c33-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(&base).unwrap();
    let plan_src = real_report_plan();
    let task_dir = ring_task_package(&base, &plan_src);
    // 环级帽 = 2¢（spec 帽压到 2；FakeRingRunner reader 耗 2¢）——reader
    // 跑完账本即尽，writer 第 1 轮熔断拒起。
    let mut spec = ReportUnitSpec::v1("glm-cp", ReportJudgeLocator::production(), repo_packages());
    spec.spend_cap_cents = 2;
    let unit = ReportExecuteUnit::new(
        spec.clone(),
        draft_fn_real_ring(runner.clone(), spec),
        Arc::new(LocalFileFetcher),
    )
    .expect("真环单元装载应过三重门");
    let err = drive_err(&unit, &base, &task_dir);
    assert!(
        err.to_string().contains("报告环预算熔断"),
        "环级共享账本耗尽应熔断：{err}"
    );
    // 恰 reader 一个 launch：writer 因熔断门未起。
    assert_eq!(runner.captured().len(), 1, "熔断后不得再起 writer");
    std::fs::remove_dir_all(&base).ok();
}

// ---- 红队 W1-C33（完整化）：plan turns/token 帽强制 + 强制集外条目拒收 ----

/// 改写真计划 plan.body.budget.budgets 后重建任务包（JSON 面 mutate）。
fn ring_task_package_mutated(base: &Path, mutate: impl FnOnce(&mut Value)) -> PathBuf {
    let mut plan: Value = serde_json::from_str(&real_report_plan()).expect("真计划应可解析");
    mutate(&mut plan);
    ring_task_package(base, &plan.to_string())
}

/// plan 级 turns 帽（study_turns，loop:study 作用域）：reader 恰耗 2 turns
/// （FakeRingRunner reader 用量），帽压到 2 → writer 第 1 轮前熔断；且
/// reader 单 launch 的 max_turns 收敛到剩余额度（min(spec 40, 2)）。
#[test]
fn ring_plan_turns_cap_circuit_breaks_before_next_launch() {
    let runner = FakeRingRunner::happy();
    let base = std::env::temp_dir().join(format!("g5-ring-w1c33-turns-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(&base).unwrap();
    let task_dir = ring_task_package_mutated(&base, |plan| {
        for e in plan["body"]["budget"]["budgets"].as_array_mut().unwrap() {
            if e["id"] == "study_turns" {
                e["cap"] = json!(2);
            }
        }
    });
    let mut spec = ReportUnitSpec::v1("glm-cp", ReportJudgeLocator::production(), repo_packages());
    spec.spend_cap_cents = 300; // cents 帽留足，隔离 turns 维
    let unit = ReportExecuteUnit::new(
        spec.clone(),
        draft_fn_real_ring(runner.clone(), spec),
        Arc::new(LocalFileFetcher),
    )
    .expect("真环单元装载应过三重门");
    let err = drive_err(&unit, &base, &task_dir);
    assert!(
        err.to_string().contains("turns"),
        "plan turns 帽应熔断拒起下一成员：{err}"
    );
    assert_eq!(runner.captured().len(), 1, "turns 熔断后不得再起 writer");
    assert_eq!(
        runner.captured()[0].max_turns,
        2,
        "reader 单 launch turns 帽收敛到剩余额度（min(spec 40, plan 2)）"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// plan 级 token_cap（tokens_total，plan 作用域）：reader 耗 1000+200=1200
/// tokens，帽压到 1000 → writer 第 1 轮前熔断（口径 in+out+cache_read，
/// 与图执行器 account_node 同源）。
#[test]
fn ring_plan_token_cap_circuit_breaks_before_next_launch() {
    let runner = FakeRingRunner::happy();
    let base = std::env::temp_dir().join(format!("g5-ring-w1c33-tok-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(&base).unwrap();
    let task_dir = ring_task_package_mutated(&base, |plan| {
        for e in plan["body"]["budget"]["budgets"].as_array_mut().unwrap() {
            if e["id"] == "token_cap" {
                e["cap"] = json!(1000);
            }
        }
    });
    let mut spec = ReportUnitSpec::v1("glm-cp", ReportJudgeLocator::production(), repo_packages());
    spec.spend_cap_cents = 300;
    let unit = ReportExecuteUnit::new(
        spec.clone(),
        draft_fn_real_ring(runner.clone(), spec),
        Arc::new(LocalFileFetcher),
    )
    .expect("真环单元装载应过三重门");
    let err = drive_err(&unit, &base, &task_dir);
    assert!(
        err.to_string().contains("tokens_total"),
        "plan token 帽应熔断拒起下一成员：{err}"
    );
    assert_eq!(runner.captured().len(), 1, "token 熔断后不得再起 writer");
    std::fs::remove_dir_all(&base).ok();
}

/// plan 预算强制集外条目（clock_ms/plan）一律 fail-loud 拒收——与图执行器
/// PlanBudgets::of 同纪律：静默忽略 = 声明帽免帽（W1-C34 同源）。
#[test]
fn ring_plan_budget_unsupported_entry_is_rejected() {
    let runner = FakeRingRunner::happy();
    let base = std::env::temp_dir().join(format!("g5-ring-w1c33-rej-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(&base).unwrap();
    let task_dir = ring_task_package_mutated(&base, |plan| {
        plan["body"]["budget"]["budgets"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "id": "wall_clock", "dimension": "clock_ms", "scope": "plan", "cap": 99999 }));
    });
    let spec = ReportUnitSpec::v1("glm-cp", ReportJudgeLocator::production(), repo_packages());
    let unit = ReportExecuteUnit::new(
        spec.clone(),
        draft_fn_real_ring(runner.clone(), spec),
        Arc::new(LocalFileFetcher),
    )
    .expect("真环单元装载应过三重门");
    let err = drive_err(&unit, &base, &task_dir);
    assert!(
        err.to_string().contains("强制集"),
        "强制集外预算条目应拒收（不得静默免帽）：{err}"
    );
    assert!(
        runner.captured().is_empty(),
        "拒收发生在起环之前，零 launch"
    );
    std::fs::remove_dir_all(&base).ok();
}
