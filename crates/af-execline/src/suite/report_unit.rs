//! ReportExecuteUnit——研究报告包 v1 执行单元（G4 报告包设计 §5；
//! swev_unit 分相骨架同构：setup→run→gate→verify→行落盘，零容器）。
//!
//! 真实现分级（裁决17）：G4 批 = 装载期三核对（planbind 复用，**不另写第二套
//! digest 核对**——裁决9）+ ingest 快照面 + 判分链（两独立验证器子进程，
//! judge.rs 三原则报告面平行件）+ 行落盘双轨；**真实模型执行端到端 = F-第五单
//! （本批已接线）**：[`draft_fn_real_ring`]——reader→writer 双节点经
//! AgentRunner 真调用（uses.model 档位 + llm.cents 预算记账，swev 同款），
//! 节点间工件按 paper_report 计划 types 表走 envelope 校验，cite_check
//! 判分失败回喂 writer（study 环）。测试缝（DraftFn 假实现）照留。
//!
//! 快照落盘归属（裁决5 三说归一）：快照文件由工具副作用落
//! `attempt_dir/sources/`（fetcher 写），图上以 reader 节点 artifact_contract
//! 声明 sources 为工件产出（paper_report 图已带 sources 工件口）——不进
//! store、进图记账；cite_check 以 sources 全集兜底。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use af_compiler::ir::BudgetDimension;
use af_compiler::ExecutablePlan;
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
    /// 渲染器 sha256 锚（红队 W1-C61：rubric 的 R5 判据 = report.md 与
    /// `render.render_markdown` 输出一致——渲染器既是生产者又是裁判基准，
    /// 此前双指纹门只盖 citation/rubric 两脚本，改 render.py 一个函数即可
    /// 翻转 R5 而两件钉死脚本字节不动；第三件入指纹门）。
    pub render_sha256: String,
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
            render_sha256: "255f68f7795706637749a9559fbb85444afcd92687b2398012dd94b077b4db76"
                .into(),
        }
    }

    /// 双指纹门（装载期 + verify 入口，双重防线）：任一脚本 sha256 ≠ 声明锚
    /// 即 EX3002 拒跑——不降级、不跳判、不静默换脚本（裁决6 判据 9 的牙齿）。
    pub fn check_fingerprints(&self) -> Result<(), ExeclineError> {
        for (script, anchor) in [
            (&self.citation_script, &self.citation_sha256),
            (&self.rubric_script, &self.rubric_sha256),
            // 红队 W1-C61：渲染器入指纹门（R5 判据基准的第三件）。
            (&self.render_script, &self.render_sha256),
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
        // 红队 W2-C52：判分目录字节码在场性门——rubric_unit.py 经
        // `import render` 动态加载渲染器，Python 对在场且头部 mtime/size
        // 匹配的 `__pycache__/render.*.pyc` 会**优先于源文件**加载（-B 只禁
        // 写不禁读）：三个被 sha256 锚住的 .py 之外若存在任何字节码，即是
        // 独立于源码锚的影子执行面——不改一个锚定字节即可翻转 R5 判据
        // （投毒复现：pyc 的 render_markdown 返回攻击者串 ⇒ R5 由 false 翻
        // true）。判分子进程 -B 只防再生，读取侧由此门兜底：判分目录
        // （三脚本所在目录）出现 __pycache__/*.pyc 一律 fail-loud。
        let mut judge_dirs: Vec<PathBuf> = Vec::new();
        for script in [
            &self.citation_script,
            &self.rubric_script,
            &self.render_script,
        ] {
            if let Some(parent) = script.parent() {
                if !judge_dirs.iter().any(|d| d == parent) {
                    judge_dirs.push(parent.to_path_buf());
                }
            }
        }
        for dir in judge_dirs {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name == "__pycache__" || name.ends_with(".pyc") {
                    return Err(ExeclineError::JudgeFingerprintMismatch {
                        script: entry.path().display().to_string(),
                        expected:
                            "判分目录不得存在字节码（__pycache__/*.pyc——影子执行面，禁写+禁读）"
                                .into(),
                        actual: "在场".into(),
                    });
                }
            }
        }
        Ok(())
    }
}

/// citation 验证器子进程调用（argv 契约：恰三位置参数
/// `[task.json, report.json, sources_dir]`；不注入 env、不经 wrapper shell）。
/// 判分/渲染子进程统一构造（红队 W2-C52）：`-B` +
/// `PYTHONDONTWRITEBYTECODE=1`——禁写 `__pycache__` 字节码。rubric_unit.py
/// 以 `import render` 动态加载渲染器（R5 判据基准），Python 对**已在场**的
/// 有效 .pyc 仍会优先加载（-B 只禁写不禁读），故读取侧由
/// [`ReportExecuteUnit::check_fingerprints`] 的 pyc 在场性门兜底。
fn judge_python(locator: &ReportJudgeLocator) -> Command {
    let mut cmd = Command::new(&locator.python);
    cmd.arg("-B");
    cmd.env("PYTHONDONTWRITEBYTECODE", "1");
    cmd
}

