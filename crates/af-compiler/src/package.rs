//! E1 缺口②：任务包内容 hash 进提交校验——防重放语义内置编译链。
//!
//! W2-C55 的正确工作只覆盖「同一订单号重放时计划面/判据面不可偷换」
//! （门面 submit 时钉死 sha256 → 凭证面留档 → 重放对账）；它不覆盖
//! 「编译后、提交前任务包内容被改」——彼时钉死值从被改后的文件现算，
//! 偷换即成为新基线（E2G2 实证：预算帽全在 plan.json，换计划面=放宽预算）。
//! 本模块把编译链侧的补齐：
//!
//! - [`write_package_manifest`]：编译/组包时把任务包判据面（task.json）与
//!   计划面（plan.json）的内容 sha256 写进 `package_manifest.json`——内容
//!   hash 在组包一刻进校验面，此后任何改动都与留档不符；
//! - [`verify_package_manifest`]：提交侧（af-order 门面，正式 + 干跑两路）
//!   在一切副作用之前重算两份摘要并与 manifest 核对——不符即 fail-loud，
//!   点名「任务包内容已改，须清态换号重提」（防重放契约的完整句：
//!   改动任务包 = 清状态、换订单号、重新提交）；
//! - manifest 缺席 = [`ManifestVerification::Absent`]（G3 收口）：不再与
//!   「核对过且通过」同住一个 `Ok(Some(..))/Ok(None)` 形态——缺席是显式
//!   可机检的「未核对」状态，要求核对的路径（af-order preflight）对它
//!   fail-closed 拒绝，不许静默放行（旧口径「缺席 = 不核对，旧任务包
//!   零破窗」在 G3 前是未组包破窗：删 manifest 即绕过核对，实证可整单
//!   真跑）。
//!
//! 确定性纪律：manifest 键序固定（BTree 序列化），同内容同字节。

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

/// manifest 文件名（任务包目录内，与 task.json/plan.json 同级）。
pub const PACKAGE_MANIFEST_FILE: &str = "package_manifest.json";
/// manifest 格式版本（拒绝式：整份拒绝未知版本）。
pub const PACKAGE_MANIFEST_VERSION: u32 = 1;

/// 任务包内容校验 manifest（E1 缺口②）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPackageManifest {
    /// 格式版本（当前 = 1）。
    pub format_version: u32,
    /// 任务包判据面内容 sha256（`<task>/task.json`）。
    pub task_sha256: String,
    /// 任务包计划面内容 sha256（`<task>/plan.json`）。
    pub plan_sha256: String,
    /// 编译产物计划身份（plan_id 确定性派生形——manifest 归属面，核对时
    /// 与计划面摘要共同确认「这份 manifest 是为这份计划签的」）。
    pub plan_id: String,
    /// 写 manifest 的 af-compiler 版本（溯源面，不参与核对）。
    pub compiler_version: String,
}

/// 任务包 manifest 错误（fail-loud，拒绝式）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageError {
    /// 组包时任务包文件缺席/不可读。
    PackageFileRead { path: PathBuf, detail: String },
    /// manifest 文件本身坏形（非本 schema / 版本非 1）。
    MalformedManifest { detail: String },
    /// 内容 hash 与 manifest 留档不符——防重放契约违例。
    ContentTampered {
        file: &'static str,
        recorded: String,
        actual: String,
    },
}

impl fmt::Display for PackageError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::PackageFileRead { path, detail } => {
                write!(f, "任务包文件不可读（{}）: {detail}", path.display())
            }
            Self::MalformedManifest { detail } => write!(f, "package_manifest.json 坏形：{detail}"),
            Self::ContentTampered {
                file,
                recorded,
                actual,
            } => write!(
                f,
                "任务包内容已改（{file}）：manifest 留档 {recorded} ≠ 实得 {actual}——\
                 防重放契约违例（W2-C55 语义内置编译链）：改动任务包须清态换号重提，\
                 不得在原订单号上重放"
            ),
        }
    }
}

impl std::error::Error for PackageError {}

/// manifest 核对结果（G3：「核对过且通过」与「缺席 = 未核对」在类型上分开）。
///
/// 旧形态 `Result<Option<Manifest>, _>` 里 `Ok(None)`（缺席）与
/// `Ok(Some(_))`（核对过且通过）同为「成功」——调用方不 match 就无从
/// 区分，未组包/被删 manifest 的任务包静默拿到「成功但什么都没核对」。
/// 本枚举让缺席成为显式可见、可机检的状态：`Absent` **不是通过**——
/// 要求核对的路径必须拒绝它（fail-closed）；确实允许缺席的路径必须把它
/// 如实标注为「未核对」，不得与 [`ManifestVerification::Verified`] 混同。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestVerification {
    /// manifest 在场，task.json/plan.json 内容 sha256 与留档逐位一致。
    Verified(TaskPackageManifest),
    /// manifest 文件缺席 = 未核对（既不是通过、也不是不适用——调用方
    /// 必须显式处理本状态）。
    Absent,
}

