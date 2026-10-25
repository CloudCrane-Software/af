//! ReportExecuteUnit——研究报告包 v1 执行单元（G4 报告包设计 §5；
//! swev_unit 分相骨架同构：setup→run→gate→verify→行落盘，零容器）。
//!
//! 真实现分级（裁决17）：本批 = 装载期三核对（planbind 复用，**不另写第二套
//! digest 核对**——裁决9）+ ingest 快照面 + 判分链（两独立验证器子进程，
//! judge.rs 三原则报告面平行件）+ 行落盘双轨，全部真实可测；**真实模型执行
//! 端到端=铺单批 F-第五单**——起草面以 [`DraftFn`] 缝承载（v1 口径，RESULT
//! 登记），F-第五单换真 harness 环（reader→writer 循环 + ToolRegistry）。
//!
//! 快照落盘归属（裁决5 三说归一）：快照文件由工具副作用落
//! `attempt_dir/sources/`（fetcher 写），图上以 reader 节点 artifact_contract
//! 声明 sources 为工件产出（paper_report 图已带 sources 工件口）——不进
//! store、进图记账；cite_check 以 sources 全集兜底。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use af_workorder::WorkArtifact;
use serde_json::{json, Value};

use crate::agent::AgentRunner;
use crate::error::ExeclineError;
use crate::exec::{ExecContext, ExecOutput, ExecuteUnit};
use crate::pkg::prompts;
use crate::planbind::{self, BindExpectations};
use crate::record::ExecutionFacts;

// ============================================================
// 判分器定位与调用（judge.rs 三原则的报告面平行件，§5.2）
// ============================================================

/// 报告双验证器定位（路径参数化 + 双指纹锚；裁决6：spec 与判分脚本
/// **同 commit 原子生成**，锚=落盘实测——两锚值即脚本本 commit 字节指纹）。
#[derive(Debug, Clone)]
pub struct ReportJudgeLocator {
    /// 判分 python 解释器（stdlib-only；判分固定本机——judge.rs R2 先例）。
    pub python: PathBuf,
    pub citation_script: PathBuf,
    pub citation_sha256: String,
    pub rubric_script: PathBuf,
    pub rubric_sha256: String,
    /// 同包确定性渲染器（report.md 与 rubric R5 的同一正本）。
    pub render_script: PathBuf,
}

impl ReportJudgeLocator {
    /// 现产值：脚本住包内 `af/packages/reportkit/{units,tools}`（随检出走，
    /// `CARGO_MANIFEST_DIR` 相对锚——比 judge.rs 绝对路径更贴检出位置；
    /// F-第五单如需钉绝对路径再演进）；锚=落盘实测（本 commit 原子）。
    pub fn production() -> Self {
        let pkg_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../af/packages/reportkit");
        Self {
            python: PathBuf::from("/usr/bin/python3"),
            citation_script: pkg_root.join("units/citation_unit.py"),
            citation_sha256: "0c34b5956bf4a09aef24368f5f4ab6cf7a1cd97132ce8643c909dacbeb9905dc"
                .into(),
            rubric_script: pkg_root.join("units/rubric_unit.py"),
            rubric_sha256: "af63af2c0d16ff890d081938a77853d6e817f3e0a7b765fc4e5096a3e16d3654"
                .into(),
            render_script: pkg_root.join("tools/render.py"),
        }
    }

    /// 双指纹门（装载期 + verify 入口，双重防线）：任一脚本 sha256 ≠ 声明锚
    /// 即 EX3002 拒跑——不降级、不跳判、不静默换脚本（裁决6 判据 9 的牙齿）。
    pub fn check_fingerprints(&self) -> Result<(), ExeclineError> {
        for (script, anchor) in [
            (&self.citation_script, &self.citation_sha256),
            (&self.rubric_script, &self.rubric_sha256),
        ] {
            let actual = crate::suite::swev::sha256_hex(script).ok_or_else(|| {
                ExeclineError::JudgeFingerprintMismatch {
                    script: script.display().to_string(),
                    expected: anchor.clone(),
                    actual: "脚本不可读（无法验真）".into(),
                }
            })?;
            if &actual != anchor {
                return Err(ExeclineError::JudgeFingerprintMismatch {
                    script: script.display().to_string(),
                    expected: anchor.clone(),
                    actual,
                });
            }
        }
        Ok(())
    }
}

