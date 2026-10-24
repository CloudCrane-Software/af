//! 形状层：serde 错误 → 稳定码分类（E1001-E1004）。
//! serde 天然 fail-fast（首错即止）；「一次收齐」在语义层（Report 范式）。

use crate::error::{ErrorCode, LoadError, Report, Violation};
use crate::model::Document;

/// 信封通过后的 body 反序列化；serde 错误按信息前缀归类到 E1001-E1004。
pub fn deserialize_document(value: &serde_yaml::Value) -> Result<Document, LoadError> {
    let doc: Document = serde_yaml::from_value(value.clone())
        .map_err(|e| LoadError::Shape(classify_serde_error(&e)))?;
    Ok(doc)
}

/// serde_yaml 错误信息 → 稳定码。映射表（RESULT-M1 登记）：
/// `unknown field` → E1001；`missing field` → E1002；
/// `unknown variant` / `invalid value` → E1004；其余（`invalid type`、
/// `data did not match any variant`、自定义类型表达式语法错等）→ E1003。
pub fn classify_serde_error(err: &serde_yaml::Error) -> Report {
    let msg = err.to_string();
    let code = if msg.starts_with("unknown field") {
        ErrorCode::UnknownField
    } else if msg.starts_with("missing field") {
        ErrorCode::MissingField
    } else if msg.starts_with("unknown variant") || msg.starts_with("invalid value") {
        ErrorCode::InvalidEnumValue
    } else {
        // invalid type / invalid length / data did not match any variant /
        // 自定义类型表达式语法错（de::Error::custom）
        ErrorCode::TypeMismatch
    };
    let mut report = Report::default();
    report.push(code, "document", msg);
    report
}

/// 手写闭环词表检查（平铺联合体承载的枚举面）：ops 算子名、section 来源名。
/// 归入语义层 Report（码位属形状域 E1004，由 validate 一并收齐）。
pub fn check_vocab(doc: &Document) -> Vec<crate::error::Violation> {
    let mut out = Vec::new();
    walk_nodes(doc, &mut |node, scope| {
        if let Some(ops) = &node.ops {
            for (i, op) in ops.iter().enumerate() {
                if !crate::model::OP_KINDS.contains(&op.op.as_str()) {
                    out.push(Violation::new(
                        ErrorCode::InvalidEnumValue,
                        scope.to_string(),
                        format!("ops[{i}] 算子 {:?} 不在闭环词表 {:?}", op.op, crate::model::OP_KINDS),
                    ));
                }
            }
        }
        if let Some(ctx) = &node.context {
            for (i, s) in ctx.sections.iter().enumerate() {
                if !crate::model::SECTION_FROMS.contains(&s.from.as_str()) {
                    out.push(Violation::new(
                        ErrorCode::InvalidEnumValue,
                        scope.to_string(),
                        format!(
                            "context.sections[{i}] 来源 {:?} 不在闭环词表 {:?}",
                            s.from, crate::model::SECTION_FROMS
                        ),
                    ));
                }
            }
        }
    });
    out
}

/// 深度遍历全部节点（含 loop/map 体内嵌），scope 标当前所在体
/// （"root" / "n1.loop_body" / "n1.map_body" / 嵌套拼接）。
pub(crate) fn walk_nodes<'a>(doc: &'a Document, f: &mut impl FnMut(&'a crate::model::NodeDecl, &str)) {
    fn walk_body<'a>(
        nodes: &'a [crate::model::NodeDecl],
        scope: &str,
        f: &mut impl FnMut(&'a crate::model::NodeDecl, &str),
    ) {
        for n in nodes {
            f(n, scope);
            let child_scope = |suffix: &str| {
                if scope == "root" {
                    format!("{}.{suffix}", n.id)
                } else {
                    format!("{scope}.{}.{suffix}", n.id)
                }
            };
            if let Some(lp) = &n.loop_spec {
                let s = child_scope("loop_body");
                walk_body(&lp.body.nodes, &s, f);
            }
            if let Some(mp) = &n.map {
                let s = child_scope("map_body");
                walk_body(&mp.body.nodes, &s, f);
            }
        }
    }
    walk_body(&doc.nodes, "root", f);
}
