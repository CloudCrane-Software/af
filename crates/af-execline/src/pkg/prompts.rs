//! template_ref 解析——两包唯一提示词装载机制（裁决10 正本）。
//!
//! 机制（G4 软件工程包设计 §3.2；报告包 §2.9 同走本解析器）：
//! `pkg.<包>.<名>[@<版本>]` → `<包根>/<包>/prompts/<名>.md`，装载期逐文件
//! sha256 与 `MANIFEST.sha256` 清单核对，不符拒绝启动（EX4xxx，同 E9004 的
//! 拒绝式纪律）；版本段为**可选语法**（与 uses.* 一致），给出则与清单版本核对。
//!
//! 已知缺口（如实，报告包 §2.9 登记）：`template_ref` 不经编译器绑定层核对
//! （af-dsl 只查在场性，af-compiler bind 只解析 `uses.*`）——写错名 L1-L4 全
//! 放行，本装载期门是 v1 的补偿面；bind 扩展到 sections 面留 G4 后裁决。
//!
//! `MANIFEST.sha256` 格式（本批定义，登记于 af/packages/README）：
//!
//! ```text
//! # manifest_version: 1
//! <sha256(64 位小写十六进制)>  prompts/<名>.md
//! ```
//!
//! 头行 `# manifest_version:` 为清单版本（template_ref `@版本` 段的核对正本）；
//! 数据行与 `sha256sum` 输出同形（两空格分隔），`sha256sum -c` 可直接校验；
//! 其余 `#` 行为注释。

use std::path::{Path, PathBuf};

use crate::error::ExeclineError;
use crate::suite::swev::sha256_hex;

/// 解析后的模板引用：`(包, 名, 版本?)`。
pub type ParsedTemplateRef = (String, String, Option<String>);

/// 解析 template_ref 语法（纯函数；路径注入防护——包/名段禁 `/` 与 `..`）。
pub fn parse_template_ref(template_ref: &str) -> Result<ParsedTemplateRef, ExeclineError> {
    let invalid = |detail: &str| ExeclineError::TemplateRefInvalid {
        reference: template_ref.to_string(),
        detail: detail.into(),
    };
    let rest = template_ref
        .strip_prefix("pkg.")
        .ok_or_else(|| invalid("须以 pkg. 前缀开头"))?;
    let (name_part, version) = match rest.split_once('@') {
        None => (rest, None),
        Some((name, ver)) => {
            if ver.is_empty() || ver.contains('@') {
                return Err(invalid("版本段为空或含多个 @"));
            }
            (name, Some(ver.to_string()))
        }
    };
    let (package, name) = name_part.split_once('.').ok_or_else(|| invalid("缺名段"))?;
    if package.is_empty() || name.is_empty() {
        return Err(invalid("包段或名段为空"));
    }
    if [package, name]
        .iter()
        .any(|seg| seg.contains('/') || seg.contains('\\') || seg.contains(".."))
    {
        return Err(invalid("包/名段不得含路径分隔符或 .."));
    }
    Ok((package.to_string(), name.to_string(), version))
}

/// 装载校验后的模板资产（正文 + 指纹元数据）。
#[derive(Debug, Clone, PartialEq)]
pub struct PromptAsset {
    /// 原引用串（原样登记）。
    pub reference: String,
    /// 包段。
    pub package: String,
    /// 名段。
    pub name: String,
    /// 引用给出的版本段（可选语法）。
    pub version: Option<String>,
    /// 模板文件绝对/相对路径（解析产物）。
    pub path: PathBuf,
    /// 文件实测 sha256（= 清单登记值，已核对）。
    pub sha256: String,
    /// 模板正文（UTF-8 全文；vars 替换由装订器做，本层不动字节）。
    pub content: String,
}

/// 清单正本结构：版本 + 逐文件 sha256。
#[derive(Debug, Clone, PartialEq)]
struct PromptManifest {
    manifest_version: String,
    /// `(sha256, 相对路径)`，按文件序。
    entries: Vec<(String, String)>,
}