/// citation 验证器子进程调用（argv 契约：恰三位置参数
/// `[task.json, report.json, sources_dir]`；不注入 env、不经 wrapper shell）。
pub fn invoke_citation(
    locator: &ReportJudgeLocator,
    task_json: &Path,
    report_json: &Path,
    sources_dir: &Path,
) -> std::io::Result<std::process::Output> {
    Command::new(&locator.python)
        .arg(&locator.citation_script)
        .arg(task_json)
        .arg(report_json)
        .arg(sources_dir)
        .output()
}

/// rubric 验证器子进程调用（argv：恰两位置参数 `[task.json, report.json]`）。
pub fn invoke_rubric(
    locator: &ReportJudgeLocator,
    task_json: &Path,
    report_json: &Path,
) -> std::io::Result<std::process::Output> {
    Command::new(&locator.python)
        .arg(&locator.rubric_script)
        .arg(task_json)
        .arg(report_json)
        .output()
}

// ============================================================
// 摄取缝（快照契约地基——裁决5「谁写、写哪」的 fetcher 半体）
// ============================================================

/// 摄取产物（快照契约：url/title/text + sha256 + snapshot_ref 相对路径）。
#[derive(Debug, Clone, PartialEq)]
pub struct FetchedDoc {
    pub url: String,
    pub title: String,
    pub text: String,
    pub sha256: String,
    /// 相对 attempt_dir 的快照路径（`sources/<sha 前 12>.txt`）。
    pub snapshot_ref: String,
}

impl FetchedDoc {
    /// 由文本构造（sha256 = 响应体规范化 UTF-8 字节指纹——§2.2 快照契约）。
    pub fn from_text(url: impl Into<String>, title: impl Into<String>, text: &str) -> Self {
        use sha2::{Digest, Sha256};
        let mut sha = String::new();
        for b in Sha256::digest(text.as_bytes()) {
            sha.push_str(&format!("{b:02x}"));
        }
        let snapshot_ref = format!("sources/{}.txt", &sha[..12]);
        Self {
            url: url.into(),
            title: title.into(),
            text: text.to_string(),
            sha256: sha,
            snapshot_ref,
        }
    }

    /// 工具副作用落盘（谁写=fetcher、写哪=`<dir>/sources/`——裁决5）。
    pub fn write_snapshot(&self, attempt_dir: &Path) -> std::io::Result<PathBuf> {
        let path = attempt_dir.join(&self.snapshot_ref);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &self.text)?;
        Ok(path)
    }
}

/// 摄取缝：url/路径 → 文档（web 面的真实现=铺单批 F-第五单——出网/代理/
/// robots 未实测，设计 §10-6 如实边界；本批 LocalFileFetcher 承载本地/缓存面）。
pub trait SourceFetcher {
    fn fetch(&self, source: &str) -> Result<FetchedDoc, String>;
}

/// 本地文件摄取（`file://` 或裸路径；文本文件——PDF 抽取=能力缺口，回退
/// HTML/文本面，设计 §9-4/§10-7 如实登记）。
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFetcher;

impl SourceFetcher for LocalFileFetcher {
    fn fetch(&self, source: &str) -> Result<FetchedDoc, String> {
        if source.starts_with("http://") || source.starts_with("https://") {
            return Err(format!(
                "web 摄取面（{source}）= 铺单批 F-第五单（出网/代理未实测，设计 §10-6）；v1 用本地路径/file:// 面"
            ));
        }
        let path = source.strip_prefix("file://").unwrap_or(source);
        let text = std::fs::read_to_string(path).map_err(|err| format!("读 {path} 失败: {err}"))?;
        let title = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.into());
        Ok(FetchedDoc::from_text(
            format!("file://{path}"),
            title,
            &text,
        ))
    }
}

// ============================================================
// 起草缝（v1 口径：真实模型环=铺单批 F-第五单——裁决17）
// ============================================================

/// 起草输入（reader/writer 模板正文 + 论文文本头 + 研读焦点）。
#[derive(Debug, Clone, PartialEq)]
pub struct DraftInput {
    pub focus: String,
    pub paper_text_head: String,
    pub min_citations: usize,
    pub reader_prompt: String,
    pub writer_prompt: String,
}

/// 起草缝：输入 → research_report JSON 文本（Err = 起草失败文案）。
/// v1 由测试/驱动层注入；F-第五单以真 harness 环（reader→writer 循环 +
/// ToolRegistry 注册 web_search/web_fetch——agent.rs registry.register 面）
/// 换入同型闭包，单元其余分相零改动。
pub type DraftFn = Arc<dyn Fn(&DraftInput) -> Result<String, String> + Send + Sync>;