pub fn invoke_citation(
    locator: &ReportJudgeLocator,
    task_json: &Path,
    report_json: &Path,
    sources_dir: &Path,
) -> std::io::Result<std::process::Output> {
    judge_python(locator)
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
    judge_python(locator)
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

/// 摄取缝：url/路径 → 文档（快照契约地基，裁决5「谁写、写哪」的 fetcher 半体）。
pub trait SourceFetcher {
    fn fetch(&self, source: &str) -> Result<FetchedDoc, String>;
}

/// 本地文件摄取（`file://` 或裸路径）：文本文件直读；`.pdf` 走 pdftotext
/// 抽取（设计 §2.3 pdf_extract——同快照契约两路同构；抽取器=本机 poppler
/// 子进程，判分子进程同款纪律，缺失即 fail-loud 点名）。
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFetcher;

impl LocalFileFetcher {
    /// PDF 抽取核心（pdftotext -q <file> -）：文本规范化（去尾空白）后走
    /// FetchedDoc 快照契约（sha256=抽取文本指纹）。
    fn pdf_to_text(path: &Path) -> Result<String, String> {
        let out = Command::new("pdftotext")
            .arg("-q")
            .arg(path)
            .arg("-")
            .output()
            .map_err(|err| format!("拉起 pdftotext 失败（本机需 poppler）: {err}"))?;
        if !out.status.success() {
            return Err(format!(
                "pdftotext 退出 {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    }
}

impl SourceFetcher for LocalFileFetcher {
    fn fetch(&self, source: &str) -> Result<FetchedDoc, String> {
        if source.starts_with("http://") || source.starts_with("https://") {
            return Err(format!(
                "LocalFileFetcher 只承载本地/file:// 面（{source}）；web 摄取走 ArxivWebFetcher"
            ));
        }
        let path = source.strip_prefix("file://").unwrap_or(source);
        let text = if path.to_lowercase().ends_with(".pdf") {
            Self::pdf_to_text(Path::new(path))?
        } else {
            std::fs::read_to_string(path).map_err(|err| format!("读 {path} 失败: {err}"))?
        };
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
// web 摄取面（F-第五单）：arXiv 域白名单 + curl 子进程 + HTML 剥除
// ============================================================

/// arXiv 域白名单（设计 §2.2：v1 限 arxiv.org/www.arxiv.org/export.arxiv.org）。
const ARXIV_HOSTS: [&str; 3] = ["arxiv.org", "www.arxiv.org", "export.arxiv.org"];

/// 出网探针/抓取超时（秒）——abs 页实测 ~2.6s，给足余量但不无限等。
const WEB_FETCH_TIMEOUT_SECS: u64 = 45;

/// HTML → 纯文本（零依赖剥除：script/style 块整段丢弃，标签换空格，五个
/// 常见实体解码，空白折叠）。citation_unit 的 quote_hit 本就规范化空白比对，
/// 此处的折叠不破坏引用核验。
pub fn html_to_text(html: &str) -> String {
    // ASCII 小写镜像（字节布局与原文逐位对齐——to_lowercase 对非 ASCII 会
    // 变长移位，不可用于索引）。
    let lower: String = html.chars().map(|c| c.to_ascii_lowercase()).collect();
    // ① script/style 块整段丢弃（取先出现者，未闭合则尾部全丢）。
    let mut cleaned = String::with_capacity(html.len());
    let mut pos = 0usize;
    while pos < html.len() {
        let script = lower[pos..].find("<script").map(|i| pos + i);
        let style = lower[pos..].find("<style").map(|i| pos + i);
        match script.into_iter().chain(style).min() {
            Some(start) => {
                cleaned.push_str(&html[pos..start]);
                let end_tag = if lower[start..].starts_with("<script") {
                    "</script>"
                } else {
                    "</style>"
                };
                match lower[start..].find(end_tag) {
                    Some(end) => pos = start + end + end_tag.len(),
                    None => pos = html.len(),
                }
            }
            None => {
                cleaned.push_str(&html[pos..]);
                pos = html.len();
            }
        }
    }
    // ② 标签 → 空格。
    let mut no_tags = String::with_capacity(cleaned.len());
    let mut in_tag = false;
    for ch in cleaned.chars() {
        match ch {
            '<' => {
                in_tag = true;
                no_tags.push(' ');
            }
            '>' => {
                in_tag = false;
                no_tags.push(' ');
            }
            _ if !in_tag => no_tags.push(ch),
            _ => {}
        }
    }
    // ③ 实体解码（五个高频件）。
    let decoded = no_tags
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    // ④ 空白折叠（quote_hit 规范化空白比对，此处折叠不破坏核验）。
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// arXiv web 摄取（F-第五单真实出网面；anolis 出网预检 2026-10-25：
/// export.arxiv.org abs/api HTTPS 200 直连可用、PDF 200 但慢）。
///
/// 域白名单外一律拒（不静默放行、不经代理绕行）；抓取用 curl 子进程
/// （判分子进程同款纪律——argv 直传不经 shell，无注入面）。host 统一改写
/// export.arxiv.org（arXiv 官方导出面），路径原样保留。
#[derive(Debug, Clone, Copy, Default)]
pub struct ArxivWebFetcher;

impl ArxivWebFetcher {
    /// 出网前域名核对（https + 白名单）。
    fn normalize_url(source: &str) -> Result<String, String> {
        let rest = source
            .strip_prefix("https://")
            .ok_or_else(|| format!("web 摄取仅支持 https（{source}）"))?;
        let (host, path) = match rest.find('/') {
            Some(pos) => (&rest[..pos], &rest[pos..]),
            None => (rest, "/"),
        };
        if !ARXIV_HOSTS.contains(&host) {
            return Err(format!(
                "web 摄取域白名单（设计 §2.2：v1 限 arxiv.org 域）拒绝 {host}——{source}"
            ));
        }
        Ok(format!("https://export.arxiv.org{path}"))
    }

    /// 抓取核心：白名单核对 → curl → title/html_to_text → FetchedDoc。
    pub fn fetch_to_doc(source: &str) -> Result<FetchedDoc, String> {
        let url = Self::normalize_url(source)?;
        let out = Command::new("curl")
            .arg("-sSL")
            .arg("--max-redirs")
            .arg("5")
            .arg("-m")
            .arg(WEB_FETCH_TIMEOUT_SECS.to_string())
            .arg(&url)
            .output()
            .map_err(|err| format!("拉起 curl 失败（web 摄取需本机 curl）: {err}"))?;
        if !out.status.success() {
            return Err(format!(
                "curl 退出 {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let body = String::from_utf8_lossy(&out.stdout).into_owned();
        if body.trim().is_empty() {
            return Err(format!("抓取 {url} 空响应体"));
        }
        let title = extract_html_title(&body).unwrap_or_else(|| url.clone());
        Ok(FetchedDoc::from_text(source, title, &html_to_text(&body)))
    }
}

impl SourceFetcher for ArxivWebFetcher {
    fn fetch(&self, source: &str) -> Result<FetchedDoc, String> {
        Self::fetch_to_doc(source)
    }
}

/// `<title>…</title>` 抽取（大小写不敏感；剥实体）。
fn extract_html_title(html: &str) -> Option<String> {
    let lower: String = html.chars().map(|c| c.to_ascii_lowercase()).collect();
    let start = lower.find("<title")?;
    let content_start = start + html[start..].find('>')? + 1;
    let end = lower[content_start..].find("</title>")? + content_start;
    Some(
        html_to_text(&html[content_start..end])
            .chars()
            .take(200)
            .collect::<String>(),
    )
}

/// 摄取分派器（任务包 source 形状选路）：https → arXiv web 面；其余 →
/// 本地面（文本/PDF）。CLI/驱动层统一注入件。
#[derive(Debug, Clone, Copy, Default)]
pub struct IngestFetcher;

impl SourceFetcher for IngestFetcher {
    fn fetch(&self, source: &str) -> Result<FetchedDoc, String> {
        if source.starts_with("https://") {
            ArxivWebFetcher.fetch(source)
        } else if source.starts_with("http://") {
            Err("web 摄取仅支持 https（明文 http 拒绝）".to_string())
        } else {
            LocalFileFetcher.fetch(source)
        }
    }
}

// ============================================================
// 起草环（F-第五单：真实模型环——reader→writer 经 AgentRunner 真调用）
// ============================================================

/// ingest 产物信封（plan 类型 `paper_doc` 的执行面镜像：url/title/sha256 +
/// 快照相对路径——writer 的 paper 字段必须逐字取自此）。
#[derive(Debug, Clone, PartialEq)]
pub struct PaperEnvelope {
    pub url: String,
    pub title: String,
    pub sha256: String,
    pub snapshot_ref: String,
}

impl PaperEnvelope {
    pub fn from_doc(doc: &FetchedDoc) -> Self {
        Self {
            url: doc.url.clone(),
            title: doc.title.clone(),
            sha256: doc.sha256.clone(),
            snapshot_ref: doc.snapshot_ref.clone(),
        }
    }
}

/// 环面计划类型（从装订计划的 types 表取正本——envelope 校验按真计划驱动，
/// 不内置第二套 schema）。
#[derive(Debug, Clone, PartialEq)]
pub struct RingTypes {
    /// reader→writer 边的工件类型（`list<note_item>` 之元素）。
    pub note_item: Value,
    /// writer 产出工件类型（research_report）。
    pub research_report: Value,
}

impl RingTypes {
    /// 从计划 types 表提取（IR 形状：name → schema 直连，lower.rs:103）；
    /// 缺任一 → Err（测试直测面：真计划必带）。
    pub fn from_plan_types(
        types: &std::collections::BTreeMap<String, Value>,
    ) -> Result<Self, String> {
        let note_item = types
            .get("note_item")
            .cloned()
            .ok_or_else(|| "计划 types 缺 note_item（reader→writer 边工件类型）".to_string())?;
        let research_report = types
            .get("research_report")
            .cloned()
            .ok_or_else(|| "计划 types 缺 research_report（writer 产出工件类型）".to_string())?;
        Ok(Self {
            note_item,
            research_report,
        })
    }

    /// 缺类型计划的宽容档（Null schema = 无约束）：DraftFn 缝假实现不消费
    /// envelope，不受影响；**真环自检见 Null 即拒跑**（不静默免检）。
    pub fn permissive() -> Self {
        Self {
            note_item: Value::Null,
            research_report: Value::Null,
        }
    }

    /// 真环装载自检：计划未携带环面类型 → 拒跑（envelope 校验不可静默省略）。
    fn require_real(&self) -> Result<(), String> {
        if self.note_item.is_null() || self.research_report.is_null() {
            return Err(
                "真环需要计划 types 携 note_item/research_report（paper_report 图）——\
                 envelope 校验不可省略，拒跑"
                    .to_string(),
            );
        }
        Ok(())
    }
}

/// envelope 校验（JSON Schema 子集：type/required/properties/items——计划
/// types 表实际使用的约束集；payload 违反即 Err 点名路径与字段）。
pub fn validate_envelope(schema: &Value, payload: &Value) -> Result<(), String> {
    validate_node(schema, payload, "$")
}

fn validate_node(schema: &Value, payload: &Value, path: &str) -> Result<(), String> {
    let Some(obj) = schema.as_object() else {
        return Ok(()); // 非 schema 节点（无约束）
    };
    if let Some(want) = obj.get("type").and_then(Value::as_str) {
        let ok = match want {
            "object" => payload.is_object(),
            "array" => payload.is_array(),
            "string" => payload.is_string(),
            "integer" => payload.is_i64() || payload.is_u64(),
            "number" => payload.is_number(),
            "boolean" => payload.is_boolean(),
            _ => true, // 未知类型词：不判（子集语义）
        };
        if !ok {
            return Err(format!("envelope {path}: 类型应为 {want}，实为 {payload}"));
        }
    }
    if let (Some(required), Some(map)) = (
        obj.get("required").and_then(Value::as_array),
        payload.as_object(),
    ) {
        for key in required {
            if let Some(name) = key.as_str() {
                if !map.contains_key(name) {
                    return Err(format!("envelope {path}: 缺 required 字段 {name}"));
                }
            }
        }
    }
    if let (Some(props), Some(map)) = (obj.get("properties"), payload.as_object()) {
        for (name, sub) in props.as_object().into_iter().flatten() {
            if let Some(value) = map.get(name) {
                validate_node(sub, value, &format!("{path}.{name}"))?;
            }
        }
    }
    if let (Some(items_schema), Some(list)) = (obj.get("items"), payload.as_array()) {
        for (index, item) in list.iter().enumerate() {
            validate_node(items_schema, item, &format!("{path}[{index}]"))?;
        }
    }
    Ok(())
}

/// 环级预算帽（红队 W1-C33 完整化）：plan.body.budget 的**白名单提取**——
/// 报告环即 paper_report 的 study 环，强制集 =
/// plan 级 `cost_cap`（cents）/`token_cap`（tokens_total）+ turns
/// （scope `plan` 或 `loop:study`——环面无节点投影，node: 作用域不在集内）。
/// **强制集外条目一律 fail-loud 拒收**（与图执行器 `PlanBudgets::of` 同纪律，
/// W1-C34 同源：静默忽略 = 声明帽免帽）。canonical id 门对 cents/tokens_total
/// 生效（只认 cost_cap/token_cap）；turns 按 scope+dimension 强制（改名不改
/// 作用域/维度即仍被强制，无免帽面，故不设 id 门）。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RingBudgets {
    /// plan 级 cents 帽（id 必须 = cost_cap）。
    pub cents: Option<i64>,
    /// turns 帽（scope plan 或 loop:study；环级累计口径）。
    pub turns: Option<u32>,
    /// plan 级 tokens_total 帽（id 必须 = token_cap；口径 in+out+cache_read，
    /// 与图执行器 account_node 同源）。
    pub tokens: Option<u64>,
}

impl RingBudgets {
    /// 白名单提取 + 强制集外拒收（fail-loud，装订期）。
    pub fn of(plan: &ExecutablePlan) -> Result<Self, ExeclineError> {
        let mut b = Self::default();
        for entry in &plan.body.budget.budgets {
            match (entry.scope.as_str(), entry.dimension) {
                ("plan", BudgetDimension::Cents) => {
                    if entry.id != "cost_cap" {
                        return Err(ExeclineError::PlanBindMismatch {
                            detail: format!(
                                "plan 级 cents 预算 id {} 非正典 cost_cap（执行面只认 \
                                 cost_cap——改名即静默免帽，拒收）",
                                entry.id
                            ),
                        });
                    }
                    b.cents = Some(entry.cap as i64);
                }
                ("plan", BudgetDimension::TokensTotal) => {
                    if entry.id != "token_cap" {
                        return Err(ExeclineError::PlanBindMismatch {
                            detail: format!(
                                "plan 级 tokens_total 预算 id {} 非正典 token_cap（执行面\
                                 只认 token_cap——改名即静默免帽，拒收）",
                                entry.id
                            ),
                        });
                    }
                    b.tokens = Some(entry.cap);
                }
                // turns：环 = study 环（paper_report loop:study 作用域正本）；
                // plan 级 turns 同样作用于整环。取小防多条目互相松绑。
                (scope, BudgetDimension::Turns) if scope == "plan" || scope == "loop:study" => {
                    let cap = entry.cap.min(u32::MAX as u64) as u32;
                    b.turns = Some(b.turns.map_or(cap, |prev: u32| prev.min(cap)));
                }
                (scope, dim) => {
                    return Err(ExeclineError::PlanBindMismatch {
                        detail: format!(
                            "预算条目 id {}（dimension={:?}, scope={:?}）不在报告环执行面强制集内\
                             （强制集：plan 级 cost_cap(cents)/token_cap(tokens_total) + turns\
                             （plan/loop:study）；其余组合声明即须强制，静默忽略=免帽，拒收）",
                            entry.id, dim, scope
                        ),
                    });
                }
            }
        }
        Ok(b)
    }
}

/// 起草输入（reader/writer 模板正文 + 论文文本头 + 研读焦点 + 环上下文）。
#[derive(Debug, Clone, PartialEq)]
pub struct DraftInput {
    pub focus: String,
    pub paper_text_head: String,
    pub min_citations: usize,
    pub reader_prompt: String,
    pub writer_prompt: String,
    /// ingest 产物信封（paper_doc envelope——writer 的 paper 字段正本）。
    pub paper: PaperEnvelope,
    /// run 目录（sources/、ring/ 子目录）。
    pub attempt_dir: PathBuf,
    /// cite_check argv[0]（任务包正本）。
    pub task_json: PathBuf,
    /// 计划 types 表提取的环面类型（envelope 校验正本）。
    pub plan_types: RingTypes,
    /// 环级预算帽（红队 W1-C33：plan.body.budget 白名单提取——reader + 全部
    /// writer 轮**共享单一账本**：cents（min(spec 帽, plan cost_cap)）、turns
    /// （plan/loop:study）、tokens_total（plan token_cap）。此前每成员各拿
    /// 整份 spend_cap_cents + 全新账本，图级上限 = 4×300¢，与 spec 自述
    /// 「图面口径 cents 300」不符；plan.body.budget 此前零消费，turns/token
    /// 帽更被整体静默忽略。Default = 无 plan 级帽（仅 spec 帽生效）。
    pub ring_caps: RingBudgets,
}

/// 起草产出：报告 JSON + 环用量（reader/writer 双节点聚合——进行落盘 usage）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DraftOutput {
    pub report_json: String,
    pub turns: u32,
    pub model_calls: u64,
    pub tokens: af_harness::TokenTotals,
    pub http_retries: u64,
    /// study 环实际跑的 writer 轮数（plan max_iterations 3 内）。
    pub iterations: u32,
}

impl DraftOutput {
    /// 缝测试便捷件（报告正文 + 零用量——v1 前测试缝同语义）。
    pub fn from_report_json(report_json: impl Into<String>) -> Self {
        Self {
            report_json: report_json.into(),
            ..Default::default()
        }
    }
}

/// 起草缝：输入 → 报告 JSON + 环用量（Err = 起草失败文案）。
/// 测试注入 Fake 环闭包；生产 = [`draft_fn_real_ring`]（真 harness 环：
/// reader→writer 循环 + ToolRegistry 注册 web_fetch/submit_*，agent.rs
/// registry.register 面）——单元其余分相零改动（G4 缝契约）。
pub type DraftFn = Arc<dyn Fn(&DraftInput) -> Result<DraftOutput, String> + Send + Sync>;

/// study 环 writer 轮上限（plan `loop.study.max_iterations: 3` 的执行面常量）。
const RING_MAX_ITERATIONS: u32 = 3;

/// reader 节点工具声明（web_fetch：external_once 面，设计 §2.2）。
fn web_fetch_decl() -> af_harness::ToolDecl {
    af_harness::ToolDecl {
        name: "web_fetch".into(),
        description: "Fetch an arXiv page (arxiv.org domain only) and land a verifiable \
snapshot; returns {ok,url,title,text,snapshot_ref,sha256}."
            .into(),
        parameters: json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "url": { "type": "string", "description": "https URL, arxiv.org domain only" }
            }
        }),
        effect: af_harness::EffectClass::ExternalOnce,
    }
}

/// submit 类工具声明（参数单字段；effect pure——本地记账）。`payload_hint`
/// 进参数 description（点名载荷形状，防模型双重包裹）。
fn submit_decl(name: &str, what: &str, payload_hint: &str) -> af_harness::ToolDecl {
    af_harness::ToolDecl {
        name: name.into(),
        description: format!("Submit the final {what}; this is the only completion path."),
        parameters: json!({
            "type": "object",
            "required": [what],
            "properties": {
                what: { "type": "object", "description": payload_hint }
            }
        }),
        effect: af_harness::EffectClass::Pure,
    }
}

/// submit 互载荷解包：模型偶发双重包裹（`{"notes": {"notes": [...]}}`——照
/// 提示词字面把整体再包一层）。归一规则：字段值若恰含同名子键，解一层；
/// 至多一层，不递归（递归会掩盖真错形）。
fn unwrap_payload(args: &Value, field: &str) -> Option<Value> {
    let outer = args.get(field)?;
    match outer.get(field) {
        Some(inner) if !inner.is_null() => Some(inner.clone()),
        _ => Some(outer.clone()),
    }
}

/// notes 载荷三形归一（真模型实测变体）：数组（正形）/ 同名包裹
/// （{"notes":[...]}）/ 单条对象（{"point","anchor"}——仅一条笔记时模型
/// 直接给对象）。其余错形不猜，交上层报错。
fn normalize_notes(raw: &Value) -> Option<Value> {
    if raw.is_array() {
        return Some(raw.clone());
    }
    if let Some(inner) = raw.get("notes") {
        if inner.is_array() {
            return Some(inner.clone());
        }
    }
    if raw.get("point").is_some() && raw.get("anchor").is_some() {
        return Some(json!([raw]));
    }
    None
}

/// 真实模型环（F-第五单兑现 G4 裁决17 的铺单批项）：
///
/// - **reader 节点**（uses.model route.main）：系统提示 = 装订模板
///   `pkg.reportkit.reader_system`；上下文 = 焦点 + paper_doc 信封 + 快照
///   截头；工具 = web_fetch（arXiv 白名单真出网，快照副作用落
///   `attempt_dir/sources/` 并入 reader artifact_contract 记账）+ submit_notes
///   （完成信号唯一通路——工具输出首行 = 信号词）。
/// - **writer 节点**（uses.model route.main）：上下文 = paper 信封逐字 +
///   notes 边（envelope 校验过）+ 快照登记表 + cite_check 反馈；工具 =
///   submit_report。产出过 `research_report` envelope 校验。
/// - **cite_check 反馈环**：每轮草稿落 `ring/draft-i<N>.json`，真判分子进程
///   （argv 契约 [task.json, report.json, sources_dir]）核验；failures 回喂
///   writer 下一轮（plan `study` 循环 `cite_feedback` 边的执行面兑现）；
///   轮上限 = plan max_iterations 3，耗尽返回末轮草稿交 verify 判（判分有
///   结论优于无结论——G4 判分纪律；与 plan `on_exhausted: fail` 的差异在
///   RESULT 登记）。
/// - **预算记账**（复用 swev 环）：llm.cents 每 turn 1 分（
///   [`crate::agent::llm_cents_policy`]），spend cap/tool cap/账本总额随
///   spec；采样覆写（temperature/top_p/max_tokens）同 swev agent_spec 面。
/// - **节点工件走 envelope**：reader notes 按 plan types `note_item` 校验、
///   writer 草稿按 `research_report` 校验（types 从装订计划提取，不内置
///   第二套 schema）。
///
/// Err 语义：供给面失败（模型/网络/装载）→ 单元 EX3001 可重试；判分失败
/// 不走 Err（交 verify 记 verdict=false）。
pub fn draft_fn_real_ring(runner: Arc<dyn AgentRunner>, spec: ReportUnitSpec) -> DraftFn {
    Arc::new(move |input: &DraftInput| {
        input.plan_types.require_real()?;
        let profile = crate::profile::profile(&spec.profile)
            .map_err(|err| format!("环内档案解析失败（{0}）: {err}", spec.profile))?;
        let mut usage = DraftOutput::default();

        // ---- reader 节点 ----
        let notes_slot: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
        let sources_ledger: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        sources_ledger
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(json!({
                "url": input.paper.url,
                "title": input.paper.title,
                "sha256": input.paper.sha256,
                "snapshot_ref": input.paper.snapshot_ref,
            }));
        let reader_dir = input.attempt_dir.join("ring/reader");
        std::fs::create_dir_all(&reader_dir)
            .map_err(|err| format!("建 reader run 目录失败: {err}"))?;

        // web_fetch 工具体：arXiv 白名单真抓取 + 快照副作用 + artifact_contract 记账。
        let web_sources = sources_ledger.clone();
        let web_attempt = input.attempt_dir.to_path_buf();
        let web_fetch_tool = Arc::new(move |args: &Value| -> Result<Value, String> {
            let url = args
                .get("url")
                .and_then(Value::as_str)
                .ok_or_else(|| "web_fetch 缺 url 参数".to_string())?;
            let doc = ArxivWebFetcher::fetch_to_doc(url)?;
            doc.write_snapshot(&web_attempt)
                .map_err(|err| format!("web_fetch 快照落盘失败: {err}"))?;
            let payload = json!({
                "ok": true,
                "url": doc.url,
                "title": doc.title,
                "text": doc.text.chars().take(4000).collect::<String>(),
                "snapshot_ref": doc.snapshot_ref,
                "sha256": doc.sha256,
            });
            web_sources
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(json!({
                    "url": doc.url, "sha256": doc.sha256, "snapshot_ref": doc.snapshot_ref,
                }));
            Ok(payload)
        }) as crate::agent::ToolFn;
        let signal = spec.completion_signal.clone();
        let notes_submit_slot = notes_slot.clone();
        let submit_notes_tool = Arc::new(move |args: &Value| -> Result<Value, String> {
            let notes = args.get("notes").and_then(normalize_notes).ok_or_else(|| {
                "submit_notes 载荷应是笔记数组 list<note_item>（[{point,anchor}, ...]）".to_string()
            })?;
            *notes_submit_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(notes);
            Ok(Value::String(signal.clone()))
        }) as crate::agent::ToolFn;

        // 环级共享预算账本（红队 W1-C33）：reader + 全部 writer 轮共用一份
        // 帽（llm.cents 口径 1 模型调用 = 1¢，与图执行器 account_node 同源）
        // ——此前每成员各拿整份 spec.spend_cap_cents + 全新 budget_ledger，
        // 图级上限 4×300¢，spec 自述「图面口径 cents 300」名不副实；plan 级
        // cost_cap 在场时取小（plan.body.budget 此前零消费）。turns/tokens
        // 两维同口径收敛（C33 完整化）：turns 逐 launch 收敛到剩余额度，
        // 三维均在成员起动前对账熔断（耗尽即拒起下一 launch）。
        let ring_cap_cents = spec
            .spend_cap_cents
            .min(input.ring_caps.cents.unwrap_or(i64::MAX));
        let mut ring_spent_cents: i64 = 0;
        let ring_remaining = |spent: i64| -> i64 { (ring_cap_cents - spent).max(1) };
        let ring_tokens_used = |turns_usage: &DraftOutput| -> u64 {
            turns_usage.tokens.input + turns_usage.tokens.output + turns_usage.tokens.cache_read
        };
        // 成员起动前对账（cents/turns/tokens_total 三维，与图执行器预算 gate
        // 同纪律：超限 fail-loud，不再各拿满份帽）。cents 维对账**有效帽**
        // ring_cap_cents = min(spec 帽, plan cost_cap)——两者取小才是环级
        // 真帽（只对 plan 帽会放跑 spec 帽压低的场景）。
        let ring_gate =
            |stage: &str, spent_cents: i64, usage: &DraftOutput| -> Result<(), String> {
                if spent_cents >= ring_cap_cents {
                    return Err(format!(
                    "报告环预算熔断：{stage} 前环内累计 {spent_cents}¢ ≥ 环级帽 {ring_cap_cents}¢ \
                         （= min(spec {spec}¢, plan cost_cap)——reader+writer 共享单一账本）",
                    spec = spec.spend_cap_cents,
                ));
                }
                if let Some(cap) = input.ring_caps.turns {
                    if u64::from(usage.turns) >= u64::from(cap) {
                        return Err(format!(
                            "报告环预算熔断：{stage} 前环内累计 turns {} ≥ 环级帽 {cap} \
                             （plan turns 帽，scope plan/loop:study——reader+writer 共享）",
                            usage.turns,
                        ));
                    }
                }
                if let Some(cap) = input.ring_caps.tokens {
                    let used = ring_tokens_used(usage);
                    if used >= cap {
                        return Err(format!(
                            "报告环预算熔断：{stage} 前环内累计 tokens_total {used} ≥ 环级帽 \
                             {cap}（plan token_cap，口径 in+out+cache_read）"
                        ));
                    }
                }
                Ok(())
            };
        // 单 launch turns 帽收敛到剩余额度（plan turns 帽在场时）——与 cents
        // 的 ring_remaining 同手法，防单成员一次性烧穿环级余量。
        let ring_max_turns = |usage: &DraftOutput| -> u32 {
            match input.ring_caps.turns {
                Some(cap) => spec.max_turns.min(cap.saturating_sub(usage.turns).max(1)),
                None => spec.max_turns,
            }
        };
        ring_gate("reader", ring_spent_cents, &usage)?;

        let reader_user = format!(
            "研究焦点：{focus}\n\n论文（摄取快照 paper_doc）：\n- url: {url}\n- title: {title}\n\
             - sha256: {sha}\n- snapshot_ref: {snap}\n\n论文快照正文（截头）：\n{head}\n\n\
             任务：按系统提示研读，产出带锚点的笔记。完成后调用 submit_notes 工具提交 \
             {{\"notes\": [{{\"point\", \"anchor\"}}, ...]}}——这是唯一的完成通路。\n\
             如需补充材料可调用 web_fetch（url 限 arxiv.org 域）；每次抓取会落快照并返回 \
             snapshot_ref/sha256，笔记锚点须取自快照原文。",
            focus = input.focus,
            url = input.paper.url,
            title = input.paper.title,
            sha = input.paper.sha256,
            snap = input.paper.snapshot_ref,
            head = input.paper_text_head,
        );
        let reader_launch = crate::agent::AgentLaunch {
            attempt_dir: &reader_dir,
            run_id: "report-reader".into(),
            user_message: reader_user,
            spec: af_harness::AgentSpec {
                endpoint: ring_endpoint(&profile.endpoint, &spec),
                context_template: af_harness::ContextTemplate {
                    system: input.reader_prompt.clone(),
                    slots: Vec::new(),
                },
                toolset: vec![
                    web_fetch_decl(),
                    submit_decl(
                        "submit_notes",
                        "notes",
                        "The notes array itself: [{\"point\": string, \"anchor\": string}, ...] — \
                         pass the array as this parameter's value, do not wrap it again",
                    ),
                ],
                policy: crate::agent::llm_cents_policy(
                    ring_max_turns(&usage),
                    &spec.completion_signal,
                    spec.tool_free_turn_limit,
                ),
            },
            bash: Arc::new(|_command| {
                "[harness] bash 不在报告环工具集——用 submit_notes 提交".to_string()
            }),
            key_cmd: profile.key_cmd,
            spend_cap_cents: ring_remaining(ring_spent_cents),
            tool_spend_cents: spec.tool_spend_cents,
            budget_ledger_cents: ring_remaining(ring_spent_cents),
            extra_tools: vec![
                crate::agent::ToolBinding {
                    name: "web_fetch".into(),
                    handler: web_fetch_tool,
                },
                crate::agent::ToolBinding {
                    name: "submit_notes".into(),
                    handler: submit_notes_tool,
                },
            ],
        };
        let reader_report = match runner.run(reader_launch) {
            Ok(crate::agent::AgentRunOutcome {
                report,
                http_retries,
            }) => {
                usage.http_retries += http_retries;
                report
            }
            Err(crate::agent::AgentRunFailure {
                message,
                http_retries,
            }) => {
                return Err(format!(
                    "reader 节点失败（http_retries={http_retries}）: {message}"
                ))
            }
        };
        usage.turns += reader_report.turns;
        usage.model_calls += reader_report.model_calls;
        ring_spent_cents += reader_report.model_calls as i64;
        usage.tokens.input += reader_report.tokens.input;
        usage.tokens.output += reader_report.tokens.output;
        usage.tokens.cache_read += reader_report.tokens.cache_read;
        if !matches!(
            reader_report.status,
            af_harness::RunStatus::Completed { .. }
        ) {
            return Err(format!(
                "reader 节点未正常完成（状态 {:?}）——笔记未提交",
                reader_report.status
            ));
        }
        let notes = notes_slot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| "reader 未经 submit_notes 提交笔记".to_string())?;
        let notes_list = notes
            .as_array()
            .cloned()
            .ok_or_else(|| "submit_notes 载荷应是笔记数组（list<note_item>）".to_string())?;
        for (index, note) in notes_list.iter().enumerate() {
            validate_envelope(&input.plan_types.note_item, note).map_err(|err| {
                format!("reader→writer 边（note_item[{index}]）envelope 拒绝: {err}")
            })?;
        }

        // reader artifact_contract 记账落盘（sources 工件口——裁决5 进图记账）。
        let ledger_snapshot = sources_ledger
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        std::fs::create_dir_all(input.attempt_dir.join("ring"))
            .map_err(|err| format!("建 ring 目录失败: {err}"))?;
        std::fs::write(
            input.attempt_dir.join("ring/sources_ledger.json"),
            serde_json::to_string(&ledger_snapshot).unwrap_or_else(|_| "[]".into()),
        )
        .map_err(|err| format!("写 sources_ledger 失败: {err}"))?;

        // ---- study 环：writer → cite_check（反馈回喂，max_iterations 内） ----
        let ledger_text =
            serde_json::to_string_pretty(&ledger_snapshot).unwrap_or_else(|_| "[]".into());
        let mut cite_feedback: Option<String> = None;
        let mut last_draft: Option<String> = None;
        for round in 1..=RING_MAX_ITERATIONS {
            usage.iterations = round;
            // 环级熔断门（W1-C33，完整化）：cents/turns/tokens_total 三维
            // 共享账本对账——任一维度耗尽即拒起下一成员（与图执行器预算
            // gate 同纪律：超限 fail-loud，不再各拿满份帽）。
            ring_gate(&format!("writer 第 {round} 轮"), ring_spent_cents, &usage)?;
            let report_slot: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
            let slot = report_slot.clone();
            let signal = spec.completion_signal.clone();
            let submit_report_tool = Arc::new(move |args: &Value| -> Result<Value, String> {
                let report = unwrap_payload(args, "report")
                    .ok_or_else(|| "submit_report 缺 report 参数".to_string())?;
                *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(report);
                Ok(Value::String(signal.clone()))
            }) as crate::agent::ToolFn;

            let feedback_section = cite_feedback
                .as_ref()
                .map(|text| {
                    format!(
                        "\n上一轮 cite_check 判分失败（逐条修复，勿整体重写无关部分）：\n{text}\n"
                    )
                })
                .unwrap_or_default();
            let writer_user = format!(
                "论文（report.paper 字段必须逐字使用以下值）：\n- url: {url}\n- title: {title}\n\
                 - sha256: {sha}\n\n研究焦点：{focus}\n\n\
                 可用快照登记表（citations 的 snapshot_ref/snapshot_sha256 必须取自本表）：\n\
                 {ledger}\n\nreader 笔记（notes 边，锚点为快照连续原文）：\n{notes}\n\n\
                 要求：引用数 ≥ min_citations={min}；每条引用五字段齐全\
                 （n,url,quote,snapshot_sha256,snapshot_ref），quote 为对应快照的连续原文\
                 片段（20-600 字符）；正文标记 [n] 与引用双向对齐。\n\
                 research_report 骨架（sections/citations 均为数组，元素形如骨架——\
                 勿以 id 为键构对象）：\n\
                 {{\"paper\": {{\"url\",\"title\",\"sha256\"}}, \"sections\": \
                 [{{\"id\",\"title\",\"body_md\"}}], \"citations\": \
                 [{{\"n\",\"url\",\"quote\",\"snapshot_sha256\",\"snapshot_ref\"}}]}}\n\n\
                 完成后调用 submit_report 工具提交 {{\"report\": <research_report JSON>}}\
                 ——这是唯一的完成通路。{feedback}",
                url = input.paper.url,
                title = input.paper.title,
                sha = input.paper.sha256,
                focus = input.focus,
                ledger = ledger_text,
                notes = serde_json::to_string_pretty(&notes).unwrap_or_default(),
                min = input.min_citations,
                feedback = feedback_section,
            );
            let writer_dir = input.attempt_dir.join(format!("ring/writer-i{round}"));
            std::fs::create_dir_all(&writer_dir)
                .map_err(|err| format!("建 writer run 目录失败: {err}"))?;
            let writer_launch = crate::agent::AgentLaunch {
                attempt_dir: &writer_dir,
                run_id: format!("report-writer-i{round}"),
                user_message: writer_user,
                spec: af_harness::AgentSpec {
                    endpoint: ring_endpoint(&profile.endpoint, &spec),
                    context_template: af_harness::ContextTemplate {
                        system: input.writer_prompt.clone(),
                        slots: Vec::new(),
                    },
                    toolset: vec![submit_decl(
                        "submit_report",
                        "report",
                        "The research_report JSON object itself: {paper, sections, citations} — \
                         pass the object as this parameter's value, do not wrap it again",
                    )],
                    policy: crate::agent::llm_cents_policy(
                        ring_max_turns(&usage),
                        &spec.completion_signal,
                        spec.tool_free_turn_limit,
                    ),
                },
                bash: Arc::new(|_command| {
                    "[harness] bash 不在报告环工具集——用 submit_report 提交".to_string()
                }),
                key_cmd: profile.key_cmd,
                spend_cap_cents: ring_remaining(ring_spent_cents),
                tool_spend_cents: spec.tool_spend_cents,
                budget_ledger_cents: ring_remaining(ring_spent_cents),
                extra_tools: vec![crate::agent::ToolBinding {
                    name: "submit_report".into(),
                    handler: submit_report_tool,
                }],
            };
            let writer_report = match runner.run(writer_launch) {
                Ok(crate::agent::AgentRunOutcome {
                    report,
                    http_retries,
                }) => {
                    usage.http_retries += http_retries;
                    report
                }
                Err(crate::agent::AgentRunFailure {
                    message,
                    http_retries,
                }) => {
                    return Err(format!(
                        "writer 节点失败（第 {round} 轮，http_retries={http_retries}）: {message}"
                    ))
                }
            };
            usage.turns += writer_report.turns;
            usage.model_calls += writer_report.model_calls;
            ring_spent_cents += writer_report.model_calls as i64;
            usage.tokens.input += writer_report.tokens.input;
            usage.tokens.output += writer_report.tokens.output;
            usage.tokens.cache_read += writer_report.tokens.cache_read;
            if !matches!(
                writer_report.status,
                af_harness::RunStatus::Completed { .. }
            ) {
                return Err(format!(
                    "writer 节点未正常完成（第 {round} 轮，状态 {:?}）——报告未提交",
                    writer_report.status
                ));
            }
            let report_value = report_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
                .ok_or_else(|| format!("writer 未经 submit_report 提交报告（第 {round} 轮）"))?;
            validate_envelope(&input.plan_types.research_report, &report_value)
                .map_err(|err| format!("writer 产出（research_report）envelope 拒绝: {err}"))?;

            // cite_check：真判分子进程（argv 契约三位置参数；零 LLM 零网络）。
            let draft_text = serde_json::to_string(&report_value)
                .map_err(|err| format!("草稿序列化失败: {err}"))?;
            let draft_path = input.attempt_dir.join(format!("ring/draft-i{round}.json"));
            std::fs::write(&draft_path, &draft_text).map_err(|err| format!("写草稿失败: {err}"))?;
            last_draft = Some(draft_text);
            let cite_out = invoke_citation(
                &spec.judges,
                &input.task_json,
                &draft_path,
                &input.attempt_dir.join("sources"),
            )
            .map_err(|err| format!("环内 citation 判分器拉起失败: {err}"))?;
            if !cite_out.status.success() {
                return Err(format!(
                    "环内 citation 判分器退出 {}: {}",
                    cite_out.status,
                    String::from_utf8_lossy(&cite_out.stderr)
                ));
            }
            let verdict: Value = serde_json::from_slice(&cite_out.stdout)
                .map_err(|err| format!("环内 citation 判定不是 JSON: {err}"))?;
            if verdict["passed"].as_bool().unwrap_or(false) {
                usage.report_json = last_draft.expect("末轮草稿在案");
                return Ok(usage);
            }
            let failures: Vec<String> = verdict["failures"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .map(|f| f.as_str().unwrap_or_default().to_string())
                        .collect()
                })
                .unwrap_or_default();
            cite_feedback = Some(failures.join("\n"));
        }
        // 轮耗尽：返回末轮草稿交 verify 判（判分有结论优于无结论；与 plan
        // on_exhausted: fail 的差异在 RESULT 登记）。
        usage.report_json = last_draft.expect("至少一轮 writer 在案");
        Ok(usage)
    })
}

/// 环面 endpoint 组装（swev agent_spec 同面：采样覆写随 spec——换档=换spec）。
fn ring_endpoint(
    endpoint: &af_harness::EndpointSpec,
    spec: &ReportUnitSpec,
) -> af_harness::EndpointSpec {
    let mut endpoint = endpoint.clone();
    endpoint.temperature_milli = Some(spec.temperature_milli);
    endpoint.top_p_milli = Some(spec.top_p_milli);
    endpoint.max_tokens = Some(spec.max_tokens);
    endpoint
}

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

/// 失败行分类相（红队 W2-C34）：setup = 供给面（任务包/计划装订），
/// run = ingest 相起（摄取/起草/判分）。
const SETUP_PHASE: &str = "setup";
const RUN_PHASE: &str = "run";

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
        // 红队 W2-C34：失败行由单元落（RESULT-P4 §5）——report 此前只在成功
        // 尾部双轨落行，任何失败路径提前 return、record_sink 零调用，失败单
        // 在结果轨与「从未开跑」不可区分（盘上实证：F5-report results/records
        // 双 0 字节）。wrapper：setup 相失败落 setup_failed、ingest 相起失败落
        // run_failed（legacy 轨，失败行无 v1 投影），行附真实错误类别与
        // verdict=null；原错误类别原样上抛不翻案。
        let phase = std::cell::Cell::new(SETUP_PHASE);
        let started = std::time::Instant::now();
        match self.drive_all(ctx, &phase, started) {
            Ok(output) => Ok(output),
            Err(err) => Err(self.emit_failure_row(ctx, err, phase.get())),
        }
    }


}

impl ReportExecuteUnit {
    fn drive_all(
        &self,
        ctx: &ExecContext<'_>,
        phase: &std::cell::Cell<&'static str>,
        started: std::time::Instant,
    ) -> Result<ExecOutput, ExeclineError> {
        // ---- setup 相：任务包装载 + 计划装订三核对（planbind 唯一正本）----
        let task_dir = ctx.data_root.join(ctx.task_id);
        let task_json = task_dir.join("task.json");
        // 红队 W1-C59①：判据面 digest 钉死核对（门面 submit 时计算）。
        crate::exec::verify_task_digest(&task_json, ctx.task_sha256)?;
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
        // 红队 W2-C55：计划面摘要复核（先于装订）——报告环预算帽（cost_cap/
        // token_cap/turns）全在 plan.json；摘要漂移 = 计划面被换，拒绝执行。
        crate::exec::verify_plan_digest(&task_dir.join("plan.json"), ctx.plan_sha256)?;
        let bound = planbind::bind(
            &plan_src,
            &self.spec.registry_root,
            &[
                &self.spec.reader_template_ref,
                &self.spec.writer_template_ref,
            ],
            // 红队 W1-C32：WorkOrder 一次性绑定的 graph_version 作装订外部
            // 期待（此前期待分支恒空 = 死代码，图版本漂移的计划照样装订）。
            &BindExpectations {
                graph_version: ctx.graph_version.map(str::to_string),
                // 红队 W2-C36：订单已绑定的 line 作装订期待（driver bind_line
                // 先于 execute）——此前恒传 None，planbind ③ 的 line 复验
                // 分支在生产是死代码，携带其他产线 line_id 的手写计划可跨线
                // 运行无感。
                line_id: Some(ctx.line_id.to_string()),
            },
        )?;
        let min_citations = task
            .rubric_params
            .get("min_citations")
            .and_then(Value::as_u64)
            .unwrap_or(3) as usize;

        // ---- ingest 相：工具副作用落快照（谁写=fetcher、写哪=sources/）----
        // 红队 W2-C34：此后失败归 run_failed（供给面已就位，run 目录随建）。
        phase.set(RUN_PHASE);
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

        // ---- run 相（真模型环/缝）：起草 ----
        let plan_types = RingTypes::from_plan_types(&bound.plan.body.types)
            .unwrap_or_else(|_| RingTypes::permissive());
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
            paper: PaperEnvelope::from_doc(&doc),
            attempt_dir: attempt_dir.to_path_buf(),
            task_json: task_json.clone(),
            plan_types,
            // W1-C33：plan.body.budget 白名单提取（cents/turns/tokens_total
            // 三维，强制集外 fail-loud 拒收）——reader + 全部 writer 轮共享
            // 单一账本；此前仅 find cost_cap，turns/token 帽被静默忽略。
            ring_caps: RingBudgets::of(&bound.plan)?,
        };
        let draft =
            (self.draft)(&draft_input).map_err(|err| self.fail(format!("起草失败: {err}")))?;
        let report: Value = serde_json::from_str(draft.report_json.trim())
            .map_err(|err| self.fail(format!("起草产物不是合法 report JSON: {err}")))?;
        std::fs::write(attempt_dir.join("report.json"), report.to_string())
            .map_err(|err| self.fail(format!("写 report.json 失败: {err}")))?;

        // report.md：与 rubric R5 同一渲染器（同构性由同一脚本保证）。
        let render_out = judge_python(&self.spec.judges)
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
            http_retries: draft.http_retries,
            turns: draft.turns,
            model_calls: draft.model_calls,
            tokens: draft.tokens,
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
    /// 失败行落盘（legacy 轨）：sink 失败并案进文案转 EX3001（与
    /// `record::fail_with_legacy_row` 同口径——不静默丢行）；sink 正常则原
    /// 错误原样返回。
    fn emit_failure_row(
        &self,
        ctx: &ExecContext<'_>,
        err: ExeclineError,
        phase: &str,
    ) -> ExeclineError {
        let note = format!("[{code}] {err}", code = err.code().as_str());
        let row = if phase == RUN_PHASE {
            crate::record::unit_run_failed_row(
                "report",
                ctx.task_id,
                &self.spec.profile,
                ctx.trial,
                err.code().as_str(),
                ctx.attempt_dir,
                &note,
            )
        } else {
            crate::record::unit_setup_failed_row(
                "report",
                ctx.task_id,
                &self.spec.profile,
                ctx.trial,
                err.code().as_str(),
                &note,
            )
        };
        match ctx.record_sink.append_legacy(&row.to_string()) {
            Ok(()) => err,
            Err(sink_err) => ExeclineError::UnitFailed {
                detail: format!("{err}；且失败行落盘失败: {sink_err}"),
            },
        }
    }
}
