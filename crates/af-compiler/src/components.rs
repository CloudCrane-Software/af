//! 组件注册表装载器（G2b stage 3 的对照面）。
//!
//! 数据源：`af/packages` 的 **components.json 组件声明清单**（P2-L11 首批，
//! 规范见该目录 README）。两种位置：包级 `<包>/components.json`（该包自有
//! tool/unit/memory）与根级 `<根>/components.json`（跨包共享——模型路由档案
//! `route.*`）。`af-dsl::registry` 只读 `manifest.json`，对本文件零感知
//! （根级文件是非目录条目，目录扫描天然跳过）——两套清单互不耦合。
//!
//! 装载纪律（承 af-dsl `registry.rs` 先例）：
//! - **fail-loud 首错即停**：清单是静态资产，装载错误（读失败/JSON 非法/
//!   静态校验不过/跨文件重复引用）全部带路径点名，不静默降级；
//! - **确定性**：目录条目按路径排序处理；声明集 BTree；`registry_digest` =
//!   SHA-256(全量声明按解析后引用字典序的规范 JSON 数组)——同一包根两次
//!   装载 digest 逐位相同；
//! - **静态校验**（装载即拒）：`name`/`version` 非空且不含 `'@'`（组合引用
//!   `name@version` 的分隔符纪律，同 manifest 规范）；tool 类必填
//!   `effect_class`（ADR-010 effect_class 回填的唯一来源）；model_route 类
//!   必填 `endpoint` 且过 af-harness 同款端点纪律（http(s) 前缀、无
//!   userinfo、`key_ref` 引用名非明文、不以 `sk-` 开头）；非 tool 类不得带
//!   `effect_class`、非 model_route 类不得带 `endpoint`（类别×字段正交）。
//!
//! `EndpointSpec` 为 af-harness `spec.rs:16-95` 的**字段级镜像**（协议/档位/
//! 认证三枚举 + 十字段原样）——G2a IR 自持有纪律的延续：不引 af-harness
//! 依赖（其拖 harness 运行面），以逐字段注记维持可审计对应；端点校验规则
//! 同源复制 `spec.rs:228-236`（`validate_endpoint`——双实现不可避免，此处
//! 登记，见 RESULT-G2b 偏差表）。

use crate::ir::EffectClass;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

// ============================================================
// 组件类别与声明
// ============================================================

/// 组件类别（落地契约 §2.2 五类 + `uses.unit` 判定单元）。DSL `uses` 四面
/// （model/tools/unit/memory）各对一类；loop_policy / context_compaction 是
/// 契约 §2.2 的另外两类，v0.1 的 `uses` 无声明面（保留装载能力）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentClass {
    ModelRoute,
    LoopPolicy,
    Memory,
    Tool,
    ContextCompaction,
    Unit,
}

/// 组件声明（components.json 单条；全文 `deny_unknown_fields`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentDecl {
    /// 组件类别。
    pub class: ComponentClass,
    /// 组件名 = DSL `uses.*` 引用串去 `@版本` 段（如 `route.main`、
    /// `pkg.sweng.locate`）。
    pub name: String,
    /// 本声明版本（组合引用 `name@version`）。
    pub version: String,
    /// 副作用类（tool 类必填——ADR-010 effect_class 回填源）。
    #[serde(default)]
    pub effect_class: Option<EffectClass>,
    /// 实现定位串（可选）。
    #[serde(default)]
    pub impl_ref: Option<String>,
    /// 端点（model_route 类必填——af-harness `EndpointSpec` 字段级镜像）。
    #[serde(default)]
    pub endpoint: Option<EndpointSpec>,
}

/// components.json 文件正本结构。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentsFile {
    pub components: Vec<ComponentDecl>,
}

/// 端点镜像：af-harness `spec.rs::EndpointSpec`（`spec.rs:57-95`）字段级
/// 对应——字段名/枚举值/Option 语义逐字相同，序列化面同形。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointSpec {
    pub protocol: Protocol,
    pub base_url: String,
    pub model: String,
    pub key_ref: String,
    pub reasoning_tier: ReasoningTier,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub auth: Option<AuthStyle>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature_milli: Option<u32>,
    #[serde(default)]
    pub top_p_milli: Option<u32>,
}

/// 协议（af-harness `Protocol` 镜像）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    #[serde(rename = "openai-chat-completions")]
    OpenaiChatCompletions,
    #[serde(rename = "anthropic-messages")]
    AnthropicMessages,
}

/// 推理档位（af-harness `ReasoningTier` 镜像）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningTier {
    Off,
    Low,
    Medium,
    High,
}

