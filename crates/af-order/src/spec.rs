//! 极简 OrderSpec：`order_id` / `line` / `budget` / `note`（可选）。
//!
//! 拒绝式解析（对齐 af-workorder 老载荷门禁的风格）：缺字段点名、未知字段
//! 名单列全、类型不合预检、`order_id` 必须是安全路径成分——它同时是
//! `state/orders/` 下的目录名，放行 `../` 等于放行目录穿越。

use af_workorder::LineBinding;
use serde::Deserialize;
use serde_json::{Map, Value};

/// spec 认识的字段（字典序，供未知字段名单对拍）。
pub const SPEC_FIELDS: &[&str] = &["budget", "line", "note", "order_id"];

/// line 对象认识的子字段。
const LINE_FIELDS: &[&str] = &["line_id", "line_version"];

/// 极简下单 spec。
#[derive(Debug, Clone, Deserialize)]
pub struct OrderSpec {
    /// 订单号。同时是 `state/orders/` 下的目录名，必须无路径分隔符、无空白。
    pub order_id: String,
    /// 产线绑定（release 门材料①）。
    pub line: LineBinding,
    /// 预算（u64，必须 > 0；进 BudgetVoucher.amount）。
    pub budget: u64,
    /// 备注（可选；记入凭证材料）。
    #[serde(default)]
    pub note: Option<String>,
}

/// spec 拒绝原因。Display 即"原因"，CLI 原样透给 stderr。
#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    #[error("不是合法 JSON: {0}")]
    Malformed(String),
    #[error("缺少必填字段: {0}")]
    MissingField(String),
    #[error("不认识的字段: {0}（spec 只收 order_id/line/budget/note）")]
    UnknownFields(String),
    #[error("字段 {field} 不合法: {reason}")]
    Invalid { field: &'static str, reason: String },
}

/// 解析 spec 文本：JSON → 未知字段 → 必填字段 → 类型预检 → 反序列化 → 语义校验。
pub fn parse_spec(text: &str) -> Result<OrderSpec, SpecError> {
    let value: Value =
        serde_json::from_str(text).map_err(|err| SpecError::Malformed(err.to_string()))?;
    let obj = value
        .as_object()
        .ok_or_else(|| SpecError::Malformed("spec 必须是 JSON 对象".into()))?;

    reject_unknown_fields(obj)?;

    for field in ["order_id", "line", "budget"] {
        if !obj.contains_key(field) {
            return Err(SpecError::MissingField(field.to_string()));
        }
    }
    check_order_id_type(obj)?;
    check_budget_type(obj)?;
    check_line(obj)?;
    if let Some(note) = obj.get("note") {
        if !note.is_string() {
            return Err(SpecError::Invalid {
                field: "note",
                reason: format!("必须是字符串，实际是 {note}"),
            });
        }
    }

    let spec: OrderSpec =
        serde_json::from_value(value).map_err(|err| SpecError::Malformed(err.to_string()))?;
    validate_order_id(&spec.order_id)?;
    if spec.budget == 0 {
        return Err(SpecError::Invalid {
            field: "budget",
            reason: "预算必须大于 0（u64）".into(),
        });
    }
    Ok(spec)
}

/// 未知字段全列（字典序），不静默丢弃。
fn reject_unknown_fields(obj: &Map<String, Value>) -> Result<(), SpecError> {
    let mut unknown: Vec<&str> = obj
        .keys()
        .map(String::as_str)
        .filter(|key| !SPEC_FIELDS.contains(key))
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    unknown.sort_unstable();
    Err(SpecError::UnknownFields(unknown.join(", ")))
}

fn check_order_id_type(obj: &Map<String, Value>) -> Result<(), SpecError> {
    match obj.get("order_id") {
        Some(Value::String(_)) => Ok(()),
        Some(other) => Err(SpecError::Invalid {
            field: "order_id",
            reason: format!("必须是字符串，实际是 {other}"),
        }),
        None => Ok(()), // 缺字段已由 MissingField 点名
    }
}

