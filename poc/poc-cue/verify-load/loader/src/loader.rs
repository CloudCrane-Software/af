//! 装载管线：信封/版本门禁 → serde 结构解析 → 语义校验（引用完整性/值域/唯一性）。
//!
//! 版本策略（见 REPORT ④）：schemaVersion 是信封层非负整数（当前支持 1），
//! 在解析 payload 之前检查；未知大版本一律拒绝（E0001），不做兼容尝试。

use crate::error::LoadError;
use crate::model::*;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

/// 当前支持的 schema 大版本（唯一支持值；不支持 → E0001）
pub const SUPPORTED_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone)]
pub struct LoadedBundle {
    pub schema_version: u64,
    pub line: Line,
    pub rules: Vec<Rule>,
}

/// 入口：装载一份 bundle JSON 文本
pub fn load_str(s: &str) -> Result<LoadedBundle, LoadError> {
    let v: Value = serde_json::from_str(s)
        .map_err(|e| LoadError::TypeMismatch { path: "/".into(), detail: format!("非法 JSON：{e}") })?;
    load_value(v)
}

pub fn load_value(v: Value) -> Result<LoadedBundle, LoadError> {
    // ---- 阶段 0：信封/版本门禁（payload 解析之前）----
    let obj = match v.as_object() {
        Some(o) => o,
        None => return Err(LoadError::TypeMismatch { path: "/".into(), detail: "顶层必须是 JSON object".into() }),
    };
    for k in obj.keys() {
        if !matches!(k.as_str(), "schemaVersion" | "line" | "rules") {
            return Err(LoadError::EnvelopeUnknownField { field: k.clone() });
        }
    }
    let sv = match obj.get("schemaVersion") {
        Some(Value::Number(n)) if n.is_u64() => n.as_u64().unwrap(),
        Some(other) => {
            return Err(LoadError::SchemaVersionMalformed {
                path: "/schemaVersion".into(),
                detail: format!("必须是非负整数，得到 {other}"),
            })
        }
        None => return Err(LoadError::SchemaVersionMalformed { path: "/schemaVersion".into(), detail: "字段缺失".into() }),
    };
    if sv != SUPPORTED_SCHEMA_VERSION {
        return Err(LoadError::SchemaVersionUnsupported { got: sv, supported: SUPPORTED_SCHEMA_VERSION });
    }

    // ---- 阶段 1：结构解析（serde，closed）----
    let line_val = match obj.get("line") {
        Some(x) => x.clone(),
        None => return Err(LoadError::MissingField { path: "/".into(), field: "line".into() }),
    };
    let rules_val = match obj.get("rules") {
        Some(x) => x.clone(),
        None => return Err(LoadError::MissingField { path: "/".into(), field: "rules".into() }),
    };
    let line: Line = from_value_path(line_val, "/line")?;
    let raw_rules: Vec<RawRule> = from_value_path(rules_val, "/rules")?;
    let mut rules = Vec::with_capacity(raw_rules.len());
    for (i, raw) in raw_rules.into_iter().enumerate() {
        rules.push(dispatch_rule(raw, &format!("/rules/{i}"))?);
    }

    let mut bundle = LoadedBundle { schema_version: sv, line, rules };
    // ---- 阶段 2：语义校验 ----
    semantic_check(&mut bundle)?;
    Ok(bundle)
}

fn from_value_path<T: for<'de> Deserialize<'de>>(v: Value, path: &str) -> Result<T, LoadError> {
    serde_json::from_value::<T>(v).map_err(|e| map_serde_err(path, e))
}

/// serde_json::Error → 结构化错误码。
/// 已知坑：from_value 的错误不带 JSON 路径，path 由调用方给（包含层）；
/// 缺字段在 serde_json 里归类是 Data 不是独立 Category，只能按报文 sniff。
fn map_serde_err(path: &str, e: serde_json::Error) -> LoadError {
    let s = e.to_string();
    if let Some(field) = extract_backtick(&s, "unknown field ") {
        return LoadError::UnknownField { path: path.into(), field };
    }
    if let Some(variant) = extract_backtick(&s, "unknown variant ") {
        return LoadError::UnknownVariant { path: path.into(), field: "枚举字段".into(), got: variant };
    }
    if let Some(field) = extract_backtick(&s, "missing field ") {
        return LoadError::MissingField { path: path.into(), field };
    }
    LoadError::TypeMismatch { path: path.into(), detail: s }
}