/// 任务包目录内单文件内容 sha256（读不到即 fail-loud）。
fn file_digest(path: &Path) -> Result<String, PackageError> {
    let bytes = std::fs::read(path).map_err(|err| PackageError::PackageFileRead {
        path: path.to_path_buf(),
        detail: err.to_string(),
    })?;
    Ok(crate::lower::digest_of(&bytes))
}

/// 组包时写 `package_manifest.json`：任务包目录须已含 task.json 与
/// plan.json（内容 hash 在组包一刻进校验面）。返回写入的 manifest
/// （调用方落盘或留证皆可——确定性序列化，同内容同字节）。
pub fn build_package_manifest(
    task_dir: &Path,
    plan_id: impl Into<String>,
) -> Result<TaskPackageManifest, PackageError> {
    Ok(TaskPackageManifest {
        format_version: PACKAGE_MANIFEST_VERSION,
        task_sha256: file_digest(&task_dir.join("task.json"))?,
        plan_sha256: file_digest(&task_dir.join("plan.json"))?,
        plan_id: plan_id.into(),
        compiler_version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// 组包 + 落盘一体入口：写 `<task_dir>/package_manifest.json`（pretty，
/// 键序固定）。已存在即覆盖（组包是确定性动作，重组包=重签内容留档）。
pub fn write_package_manifest(
    task_dir: &Path,
    plan_id: impl Into<String>,
) -> Result<TaskPackageManifest, PackageError> {
    let manifest = build_package_manifest(task_dir, plan_id)?;
    let json =
        serde_json::to_string_pretty(&manifest).map_err(|err| PackageError::MalformedManifest {
            detail: format!("manifest 序列化失败: {err}"),
        })?;
    std::fs::write(task_dir.join(PACKAGE_MANIFEST_FILE), format!("{json}\n")).map_err(|err| {
        PackageError::PackageFileRead {
            path: task_dir.join(PACKAGE_MANIFEST_FILE),
            detail: err.to_string(),
        }
    })?;
    Ok(manifest)
}

/// 提交侧核对：在场即重算 task.json/plan.json 内容 sha256 与留档逐位
/// 比对——任一不符即 [`PackageError::ContentTampered`]（文案含「清态换号
/// 重提」行动指令）。manifest 缺席返回 [`ManifestVerification::Absent`]
/// ——显式「未核对」状态（G3：不是 `Ok(None)` 时代的静默不核对，调用方
/// 必须处理；要求核对的路径对它 fail-closed 拒绝）。
pub fn verify_package_manifest(
    task_dir: &Path,
) -> Result<ManifestVerification, PackageError> {
    let path = task_dir.join(PACKAGE_MANIFEST_FILE);
    if !path.is_file() {
        return Ok(ManifestVerification::Absent);
    }
    let manifest: TaskPackageManifest =
        serde_json::from_str(&std::fs::read_to_string(&path).map_err(|err| {
            PackageError::PackageFileRead {
                path: path.clone(),
                detail: err.to_string(),
            }
        })?)
        .map_err(|err| PackageError::MalformedManifest {
            detail: err.to_string(),
        })?;
    if manifest.format_version != PACKAGE_MANIFEST_VERSION {
        return Err(PackageError::MalformedManifest {
            detail: format!(
                "format_version={} 非 {PACKAGE_MANIFEST_VERSION}（拒绝式：整份拒绝）",
                manifest.format_version
            ),
        });
    }
    let actual_task = file_digest(&task_dir.join("task.json"))?;
    if actual_task != manifest.task_sha256 {
        return Err(PackageError::ContentTampered {
            file: "task.json",
            recorded: manifest.task_sha256.clone(),
            actual: actual_task,
        });
    }
    let actual_plan = file_digest(&task_dir.join("plan.json"))?;
    if actual_plan != manifest.plan_sha256 {
        return Err(PackageError::ContentTampered {
            file: "plan.json",
            recorded: manifest.plan_sha256.clone(),
            actual: actual_plan,
        });
    }
    Ok(ManifestVerification::Verified(manifest))
}
