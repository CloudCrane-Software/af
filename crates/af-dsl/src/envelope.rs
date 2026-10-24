//! 拒绝式版本信封（E0001-E0004）。承 af-harness `decode_spec` 与 af-dspy-ir
//! `Program.version` 双先例：version 非 1 → 整份拒绝、不解析 body。
//!
//! 检查顺序固定：先 version（缺/非整数/非 1 → 单违例短路返回，连其余顶层键
//! 都不扫——「整份拒绝」语义），version 合法再收齐全部未知键（E0004 逐键一条）。

use crate::error::{ErrorCode, LoadError, Report};

/// 信封合法的顶层键：version + 六个集合段（七段 = version/meta/types/stores/
/// nodes/edges/policies，见提案 B §1.1）。
pub const BODY_KEYS: [&str; 6] = ["meta", "types", "stores", "nodes", "edges", "policies"];

/// 信封检查。
pub fn check(value: &serde_yaml::Value) -> Result<(), LoadError> {
    let mut report = Report::default();
    let mapping = match value {
        serde_yaml::Value::Mapping(m) => m,
        other => {
            report.push(
                ErrorCode::MissingVersion,
                "envelope",
                format!("文档根不是映射（实为 {}），无 version 键", kind_name(other)),
            );
            return Err(LoadError::Envelope(report));
        }
    };

    // ① version：缺 → E0001；非整数 → E0002；非 1 → E0003。任一命中即整份拒绝。
    match mapping.get(serde_yaml::Value::String("version".into())) {
        None => {
            report.push(ErrorCode::MissingVersion, "envelope", "缺 version 键");
            return Err(LoadError::Envelope(report));
        }
        Some(serde_yaml::Value::Number(n)) => match as_integer(n) {
            Some(1) => {}
            Some(v) => {
                report.push(
                    ErrorCode::VersionNotOne,
                    "envelope",
                    format!("version = {v}，只支持 1"),
                );
                return Err(LoadError::Envelope(report));
            }
            None => {
                report.push(
                    ErrorCode::VersionNotInteger,
                    "envelope",
                    format!("version 非整数：{n}"),
                );
                return Err(LoadError::Envelope(report));
            }
        },
        Some(other) => {
            report.push(
                ErrorCode::VersionNotInteger,
                "envelope",
                format!("version 非整数：{}", kind_name(other)),
            );
            return Err(LoadError::Envelope(report));
        }
    }

    // ② version 合法 → 收齐全部未知顶层键（E0004 逐键一条）。
    for key in mapping.keys() {
        let name = match key {
            serde_yaml::Value::String(s) => s.clone(),
            other => format!("非字符串键 {}", kind_name(other)),
        };
        if name != "version" && !BODY_KEYS.contains(&name.as_str()) {
            report.push(
                ErrorCode::EnvelopeUnknownKey,
                "envelope",
                format!("信封未知键 {name:?}（合法：version + 六段 {BODY_KEYS:?}）"),
            );
        }
    }
    if report.is_ok() {
        Ok(())
    } else {
        Err(LoadError::Envelope(report))
    }
}

/// YAML 数 → 整数值（i64/u64 才算整数；f64 不算）。
fn as_integer(n: &serde_yaml::Number) -> Option<i64> {
    if let Some(i) = n.as_i64() {
        Some(i)
    } else {
        n.as_u64().and_then(|u| i64::try_from(u).ok())
    }
}

fn kind_name(v: &serde_yaml::Value) -> &'static str {
    match v {
        serde_yaml::Value::Null => "null",
        serde_yaml::Value::Bool(_) => "布尔值",
        serde_yaml::Value::Number(_) => "数字",
        serde_yaml::Value::String(_) => "字符串",
        serde_yaml::Value::Sequence(_) => "序列",
        serde_yaml::Value::Mapping(_) => "映射",
        serde_yaml::Value::Tagged(_) => "带标签值",
    }
}
