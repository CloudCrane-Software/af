//! graph_version 传递依赖锁(v0.3 方案 §开放风险登记 #6 收口)。
//!
//! # 施工前的断链(读数见 `poc/poc-graph-version-digest/`)
//! `GraphVersion` 是不透明字符串(调用方自报),工单面唯一的"版本锁"
//! 是字符串相等性——依赖方(evaluator/adapter/template)的版本没有
//! 任何进路进入它:同图同串、依赖闭包版本不同 ⇒ 两个"世界"的工单
//! 逐字段全等,零检出。编译面仅有的 digest 是 af-compiler 的
//! `registry_digest`,它是**全量声明集合**锁:注册表多版本共存时,
//! 图引用取向 `@1 → @2` 对它不可见(漏检),闭包外组件变化又会让它
//! 变(过敏);且组件声明(`ComponentDecl`)没有依赖字段,组件间
//! 传递依赖链在声明面无口子。传递链断在两头:**工单面**版本串不含
//! 依赖版本,**声明面**不存在"A 依赖 B"的数据。
//!
//! # 锁语义(本模块)
//! 把"图 + 依赖闭包"钉进一个域分离 SHA-256:
//! `graph_deps_digest = sha256(GRAPH_DEPS_DOMAIN + 长度前缀材料)`,
//! 材料 = graph_version 串 + 闭包全体成员 `(kind, name, version)`
//! 按字典序的规范序列。由此:
//! - **确定性**:同图同依赖版本 ⇒ 材料逐字节相同 ⇒ digest 相同
//!   (可复算;材料格式见 [`digest_material`],无 serde 序列化参与);
//! - **传递闭包**:`A 依赖 B, B 依赖 C` ⇒ C 的版本变化传导到 A 的
//!   digest(闭包解析到不动点,不是一层);
//! - **契约面**:闭包内任何版本/类别变化 ⇒ digest 变;闭包外变化
//!   ⇒ digest 不变(与全量 registry_digest 的差异位)。
//!
//! 依赖表 [`DependencyTable`] 是组件仓声明面的**投影**(谁依赖谁由
//! 调用方装载);闭包解析 fail-loud:引用表外组件 = 错误,不给 digest。
//! 循环依赖(A→B→A)按集合不动点收敛,不构成错误。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::error::GraphDepsError;
use crate::hash::sha256_hex;
use crate::order::GraphVersion;

/// 依赖锁 digest 的域分离前缀(材料首字节,对齐 [`crate::identity::STABLE_ID_DOMAIN`] 范式)。
pub const GRAPH_DEPS_DOMAIN: &[u8] = b"af-workorder/graph-deps/v1\0";

/// 图依赖方的类别口径(风险 #6 的三方:评估器/环境适配器/提示词模板)。
///
/// 类别是契约的一部分:同名同版本、不同类别 ⇒ 不同闭包成员 ⇒ 不同 digest。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepKind {
    Evaluator,
    Adapter,
    Template,
}

impl DepKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Evaluator => "evaluator",
            Self::Adapter => "adapter",
            Self::Template => "template",
        }
    }
}

/// 一条依赖引用:`kind + name@version`。
///
/// `name`/`version` 非空且不含 `'@'`(组合引用分隔符纪律,同 af-compiler
/// `components.rs` 的 `validate_decl`)。构造即校验,fail-loud。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DepRef {
    pub kind: DepKind,
    pub name: String,
    pub version: String,
}

impl DepRef {
    pub fn new(
        kind: DepKind,
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> Result<Self, GraphDepsError> {
        let name = name.into();
        let version = version.into();
        check_ref_part(&name, "name")?;
        check_ref_part(&version, "version")?;
        Ok(Self { kind, name, version })
    }

    /// 组合引用串 `name@version`。
    pub fn reference(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

fn check_ref_part(value: &str, what: &str) -> Result<(), GraphDepsError> {
    if value.is_empty() || value.contains('@') {
        return Err(GraphDepsError::InvalidReference {
            part: what.to_string(),
            value: value.to_string(),
        });
    }
    Ok(())
}

/// 闭包成员:到达过的 `(name, version, kind)` 三元组。字段声明序即
/// Ord 序(名称主序,同名同版本相邻,类别差异可见);digest 材料按此序
/// 展开,跨进程确定。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClosureMember {
    pub name: String,
    pub version: String,
    pub kind: DepKind,
}

impl ClosureMember {
    /// 组合引用串 `kind:name@version`(点名类别,人读用)。
    pub fn display_reference(&self) -> String {
        format!("{}:{}@{}", self.kind.as_str(), self.name, self.version)
    }
}

/// 依赖表:组件仓声明面的投影,`name → version → 直接依赖清单`。
///
/// 装载纪律对齐 af-compiler `ComponentRegistry`:同一 `name@version`
/// 二次声明 = 错误(不覆盖、不静默合并);条目 [`BTreeMap`] 双层,
/// 遍历序确定。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DependencyTable {
    entries: BTreeMap<String, BTreeMap<String, Vec<DepRef>>>,
}

impl DependencyTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// 声明一条组件:`name@version` 依赖 `requires`。
    ///
    /// 重复声明同一 `name@version` 拒绝(fail-loud,不覆盖)。
    pub fn declare(
        &mut self,
        name: &str,
        version: &str,
        requires: Vec<DepRef>,
    ) -> Result<(), GraphDepsError> {
        check_ref_part(name, "name")?;
        check_ref_part(version, "version")?;
        let versions = self.entries.entry(name.to_string()).or_default();
        if versions.contains_key(version) {
            return Err(GraphDepsError::DuplicateDeclaration {
                reference: format!("{name}@{version}"),
            });
        }
        versions.insert(version.to_string(), requires);
        Ok(())
    }