// ============================================================
// 单元静态声明（§5.1；档位差异化兑现点——裁决11 单元 spec 覆写正本机制）
// ============================================================

/// 报告任务包（`<data_root>/<task>/task.json`——§6.1）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ReportTask {
    #[serde(default)]
    pub paper_url: Option<String>,
    #[serde(default)]
    pub local_path: Option<String>,
    pub focus: String,
    #[serde(default)]
    pub paper_sha256: Option<String>,
    #[serde(default)]
    pub rubric_params: Value,
}

/// 报告单元静态声明（SwevUnitSpec 同构：「换一档实验要改代码的东西」；
/// 裁决6：与两判分脚本同 commit 原子生成）。
#[derive(Debug, Clone)]
pub struct ReportUnitSpec {
    pub profile: String,
    pub temperature_milli: u32,
    pub top_p_milli: u32,
    pub max_tokens: u32,
    /// reader/writer 提示词模板引用（裁决10 统一机制，装载期经 pkg::prompts）。
    pub reader_template_ref: String,
    pub writer_template_ref: String,
    /// mem.notes 装配参数（latest_k 窗口——v0 memory 组件=装配策略挂载位）。
    pub notes_k: usize,
    pub max_turns: u32,
    pub completion_signal: String,
    pub tool_free_turn_limit: u32,
    pub spend_cap_cents: i64,
    pub tool_spend_cents: i64,
    pub scaffold: &'static str,
    pub reasoning_tier: &'static str,
    pub judges: ReportJudgeLocator,
    /// 组件包根（planbind E9004/模板面核对用）。
    pub registry_root: PathBuf,
    pub unit_version: &'static str,
}

impl ReportUnitSpec {
    /// v1 缺省（采样/预算先按报告包 §4.2 图面口径：turns 40/cents 300）。
    pub fn v1(
        profile: impl Into<String>,
        judges: ReportJudgeLocator,
        registry_root: PathBuf,
    ) -> Self {
        Self {
            profile: profile.into(),
            temperature_milli: 0,
            top_p_milli: 1_000,
            max_tokens: 32_000,
            reader_template_ref: "pkg.reportkit.reader_system".into(),
            writer_template_ref: "pkg.reportkit.writer_system".into(),
            notes_k: 12,
            max_turns: 40,
            completion_signal: "REPORT_DONE".into(),
            tool_free_turn_limit: 3,
            spend_cap_cents: 300,
            tool_spend_cents: 1,
            scaffold: "report-v1",
            reasoning_tier: "off",
            judges,
            registry_root,
            unit_version: "0.1.0",
        }
    }
}

// ============================================================
// 执行单元
// ============================================================

/// 研究报告执行单元：setup（三核对）→ ingest（快照面）→ draft（缝）→
/// gate（记账）→ verify（双判分子进程）→ 行落盘双轨；零容器、无 cleanup 相。
pub struct ReportExecuteUnit {
    spec: ReportUnitSpec,
    draft: DraftFn,
    fetcher: Arc<dyn SourceFetcher + Send + Sync>,
    #[allow(dead_code)]
    runner: Arc<dyn AgentRunner>,
}

impl std::fmt::Debug for ReportExecuteUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReportExecuteUnit")
            .field("unit_id", &self.unit_id())
            .field("profile", &self.spec.profile)
            .finish_non_exhaustive()
    }
}

impl ReportExecuteUnit {
    /// 装载期三重门（fail-loud，拒在起判分/起草之前）：判分双指纹锚 +
    /// profile 可解析 + 模板引用可装载（裁决10 面）。
    pub fn new(
        spec: ReportUnitSpec,
        draft: DraftFn,
        fetcher: Arc<dyn SourceFetcher + Send + Sync>,
    ) -> Result<Self, ExeclineError> {
        spec.judges.check_fingerprints()?;
        crate::profile::profile(&spec.profile).map_err(|err| ExeclineError::UnitFailed {
            detail: format!("装载失败：{err}"),
        })?;
        prompts::resolve_template(&spec.registry_root, &spec.reader_template_ref)?;
        prompts::resolve_template(&spec.registry_root, &spec.writer_template_ref)?;
        Ok(Self {
            spec,
            draft,
            fetcher,
            runner: Arc::new(crate::agent::HarnessRunner),
        })
    }