/// 认证形态（af-harness `AuthStyle` 镜像）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthStyle {
    Bearer,
    XApiKey,
}

// ============================================================
// 装载错误
// ============================================================

/// 组件清单装载错误（fail-loud 首错即停——静态资产不走一次收齐范式，
/// 承 af-dsl `RegistryError` 纪律）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentRegistryError {
    /// 包根目录不存在。
    RootMissing(PathBuf),
    /// 包根存在但不是目录。
    RootNotDir(PathBuf),
    /// components.json 读失败。
    ReadFailed { path: PathBuf, cause: String },
    /// JSON 解析或静态校验失败（inline 解析无路径 → None）。
    Invalid { path: Option<PathBuf>, message: String },
    /// 同一 `name@version` 被多份清单声明（first = 首声者路径）。
    DuplicateComponentRef { reference: String, first: PathBuf, second: PathBuf },
}

impl fmt::Display for ComponentRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RootMissing(p) => write!(f, "包根目录不存在：{}", p.display()),
            Self::RootNotDir(p) => write!(f, "包根不是目录：{}", p.display()),
            Self::ReadFailed { path, cause } => {
                write!(f, "components.json 读取失败 {}: {cause}", path.display())
            }
            Self::Invalid { path, message } => match path {
                Some(p) => write!(f, "components.json 非法 {}: {message}", p.display()),
                None => write!(f, "components.json 非法：{message}"),
            },
            Self::DuplicateComponentRef { reference, first, second } => write!(
                f,
                "组件引用 {reference:?} 重复声明：{} 与 {}",
                first.display(),
                second.display()
            ),
        }
    }
}

impl std::error::Error for ComponentRegistryError {}

// ============================================================
// 注册表
// ============================================================

/// 装载好的组件注册表：`name → version → 声明`（BTree 双层，解析与 digest
/// 全确定序）。
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentRegistry {
    declarations: BTreeMap<String, BTreeMap<String, ComponentDecl>>,
    /// 全量声明的 SHA-256（§2.2 registry_digest——计划对象携带，执行面装载
    /// 时核对，E9004 面；编译期只产出不消费）。
    digest: String,
}

impl ComponentRegistry {
    /// 空注册表（无 --registry 时的绑定面：一切带引用的文档将被 E9001 拒，
    /// fail-closed——与 af-dsl `declared_graphs` 默认空集同语义）。
    pub fn empty() -> Self {
        Self { declarations: BTreeMap::new(), digest: crate::lower::digest_of(b"[]") }
    }

    /// 扫描包根：根级 + 全部包目录的 components.json（规则见模块注释）。
    pub fn load(root: impl AsRef<Path>) -> Result<Self, ComponentRegistryError> {
        let root = root.as_ref();
        if !root.exists() {
            return Err(ComponentRegistryError::RootMissing(root.to_path_buf()));
        }
        if !root.is_dir() {
            return Err(ComponentRegistryError::RootNotDir(root.to_path_buf()));
        }
        // 装载文件集（确定序）：根级共享清单在前，包目录按路径序在后。
        let mut files: Vec<PathBuf> = Vec::new();
        let root_file = root.join("components.json");
        if root_file.is_file() {
            files.push(root_file);
        }
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
            .map_err(|e| ComponentRegistryError::ReadFailed {
                path: root.to_path_buf(),
                cause: e.to_string(),
            })?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            if dir
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'))
            {
                continue; // 隐藏目录不是包
            }
            let f = dir.join("components.json");
            if f.is_file() {
                files.push(f); // 无 components.json 的包跳过（组件清单与图清单解耦）
            }
        }

