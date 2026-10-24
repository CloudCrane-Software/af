//! 包图清单 registry 装载器（P2-L09，失败分类账 #1-4「缺组件」终裁补齐）。
//!
//! 裁决背景：ADAS / AutoGen-Team-JSON / GPTSwarm / MetaGPT 四份对齐草稿被
//! E2001 拒（topology_selector 候选图未在 declared_graphs）。终裁=语法不动
//! （引用形合法），补装载侧组件：
//!
//! - ① manifest 规范：`af/packages/<包>/manifest.json`（JSON——serde_json
//!   已在依赖面内，`deny_unknown_fields` + 本模块静态校验）。每包声明其
//!   提供的 graphs 清单：局部名 + 版本 + 来源引用（AFB 图本体可后续提供，
//!   装载期只登记引用）。组合规则：`pkg.{package}.{name}@{version}`。
//! - ③ registry 装载：扫描 `<根>/<包>/manifest.json` → 版本化图引用集 →
//!   经 [`crate::ValidateOptions::declared_graphs`] 注入 `validate_with`。
//!   不注入（默认空集）= 一切候选引用仍拒（E2001），fail-closed，无隐式放行。
//!
//! 静态校验（装载即拒，不留到 validate 层）：
//! - `package` / `name` / `version` / `source` 非空，且前三者不含 '@'
//!   （组合面以 '@' 分隔 name@version，字段内再含 '@' 即歧义）；
//! - `graphs` 至少一项（manifest 唯一职责 = 声明图）；
//! - 同清单内组合引用不重复；跨包重复 → 装载错误（首声者路径在案）。
//!
//! 确定性：目录条目按路径排序后处理；引用集为 BTree（同
//! `ValidateOptions.declared_graphs` 的类型，直接注入）。
//!
//! 扫描规则：**目录即包候选**——根下的目录（'.' 起始的隐藏目录除外）必须
//! 有 `manifest.json`（缺 = 错误，抓命名笔误）；非目录条目（README.md、
//! .gitkeep 等）不是包，跳过；空根合法 = 空集。

use crate::ValidateOptions;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

/// registry 装载错误（装载侧首错即停——manifest 是静态资产，非文档语义，
/// 不走 Report 一次收齐范式）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// 包根目录不存在。
    RootMissing(PathBuf),
    /// 包根存在但不是目录。
    RootNotDir(PathBuf),
    /// 包目录缺少 manifest.json。
    ManifestMissing(PathBuf),
    /// manifest 读失败。
    ManifestRead { path: PathBuf, cause: String },
    /// manifest JSON 解析或静态校验失败（inline 解析无路径 → None）。
    ManifestInvalid {
        path: Option<PathBuf>,
        message: String,
    },
    /// 同一组合引用被多个清单声明（first = 首声者路径，second = 重复者）。
    DuplicateGraphRef {
        reference: String,
        first: PathBuf,
        second: PathBuf,
    },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RootMissing(p) => write!(f, "包根目录不存在：{}", p.display()),
            Self::RootNotDir(p) => write!(f, "包根不是目录：{}", p.display()),
            Self::ManifestMissing(p) => write!(f, "包目录缺少 manifest.json：{}", p.display()),
            Self::ManifestRead { path, cause } => {
                write!(f, "manifest 读取失败 {}: {cause}", path.display())
            }
            Self::ManifestInvalid { path, message } => match path {
                Some(p) => write!(f, "manifest 非法 {}: {message}", p.display()),
                None => write!(f, "manifest 非法：{message}"),
            },
            Self::DuplicateGraphRef {
                reference,
                first,
                second,
            } => write!(
                f,
                "图引用 {reference:?} 重复声明：{} 与 {}",
                first.display(),
                second.display()
            ),
        }
    }
}

impl std::error::Error for RegistryError {}

/// 包清单：`af/packages/<包>/manifest.json` 的正本结构。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    /// 包名（组合引用的第二段，如 "adaskit" → `pkg.adaskit.*`）。
    pub package: String,
    /// 本包提供的图清单（至少一项）。
    pub graphs: Vec<GraphDecl>,
}

/// 一条图声明：局部名 + 版本 + 来源引用。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphDecl {
    /// 图局部名（可含点号分层，如 "team.round_robin"）。
    pub name: String,
    /// 版本（自由串，同 meta.graph_version，如 "1"、"1.0"、"2.1"）。
    pub version: String,
    /// 来源引用（AFB 图本体的定位串；首版只登记不打开）。
    pub source: String,
}

impl PackageManifest {
    /// 解析 JSON + 静态校验（serde 首错；校验规则见模块注释）。
    pub fn parse(src: &str) -> Result<Self, RegistryError> {
        let m: Self = serde_json::from_str(src).map_err(|e| RegistryError::ManifestInvalid {
            path: None,
            message: e.to_string(),
        })?;
        m.validate()?;
        Ok(m)
    }

