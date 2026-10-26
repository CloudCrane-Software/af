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

/// serde_yaml 错误信息 → 稳定码。映射表（RESULT-M1 登记；红队 W3-C17 勘误：
/// `invalid value` 按 serde 实际语义二分——**数值越界**（如 `integer -1,
/// expected u32`）归 **E2002 值域**（error.rs:62-63 域定义），**枚举未知
/// 值**（`invalid value: string "x", expected ...`）仍归 E1004；原映射把
/// 两者一并塞 E1004，负数入无符号字段被误报「枚举值非法」）：
/// `unknown field` → E1001；`missing field` → E1002；
/// `unknown variant` / 枚举形 `invalid value` → E1004；
/// 数值越界形 `invalid value: integer <n>, expected <无符号/正数域>` → E2002；
/// 其余（`invalid type`、`data did not match any variant`、自定义类型表达式
/// 语法错等）→ E1003。
pub fn classify_serde_error(err: &serde_yaml::Error) -> Report {
    let msg = err.to_string();
    // 数值越界形：serde 对无符号/正数域字段报 `invalid value: integer -1,
    // expected u32`——类型家族对（integer），值越界。枚举形报
    // `invalid value: string "x", expected one of ...`。以 expected 侧的
    // 数值域类型名（u8..u64/usize/i8../f32/f64 及布尔）判别。
    let numeric_domain = msg.starts_with("invalid value")
        && (msg.contains("expected u8")
            || msg.contains("expected u16")
            || msg.contains("expected u32")
            || msg.contains("expected u64")
            || msg.contains("expected usize")
            || msg.contains("expected i8")
            || msg.contains("expected i16")
            || msg.contains("expected i32")
            || msg.contains("expected i64")
            || msg.contains("expected f32")
            || msg.contains("expected f64")
            || msg.contains("expected bool"));
    let code = if msg.starts_with("unknown field") {
        ErrorCode::UnknownField
    } else if msg.starts_with("missing field") {
        ErrorCode::MissingField
    } else if numeric_domain {
        ErrorCode::ValueOutOfRange
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

/// types[].schema 内联 JSON Schema 形状检查（红队 W3-C18）：model.rs:71-74
/// 自述 schema 为「内联 JSON Schema 对象（唯一 schema 家）」，实现却是裸
/// `serde_json::Value` 零校验——字符串/数字/数组/布尔/null 均零违例过审，
/// 而 cel.rs synth_from_schema 对非 object schema 一律退化为 Null 绑定、
/// 求值错误按设计不判 ⇒ 整份文档的 CEL 布尔判定静默变成无证据。本检查
/// fail-loud：schema 必须是对象、type 段必须是 JSON Schema 原始六型之一、
/// properties/items 递归同规则（无 type 节点的 schema 拒）。原农田 B10
/// （schema: "x" 标量 PASS）随之翻转为拒收（文档化边界勘误，见
/// ANOMALIES.md 与 farm_anomalies_test 的钉子迁移）。
const SCHEMA_PRIMITIVE_TYPES: [&str; 6] =
    ["object", "string", "integer", "number", "boolean", "array"];

fn json_kind_name(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "布尔",
        serde_json::Value::Number(_) => "数字",
        serde_json::Value::String(_) => "字符串",
        serde_json::Value::Array(_) => "数组",
        serde_json::Value::Object(_) => "对象",
    }
}

/// 已声明类型名集（schema 内命名引用合法位：`items: {type: plan_step}`
/// 指向同文档 types 表条目——sop_full task_plan.steps 即此形；红队
/// W3-C18 首版漏放行命名引用误伤正本，本版补）。
fn declared_type_names(doc: &Document) -> std::collections::BTreeSet<String> {
    doc.types.iter().map(|t| t.name.clone()).collect()
}

fn schema_node_violation(
    subject: &str,
    path: &str,
    schema: &serde_json::Value,
    declared: &std::collections::BTreeSet<String>,
) -> Option<String> {
    let Some(obj) = schema.as_object() else {
        return Some(format!(
            "{subject} 的 schema（{path}）非 JSON 对象（实为 {}）——schema 必须是内联 \
             JSON Schema 对象（唯一 schema 家），标量/数组/null 形状拒收",
            json_kind_name(schema)
        ));
    };
    let Some(ty) = obj.get("type").and_then(|t| t.as_str()) else {
        return Some(format!(
            "{subject} 的 schema（{path}）缺 type 段或 type 非字符串——JSON Schema \
             节点必须声明原始六型之一（object/string/integer/number/boolean/array）"
        ));
    };
    if !SCHEMA_PRIMITIVE_TYPES.contains(&ty) && !declared.contains(ty) {
        return Some(format!(
            "{subject} 的 schema（{path}）type={ty:?} 不在原始六型 \
             （object/string/integer/number/boolean/array）也不是已声明类型名——\
             对象 schema 必须住 types 表（命名引用指向同文档 types 条目）"
        ));
    }
    let is_named_ref = !SCHEMA_PRIMITIVE_TYPES.contains(&ty);
    if !is_named_ref {
        if ty == "object" {
            if let Some(props) = obj.get("properties") {
                let Some(map) = props.as_object() else {
                    return Some(format!("{subject} 的 schema（{path}）properties 非对象"));
                };
                for (k, v) in map {
                    if let Some(detail) =
                        schema_node_violation(subject, &format!("{path}.{k}"), v, declared)
                    {
                        return Some(detail);
                    }
                }
            }
        }
        if ty == "array" {
            if let Some(items) = obj.get("items") {
                if let Some(detail) =
                    schema_node_violation(subject, &format!("{path}[]"), items, declared)
                {
                    return Some(detail);
                }
            }
        }
    }
    None
}

/// schema 形状检查（E1003 值域/形状面；随 validate 一次收齐）。两个裸
/// `serde_json::Value` schema 位同门：types[].schema（W3-C18）与
/// human.resume.schema（红队 W2-C-24）——后者同为裸 Value 且同走
/// cel::synth_from_schema（validate/m4.rs add_resume），漏门则标量/数组/null
/// resume schema 零违例过审、human 节点 route 的 CEL 布尔判定静默无证据。
pub fn check_schema_shape(doc: &Document) -> Vec<crate::error::Violation> {
    let mut out = Vec::new();
    let declared = declared_type_names(doc);
    for t in &doc.types {
        if let Some(detail) =
            schema_node_violation(&format!("类型 {}", t.name), "schema", &t.schema, &declared)
        {
            out.push(crate::error::Violation::new(
                ErrorCode::TypeMismatch,
                "types",
                detail,
            ));
        }
    }
    for n in &doc.nodes {
        let Some(h) = &n.human else { continue };
        if let Some(detail) = schema_node_violation(
            &format!("节点 {} human.resume", n.id),
            "schema",
            &h.resume.schema,
            &declared,
        ) {
            out.push(crate::error::Violation::new(
                ErrorCode::TypeMismatch,
                "human",
                detail,
            ));
        }
    }
    out
}

/// 手写闭环词表/词法检查（平铺联合体承载的枚举面 + id 字符集）：
/// ops 算子名、section 来源名（E1004）；节点 id 点号禁用（E1008——点号是
/// 构造体路径分隔符，id 段含点号会与路径同形，语义索引键覆盖致盲五防线，
/// 农田 B-1 路径别名族）。归入语义层 Report（码位属形状域，由 validate
/// 一并收齐——先于索引构建，别名结构不进语义判定面）。
pub fn check_vocab(doc: &Document) -> Vec<crate::error::Violation> {
    let mut out = Vec::new();
    walk_nodes(doc, &mut |node, scope| {
        if node.id.contains('.') {
            out.push(Violation::new(
                ErrorCode::DottedNodeId,
                scope.to_string(),
                format!(
                    "节点 id {:?} 含点号——点号保留给构造体路径分隔\
                     （writer/safety_points 的「点号路径」语义），id 段禁用",
                    node.id
                ),
            ));
        }
        if let Some(ops) = &node.ops {
            for (i, op) in ops.iter().enumerate() {
                if !crate::model::OP_KINDS.contains(&op.op.as_str()) {
                    out.push(Violation::new(
                        ErrorCode::InvalidEnumValue,
                        scope.to_string(),
                        format!(
                            "ops[{i}] 算子 {:?} 不在闭环词表 {:?}",
                            op.op,
                            crate::model::OP_KINDS
                        ),
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
                            s.from,
                            crate::model::SECTION_FROMS
                        ),
                    ));
                }
            }
        }
    });
    out
}

/// 深度遍历全部节点（含 loop/map 体内嵌），scope 标当前所在体
/// （"root" / "<节点路径>.loop_body" / "<节点路径>.map_body"，与 validate 索引一致）。
pub(crate) fn walk_nodes<'a>(
    doc: &'a Document,
    f: &mut impl FnMut(&'a crate::model::NodeDecl, &str),
) {
    fn walk_body<'a>(
        nodes: &'a [crate::model::NodeDecl],
        parent_path: &str,
        scope: &str,
        f: &mut impl FnMut(&'a crate::model::NodeDecl, &str),
    ) {
        for n in nodes {
            f(n, scope);
            let path = if parent_path.is_empty() {
                n.id.clone()
            } else {
                format!("{parent_path}.{}", n.id)
            };
            if let Some(lp) = &n.loop_spec {
                walk_body(&lp.body.nodes, &path, &format!("{path}.loop_body"), f);
            }
            if let Some(mp) = &n.map {
                walk_body(&mp.body.nodes, &path, &format!("{path}.map_body"), f);
            }
        }
    }
    walk_body(&doc.nodes, "", "root", f);
}