fn check_budget_type(obj: &Map<String, Value>) -> Result<(), SpecError> {
    match obj.get("budget") {
        Some(value) if value.is_u64() => Ok(()),
        Some(other) => Err(SpecError::Invalid {
            field: "budget",
            reason: format!("预算必须是正整数（u64），实际是 {other}"),
        }),
        None => Ok(()),
    }
}

/// line：对象、两个非空字符串子字段、无多余子字段。
fn check_line(obj: &Map<String, Value>) -> Result<(), SpecError> {
    let Some(line) = obj.get("line").and_then(Value::as_object) else {
        return Err(SpecError::Invalid {
            field: "line",
            reason: "必须是对象 {\"line_id\": …, \"line_version\": …}".into(),
        });
    };
    for key in LINE_FIELDS {
        match line.get(*key) {
            Some(Value::String(text)) if !text.trim().is_empty() => {}
            Some(other) => {
                return Err(SpecError::Invalid {
                    field: "line",
                    reason: format!("line.{key} 必须是非空字符串，实际是 {other}"),
                })
            }
            None => {
                return Err(SpecError::Invalid {
                    field: "line",
                    reason: format!("缺少 line.{key}"),
                })
            }
        }
    }
    let mut extra: Vec<&str> = line
        .keys()
        .map(String::as_str)
        .filter(|key| !LINE_FIELDS.contains(key))
        .collect();
    if !extra.is_empty() {
        extra.sort_unstable();
        return Err(SpecError::Invalid {
            field: "line",
            reason: format!("不认识的子字段 {}", extra.join(", ")),
        });
    }
    Ok(())
}