/// 取 "marker `value`" 形态报文里反引号包裹的值（marker 后第一个反引号是开头，取到第二个）
fn extract_backtick(s: &str, after: &str) -> Option<String> {
    let i = s.find(after)? + after.len();
    let rest = &s[i..];
    let j1 = rest.find('`')?;
    let rest2 = &rest[j1 + 1..];
    let j2 = rest2.find('`')?;
    Some(rest2[..j2].to_string())
}

/// kind → per-kind params 分派（结构层收不进 internally-tagged enum 的原因见 model.rs）
fn dispatch_rule(raw: RawRule, path: &str) -> Result<Rule, LoadError> {
    // 先拆出所有权，避免 params 移动后 &raw 借用冲突
    let RawRule { id, kind: kind_str, desc, enabled, params } = raw;
    let kind = match RuleKind::from_str(&kind_str) {
        Some(k) => k,
        None => return Err(LoadError::UnknownKind { path: path.into(), rule_id: id.clone(), got: kind_str }),
    };
    if !params.is_object() {
        return Err(LoadError::TypeMismatch { path: format!("{path}/params"), detail: "params 必须是 object".into() });
    }
    let ppath = format!("{path}/params");
    macro_rules! try_params {
        ($t:ty) => {
            match serde_json::from_value::<$t>(params.clone()) {
                Ok(p) => p,
                Err(e) => return Err(mismatch_or_unknown(&ppath, &id, kind, e)),
            }
        };
    }
    let rule_params = match kind {
        RuleKind::Threshold => RuleParams::Threshold(try_params!(ThresholdParams)),
        RuleKind::Ratio => RuleParams::Ratio(try_params!(RatioParams)),
        RuleKind::TimeWindow => RuleParams::TimeWindow(try_params!(TimeWindowParams)),
        RuleKind::Dwell => RuleParams::Dwell(try_params!(DwellParams)),
        RuleKind::Count => RuleParams::Count(try_params!(CountParams)),
        RuleKind::Fingerprint => RuleParams::Fingerprint(try_params!(FingerprintParams)),
        RuleKind::Escalation => RuleParams::Escalation(try_params!(EscalationParams)),
        RuleKind::Combo => RuleParams::Combo(try_params!(ComboParams)),
    };
    Ok(Rule { id, kind, desc, enabled, params: rule_params })
}

/// params 解析失败时的细分：字段属于其它 kind → E1007（kind/params 不匹配）；
/// 纯未知字段 → E1001；其余按 map_serde_err。
fn mismatch_or_unknown(ppath: &str, rule_id: &str, kind: RuleKind, e: serde_json::Error) -> LoadError {
    let s = e.to_string();
    if let Some(field) = extract_backtick(&s, "unknown field ") {
        if all_known_param_fields().contains(&field.as_str()) {
            return LoadError::KindParamsMismatch {
                path: ppath.into(),
                rule_id: rule_id.into(),
                kind: kind.as_str().into(),
                field,
            };
        }
        return LoadError::UnknownField { path: ppath.into(), field };
    }
    map_serde_err(ppath, e)
}

/// 所有 kind 的合法 params 字段全集（用于 E1007 判定）
fn all_known_param_fields() -> &'static [&'static str] {
    &[
        "metric", "op", "value", "numerator", "denominator", "warn", "critical", "start", "end",
        "samples", "atLeast", "within", "fields", "ttl", "chain", "allOf",
    ]
}

// ============ 阶段 2：语义校验 ============