        let mut declarations: BTreeMap<String, BTreeMap<String, ComponentDecl>> = BTreeMap::new();
        let mut first_seen: BTreeMap<String, PathBuf> = BTreeMap::new();
        for path in files {
            let src =
                std::fs::read_to_string(&path).map_err(|e| ComponentRegistryError::ReadFailed {
                    path: path.clone(),
                    cause: e.to_string(),
                })?;
            let file: ComponentsFile =
                serde_json::from_str(&src).map_err(|e| ComponentRegistryError::Invalid {
                    path: Some(path.clone()),
                    message: e.to_string(),
                })?;
            for decl in file.components {
                validate_decl(&decl).map_err(|m| ComponentRegistryError::Invalid {
                    path: Some(path.clone()),
                    message: m,
                })?;
                let r = format!("{}@{}", decl.name, decl.version);
                if let Some(first) = first_seen.get(&r) {
                    return Err(ComponentRegistryError::DuplicateComponentRef {
                        reference: r,
                        first: first.clone(),
                        second: path.clone(),
                    });
                }
                first_seen.insert(r, path.clone());
                declarations
                    .entry(decl.name.clone())
                    .or_default()
                    .insert(decl.version.clone(), decl);
            }
        }
        let digest = Self::compute_digest(&declarations);
        Ok(Self { declarations, digest })
    }

    /// `registry_digest`：全量声明按 `(name, version)` 字典序（BTreeMap 迭代
    /// 序）的规范序列化取 SHA-256——同包根两次装载逐位相同。
    fn compute_digest(declarations: &BTreeMap<String, BTreeMap<String, ComponentDecl>>) -> String {
        let all: Vec<&ComponentDecl> = declarations.values().flat_map(BTreeMap::values).collect();
        // serde_json 对本类型理论不可达失败（内部不变量；装载面已收全部错误源）
        let canonical =
            serde_json::to_string(&all).expect("组件声明规范序列化失败（内部不变量）");
        crate::lower::digest_of(canonical.as_bytes())
    }

    /// `registry_digest`（64 位小写十六进制）。
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// 按组件名取版本表（版本字典序）。
    pub fn versions_of(&self, name: &str) -> Option<&BTreeMap<String, ComponentDecl>> {
        self.declarations.get(name)
    }

    /// 声明总数（name×version 计）。
    pub fn component_count(&self) -> usize {
        self.declarations.values().map(BTreeMap::len).sum()
    }
}

// ============================================================
// 静态校验（装载即拒）
// ============================================================

/// 单条声明校验：非空/无 `'@'`；类别×字段正交（tool↔effect_class、
/// model_route↔endpoint）；端点纪律（af-harness `validate_endpoint` 同源）。
fn validate_decl(d: &ComponentDecl) -> Result<(), String> {
    if d.name.is_empty() || d.name.contains('@') {
        return Err(format!("name {:?} 非空必填且不得含 '@'（组合引用保留分隔符）", d.name));
    }
    if d.version.is_empty() || d.version.contains('@') {
        return Err(format!(
            "name {:?} 的 version {:?} 非空必填且不得含 '@'",
            d.name, d.version
        ));
    }
    match d.class {
        ComponentClass::Tool => {
            if d.effect_class.is_none() {
                return Err(format!(
                    "tool 组件 {:?} 必填 effect_class（pure|persist|external_once——ADR-010 回填源）",
                    d.name
                ));
            }
            if d.endpoint.is_some() {
                return Err(format!("tool 组件 {:?} 不得携带 endpoint（类别×字段正交）", d.name));
            }
        }
        ComponentClass::ModelRoute => {
            let Some(endpoint) = &d.endpoint else {
                return Err(format!(
                    "model_route 组件 {:?} 必填 endpoint（af-harness EndpointSpec 同形）",
                    d.name
                ));
            };
            validate_endpoint(endpoint)
                .map_err(|m| format!("model_route 组件 {:?} 端点非法：{m}", d.name))?;
            if d.effect_class.is_some() {
                return Err(format!(
                    "model_route 组件 {:?} 不得携带 effect_class（类别×字段正交）",
                    d.name
                ));
            }
        }
        _ => {
            if d.effect_class.is_some() || d.endpoint.is_some() {
                return Err(format!(
                    "组件 {:?}（{:?}）不得携带 effect_class/endpoint（类别×字段正交）",
                    d.name, d.class
                ));
            }
        }
    }
    Ok(())
}

/// 端点纪律（af-harness `spec.rs validate_endpoint` 同源复制——模块注释的
/// 双实现登记处）：http(s) 前缀、无 userinfo/空白、model 非空无空白、
/// key_ref 引用名字符集且非 `sk-` 明文前缀。
fn validate_endpoint(e: &EndpointSpec) -> Result<(), String> {
    let base = e.base_url.as_str();
    if !(base.starts_with("https://") || base.starts_with("http://")) {
        return Err("base_url 必须以 http:// 或 https:// 开头".into());
    }
    if base.contains('@') || base.chars().any(char::is_whitespace) {
        return Err("base_url 不允许用户信息或空白，密钥不能写进 URL".into());
    }
    if e.model.is_empty() || e.model.chars().any(char::is_whitespace) {
        return Err("model 不能为空或含空白".into());
    }
    let ok_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-');
    if e.key_ref.is_empty() || !e.key_ref.chars().all(ok_char) || e.key_ref.starts_with("sk-") {
        return Err("key_ref 只存引用名（字母数字和 _.:-），不存明文密钥，也不以 sk- 开头".into());
    }
    Ok(())
}