fn load_manifest(prompts_dir: &Path, reference: &str) -> Result<PromptManifest, ExeclineError> {
    let unregistered = |detail: String| ExeclineError::TemplateUnregistered {
        reference: reference.to_string(),
        detail,
    };
    let text = std::fs::read_to_string(prompts_dir.join("MANIFEST.sha256"))
        .map_err(|err| unregistered(format!("读 MANIFEST.sha256 失败: {err}")))?;
    let mut manifest_version: Option<String> = None;
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        if let Some(ver) = line.strip_prefix("# manifest_version:") {
            manifest_version = Some(ver.trim().to_string());
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        match (parts.next(), parts.next(), parts.next()) {
            (Some(sha), Some(rel), None) if sha.len() == 64 && !rel.is_empty() => {
                entries.push((sha.to_string(), rel.to_string()));
            }
            _ => {
                return Err(unregistered(format!("清单行不合法: {line}")));
            }
        }
    }
    let manifest_version =
        manifest_version.ok_or_else(|| unregistered("清单缺 # manifest_version: 头".into()))?;
    if manifest_version.is_empty() {
        return Err(unregistered("清单版本段为空".into()));
    }
    Ok(PromptManifest {
        manifest_version,
        entries,
    })
}

/// 解析并核对一个 template_ref（装载期门）：
/// 清单在册 → 文件在盘 → sha256 逐位一致 → 版本段（若给出）与清单版本一致。
pub fn resolve_template(pkg_root: &Path, template_ref: &str) -> Result<PromptAsset, ExeclineError> {
    let (package, name, version) = parse_template_ref(template_ref)?;
    let drift = |detail: String| ExeclineError::TemplateDrift {
        reference: template_ref.to_string(),
        detail,
    };
    let prompts_dir = pkg_root.join(&package).join("prompts");
    let manifest = load_manifest(&prompts_dir, template_ref)?;
    let rel = format!("prompts/{name}.md");
    let expected_sha = manifest
        .entries
        .iter()
        .find(|(_, r)| r == &rel)
        .map(|(sha, _)| sha.clone())
        .ok_or_else(|| ExeclineError::TemplateUnregistered {
            reference: template_ref.to_string(),
            detail: format!("模板 {rel} 未登记进 MANIFEST.sha256"),
        })?;
    let path = prompts_dir.join(format!("{name}.md"));
    let actual_sha = sha256_hex(&path).ok_or_else(|| ExeclineError::TemplateUnregistered {
        reference: template_ref.to_string(),
        detail: format!("模板文件缺失或不可读: {}", path.display()),
    })?;
    if actual_sha != expected_sha {
        return Err(drift(format!(
            "文件 sha256 {actual_sha} ≠ 清单登记 {expected_sha}（{}）",
            path.display()
        )));
    }
    if let Some(ver) = &version {
        if ver != &manifest.manifest_version {
            return Err(drift(format!(
                "引用版本段 {ver} ≠ 清单版本 {}",
                manifest.manifest_version
            )));
        }
    }
    let content =
        std::fs::read_to_string(&path).map_err(|err| ExeclineError::TemplateUnregistered {
            reference: template_ref.to_string(),
            detail: format!("读模板失败 {}: {err}", path.display()),
        })?;
    Ok(PromptAsset {
        reference: template_ref.to_string(),
        package,
        name,
        version,
        path,
        sha256: actual_sha,
        content,
    })
}

/// swev 线两段提示词的包内装载（设计 §3.2-3 装载化：`SwevUnitSpec` 的
/// system_prompt/instance_protocol 从冻结常量改为装载包资产的正路入口；
/// 字节一致性由 tests/pkg_sweng_package.rs 钉死——迁移纪律：与
/// `suite::swev` 常量 UTF-8 字节相同，一个字节不动）。
pub fn load_swev_prompts(pkg_root: &Path) -> Result<(String, String), ExeclineError> {
    let system = resolve_template(pkg_root, "pkg.sweng.swev_system")?;
    let protocol = resolve_template(pkg_root, "pkg.sweng.swev_instance_protocol")?;
    Ok((system.content, protocol.content))
}