fn semantic_check(b: &mut LoadedBundle) -> Result<(), LoadError> {
    // 1. id 唯一性（规则 + 安灯）
    check_unique(&b.rules.iter().map(|r| r.id.clone()).collect::<Vec<_>>(), "rule")?;
    check_unique(&b.line.andon_rules.iter().map(|r| r.id.clone()).collect::<Vec<_>>(), "andon")?;

    let rule_ids: HashSet<&str> = b.rules.iter().map(|r| r.id.as_str()).collect();
    let eval_fn_ids: HashSet<&str> = b.line.eval_functions.keys().map(|s| s.as_str()).collect();
    let field_ids: HashSet<&str> = b.line.artifact_schema.fields.keys().map(|s| s.as_str()).collect();

    // 2. 规则侧语义（引用完整性 + 值域 + 非空）
    for (i, r) in b.rules.iter().enumerate() {
        let p = format!("/rules/{i}/params");
        match &r.params {
            RuleParams::Combo(c) => {
                if c.all_of.is_empty() {
                    return Err(LoadError::EmptyCollection { path: p, field: "allOf".into() });
                }
                for t in &c.all_of {
                    if !rule_ids.contains(t.as_str()) {
                        return Err(LoadError::DanglingReference { path: p.clone(), target_kind: "rule", target: t.clone() });
                    }
                }
            }
            RuleParams::Ratio(r) => {
                if !(r.critical > r.warn) {
                    return Err(LoadError::ValueOutOfRange {
                        path: p, detail: format!("critical({}) 必须严于 warn({})", r.critical, r.warn),
                    });
                }
            }
            RuleParams::Escalation(e) => {
                if e.chain.is_empty() {
                    return Err(LoadError::EmptyCollection { path: p, field: "chain".into() });
                }
                for (j, st) in e.chain.iter().enumerate() {
                    if st.notify.is_empty() {
                        return Err(LoadError::EmptyCollection { path: format!("{p}/chain/{j}"), field: "notify".into() });
                    }
                }
                for j in 1..e.chain.len() {
                    if e.chain[j].after_ms <= e.chain[j - 1].after_ms {
                        return Err(LoadError::ValueOutOfRange {
                            path: format!("{p}/chain/{j}/afterMs"),
                            detail: format!("升级链 afterMs 必须严格递增：{} <= {}", e.chain[j].after_ms, e.chain[j - 1].after_ms),
                        });
                    }
                }
            }
            RuleParams::Fingerprint(f) => {
                if f.fields.is_empty() {
                    return Err(LoadError::EmptyCollection { path: p, field: "fields".into() });
                }
            }
            RuleParams::Dwell(d) => {
                if matches!(d.op, CmpOp::Lt | CmpOp::Le) {
                    return Err(LoadError::ValueOutOfRange {
                        path: format!("{p}/op"),
                        detail: format!("kind dwell 的 op 只允许 >/>=，得到 `{}`", d.op.as_str()),
                    });
                }
            }
            RuleParams::Count(c) => {
                if matches!(c.op, CmpOp::Lt | CmpOp::Le) {
                    return Err(LoadError::ValueOutOfRange {
                        path: format!("{p}/op"),
                        detail: format!("kind count 的 op 只允许 >/>=，得到 `{}`", c.op.as_str()),
                    });
                }
            }
            _ => {}
        }
    }
    // 3. 安灯侧：notify 非空 + trigger 引用完整性
    for (i, a) in b.line.andon_rules.iter().enumerate() {
        let p = format!("/line/andonRules/{i}");
        if a.notify.is_empty() {
            return Err(LoadError::EmptyCollection { path: p, field: "notify".into() });
        }
        if !eval_fn_ids.contains(a.trigger.as_str()) {
            return Err(LoadError::DanglingReference { path: format!("{p}/trigger"), target_kind: "evalFunction", target: a.trigger.clone() });
        }
    }
    // 4. 工件 schema：required 引用完整性 + enum 字段必须带非空取值表
    for (i, req) in b.line.artifact_schema.required.iter().enumerate() {
        if !field_ids.contains(req.as_str()) {
            return Err(LoadError::DanglingReference {
                path: format!("/line/artifactSchema/required/{i}"),
                target_kind: "artifactField",
                target: req.clone(),
            });
        }
    }
    for (name, f) in b.line.artifact_schema.fields.iter() {
        if f.field_type == FieldType::Enum && f.enum_values.is_empty() {
            return Err(LoadError::MissingField {
                path: format!("/line/artifactSchema/fields/{name}"),
                field: "enumValues".into(),
            });
        }
    }
    Ok(())
}

fn check_unique(ids: &[String], entity: &'static str) -> Result<(), LoadError> {
    let mut seen: BTreeMap<&str, ()> = BTreeMap::new();
    for id in ids {
        if seen.insert(id.as_str(), ()).is_some() {
            return Err(LoadError::DuplicateId { entity, id: id.clone() });
        }
    }
    Ok(())
}
