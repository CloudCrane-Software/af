//! 错误分类学 —— 装载管线的结构化错误枚举。
//!
//! 分层：
//!   E0xxx 信封/版本层（装载入口，payload 解析之前）
//!   E1xxx 形状层（serde 结构解析：未知字段/类型/缺字段/枚举值/kind 分派）
//!   E2xxx 语义层（跨字段/跨实体不变量：引用完整性/值域/唯一性/非空）
//!   E9xxx 部署期 CUE 阶段（装载器之外，管线工具用同一枚举上报；
//!         装载器自身永远不会产出这两类 —— 见 REPORT 的"JSON 不可见性"实验）
//!
//! 每个变体携带 path（JSON-pointer 风格定位）与 detail，`code()` 给稳定错误码。

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    // ---- E0xxx 信封/版本层 ----
    /// E0001 schemaVersion 是不支持的大版本（当前只认 1）→ 拒绝装载
    SchemaVersionUnsupported { got: u64, supported: u64 },
    /// E0002 schemaVersion 缺失 / 不是非负整数
    SchemaVersionMalformed { path: String, detail: String },
    /// E0003 信封层出现未声明字段
    EnvelopeUnknownField { field: String },

    // ---- E1xxx 形状层 ----
    /// E1001 未知字段（closed schema；等价 CUE 的 field not allowed）
    UnknownField { path: String, field: String },
    /// E1002 类型不符（等价 CUE 的 conflicting values … and …）
    TypeMismatch { path: String, detail: String },
    /// E1003 必填字段缺失
    MissingField { path: String, field: String },
    /// E1004 枚举取值不在表内（driver/severity/action/op 等）
    UnknownVariant { path: String, field: String, got: String },
    /// E1007 kind 与 params 形状不一致（params 字段属于其它 kind）
    KindParamsMismatch { path: String, rule_id: String, kind: String, field: String },
    /// E1008 未知 kind（等价 CUE 的 empty disjunction）
    UnknownKind { path: String, rule_id: String, got: String },

    // ---- E2xxx 语义层 ----
    /// E2001 悬空引用（combo.allOf→规则 id；andon.trigger→评估函数；required→工件字段）
    DanglingReference { path: String, target_kind: &'static str, target: String },
    /// E2002 值域/跨字段约束不符（critical>warn；升级链递增；kind 特定 op 白名单）
    ValueOutOfRange { path: String, detail: String },
    /// E2003 id 重复（规则/安灯）
    DuplicateId { entity: &'static str, id: String },
    /// E2004 应非空的集合为空（notify/chain/fields/allOf）
    EmptyCollection { path: String, field: String },

    // ---- E9xxx 部署期 CUE 阶段（管线统一上报用；装载器不产出） ----
    /// E9001 可覆盖字段缺 default 标记（*x | 类型）：无覆盖 export incomplete，
    /// 或裸具体值在 L4 覆盖时 conflicting —— 只能在 CUE 源上查出，JSON 里不可见
    DefaultMarkerMissing { cue_stderr: String },
    /// E9002 cue vet 拒绝（closed/约束/引用完整性在部署期先挡一道）
    CueStageRejected { cue_stderr: String },
}

impl LoadError {
    /// 稳定错误码（测试与内核日志按此断言）
    pub fn code(&self) -> &'static str {
        match self {
            LoadError::SchemaVersionUnsupported { .. } => "E0001",
            LoadError::SchemaVersionMalformed { .. } => "E0002",
            LoadError::EnvelopeUnknownField { .. } => "E0003",
            LoadError::UnknownField { .. } => "E1001",
            LoadError::TypeMismatch { .. } => "E1002",
            LoadError::MissingField { .. } => "E1003",
            LoadError::UnknownVariant { .. } => "E1004",
            LoadError::KindParamsMismatch { .. } => "E1007",
            LoadError::UnknownKind { .. } => "E1008",
            LoadError::DanglingReference { .. } => "E2001",
            LoadError::ValueOutOfRange { .. } => "E2002",
            LoadError::DuplicateId { .. } => "E2003",
            LoadError::EmptyCollection { .. } => "E2004",
            LoadError::DefaultMarkerMissing { .. } => "E9001",
            LoadError::CueStageRejected { .. } => "E9002",
        }
    }

    /// 错误大类名（结构化日志用）
    pub fn kind_name(&self) -> &'static str {
        match self {
            LoadError::SchemaVersionUnsupported { .. } => "SCHEMA_VERSION_UNSUPPORTED",
            LoadError::SchemaVersionMalformed { .. } => "SCHEMA_VERSION_MALFORMED",
            LoadError::EnvelopeUnknownField { .. } => "ENVELOPE_UNKNOWN_FIELD",
            LoadError::UnknownField { .. } => "UNKNOWN_FIELD",
            LoadError::TypeMismatch { .. } => "TYPE_MISMATCH",
            LoadError::MissingField { .. } => "MISSING_FIELD",
            LoadError::UnknownVariant { .. } => "UNKNOWN_VARIANT",
            LoadError::KindParamsMismatch { .. } => "KIND_PARAMS_MISMATCH",
            LoadError::UnknownKind { .. } => "UNKNOWN_KIND",
            LoadError::DanglingReference { .. } => "DANGLING_REFERENCE",
            LoadError::ValueOutOfRange { .. } => "VALUE_OUT_OF_RANGE",
            LoadError::DuplicateId { .. } => "DUPLICATE_ID",
            LoadError::EmptyCollection { .. } => "EMPTY_COLLECTION",
            LoadError::DefaultMarkerMissing { .. } => "DEFAULT_MARKER_MISSING",
            LoadError::CueStageRejected { .. } => "CUE_STAGE_REJECTED",
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::SchemaVersionUnsupported { got, supported } => write!(
                f, "schemaVersion {got} 不支持，当前支持 {supported}（拒绝装载，不做兼容尝试）"),
            LoadError::SchemaVersionMalformed { path, detail } => {
                write!(f, "schemaVersion 缺失或非法（{path}）：{detail}")
            }
            LoadError::EnvelopeUnknownField { field } => {
                write!(f, "信封层未知字段 `{field}`（信封 closed：只允许 schemaVersion/line/rules）")
            }
            LoadError::UnknownField { path, field } => write!(f, "未知字段 `{field}`（{path}）"),
            LoadError::TypeMismatch { path, detail } => write!(f, "类型不符（{path}）：{detail}"),
            LoadError::MissingField { path, field } => write!(f, "缺少必填字段 `{field}`（{path}）"),
            LoadError::UnknownVariant { path, field, got } => {
                write!(f, "字段 `{field}` 取值不在表内（{path}）：`{got}`")
            }
            LoadError::KindParamsMismatch { path, rule_id, kind, field } => write!(
                f, "规则 `{rule_id}` kind=`{kind}` 的 params 含其它 kind 的字段 `{field}`（{path}）"),
            LoadError::UnknownKind { path, rule_id, got } => {
                write!(f, "规则 `{rule_id}` 未知 kind `{got}`（{path}）")
            }
            LoadError::DanglingReference { path, target_kind, target } => {
                write!(f, "悬空引用（{path}）：{target_kind} `{target}` 未声明")
            }
            LoadError::ValueOutOfRange { path, detail } => write!(f, "值域/约束不符（{path}）：{detail}"),
            LoadError::DuplicateId { entity, id } => write!(f, "{entity} id 重复：`{id}`"),
            LoadError::EmptyCollection { path, field } => write!(f, "集合 `{field}` 不应为空（{path}）"),
            LoadError::DefaultMarkerMissing { cue_stderr } => {
                write!(f, "可覆盖字段缺 default 标记（部署期 CUE 查出）：{cue_stderr}")
            }
            LoadError::CueStageRejected { cue_stderr } => {
                write!(f, "cue vet/export 拒绝（部署期）：{cue_stderr}")
            }
        }
    }
}