    fn fail(&self, detail: impl Into<String>) -> ExeclineError {
        ExeclineError::UnitFailed {
            detail: detail.into(),
        }
    }
}

impl ExecuteUnit for ReportExecuteUnit {
    fn unit_id(&self) -> &str {
        "report-unit"
    }

    fn execute(&self, ctx: &ExecContext<'_>) -> Result<ExecOutput, ExeclineError> {
        let started = std::time::Instant::now();
        // ---- setup 相：任务包装载 + 计划装订三核对（planbind 唯一正本）----
        let task_dir = ctx.data_root.join(ctx.task_id);
        let task_json = task_dir.join("task.json");
        let task: ReportTask =
            serde_json::from_str(&std::fs::read_to_string(&task_json).map_err(|err| {
                self.fail(format!("读任务包失败 {}: {err}", task_json.display()))
            })?)
            .map_err(|err| self.fail(format!("任务包不是合法 JSON: {err}")))?;
        let plan_src = std::fs::read_to_string(task_dir.join("plan.json")).map_err(|err| {
            self.fail(format!(
                "读计划失败（v1 约定：计划随任务包 plan.json）: {err}"
            ))
        })?;
        let bound = planbind::bind(
            &plan_src,
            &self.spec.registry_root,
            &[
                &self.spec.reader_template_ref,
                &self.spec.writer_template_ref,
            ],
            &BindExpectations::default(),
        )?;
        let min_citations = task
            .rubric_params
            .get("min_citations")
            .and_then(Value::as_u64)
            .unwrap_or(3) as usize;

        // ---- ingest 相：工具副作用落快照（谁写=fetcher、写哪=sources/）----
        let attempt_dir = ctx.attempt_dir;
        std::fs::create_dir_all(attempt_dir)
            .map_err(|err| self.fail(format!("建 run 目录失败: {err}")))?;
        let source = task
            .local_path
            .clone()
            .or_else(|| task.paper_url.clone())
            .ok_or_else(|| self.fail("任务包缺 local_path/paper_url 摄取源"))?;
        let doc = self
            .fetcher
            .fetch(&source)
            .map_err(|err| self.fail(format!("摄取失败: {err}")))?;
        doc.write_snapshot(attempt_dir)
            .map_err(|err| self.fail(format!("快照落盘失败: {err}")))?;
        std::fs::write(
            attempt_dir.join("paper.json"),
            json!({"url": doc.url, "title": doc.title, "sha256": doc.sha256, "snapshot_ref": doc.snapshot_ref}).to_string(),
        )
        .map_err(|err| self.fail(format!("登记摄取面失败: {err}")))?;

        // ---- run 相（v1 缝）：起草 ----
        let draft_input = DraftInput {
            focus: task.focus.clone(),
            paper_text_head: doc.text.chars().take(4000).collect(),
            min_citations,
            reader_prompt: bound.templates[&self.spec.reader_template_ref]
                .content
                .clone(),
            writer_prompt: bound.templates[&self.spec.writer_template_ref]
                .content
                .clone(),
        };
        let draft_text =
            (self.draft)(&draft_input).map_err(|err| self.fail(format!("起草失败: {err}")))?;
        let report: Value = serde_json::from_str(draft_text.trim())
            .map_err(|err| self.fail(format!("起草产物不是合法 report JSON: {err}")))?;
        std::fs::write(attempt_dir.join("report.json"), report.to_string())
            .map_err(|err| self.fail(format!("写 report.json 失败: {err}")))?;

        // report.md：与 rubric R5 同一渲染器（同构性由同一脚本保证）。
        let render_out = Command::new(&self.spec.judges.python)
            .arg(&self.spec.judges.render_script)
            .arg(attempt_dir.join("report.json"))
            .output()
            .map_err(|err| self.fail(format!("渲染器拉起失败: {err}")))?;
        if !render_out.status.success() {
            return Err(self.fail(format!(
                "渲染 report.md 失败: {}",
                String::from_utf8_lossy(&render_out.stderr)
            )));
        }
        std::fs::write(attempt_dir.join("report.md"), &render_out.stdout)
            .map_err(|err| self.fail(format!("写 report.md 失败: {err}")))?;

        // ---- gate 相（记账非门控）：快照数/引用数 ----
        let citations = report["citations"].as_array().map(Vec::len).unwrap_or(0);
        let snapshots = std::fs::read_dir(attempt_dir.join("sources"))
            .map(|d| d.filter_map(|e| e.ok()).count())
            .unwrap_or(0);

        // ---- verify 相：双验证器子进程（判定只取 stdout JSON）----
        self.spec.judges.check_fingerprints()?; // verify 入口第二道门
        let cite_out = invoke_citation(
            &self.spec.judges,
            &task_json,
            &attempt_dir.join("report.json"),
            &attempt_dir.join("sources"),
        )
        .map_err(|err| self.fail(format!("citation 验证器拉起失败: {err}")))?;
        if !cite_out.status.success() {
            return Err(self.fail(format!(
                "citation 验证器退出 {}: {}",
                cite_out.status,
                String::from_utf8_lossy(&cite_out.stderr)
            )));
        }
        let cite_verdict: Value = serde_json::from_slice(&cite_out.stdout)
            .map_err(|err| self.fail(format!("citation 判定不是 JSON: {err}")))?;
        let rubric_out = invoke_rubric(
            &self.spec.judges,
            &task_json,
            &attempt_dir.join("report.json"),
        )
        .map_err(|err| self.fail(format!("rubric 验证器拉起失败: {err}")))?;
        if !rubric_out.status.success() {
            return Err(self.fail(format!(
                "rubric 验证器退出 {}: {}",
                rubric_out.status,
                String::from_utf8_lossy(&rubric_out.stderr)
            )));
        }
        let rubric_verdict: Value = serde_json::from_slice(&rubric_out.stdout)
            .map_err(|err| self.fail(format!("rubric 判定不是 JSON: {err}")))?;
        let verify_log = format!(
            "# citation\n{}\n# rubric\n{}\n",
            cite_verdict, rubric_verdict
        );
        std::fs::write(attempt_dir.join("verify.log"), &verify_log)
            .map_err(|err| self.fail(format!("写 verify.log 失败: {err}")))?;

        let citation_passed = cite_verdict["passed"].as_bool().unwrap_or(false);
        let rubric_passed = rubric_verdict["passed"].as_bool().unwrap_or(false);
        let verdict = citation_passed && rubric_passed;
        let note = format!(
            "citation passed={citation_passed}（{} failures）；rubric passed={rubric_passed}（{} failures）；snapshots={snapshots} citations={citations}",
            cite_verdict["failures"].as_array().map(Vec::len).unwrap_or(0),
            rubric_verdict["failures"].as_array().map(Vec::len).unwrap_or(0),
        );

        // ---- 行落盘相：双轨同源（record.rs 复用，suite=report）----
        let facts = ExecutionFacts {
            suite: "report",
            task_id: ctx.task_id.to_string(),
            profile: self.spec.profile.clone(),
            trial: ctx.trial,
            reward: Some(verdict),
            status: "completed".into(),
            scaffold: self.spec.scaffold,
            reasoning_tier: self.spec.reasoning_tier,
            run_dir: Some(attempt_dir.display().to_string()),
            eval_sha256: None,
            gate: if verdict { "ok" } else { "verify_failed" }.into(),
            attempts: 1,
            http_retries: 0,
            turns: 0,
            model_calls: 0,
            tokens: Default::default(),
            agent_wall_ms: started.elapsed().as_millis() as u64,
            verifier_note: note.clone(),
        };
        let legacy_row = crate::record::swev_legacy_row(&facts);
        let v1_row = crate::record::swev_v1_row(
            &facts,
            bound.plan.body.plan_id.as_str(),
            ctx.line_id,
            ctx.host_alias,
            self.unit_id(),
            self.spec.unit_version,
        );
        crate::record::append_legacy_row(ctx, &legacy_row)?;
        ctx.record_sink
            .append_v1(&v1_row.to_string())
            .map_err(|err| self.fail(format!("写 records 失败: {err}")))?;

        let evidence = vec![
            attempt_dir.join("report.json"),
            attempt_dir.join("report.md"),
            attempt_dir.join("verify.log"),
        ];
        Ok(ExecOutput {
            artifacts: vec![WorkArtifact {
                artifact_id: format!("report-{}", ctx.task_id),
                is_final: true,
                qualified: verdict, // 工件资格只看判分（双 passed）
            }],
            outputs: evidence.clone(),
            note: note.chars().take(300).collect(),
            verdict: Some(verdict),
            record: Some(v1_row),
            evidence,
        })
    }
}