/// order_id 要当 state/orders/ 的目录名：非空、非 `.`/`..`、无路径分隔符、无空白。
/// pub(crate)（红队 W2-C39）：show 分支的位置参数同样要当目录名——此前只
/// submit 路径过此门，`show <任意串>` 可读出 state 目录外乃至任意路径下
/// 名为 ledger.jsonl 的文件（绝对路径还会替换 state 根）。
pub(crate) fn validate_order_id(order_id: &str) -> Result<(), SpecError> {
    let invalid = |reason: &str| SpecError::Invalid {
        field: "order_id",
        reason: reason.into(),
    };
    if order_id.is_empty() {
        Err(invalid("不能为空"))
    } else if order_id == "." || order_id == ".." {
        Err(invalid("不得是 . 或 ..（它要作 state/orders/ 的目录名）"))
    } else if order_id.chars().any(|c| c == '/' || c == '\\' || c == '\0') {
        Err(invalid(
            "不得包含路径分隔符（它要作 state/orders/ 的目录名）",
        ))
    } else if order_id.chars().any(char::is_whitespace) {
        Err(invalid("不得包含空白字符"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"{
        "order_id": "wo-1",
        "line": { "line_id": "line-poc", "line_version": "lv-1" },
        "budget": 42,
        "note": "首批试产"
    }"#;

    fn minimal(order_id: &str) -> String {
        format!(
            r#"{{"order_id":"{order_id}","line":{{"line_id":"l","line_version":"v"}},"budget":1}}"#
        )
    }

    /// 合法 spec（含/不含 note）解析出全部字段。
    #[test]
    fn valid_spec_parses_with_optional_note() {
        let spec = parse_spec(VALID).expect("合法 spec 应可解析");
        assert_eq!(spec.order_id, "wo-1");
        assert_eq!(spec.line.line_id, "line-poc");
        assert_eq!(spec.line.line_version, "lv-1");
        assert_eq!(spec.budget, 42);
        assert_eq!(spec.note.as_deref(), Some("首批试产"));

        let bare = parse_spec(&minimal("wo-2")).expect("无 note 的极简 spec 应可解析");
        assert_eq!(bare.note, None);
    }

    /// 缺必填字段：点名到字段名。
    #[test]
    fn missing_required_field_is_named() {
        let text = r#"{"order_id":"wo-1","line":{"line_id":"l","line_version":"v"}}"#;
        match parse_spec(text).unwrap_err() {
            SpecError::MissingField(field) => assert_eq!(field, "budget"),
            other => panic!("应点名缺 budget，实际 {other:?}"),
        }
    }

    /// 未知字段：全部列出，字典序。
    #[test]
    fn unknown_fields_are_all_listed_sorted() {
        let text = r#"{"order_id":"wo-1","line":{"line_id":"l","line_version":"v"},"budget":5,"priority":"High"}"#;
        match parse_spec(text).unwrap_err() {
            SpecError::UnknownFields(names) => assert_eq!(names, "priority"),
            other => panic!("应列出未知字段 priority，实际 {other:?}"),
        }
        let text = r#"{"zz":1,"aa":2,"order_id":"wo-1","line":{"line_id":"l","line_version":"v"},"budget":5}"#;
        match parse_spec(text).unwrap_err() {
            SpecError::UnknownFields(names) => assert_eq!(names, "aa, zz"),
            other => panic!("多个未知字段应字典序全列，实际 {other:?}"),
        }
    }

    /// order_id 要当目录名：路径不安全/空白/空串一律拒。
    /// （反斜杠检查在实现里保留；JSON 文本里 `\b` 是退格转义，不入本表。）
    #[test]
    fn path_unsafe_order_id_is_rejected() {
        for bad in ["../escape", "a/b", ".", "..", "", "has space"] {
            match parse_spec(&minimal(bad)).unwrap_err() {
                SpecError::Invalid { field, .. } => assert_eq!(field, "order_id"),
                other => panic!("order_id={bad:?} 应被拒为 Invalid(order_id)，实际 {other:?}"),
            }
        }
    }

    /// 预算必须 > 0 且是 u64：0 / 负数 / 小数 / 字符串全部拒，且点名 budget。
    #[test]
    fn bad_budget_is_rejected_with_field_named() {
        let text = r#"{"order_id":"wo-1","line":{"line_id":"l","line_version":"v"},"budget":0}"#;
        match parse_spec(text).unwrap_err() {
            SpecError::Invalid { field, reason } => {
                assert_eq!(field, "budget");
                assert!(reason.contains("大于 0"), "{reason}");
            }
            other => panic!("budget=0 应被拒，实际 {other:?}"),
        }
        for bad in [r#""42""#, "-5", "3.5"] {
            let text = format!(
                r#"{{"order_id":"wo-1","line":{{"line_id":"l","line_version":"v"}},"budget":{bad}}}"#
            );
            match parse_spec(&text).unwrap_err() {
                SpecError::Invalid { field, .. } => assert_eq!(field, "budget"),
                other => panic!("budget={bad} 应被拒为 Invalid(budget)，实际 {other:?}"),
            }
        }
    }

    /// 坏 JSON：报原因（serde 错误含行列位置）。
    #[test]
    fn malformed_json_reports_reason() {
        let err = parse_spec("{ nope").unwrap_err();
        match &err {
            SpecError::Malformed(detail) => assert!(
                detail.contains("line"),
                "解析错误应含位置（原因），实际 {detail}"
            ),
            other => panic!("坏 JSON 应为 Malformed，实际 {other:?}"),
        }
    }

    /// line 的子字段校验：缺子字段 / 非字符串 / 多余子字段 / line 非对象。
    #[test]
    fn line_shape_is_strictly_checked() {
        let cases = [
            (
                r#"{"order_id":"wo-1","line":"line-poc","budget":5}"#,
                "对象",
            ),
            (
                r#"{"order_id":"wo-1","line":{"line_id":"l"},"budget":5}"#,
                "line.line_version",
            ),
            (
                r#"{"order_id":"wo-1","line":{"line_id":"l","line_version":7},"budget":5}"#,
                "line.line_version",
            ),
            (
                r#"{"order_id":"wo-1","line":{"line_id":"l","line_version":"v","extra":1},"budget":5}"#,
                "extra",
            ),
        ];
        for (text, needle) in cases {
            match parse_spec(text).unwrap_err() {
                SpecError::Invalid {
                    field: "line",
                    reason,
                } => {
                    assert!(reason.contains(needle), "{reason} 应含 {needle}");
                }
                other => panic!("line 形状 {text} 应被拒为 Invalid(line)，实际 {other:?}"),
            }
        }
    }
}