impl std::error::Error for LoadError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// 错误码与变体一一对应、无重复码
    #[test]
    fn codes_are_unique() {
        let all = [
            LoadError::SchemaVersionUnsupported { got: 2, supported: 1 }, // u64 字段

            LoadError::SchemaVersionMalformed { path: "/".into(), detail: "x".into() },
            LoadError::EnvelopeUnknownField { field: "owner".into() },
            LoadError::UnknownField { path: "/line".into(), field: "owner".into() },
            LoadError::TypeMismatch { path: "/x".into(), detail: "d".into() },
            LoadError::MissingField { path: "/x".into(), field: "f".into() },
            LoadError::UnknownVariant { path: "/x".into(), field: "driver".into(), got: "canbus".into() },
            LoadError::KindParamsMismatch { path: "/x".into(), rule_id: "r".into(), kind: "threshold".into(), field: "warn".into() },
            LoadError::UnknownKind { path: "/x".into(), rule_id: "r".into(), got: "quantum".into() },
            LoadError::DanglingReference { path: "/x".into(), target_kind: "rule", target: "nope".into() },
            LoadError::ValueOutOfRange { path: "/x".into(), detail: "d".into() },
            LoadError::DuplicateId { entity: "rule", id: "r".into() },
            LoadError::EmptyCollection { path: "/x".into(), field: "notify".into() },
            LoadError::DefaultMarkerMissing { cue_stderr: "s".into() },
            LoadError::CueStageRejected { cue_stderr: "s".into() },
        ];
        let mut codes: Vec<&str> = all.iter().map(|e| e.code()).collect();
        let n = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), n, "错误码必须互不相同");
    }
}