    /// 查 `name@version` 的直接依赖清单;未声明返回 `None`。
    pub fn requires_of(&self, name: &str, version: &str) -> Option<&[DepRef]> {
        self.entries
            .get(name)
            .and_then(|versions| versions.get(version))
            .map(Vec::as_slice)
    }

    /// 声明总数(name×version 计)。
    pub fn len(&self) -> usize {
        self.entries.values().map(BTreeMap::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 从 `roots` 出发解析传递闭包(不动点集合,不是一层)。
///
/// 每个到达的引用必须在表中有声明,否则 [`GraphDepsError::UnknownDependency`]
/// ——不给"半个闭包"的 digest。循环依赖按集合收敛,不报错。
pub fn dependency_closure(
    roots: &[DepRef],
    table: &DependencyTable,
) -> Result<BTreeSet<ClosureMember>, GraphDepsError> {
    let mut closure: BTreeSet<ClosureMember> = BTreeSet::new();
    let mut pending: Vec<ClosureMember> = Vec::new();
    for root in roots {
        pending.push(ClosureMember {
            kind: root.kind,
            name: root.name.clone(),
            version: root.version.clone(),
        });
    }
    while let Some(member) = pending.pop() {
        if !closure.insert(member.clone()) {
            continue; // 已到达:闭包是集合,循环在此收敛
        }
        let requires = table
            .requires_of(&member.name, &member.version)
            .ok_or_else(|| GraphDepsError::UnknownDependency {
                reference: member.display_reference(),
            })?;
        for dep in requires {
            pending.push(ClosureMember {
                kind: dep.kind,
                name: dep.name.clone(),
                version: dep.version.clone(),
            });
        }
    }
    Ok(closure)
}

/// 依赖锁 digest(64 位小写十六进制)。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GraphDepsDigest(String);

impl GraphDepsDigest {
    /// 由已算好的十六进制串构造(线格式读入位);长度与字符集校验,
    /// 拒绝空串/大写/非 hex(同 [`crate::order::WorkArtifact`] 的摘要纪律)。
    pub fn parse(raw: impl Into<String>) -> Result<Self, GraphDepsError> {
        let raw = raw.into();
        let valid = raw.len() == crate::hash::SHA256_HEX_LEN
            && raw
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !valid {
            return Err(GraphDepsError::InvalidDigest(raw));
        }
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for GraphDepsDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// digest 材料(明文可复算):域前缀 + 长度前缀逐段展开。
///
/// 格式(对齐 [`crate::identity::decision_payload`] 的 `|长度|值` 范式,
/// 字段值里的分隔符不改写边界):
/// `domain\0|{gl}|{graph_version}|{count}` 然后
/// 每个闭包成员(字典序)接 `|{kl}|{kind}|{nl}|{name}|{vl}|{version}`。
pub fn digest_material(graph_version: &GraphVersion, closure: &BTreeSet<ClosureMember>) -> String {
    let gv = graph_version.as_str();
    let mut out = format!(
        "{}|{}|{gv}|{}",
        String::from_utf8_lossy(GRAPH_DEPS_DOMAIN),
        gv.len(),
        closure.len()
    );
    for member in closure {
        let kind = member.kind.as_str();
        out.push_str(&format!(
            "|{}|{kind}|{}|{}|{}|{}",
            kind.len(),
            member.name.len(),
            member.name,
            member.version.len(),
            member.version
        ));
    }
    out
}

/// 传递依赖锁 digest:图 + 依赖闭包全量成员的域分离 SHA-256。
///
/// 同图同闭包 ⇒ 同 digest;闭包内版本/类别变化 ⇒ digest 变;
/// 闭包外变化 ⇒ 不参与材料 ⇒ digest 不变。
pub fn graph_deps_digest(
    graph_version: &GraphVersion,
    roots: &[DepRef],
    table: &DependencyTable,
) -> Result<GraphDepsDigest, GraphDepsError> {
    let closure = dependency_closure(roots, table)?;
    Ok(GraphDepsDigest(sha256_hex(
        digest_material(graph_version, &closure).as_bytes(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gv() -> GraphVersion {
        GraphVersion::new("g-3")
    }

    /// evaluator → adapter → template 三层链(风险 #6 的传递场景)。
    fn chain_table() -> DependencyTable {
        let mut t = DependencyTable::new();
        t.declare(
            "pkg.x.evaluator",
            "1",
            vec![DepRef::new(
                DepKind::Adapter,
                "pkg.x.adapter",
                "1",
            )
            .unwrap()],
        )
        .unwrap();
        t.declare(
            "pkg.x.adapter",
            "1",
            vec![DepRef::new(
                DepKind::Template,
                "pkg.x.template",
                "1",
            )
            .unwrap()],
        )
        .unwrap();
        t.declare("pkg.x.template", "1", vec![]).unwrap();
        t
    }

    fn root_evaluator(version: &str) -> DepRef {
        DepRef::new(DepKind::Evaluator, "pkg.x.evaluator", version).unwrap()
    }

    #[test]
    fn digest_material_is_explicit_and_stable() {
        let table = chain_table();
        let closure = dependency_closure(&[root_evaluator("1")], &table).unwrap();
        // 材料逐段手拼:三成员按 (name, version, kind) 字典序——
        // pkg.x.adapter < pkg.x.evaluator < pkg.x.template(名称主序)。
        let expect = concat!(
            "af-workorder/graph-deps/v1\0|3|g-3|3",
            "|7|adapter|13|pkg.x.adapter|1|1",
            "|9|evaluator|15|pkg.x.evaluator|1|1",
            "|8|template|14|pkg.x.template|1|1",
        );
        assert_eq!(digest_material(&gv(), &closure), expect);
    }

    #[test]
    fn digest_is_deterministic_and_hex() {
        let table = chain_table();
        let d1 = graph_deps_digest(&gv(), &[root_evaluator("1")], &table).unwrap();
        let d2 = graph_deps_digest(&gv(), &[root_evaluator("1")], &table).unwrap();
        assert_eq!(d1, d2);
        assert_eq!(d1.as_str().len(), crate::hash::SHA256_HEX_LEN);
        assert!(d1
            .as_str()
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        // 材料与 sha256_hex 的直接对算:digest 可在 crate 外复算。
        let closure = dependency_closure(&[root_evaluator("1")], &table).unwrap();
        assert_eq!(
            d1.as_str(),
            sha256_hex(digest_material(&gv(), &closure).as_bytes())
        );
    }

    #[test]
    fn parse_rejects_malformed_digest() {
        assert!(GraphDepsDigest::parse("0".repeat(64)).is_ok());
        assert!(GraphDepsDigest::parse("0".repeat(63)).is_err());
        assert!(GraphDepsDigest::parse(format!("{:X}", 0xabcdef).repeat(11)).is_err());
        assert!(GraphDepsDigest::parse("z".repeat(64)).is_err());
    }

    #[test]
    fn ref_and_declare_validation() {
        assert_eq!(
            DepRef::new(DepKind::Template, "a@b", "1").unwrap_err(),
            GraphDepsError::InvalidReference {
                part: "name".into(),
                value: "a@b".into()
            }
        );
        assert_eq!(
            DepRef::new(DepKind::Template, "a", "").unwrap_err(),
            GraphDepsError::InvalidReference {
                part: "version".into(),
                value: String::new()
            }
        );
        let mut t = DependencyTable::new();
        t.declare("pkg.a", "1", vec![]).unwrap();
        assert_eq!(
            t.declare("pkg.a", "1", vec![]).unwrap_err(),
            GraphDepsError::DuplicateDeclaration {
                reference: "pkg.a@1".into()
            }
        );
        assert_eq!(t.len(), 1);
        assert!(!t.is_empty());
        assert_eq!(
            t.declare("", "1", vec![]).unwrap_err(),
            GraphDepsError::InvalidReference {
                part: "name".into(),
                value: String::new()
            }
        );
    }

    #[test]
    fn closure_rejects_unreachable_and_converges_on_cycles() {
        let table = chain_table();
        // 表外版本:闭包 fail-loud,不给半个闭包。
        assert_eq!(
            dependency_closure(&[root_evaluator("2")], &table).unwrap_err(),
            GraphDepsError::UnknownDependency {
                reference: "evaluator:pkg.x.evaluator@2".into()
            }
        );
        // A→B→A 循环:集合不动点收敛。
        let mut cyclic = DependencyTable::new();
        cyclic
            .declare(
                "pkg.c.a",
                "1",
                vec![DepRef::new(DepKind::Adapter, "pkg.c.b", "1").unwrap()],
            )
            .unwrap();
        cyclic
            .declare(
                "pkg.c.b",
                "1",
                vec![DepRef::new(DepKind::Template, "pkg.c.a", "1").unwrap()],
            )
            .unwrap();
        let closure = dependency_closure(
            &[DepRef::new(DepKind::Evaluator, "pkg.c.a", "1").unwrap()],
            &cyclic,
        )
        .unwrap();
        // 3 个成员:root (pkg.c.a, evaluator) + (pkg.c.b, adapter) + 回边
        // (pkg.c.a, template)——kind 是契约的一部分,同名同版本不同 kind
        // 是两个成员;集合不动点收敛,循环不发散。
        assert_eq!(closure.len(), 3);
        // 同输入二次解析同集合(不动点确定)。
        let again = dependency_closure(
            &[DepRef::new(DepKind::Evaluator, "pkg.c.a", "1").unwrap()],
            &cyclic,
        )
        .unwrap();
        assert_eq!(closure, again);
    }
}