    /// 从文件解析（错误带路径）。
    fn parse_file(path: &Path) -> Result<Self, RegistryError> {
        let src = std::fs::read_to_string(path).map_err(|e| RegistryError::ManifestRead {
            path: path.to_path_buf(),
            cause: e.to_string(),
        })?;
        Self::parse(&src).map_err(|e| match e {
            RegistryError::ManifestInvalid { message, .. } => RegistryError::ManifestInvalid {
                path: Some(path.to_path_buf()),
                message,
            },
            other => other,
        })
    }

    fn validate(&self) -> Result<(), RegistryError> {
        let invalid = |message: String| RegistryError::ManifestInvalid {
            path: None,
            message,
        };
        if self.package.is_empty() {
            return Err(invalid("package 非空必填".to_string()));
        }
        if self.package.contains('@') {
            return Err(invalid(format!(
                "package {:?} 不得含 '@'（组合面保留分隔符）",
                self.package
            )));
        }
        if self.graphs.is_empty() {
            return Err(invalid(
                "graphs 至少声明一项（manifest 唯一职责 = 声明图）".to_string(),
            ));
        }
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for g in &self.graphs {
            if g.name.is_empty() || g.version.is_empty() || g.source.is_empty() {
                return Err(invalid(format!(
                    "graphs 条目 name/version/source 非空必填：{{ name: {:?}, version: {:?}, source: {:?} }}",
                    g.name, g.version, g.source
                )));
            }
            if g.name.contains('@') || g.version.contains('@') {
                return Err(invalid(format!(
                    "graphs 条目 {{ name: {:?}, version: {:?} }} 不得含 '@'（组合面保留分隔符）",
                    g.name, g.version
                )));
            }
            let r = g.composed_ref(&self.package);
            if !seen.insert(r.clone()) {
                return Err(invalid(format!("图引用 {r:?} 在同清单内重复声明")));
            }
        }
        Ok(())
    }

    /// 本清单产出的全部版本化图引用（声明序）。
    pub fn graph_refs(&self) -> Vec<String> {
        self.graphs
            .iter()
            .map(|g| g.composed_ref(&self.package))
            .collect()
    }
}

impl GraphDecl {
    /// 组合引用：`pkg.{package}.{name}@{version}`。
    fn composed_ref(&self, package: &str) -> String {
        format!("pkg.{package}.{}@{}", self.name, self.version)
    }
}

/// 装载好的图引用集（注入面：`options()` → `validate_with`）。
#[derive(Debug, Clone, Default)]
pub struct Registry {
    declared_graphs: BTreeSet<String>,
    /// 包目录 → 该包清单。BTreeMap：装载错误报序确定（路径序）。
    packages: BTreeMap<PathBuf, PackageManifest>,
}

impl Registry {
    /// 扫描包根：`<根>/<包>/manifest.json` 全量装载（规则见模块注释）。
    pub fn load(packages_root: impl AsRef<Path>) -> Result<Self, RegistryError> {
        let root = packages_root.as_ref();
        if !root.exists() {
            return Err(RegistryError::RootMissing(root.to_path_buf()));
        }
        if !root.is_dir() {
            return Err(RegistryError::RootNotDir(root.to_path_buf()));
        }
        // read_dir 序不确定 → 收集后按路径排序，处理与报错皆确定。
        let mut entries: Vec<PathBuf> = std::fs::read_dir(root)
            .map_err(|e| RegistryError::ManifestRead {
                path: root.to_path_buf(),
                cause: e.to_string(),
            })?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        entries.sort();
        let mut reg = Self::default();
        for entry in entries {
            if entry
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'))
            {
                continue; // 隐藏目录（.git 等）不是包。
            }
            let manifest_path = entry.join("manifest.json");
            if !manifest_path.is_file() {
                return Err(RegistryError::ManifestMissing(entry));
            }
            let m = PackageManifest::parse_file(&manifest_path)?;
            reg.packages.insert(entry.clone(), m);
        }
        // 跨包重复引用：首声者（路径序）胜出报 first，后到者报 second。
        let mut first_seen: BTreeMap<String, PathBuf> = BTreeMap::new();
        for (dir, m) in &reg.packages {
            for r in m.graph_refs() {
                if let Some(first) = first_seen.get(&r) {
                    return Err(RegistryError::DuplicateGraphRef {
                        reference: r,
                        first: first.clone(),
                        second: dir.clone(),
                    });
                }
                first_seen.insert(r.clone(), dir.clone());
                reg.declared_graphs.insert(r);
            }
        }
        Ok(reg)
    }

    /// 已声明的版本化图引用集（即 `ValidateOptions.declared_graphs` 的注入值）。
    pub fn declared_graphs(&self) -> &BTreeSet<String> {
        &self.declared_graphs
    }

    /// 装载的包数。
    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    /// 注入面：以本 registry 构造 `ValidateOptions`（其余可配置项维持默认）。
    pub fn options(&self) -> ValidateOptions {
        ValidateOptions {
            declared_graphs: self.declared_graphs.clone(),
            ..Default::default()
        }
    }
}
