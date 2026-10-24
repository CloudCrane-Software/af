//! 极简 OrderSpec：`order_id` / `line` / `budget` / `note`（可选）。
//!
//! 拒绝式解析（对齐 af-workorder 老载荷门禁的风格）：缺字段点名、未知字段
//! 名单列全、类型不合预检、`order_id` 必须是安全路径成分——它同时是
//! `state/orders/` 下的目录名，放行 `../` 等于放行目录穿越。
//!
//! RED 桩：类型与错误契约已定，`parse_spec` 尚未实现（转绿时落地）。

use af_workorder::LineBinding;
use serde::Deserialize;

/// spec 认识的字段（字典序，供未知字段名单对拍）。
pub const SPEC_FIELDS: &[&str] = &["budget", "line", "note", "order_id"];

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

/// 解析 spec 文本。RED 桩：一律拒绝。
pub fn parse_spec(_text: &str) -> Result<OrderSpec, SpecError> {
    Err(SpecError::Malformed("未实现：spec 解析（RED 桩）".into()))
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
    #[test]
    fn path_unsafe_order_id_is_rejected() {
        for bad in ["../escape", "a/b", "a\\b", ".", "..", "", "has space"] {
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
}
